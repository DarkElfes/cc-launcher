import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  BrainIcon,
  CaretDownIcon,
  CaretRightIcon,
  CheckCircleIcon,
  FileTextIcon,
  GlobeIcon,
  ImageIcon,
  ListChecksIcon,
  MagnifyingGlassIcon,
  NotePencilIcon,
  RobotIcon,
  ShieldWarningIcon,
  SpinnerIcon,
  TerminalWindowIcon,
  WarningCircleIcon,
  WrenchIcon,
  XCircleIcon
} from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { ChatPermissionDecision } from '../../../shared/types'
import { useChat, type ChatItem, type ToolResult } from './store'

// ---------- Markdown ----------

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="chat-md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer">
              {children}
            </a>
          )
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  )
})

// ---------- Tool cards ----------

const s = (v: unknown) => (typeof v === 'string' ? v : '')
const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p
const lines = (t: string) => (t ? t.split('\n').length : 0)

function toolMeta(name: string, input: Record<string, unknown>): { icon: typeof WrenchIcon; title: string; detail?: string } {
  switch (name) {
    case 'Read':
      return { icon: FileTextIcon, title: `Read ${baseName(s(input.file_path))}`, detail: s(input.file_path) }
    case 'Write':
      return { icon: NotePencilIcon, title: `Write ${baseName(s(input.file_path))}`, detail: `${lines(s(input.content))} lines` }
    case 'Edit':
      return { icon: NotePencilIcon, title: `Edit ${baseName(s(input.file_path))}`, detail: s(input.file_path) }
    case 'MultiEdit':
      return { icon: NotePencilIcon, title: `Edit ${baseName(s(input.file_path))}`, detail: `${Array.isArray(input.edits) ? input.edits.length : 0} edits` }
    case 'Bash':
    case 'PowerShell':
      return { icon: TerminalWindowIcon, title: s(input.description) || 'Run a command', detail: s(input.command) }
    case 'Grep':
      return { icon: MagnifyingGlassIcon, title: `Search for "${s(input.pattern)}"`, detail: s(input.path) || s(input.glob) }
    case 'Glob':
      return { icon: MagnifyingGlassIcon, title: `Find files ${s(input.pattern)}`, detail: s(input.path) }
    case 'WebFetch':
      return { icon: GlobeIcon, title: 'Fetch a web page', detail: s(input.url) }
    case 'WebSearch':
      return { icon: GlobeIcon, title: `Web search "${s(input.query)}"` }
    case 'Agent':
    case 'Task':
      return { icon: RobotIcon, title: s(input.description) || `Subagent ${s(input.subagent_type)}`, detail: s(input.subagent_type) }
    case 'TodoWrite':
    case 'TaskCreate':
    case 'TaskUpdate':
      return { icon: ListChecksIcon, title: name === 'TaskCreate' ? `Task: ${s(input.subject)}` : 'Update tasks' }
    default:
      return { icon: WrenchIcon, title: name }
  }
}

function Diff({ oldText, newText }: { oldText: string; newText: string }) {
  return (
    <pre className="max-h-80 overflow-auto bg-background p-2 text-[11px] leading-relaxed">
      {oldText.split('\n').map((l, i) => (
        <div key={`o${i}`} className="bg-red-500/10 text-red-300">
          − {l}
        </div>
      ))}
      {newText.split('\n').map((l, i) => (
        <div key={`n${i}`} className="bg-emerald-500/10 text-emerald-300">
          + {l}
        </div>
      ))}
    </pre>
  )
}

function ToolBody({ name, input, result }: { name: string; input: Record<string, unknown>; result?: ToolResult }) {
  return (
    <div className="flex flex-col gap-2 border-t p-2">
      {name === 'Edit' && <Diff oldText={s(input.old_string)} newText={s(input.new_string)} />}
      {name === 'MultiEdit' &&
        Array.isArray(input.edits) &&
        (input.edits as Array<Record<string, unknown>>).map((ed, i) => <Diff key={i} oldText={s(ed.old_string)} newText={s(ed.new_string)} />)}
      {name === 'Write' && <pre className="max-h-80 overflow-auto bg-background p-2 text-[11px]">{s(input.content).slice(0, 8000)}</pre>}
      {(name === 'Bash' || name === 'PowerShell') && <pre className="overflow-auto bg-background p-2 text-[11px] text-sky-200">$ {s(input.command)}</pre>}
      {!['Edit', 'MultiEdit', 'Write', 'Bash', 'PowerShell'].includes(name) && (
        <pre className="max-h-48 overflow-auto bg-background p-2 text-[11px] text-muted-foreground">{JSON.stringify(input, null, 2)}</pre>
      )}
      {result && (
        <pre className={cn('max-h-80 overflow-auto bg-background p-2 text-[11px]', result.isError && 'text-red-300')}>
          {result.text.slice(0, 12000) || '(no output)'}
          {result.text.length > 12000 && '\n…'}
        </pre>
      )}
    </div>
  )
}

