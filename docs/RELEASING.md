# Releasing and auto-update

The repo `DarkElfes/cc-launcher` is **public**, so installed apps download updates from its GitHub
releases without any token. Publishing uses the signed-in GitHub CLI (`gh auth login`).

Nothing secret is in the repo or the app. Claude logins live only in `~/.claude-profiles/<name>`
on each PC, and `.gitignore` blocks credential files. Keep it that way: never commit tokens,
`.credentials.json`, `.claude.json` or profile folders.

## Each release
1. Bump `version` in `package.json` (e.g. 0.3.0 → 0.3.1). Commit and push.
2. In the project folder: `pnpm release` (`scripts/release.mjs`). It builds with
   `--publish never`, checks that `dist/latest.yml` matches the version, then creates GitHub release
   `v<ver>` with `cc-launcher-Setup-<ver>.exe`, `.blockmap` and `latest.yml` in one
   `gh release create`. It won't overwrite an existing release.
   (electron-builder's own publishing was dropped: its parallel uploads raced on creating the release
   and once left v0.2.0 without its installer.)
3. Installed apps check at startup and every 4 h, download in the background, then show
   "Restart". Quitting also installs the update.

## First install on a PC
Download `cc-launcher-Setup-<ver>.exe` from the latest release and run it. SmartScreen warns
because the app isn't signed: More info → Run anyway. Updates are automatic from then on.

Versions before 0.3.0 expected a private repo and a token; install 0.3.0 or later by hand once.
