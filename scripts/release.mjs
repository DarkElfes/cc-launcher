// Builds the installer, then publishes it as a GitHub release with the signed-in GitHub CLI.
// Run: pnpm release   (needs `gh auth login` and CCL_UPDATE_TOKEN set in this terminal; see docs/RELEASING.md)
//
// electron-builder's own GitHub publishing uploads files in parallel and can race on creating
// the release ("tag_name already_exists"), leaving a release without its installer. Building
// with --publish never and uploading everything in one `gh release create` avoids that.
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'

const run = (cmd, args) => execFileSync(cmd, args, { stdio: 'inherit', shell: process.platform === 'win32' })

const { version } = JSON.parse(readFileSync('package.json', 'utf8'))
const tag = `v${version}`
const files = [`dist/cc-launcher-Setup-${version}.exe`, `dist/cc-launcher-Setup-${version}.exe.blockmap`, 'dist/latest.yml']

// Installed apps need the built-in read-only token to download this update without asking.
const token = process.env.CCL_UPDATE_TOKEN?.trim() ?? ''
if (!/^(github_pat_|ghp_)[A-Za-z0-9_]+$/.test(token)) {
  console.error('CCL_UPDATE_TOKEN is not set (or is not a GitHub token) in this terminal.')
  console.error("Run:  $env:CCL_UPDATE_TOKEN = Read-Host 'Update token'   then pnpm release again.")
  process.exit(1)
}

// Refuse to overwrite an existing release: bump the version instead.
try {
  execFileSync('gh', ['release', 'view', tag], { stdio: 'ignore', shell: process.platform === 'win32' })
  console.error(`Release ${tag} already exists. Bump "version" in package.json first.`)
  process.exit(1)
} catch {
  // not found: good
}

run('pnpm', ['exec', 'electron-vite', 'build'])
// Check the token actually made it into the bundle (without printing it).
if (!readFileSync('out/main/index.js', 'utf8').includes(token)) throw new Error('Update token was not embedded in the build')
run('pnpm', ['exec', 'electron-builder', '--win', '--publish', 'never'])

for (const f of files) if (!existsSync(f)) throw new Error(`Missing build output: ${f}`)
const latest = readFileSync('dist/latest.yml', 'utf8')
if (!latest.includes(`version: ${version}`)) throw new Error('dist/latest.yml does not match this version')

run('gh', ['release', 'create', tag, ...files, '--title', tag, '--generate-notes'])
console.log(`Published ${tag}. Installed apps will pick it up within 4 hours (or via Check for updates).`)