function ToolCard({ item }: { item: Extract<ChatItem, { kind: 'tool' }> }) {
  const [open, setOpen] = useState(false)
  const { icon: Icon, title, detail } = toolMeta(item.name, item.input)
  const done = !!item.result
  return (
    <div className="border bg-card/40">
      <button className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs" onClick={() => setOpen(!open)}>
        {open ? <CaretDownIcon className="size-3 shrink-0" /> : <CaretRightIcon className="size-3 shrink-0" />}
        <Icon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="shrink-0 font-medium">{title}</span>
        {detail && <span className="min-w-0 truncate text-muted-foreground">{detail}</span>}
        {item.steps > 0 && <span className="shrink-0 text-[10px] text-muted-foreground">{item.steps} steps</span>}
        <span className="ml-auto shrink-0">
          {!done ? (
            <SpinnerIcon className="size-3.5 animate-spin text-sky-400" />
          ) : item.result!.isError ? (
            <XCircleIcon weight="fill" className="size-3.5 text-red-400" />
          ) : (
            <CheckCircleIcon weight="fill" className="size-3.5 text-emerald-500" />
          )}
        </span>
      </button>
      {open && <ToolBody name={item.name} input={item.input} result={item.result} />}
    </div>
  )
}

// ---------- Permission cards ----------

function decide(tabId: string, requestId: string, d: ChatPermissionDecision) {
  window.api.chatPermissionReply(tabId, requestId, d)
}

function AskQuestionCard({ tabId, item }: { tabId: string; item: Extract<ChatItem, { kind: 'permission' }> }) {
  const questions = (Array.isArray(item.request.input.questions) ? item.request.input.questions : []) as Array<{
    question: string
    header?: string
    multiSelect?: boolean
    options?: Array<{ label: string; description?: string }>
  }>
  const [answers, setAnswers] = useState<Record<string, string[]>>({})
  const pending = item.state === 'pending'
  const toggle = (q: string, label: string, multi?: boolean) =>
    setAnswers((a) => {
      const cur = a[q] ?? []
      return { ...a, [q]: multi ? (cur.includes(label) ? cur.filter((x) => x !== label) : [...cur, label]) : [label] }
    })
  const complete = questions.every((q) => (answers[q.question] ?? []).length > 0)
  return (
    <div className="flex flex-col gap-3 border border-sky-500/40 bg-sky-500/5 p-3">
      {questions.map((q) => (
        <div key={q.question} className="flex flex-col gap-1.5">
          <span className="text-xs font-medium">{q.question}</span>
          <div className="flex flex-col gap-1">
            {(q.options ?? []).map((o) => {
              const on = (answers[q.question] ?? []).includes(o.label)
              return (
                <button
                  key={o.label}
                  disabled={!pending}
                  onClick={() => toggle(q.question, o.label, q.multiSelect)}
                  className={cn('flex flex-col items-start border px-2 py-1 text-left text-xs', on ? 'border-sky-400 bg-sky-500/10' : 'hover:border-ring')}
                >
                  <span>{o.label}</span>
                  {o.description && <span className="text-[11px] text-muted-foreground">{o.description}</span>}
                </button>
              )
            })}
          </div>
        </div>
      ))}
      {pending ? (
        <div className="flex gap-2">
          <Button
            size="sm"
            disabled={!complete}
            onClick={() =>
              decide(tabId, item.id, {
                behavior: 'allow',
                updatedInput: {
                  ...item.request.input,
                  answers: Object.fromEntries(
                    questions.map((q) => [q.question, q.multiSelect ? answers[q.question] : answers[q.question]?.[0]])
                  )
                }
              })
            }
          >
            Answer
          </Button>
          <Button size="sm" variant="outline" onClick={() => decide(tabId, item.id, { behavior: 'deny', message: 'The user skipped the question.' })}>
            Skip
          </Button>
        </div>
      ) : (
        <span className="text-[11px] text-muted-foreground">{item.state === 'allow' ? 'Answered.' : 'Skipped.'}</span>
      )}
    </div>
  )
}

