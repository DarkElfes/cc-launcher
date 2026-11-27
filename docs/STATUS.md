# Status

_Last updated: 2026-11-27_

## 0.10.0: rewrite from Electron to Rust + GPUI (branch `gpui-rewrite`)
- The whole app is now native Rust on GPUI (`gpui-kit` 0.11 / gpui-component). All Electron, React,
  Vite, pnpm and electron-builder files are removed; see CLAUDE.md for the new layout.
- Same data file (`%APPDATA%\cc-launcher\app.json`) and same profile folders, hooks, status-line
  script and keybindings, so existing projects and accounts carry over unchanged.
- Ported: projects, accounts (email/plan/limits), settings (language, voice), terminal tabs
  (ConPTY + alacritty_terminal, own renderer: colors, bold/italic, selection, right-click copy/paste,
  scrollback, bracketed paste, text input incl. Cyrillic), chat tabs (stream-json, Markdown via
  gpui-component TextView, tool cards with diffs, permission / plan / question cards, permission
  modes, stop/resume), message box (drafts per tab, Ctrl+↑/↓ history, screenshot paste, drag & drop,
  image picker), session panel (session, plan, tasks, background, outputs, git changes, connectors,
  history + resume), handoff to another account (summary via STATUS.md or full conversation),
  Windows toasts with click-to-open, single instance, auto-update from GitHub releases (self-replace).
- Distribution is one portable `cc-launcher.exe` (icon embedded); no installer any more.
- Checked by hand on this PC: app starts with the old projects; project view shows accounts with
  limits, git changes (Cyrillic file names fixed with core.quotepath=false) and history; chat tab
  opens; terminal tab runs Claude Code 2.1.332 and renders it; typing Latin and Cyrillic works;
  status-line data (model, context) reaches the panel; closing the app ends its claude processes.
- Checked by hand on 2026-11-26 (debug build, darkess, scratch project): chat send + Done footer and
  panel stats; Write permission card → Allow → file written; Stop → "Stopped" → next message
  continues with the partial answer remembered; AskUserQuestion card → answer reaches Claude; plan
  card → Approve → mode back to Ask → Write card → file written; terminal tab: workspace trust,
  message box send, UserPromptSubmit/Stop hooks, green "finished" dot on the project; closing the app
  leaves no claude processes.
- Fixed during that check:
  - Stop now sends a stream-json `interrupt` control request (as the Agent SDK does) instead of
    killing the process, so the partial answer stays in the session; it falls back to ending the
    process after 5 s. No extra "Done 0.1 s" after resuming, and the panel leaves "working".
  - Chat processes run with `--permission-prompt-tool stdio`; without it `claude -p` has no
    AskUserQuestion / ExitPlanMode tools, so question and plan cards never appeared. The hook still
    shows the card; the stdio request gets the same answer (first answer wins).
  - The chat view follows a streaming reply while you're at the bottom.
  - Status-line script reads stdin as UTF-8 (Cyrillic session titles were garbled).
  - Resume / refresh buttons in the panel used glyphs that aren't icons (empty squares).
