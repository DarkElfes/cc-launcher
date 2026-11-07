import { useCallback, useEffect, useMemo, useState } from 'react'
import { FolderSimpleIcon, GearSixIcon, MicrophoneIcon, PlusIcon, SquaresFourIcon, UsersThreeIcon, WarningIcon, XIcon } from '@phosphor-icons/react'
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
import type { AppState, ProjectInfo, SpawnRequest } from '../../shared/types'
import iconUrl from '../../../build/icon.svg'
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
}

/** Key for the group of tabs shown in a view: a project id, or "accounts" for sign-in tabs. */
const groupOf = (t: Tab) => (t.kind === 'login' ? 'accounts' : t.projectId!)

/** Draggable top bar of the main area. Windows draws min/max/close over its right end. */
function TitleBar({ view, project }: { view: View; project?: ProjectInfo }) {
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

  const refresh = useCallback(async () => setState(await window.api.getState()), [])

  useEffect(() => {
    void refresh()
    const onFocus = () => void refresh()
    window.addEventListener('focus', onFocus)
    // Usage numbers are written by running Claude Code sessions; pick them up regularly.
    const poll = setInterval(() => document.visibilityState === 'visible' && void refresh(), 20_000)
    const off = window.api.onTermExit(({ id, exitCode }) => {
      setTabs((ts) => {
        // A session ended: login state or usage may have changed for that account.
        const tab = ts.find((t) => t.id === id)
        if (tab) void window.api.refreshAccounts(tab.profile)
        return ts.map((t) => (t.id === id ? { ...t, exitCode } : t))
      })
      void refresh()
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
      offState()
      offUpdate()
    }
  }, [refresh])

  const openTerminal = useCallback(
    async (req: Omit<SpawnRequest, 'cols' | 'rows'>) => {
      const res = await window.api.spawnTerminal({ ...req, cols: 120, rows: 32 })
      if (!res.ok) return showError(res.error)
      const tab: Tab = { id: res.value, kind: req.kind, projectId: req.projectId, profile: req.profile }
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

  // Ctrl+Space anywhere in the window: start/stop voice dictation in the visible Claude terminal.
  const activeId = (() => {
    const group = view.kind === 'project' ? view.id : view.kind === 'accounts' ? 'accounts' : null
    const id = group ? activeTab[group] : undefined
    return tabs.find((t) => t.id === id && t.exitCode === undefined)?.id
  })()
  const voiceTap = state?.settings.voiceMode === 'tap'
  useEffect(() => {
    if (!activeId || !voiceTap) return
    const onKey = (e: KeyboardEvent) => {
      if (e.type === 'keydown' && e.ctrlKey && !e.altKey && !e.shiftKey && e.code === 'Space' && !e.repeat) {
        e.preventDefault()
        e.stopPropagation()
        window.api.dictate(activeId)
      }
    }
    window.addEventListener('keydown', onKey, true) // capture: before xterm sees it
    return () => window.removeEventListener('keydown', onKey, true)
  }, [activeId, voiceTap])

  if (!state) return <div className="p-8 text-sm text-muted-foreground">Loading…</div>

  const currentGroup = view.kind === 'project' ? view.id : view.kind === 'accounts' ? 'accounts' : null
  const project = view.kind === 'project' ? state.projects.find((p) => p.id === view.id) : undefined
  const groupTabs = currentGroup ? tabs.filter((t) => groupOf(t) === currentGroup) : []
  const showTerminals = currentGroup !== null

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
                    {running[p.id] ? <SidebarMenuBadge>{running[p.id]}</SidebarMenuBadge> : null}
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
              {running.accounts ? <SidebarMenuBadge>{running.accounts}</SidebarMenuBadge> : null}
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
        <TitleBar view={view} project={project} />
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
                    <ProfileTag name={t.profile} />
                    <span className={cn(t.exitCode !== undefined && 'line-through')}>{t.kind === 'login' ? 'sign in' : 'claude'}</span>
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
              {activeId && (
                <div className="ml-auto flex items-center pr-2">
                  <Button
                    variant="ghost"
                    size="xs"
                    disabled={!voiceTap}
                    title={
                      voiceTap
                        ? 'Voice dictation: click (or Ctrl+Space) to start talking, again to send'
                        : 'Turn on voice (tap mode) in Settings to use this button'
                    }
                    onClick={() => {
                      window.api.dictate(activeId)
                      // Give keyboard focus back to the terminal.
                      ;(document.querySelector('.terminal-host[style*="block"] textarea') as HTMLTextAreaElement | null)?.focus()
                    }}
                  >
                    <MicrophoneIcon data-icon="inline-start" />
                    Dictate
                    <kbd className="ml-1 text-[10px] text-muted-foreground">Ctrl+Space</kbd>
                  </Button>
                </div>
              )}
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
        </section>
      </SidebarInset>
    </SidebarProvider>
  )
}
