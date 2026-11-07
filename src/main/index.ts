import { spawn as spawnProcess } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync, rmSync, statSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join } from 'node:path'
import { app, BrowserWindow, dialog, ipcMain, Notification, shell } from 'electron'
import type { AppState, GitInfo, HistorySession, McpServer, ProjectInfo, Result, Settings, SpawnRequest } from '../shared/types'
import {
  createProfile,
  ensureAllStatusLines,
  listProfileNames,
  listProfiles,
  profileDir,
  profileEnv,
  profileExists,
  validateProfileName
} from './profiles'
import { AccountMonitor } from './accountInfo'
import { getGitInfo } from './git'
import { listHistory } from './history'
import { HookServer } from './hookServer'
import { ensureHooks } from './hooksConfig'
import { listMcp } from './mcp'
import { applyVoiceSettings, DICTATE_SEQUENCE } from './profileSettings'
import { pruneSessionFiles, SessionTracker } from './sessions'
import { Store } from './store'
import { Terminals } from './terminals'
import { Updater } from './updater'

/** Must match the title bar height in the renderer (h-10 = 40px). */
const TITLEBAR_HEIGHT = 40

let win: BrowserWindow | null = null
let store: Store
let monitor: AccountMonitor
const sendToWindow = (channel: string, payload: unknown) => {
  if (win && !win.isDestroyed()) win.webContents.send(channel, payload)
}
const sessions = new SessionTracker((info) => sendToWindow('session:update', info))
const terminals = new Terminals((channel, payload) => {
  if (channel === 'term:exit') sessions.exited((payload as { id: string }).id)
  sendToWindow(channel, payload)
})
const updater = new Updater((s) => sendToWindow('update:status', s))
const hooks = new HookServer((tabId, event) => sessions.handle(tabId, event))

/** Env for any claude process the app runs for a profile (terminals and background probes). */
function claudeEnv(profile: string, tabId = ''): Record<string, string> {
  return { ...profileEnv(profile), CCL_TAB_ID: tabId, CCL_HOOK_TOKEN: hooks.token }
}

const ok = <T>(value: T): Result<T> => ({ ok: true, value })
const fail = (e: unknown): { ok: false; error: string } => ({
  ok: false,
  error: e instanceof Error ? e.message : String(e)
})

function isDir(path: string): boolean {
  try {
    return statSync(path).isDirectory()
  } catch {
    return false
  }
}

/** The claude executable to launch: the setting, the native installer location, or PATH. */
function resolveClaude(): string | null {
  const configured = store.data.settings.claudePath?.trim()
  if (configured) return existsSync(configured) ? configured : null
  const native = join(homedir(), '.local', 'bin', process.platform === 'win32' ? 'claude.exe' : 'claude')
  if (existsSync(native)) return native
  const exts = process.platform === 'win32' ? ['.exe', '.cmd'] : ['']
  for (const dir of (process.env.PATH ?? '').split(process.platform === 'win32' ? ';' : ':')) {
    for (const ext of exts) {
      const candidate = join(dir, `claude${ext}`)
      if (dir && existsSync(candidate)) return candidate
    }
  }
  return null
}

/** Writes the app's language/voice choices and hook wiring into one profile (or all). Never throws. */
function applyProfileSettings(only?: string): void {
  for (const name of only ? [only] : listProfileNames()) {
    try {
      applyVoiceSettings(profileDir(name), store.data.settings)
      ensureHooks(profileDir(name), hooks.port)
    } catch {
      // A locked or read-only file shouldn't break the app.
    }
  }
}

const mcpCache = new Map<string, { at: number; servers: McpServer[] }>()

function projectInfo(id: string): ProjectInfo | undefined {
  const p = store.data.projects.find((x) => x.id === id)
  return p && { ...p, exists: isDir(p.path) }
}

function getState(): AppState {
  return {
    appVersion: app.getVersion(),
    update: updater.status,
    projects: store.data.projects
      .map((p) => ({ ...p, exists: isDir(p.path) }))
      .sort((a, b) => (b.lastOpenedAt ?? b.addedAt) - (a.lastOpenedAt ?? a.addedAt)),
    profiles: listProfiles((n) => monitor.get(n)),
    accountsRefreshing: monitor.busy,
    settings: store.data.settings,
    claudePath: resolveClaude()
  }
}

