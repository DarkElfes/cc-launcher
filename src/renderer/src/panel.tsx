import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  ArrowClockwiseIcon,
  ArrowsLeftRightIcon,
  CaretDownIcon,
  CaretRightIcon,
  CheckCircleIcon,
  CircleDashedIcon,
  CircleIcon,
  ClockCounterClockwiseIcon,
  GitBranchIcon,
  PlayIcon,
  PlugsConnectedIcon,
  RobotIcon,
  SpinnerIcon,
  TerminalIcon
} from '@phosphor-icons/react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { cn } from '@/lib/utils'
import type { GitChange, GitInfo, HistorySession, McpServer, ProfileInfo, ProjectInfo, SessionInfo } from '../../shared/types'
import { ProfileTag, showError, timeAgo } from './ui'
import { ActivityBadge } from './activity'

/** Collapsible panel section with a title row. */
function Section(props: { title: string; right?: ReactNode; children: ReactNode; defaultOpen?: boolean; count?: number }) {
  const [open, setOpen] = useState(props.defaultOpen ?? true)
  return (
    <section className="border-b">
      <div className="flex items-center gap-1 px-3 py-2">
        <button className="flex flex-1 items-center gap-1.5 text-left text-xs font-semibold" onClick={() => setOpen(!open)}>
          {open ? <CaretDownIcon className="size-3" /> : <CaretRightIcon className="size-3" />}
          {props.title}
          {props.count ? <span className="font-normal text-muted-foreground">{props.count}</span> : null}
        </button>
        {props.right}
      </div>
      {open && <div className="px-3 pb-3">{props.children}</div>}
    </section>
  )
}

const Row = ({ label, children }: { label: string; children: ReactNode }) => (
  <div className="flex items-baseline justify-between gap-3 py-0.5 text-xs">
    <span className="shrink-0 text-muted-foreground">{label}</span>
    <span className="min-w-0 truncate text-right">{children}</span>
  </div>
)

const Empty = ({ children }: { children: ReactNode }) => <p className="text-[11px] text-muted-foreground">{children}</p>

const fmtTokens = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 100_000 ? 0 : 1)}k` : String(n))
const fmtDuration = (ms: number) => {
  const m = Math.round(ms / 60000)
  return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${m % 60} min`
}

function relPath(path: string, root: string): string {
  const norm = (p: string) => p.replace(/\//g, '\\').toLowerCase()
  return norm(path).startsWith(norm(root) + '\\') ? path.slice(root.length + 1) : path
}

const openFile = async (path: string) => {
  const r = await window.api.openInEditor(path)
  if (!r.ok) showError(r.error)
}

// ---------- Sections ----------

function SessionSection({ s, profile }: { s?: SessionInfo; profile?: ProfileInfo }) {
  if (!s) return <Empty>Open a Claude Code tab to see its session here.</Empty>
  const ctx = s.context
  const pct = ctx ? Math.round(ctx.usedPercent ?? (ctx.tokens / ctx.size) * 100) : null
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        {profile ? <ProfileTag name={profile.name} /> : <span />}
        <ActivityBadge activity={s.activity} />
      </div>
      {profile?.account?.email && <span className="truncate text-[11px] text-muted-foreground">{profile.account.email}</span>}
      {s.sessionName && <Row label="Session">{s.sessionName}</Row>}
      <Row label="Model">{s.model ?? '—'}</Row>
      <div className="flex flex-col gap-1">
        <div className="flex justify-between text-xs">
          <span className="text-muted-foreground">Context</span>
          <span>{ctx ? `${pct}% · ${fmtTokens(ctx.tokens)} / ${fmtTokens(ctx.size)}` : '—'}</span>
        </div>
        <Progress
          value={pct ?? 0}
          className={cn('h-1.5', (pct ?? 0) >= 85 ? '[&>div]:bg-red-500' : (pct ?? 0) >= 60 ? '[&>div]:bg-amber-400' : '[&>div]:bg-sky-500')}
        />
      </div>
      <Row label="API-price equivalent">
        <span title="Claude Code's own estimate at API list prices. On a Pro/Max plan you don't pay this; it shows what the same work would cost when buying tokens.">
          {s.costUsd !== undefined ? `$${s.costUsd.toFixed(2)}` : '—'}
        </span>
      </Row>
      {(s.linesAdded !== undefined || s.linesRemoved !== undefined) && (
        <Row label="Lines">
          <span className="text-emerald-400">+{s.linesAdded ?? 0}</span> <span className="text-red-400">−{s.linesRemoved ?? 0}</span>
        </Row>
      )}
      {s.durationMs !== undefined && <Row label="Time">{fmtDuration(s.durationMs)}</Row>}
      {s.error && <p className="text-[11px] text-red-400">{s.error}</p>}
      {!s.error && s.message && (s.activity === 'needs-you' || s.activity === 'done') && (
        <p className="line-clamp-3 text-[11px] text-muted-foreground">{s.message}</p>
      )}
    </div>
  )
}

