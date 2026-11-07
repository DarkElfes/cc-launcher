import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { app } from 'electron'
import electronUpdater from 'electron-updater'
import type { UpdateStatus } from '../shared/types'
import { getUpdateToken } from './updateToken'

const { autoUpdater } = electronUpdater

/**
 * Auto-update through electron-updater from a private GitHub repo's releases.
 *
 * Runs only in the installed app, and only when the build had a `publish` target (which writes
 * resources/app-update.yml with owner/repo). The read-only token comes from updateToken.ts and
 * is passed straight to the updater. New versions download in the background and install on the
 * next restart, or immediately via install().
 */
export class Updater {
  status: UpdateStatus = { state: 'idle' }
  private timer: NodeJS.Timeout | null = null
  private wired = false

  constructor(private emit: (s: UpdateStatus) => void) {}

  private get configFile(): string {
    return join(process.resourcesPath, 'app-update.yml')
  }

  get enabled(): boolean {
    return app.isPackaged && existsSync(this.configFile)
  }

  /** owner/repo from app-update.yml (a flat YAML file written by electron-builder). */
  private repo(): { owner: string; repo: string } | null {
    const text = readFileSync(this.configFile, 'utf8')
    const get = (k: string) => text.match(new RegExp(`^${k}:\\s*['"]?([^'"\\r\\n]+)`, 'm'))?.[1]?.trim()
    const owner = get('owner')
    const repo = get('repo')
    return owner && repo ? { owner, repo } : null
  }

  private set(s: UpdateStatus): void {
    this.status = s
    this.emit(s)
  }

  start(): void {
    if (!this.enabled) {
      this.set({ state: 'disabled' })
      return
    }
    if (!this.wired) {
      this.wired = true
      autoUpdater.autoDownload = true
      autoUpdater.autoInstallOnAppQuit = true
      autoUpdater.on('checking-for-update', () => this.set({ state: 'checking' }))
      autoUpdater.on('update-not-available', () => this.set({ state: 'none' }))
      autoUpdater.on('update-available', (i) => this.set({ state: 'downloading', version: i.version, percent: 0 }))
      autoUpdater.on('download-progress', (p) =>
        this.set({ state: 'downloading', version: this.status.version, percent: Math.round(p.percent) })
      )
      autoUpdater.on('update-downloaded', (i) => this.set({ state: 'ready', version: i.version }))
      autoUpdater.on('error', (e) => this.set({ state: 'error', error: friendlyError(e) }))
    }
    void this.check()
    if (!this.timer) this.timer = setInterval(() => void this.check(), 4 * 60 * 60 * 1000) // every 4 hours
  }

  async check(): Promise<void> {
    if (!this.enabled) return this.set({ state: 'disabled' })
    if (this.status.state === 'downloading' || this.status.state === 'ready') return
    const target = this.repo()
    if (!target) return this.set({ state: 'error', error: 'app-update.yml has no owner/repo.' })
    const token = getUpdateToken()
    if (!token) return this.set({ state: 'needs-token' })
    try {
      autoUpdater.setFeedURL({ provider: 'github', owner: target.owner, repo: target.repo, private: true, token })
      await autoUpdater.checkForUpdates()
    } catch (e) {
      this.set({ state: 'error', error: friendlyError(e) })
    }
  }

  install(): void {
    if (this.status.state !== 'ready') return
    if (this.timer) clearInterval(this.timer)
    autoUpdater.quitAndInstall(true, true) // silent install, then relaunch
  }
}

function friendlyError(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e)
  if (/\b(401|Bad credentials)\b/i.test(msg)) return 'GitHub rejected the update token (expired or revoked?). Add a new one in Settings.'
  if (/\b404\b/.test(msg)) return 'Release not found. Check that the token can read the repo and that a release is published.'
  return msg.split('\n')[0].slice(0, 300)
}
