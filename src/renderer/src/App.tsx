import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  FolderSimpleIcon,
  GearSixIcon,
  PlusIcon,
  SidebarSimpleIcon,
  SquaresFourIcon,
  UsersThreeIcon,
  WarningIcon,
  XIcon
} from '@phosphor-icons/react'
import { toast } from 'sonner'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider
} from '@/components/ui/sidebar'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { ConfirmProvider, useConfirm } from '@/components/confirm'
import type { AppState, HistorySession, ProjectInfo, SessionInfo, SpawnRequest } from '../../shared/types'
import iconUrl from '../../../build/icon.svg'
import { SignalDot, signalOf, strongest } from './activity'
import { CONTINUE_PROMPT, HANDOFF_PROMPT, HandoffDialog, type HandoffMode } from './handoff'
import { Composer } from './composer'
import { SessionPanel } from './panel'
import { forget } from './termBus'
import { TerminalPane } from './TerminalPane'
import { AccountsView, Dashboard, ProjectView, SettingsView } from './views'
import { ProfileTag, showError } from './ui'

export type View = { kind: 'dashboard' } | { kind: 'project'; id: string } | { kind: 'accounts' } | { kind: 'settings' }

export interface Tab {
  id: string
  kind: 'project' | 'login'
  projectId?: string
  profile: string
  exitCode?: number
  /** Epoch ms; sessions started before voice was turned on don't have it. */
  startedAt: number
}

/** Key for the group of tabs shown in a view: a project id, or "accounts" for sign-in tabs. */
const groupOf = (t: Tab) => (t.kind === 'login' ? 'accounts' : t.projectId!)

const PANEL_KEY = 'ccl.panelOpen'
const readPanelPref = () => {
  try {
    return localStorage.getItem(PANEL_KEY) !== '0'
  } catch {
    return true
  }
}

/** Draggable top bar of the main area. Windows draws min/max/close over its right end. */
function TitleBar(props: { view: View; project?: ProjectInfo; panelOpen?: boolean; onTogglePanel?: () => void }) {
  const { view, project } = props
  const crumbs =
    view.kind === 'project'
      ? ['Projects', project?.name ?? '']
      : [{ dashboard: 'Dashboard', accounts: 'Accounts', settings: 'Settings' }[view.kind]]
  return (
    <header className="app-drag titlebar-safe flex h-10 shrink-0 items-center gap-2 border-b pl-4 text-xs">
      {crumbs.map((c, i) => (
        <span key={i} className={cn('truncate', i < crumbs.length - 1 ? 'text-muted-foreground' : 'font-medium')}>
          {i > 0 && <span className="mr-2 text-muted-foreground">/</span>}
          {c}
        </span>
      ))}
      {props.onTogglePanel && (
        <Button
          variant="ghost"
          size="icon-xs"
          className={cn('ml-auto', props.panelOpen && 'text-foreground')}
          title={props.panelOpen ? 'Hide session panel' : 'Show session panel'}
          onClick={props.onTogglePanel}
        >
          <SidebarSimpleIcon className="-scale-x-100" />
        </Button>
      )}
    </header>
  )
}

export function App() {
  return (
    <TooltipProvider>
      <ConfirmProvider>
        <Shell />
        <Toaster position="bottom-right" />
      </ConfirmProvider>
    </TooltipProvider>
  )
}