function PlanSection({ s }: { s?: SessionInfo }) {
  if (!s?.plan) return <Empty>No plan in this session. Claude shows one in plan mode (Shift+Tab).</Empty>
  return (
    <div className="max-h-64 overflow-y-auto border bg-muted/30 p-2 text-[11px] leading-relaxed whitespace-pre-wrap">{s.plan.text}</div>
  )
}

function TasksSection({ s }: { s?: SessionInfo }) {
  const tasks = s?.tasks ?? []
  if (!tasks.length) return <Empty>No task list yet. Claude creates one for multi-step work.</Empty>
  return (
    <ul className="flex flex-col gap-1">
      {tasks.map((t) => (
        <li key={t.id} className="flex items-start gap-2 text-xs">
          {t.status === 'completed' ? (
            <CheckCircleIcon weight="fill" className="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
          ) : t.status === 'in_progress' ? (
            <SpinnerIcon className="mt-0.5 size-3.5 shrink-0 animate-spin text-sky-400" />
          ) : (
            <CircleIcon className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
          )}
          <span className={cn(t.status === 'completed' && 'text-muted-foreground line-through')}>{t.subject}</span>
        </li>
      ))}
    </ul>
  )
}

function BackgroundSection({ s }: { s?: SessionInfo }) {
  const items = [...(s?.agents ?? [])].sort((a, b) => Number(b.running) - Number(a.running) || b.startedAt - a.startedAt)
  if (!items.length) return <Empty>No subagents or background commands.</Empty>
  return (
    <ul className="flex flex-col gap-1.5">
      {items.slice(0, 20).map((a) => (
        <li key={a.id} className="flex items-center gap-2 text-xs">
          {a.kind === 'bash' ? <TerminalIcon className="size-3.5 shrink-0" /> : <RobotIcon className="size-3.5 shrink-0" />}
          <span className="min-w-0 flex-1 truncate" title={a.label}>
            {a.kind === 'bash' ? (a.label ?? 'background command') : a.kind}
          </span>
          {a.running ? (
            <Badge variant="outline" className="border-sky-500/40 text-sky-300">
              running
            </Badge>
          ) : (
            <span className="text-[10px] text-muted-foreground">{a.endedAt ? timeAgo(a.endedAt) : 'done'}</span>
          )}
        </li>
      ))}
    </ul>
  )
}

function OutputsSection({ s, root }: { s?: SessionInfo; root: string }) {
  const outputs = s?.outputs ?? []
  if (!outputs.length) return <Empty>Files Claude writes or edits in this session show up here.</Empty>
  return (
    <ul className="flex flex-col">
      {outputs.slice(0, 50).map((o) => (
        <li key={o.path}>
          <button
            className="flex w-full items-center gap-2 py-0.5 text-left text-xs hover:text-foreground"
            title={`${o.path}\nClick to open in the editor`}
            onClick={() => void openFile(o.path)}
          >
            <span className={cn('w-3 shrink-0 text-[10px] font-bold', o.op === 'written' ? 'text-emerald-400' : 'text-amber-400')}>
              {o.op === 'written' ? 'W' : 'E'}
            </span>
            <span className="min-w-0 flex-1 truncate">{relPath(o.path, root)}</span>
            {o.count > 1 && <span className="text-[10px] text-muted-foreground">×{o.count}</span>}
          </button>
        </li>
      ))}
    </ul>
  )
}

const GIT_MARK: Record<GitChange['status'], [string, string]> = {
  modified: ['M', 'text-amber-400'],
  added: ['A', 'text-emerald-400'],
  untracked: ['U', 'text-emerald-400'],
  deleted: ['D', 'text-red-400'],
  renamed: ['R', 'text-sky-400'],
  conflict: ['!', 'text-red-500']
}

