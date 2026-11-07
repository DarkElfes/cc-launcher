# Releasing and auto-update

Updates are hosted as **releases of the private GitHub repo** `GITHUB_USER/cc-launcher`
(set `owner` in `electron-builder.yml`). Two separate tokens are used:

| Token | Where it lives | Permissions |
| --- | --- | --- |
| **Publish token** | `GH_TOKEN` user environment variable on the PC you release from | Fine-grained, this repo only: Contents **Read and write** |
| **Update token** | Entered in the app (Settings → GitHub update token) on each PC | Fine-grained, this repo only: Contents **Read-only** |

Neither token goes into the repo or the installer. The app stores the update token encrypted
with Windows DPAPI (Electron `safeStorage`) in `%APPDATA%\cc-launcher\update-token.bin`, and never
passes it to the renderer or the `claude` processes it starts.

## One-time setup
1. Create a **private** repo `cc-launcher` on GitHub, then push:
   `git remote add origin https://github.com/GITHUB_USER/cc-launcher.git` → `git push -u origin main`
2. Set `owner:` in `electron-builder.yml` to your GitHub username.
3. GitHub → Settings → Developer settings → Fine-grained tokens → create the two tokens above
   (Repository access: Only select repositories → cc-launcher).
4. Save the publish token for your user (new terminals pick it up):
   `[Environment]::SetEnvironmentVariable('GH_TOKEN', '<publish token>', 'User')`

## Each release
1. Bump `version` in `package.json` (e.g. 0.1.0 → 0.2.0). Commit.
2. `pnpm release`: builds the installer and uploads `cc-launcher-Setup-<ver>.exe`, `.blockmap`
   and `latest.yml` to a GitHub release `v<ver>`.
3. Installed apps check at startup and every 4 h, download in the background, then show
   "Restart". Quitting also installs the update.

## First install on a PC
Run `cc-launcher-Setup-<ver>.exe` (download it from the release). SmartScreen warns because the app
isn't signed: More info → Run anyway. Then go to Settings → paste the read-only update token → Save.
