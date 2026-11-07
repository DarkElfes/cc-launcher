# Releasing and auto-update

Updates are hosted as **releases of the private GitHub repo** `DarkElfes/cc-launcher`
(`publish` in `electron-builder.yml`).

| Who | How it authenticates | Access |
| --- | --- | --- |
| **You, publishing** (`pnpm release`) | The signed-in GitHub CLI (`gh auth login`). Its token is handed to electron-builder for that one command only | Your account |
| **Installed apps, downloading updates** | A fine-grained token pasted into the app (Settings → GitHub update token) on each PC | This repo only, Contents: **Read-only** |

No token goes into the repo or the installer, or into a permanent environment variable. A
permanent `GH_TOKEN` would be visible to every program, including the Claude Code sessions the
app starts. The app stores the update token encrypted with Windows DPAPI (Electron `safeStorage`)
in `%APPDATA%\cc-launcher\update-token.bin`, and never passes it to the renderer or to `claude`.

## One-time setup
1. `gh auth login` (already done on this PC).
2. Create the **update token**: GitHub → Settings → Developer settings → Personal access tokens →
   Fine-grained tokens → Generate new token → Only select repositories: `cc-launcher` →
   Repository permissions → Contents: **Read-only**. Pick a long expiration; when it expires,
   updates stop until you paste in a new one.

## Each release
1. Bump `version` in `package.json` (e.g. 0.2.0 → 0.2.1). Commit and push.
2. In the project folder:
   ```powershell
   $env:GH_TOKEN = gh auth token; pnpm release; Remove-Item Env:GH_TOKEN
   ```
   This builds the installer and uploads `cc-launcher-Setup-<ver>.exe`, `.blockmap` and `latest.yml`
   to a GitHub release `v<ver>`.
3. Installed apps check at startup and every 4 h, download in the background, then show
   "Restart". Quitting also installs the update.

## First install on a PC
Download `cc-launcher-Setup-<ver>.exe` from the latest release and run it. SmartScreen warns
because the app isn't signed: More info → Run anyway. Then go to Settings → paste the read-only
update token → Save.