function registerIpc(): void {
  ipcMain.handle('state:get', () => getState())

  ipcMain.handle('project:add', async (): Promise<Result<ProjectInfo | null>> => {
    try {
      const res = await dialog.showOpenDialog(win!, {
        title: 'Choose or create a project folder',
        properties: ['openDirectory', 'createDirectory']
      })
      if (res.canceled || res.filePaths.length === 0) return ok(null)
      const path = res.filePaths[0]
      const existing = store.data.projects.find((p) => p.path.toLowerCase() === path.toLowerCase())
      if (existing) return ok({ ...existing, exists: true })
      const project = { id: randomUUID(), name: basename(path) || path, path, addedAt: Date.now() }
      store.data.projects.push(project)
      store.save()
      return ok({ ...project, exists: true })
    } catch (e) {
      return fail(e)
    }
  })

  ipcMain.handle('project:remove', (_e, id: string): Result => {
    store.data.projects = store.data.projects.filter((p) => p.id !== id)
    store.save()
    return ok(null)
  })

  ipcMain.handle('profile:add', (_e, name: string): Result => {
    const error = validateProfileName(name)
    if (error) return { ok: false, error }
    if (profileExists(name)) return { ok: false, error: `Profile "${name}" already exists.` }
    try {
      createProfile(name)
      applyProfileSettings(name)
      void monitor.refresh(name)
      return ok(null)
    } catch (e) {
      return fail(e)
    }
  })

  ipcMain.handle('profile:remove', async (_e, name: string): Promise<Result<boolean>> => {
    if (validateProfileName(name)) return { ok: false, error: 'Invalid profile name.' }
    const dir = profileDir(name)
    const answer = await dialog.showMessageBox(win!, {
      type: 'warning',
      buttons: ['Move to Recycle Bin', 'Cancel'],
      defaultId: 1,
      cancelId: 1,
      title: 'Remove profile',
      message: `Remove profile "${name}"?`,
      detail:
        `The folder ${dir} will be moved to the Recycle Bin. ` +
        'This signs that account out on this PC and removes its local session history. You can restore it from the Recycle Bin.'
    })
    if (answer.response !== 0) return ok(false)
    try {
      await shell.trashItem(dir)
      return ok(true)
    } catch (e) {
      return fail(e)
    }
  })

  ipcMain.handle('settings:save', (_e, settings: Settings): Result => {
    store.data.settings = {
      claudePath: settings.claudePath?.trim() || undefined,
      editorCommand: settings.editorCommand?.trim() || undefined,
      claudeLanguage: settings.claudeLanguage?.trim() || undefined,
      voiceMode: settings.voiceMode && ['tap', 'hold', 'off'].includes(settings.voiceMode) ? settings.voiceMode : undefined
    }
    store.save()
    applyProfileSettings()
    return ok(null)
  })

  ipcMain.handle('shell:openFolder', async (_e, path: string): Promise<Result> => {
    const err = await shell.openPath(path)
    return err ? { ok: false, error: err } : ok(null)
  })

  ipcMain.handle('shell:openInEditor', (_e, path: string): Promise<Result> => {
    const cmd = store.data.settings.editorCommand || 'zed'
    return new Promise((resolve) => {
      // shell: true so .cmd launchers on PATH work; Windows paths cannot contain quotes.
      const child = spawnProcess(`${cmd} "${path}"`, { shell: true, detached: true, stdio: 'ignore' })
      child.once('error', (e) => resolve(fail(e)))
      child.once('spawn', () => {
        child.unref()
        resolve(ok(null))
      })
    })
  })

  ipcMain.handle('term:spawn', (_e, req: SpawnRequest): Result<string> => {
    const claude = resolveClaude()
    if (!claude) return { ok: false, error: 'Claude Code was not found. Install it or set its path in Settings.' }
    if (!profileExists(req.profile)) return { ok: false, error: `Profile "${req.profile}" does not exist.` }

    let cwd: string
    if (req.kind === 'project') {
      const project = req.projectId ? projectInfo(req.projectId) : undefined
      if (!project) return { ok: false, error: 'Project not found.' }
      if (!project.exists) return { ok: false, error: `Folder not found: ${project.path}` }
      cwd = project.path
    } else {
      cwd = profileDir(req.profile)
    }
    createProfile(req.profile) // also (re)installs the usage status line
    applyProfileSettings(req.profile)

    try {
      const tabId = randomUUID()
      const args = req.resumeSessionId && /^[A-Za-z0-9-]+$/.test(req.resumeSessionId) ? ['--resume', req.resumeSessionId] : []
      sessions.open(tabId, profileDir(req.profile))
      const id = terminals.spawn({ id: tabId, file: claude, args, cwd, env: claudeEnv(req.profile, tabId), cols: req.cols, rows: req.rows })
      if (req.kind === 'project') {
        const p = store.data.projects.find((x) => x.id === req.projectId)
        if (p) {
          p.lastProfile = req.profile
          p.lastOpenedAt = Date.now()
          store.save()
        }
      }
      return ok(id)
    } catch (e) {
      return fail(e)
    }
  })

  ipcMain.on('term:write', (_e, id: string, data: string) => terminals.write(id, data))
  ipcMain.on('term:resize', (_e, id: string, cols: number, rows: number) => terminals.resize(id, cols, rows))
  ipcMain.on('term:kill', (_e, id: string) => {
    terminals.kill(id)
    sessions.close(id)
  })

  ipcMain.handle('sessions:get', () => sessions.all())

  ipcMain.on('notify', (_e, n: { title: string; body: string; tabId: string }) => {
    if (!Notification.isSupported()) return
    const note = new Notification({
      title: String(n.title).slice(0, 120),
      body: String(n.body).slice(0, 300),
      icon: join(app.getAppPath(), 'resources', 'icon.png')
    })
    note.on('click', () => {
      if (!win) return
      if (win.isMinimized()) win.restore()
      win.show()
      win.focus()
      sendToWindow('notify:click', n.tabId)
    })
    note.show()
  })

  ipcMain.handle('git:info', async (_e, projectId: string): Promise<Result<GitInfo>> => {
    const p = projectInfo(projectId)
    if (!p?.exists) return { ok: false, error: 'Project folder not found.' }
    try {
      return ok(await getGitInfo(p.path))
    } catch (e) {
      return fail(e)
    }
  })

  ipcMain.handle('history:list', (_e, projectId: string): Result<HistorySession[]> => {
    const p = projectInfo(projectId)
    if (!p) return { ok: false, error: 'Project not found.' }
    try {
      return ok(listHistory(p.path, listProfileNames().map((name) => ({ name, dir: profileDir(name) }))))
    } catch (e) {
      return fail(e)
    }
  })

  ipcMain.handle('mcp:list', async (_e, profile: string, projectId: string): Promise<Result<McpServer[]>> => {
    const p = projectInfo(projectId)
    const claude = resolveClaude()
    if (!p?.exists || !claude || !profileExists(profile)) return { ok: false, error: 'Project, account or Claude Code not found.' }
    const key = `${profile}|${p.path}`
    const cached = mcpCache.get(key)
    if (cached && Date.now() - cached.at < 5 * 60_000) return ok(cached.servers)
    try {
      const servers = await listMcp(claude, p.path, claudeEnv(profile))
      mcpCache.set(key, { at: Date.now(), servers })
      return ok(servers)
    } catch (e) {
      return fail(e)
    }
  })
  ipcMain.on('term:dictate', (_e, id: string) => terminals.write(id, DICTATE_SEQUENCE))

  ipcMain.handle('accounts:refresh', async (_e, name?: string) => {
    await monitor.refresh(name)
    return ok(null)
  })

  ipcMain.handle('update:check', async () => {
    await updater.check()
    return updater.status
  })
  ipcMain.on('update:install', () => {
    terminals.killAll()
    updater.install()
  })
}

