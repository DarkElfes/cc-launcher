# Releasing and auto-update

Updates are hosted as **releases of the private GitHub repo** `DarkElfes/cc-launcher`
(`publish` in `electron-builder.yml`).

| Who | How it authenticates | Access |
| --- | --- | --- |
| **You, publishing** (`pnpm release`) | The signed-in GitHub CLI (`gh auth login`) | Your account |
| **Installed apps, downloading updates** | A read-only token **built into each release** from `CCL_UPDATE_TOKEN`. A token pasted in Settings overrides it | This repo only, Contents: **Read-only** |

### About the built-in token
- It comes from the `CCL_UPDATE_TOKEN` variable of the terminal that runs `pnpm release`. It is
  never in a source file, in git, or in a permanent environment variable. `out/` and `dist/` are gitignored.
- Anyone who has the installer could extract it. That's acceptable because the installers only live
  in this private repo's releases, and the token can only *read* this one repo. Never use a token with
  more access here.
- **When it expires, installed apps stop updating.** Before that, create a new token and release a new
  version with it. If it already expired, paste a fresh token in Settings, or install the latest
  release by hand.
- The app never passes the token to the renderer or to the `claude` processes it starts.

## One-time setup
1. `gh auth login` (already done on this PC).
2. Create the **update token**: GitHub → Settings → Developer settings → Personal access tokens →
   Fine-grained tokens → Generate new token → Only select repositories: `cc-launcher` →
   Repository permissions → Contents: **Read-only** → the longest expiration offered.
   Keep it in a password manager.

## Each release
1. Bump `version` in `package.json` (e.g. 0.2.1 → 0.2.2). Commit and push.
2. In a terminal in the project folder:
   ```powershell
   $env:CCL_UPDATE_TOKEN = Read-Host 'Update token'   # paste the read-only token; not saved in history
   pnpm release
   ```
   `scripts/release.mjs` refuses to run without the token. It builds, checks that the token got
   embedded and that `dist/latest.yml` matches the version, then creates GitHub release `v<ver>` with
   `cc-launcher-Setup-<ver>.exe`, `.blockmap` and `latest.yml` in one `gh release create`. It won't
   overwrite an existing release.
   (electron-builder's own publishing was dropped: its parallel uploads raced on creating the release
   and once left v0.2.0 without its installer.)
3. Installed apps check at startup and every 4 h, download in the background, then show
   "Restart". Quitting also installs the update.

## First install on a PC
Download `cc-launcher-Setup-<ver>.exe` from the latest release and run it. SmartScreen warns
because the app isn't signed: More info → Run anyway. Releases from 0.2.1 on have the update token
built in, so there's nothing to set up.
