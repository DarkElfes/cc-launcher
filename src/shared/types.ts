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
  /** True when a credentials file exists in the folder. Its contents are never read. */
  signedIn: boolean
  /** Last plan usage reported by Claude Code's status line for this account, if any. */
  usage?: ProfileUsage
}

export interface UsageWindow {
  /** 0–100. 0 with stale=true means the window has reset since the last report. */
  usedPercent: number
  /** Epoch ms when the window resets, or null when it already reset. */
  resetsAt: number | null
  stale: boolean
}

export interface ProfileUsage {
  /** Epoch ms of the last report from a running Claude Code session. */
  updatedAt: number
  fiveHour?: UsageWindow
  week?: UsageWindow
}

export interface Settings {
  /** Full path to claude.exe. Empty means auto-detect. */
  claudePath?: string
  /** Command used for "Open in editor". Defaults to "zed". */
  editorCommand?: string
}

export interface UpdateStatus {
  /** disabled = dev build, or installer built without a publish target. needs-token = no GitHub token saved. */
  state: 'idle' | 'disabled' | 'needs-token' | 'checking' | 'none' | 'downloading' | 'ready' | 'error'
  version?: string
  percent?: number
  error?: string
}

export interface AppState {
  appVersion: string
  update: UpdateStatus
  /** Whether a GitHub update token is saved. The token itself never leaves the main process. */
  hasUpdateToken: boolean
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
  checkForUpdates(): Promise<UpdateStatus>
  /** Saves (or with "" removes) the GitHub update token, then re-checks for updates. */
  setUpdateToken(token: string): Promise<Result>
  installUpdate(): void
  onUpdateStatus(cb: (s: UpdateStatus) => void): () => void
}
