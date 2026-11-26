# cc-launcher

A Windows desktop app built around projects for working with Claude Code across several of the
owner's own Claude accounts. Projects keep their state in the repo. Accounts are "profiles", each
with its own `CLAUDE_CONFIG_DIR`. The app runs the official `claude.exe` in embedded terminals or in
its own chat view.

Plan: `docs/PLAN.md`. Current state: `docs/STATUS.md`. **Update STATUS.md at the end of every session.**

Since 0.10.0 the app is native Rust on [GPUI](https://www.gpui.rs) (through `gpui-kit` /
gpui-component). The Electron/React version lives in git history (≤ 0.9.0).

## Commands (cargo)

| Command | What it does |
| --- | --- |
| `cargo run` | build (debug) and start the app |
| `cargo check` | type-check quickly |
| `cargo build --release` | optimized build → `target/release/cc-launcher.exe` (one portable exe) |
| `.\scripts\release.ps1` | release build + GitHub release `v<version>` with `cc-launcher.exe` (auto-update source) |

Set `CCL_DEBUG_HOOKS=1` to print every received Claude Code hook to stderr (debug builds show a console).

Releasing: see `docs/RELEASING.md`. **The repo is public**, and updates come from its GitHub releases
without a token. Never commit secrets: tokens, `.credentials.json`, `.claude.json`, profile folders.

## Architecture

```
src/
  main.rs              entry: clears inherited Claude env vars, single instance, opens the window
  backend/             everything that isn't drawing; background threads → BackendEvent channel
    mod.rs             BackendEvent (hooks, chat events, terminal output/exit, accounts, updates)
    types.rs           shared data types, JSON helpers, write_atomic
    store.rs           %APPDATA%\cc-launcher\app.json (projects + settings, never credentials)
    profiles.rs        profile folders (~/.claude-profiles/<name>), env for claude
    terminal.rs        ConPTY + VT parser (alacritty_terminal); one Terminal per terminal tab
    chat.rs            Chat mode: `claude -p` stream-json process per chat tab; PermissionRequest
                       responders; transcript → earlier messages on resume
    hook_server.rs     local HTTP endpoint for Claude Code hooks (127.0.0.1, per-run token); holds the
                       response for PermissionRequest until the UI answers
    hooks_config.rs    writes our http hooks into each profile's settings.json (url ends /ccl-hook)
    sessions.rs        SessionTracker: per-tab live state from hooks + status-line session files
    usage.rs           per-profile status-line script that records rate_limits (5h/weekly usage)
    profile_settings.rs language/voice choices → each profile (settings.json, keybindings.json)
    account_info.rs    email/plan via `claude auth status --json`, limits via `claude -p "/usage"`
    git.rs / history.rs / mcp.rs   panel data: git status, past sessions, `claude mcp list`
    updater.rs         GitHub releases check + download; self-replace on restart (release builds)
    platform.rs        Windows toasts, clipboard images, Recycle Bin, single instance
    util.rs            hidden-console process helpers, open/editor
  ui/
    app.rs             LauncherApp: all state + actions (tabs, projects, profiles, handoff, events)
    mod.rs             Render: sidebar, title bar, tab strip, layout, handoff dialog
    views.rs           Dashboard, project header, Accounts, Settings
    panel.rs           session side panel
    terminal_view.rs   draws a terminal grid, keys → bytes, text input handler, selection, scroll
    chat_store.rs      stream-json events → chat items
    chat_view.rs       chat rendering: Markdown, tool cards, permission / question cards
    composer.rs        message box under Claude tabs (drafts, history, image attachments)
    theme.rs           colors (dark only) and small shared elements
build.rs               embeds build/icon.png as the exe icon
```

Stack: Rust 2024, gpui-kit 0.11 (GPUI + gpui-component), alacritty_terminal, tiny_http, ureq,
self-replace, tauri-winrt-notification, arboard.

## Conventions

- Background work never blocks the UI thread: run it on `cx.background_executor()` or a thread and
  report back through `BackendEvent` or `this.update(cx, …)`.
- Errors shown to the user go through `show_error` (a notification). Destructive actions confirm with
  `window.prompt(...)`.
- Colors come from `ui/theme.rs`; profile colors from `profile_color()`.
- Profile names: `^[a-z0-9][a-z0-9_-]{0,31}$`. Profiles live only in `~/.claude-profiles/<name>`.
  Never use the shared `~/.claude` folder as a profile.
- Files the app writes into a profile are left alone when they aren't valid JSON.
- Icon: edit `build/icon.svg`, export `build/icon.png` (256×256 or larger).

## Hard rules

- Only the owner's own accounts. No sharing, no selling access.
- **Never read, copy, log or send credentials.** The app may only check whether
  `<profile>/.credentials.json` exists. Login happens inside the official `claude` CLI.
- Launch only the official `claude` executable. Don't call Claude APIs directly.
- Session transcripts (`<profile>/projects/...jsonl`) use an internal format that can change.
  Read them defensively and only for display.

## Git

One branch or worktree per task (`feat/...`, `fix/...`). Commit often. `main` should always build.
