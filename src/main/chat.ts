import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { extname } from 'node:path'
import type { ChatPermissionDecision, ChatPermissionRequest } from '../shared/types'

/**
 * Chat mode: drives the official Claude Code CLI in print mode with documented stream-json I/O
 * (https://code.claude.com/docs/en/headless):
 *
 *   claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages
 *
 * One long-running process per chat tab. User turns go to stdin as JSON lines, events come back on
 * stdout and are forwarded to the renderer, which draws the conversation itself.
 *
 * Permissions: Claude Code runs the PermissionRequest hook (our HTTP hook server) before a tool that
 * needs approval. For chat tabs the request is held open until you click Allow/Deny in the chat
 * (https://code.claude.com/docs/en/hooks#permissionrequest). Without a decision, -p denies the call.
 *
 * Stop: there's no documented stdin interrupt, so the process is ended; the next message resumes the
 * same session with --resume <session id>.
 */

export interface ChatStartOptions {
  claude: string
  cwd: string
  env: Record<string, string>
  resume?: string
  permissionMode?: string
}

type Send = (channel: string, payload: unknown) => void

interface ChatProc {
  proc: ChildProcessWithoutNullStreams | null
  opts: ChatStartOptions
  sessionId?: string
  buf: string
  stderr: string[]
  pendingPermissions: Map<string, (d: ChatPermissionDecision | null) => void>
}

/** Keys of the init/result events that are only diagnostics; dropped before forwarding. */
const NOISE = ['startup_timing', 'time_to_request_phases_ms', 'input_attachments_detail', 'tool_schema_build_detail', 'message_normalization_detail', 'request_build_detail', 'model_usage_since_last_result', 'subagent_stats']

const MEDIA: Record<string, string> = { '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp' }

export class ChatManager {
  private chats = new Map<string, ChatProc>()

  constructor(
    private send: Send,
    private hooks: {
      onEvent: (tabId: string, e: Record<string, unknown>) => void
      onExit: (tabId: string) => void
    }
  ) {}

  has(tabId: string): boolean {
    return this.chats.has(tabId)
  }

  open(tabId: string, opts: ChatStartOptions): void {
    this.chats.set(tabId, { proc: null, opts, sessionId: opts.resume, buf: '', stderr: [], pendingPermissions: new Map() })
  }

  private start(tabId: string): ChatProc | undefined {
    const c = this.chats.get(tabId)
    if (!c) return
    if (c.proc) return c
    const args = ['-p', '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose', '--include-partial-messages']
    if (c.sessionId) args.push('--resume', c.sessionId)
    if (c.opts.permissionMode) args.push('--permission-mode', c.opts.permissionMode)
    const proc = spawn(c.opts.claude, args, { cwd: c.opts.cwd, env: c.opts.env, windowsHide: true })
    c.proc = proc
    c.buf = ''
    c.stderr = []
    proc.stdout.setEncoding('utf8')
    proc.stdout.on('data', (chunk: string) => this.onData(tabId, chunk))
    proc.stderr.setEncoding('utf8')
    proc.stderr.on('data', (chunk: string) => {
      c.stderr.push(...chunk.split(/\r?\n/).filter(Boolean))
      c.stderr = c.stderr.slice(-20)
    })
    proc.on('exit', (code) => {
      if (c.proc !== proc) return
      c.proc = null
      this.denyPending(c, 'The session stopped.')
      this.send('chat:exit', { tabId, code, stderr: code ? c.stderr.slice(-5).join('\n') : '' })
      this.hooks.onExit(tabId)
    })
    proc.on('error', (err) => {
      if (c.proc !== proc) return
      c.proc = null
      this.send('chat:exit', { tabId, code: -1, stderr: err.message })
    })
    return c
  }

  private onData(tabId: string, chunk: string): void {
    const c = this.chats.get(tabId)
    if (!c) return
    c.buf += chunk
    let i: number
    while ((i = c.buf.indexOf('\n')) >= 0) {
      const line = c.buf.slice(0, i).trim()
      c.buf = c.buf.slice(i + 1)
      if (!line) continue
      let e: Record<string, unknown>
      try {
        e = JSON.parse(line)
      } catch {
        continue
      }
      if (typeof e.session_id === 'string') c.sessionId = e.session_id
      for (const k of NOISE) delete e[k]
      this.hooks.onEvent(tabId, e)
      this.send('chat:event', { tabId, event: e })
    }
  }