function PermissionCard({ tabId, item }: { tabId: string; item: Extract<ChatItem, { kind: 'permission' }> }) {
  const [reason, setReason] = useState('')
  const { toolName, input, suggestions } = item.request
  const pending = item.state === 'pending'
  if (toolName === 'AskUserQuestion') return <AskQuestionCard tabId={tabId} item={item} />
  const isPlan = toolName === 'ExitPlanMode'
  const { icon: Icon, title, detail } = toolMeta(toolName, input)
  return (
    <div className={cn('flex flex-col gap-2 border p-3', pending ? 'border-amber-500/50 bg-amber-500/5' : 'opacity-70')}>
      <div className="flex items-center gap-2 text-xs">
        <ShieldWarningIcon className="size-4 shrink-0 text-amber-400" />
        <span className="font-medium">{isPlan ? 'Claude has a plan. Approve it?' : 'Claude wants to:'}</span>
        {!isPlan && (
          <>
            <Icon className="size-3.5 shrink-0" />
            <span className="min-w-0 truncate">{title}</span>
          </>
        )}
      </div>
      {isPlan ? (
        <div className="max-h-96 overflow-y-auto border bg-background p-2">
          <Markdown text={s(input.plan)} />
        </div>
      ) : (
        <>
          {detail && <pre className="max-h-40 overflow-auto bg-background p-2 text-[11px]">{detail}</pre>}
          {(toolName === 'Edit' || toolName === 'MultiEdit' || toolName === 'Write') && <ToolBody name={toolName} input={input} />}
        </>
      )}
      {pending ? (
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" onClick={() => decide(tabId, item.id, { behavior: 'allow', updatedInput: input })}>
            {isPlan ? 'Approve plan' : 'Allow'}
          </Button>
          {!isPlan && suggestions.length > 0 && (
            <Button
              size="sm"
              variant="outline"
              title="Allow, and don't ask again for this in this session/project (Claude Code's own suggestion)"
              onClick={() => decide(tabId, item.id, { behavior: 'allow', updatedInput: input, updatedPermissions: suggestions })}
            >
              Always allow
            </Button>
          )}
          <Input
            className="h-7 min-w-40 flex-1 text-xs"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder={isPlan ? 'What to change (optional)' : 'Why not / what instead (optional)'}
          />
          <Button
            size="sm"
            variant="destructive"
            onClick={() =>
              decide(tabId, item.id, {
                behavior: 'deny',
                message: reason.trim() || (isPlan ? 'Keep planning; the user did not approve this plan yet.' : 'The user denied this.')
              })
            }
          >
            {isPlan ? 'Keep planning' : 'Deny'}
          </Button>
        </div>
      ) : (
        <span className="text-[11px] text-muted-foreground">
          {item.state === 'allow' ? 'Allowed.' : item.state === 'deny' ? 'Denied.' : 'Timed out (denied).'}
        </span>
      )}
    </div>
  )
}

// ---------- Messages ----------

function UserBubble({ item }: { item: Extract<ChatItem, { kind: 'user' }> }) {
  const [thumbs, setThumbs] = useState<Record<string, string | null>>({})
  useEffect(() => {
    for (const p of item.images) void window.api.imageThumbnail(p).then((u) => setThumbs((t) => ({ ...t, [p]: u })))
  }, [item.images])
  return (
    <div className="flex justify-end">
      <div className="flex max-w-[80%] flex-col gap-2 border bg-secondary px-3 py-2">
        {item.images.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {item.images.map((p) => (
              <button key={p} className="h-20 w-28 overflow-hidden border" onClick={() => void window.api.openImage(p)}>
                {thumbs[p] ? <img src={thumbs[p]!} alt="" className="size-full object-cover" /> : <ImageIcon className="m-auto size-4" />}
              </button>
            ))}
          </div>
        )}
        {!!item.imageCount && <span className="text-[11px] text-muted-foreground">{item.imageCount} image(s)</span>}
        {item.text && <div className="text-sm whitespace-pre-wrap">{item.text}</div>}
      </div>
    </div>
  )
}

function Thinking({ item }: { item: Extract<ChatItem, { kind: 'thinking' }> }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="text-xs text-muted-foreground">
      <button className="flex items-center gap-1.5" onClick={() => setOpen(!open)}>
        <BrainIcon className={cn('size-3.5', item.streaming && 'animate-pulse')} />
        {item.streaming ? 'Thinking…' : 'Thought'}
        {open ? <CaretDownIcon className="size-3" /> : <CaretRightIcon className="size-3" />}
      </button>
      {open && <div className="mt-1 border-l pl-3 whitespace-pre-wrap">{item.text}</div>}
    </div>
  )
}

