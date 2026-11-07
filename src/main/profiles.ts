import { existsSync, mkdirSync, readdirSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import type { ProfileInfo } from '../shared/types'
import { ensureStatusLine, readUsage } from './usage'

/**
 * A profile is one Claude account = one config folder under ~/.claude-profiles, passed to
 * Claude Code as CLAUDE_CONFIG_DIR. Each folder holds that account's login, settings and session
 * history.
 *
 * The normal ~/.claude folder is deliberately NOT a profile. Plain `claude` in any terminal, and
 * other tools, use it too, so a login there could silently switch "which account" a profile
 * means. Every account the app manages gets its own folder.
 *
 * This module only checks whether a credentials file exists. It never opens, copies or sends it.
 */

export const PROFILES_ROOT = join(homedir(), '.claude-profiles')

const NAME_RE = /^[a-z0-9][a-z0-9_-]{0,31}$/

export function validateProfileName(name: string): string | null {
  if (!NAME_RE.test(name)) return 'Use 1–32 lowercase letters, digits, "-" or "_", starting with a letter or digit.'
  return null
}

export function profileDir(name: string): string {
  return join(PROFILES_ROOT, name)
}

function info(name: string): ProfileInfo {
  const dir = profileDir(name)
  return { name, dir, signedIn: existsSync(join(dir, '.credentials.json')), usage: readUsage(dir) }
}

/** Every folder under ~/.claude-profiles, sorted by name. */
export function listProfiles(): ProfileInfo[] {
  if (!existsSync(PROFILES_ROOT)) return []
  return readdirSync(PROFILES_ROOT, { withFileTypes: true })
    .filter((e) => e.isDirectory() && NAME_RE.test(e.name))
    .map((e) => e.name)
    .sort()
    .map(info)
}

export function profileExists(name: string): boolean {
  return !validateProfileName(name) && existsSync(profileDir(name))
}

export function createProfile(name: string): void {
  mkdirSync(profileDir(name), { recursive: true })
  ensureStatusLine(profileDir(name))
}

/** Makes sure every profile reports its plan usage (see usage.ts). Safe to call often. */
export function ensureAllStatusLines(): void {
  for (const p of listProfiles()) {
    try {
      ensureStatusLine(p.dir)
    } catch {
      // A locked or read-only folder shouldn't break the app.
    }
  }
}

/** Environment for a Claude Code process running under the given profile. */
export function profileEnv(name: string): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [k, v] of Object.entries(process.env)) if (v !== undefined) env[k] = v
  // Don't leak settings from whatever launched this app (for example a Claude Code session).
  delete env.CLAUDECODE
  delete env.CLAUDE_CODE_ENTRYPOINT
  env.CLAUDE_CONFIG_DIR = profileDir(name)
  env.COLORTERM = 'truecolor'
  return env
}
