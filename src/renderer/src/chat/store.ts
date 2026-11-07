import { useSyncExternalStore } from 'react'
import type { ChatPermissionRequest } from '../../../shared/types'

/**
 * Conversation state per chat tab, built from Claude Code's stream-json events
 * (https://code.claude.com/docs/en/headless). Kept outside React so it survives switching tabs.
 */

export interface ToolResult {
  text: string
  isError: boolean
  /** Claude Code's structured tool output (`tool_use_result`), shape depends on the tool. */
  structured?: unknown
}

export type ChatItem =
  | { kind: 'user'; id: string; text: string; images: string[]; imageCount?: number }
  | { kind: 'text'; id: string; msgId?: string; text: string; streaming: boolean }
  | { kind: 'thinking'; id: string; msgId?: string; text: string; streaming: boolean }
  | { kind: 'tool'; id: string; name: string; input: Record<string, unknown>; result?: ToolResult; steps: number }
  | { kind: 'permission'; id: string; request: ChatPermissionRequest; state: 'pending' | 'allow' | 'deny' | 'expired' }
  | { kind: 'notice'; id: string; text: string; tone: 'info' | 'warn' | 'error' }
  | { kind: 'result'; id: string; durationMs?: number; costUsd?: number; isError: boolean; subtype?: string; denials: string[] }

export interface ChatState {
  items: ChatItem[]
  busy: boolean
  model?: string
  permissionMode?: string
  slashCommands: string[]
}

const EMPTY: ChatState = { items: [], busy: false, slashCommands: [] }
const chats = new Map<string, ChatState>()
const listeners = new Set<() => void>()
let seq = 0
const nid = () => `i${++seq}`

function set(tabId: string, fn: (s: ChatState) => ChatState): void {
  chats.set(tabId, fn(chats.get(tabId) ?? EMPTY))
  for (const l of listeners) l()
}

export function useChat(tabId: string): ChatState {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb)
      return () => listeners.delete(cb)
    },
    () => chats.get(tabId) ?? EMPTY
  )
}

export function forgetChat(tabId: string): void {
  chats.delete(tabId)
}

// ---------- helpers ----------

const obj = (v: unknown): Record<string, unknown> => (v && typeof v === 'object' ? (v as Record<string, unknown>) : {})
const str = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined)

function resultText(content: unknown): string {
  if (typeof content === 'string') return content
  if (Array.isArray(content))
    return content
      .map((b) => {
        const o = obj(b)
        return o.type === 'text' ? String(o.text ?? '') : o.type === 'image' ? '[image]' : ''
      })
      .filter(Boolean)
      .join('\n')
  return ''
}

/** Last streaming item of a kind for an API message id (to finalize it with the complete text). */
function findStreaming(items: ChatItem[], kind: 'text' | 'thinking', msgId?: string): number {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i]
    if (it.kind === kind && it.streaming && (!msgId || it.msgId === msgId)) return i
  }
  return -1
}

// ---------- event reducer ----------

let currentMsg: Record<string, string | undefined> = {}

