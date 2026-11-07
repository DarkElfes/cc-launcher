import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { app, safeStorage } from 'electron'

/**
 * Read-only GitHub token used only to download updates from the private releases repo.
 * Stored encrypted with Electron safeStorage (Windows DPAPI, tied to this Windows user).
 * Never sent to the renderer, never put in process.env (so it can't leak into the
 * claude processes this app starts), and never built into the installer.
 */

const file = () => join(app.getPath('userData'), 'update-token.bin')

export function hasUpdateToken(): boolean {
  return existsSync(file())
}

export function getUpdateToken(): string | null {
  if (!hasUpdateToken() || !safeStorage.isEncryptionAvailable()) return null
  try {
    return safeStorage.decryptString(readFileSync(file()))
  } catch {
    return null
  }
}

/** Saves the token, or deletes it when `token` is empty. */
export function setUpdateToken(token: string): void {
  const t = token.trim()
  if (!t) {
    rmSync(file(), { force: true })
    return
  }
  if (!/^(github_pat_|ghp_)[A-Za-z0-9_]+$/.test(t)) {
    throw new Error('That does not look like a GitHub token (it should start with github_pat_ or ghp_).')
  }
  if (!safeStorage.isEncryptionAvailable()) {
    throw new Error('Windows encryption is not available, so the token cannot be stored safely.')
  }
  mkdirSync(dirname(file()), { recursive: true })
  writeFileSync(file(), safeStorage.encryptString(t))
}