function ChangesSection({ project, tick }: { project: ProjectInfo; tick: number }) {
  const [info, setInfo] = useState<GitInfo | null>(null)
  const load = useCallback(async () => {
    const r = await window.api.gitInfo(project.id)
    if (r.ok) setInfo(r.value)
  }, [project.id])
  useEffect(() => {
    void load()
    const t = setInterval(() => document.visibilityState === 'visible' && void load(), 5000)
    return () => clearInterval(t)
  }, [load, tick])
  if (!info) return <Empty>Loading…</Empty>
  if (!info.isRepo) return <Empty>Not a git repository.</Empty>
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2 text-xs">
        <GitBranchIcon className="size-3.5" />
        <span className="font-medium">{info.branch ?? '—'}</span>
        {info.ahead ? <span className="text-[10px] text-muted-foreground">↑{info.ahead}</span> : null}
        {info.behind ? <span className="text-[10px] text-muted-foreground">↓{info.behind}</span> : null}
        <span className="ml-auto">
          <span className="text-emerald-400">+{info.added}</span> <span className="text-red-400">−{info.removed}</span>
        </span>
      </div>
      {info.files.length === 0 ? (
        <Empty>No uncommitted changes.</Empty>
      ) : (
        <ul className="flex flex-col">
          {info.files.slice(0, 60).map((f) => {
            const [mark, color] = GIT_MARK[f.status]
            return (
              <li key={f.path}>
                <button
                  className="flex w-full items-center gap-2 py-0.5 text-left text-xs hover:text-foreground"
                  title={`${f.path} (${f.status}${f.staged ? ', staged' : ''})\nClick to open in the editor`}
                  onClick={() => void openFile(`${project.path}\\${f.path.replace(/\//g, '\\')}`)}
                >
                  <span className={cn('w-3 shrink-0 text-[10px] font-bold', color)}>{mark}</span>
                  <span className="min-w-0 flex-1 truncate">{f.path}</span>
                  {(f.added || f.removed) && (
                    <span className="text-[10px]">
                      <span className="text-emerald-400">+{f.added ?? 0}</span> <span className="text-red-400">−{f.removed ?? 0}</span>
                    </span>
                  )}
                </button>
              </li>
            )
          })}
        </ul>
      )}
      {info.lastCommit && (
        <p className="truncate text-[10px] text-muted-foreground" title={info.lastCommit.subject}>
          Last commit {info.lastCommit.hash} · {timeAgo(info.lastCommit.at)} · {info.lastCommit.subject}
        </p>
      )}
    </div>
  )
}

const MCP_STYLE: Record<McpServer['status'], string> = {
  connected: 'text-emerald-400',
  'needs-auth': 'text-amber-400',
  failed: 'text-red-400',
  pending: 'text-amber-400',
  disabled: 'text-muted-foreground',
  unknown: 'text-muted-foreground'
}

function ConnectorsSection({ project, profile }: { project: ProjectInfo; profile?: string }) {
  const [state, setState] = useState<{ loading: boolean; servers?: McpServer[]; error?: string; for?: string }>({ loading: false })
  const load = useCallback(async () => {
    if (!profile) return
    setState({ loading: true, for: profile })
    const r = await window.api.listMcp(profile, project.id)
    setState(r.ok ? { loading: false, servers: r.value, for: profile } : { loading: false, error: r.error, for: profile })
  }, [profile, project.id])
  useEffect(() => {
    void load()
  }, [load])
  if (!profile) return <Empty>Open a Claude Code tab to see its connectors.</Empty>
  if (state.loading) return <Empty>Checking connectors (Claude Code tests each one)…</Empty>
  if (state.error) return <p className="text-[11px] text-red-400">{state.error}</p>
  if (!state.servers?.length) return <Empty>No MCP servers or connectors for this account here.</Empty>
  return (
    <ul className="flex flex-col gap-1">
      {state.servers.map((m) => (
        <li key={m.name} className="flex items-center gap-2 text-xs" title={m.detail}>
          <CircleIcon weight="fill" className={cn('size-2 shrink-0', MCP_STYLE[m.status])} />
          <span className="min-w-0 flex-1 truncate">{m.name}</span>
          <span className={cn('text-[10px]', MCP_STYLE[m.status])}>{m.status.replace('-', ' ')}</span>
        </li>
      ))}
    </ul>
  )
}