function Item({ tabId, item }: { tabId: string; item: ChatItem }) {
  switch (item.kind) {
    case 'user':
      return <UserBubble item={item} />
    case 'text':
      return (
        <div className={cn(item.streaming && 'after:ml-0.5 after:inline-block after:h-3 after:w-1.5 after:animate-pulse after:bg-foreground/70')}>
          <Markdown text={item.text} />
        </div>
      )
    case 'thinking':
      return <Thinking item={item} />
    case 'tool':
      return <ToolCard item={item} />
    case 'permission':
      return <PermissionCard tabId={tabId} item={item} />
    case 'notice':
      return (
        <div
          className={cn(
            'flex items-center gap-2 text-[11px]',
            item.tone === 'error' ? 'text-red-400' : item.tone === 'warn' ? 'text-amber-300' : 'text-muted-foreground'
          )}
        >
          <WarningCircleIcon className="size-3.5 shrink-0" />
          {item.text}
        </div>
      )
    case 'result':
      return (
        <div className="flex flex-col gap-1 text-[10px] text-muted-foreground">
          <span>
            {item.isError ? `Stopped: ${item.subtype}` : 'Done'}
            {item.durationMs !== undefined && ` · ${(item.durationMs / 1000).toFixed(1)} s`}
            {item.costUsd !== undefined && ` · ≈ $${item.costUsd.toFixed(2)} API-price`}
          </span>
          {item.denials.length > 0 && (
            <span className="text-amber-300">
              Not allowed: {[...new Set(item.denials)].join(', ')}. Change the permission mode above, or allow it when asked.
            </span>
          )}
        </div>
      )
  }
}

// ---------- Header + view ----------

/** "claude-opus-5-5" → "Opus 5.5" */
const prettyModel = (id: string) => {
  const m = id.match(/claude-([a-z]+)-(\d+)-(\d+)/)
  return m ? `${m[1][0].toUpperCase()}${m[1].slice(1)} ${m[2]}.${m[3]}` : id
}

const MODES: Array<[string, string]> = [
  ['', 'Account default'],
  ['default', 'Ask before acting'],
  ['acceptEdits', 'Auto-accept edits'],
  ['plan', 'Plan first'],
  ['auto', 'Auto mode']
]

export function ChatView({ tabId, active }: { tabId: string; active: boolean }) {
  const chat = useChat(tabId)
  const scroller = useRef<HTMLDivElement>(null)
  const stick = useRef(true)

  // Follow new output unless you scrolled up.
  useLayoutEffect(() => {
    const el = scroller.current
    if (el && stick.current) el.scrollTop = el.scrollHeight
  }, [chat.items, active])

  return (
    <div className="absolute inset-0 flex flex-col" style={{ display: active ? 'flex' : 'none' }}>
      <div className="flex shrink-0 items-center gap-3 border-b px-4 py-1.5 text-[11px] text-muted-foreground">
        <span>{chat.model ? prettyModel(chat.model) : 'Claude Code'}</span>
        <label className="flex items-center gap-1.5">
          Permissions
          <select
            className="border bg-background px-1 py-0.5 text-[11px] text-foreground"
            value={chat.permissionMode && MODES.some(([m]) => m === chat.permissionMode) ? chat.permissionMode : ''}
            onChange={(e) => {
              const mode = e.target.value || undefined
              window.api.chatSetPermissionMode(tabId, mode)
            }}
            title="Applies from your next message"
          >
            {MODES.map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </label>
        {chat.busy && (
          <span className="ml-auto flex items-center gap-1.5 text-sky-300">
            <SpinnerIcon className="size-3 animate-spin" /> Working…
          </span>
        )}
      </div>
      <div
        ref={scroller}
        className="min-h-0 flex-1 overflow-y-auto"
        onScroll={(e) => {
          const el = e.currentTarget
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80
        }}
      >
        <div className="mx-auto flex max-w-3xl flex-col gap-3 px-4 py-4">
          {chat.items.length === 0 && (
            <p className="py-12 text-center text-xs text-muted-foreground">Ask Claude anything about this project.</p>
          )}
          {chat.items.map((it) => (
            <Item key={it.id} tabId={tabId} item={it} />
          ))}
        </div>
      </div>
    </div>
  )
}
