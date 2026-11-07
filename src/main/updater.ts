import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { app } from 'electron'
import electronUpdater from 'electron-updater'
import type { UpdateStatus } from '../shared/types'

const { autoUpdater } = electronUpdater

/**
 * Auto-update through electron-updater from the public GitHub repo's releases (no token needed).
 *
 * Runs only in the installed app, and only when the build had a `publish` target (which writes
 * resources/app-update.yml). New versions download in the background and install on the next
 * restart, or immediately via install().
 */
export class Updater {
  status: UpdateStatus = { state: 'idle' }
  private timer: NodeJS.Timeout | null = null

  constructor(private emit: (s: UpdateStatus) => void) {}

  get enabled(): boolean {
    return app.isPackaged && existsSync(join(process.resourcesPath, 'app-update.yml'))
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

    void this.check()
    this.timer = setInterval(() => void this.check(), 4 * 60 * 60 * 1000) // every 4 hours
  }

  async check(): Promise<void> {
    if (!this.enabled) return this.set({ state: 'disabled' })
    if (this.status.state === 'downloading' || this.status.state === 'ready') return
    try {
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
  if (/\b404\b/.test(msg)) return 'No release found on GitHub yet.'
  if (/ENOTFOUND|ETIMEDOUT|ECONNRESET|net::/i.test(msg)) return 'Could not reach GitHub. Check your internet connection.'
  return msg.split('\n')[0].slice(0, 300)
}
