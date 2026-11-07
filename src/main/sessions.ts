import { existsSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs'
import { join } from 'node:path'
import type { SessionInfo, SessionTask } from '../shared/types'
import type { HookEvent } from './hookServer'
import { SESSIONS_DIR } from './usage'

/**
 * Live state per terminal tab, built from:
 * - Claude Code hooks (hookServer.ts): activity, plan, tasks, subagents, files written.
 * - The status line's per-session file (usage.ts writes <profile>/cc-launcher-sessions/<id>.json):
 *   model, context window, cost estimate, lines added/removed, duration.
 *
 * Hook payload fields follow https://code.claude.com/docs/en/hooks. Where the docs don't fix a
 * shape (Task tool inputs, ExitPlanMode's plan field), parsing is lenient.
 */

type Emit = (info: SessionInfo) => void

const str = (v: unknown): string | undefined => (typeof v === 'string' && v.trim() ? v : undefined)
const obj = (v: unknown): Record<string, unknown> => (v && typeof v === 'object' ? (v as Record<string, unknown>) : {})
const short = (s: string | undefined, n = 240) => (s && s.length > n ? `${s.slice(0, n - 1)}…` : s)

const WAITING_NOTIFICATIONS = new Set([
  'permission_prompt',
  'elicitation_dialog',
  'elicitation_url_dialog',
  'agent_needs_input'
])

export class SessionTracker {
  private tabs = new Map<string, SessionInfo>()
  private profileOf = new Map<string, string>() // tabId -> profile dir
  private timer: NodeJS.Timeout | null = null

  constructor(private emit: Emit) {}

  all(): Record<string, SessionInfo> {
    return Object.fromEntries(this.tabs)
  }

  open(tabId: string, profileDir: string): void {
    this.profileOf.set(tabId, profileDir)
    this.tabs.set(tabId, { tabId, activity: 'starting', activityAt: Date.now(), tasks: [], agents: [], outputs: [] })
    this.timer ??= setInterval(() => this.pollStatus(), 3000)
    this.update(tabId, () => {}) // let the UI know about the new tab right away
  }

  /** The claude process of a tab exited (the tab may stay open to show its last screen). */
  exited(tabId: string): void {
    this.update(tabId, (s) => {
      this.setActivity(s, 'ended')
      for (const a of s.agents) a.running = false
    })
  }

  close(tabId: string): void {
    this.tabs.delete(tabId)
    this.profileOf.delete(tabId)
    this.chatCostBase.delete(tabId)
    this.chatCostRun.delete(tabId)
  }

  private update(tabId: string, fn: (s: SessionInfo) => void): void {
    const s = this.tabs.get(tabId)
    if (!s) return
    fn(s)
    this.emit({ ...s, tasks: [...s.tasks], agents: [...s.agents], outputs: [...s.outputs] })
  }

  private setActivity(s: SessionInfo, a: SessionInfo['activity']): void {
    if (s.activity !== a) {
      s.activity = a
      s.activityAt = Date.now()
    }
  }

  handle(tabId: string, e: HookEvent): void {
    if (!this.tabs.has(tabId)) return
    const name = e.hook_event_name
    this.update(tabId, (s) => {
      if (str(e.session_id)) s.sessionId = e.session_id as string
      if (str(e.transcript_path) && !e.agent_id) s.transcriptPath = e.transcript_path as string
      switch (name) {
        case 'SessionStart': {
          const m = e.model
          s.model = str(m) ?? str(obj(m).display_name) ?? s.model
          s.sessionName = str(e.session_title) ?? s.sessionName
          this.setActivity(s, 'idle')
          break
        }
        case 'UserPromptSubmit':
          s.sessionName = str(e.session_title) ?? s.sessionName
          s.error = undefined
          this.setActivity(s, 'working')
          break
        case 'PreToolUse':
          if (e.tool_name === 'ExitPlanMode') {
            const input = obj(e.tool_input)
            const text =
              str(input.plan) ??
              Object.values(input)
                .filter((v): v is string => typeof v === 'string')
                .sort((a, b) => b.length - a.length)[0]
            if (text) s.plan = { text, at: Date.now() }
            this.setActivity(s, 'needs-you') // plan approval
          }
          break
        case 'PostToolUse':
          if (s.activity !== 'working') this.setActivity(s, 'working')
          this.onTool(s, e)
          break
        case 'TaskCreated': {
          const id = str(e.task_id)
          if (id && !s.tasks.some((t) => t.id === id)) {
            s.tasks.push({ id, subject: str(e.task_subject) ?? id, status: 'pending' })
          }
          break
        }
        case 'TaskCompleted': {
          const t = s.tasks.find((t) => t.id === str(e.task_id))
          if (t) t.status = 'completed'
          break
        }
        case 'SubagentStart': {
          const type = str(e.agent_type)
          if (!type) break // internal helper agents
          s.agents.push({ id: str(e.agent_id) ?? String(Date.now()), kind: type, running: true, startedAt: Date.now() })
          break
        }
        case 'SubagentStop': {
          const a = s.agents.find((a) => a.id === str(e.agent_id))
          if (a) {
            a.running = false
            a.endedAt = Date.now()
          }
          break
        }
        case 'PermissionRequest':
          s.message = `Wants to use ${str(e.tool_name) ?? 'a tool'}`
          this.setActivity(s, 'needs-you')
          break
        case 'Notification': {
          const type = str(e.notification_type) ?? ''
          if (WAITING_NOTIFICATIONS.has(type)) {
            s.message = short(str(e.message))
            this.setActivity(s, 'needs-you')
          }
          break
        }
        case 'Stop':
          s.message = short(str(e.last_assistant_message))
          this.setActivity(s, 'done')
          break
        case 'StopFailure':
          s.error = short(str(e.error_details) ?? str(e.last_assistant_message) ?? str(e.error), 300)
          this.setActivity(s, 'error')
          break
        case 'SessionEnd':
          this.setActivity(s, 'ended')
          break
      }
    })
  }

  private onTool(s: SessionInfo, e: HookEvent): void {
    const tool = str(e.tool_name) ?? ''
    const input = obj(e.tool_input)
    if (tool === 'Write' || tool === 'Edit' || tool === 'MultiEdit' || tool === 'NotebookEdit') {
      const path = str(input.file_path) ?? str(input.notebook_path)
      if (!path) return
      const existing = s.outputs.find((o) => o.path === path)
      if (existing) {
        existing.count++
        existing.at = Date.now()
        if (tool === 'Write') existing.op = 'written'
      } else {
        s.outputs.unshift({ path, op: tool === 'Write' ? 'written' : 'edited', count: 1, at: Date.now() })
      }
      return
    }
    if (tool === 'TaskUpdate') {
      const id = str(input.taskId) ?? str(input.task_id) ?? str(input.id)
      const status = str(input.status)
      const t = s.tasks.find((t) => t.id === id)
      if (t && (status === 'pending' || status === 'in_progress' || status === 'completed')) t.status = status
      if (t && str(input.subject)) t.subject = input.subject as string
      return
    }
    if (tool === 'TodoWrite' && Array.isArray(input.todos)) {
      s.tasks = (input.todos as unknown[]).map((raw, i) => {
        const t = obj(raw)
        const status = str(t.status)
        return {
          id: str(t.id) ?? String(i),
          subject: str(t.content) ?? str(t.subject) ?? `Task ${i + 1}`,
          status: (status === 'in_progress' || status === 'completed' ? status : 'pending') as SessionTask['status']
        }
      })
      return
    }
    if (tool === 'Bash' && input.run_in_background === true) {
      s.agents.push({
        id: str(e.tool_use_id) ?? String(Date.now()),
        kind: 'bash',
        label: short(str(input.description) ?? str(input.command), 80),
        running: true,
        startedAt: Date.now()
      })
    }
  }

  private chatCostBase = new Map<string, number>()
  private chatCostRun = new Map<string, number>()

  /**
   * Chat tabs have no status line (print mode), so model / context / cost come from the stream-json
   * events themselves: assistant `usage` (current context) and `result` (cost estimate, context window).
   */
  chatEvent(tabId: string, e: Record<string, unknown>): void {
    if (!this.tabs.has(tabId)) return
    if (e.type === 'system' && e.subtype === 'init') {
      const model = str(e.model)
      if (model) this.update(tabId, (s) => (s.model ??= prettyModel(model)))
    } else if (e.type === 'assistant' && !e.parent_tool_use_id) {
      const u = obj(obj(e.message).usage)
      const n = (k: string) => (typeof u[k] === 'number' ? (u[k] as number) : 0)
      const tokens = n('input_tokens') + n('cache_read_input_tokens') + n('cache_creation_input_tokens')
      if (tokens > 0)
        this.update(tabId, (s) => {
          const size = s.context?.size ?? 200_000
          s.context = { usedPercent: Math.round((tokens / size) * 100), tokens, size }
        })
    } else if (e.type === 'result') {
      const run = typeof e.total_cost_usd === 'number' ? e.total_cost_usd : 0
      this.chatCostRun.set(tabId, run)
      const window = Object.values(obj(e.modelUsage)).map((m) => obj(m).contextWindow).find((w) => typeof w === 'number') as number | undefined
      this.update(tabId, (s) => {
        s.costUsd = (this.chatCostBase.get(tabId) ?? 0) + run
        s.durationMs = (s.durationMs ?? 0) + (typeof e.duration_ms === 'number' ? e.duration_ms : 0)
        if (window && s.context) s.context = { ...s.context, size: window, usedPercent: Math.round((s.context.tokens / window) * 100) }
      })
    }
  }

  /** A chat process ended: its cost total becomes the base for the next (resumed) process. */
  chatProcessEnded(tabId: string): void {
    this.chatCostBase.set(tabId, (this.chatCostBase.get(tabId) ?? 0) + (this.chatCostRun.get(tabId) ?? 0))
    this.chatCostRun.delete(tabId)
    this.update(tabId, (s) => {
      if (s.activity === 'working' || s.activity === 'needs-you') this.setActivity(s, 'idle')
    })
  }

  /** A message was sent in a chat tab. */
  chatWorking(tabId: string): void {
    this.update(tabId, (s) => {
      s.error = undefined
      this.setActivity(s, 'working')
    })
  }

  /** Reads the status line's per-session files (model, context, cost) for open tabs. */
  private pollStatus(): void {
    for (const [tabId, s] of this.tabs) {
      const dir = this.profileOf.get(tabId)
      if (!dir) continue
      const folder = join(dir, SESSIONS_DIR)
      let file = s.sessionId ? join(folder, `${s.sessionId}.json`) : null
      // Before the first hook arrives, find the file the status line tagged with this tab.
      if (!file && existsSync(folder)) {
        for (const f of readdirSync(folder)) {
          try {
            const d = JSON.parse(readFileSync(join(folder, f), 'utf8').replace(/^﻿/, ''))
            if (d.tab === tabId) file = join(folder, f)
          } catch {
            // skip
          }
        }
      }
      if (!file || !existsSync(file)) continue
      try {
        const d = JSON.parse(readFileSync(file, 'utf8').replace(/^﻿/, '')) as Record<string, unknown>
        const cw = obj(d.context_window)
        const cost = obj(d.cost)
        const next = {
          sessionId: str(d.session_id) ?? s.sessionId,
          sessionName: str(d.session_name) ?? s.sessionName,
          model: str(obj(d.model).display_name) ?? s.model,
          context:
            typeof cw.context_window_size === 'number'
              ? {
                  usedPercent: typeof cw.used_percentage === 'number' ? cw.used_percentage : null,
                  tokens: typeof cw.total_input_tokens === 'number' ? cw.total_input_tokens : 0,
                  size: cw.context_window_size as number
                }
              : s.context,
          costUsd: typeof cost.total_cost_usd === 'number' ? cost.total_cost_usd : s.costUsd,
          durationMs: typeof cost.total_duration_ms === 'number' ? cost.total_duration_ms : s.durationMs,
          linesAdded: typeof cost.total_lines_added === 'number' ? cost.total_lines_added : s.linesAdded,
          linesRemoved: typeof cost.total_lines_removed === 'number' ? cost.total_lines_removed : s.linesRemoved
        }
        const changed = Object.entries(next).some(([k, v]) => JSON.stringify(v) !== JSON.stringify((s as unknown as Record<string, unknown>)[k]))
        if (changed) this.update(tabId, (x) => Object.assign(x, next))
      } catch {
        // half-written file: next poll
      }
    }
  }
}

/** "claude-opus-5-5" → "Opus 5.5" */
function prettyModel(id: string): string {
  const m = id.match(/claude-([a-z]+)-(\d+)-(\d+)/)
  return m ? `${m[1][0].toUpperCase()}${m[1].slice(1)} ${m[2]}.${m[3]}` : id
}

/** Deletes status-line session files older than a week. */
export function pruneSessionFiles(profileDirs: string[]): void {
  const cutoff = Date.now() - 7 * 86_400_000
  for (const dir of profileDirs) {
    const folder = join(dir, SESSIONS_DIR)
    if (!existsSync(folder)) continue
    for (const f of readdirSync(folder)) {
      try {
        if (statSync(join(folder, f)).mtimeMs < cutoff) rmSync(join(folder, f), { force: true })
      } catch {
        // ignore
      }
    }
  }
}
