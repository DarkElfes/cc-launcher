// Types shared by the main process, preload and renderer.

export interface Project {
  id: string
  name: string
  path: string
  addedAt: number
  lastOpenedAt?: number
  /** Profile used the last time Claude Code was opened for this project. */
  lastProfile?: string
}

export interface ProjectInfo extends Project {
  /** False when the folder no longer exists on disk. */
  exists: boolean
}

export interface ProfileInfo {
  name: string
  /** Config folder passed as CLAUDE_CONFIG_DIR: ~/.claude-profiles/<name>. */
  dir: string
  /** From `claude auth status` when known, else whether a credentials file exists (never read). */
  signedIn: boolean
  /** Account behind this profile, from `claude auth status --json`. */
  account?: AccountInfo
  /** Latest plan usage: from `/usage` or from a running session's status line, whichever is newer. */
  usage?: ProfileUsage
  /** Last refresh error, if any. */
  error?: string
}

export interface AccountInfo {
  loggedIn: boolean
  email?: string
  /** e.g. "pro", "max" */
  plan?: string
  orgName?: string
}

export interface UsageWindow {
  /** 0–100. 0 with stale=true means the window has reset since the last report. */
  usedPercent: number
  /** Epoch ms when the window resets, or null when unknown / already reset. */
  resetsAt: number | null
  /** Reset time as Claude Code printed it (e.g. "Nov 10, 7pm"), when available. */
  resetsLabel?: string
  stale: boolean
}

export interface ProfileUsage {
  /** Epoch ms when this was measured. */
  updatedAt: number
  fiveHour?: UsageWindow
  week?: UsageWindow
  source: 'usage-command' | 'status-line'
}

export interface Settings {
  /** Full path to claude.exe. Empty means auto-detect. */
  claudePath?: string
  /** Command used for "Open in editor". Defaults to "zed". */
  editorCommand?: string
}

export interface UpdateStatus {
  /** disabled = dev build, or installer built without a publish target. */
  state: 'idle' | 'disabled' | 'checking' | 'none' | 'downloading' | 'ready' | 'error'
  version?: string
  percent?: number
  error?: string
}

export interface AppState {
  appVersion: string
  update: UpdateStatus
  /** True while account emails / plan usage are being refreshed in the background. */
  accountsRefreshing: boolean
  projects: ProjectInfo[]
  profiles: ProfileInfo[]
  settings: Settings
  /** The claude executable that will be launched, or null when none was found. */
  claudePath: string | null
}

export interface SpawnRequest {
  /** "project" opens Claude Code in a project; "login" opens it in the profile folder to sign in. */
  kind: 'project' | 'login'
  projectId?: string
  profile: string
  cols: number
  rows: number
}

export type Result<T = null> = { ok: true; value: T } | { ok: false; error: string }

export interface TermDataEvent {
  id: string
  data: string
}

export interface TermExitEvent {
  id: string
  exitCode: number
}

export interface Api {
  getState(): Promise<AppState>
  addProject(): Promise<Result<ProjectInfo | null>>
  removeProject(id: string): Promise<Result>
  addProfile(name: string): Promise<Result>
  removeProfile(name: string): Promise<Result<boolean>>
  saveSettings(settings: Settings): Promise<Result>
  openFolder(path: string): Promise<Result>
  openInEditor(path: string): Promise<Result>
  spawnTerminal(req: SpawnRequest): Promise<Result<string>>
  writeTerminal(id: string, data: string): void
  resizeTerminal(id: string, cols: number, rows: number): void
  killTerminal(id: string): void
  onTermData(cb: (e: TermDataEvent) => void): () => void
  onTermExit(cb: (e: TermExitEvent) => void): () => void
  /** Re-reads account email and plan usage (all profiles, or one). */
  refreshAccounts(profile?: string): Promise<Result>
  /** Fires when background data (accounts, usage) changed; call getState() again. */
  onStateChanged(cb: () => void): () => void
  checkForUpdates(): Promise<UpdateStatus>
  installUpdate(): void
  onUpdateStatus(cb: (s: UpdateStatus) => void): () => void
}
