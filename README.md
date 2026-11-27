# cc-launcher

A Windows desktop app for working with [Claude Code](https://code.claude.com) across several of
**your own** Claude accounts. It is organized around projects, not accounts: you open a project and
pick the account to run Claude with. Conversations and project memory are shared by all accounts,
so switching accounts keeps the context.

The app runs the official `claude` CLI. It never calls Claude APIs itself, and it never reads or
stores your login credentials.

Native Rust app built on [GPUI](https://www.gpui.rs) (via gpui-kit / gpui-component). Versions up to
0.9 were Electron, and that code is still in git history.

## Features

- **Projects**: a dashboard of your project folders. The project view shows git changes, past
  sessions, connectors (MCP) and the plan / task list of the running session.
- **Accounts (profiles)**: each account lives in its own `~/.claude-profiles/<name>` folder
  (`CLAUDE_CONFIG_DIR`) with its own login. The app shows email, plan and 5-hour / weekly usage, and
  warns when an account is close to its limit.
- **Two tab modes**:
  - **Chat**: the app's own chat view on top of `claude -p` (stream-json). It shows Markdown, tool
    cards with diffs, and permission, question and plan cards. You can stop and continue a turn,
    pick the permission mode (auto by default, like the CLI) and the model.
  - **Terminal**: the real Claude Code TUI in an embedded terminal (ConPTY).
- **Shared sessions**: all accounts use one session folder per project, linked with Windows
  junctions. Any account can continue any conversation. The account buttons continue the latest
  session or start a new one. Sessions can be deleted from History; they go to the Recycle Bin.
- **Handoff**: pass work to another account, as a STATUS.md summary or as the full conversation.
- **Message box**: drafts per tab, history (Ctrl+↑/↓), screenshots and copied files with Ctrl+V,
  drag & drop, and voice dictation through Claude Code's own `/voice`.
- **Notifications**: Windows notifications when Claude finishes or needs you. Click one to open
  the tab.
- **Auto-update** from this repo's GitHub releases. Tabs come back after a restart.

## Install

1. Download `cc-launcher.exe` from the
   [latest release](https://github.com/DarkElfes/cc-launcher/releases/latest). It is one portable
   exe, no installer.
2. Put it anywhere, for example `%LOCALAPPDATA%\Programs\cc-launcher\`, and pin it or make a
   shortcut. SmartScreen warns because the exe isn't signed: **More info → Run anyway**.
3. You need [Claude Code](https://code.claude.com) installed (`claude` on PATH, or set its path in
   Settings).
4. Add an account under **Accounts** and sign in. Login happens inside the official CLI.

The app keeps its data in `%APPDATA%\cc-launcher\` (`app.json`: projects and settings, never
credentials). Accounts live in `%USERPROFILE%\.claude-profiles\`.

## Build from source

Needs Rust (stable, MSVC toolchain) and Visual Studio 2022 Build Tools with the C++ workload.
Windows Smart App Control blocks Rust build scripts, so it must be off.

| Command | What it does |
| --- | --- |
| `cargo run` | debug build, then start the app |
| `cargo check` | fast type check |
| `cargo test` | unit tests |
| `cargo build --release` | `target\release\cc-launcher.exe` |
| `.\scripts\release.ps1` | release build plus GitHub release `v<version>` (see [docs/RELEASING.md](docs/RELEASING.md)) |

`CCL_DEBUG_HOOKS=1` prints every Claude Code hook the app receives to stderr.

## Docs

- [CLAUDE.md](CLAUDE.md): architecture, conventions and hard rules
- [docs/STATUS.md](docs/STATUS.md): current state and what has been tested
- [docs/PLAN.md](docs/PLAN.md): the original plan
- [docs/RELEASING.md](docs/RELEASING.md): releases and auto-update

## Rules

- For your own accounts only. No sharing accounts, no selling access.
- The app only checks whether `<profile>/.credentials.json` exists. It never reads, copies, logs or
  sends credentials.
- It launches only the official `claude` executable.

No license file has been added yet.