function HistorySection(props: { project: ProjectInfo; tick: number; openIds: Set<string>; onResume: (h: HistorySession) => void }) {
  const [items, setItems] = useState<HistorySession[] | null>(null)
  const load = useCallback(async () => {
    const r = await window.api.listHistory(props.project.id)
    if (r.ok) setItems(r.value)
    else showError(r.error)
  }, [props.project.id])
  useEffect(() => {
    void load()
  }, [load, props.tick])
  if (!items) return <Empty>Loading…</Empty>
  if (!items.length) return <Empty>No past sessions for this project yet.</Empty>
  return (
    <ul className="flex flex-col gap-1">
      {items.map((h) => (
        <li key={`${h.profile}/${h.id}`} className="group flex items-center gap-2 text-xs">
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="truncate" title={h.title || h.id}>
              {h.title || <span className="text-muted-foreground">Untitled session</span>}
            </span>
            <span className="flex items-center gap-2 text-[10px] text-muted-foreground">
              <ProfileTag name={h.profile} /> {timeAgo(h.modifiedAt)}
            </span>
          </div>
          {props.openIds.has(h.id) ? (
            <span className="text-[10px] text-muted-foreground">open</span>
          ) : (
            <Button variant="outline" size="icon-xs" title={`Resume with ${h.profile}`} onClick={() => props.onResume(h)}>
              <PlayIcon />
            </Button>
          )}
        </li>
      ))}
    </ul>
  )
}

// ---------- Panel ----------

export function SessionPanel(props: {
  project: ProjectInfo
  session?: SessionInfo
  profile?: ProfileInfo
  /** Changes when something happened that may affect git/history (e.g. Claude finished). */
  tick: number
  openSessionIds: Set<string>
  onResume: (h: HistorySession) => void
  /** Opens the "hand off to another account" dialog; undefined when not possible. */
  onHandoff?: () => void
}) {
  const { project, session: s } = props
  const [refreshKey, setRefreshKey] = useState(0)
  const tick = props.tick + refreshKey
  const running = (s?.agents ?? []).filter((a) => a.running).length
  const done = (s?.tasks ?? []).filter((t) => t.status === 'completed').length
  return (
    <aside className="flex w-80 shrink-0 flex-col overflow-y-auto border-l bg-sidebar/40">
      <div className="flex items-center justify-between gap-2 border-b px-3 py-2">
        <span className="text-xs font-semibold">Session</span>
        <div className="flex items-center gap-1">
          {props.onHandoff && (
            <Button variant="outline" size="xs" title="Continue this work with another account" onClick={props.onHandoff}>
              <ArrowsLeftRightIcon data-icon="inline-start" />
              Hand off
            </Button>
          )}
          <Button variant="ghost" size="icon-xs" title="Refresh git and history" onClick={() => setRefreshKey((k) => k + 1)}>
            <ArrowClockwiseIcon />
          </Button>
        </div>
      </div>
      <div className="border-b px-3 py-3">
        <SessionSection s={s} profile={props.profile} />
      </div>
      {s?.plan && (
        <Section title="Plan">
          <PlanSection s={s} />
        </Section>
      )}
      <Section title="Tasks" count={s?.tasks.length ? undefined : 0} right={s?.tasks.length ? <span className="text-[10px] text-muted-foreground">{done}/{s.tasks.length}</span> : null}>
        <TasksSection s={s} />
      </Section>
      <Section
        title="Background"
        right={running ? <span className="flex items-center gap-1 text-[10px] text-sky-300"><CircleDashedIcon className="size-3 animate-spin" />{running}</span> : null}
      >
        <BackgroundSection s={s} />
      </Section>
      <Section title="Outputs" count={s?.outputs.length}>
        <OutputsSection s={s} root={project.path} />
      </Section>
      <Section title="Changes">
        <ChangesSection project={project} tick={tick} />
      </Section>
      <Section
        title="Connectors"
        defaultOpen={false}
        right={<PlugsConnectedIcon className="size-3.5 text-muted-foreground" />}
      >
        <ConnectorsSection project={project} profile={props.profile?.name} />
      </Section>
      <Section title="History" right={<ClockCounterClockwiseIcon className="size-3.5 text-muted-foreground" />}>
        <HistorySection project={project} tick={tick} openIds={props.openSessionIds} onResume={props.onResume} />
      </Section>
    </aside>
  )
}
