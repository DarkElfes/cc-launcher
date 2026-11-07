import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { app, safeStorage } from 'electron'

/**
 * Read-only GitHub token used only to download updates from the private releases repo.
 *
 * Two sources, in order:
 * 1. A token saved in Settings: stored encrypted with Electron safeStorage (Windows DPAPI, tied to
 *    this Windows user). Overrides the built-in one, e.g. after the built-in token expires.
 * 2. A token built in at release time from the CCL_UPDATE_TOKEN env var of the building shell.
 *    It is never in a source file or in git, but anyone holding the installer could extract it.
 *    That's acceptable only because it is read-only and scoped to this one repo.
 *
 * The token is never sent to the renderer, and never put in process.env (so it can't leak into
 * the claude processes this app starts).
 */

export type TokenSource = 'saved' | 'built-in' | 'none'

const file = () => join(app.getPath('userData'), 'update-token.bin')
const builtIn = (): string => (typeof __BUNDLED_UPDATE_TOKEN__ === 'string' ? __BUNDLED_UPDATE_TOKEN__ : '')

function savedToken(): string | null {
  if (!existsSync(file()) || !safeStorage.isEncryptionAvailable()) return null
  try {
    return safeStorage.decryptString(readFileSync(file()))
  } catch {
    return null
  }
}

export function updateTokenSource(): TokenSource {
  if (existsSync(file())) return 'saved'
  return builtIn() ? 'built-in' : 'none'
}

export function getUpdateToken(): string | null {
  return savedToken() ?? (builtIn() || null)
}

/** Saves the token, or deletes the saved one (falling back to the built-in token) when `token` is empty. */
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
