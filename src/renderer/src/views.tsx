import { useState, type ReactNode } from 'react'
import {
  ArrowClockwiseIcon,
  CodeIcon,
  FolderOpenIcon,
  FolderPlusIcon,
  FolderSimpleIcon,
  PlayIcon,
  PlusIcon,
  SignInIcon,
  TrashIcon
} from '@phosphor-icons/react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { cn } from '@/lib/utils'
import type { AppState, ProfileInfo, ProjectInfo, Result } from '../../shared/types'
import { AccountLine, ProfileTag, showError, SignedIn, timeAgo, UsageInline, UsageMeter } from './ui'

async function run<T>(p: Promise<Result<T>>): Promise<T | undefined> {
  const r = await p
  if (!r.ok) {
    showError(r.error)
    return undefined
  }
  return r.value
}

function PageHeader(props: { title: string; description?: ReactNode; actions?: ReactNode; mono?: boolean }) {
  return (
    <header className="flex items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-lg font-semibold tracking-tight">{props.title}</h1>
        {props.description && (
          <p className={cn('mt-1 text-xs text-muted-foreground', props.mono && 'truncate')}>{props.description}</p>
        )}
      </div>
      {props.actions && <div className="flex shrink-0 gap-2">{props.actions}</div>}
    </header>
  )
}

