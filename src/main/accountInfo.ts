import { execFile } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import type { AccountInfo, ProfileUsage, UsageWindow } from '../shared/types'

/**
 * Account details and plan usage per profile, from Claude Code's own CLI (no credentials are read):
 *
 * - `claude auth status --json`: email, plan (subscriptionType), org, loggedIn.
 * - `claude -p "/usage" --output-format json`: the /usage screen as text. It runs locally as a
 *   slash command (no model call, $0 cost, doesn't touch the limits), e.g.
 *     "Current session: 33% used · resets 11pm (Europe/Kyiv)"
 *     "Current week (all models): 5% used · resets Nov 10, 7pm (Europe/Kyiv)"
 *   The text format isn't a documented API, so parsing is defensive. The raw "resets …" label is
 *   kept for display when the time can't be parsed.
 *
 * Results are cached; refresh() runs in the background and calls onChange when done.
 */

export interface ProfileLive {
  account?: AccountInfo
  usage?: ProfileUsage
  error?: string
  fetchedAt?: number
}

type Env = Record<string, string>

function run(file: string, args: string[], env: Env, cwd: string, timeout = 30_000): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(file, args, { env, cwd, timeout, windowsHide: true, maxBuffer: 4 * 1024 * 1024 }, (err, stdout) => {
      if (err && !stdout) reject(err)
      else resolve(String(stdout))
    })
  })
}

export async function fetchAccount(claude: string, env: Env, cwd: string): Promise<AccountInfo> {
  const j = JSON.parse(await run(claude, ['auth', 'status', '--json'], env, cwd)) as Record<string, unknown>
  const str = (v: unknown) => (typeof v === 'string' && v.trim() ? v.trim() : undefined)
  return {
    loggedIn: j.loggedIn === true,
    email: str(j.email),
    plan: str(j.subscriptionType) ?? str(j.planTier),
    orgName: str(j.orgName)
  }
}

const MONTHS = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec']

/** "Nov 10, 7pm" / "11pm" / "10:30pm" (local time, as shown by Claude Code) → epoch ms, or null. */
export function parseResetTime(text: string, now = new Date()): number | null {
  const m = text.trim().match(/^(?:([A-Za-z]{3})[a-z]*\.? (\d{1,2}),? )?(\d{1,2})(?::(\d{2}))?\s*(am|pm)$/i)
  if (!m) return null
  let hour = Number(m[3]) % 12
  if (m[5].toLowerCase() === 'pm') hour += 12
  const minute = m[4] ? Number(m[4]) : 0
  const d = new Date(now)
  d.setSeconds(0, 0)
  d.setHours(hour, minute)
  if (m[1]) {
    const month = MONTHS.indexOf(m[1].toLowerCase())
    if (month < 0) return null
    d.setMonth(month, Number(m[2]))
    if (d.getTime() < now.getTime() - 86_400_000) d.setFullYear(d.getFullYear() + 1)
  } else if (d.getTime() <= now.getTime()) {
    d.setDate(d.getDate() + 1)
  }
  return d.getTime()
}

function parseLine(line: string): UsageWindow | undefined {
  const m = line.match(/:\s*(\d+(?:\.\d+)?)%\s*used(?:\s*[·•-]\s*resets\s+(.+))?$/i)
  if (!m) return undefined
  const label = m[2]?.replace(/\s*\([^)]*\)\s*$/, '').trim() // drop "(Europe/Kyiv)"
  return {
    usedPercent: Math.min(100, Number(m[1])),
    resetsAt: label ? parseResetTime(label) : null,
    resetsLabel: label,
    stale: false
  }
}

/** Parses the text of /usage into the 5-hour (session) and weekly windows. */
export function parseUsageText(text: string, now = Date.now()): ProfileUsage | undefined {
  let fiveHour: UsageWindow | undefined
  let week: UsageWindow | undefined
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim()
    if (/^current session/i.test(line)) fiveHour = parseLine(line)
    else if (/^current week \(all models\)/i.test(line) || (!week && /^current week/i.test(line))) week = parseLine(line) ?? week
  }
  if (!fiveHour && !week) return undefined
  return { updatedAt: now, fiveHour, week, source: 'usage-command' }
}

export async function fetchUsage(claude: string, env: Env, cwd: string): Promise<ProfileUsage | undefined> {
  const out = await run(claude, ['-p', '/usage', '--output-format', 'json'], env, cwd, 45_000)
  const j = JSON.parse(out) as { result?: unknown; is_error?: boolean }
  if (j.is_error || typeof j.result !== 'string') throw new Error(typeof j.result === 'string' ? j.result : 'usage command failed')
  return parseUsageText(j.result)
}

export class AccountMonitor {
  private cache = new Map<string, ProfileLive>()
  private running: Promise<void> | null = null
  private timer: NodeJS.Timeout | null = null

  constructor(
    private workDir: string,
    private deps: {
      claudePath: () => string | null
      profiles: () => string[]
      env: (profile: string) => Env
      onChange: () => void
    }
  ) {
    mkdirSync(workDir, { recursive: true })
  }

  get(profile: string): ProfileLive | undefined {
    return this.cache.get(profile)
  }

  get busy(): boolean {
    return this.running !== null
  }

  start(intervalMs = 5 * 60_000): void {
    void this.refresh()
    this.timer ??= setInterval(() => void this.refresh(), intervalMs)
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer)
    this.timer = null
  }

  /** Refreshes all profiles (or one), one at a time. Concurrent calls share the same run. */
  refresh(only?: string): Promise<void> {
    if (this.running) return this.running
    this.running = (async () => {
      const claude = this.deps.claudePath()
      if (!claude) return
      const names = only ? [only] : this.deps.profiles()
      for (const name of names) {
        const env = this.deps.env(name)
        const cwd = join(this.workDir, name)
        mkdirSync(cwd, { recursive: true })
        const prev = this.cache.get(name) ?? {}
        const next: ProfileLive = { ...prev, error: undefined }
        try {
          next.account = await fetchAccount(claude, env, cwd)
          if (next.account.loggedIn) next.usage = (await fetchUsage(claude, env, cwd)) ?? prev.usage
          else next.usage = undefined
          next.fetchedAt = Date.now()
        } catch (e) {
          next.error = e instanceof Error ? e.message.split('\n')[0].slice(0, 200) : String(e)
        }
        this.cache.set(name, next)
        this.deps.onChange()
      }
      // Forget removed profiles.
      if (!only) for (const k of [...this.cache.keys()]) if (!names.includes(k)) this.cache.delete(k)
    })().finally(() => {
      this.running = null
      this.deps.onChange()
    })
    return this.running
  }
}
