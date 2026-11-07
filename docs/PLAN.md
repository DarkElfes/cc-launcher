# cc-launcher — plan (draft)

A Windows desktop app built around **projects**, not accounts. Each project
keeps its own state (git, CLAUDE.md, docs/STATUS.md), and you can open it in
the official Claude Code CLI with any of your own Claude accounts.

## Core idea

```
Project  ──(open with)──►  Account profile  ──►  official `claude` CLI in an embedded terminal
   │                         (own CLAUDE_CONFIG_DIR, own login, own history)
   └─ state lives in the repo: CLAUDE.md, docs/STATUS.md, git branches/worktrees
```

- Accounts are interchangeable "engines". The project is what persists.
- Chat history stays with each account. Continuity between accounts comes from
  the repo files (STATUS.md + git), not from copying history.
- The app never reads, copies or sends login tokens. Logging in happens inside
  the official `claude` CLI.

## Tech stack

- **Electron + React + TypeScript**: the app shell
- **shadcn/ui (radix-lyra style) + Tailwind v4**: UI components
- **xterm.js + node-pty**: the embedded terminal that runs the real `claude.exe`
  (VS Code's terminal uses the same pair)
- **simple-git**: reads branches, worktrees and status
- App data (project list, profile list, settings) in `%APPDATA%\cc-launcher\app.json`
- Profile config folders in `%USERPROFILE%\.claude-profiles\<name>\`

## Screens

1. **Projects dashboard**: cards for every project, showing name, path, git
   branch, uncommitted changes, last commit, the "Next" item from STATUS.md,
   and which account was last used.
2. **Project view**
   - Tabs with embedded Claude Code terminals (several at once, each tagged
     with its account)
   - An account switcher: "Continue with ▸ work / personal / …"
   - Side panel: STATUS.md (rendered and editable), CLAUDE.md, git branches
     and worktrees, recent commits
   - Sessions list for this project, per account (read-only, from that
     profile's local session files), with "resume" running `claude --resume`
     under the same account
3. **Accounts**: add or remove a profile, sign in (opens `claude` in that
   profile so you log in there), signed-in status (credentials file exists:
   yes/no), and an optional "limit reached until …" marker
4. **Settings**: path to `claude`, default account, shared settings to copy
   into every profile (settings.json, global CLAUDE.md, agents; never
   credentials)

## Features by milestone

### M1: skeleton (first usable version)
- [x] Electron app shell, projects list (add existing folder / create new)
- [x] Account profiles: create, list, sign-in status
- [x] Open project → embedded terminal running `claude` with the chosen account
- [x] Switch account for the open project (closes or keeps the old tab, opens a new one)

### M2: project state
- [ ] Git info on cards and in the project view (branch, dirty, last commit)
- [ ] Create a new project with git init + CLAUDE.md + docs/STATUS.md templates
- [ ] STATUS.md viewer/editor; dashboard shows "Next" and "Known problems"
- [ ] Remember the last account used per project

### M3: smooth switching
- [ ] **Handoff button**: asks the current Claude session to update STATUS.md
      and commit, then opens the same project under another account with a
      starting prompt such as "Read docs/STATUS.md and continue"
- [ ] Detect the "usage limit reached" message in the terminal → offer to
      switch account and mark the account as limited until the reset time
- [ ] Per-account session list + resume

### M4: worktrees and polish
- [ ] One worktree per task: create, list, open a worktree in its own tab
- [ ] Tray icon with recent projects and quick launch
- [ ] `claude` version check and a warning if `CLAUDE_CONFIG_DIR` behaviour changes
- [x] Installer (.exe) via electron-builder
- [x] Auto-update via electron-updater (code done; needs a publish target, e.g. GitHub Releases)
- [ ] App icon and code signing

## Rules (from the handoff)
- Only your own accounts. No sharing, no selling access.
- Credentials stay in their profile folders. The app never reads or copies tokens.
- Only official Claude apps and Claude Code. This app just launches the
  official `claude.exe`.

## Open questions
- ~~`CLAUDE_CONFIG_DIR`: check against the current docs before M1.~~ Confirmed: it's documented.
- The Desktop app can't use profiles (it has one account per sign-in), so
  this app works with Claude Code CLI only.
