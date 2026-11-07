# Status

_Last updated: 2026-11-07_

## Done
- Tooling on this PC: Git 2.56, Node 26 LTS, pnpm 12, Claude Code 2.1.319, Zed.
- Checked: `CLAUDE_CONFIG_DIR` is documented officially (code.claude.com, authentication →
  "Log in with multiple accounts"). It moves settings, credentials (`.credentials.json`)
  and session transcripts (`projects/`) to the chosen folder.
- **M1 skeleton**
  - Electron + React + TS app (electron-vite, pnpm)
  - Projects list in the sidebar and on the dashboard (add an existing folder or create one in the dialog;
    remove from the list; flags missing folders)
  - Profiles: `default` (~/.claude) plus `~/.claude-profiles/<name>`; add, remove (to the Recycle Bin),
    signed-in status, a Sign in terminal
  - Opening a project runs `claude.exe` in an embedded terminal with the chosen profile.
    Several tabs per project, each tagged with its account.
  - Settings: claude.exe path override, editor command (default `zed`)
- **UI moved to shadcn/ui (radix-lyra style)**: Sidebar, Card, Table, Field, Empty, Alert,
  AlertDialog confirmations, sonner toasts, Phosphor icons. Tailwind v4, dark only.
- **Installer**: `pnpm dist` builds `dist/cc-launcher-Setup-<ver>.exe` (NSIS, per-user, one click,
  desktop + Start menu shortcuts). Smoke-tested: the packaged app runs Claude Code in its terminal.
- **Auto-update code**: electron-updater checks at startup and every 4 h, downloads in the background,
  then shows a "Restart" toast (it also installs on quit). Settings shows the version and has a
  "Check for updates" button.

- **Private GitHub updates** (branch `feat/private-updates`): publish target is a private repo.
  The app asks for a read-only fine-grained token in Settings, stores it encrypted (safeStorage/DPAPI),
  and passes it straight to electron-updater (not via env vars). Checked in the packaged app:
  "needs token" state and token format validation. See `docs/RELEASING.md`.

- **v0.2.0** (branch `feat/titlebar-accounts-usage`)
  - Custom title bar: frameless window (`titleBarStyle: hidden` + native min/max/close overlay),
    full-height sidebar with the app icon, breadcrumb bar on the main side.
  - App icon: `build/icon.svg` → `pnpm icon` → `build/icon.png` (installer/exe) + `resources/icon.png` (window).
  - Accounts: the `default` (~/.claude) profile is gone. Every account is its own folder in
    ~/.claude-profiles, so `claude` used elsewhere can't change which account a profile is.
  - Plan usage (5-hour + weekly) per account, from Claude Code's documented status-line
    `rate_limits` data. The app installs `cc-launcher-statusline.ps1` into each profile and sets
    `statusLine` in its settings.json, unless one is already set. Shown as meters on Accounts and as
    "5h x% · wk y%" on the launch buttons. Refreshed every 20 s. Tested the script under PowerShell and
    Git Bash with sample data.

- **GitHub**: private repo DarkElfes/cc-launcher. `main` is pushed. Release **v0.2.0** is published with
  the installer, blockmap and latest.yml (checked against the installer's size and sha512).
  `pnpm release` now builds first, then publishes with `gh release create`.

- **0.3.1**: each account shows its **email and plan** (`claude auth status --json`), plus **5-hour and
  weekly limits with reset times** from `claude -p "/usage" --output-format json`. That runs locally as
  a slash command: 0 turns, $0, and doesn't touch the limits. Both run at start, every 5 min, after a
  session ends, and on the Refresh button. The status line still gives live numbers during sessions;
  whichever is newer wins. Checked against both real profiles; the numbers match the Claude app.
  Note: the /usage text format isn't a documented API, so the parser (`accountInfo.ts`) is defensive.

- **0.4.0**: voice dictation without typing commands. Settings has **Claude language** (replies + dictation)
  and **Voice dictation** (tap / hold / off), written into every profile's settings.json (`language`,
  `voice`) by `profileSettings.ts`. In tap mode the app also binds Alt+K (`meta+k` → `voice:pushToTalk`) in
  each profile's keybindings.json. The **Dictate** button in the tab bar and **Ctrl+Space** send that key
  to the active Claude terminal. Tested in a fake home: files written correctly, button shown,
  Ctrl+Space caught. **Not yet tested with a real microphone**, and hold mode inside the app's
  terminal is untested (tap is the recommended mode).