function Shell() {
  const confirm = useConfirm()
  const [state, setState] = useState<AppState | null>(null)
  const [view, setView] = useState<View>({ kind: 'dashboard' })
  const [tabs, setTabs] = useState<Tab[]>([])
  const [activeTab, setActiveTab] = useState<Record<string, string>>({})
  const [sessions, setSessions] = useState<Record<string, SessionInfo>>({})
  /** Tabs whose finished/failed result you haven't looked at yet. */
  const [unseen, setUnseen] = useState<Record<string, true>>({})
  const [panelOpen, setPanelOpen] = useState(readPanelPref)
  /** Bumped when Claude finishes in some tab, so git changes / history reload. */
  const [tick, setTick] = useState(0)

  // Latest values for event handlers registered once.
  const live = useRef({ tabs, view, activeTab, state, sessions })
  live.current = { tabs, view, activeTab, state, sessions }

  const refresh = useCallback(async () => setState(await window.api.getState()), [])

  const isTabVisible = (tabId: string) => {
    const { tabs, view, activeTab } = live.current
    const tab = tabs.find((t) => t.id === tabId)
    if (!tab || !document.hasFocus()) return false
    const group = view.kind === 'project' ? view.id : view.kind === 'accounts' ? 'accounts' : null
    return group === groupOf(tab) && activeTab[group] === tabId
  }

  /** Marks a tab as unseen and notifies, unless you're looking at it right now. */
  const attention = (info: SessionInfo) => {
    if (isTabVisible(info.tabId)) return
    const { tabs, state } = live.current
    const tab = tabs.find((t) => t.id === info.tabId)
    if (!tab) return
    setUnseen((u) => ({ ...u, [info.tabId]: true }))
    const where = state?.projects.find((p) => p.id === tab.projectId)?.name ?? 'sign-in'
    const title =
      info.activity === 'needs-you'
        ? `Claude needs you · ${where}`
        : info.activity === 'error'
          ? `Claude stopped with an error · ${where}`
          : `Claude finished · ${where}`
    const body =
      info.activity === 'error'
        ? (info.error ?? 'The request failed.')
        : (info.message ?? (info.activity === 'needs-you' ? 'Waiting for your permission or answer.' : 'Done.'))
    window.api.notify({ title: `${title} (${tab.profile})`, body, tabId: info.tabId })
  }

  useEffect(() => {
    void refresh()
    void window.api.getSessions().then(setSessions)
    const onFocus = () => void refresh()
    window.addEventListener('focus', onFocus)
    // Usage numbers are written by running Claude Code sessions; pick them up regularly.
    const poll = setInterval(() => document.visibilityState === 'visible' && void refresh(), 20_000)
    const off = window.api.onTermExit(({ id, exitCode }) => {
      const tab = live.current.tabs.find((t) => t.id === id)
      // A session ended: login state or usage may have changed for that account.
      if (tab) void window.api.refreshAccounts(tab.profile)
      setTabs((ts) => ts.map((t) => (t.id === id ? { ...t, exitCode } : t)))
      void refresh()
    })
    const offSession = window.api.onSessionUpdate((info) => {
      const prev = live.current.sessions[info.tabId]
      setSessions((s) => ({ ...s, [info.tabId]: info }))
      if (prev?.activity !== info.activity) {
        if (info.activity === 'needs-you' || info.activity === 'done' || info.activity === 'error') attention(info)
        if (info.activity === 'done' || info.activity === 'ended') setTick((t) => t + 1)
        if (info.activity === 'working') setUnseen(({ [info.tabId]: _, ...rest }) => rest)
      }
    })
    const offClick = window.api.onNotificationClick((tabId) => {
      const tab = live.current.tabs.find((t) => t.id === tabId)
      if (!tab) return
      setView(tab.kind === 'login' ? { kind: 'accounts' } : { kind: 'project', id: tab.projectId! })
      setActiveTab((a) => ({ ...a, [groupOf(tab)]: tab.id }))
    })
    const offState = window.api.onStateChanged(() => void refresh())
    const offUpdate = window.api.onUpdateStatus((update) => {
      setState((s) => (s ? { ...s, update } : s))
      if (update.state === 'ready') {
        toast.success(`Version ${update.version} is ready`, {
          id: 'update-ready',
          duration: Infinity,
          description: 'Restart to install. It also installs automatically when you quit.',
          action: { label: 'Restart', onClick: () => window.api.installUpdate() }
        })
      }
    })
    return () => {
      window.removeEventListener('focus', onFocus)
      clearInterval(poll)
      off()
      offSession()
      offClick()
      offState()
      offUpdate()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refresh])

  const openTerminal = useCallback(
    async (req: Omit<SpawnRequest, 'cols' | 'rows'>) => {
      const res = await window.api.spawnTerminal({ ...req, cols: 120, rows: 32 })
      if (!res.ok) return showError(res.error)
      const tab: Tab = { id: res.value, kind: req.kind, projectId: req.projectId, profile: req.profile, startedAt: Date.now() }
      setTabs((ts) => [...ts, tab])
      setActiveTab((a) => ({ ...a, [groupOf(tab)]: tab.id }))
      void refresh()
    },
    [refresh]
  )

  const closeTab = useCallback(
    async (tab: Tab) => {
      if (
        tab.exitCode === undefined &&
        !(await confirm({
          title: 'Close this session?',
          description: `Claude Code (${tab.profile}) is still running in this tab. Closing it stops the process.`,
          action: 'Close tab',
          destructive: true
        }))
      )
        return
      window.api.killTerminal(tab.id)
      forget(tab.id)
      setSessions(({ [tab.id]: _, ...rest }) => rest)
      setUnseen(({ [tab.id]: _, ...rest }) => rest)
      setTabs((ts) => {
        const rest = ts.filter((t) => t.id !== tab.id)
        const group = groupOf(tab)
        setActiveTab((a) => {
          if (a[group] !== tab.id) return a
          const next = rest.filter((t) => groupOf(t) === group).at(-1)
          return { ...a, [group]: next?.id ?? '' }
        })
        return rest
      })
    },
    [confirm]
  )

  const addProject = useCallback(async () => {
    const res = await window.api.addProject()
    if (!res.ok) return showError(res.error)
    await refresh()
    if (res.value) setView({ kind: 'project', id: res.value.id })
  }, [refresh])

  const removeProject = useCallback(
    async (project: ProjectInfo) => {
      const ok = await confirm({
        title: `Remove "${project.name}"?`,
        description: 'It is only removed from this list. Files on disk stay untouched. Open sessions for it are closed.',
        action: 'Remove',
        destructive: true
      })
      if (!ok) return
      for (const t of tabs.filter((t) => t.projectId === project.id)) {
        window.api.killTerminal(t.id)
        forget(t.id)
      }
      setTabs((ts) => ts.filter((t) => t.projectId !== project.id))
      await window.api.removeProject(project.id)
      await refresh()
      setView({ kind: 'dashboard' })
    },
    [confirm, refresh, tabs]
  )

  const running = useMemo(() => {
    const m: Record<string, number> = {}
    for (const t of tabs) if (t.exitCode === undefined) m[groupOf(t)] = (m[groupOf(t)] ?? 0) + 1
    return m
  }, [tabs])

  /** Signal per tab and the strongest one per group (project / accounts). */
  const tabSignal = (t: Tab) => (t.exitCode !== undefined ? null : signalOf(sessions[t.id]?.activity, !!unseen[t.id]))
  const groupSignal = useMemo(() => {
    const m: Record<string, ReturnType<typeof strongest>> = {}
    for (const t of tabs) m[groupOf(t)] = strongest([m[groupOf(t)] ?? null, tabSignal(t)])
    return m
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabs, sessions, unseen])

  // ---- Voice dictation (Dictate button / Ctrl+Space) ----
  const activeTabObj = (() => {
    const group = view.kind === 'project' ? view.id : view.kind === 'accounts' ? 'accounts' : null
    const id = group ? activeTab[group] : undefined
    return tabs.find((t) => t.id === id && t.exitCode === undefined)
  })()
  const activeId = activeTabObj?.id
  const voiceTap = state?.settings.voiceMode === 'tap'

  // Looking at a tab (window focused) clears its "unseen" mark.
  useEffect(() => {
    if (!activeId) return
    const clear = () => document.hasFocus() && setUnseen(({ [activeId]: _, ...rest }) => rest)
    clear()
    window.addEventListener('focus', clear)
    return () => window.removeEventListener('focus', clear)
  }, [activeId, view])

  // When voice was turned on in this run: sessions started earlier don't have it yet.
  const voiceOnSince = useRef(0)
  const prevVoice = useRef<string | undefined | null>(null) // null = state not loaded yet
  useEffect(() => {
    if (!state) return
    const now = state.settings.voiceMode
    if (prevVoice.current !== null && prevVoice.current !== 'tap' && now === 'tap') voiceOnSince.current = Date.now()
    prevVoice.current = now
  }, [state])

  const enableVoice = useCallback(async () => {
    if (!state) return
    const r = await window.api.saveSettings({ ...state.settings, voiceMode: 'tap' })
    if (!r.ok) return showError(r.error)
    await refresh()
    toast.success('Voice dictation is on for all accounts', {
      description: 'Open a new Claude tab, then press Ctrl+Space (or Dictate) and talk. Press again to send.'
    })
  }, [state, refresh])

  const dictate = useCallback(
    (tab: Tab) => {
      if (!voiceTap) {
        toast('Voice dictation is off', {
          id: 'voice-off',
          description: state?.settings.claudeLanguage
            ? 'Turn it on for all accounts (tap mode)?'
            : 'Turn it on for all accounts (tap mode)? Also set Claude language in Settings, or dictation expects English.',
          action: { label: 'Turn on', onClick: () => void enableVoice() }
        })
        return
      }
      if (tab.startedAt < voiceOnSince.current) {
        toast('This session started before voice was turned on', {
          id: 'voice-old-session',
          description: 'Dictation works in sessions started after that. Open a new tab for this account.',
          action: {
            label: 'New tab',
            onClick: () => void openTerminal({ kind: tab.kind, projectId: tab.projectId, profile: tab.profile })
          }
        })
        return
      }
      window.api.dictate(tab.id)
      // Give keyboard focus back to the terminal.
      ;(document.querySelector('.terminal-host[style*="block"] textarea') as HTMLTextAreaElement | null)?.focus()
    },
    [voiceTap, state, enableVoice, openTerminal]
  )

  useEffect(() => {
    if (!activeTabObj) return
    const onKey = (e: KeyboardEvent) => {
      if (e.type === 'keydown' && e.ctrlKey && !e.altKey && !e.shiftKey && e.code === 'Space' && !e.repeat) {
        e.preventDefault()
        e.stopPropagation()
        dictate(activeTabObj)
      }
    }
    window.addEventListener('keydown', onKey, true) // capture: before xterm sees it
    return () => window.removeEventListener('keydown', onKey, true)
  }, [activeTabObj, dictate])

  // ---- Hand off to another account ----
  // phase: 'wait-idle' (Claude still busy) -> 'writing' (STATUS.md prompt sent) -> new tab on the target.
  const [handoffOpen, setHandoffOpen] = useState(false)
  const [handoff, setHandoff] = useState<{
    from: string
    to: string
    projectId: string
    phase: 'wait-idle' | 'writing'
    sawWorking: boolean
  } | null>(null)

  const sendToTab = (tabId: string, text: string) => {
    window.api.writeTerminal(tabId, text)
    setTimeout(() => window.api.writeTerminal(tabId, '\r'), 300)
  }

  const finishHandoff = useCallback(
    (h: NonNullable<typeof handoff>) => {
      setHandoff(null)
      void openTerminal({ kind: 'project', projectId: h.projectId, profile: h.to, initialPrompt: CONTINUE_PROMPT })
      const from = live.current.tabs.find((t) => t.id === h.from)
      // Same id: replaces the "updating STATUS.md…" loading toast.
      toast.success(`Handed off to ${h.to}`, {
        id: 'handoff',
        duration: 10_000,
        cancel: undefined,
        description: 'It reads docs/STATUS.md and continues. You can close the old tab.',
        action: from ? { label: 'Close old tab', onClick: () => void closeTab(from) } : undefined
      })
    },
    [openTerminal, closeTab]
  )

  const startHandoff = useCallback(
    (from: Tab, to: string, mode: HandoffMode) => {
      const s = live.current.sessions[from.id]
      if (mode === 'full') {
        if (!s?.transcriptPath) return showError('This session has no saved conversation yet.')
        void openTerminal({ kind: 'project', projectId: from.projectId, profile: to, resumePath: s.transcriptPath })
        toast.success(`Continuing the conversation on ${to}`, {
          description: 'Close the old tab so both accounts don\'t work on the same thing.',
          action: { label: 'Close old tab', onClick: () => void closeTab(from) }
        })
        return
      }
      const busy = s?.activity === 'working'
      if (!busy) sendToTab(from.id, HANDOFF_PROMPT)
      setHandoff({ from: from.id, to, projectId: from.projectId!, phase: busy ? 'wait-idle' : 'writing', sawWorking: false })
      toast.loading(busy ? `Waiting for Claude (${from.profile}) to finish…` : `Claude (${from.profile}) is updating docs/STATUS.md…`, {
        id: 'handoff',
        duration: Infinity,
        description: `Then a new ${to} session continues from it.`,
        action: { label: 'Open now', onClick: () => finishHandoff({ from: from.id, to, projectId: from.projectId!, phase: 'writing', sawWorking: true }) },
        cancel: { label: 'Cancel', onClick: () => setHandoff(null) }
      })
    },
    [openTerminal, closeTab, finishHandoff]
  )

  // Drive the summary handoff from the old tab's activity.
  useEffect(() => {
    if (!handoff) return
    const a = sessions[handoff.from]?.activity
    if (handoff.phase === 'wait-idle' && (a === 'done' || a === 'idle')) {
      sendToTab(handoff.from, HANDOFF_PROMPT)
      setHandoff({ ...handoff, phase: 'writing', sawWorking: false })
      toast.loading('Claude is updating docs/STATUS.md…', { id: 'handoff', duration: Infinity })
    } else if (handoff.phase === 'writing') {
      if (a === 'working' && !handoff.sawWorking) setHandoff({ ...handoff, sawWorking: true })
      else if (a === 'done' && handoff.sawWorking) finishHandoff(handoff)
      else if (a === 'needs-you')
        toast.loading('Claude needs your OK in the old tab (for example, to commit).', {
          id: 'handoff',
          duration: Infinity,
          description: 'Approve it there; the handoff continues when Claude finishes.'
        })
    }
    if (a === 'error' || a === 'ended') {
      setHandoff(null)
      toast.error('Handoff stopped: the old session failed or closed', {
        id: 'handoff',
        duration: 10_000,
        action: { label: 'Open anyway', onClick: () => finishHandoff({ ...handoff, sawWorking: true }) }
      })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [handoff, sessions])

  const togglePanel = useCallback(() => {
    setPanelOpen((open) => {
      try {
        localStorage.setItem(PANEL_KEY, open ? '0' : '1')
      } catch {
        // storage unavailable: just don't remember
      }
      return !open
    })
  }, [])

  if (!state) return <div className="p-8 text-sm text-muted-foreground">Loading…</div>

  const currentGroup = view.kind === 'project' ? view.id : view.kind === 'accounts' ? 'accounts' : null
  const project = view.kind === 'project' ? state.projects.find((p) => p.id === view.id) : undefined
  const groupTabs = currentGroup ? tabs.filter((t) => groupOf(t) === currentGroup) : []
  const showTerminals = currentGroup !== null
  const showPanel = view.kind === 'project' && !!project && panelOpen

  // The panel follows the selected tab of this project (even if its process already exited).
  const panelTab = project ? tabs.find((t) => t.id === activeTab[project.id]) : undefined
  const openSessionIds = new Set(
    tabs.filter((t) => t.exitCode === undefined).map((t) => sessions[t.id]?.sessionId).filter((x): x is string => !!x)
  )

  return (
    <SidebarProvider className="h-full min-h-0">
      <Sidebar collapsible="none" className="h-full border-r">
        <SidebarHeader className="app-drag h-10 shrink-0 flex-row items-center px-4 py-0">
          <img src={iconUrl} alt="" className="size-5" draggable={false} />
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton isActive={view.kind === 'dashboard'} onClick={() => setView({ kind: 'dashboard' })}>
                  <SquaresFourIcon />
                  <span>Dashboard</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroup>
          <SidebarGroup>
            <SidebarGroupLabel>Projects</SidebarGroupLabel>
            <SidebarGroupAction title="Add project" onClick={addProject}>
              <PlusIcon />
            </SidebarGroupAction>
            <SidebarGroupContent>
              <SidebarMenu>
                {state.projects.map((p) => (
                  <SidebarMenuItem key={p.id}>
                    <SidebarMenuButton
                      isActive={view.kind === 'project' && view.id === p.id}
                      onClick={() => setView({ kind: 'project', id: p.id })}
                      tooltip={p.path}
                      className={cn(!p.exists && 'text-muted-foreground line-through')}
                    >
                      <FolderSimpleIcon />
                      <span>{p.name}</span>
                    </SidebarMenuButton>
                    {running[p.id] ? (
                      <SidebarMenuBadge className="gap-1.5">
                        <SignalDot signal={groupSignal[p.id] ?? null} />
                        {running[p.id]}
                      </SidebarMenuBadge>
                    ) : null}
                  </SidebarMenuItem>
                ))}
                {state.projects.length === 0 && (
                  <p className="px-2 py-1 text-xs text-muted-foreground">No projects yet.</p>
                )}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton isActive={view.kind === 'accounts'} onClick={() => setView({ kind: 'accounts' })}>
                <UsersThreeIcon />
                <span>Accounts</span>
              </SidebarMenuButton>
              {running.accounts ? (
                <SidebarMenuBadge className="gap-1.5">
                  <SignalDot signal={groupSignal.accounts ?? null} />
                  {running.accounts}
                </SidebarMenuBadge>
              ) : null}
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuButton isActive={view.kind === 'settings'} onClick={() => setView({ kind: 'settings' })}>
                <GearSixIcon />
                <span>Settings</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
      </Sidebar>

      <SidebarInset className="flex min-h-0 min-w-0 flex-col overflow-hidden">
        <TitleBar
          view={view}
          project={project}
          panelOpen={panelOpen}
          onTogglePanel={view.kind === 'project' && project ? togglePanel : undefined}
        />
        <div className="flex min-h-0 flex-1">
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            {!state.claudePath && (
              <Alert variant="destructive" className="m-4 mb-0 w-auto">
                <WarningIcon />
                <AlertTitle>Claude Code was not found</AlertTitle>
                <AlertDescription>Install it, or set the path to claude.exe in Settings.</AlertDescription>
              </Alert>
            )}

            {view.kind === 'dashboard' && (
              <Dashboard state={state} running={running} onOpen={(id) => setView({ kind: 'project', id })} onAdd={addProject} />
            )}
            {view.kind === 'project' && project && (
              <ProjectView
                project={project}
                profiles={state.profiles}
                onLaunch={(profile) => openTerminal({ kind: 'project', projectId: project.id, profile })}
                onRemove={() => removeProject(project)}
              />
            )}
            {view.kind === 'project' && !project && <p className="p-8 text-sm text-muted-foreground">This project was removed.</p>}
            {view.kind === 'accounts' && (
              <AccountsView
                profiles={state.profiles}
                refreshing={state.accountsRefreshing}
                onChanged={refresh}
                onSignIn={(profile) => openTerminal({ kind: 'login', profile })}
              />
            )}
            {view.kind === 'settings' && <SettingsView state={state} onSaved={refresh} />}

            <section className={cn('min-h-0 flex-1 flex-col border-t', showTerminals ? 'flex' : 'hidden')}>
              {groupTabs.length > 0 && (
                <div className="flex shrink-0 gap-px overflow-x-auto border-b bg-sidebar">
                  {groupTabs.map((t) => {
                    const active = activeTab[currentGroup!] === t.id
                    return (
                      <div
                        key={t.id}
                        onClick={() => setActiveTab((a) => ({ ...a, [currentGroup!]: t.id }))}
                        className={cn(
                          'flex cursor-pointer items-center gap-2 border-r py-1 pr-1 pl-3 text-xs whitespace-nowrap text-muted-foreground',
                          active && 'bg-background text-foreground'
                        )}
                      >
                        <SignalDot signal={tabSignal(t)} />
                        <ProfileTag name={t.profile} />
                        <span className={cn(t.exitCode !== undefined && 'line-through')}>
                          {t.kind === 'login' ? 'sign in' : (sessions[t.id]?.sessionName?.slice(0, 28) ?? 'claude')}
                        </span>
                        <Button
                          variant="ghost"
                          size="icon-xs"
                          title="Close tab"
                          onClick={(e) => {
                            e.stopPropagation()
                            void closeTab(t)
                          }}
                        >
                          <XIcon />
                        </Button>
                      </div>
                    )
                  })}
                </div>
              )}
              <div className="relative min-h-0 flex-1 bg-background">
                {/* Every terminal stays mounted; only the active one in the current view is shown. */}
                {tabs.map((t) => (
                  <TerminalPane
                    key={t.id}
                    id={t.id}
                    exitCode={t.exitCode}
                    active={showTerminals && groupOf(t) === currentGroup && activeTab[currentGroup!] === t.id}
                  />
                ))}
                {showTerminals && groupTabs.length === 0 && (
                  <p className="p-6 text-xs text-muted-foreground">
                    {view.kind === 'accounts' ? 'Sign-in terminals appear here.' : 'No Claude Code session open. Pick an account above to start one.'}
                  </p>
                )}
              </div>
              {activeTabObj?.kind === 'project' && (
                <Composer
                  tabId={activeTabObj.id}
                  activity={sessions[activeTabObj.id]?.activity}
                  voiceOn={voiceTap}
                  onDictate={() => dictate(activeTabObj)}
                />
              )}
            </section>
          </div>

          {showPanel && (
            <SessionPanel
              project={project!}
              session={panelTab ? sessions[panelTab.id] : undefined}
              profile={panelTab ? state.profiles.find((p) => p.name === panelTab.profile) : undefined}
              tick={tick}
              openSessionIds={openSessionIds}
              onResume={(h: HistorySession) =>
                void openTerminal({ kind: 'project', projectId: project!.id, profile: h.profile, resumeSessionId: h.id })
              }
              onHandoff={panelTab && panelTab.exitCode === undefined && !handoff ? () => setHandoffOpen(true) : undefined}
            />
          )}
          {panelTab && (
            <HandoffDialog
              open={handoffOpen}
              onOpenChange={setHandoffOpen}
              from={panelTab.profile}
              session={sessions[panelTab.id]}
              profiles={state.profiles}
              onConfirm={(to, mode) => startHandoff(panelTab, to, mode)}
            />
          )}
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}
