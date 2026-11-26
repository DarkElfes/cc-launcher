# Releasing and auto-update

The repo `DarkElfes/cc-launcher` is **public**, so the app downloads updates from its GitHub releases
without any token. Publishing uses the signed-in GitHub CLI (`gh auth login`).

Nothing secret is in the repo or the app. Claude logins live only in `~/.claude-profiles/<name>`
on each PC, and `.gitignore` blocks credential files. Keep it that way: never commit tokens,
`.credentials.json`, `.claude.json` or profile folders.

## Each release
1. Bump `version` in `Cargo.toml` (e.g. 0.10.0 → 0.10.1). Commit and push.
2. In the project folder: `.\scripts\release.ps1`. It runs `cargo build --release` and creates GitHub
   release `v<ver>` with `cc-launcher.exe`. It won't overwrite an existing release.
3. Running apps check at startup and every 4 h. A newer `cc-launcher.exe` downloads in the background
   to `%APPDATA%\cc-launcher\update\`; "Restart" (banner or Settings) swaps it in and starts it.

## First install on a PC
Download `cc-launcher.exe` from the latest release and put it anywhere you like (for example
`%LOCALAPPDATA%\Programs\cc-launcher\`), then pin it or make a shortcut. SmartScreen warns because the
app isn't signed: More info → Run anyway.

Projects and settings are kept in `%APPDATA%\cc-launcher\app.json`, the same file the Electron
versions used, so nothing is lost when switching.

Electron versions (≤ 0.9.0) can't update to the Rust app by themselves: their updater expects an
NSIS installer and `latest.yml`. Install the new exe by hand once, then uninstall the old app from
Windows Settings → Apps.
