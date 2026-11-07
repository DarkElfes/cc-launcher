import { existsSync, mkdirSync, readdirSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import type { ProfileInfo } from '../shared/types'

/**
 * A profile is one Claude account = one config folder passed to Claude Code as
 * CLAUDE_CONFIG_DIR. Each folder holds that account's login, settings and
 * session history. This module only checks whether a credentials file exists;
 * it never opens, copies or sends it anywhere.
 */

export const DEFAULT_PROFILE = 'default'
export const PROFILES_ROOT = join(homedir(), '.claude-profiles')

const NAME_RE = /^[a-z0-9][a-z0-9_-]{0,31}$/

export function validateProfileName(name: string): string | null {
  if (!NAME_RE.test(name)) return 'Use 1–32 lowercase letters, digits, "-" or "_", starting with a letter or digit.'
  if (name === DEFAULT_PROFILE) return '"default" is reserved for your normal ~/.claude folder.'
  return null
}

export function profileDir(name: string): string {
  return name === DEFAULT_PROFILE ? join(homedir(), '.claude') : join(PROFILES_ROOT, name)
}

function info(name: string): ProfileInfo {
  const dir = profileDir(name)
  return {
    name,
    dir,
    signedIn: existsSync(join(dir, '.credentials.json')),
    isDefault: name === DEFAULT_PROFILE
  }
}

/** The default profile first, then every folder under ~/.claude-profiles. */
export function listProfiles(): ProfileInfo[] {
  const names: string[] = []
  if (existsSync(PROFILES_ROOT)) {
    for (const entry of readdirSync(PROFILES_ROOT, { withFileTypes: true })) {
      if (entry.isDirectory() && NAME_RE.test(entry.name) && entry.name !== DEFAULT_PROFILE) names.push(entry.name)
    }
  }
  names.sort()
  return [info(DEFAULT_PROFILE), ...names.map(info)]
}

export function profileExists(name: string): boolean {
  return name === DEFAULT_PROFILE || existsSync(profileDir(name))
}

export function createProfile(name: string): void {
  mkdirSync(profileDir(name), { recursive: true })
}

/** Environment for a Claude Code process running under the given profile. */
export function profileEnv(name: string): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [k, v] of Object.entries(process.env)) if (v !== undefined) env[k] = v
  // Don't leak settings from whatever launched this app (for example a Claude Code session).
  delete env.CLAUDE_CONFIG_DIR
  delete env.CLAUDECODE
  delete env.CLAUDE_CODE_ENTRYPOINT
  if (name !== DEFAULT_PROFILE) env.CLAUDE_CONFIG_DIR = profileDir(name)
  env.COLORTERM = 'truecolor'
  return env
}
