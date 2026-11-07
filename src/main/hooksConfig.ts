import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { HOOK_PATH } from './hookServer'

/**
 * Wires a profile's settings.json `hooks` to the app's local hook server
 * (https://code.claude.com/docs/en/hooks, `type: "http"`). Every entry the app adds has a url
 * ending in /ccl-hook, so it can be replaced on the next start (the port changes per run) without
 * touching the user's own hooks.
 *
 * The tab id and the per-run secret come from the claude process env (CCL_TAB_ID, CCL_HOOK_TOKEN)
 * via `allowedEnvVars`. Nothing secret is written to the file.
 */

const EVENTS: Array<{ event: string; matcher?: string }> = [
  { event: 'SessionStart' },
  { event: 'UserPromptSubmit' },
  { event: 'PreToolUse', matcher: 'ExitPlanMode' },
  { event: 'PostToolUse' },
  { event: 'Notification' },
  { event: 'Stop' },
  { event: 'StopFailure' },
  { event: 'SubagentStart' },
  { event: 'SubagentStop' },
  { event: 'TaskCreated' },
  { event: 'TaskCompleted' },
  { event: 'SessionEnd' }
]

type HookGroup = { matcher?: string; hooks?: Array<Record<string, unknown>> }

const isOurs = (h: Record<string, unknown>) => typeof h.url === 'string' && h.url.endsWith(HOOK_PATH)

export function ensureHooks(dir: string, port: number): void {
  if (!existsSync(dir) || !port) return
  const path = join(dir, 'settings.json')
  let settings: Record<string, unknown> = {}
  if (existsSync(path)) {
    try {
      settings = JSON.parse(readFileSync(path, 'utf8').replace(/^﻿/, ''))
    } catch {
      return // never clobber a settings file we can't parse
    }
  }
  const before = JSON.stringify(settings)
  const hook = {
    type: 'http',
    url: `http://127.0.0.1:${port}${HOOK_PATH}`,
    timeout: 3,
    headers: { 'X-CCL-Tab': '$CCL_TAB_ID', Authorization: 'Bearer $CCL_HOOK_TOKEN' },
    allowedEnvVars: ['CCL_TAB_ID', 'CCL_HOOK_TOKEN']
  }
  const hooks = (settings.hooks && typeof settings.hooks === 'object' ? settings.hooks : {}) as Record<string, HookGroup[]>
  for (const { event, matcher } of EVENTS) {
    // Drop our previous entries for this event, keep everything else.
    const groups = (Array.isArray(hooks[event]) ? hooks[event] : [])
      .map((g) => ({ ...g, hooks: (g.hooks ?? []).filter((h) => !isOurs(h)) }))
      .filter((g) => g.hooks.length > 0)
    groups.push(matcher ? { matcher, hooks: [hook] } : { hooks: [hook] })
    hooks[event] = groups
  }
  settings.hooks = hooks
  if (JSON.stringify(settings) === before) return
  const tmp = `${path}.tmp`
  writeFileSync(tmp, JSON.stringify(settings, null, 2), 'utf8')
  renameSync(tmp, path)
}