export function applyEvent(tabId: string, e: Record<string, unknown>): void {
  const type = str(e.type)
  const parent = str(e.parent_tool_use_id)
  set(tabId, (s) => {
    let items = s.items
    const push = (it: ChatItem) => (items = [...items, it])
    const patch = (i: number, it: ChatItem) => (items = items.map((x, k) => (k === i ? it : x)))
    let next: ChatState = s

    if (type === 'system') {
      const sub = str(e.subtype)
      if (sub === 'init')
        next = {
          ...s,
          model: str(e.model) ?? s.model,
          permissionMode: str(e.permissionMode) ?? s.permissionMode,
          slashCommands: Array.isArray(e.slash_commands) ? (e.slash_commands as string[]) : s.slashCommands
        }
      else if (sub === 'api_retry')
        push({ kind: 'notice', id: nid(), tone: 'warn', text: `Retrying (${str(e.error) ?? 'error'}), attempt ${e.attempt}/${e.max_retries}…` })
      else if (sub === 'compact_boundary') push({ kind: 'notice', id: nid(), tone: 'info', text: 'Conversation compacted to free up context.' })
      else if (sub === 'status' && str(e.permissionMode)) next = { ...s, permissionMode: str(e.permissionMode) }
    } else if (type === 'stream_event' && !parent) {
      const ev = obj(e.event)
      const evType = str(ev.type)
      if (evType === 'message_start') currentMsg[tabId] = str(obj(ev.message).id)
      else if (evType === 'content_block_start') {
        const block = obj(ev.content_block)
        const msgId = currentMsg[tabId]
        if (block.type === 'text') push({ kind: 'text', id: nid(), msgId, text: '', streaming: true })
        else if (block.type === 'thinking') push({ kind: 'thinking', id: nid(), msgId, text: '', streaming: true })
      } else if (evType === 'content_block_delta') {
        const d = obj(ev.delta)
        const kind = d.type === 'text_delta' ? 'text' : d.type === 'thinking_delta' ? 'thinking' : null
        const piece = str(d.text) ?? str(d.thinking) ?? ''
        if (kind && piece) {
          const i = findStreaming(items, kind, currentMsg[tabId])
          const it = items[i]
          if (it && (it.kind === 'text' || it.kind === 'thinking')) patch(i, { ...it, text: it.text + piece })
        }
      }
    } else if (type === 'assistant') {
      const msg = obj(e.message)
      const msgId = str(msg.id)
      for (const raw of Array.isArray(msg.content) ? msg.content : []) {
        const b = obj(raw)
        if (parent) {
          // Subagent work: count its tool calls on the parent tool card.
          if (b.type === 'tool_use') {
            const i = items.findIndex((x) => x.kind === 'tool' && x.id === parent)
            const it = items[i]
            if (it?.kind === 'tool') patch(i, { ...it, steps: it.steps + 1 })
          }
          continue
        }
        if (b.type === 'text' || b.type === 'thinking') {
          const kind = b.type
          const text = str(b.text) ?? str(b.thinking) ?? ''
          const i = findStreaming(items, kind, msgId)
          const it = items[i]
          if (it && (it.kind === 'text' || it.kind === 'thinking')) patch(i, { ...it, text: text || it.text, streaming: false })
          else if (text) push({ kind, id: nid(), msgId, text, streaming: false })
        } else if (b.type === 'tool_use') {
          const id = str(b.id) ?? nid()
          if (!items.some((x) => x.kind === 'tool' && x.id === id))
            push({ kind: 'tool', id, name: str(b.name) ?? 'tool', input: obj(b.input), steps: 0 })
        }
      }
    } else if (type === 'user') {
      if (e.isReplay) return s
      const msg = obj(e.message)
      const content = msg.content
      if (typeof content === 'string') {
        if (e.history && !content.startsWith('<')) push({ kind: 'user', id: str(e.uuid) ?? nid(), text: content, images: [] })
      } else if (Array.isArray(content)) {
        let text = ''
        let imageCount = 0
        for (const raw of content) {
          const b = obj(raw)
          if (b.type === 'tool_result') {
            const i = items.findIndex((x) => x.kind === 'tool' && x.id === b.tool_use_id)
            const it = items[i]
            if (it?.kind === 'tool')
              patch(i, { ...it, result: { text: resultText(b.content), isError: b.is_error === true, structured: e.tool_use_result } })
          } else if (b.type === 'text' && e.history && !parent) text += String(b.text ?? '')
          else if (b.type === 'image' && e.history && !parent) imageCount++
        }
        if ((text && !text.startsWith('<')) || imageCount)
          push({ kind: 'user', id: str(e.uuid) ?? nid(), text, images: [], imageCount })
      }
    } else if (type === 'result') {
      const denials = (Array.isArray(e.permission_denials) ? e.permission_denials : []).map((d) => str(obj(d).tool_name) ?? 'tool')
      push({
        kind: 'result',
        id: nid(),
        durationMs: typeof e.duration_ms === 'number' ? e.duration_ms : undefined,
        costUsd: typeof e.total_cost_usd === 'number' ? e.total_cost_usd : undefined,
        isError: e.is_error === true,
        subtype: str(e.subtype),
        denials
      })
      // Close anything still marked as streaming.
      items = items.map((x) => ((x.kind === 'text' || x.kind === 'thinking') && x.streaming ? { ...x, streaming: false } : x))
      return { ...next, items, busy: false }
    }
    return { ...next, items }
  })
}

export function loadHistory(tabId: string, events: Array<Record<string, unknown>>): void {
  for (const e of events) applyEvent(tabId, e)
  if (events.length) set(tabId, (s) => ({ ...s, items: [...s.items, { kind: 'notice', id: nid(), tone: 'info', text: 'Earlier messages of this session. New messages continue it.' }] }))
}

export function addUserMessage(tabId: string, text: string, images: string[]): void {
  set(tabId, (s) => ({ ...s, busy: true, items: [...s.items, { kind: 'user', id: nid(), text, images }] }))
}

export function addNotice(tabId: string, text: string, tone: 'info' | 'warn' | 'error' = 'info'): void {
  set(tabId, (s) => ({ ...s, items: [...s.items, { kind: 'notice', id: nid(), text, tone }] }))
}

export function setBusy(tabId: string, busy: boolean): void {
  set(tabId, (s) => ({ ...s, busy }))
}

export function setPermissionMode(tabId: string, mode: string | undefined): void {
  set(tabId, (s) => ({ ...s, permissionMode: mode }))
}

function addPermission(r: ChatPermissionRequest): void {
  set(r.tabId, (s) => ({ ...s, items: [...s.items, { kind: 'permission', id: r.requestId, request: r, state: 'pending' }] }))
}

function permissionDone(tabId: string, requestId: string, decision: string): void {
  set(tabId, (s) => ({
    ...s,
    items: s.items.map((x) =>
      x.kind === 'permission' && x.id === requestId
        ? { ...x, state: decision === 'allow' ? 'allow' : decision === 'deny' ? 'deny' : 'expired' }
        : x
    )
  }))
}

// Wire up once.
window.api.onChatEvent(({ tabId, event }) => applyEvent(tabId, event))
window.api.onChatPermission(addPermission)
window.api.onChatPermissionDone(({ tabId, requestId, decision }) => permissionDone(tabId, requestId, decision))
window.api.onChatExit(({ tabId, code, stopped, stderr }) => {
  set(tabId, (s) => ({
    ...s,
    busy: false,
    items: s.items.map((x) => ((x.kind === 'text' || x.kind === 'thinking') && x.streaming ? { ...x, streaming: false } : x))
  }))
  if (stopped) addNotice(tabId, 'Stopped. Your next message continues the conversation.', 'warn')
  else if (code && code !== 0) addNotice(tabId, `Claude Code exited (code ${code}).${stderr ? ` ${stderr}` : ''}`, 'error')
})
