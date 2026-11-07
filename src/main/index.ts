import { spawn as spawnProcess } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync, statSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join } from 'node:path'
import { app, BrowserWindow, dialog, ipcMain, shell } from 'electron'
import type { AppState, ProjectInfo, Result, Settings, SpawnRequest } from '../shared/types'
import {
  createProfile,
  ensureAllStatusLines,
  listProfiles,
  profileDir,
  profileEnv,
  profileExists,
  validateProfileName
} from './profiles'
import { Store } from './store'
import { Terminals } from './terminals'
import { Updater } from './updater'
import { setUpdateToken, updateTokenSource } from './updateToken'

/** Must match the title bar height in the renderer (h-10 = 40px). */
const TITLEBAR_HEIGHT = 40

let win: BrowserWindow | null = null
let store: Store
const sendToWindow = (channel: string, payload: unknown) => {
  if (win && !win.isDestroyed()) win.webContents.send(channel, payload)
}
const terminals = new Terminals(sendToWindow)
const updater = new Updater((s) => sendToWindow('update:status', s))

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

function projectInfo(id: string): ProjectInfo | undefined {
  const p = store.data.projects.find((x) => x.id === id)
  return p && { ...p, exists: isDir(p.path) }
}

function getState(): AppState {
  return {
    appVersion: app.getVersion(),
    update: updater.status,
    updateTokenSource: updateTokenSource(),
    projects: store.data.projects
      .map((p) => ({ ...p, exists: isDir(p.path) }))
      .sort((a, b) => (b.lastOpenedAt ?? b.addedAt) - (a.lastOpenedAt ?? a.addedAt)),
    profiles: listProfiles(),
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
      editorCommand: settings.editorCommand?.trim() || undefined
    }
    store.save()
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

    try {
      const id = terminals.spawn({ file: claude, args: [], cwd, env: profileEnv(req.profile), cols: req.cols, rows: req.rows })
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
  ipcMain.on('term:kill', (_e, id: string) => terminals.kill(id))

  ipcMain.handle('update:check', async () => {
    await updater.check()
    return updater.status
  })
  ipcMain.handle('update:setToken', async (_e, token: string): Promise<Result> => {
    try {
      setUpdateToken(token)
      void updater.check()
      return ok(null)
    } catch (e) {
      return fail(e)
    }
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

  void app.whenReady().then(() => {
    store = new Store(join(app.getPath('userData'), 'app.json'))
    ensureAllStatusLines()
    registerIpc()
    createWindow()
    updater.start()
  })

  app.on('before-quit', () => terminals.killAll())
  app.on('window-all-closed', () => {
    terminals.killAll()
    app.quit()
  })
}
