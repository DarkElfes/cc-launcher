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

## In progress
- Nothing.

## Next
- Pick where updates are hosted (GitHub Releases is the plan) → fill in `publish` in
  `electron-builder.yml` → `pnpm release`. Until then, installed builds show "auto-update is off".
- M2: git info on project cards and in the project view; "New project" with templates; STATUS.md
  viewer/editor.

## Known problems
- No code-signing certificate, so Windows SmartScreen warns on first install ("More info → Run anyway").
  Auto-update still works without signing.
- Default Electron icon. Add `build/icon.ico` (256×256) to fix.
- If the app is killed (not closed), running `claude` children may be left behind. A normal close kills them.
- Not yet tested by hand: real sign-in flow, Ctrl+V paste, images pasted into Claude.