  /** Sends a user turn (text + image files). Starts or resumes the process if needed. */
  sendMessage(tabId: string, text: string, imagePaths: string[]): string | null {
    const c = this.start(tabId)
    if (!c?.proc) return 'Could not start Claude Code.'
    const content: Array<Record<string, unknown>> = []
    for (const p of imagePaths) {
      const media = MEDIA[extname(p).toLowerCase()]
      if (!media || !existsSync(p)) continue
      content.push({ type: 'image', source: { type: 'base64', media_type: media, data: readFileSync(p).toString('base64') } })
    }
    if (text.trim()) content.push({ type: 'text', text })
    if (!content.length) return null
    const msg = { type: 'user', message: { role: 'user', content }, parent_tool_use_id: null }
    c.proc.stdin.write(JSON.stringify(msg) + '\n')
    return null
  }

  /** Ends the running turn by ending the process; the next message resumes the session. */
  stop(tabId: string): void {
    const c = this.chats.get(tabId)
    const proc = c?.proc
    if (!c || !proc) return
    c.proc = null
    this.denyPending(c, 'Stopped by the user.')
    killTree(proc)
    this.send('chat:exit', { tabId, code: 0, stopped: true })
  }

  /** Restart with another permission mode (keeps the conversation). */
  setPermissionMode(tabId: string, mode: string | undefined): void {
    const c = this.chats.get(tabId)
    if (!c) return
    c.opts.permissionMode = mode
    if (c.proc) {
      const proc = c.proc
      c.proc = null
      killTree(proc)
    }
  }

  close(tabId: string): void {
    const c = this.chats.get(tabId)
    if (!c) return
    this.denyPending(c, 'The tab was closed.')
    if (c.proc) killTree(c.proc)
    this.chats.delete(tabId)
  }

  closeAll(): void {
    for (const id of [...this.chats.keys()]) this.close(id)
  }

  /**
   * PermissionRequest hook for a chat tab: ask the renderer and wait (up to just under the hook timeout).
   * Resolves to the hook's JSON response, or undefined (no decision → Claude Code denies in -p).
   */
  requestPermission(tabId: string, e: Record<string, unknown>): Promise<Record<string, unknown> | undefined> {
    const c = this.chats.get(tabId)
    if (!c) return Promise.resolve(undefined)
    const request: ChatPermissionRequest = {
      tabId,
      requestId: randomUUID(),
      toolName: String(e.tool_name ?? ''),
      input: (e.tool_input as Record<string, unknown>) ?? {},
      suggestions: Array.isArray(e.permission_suggestions) ? e.permission_suggestions : []
    }
    return new Promise((resolve) => {
      const timer = setTimeout(() => finish(null), 590_000)
      const finish = (d: ChatPermissionDecision | null) => {
        clearTimeout(timer)
        c.pendingPermissions.delete(request.requestId)
        this.send('chat:permission-done', { tabId, requestId: request.requestId, decision: d?.behavior ?? 'expired' })
        if (!d) return resolve(undefined)
        const decision: Record<string, unknown> =
          d.behavior === 'allow'
            ? { behavior: 'allow', updatedInput: d.updatedInput ?? request.input, ...(d.updatedPermissions ? { updatedPermissions: d.updatedPermissions } : {}) }
            : { behavior: 'deny', message: d.message ?? 'The user denied this.', ...(d.interrupt ? { interrupt: true } : {}) }
        resolve({ hookSpecificOutput: { hookEventName: 'PermissionRequest', decision } })
      }
      c.pendingPermissions.set(request.requestId, finish)
      this.send('chat:permission', request)
    })
  }

  replyPermission(tabId: string, requestId: string, decision: ChatPermissionDecision): void {
    this.chats.get(tabId)?.pendingPermissions.get(requestId)?.(decision)
  }

  private denyPending(c: ChatProc, message: string): void {
    for (const finish of [...c.pendingPermissions.values()]) finish({ behavior: 'deny', message })
  }
}

function killTree(proc: ChildProcessWithoutNullStreams): void {
  if (process.platform === 'win32' && proc.pid) {
    spawn('taskkill', ['/pid', String(proc.pid), '/T', '/F'], { windowsHide: true }).on('error', () => proc.kill())
  } else {
    proc.kill('SIGINT')
  }
}

/**
 * Earlier messages of a resumed session, from its transcript file. The transcript's entries share
 * the stream-json message shape for user/assistant lines; anything else is skipped. Best effort: the
 * file format is internal to Claude Code.
 */
export function readTranscriptMessages(path: string, limit = 400): Array<Record<string, unknown>> {
  if (!existsSync(path)) return []
  const out: Array<Record<string, unknown>> = []
  for (const line of readFileSync(path, 'utf8').split('\n')) {
    if (!line.trim()) continue
    try {
      const e = JSON.parse(line) as Record<string, unknown>
      if ((e.type === 'user' || e.type === 'assistant') && e.message && !e.isMeta && !e.isSidechain) {
        out.push({ type: e.type, message: e.message, parent_tool_use_id: null, uuid: e.uuid, history: true })
      }
    } catch {
      // skip
    }
  }
  return out.slice(-limit)
}