- **Not yet checked by hand:** Windows toasts (couldn't see them in screenshots), handoff, updater
  (needs a release), sign-in flow, mic/dictation, image paste / attach.
- **Ported from Electron 0.9.1–0.9.2 + look (2026-11-26, second PC):**
  - Open tabs come back after a restart or update: `openTabs` in app.json (same shape as Electron
    0.9.1), saved every 3 s when changed, on close and before an update; reopened with `--resume`,
    chat tabs with their earlier messages, the active tab selected. Bad entries are skipped, not fatal.
  - Inter (static TTFs in `assets/fonts`, SIL OFL, embedded) for the UI, chat and Markdown;
    Cascadia Mono for code. Claude's own app uses Anthropic Sans, which can't be redistributed.
  - Chat message box is centered with the message column (`theme::CHAT_COLUMN` = 820 px, same
    padding); help "?" moves into its control row. Terminal tabs keep the full-width box.
  - Softer palette: background #1c1c1c instead of near-black, text #d4d4d4 instead of near-white;
    gpui-component widgets (inputs, Markdown, menus) get the same colors and fonts
    (`theme::apply_component_theme`). Colored cards (permission/question/plan) unchanged for now.
  - Single line breaks in replies are kept (hard breaks outside code fences and tables).
  - Window position/size/maximized is remembered (`settings.window`); ignored if off-screen or tiny.
  - Checked by hand: tab restore (chat tab + history + project view), Inter and colors, message box
    alignment, window placement after close → reopen, no claude processes left after closing.
    **Not checked by hand:** line breaks in a live reply, tab restore of a terminal tab, restore
    after an actual update.
- **Settings, icons, chat dictation (2026-11-26, later):**
  - Claude language is a dropdown (gpui-component Select). "Default" replaces "Don't change": on Save
    it removes `language` from every account (Claude answers in the language you write in;
    dictation expects English). Voice dictation's first option reads "Default (as each account has it)".
  - All emoji/text glyphs in the UI replaced with Phosphor outline icons (dashboard folder, Chat /
    Terminal toggle, launch play buttons, account folder/trash, Refresh, Hand off, panel carets, task
    status, background agents, git branch, hand-off radios). Added `circle.svg`, `radio-button.svg`.
  - Chat-tab dictation goes through a **hidden interactive Claude Code** per account
    (`backend/dictation.rs`, `ui/dictation.rs`). `claude -p` has no prompt and no `/voice`, and the
    app must not call Anthropic's voice service itself (it would need the account's credentials), so:
    mic / Ctrl+Space starts `claude --settings {voice: hold, autoSubmit: false}` in a ConPTY (no
    window, no hook token), clears its prompt, sends Alt+K (`voice:pushToTalk`) every 33 ms like a
    held key, reads the prompt line off the screen (`❯ …` between the last two `────` rules) into the
    message box live; mic again stops the repeats, waits for the text to settle, clears the hidden
    prompt. Nothing is ever submitted. Sessions are reused per account, closed after 5 idle minutes
    and on exit. Trust prompt / not signed in → a clear error. Windows voice typing (Win+H, an
    earlier attempt) didn't support Ukrainian and was removed.
  - Checked: the hidden session starts, recording starts ("listening…" in its footer; an
    unspoken run gets "No speech detected" from the service), the app shows "Listening…", stop
    ends cleanly, no claude processes left after closing. **Not checked** (needs speaking): actual
    words arriving in the message box, Ukrainian recognition.
  - Debug helpers: `cc-launcher --dump-claude-screen <claude.exe> <cwd> <config dir> [secs]
    [text|hold:N]` prints the hidden screen (check the prompt layout after a Claude Code update);
    debug builds with `CCL_DEBUG_DICTATE=<secs>` start dictation in the active tab and stop it.
  - **Dictation fixes (2026-11-27):** the user saw old words come back. Two causes: sending
    with Enter didn't stop the mic, and the hidden prompt kept the old text. Now: Send ends the
    run (`dictation_on_send`). Ending clears the hidden prompt now and again 1/2/3 s later, because
    the final transcript can arrive late. Start keeps clearing until the prompt is empty (up to 2 s)
    and ignores any leftover as a prefix. Editing the box while dictating (e.g. clearing it)
    rebases: the user's text is kept and only new words are added. Not hand-tested yet.
  - **Thinking in chat history (2026-11-27):** Claude Code transcripts keep thinking blocks
    without text (`"thinking": ""` + signature), so a reopened chat had no thoughts. The app now saves
    live thinking to `%APPDATA%\cc-launcher\thinking.jsonl` (key: message id + block position;
    capped at 20 MB) and fills it in on resume (`backend/thinking_cache.rs`). Older sessions show
    "Thought (text not saved by Claude Code)". Not hand-tested yet.
  - **Permission mode + dropdowns (2026-11-27):** chat tabs started without `--permission-mode`,
    and `claude -p` then runs in Manual, while the interactive CLI's built-in default is auto. So the
    button read "Ask permissions" even though nobody picked it, and a pick was lost on a new tab or a
    restart. Now the pick is saved as `settings.permissionMode` and used for new and restored tabs
    (the VS Code extension also reuses the last pick). Nothing picked = `auto`, as in the
    interactive CLI; the "Default" option is gone. When Claude Code reports a different mode (auto not available → Manual, or a plan
    was approved), the button shows that mode. The mode and model menus close when you click outside
    them (`on_mouse_down_out`). A click on the menu's own button doesn't open it again. Not hand-tested yet.
  - Chat history check (2026-11-27): loading a real transcript gave 111 events → 71 chat items, so
    resume works. A **new** chat tab is a new session and starts empty, like `claude` without
    `--continue`. Earlier sessions open through the panel's History list (play button) or come back
    on restart.
  - **Shared sessions across accounts (2026-11-27):** Claude Code keeps a project's sessions and
    auto memory in `<config dir>/projects/<dir>/`, so each account had its own history. Now
    `backend/shared_sessions.rs` keeps the real folder in `~/.claude-profiles/_shared/projects/<dir>/`
    and makes every profile's folder a directory junction to it (`junction` crate, no admin needed).
    It runs when a project tab opens and no other tab of that project is open. Existing folders are
    merged first: renames only, nothing deleted; a clash with different content is kept as
    `<name>.from-<profile>.bak`. History lists each session once (no account tag for shared ones); ▶
    shows "Open with [account]…" and you pick the account. Project header: **Last session / New session** chips
    (`settings.continueLast`, default Last). An account button continues the newest shared session
    in that account, or shows its tab if it's already open.
    Checked with real claude 2.1.x in a temp project: a session written through the junction lands in
    `_shared`, and `--resume <id>` from the other account finds it and appends to the **same file**.
    The answer itself hit that account's 5-hour limit. Unit test covers merge and link. **Not hand-tested
    in the app yet.** The first run moves the existing cc-launcher sessions of `darkness` into `_shared`.
  - History rows have a trash button: the session (.jsonl + its `<id>/` folder) goes to the Recycle
    Bin after a confirm; not offered for a session open in a tab. Project header: Explorer / Editor /
    Remove are icon buttons with hover hints; Remove only takes the project off the list (files and
    sessions stay). Not hand-tested yet.
  - Ctrl+V in the message box also attaches **files copied in Explorer** (clipboard file list; before,
    only bitmap screenshots worked; checked with a copied PNG and a bitmap) and works with the
    Ukrainian layout (`м`). Confirmations (delete session, close a running tab, remove project,
    remove profile) use the app's own gpui-component AlertDialog (`LauncherApp::confirm`) instead of
    Windows message boxes. Removing a profile first deletes its junctions, so the Recycle Bin never
    touches shared sessions. Dialog look not hand-checked yet.
  - Memory (release, Task Manager "Memory" = private working set): ~60 MB in a normal window, ~80 MB
    maximized at 2880×1800, ~105 MB with a chat tab maximized; the build before these changes measured
    the same (60 / 95 MB). Debug builds are about the same plus a console window.