export function Dashboard(props: {
  state: AppState
  running: Record<string, number>
  onOpen: (id: string) => void
  onAdd: () => void
}) {
  const { state, running } = props
  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="flex flex-col gap-6">
        <PageHeader
          title="Projects"
          description="Each project keeps its own state. Open it with any of your accounts."
          actions={
            <Button onClick={props.onAdd}>
              <PlusIcon data-icon="inline-start" />
              Add project
            </Button>
          }
        />
        {state.projects.length === 0 ? (
          <Empty className="border border-dashed">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <FolderPlusIcon />
              </EmptyMedia>
              <EmptyTitle>No projects yet</EmptyTitle>
              <EmptyDescription>Add an existing folder, or create a new one from the folder picker.</EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button onClick={props.onAdd}>Add project</Button>
            </EmptyContent>
          </Empty>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-4">
            {state.projects.map((p) => (
              <Card
                key={p.id}
                role="button"
                tabIndex={0}
                onClick={() => props.onOpen(p.id)}
                onKeyDown={(e) => e.key === 'Enter' && props.onOpen(p.id)}
                className={cn('cursor-pointer transition-colors hover:ring-primary/40', !p.exists && 'opacity-60')}
              >
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <FolderSimpleIcon />
                    {p.name}
                    {running[p.id] ? <Badge className="ml-auto">{running[p.id]} running</Badge> : null}
                  </CardTitle>
                  <CardDescription className="truncate">{p.path}</CardDescription>
                </CardHeader>
                <CardContent className="flex items-center gap-3 text-xs text-muted-foreground">
                  {p.exists ? (
                    <>
                      <span>Opened {timeAgo(p.lastOpenedAt)}</span>
                      {p.lastProfile && <ProfileTag name={p.lastProfile} />}
                    </>
                  ) : (
                    <Badge variant="destructive">folder not found</Badge>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

export function ProjectView(props: {
  project: ProjectInfo
  profiles: ProfileInfo[]
  onLaunch: (profile: string) => void
  onRemove: () => void
}) {
  const { project, profiles } = props
  return (
    <div className="flex shrink-0 flex-col gap-4 p-6 pb-4">
      <PageHeader
        title={project.name}
        description={project.path}
        mono
        actions={
          <>
            <Button variant="outline" size="sm" onClick={() => run(window.api.openFolder(project.path))}>
              <FolderOpenIcon data-icon="inline-start" />
              Explorer
            </Button>
            <Button variant="outline" size="sm" onClick={() => run(window.api.openInEditor(project.path))}>
              <CodeIcon data-icon="inline-start" />
              Editor
            </Button>
            <Button variant="destructive" size="sm" onClick={props.onRemove}>
              <TrashIcon data-icon="inline-start" />
              Remove
            </Button>
          </>
        }
      />
      {!project.exists ? (
        <p className="text-xs text-destructive">The folder no longer exists. Remove the project or restore the folder.</p>
      ) : (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-muted-foreground">
            {profiles.length ? 'Open Claude Code with' : 'No accounts yet. Add one under Accounts first.'}
          </span>
          {profiles.map((p) => (
            <Button
              key={p.name}
              variant="outline"
              size="sm"
              className={cn(project.lastProfile === p.name && 'border-primary')}
              title={
                p.signedIn
                  ? `${p.account?.email ?? ''}${p.account?.plan ? ` (${p.account.plan})` : ''}\n${p.dir}`.trim()
                  : `${p.dir}\nNot signed in yet. Claude Code will ask you to log in.`
              }
              onClick={() => props.onLaunch(p.name)}
            >
              <PlayIcon data-icon="inline-start" />
              <ProfileTag name={p.name} />
              <UsageInline usage={p.usage} />
              {!p.signedIn && (
                <Badge variant="outline" className="border-amber-500/40 text-amber-400">
                  login
                </Badge>
              )}
            </Button>
          ))}
        </div>
      )}
    </div>
  )
}

export function AccountsView(props: {
  profiles: ProfileInfo[]
  refreshing: boolean
  onChanged: () => void
  onSignIn: (profile: string) => void
}) {
  const [name, setName] = useState('')
  const add = async () => {
    const n = name.trim().toLowerCase()
    if (!n) return
    const r = await window.api.addProfile(n)
    if (!r.ok) return showError(r.error)
    setName('')
    props.onChanged()
  }
  return (
    <div className="flex max-h-[55%] shrink-0 flex-col gap-4 overflow-y-auto p-6 pb-4">
      <PageHeader
        title="Accounts"
        description="One profile per Claude account, each in its own folder under ~/.claude-profiles. The normal ~/.claude folder isn't used, so `claude` in other terminals can never switch an account here."
        actions={
          <Button variant="outline" size="sm" disabled={props.refreshing} onClick={() => void run(window.api.refreshAccounts())}>
            <ArrowClockwiseIcon data-icon="inline-start" className={cn(props.refreshing && 'animate-spin')} />
            {props.refreshing ? 'Refreshing…' : 'Refresh'}
          </Button>
        }
      />
      {props.profiles.length === 0 && (
        <p className="text-xs text-amber-400">No accounts yet. Add one below, then click Sign in.</p>
      )}
      <div className="border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Profile</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>5-hour limit</TableHead>
              <TableHead>Weekly limit</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {props.profiles.map((p) => (
              <TableRow key={p.name}>
                <TableCell title={p.dir}>
                  <div className="flex max-w-64 flex-col gap-1">
                    <ProfileTag name={p.name} />
                    <AccountLine profile={p} />
                  </div>
                </TableCell>
                <TableCell>
                  <div className="flex flex-col items-start gap-1">
                    <SignedIn profile={p} />
                    <span className="text-[10px] text-muted-foreground" title={p.error}>
                      {p.error
                        ? 'refresh failed'
                        : p.usage
                          ? `updated ${timeAgo(p.usage.updatedAt)}`
                          : p.signedIn
                            ? 'loading usage…'
                            : 'sign in to see usage'}
                    </span>
                  </div>
                </TableCell>
                <TableCell>
                  <UsageMeter label="5-hour limit" w={p.usage?.fiveHour} />
                </TableCell>
                <TableCell>
                  <UsageMeter label="Weekly · all models" w={p.usage?.week} />
                </TableCell>
                <TableCell>
                  <div className="flex justify-end gap-2">
                    <Button variant="outline" size="xs" onClick={() => props.onSignIn(p.name)}>
                      <SignInIcon data-icon="inline-start" />
                      {p.signedIn ? 'Open' : 'Sign in'}
                    </Button>
                    <Button variant="outline" size="xs" onClick={() => run(window.api.openFolder(p.dir))}>
                      <FolderOpenIcon data-icon="inline-start" />
                      Folder
                    </Button>
                    <Button
                      variant="destructive"
                      size="xs"
                      onClick={async () => {
                        await run(window.api.removeProfile(p.name))
                        props.onChanged()
                      }}
                    >
                      <TrashIcon data-icon="inline-start" />
                      Remove
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault()
          void add()
        }}
      >
        <Input className="w-72" value={name} onChange={(e) => setName(e.target.value)} placeholder="new profile name, e.g. work" />
        <Button type="submit" disabled={!name.trim()}>
          <PlusIcon data-icon="inline-start" />
          Add profile
        </Button>
      </form>
      <p className="text-xs text-muted-foreground">
        To sign in, click <b>Sign in</b>. Claude Code opens below; follow its login steps (type <code>/login</code> if it doesn't ask).
      </p>
      <p className="text-xs text-muted-foreground">
        Email and plan come from <code>claude auth status</code>. Limits come from Claude Code's own{' '}
        <code>/usage</code>, which runs locally and doesn't use your limits. They refresh every 5 minutes, and
        live from the status line while a session is running.
      </p>
    </div>
  )
}

/** Languages Claude Code voice dictation supports (https://code.claude.com/docs/en/voice-dictation). */
const LANGUAGES = [
  'ukrainian', 'english', 'russian', 'polish', 'german', 'french', 'spanish', 'italian', 'portuguese', 'dutch',
  'czech', 'danish', 'norwegian', 'swedish', 'greek', 'turkish', 'hindi', 'indonesian', 'japanese', 'korean'
]
const KEEP = 'keep'
const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

export function SettingsView(props: { state: AppState; onSaved: () => void }) {
  const [claudePath, setClaudePath] = useState(props.state.settings.claudePath ?? '')
  const [editor, setEditor] = useState(props.state.settings.editorCommand ?? '')
  const [language, setLanguage] = useState(props.state.settings.claudeLanguage ?? KEEP)
  const [voiceMode, setVoiceMode] = useState<string>(props.state.settings.voiceMode ?? KEEP)
  const [saved, setSaved] = useState(false)
  const touch = () => setSaved(false)
  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="flex max-w-xl flex-col gap-6">
        <PageHeader title="Settings" />
        <form
          onSubmit={async (e) => {
            e.preventDefault()
            const r = await window.api.saveSettings({
              claudePath,
              editorCommand: editor,
              claudeLanguage: language === KEEP ? undefined : language,
              voiceMode: voiceMode === KEEP ? undefined : (voiceMode as 'tap' | 'hold' | 'off')
            })
            if (!r.ok) return showError(r.error)
            setSaved(true)
            props.onSaved()
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="claude-path">Claude Code executable</FieldLabel>
              <Input
                id="claude-path"
                value={claudePath}
                onChange={(e) => {
                  setClaudePath(e.target.value)
                  setSaved(false)
                }}
                placeholder="auto-detect"
              />
              <FieldDescription>Currently using: {props.state.claudePath ?? 'not found'}</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="editor">Editor command</FieldLabel>
              <Input
                id="editor"
                value={editor}
                onChange={(e) => {
                  setEditor(e.target.value)
                  setSaved(false)
                }}
                placeholder="zed"
              />
              <FieldDescription>Runs as: &lt;command&gt; "&lt;project folder&gt;"</FieldDescription>
            </Field>
            <Field>
              <FieldLabel>Claude language (replies and voice)</FieldLabel>
              <Select
                value={language}
                onValueChange={(v) => {
                  setLanguage(v)
                  touch()
                }}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={KEEP}>Don't change (set per account with /config)</SelectItem>
                  {LANGUAGES.map((l) => (
                    <SelectItem key={l} value={l}>
                      {cap(l)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <FieldDescription>
                Applied to every account. Claude replies in this language, and voice dictation recognizes it.
              </FieldDescription>
            </Field>
            <Field>
              <FieldLabel>Voice dictation</FieldLabel>
              <Select
                value={voiceMode}
                onValueChange={(v) => {
                  setVoiceMode(v)
                  touch()
                }}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={KEEP}>Don't change (set per account with /voice)</SelectItem>
                  <SelectItem value="tap">On: tap (Dictate button or Ctrl+Space: start, again to send)</SelectItem>
                  <SelectItem value="hold">On: hold Space while talking</SelectItem>
                  <SelectItem value="off">Off</SelectItem>
                </SelectContent>
              </Select>
              <FieldDescription>
                Uses Claude Code's own dictation (Anthropic speech-to-text, doesn't count toward your limits).
                Applies to sessions started after saving. Windows must allow microphone access for desktop apps.
              </FieldDescription>
            </Field>
            <Field orientation="horizontal">
              <Button type="submit">Save</Button>
              {saved && <span className="text-xs text-muted-foreground">Saved.</span>}
            </Field>
          </FieldGroup>
        </form>
        <UpdatesSection state={props.state} />
      </div>
    </div>
  )
}

function updateText(s: AppState['update']): string {
  switch (s.state) {
    case 'disabled':
      return 'Auto-update is off in this build (development build, or no update source configured).'
    case 'checking':
      return 'Checking for updates…'
    case 'none':
      return 'You have the latest version.'
    case 'downloading':
      return `Downloading ${s.version ?? 'update'}… ${s.percent ?? 0}%`
    case 'ready':
      return `Version ${s.version} is downloaded and ready to install.`
    case 'error':
      return `Update check failed: ${s.error}`
    default:
      return 'Updates are checked at startup and every 4 hours.'
  }
}

function UpdatesSection({ state }: { state: AppState }) {
  const u = state.update
  return (
    <Field>
      <FieldLabel>Version {state.appVersion}</FieldLabel>
      <FieldDescription>{updateText(u)}</FieldDescription>
      <div className="flex gap-2">
        {u.state === 'ready' ? (
          <Button onClick={() => window.api.installUpdate()}>Restart and update</Button>
        ) : (
          <Button
            variant="outline"
            disabled={u.state === 'disabled' || u.state === 'checking' || u.state === 'downloading'}
            onClick={() => void window.api.checkForUpdates()}
          >
            Check for updates
          </Button>
        )}
      </div>
    </Field>
  )
}