- **0.5.0 (local build, not released)**: session panel and attention signals.
  - `hookServer.ts`: Claude Code **HTTP hooks** go to a local server (127.0.0.1, random port, per-run secret
    passed as CCL_HOOK_TOKEN; tab id as CCL_TAB_ID). `hooksConfig.ts` writes them into every profile
    (our entries end in `/ccl-hook`; the user's own hooks are kept).
  - `sessions.ts`: per-tab state from hooks (activity, plan, tasks, subagents, background bash, files
    written) and from the status line's per-session file (model, context, API-price cost, lines, time).
  - Right **session panel** (toggle in the title bar): Session, Plan, Tasks, Background, Outputs,
    Changes (git), Connectors (`claude mcp list`, on demand, cached 5 min), History with Resume
    (`claude --resume <id>`).
  - **Signals** on tabs and on sidebar projects: needs-you (amber, pulsing), working (spinner), finished
    and not seen (green), error (red). Seen/idle tabs show nothing. A **Windows notification** fires when
    Claude needs you, finishes or fails in a tab you're not looking at; clicking it opens that tab.
  - The account button turns red at ≥90% of either limit.
  - Tested in a real session (darkness, scratch folder): starting → working → done, Outputs, context
    44k/1M, cost $0.13, history entry. **Not yet tested live:** needs-you/permission prompts, Windows
    notifications, plan, tasks, subagents, connectors, Resume.

- **0.6.0 (local build, not released)**: **Hand off** button in the session panel.
  - Summary mode (default): sends `HANDOFF_PROMPT` to the current tab (Claude updates and commits
    docs/STATUS.md). When that turn finishes, opens a new tab on the chosen account with
    `CONTINUE_PROMPT` as its first message (`claude "<prompt>"`). If Claude is busy, it waits for it
    first. "Open now" / "Cancel" on the toast.
  - Full mode: the other account resumes the same conversation file (`claude --resume <path>`; the path
    must be inside ~/.claude-profiles). Verified across profiles. It costs more: the whole conversation
    is sent again.
  - The target list picks the signed-in account with the most room left by default.
  - Tested end-to-end darkness → tree in a scratch folder. Note: even this tiny test used ~3% of tree's
    weekly limit (each new session sends ~45k tokens of system prompt/context).

- **0.7.0 (local build, not released)**: images for Claude, like in the chat app.
  - **Ctrl+V** with an image on the clipboard: main saves it to `%APPDATA%\cc-launcher\pasted-images\*.png`
    (pruned after 14 days) and the path is pasted via xterm's bracketed paste. Claude Code turns it into
    an `[Image #N]` attachment. Ctrl+V with text pastes text as before.
  - **Drag & drop** files onto the terminal pastes their paths (`webUtils.getPathForFile`).
  - **Image** button in the tab bar: file picker for png/jpg/gif/webp.
  - Tested: clipboard screenshot → `[Image #1]` in a real darkness session (not sent).

- **0.8.0 (local build, not released)**: **message box** under the Claude terminal (`composer.tsx`):
  normal text editing, Enter sends, Shift+Enter adds a new line, ↑/↓ recalls sent messages, per-tab drafts,
  images by Ctrl+V / drag & drop / button shown as chips. Sending pastes images one at a time and then
  the text via bracketed paste (newlines kept, paths become [Image #N]), then Enter. Sending is blocked
  while Claude waits for an answer in the terminal (permission/menus). Tested: a 2-line message arrives as one prompt.
  - `pnpm release` now creates a **draft**, uploads each file with 3 retries, then publishes. v0.7.0's
    installer upload had broken and left a draft; it was fixed by hand.

## In progress
- Nothing.

- **0.3.0**: the repo is public, so updates need no token. All token code is removed, and old saved
  tokens are deleted on startup. Repo and history were audited before going public: no secrets or
  credential files, and only the commit author email.

## Next
- Owner: install 0.3.0 by hand once (0.2.0 expects a private repo). Then release 0.3.1 to confirm an
  installed copy updates itself.
- M2: git info on project cards and in the project view; "New project" with templates; STATUS.md
  viewer/editor.

## Known problems
- No code-signing certificate, so Windows SmartScreen warns on first install ("More info → Run anyway").
  Auto-update still works without signing.
- Usage only updates while a session of that account runs (after its first reply). It can't be
  fetched on demand without using the login token, which we don't do. Not yet seen with a real account.
- Usage needs workspace trust to be accepted in the folder (the status line is blank before that).
- If the app is killed (not closed), running `claude` children may be left behind. A normal close kills them.
- Not yet tested by hand: real sign-in flow, Ctrl+V paste, images pasted into Claude.