- Build note: Windows Smart App Control blocks Rust build scripts (os error 4551); it must be off
  to build on a PC. Needs rustup (stable-msvc) + VS 2022 Build Tools (C++ workload).
- Testing note: "cc-launcher" in the Start menu is still the old Electron 0.9.0 install; run the
  Rust build with `cargo run` (or target\debug\cc-launcher.exe), not from the Start menu.
- Note: SessionStart supports only command hooks, so a new tab shows "ready" once its status line
  runs, and "working"/"finished" come from the other hooks.

## Earlier (Electron, ≤ 0.9.0)
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

- **0.8.1 (local build, not released)**: attached images in the message box show a real **thumbnail**
  (main makes a ≤240px data URL with `nativeImage`; click opens the file, × removes it). Pasted screenshots are
  labelled "Screenshot <time>" instead of the generated file name. The placeholder is gone; an **ⓘ** tooltip
  next to the box lists everything it can do.

- **0.9.0 (local build, not released)**: **Chat mode**, like the Claude app. Tabs open as Chat (default) or
  Terminal (switch next to the account buttons, saved as `settings.tabMode`).
  - `main/chat.ts`: one `claude -p --input-format stream-json --output-format stream-json --verbose
    --include-partial-messages` process per chat tab, using documented flags only. Images go as base64
    content blocks. **Stop** ends the process; the next message resumes with `--resume <session>`. Changing
    the permission mode restarts it the same way.
  - **Permissions** via the documented `PermissionRequest` HTTP hook (timeout 600 s): for chat tabs the hook
    waits for Allow / Always allow (Claude's `permission_suggestions`) / Deny (+ reason). AskUserQuestion →
    option buttons (answers via `updatedInput`); ExitPlanMode → "Approve plan / Keep planning". Terminal
    tabs get no decision, so their normal prompt shows.
  - `renderer/chat/store.ts` turns events into items: user bubbles (with thumbnails), streaming Markdown
    text, thinking, tool cards (diffs for Edit, commands, outputs, subagent step counts), permission cards,
    notices, and "Done · time · API-price".
  - Resumed sessions show earlier messages from the transcript (best effort; internal format).
  - Session panel stats for chat tabs come from stream events (context from `usage`, cost from `result`).
  - Not using the Agent SDK: its docs disallow claude.ai login in third-party products. The CLI in -p
    mode is Claude Code itself.
  - Tested end-to-end (darkness): Write needed approval → card → Allow → file created, Markdown bold,
    footer, panel stats, no page errors. **Not yet tested:** AskUserQuestion, plan approval, Stop/resume,
    images in chat, subagents.

## In progress
- Branch `gpui-rewrite`: hand-testing 0.10.0 (see the 0.10.0 section above for what's checked and
  fixed). Left to check: Windows toasts, handoff, image paste / attach, sign-in. Then merge into main
  and release. Test project used: `D:\Projects\Temp\cc-launcher-temp` (on the desktop PC).

- **0.3.0**: the repo is public, so updates need no token. All token code is removed, and old saved
  tokens are deleted on startup. Repo and history were audited before going public: no secrets or
  credential files, and only the commit author email.

## Next
- Check the remaining items of 0.10.0 (toasts, handoff, images, sign-in), then merge `gpui-rewrite`
  into main.
- Release 0.10.0 (`.\scripts\release.ps1`), install the exe by hand once (Electron builds can't
  update to it), then release 0.10.1 to confirm self-update.
- Maybe: an installer (Start menu shortcut) instead of the portable exe.
- M2: git info on project cards and in the project view; "New project" with templates; STATUS.md
  viewer/editor.

## Known problems
- No code-signing certificate, so Windows SmartScreen warns on first run ("More info → Run anyway").
  Auto-update still works without signing.
- GPUI rewrite: emoji glyphs in a few buttons (mic) fall back to other symbols; IME composition
  (marked text) in the terminal isn't shown while composing, only the committed text is sent.
- Usage only updates while a session of that account runs (after its first reply). It can't be
  fetched on demand without using the login token, which we don't do. Not yet seen with a real account.
- Usage needs workspace trust to be accepted in the folder (the status line is blank before that).
- If the app is killed (not closed), running `claude` children may be left behind. A normal close kills them.
- Not yet tested by hand: real sign-in flow, Ctrl+V paste, images pasted into Claude.