function createWindow(): void {
  win = new BrowserWindow({
    width: 1320,
    height: 840,
    minWidth: 900,
    minHeight: 560,
    title: 'cc-launcher',
    icon: join(app.getAppPath(), 'resources', 'icon.png'),
    backgroundColor: '#0a0a0a',
    autoHideMenuBar: true,
    // Custom title bar: no native caption; Windows draws only the min/max/close buttons on top.
    titleBarStyle: 'hidden',
    titleBarOverlay: { color: '#0a0a0a', symbolColor: '#a3a3a3', height: TITLEBAR_HEIGHT },
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  })

  // Links open in the real browser; the app window never navigates away.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//.test(url)) void shell.openExternal(url)
    return { action: 'deny' }
  })
  win.webContents.on('will-navigate', (e, url) => {
    if (process.env.ELECTRON_RENDERER_URL && url.startsWith(process.env.ELECTRON_RENDERER_URL)) return
    e.preventDefault()
  })

  if (process.env.ELECTRON_RENDERER_URL) void win.loadURL(process.env.ELECTRON_RENDERER_URL)
  else void win.loadFile(join(__dirname, '../renderer/index.html'))

  win.on('closed', () => {
    win = null
  })
}

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', () => {
    if (win) {
      if (win.isMinimized()) win.restore()
      win.focus()
    }
  })

  app.setAppUserModelId('dev.cclauncher.app') // Windows notifications need this

  void app.whenReady().then(async () => {
    store = new Store(join(app.getPath('userData'), 'app.json'))
    try {
      await hooks.start() // before profiles are wired to its port
    } catch {
      // Without the hook server the app still works; the session panel just stays empty.
    }
    pruneSessionFiles(listProfileNames().map(profileDir))
    // Releases are public now; delete the update token older versions may have saved.
    rmSync(join(app.getPath('userData'), 'update-token.bin'), { force: true })
    ensureAllStatusLines()
    applyProfileSettings()
    monitor = new AccountMonitor(join(app.getPath('userData'), 'usage-probe'), {
      claudePath: resolveClaude,
      profiles: listProfileNames,
      env: (profile) => claudeEnv(profile),
      onChange: () => sendToWindow('state:changed', null)
    })
    registerIpc()
    createWindow()
    updater.start()
    monitor.start() // account email + /usage for every profile, then every 5 minutes
  })

  app.on('before-quit', () => {
    monitor?.stop()
    hooks.stop()
    terminals.killAll()
  })
  app.on('window-all-closed', () => {
    terminals.killAll()
    app.quit()
  })
}
