import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { ProfileUsage, UsageWindow } from '../shared/types'

/**
 * Plan usage (5-hour and weekly limits) per profile.
 *
 * The only documented source is Claude Code's status line: its stdin JSON carries
 * `rate_limits.five_hour` / `rate_limits.seven_day` with `used_percentage` and `resets_at`
 * (Pro/Max only, and only after the first API response in a session). See
 * https://code.claude.com/docs/en/statusline.md
 *
 * For each profile the app installs a small PowerShell status-line script into the profile folder
 * and points that profile's settings.json `statusLine` at it. The script saves `rate_limits` to
 * cc-launcher-usage.json (next to it) and prints a short status line. No credentials are touched.
 * Numbers refresh whenever a Claude Code session of that profile is running.
 */

const SCRIPT_NAME = 'cc-launcher-statusline.ps1'
const USAGE_FILE = 'cc-launcher-usage.json'
/** Per-session status files (model, context, cost) read by sessions.ts. */
export const SESSIONS_DIR = 'cc-launcher-sessions'

const SCRIPT = `# Written by cc-launcher. Saves plan usage (rate_limits) and per-session info (model, context,
# cost) for the app, and prints a short status line.
$ErrorActionPreference = 'SilentlyContinue'
$j = [Console]::In.ReadToEnd() | ConvertFrom-Json
$now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
if ($j.rate_limits) {
  $out = Join-Path $PSScriptRoot '${USAGE_FILE}'
  $data = @{ updatedAt = $now; rate_limits = $j.rate_limits }
  [IO.File]::WriteAllText("$out.tmp", ($data | ConvertTo-Json -Depth 6 -Compress))
  Move-Item -LiteralPath "$out.tmp" -Destination $out -Force
}
if ($j.session_id -match '^[A-Za-z0-9-]+$') {
  $dir = Join-Path $PSScriptRoot '${SESSIONS_DIR}'
  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
  $out = Join-Path $dir ($j.session_id + '.json')
  $data = @{ updatedAt = $now; tab = $env:CCL_TAB_ID; session_id = $j.session_id; session_name = $j.session_name; model = $j.model; cost = $j.cost; context_window = $j.context_window }
  [IO.File]::WriteAllText("$out.tmp", ($data | ConvertTo-Json -Depth 6 -Compress))
  Move-Item -LiteralPath "$out.tmp" -Destination $out -Force
}
$parts = @()
if ($j.model.display_name) { $parts += $j.model.display_name }
if ($null -ne $j.context_window.used_percentage) { $parts += ('ctx {0:N0}%' -f [double]$j.context_window.used_percentage) }
if ($j.rate_limits.five_hour) { $parts += ('5h {0:N0}%' -f [double]$j.rate_limits.five_hour.used_percentage) }
if ($j.rate_limits.seven_day) { $parts += ('week {0:N0}%' -f [double]$j.rate_limits.seven_day.used_percentage) }
[Console]::Out.Write(($parts -join ' | '))
`

function statusLineCommand(dir: string): string {
  // Forward slashes and quotes: Claude Code may run this through Git Bash or PowerShell.
  const script = join(dir, SCRIPT_NAME).replace(/\\/g, '/')
  return `powershell -NoProfile -ExecutionPolicy Bypass -File "${script}"`
}

/**
 * Installs/refreshes the script and wires it into <profile>/settings.json.
 * Leaves settings.json alone if it isn't valid JSON or already has someone else's statusLine.
 */
export function ensureStatusLine(dir: string): void {
  if (!existsSync(dir)) return
  const scriptPath = join(dir, SCRIPT_NAME)
  if (!existsSync(scriptPath) || readFileSync(scriptPath, 'utf8') !== SCRIPT) writeFileSync(scriptPath, SCRIPT, 'utf8')

  const settingsPath = join(dir, 'settings.json')
  let settings: Record<string, unknown> = {}
  if (existsSync(settingsPath)) {
    try {
      settings = JSON.parse(readFileSync(settingsPath, 'utf8').replace(/^﻿/, ''))
    } catch {
      return // never clobber a settings file we can't parse
    }
  }
  const current = settings.statusLine as { command?: string } | undefined
  if (current && !String(current.command ?? '').includes(SCRIPT_NAME)) return // user's own status line
  const command = statusLineCommand(dir)
  if (current?.command === command) return
  settings.statusLine = { type: 'command', command }
  const tmp = `${settingsPath}.tmp`
  writeFileSync(tmp, JSON.stringify(settings, null, 2), 'utf8')
  renameSync(tmp, settingsPath)
}

function toWindow(raw: unknown, nowSec: number): UsageWindow | undefined {
  const w = raw as { used_percentage?: number; resets_at?: number } | undefined
  if (!w || typeof w.used_percentage !== 'number' || typeof w.resets_at !== 'number') return undefined
  // After the reset time the saved number is stale: the window has started over.
  if (w.resets_at <= nowSec) return { usedPercent: 0, resetsAt: null, stale: true }
  return { usedPercent: Math.max(0, Math.min(100, w.used_percentage)), resetsAt: w.resets_at * 1000, stale: false }
}

export function readUsage(dir: string): ProfileUsage | undefined {
  const file = join(dir, USAGE_FILE)
  if (!existsSync(file)) return undefined
  try {
    const data = JSON.parse(readFileSync(file, 'utf8').replace(/^﻿/, '')) as {
      updatedAt?: number
      rate_limits?: { five_hour?: unknown; seven_day?: unknown }
    }
    const now = Date.now() / 1000
    return {
      updatedAt: (data.updatedAt ?? 0) * 1000,
      fiveHour: toWindow(data.rate_limits?.five_hour, now),
      week: toWindow(data.rate_limits?.seven_day, now),
      source: 'status-line'
    }
  } catch {
    return undefined
  }
}
