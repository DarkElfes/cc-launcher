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

## In progress
- Nothing.

- **0.2.1 (not released yet)**: the read-only update token is built in at release time from
  `CCL_UPDATE_TOKEN` (never in git), so installs need no setup. A token saved in Settings overrides it.
  `pnpm release` requires the variable and checks that the token got embedded. Tested with a dummy value.

## Next
- Owner: in a terminal, set `CCL_UPDATE_TOKEN` (Read-Host) and run `pnpm release` to publish 0.2.1.
  0.2.0 has no token, so install 0.2.1 by hand once (or paste the token in 0.2.0's Settings to test
  auto-update). Then release 0.2.2 to confirm an installed copy updates itself.
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
