// Builds the installer, then publishes it as a GitHub release with the signed-in GitHub CLI.
// Run: pnpm release   (needs `gh auth login`; see docs/RELEASING.md)
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

// Refuse to overwrite an existing release: bump the version instead. (A leftover draft from a failed
// run also counts: finish it with `gh release upload` + `gh release edit --draft=false`, or delete it.)
try {
  execFileSync('gh', ['release', 'view', tag], { stdio: 'ignore', shell: process.platform === 'win32' })
  console.error(`Release ${tag} already exists. Bump "version" in package.json first.`)
  process.exit(1)
} catch {
  // not found: good
}

run('pnpm', ['exec', 'electron-vite', 'build'])
// makensis sometimes fails once on Windows ("Error - aborting creation process"), likely a file briefly
// locked by antivirus. One retry has always been enough.
try {
  run('pnpm', ['exec', 'electron-builder', '--win', '--publish', 'never'])
} catch {
  console.warn('electron-builder failed; retrying once...')
  run('pnpm', ['exec', 'electron-builder', '--win', '--publish', 'never'])
}

for (const f of files) if (!existsSync(f)) throw new Error(`Missing build output: ${f}`)
const latest = readFileSync('dist/latest.yml', 'utf8')
if (!latest.includes(`version: ${version}`)) throw new Error('dist/latest.yml does not match this version')

// Create as a draft first, upload with retries (big uploads sometimes break), then publish. Installed
// apps only see the release once it's published with all files.
run('gh', ['release', 'create', tag, '--draft', '--title', tag, '--generate-notes'])
for (const f of files) {
  for (let attempt = 1; ; attempt++) {
    try {
      run('gh', ['release', 'upload', tag, f, '--clobber'])
      break
    } catch (e) {
      if (attempt >= 3) throw e
      console.warn(`Upload of ${f} failed (attempt ${attempt}); retrying...`)
    }
  }
}
run('gh', ['release', 'edit', tag, '--draft=false', '--latest'])
console.log(`Published ${tag}. Installed apps will pick it up within 4 hours (or via Check for updates).`)
