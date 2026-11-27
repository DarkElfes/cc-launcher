# Builds cc-launcher.exe and publishes it as GitHub release v<version> (the auto-update source).
# Needs the GitHub CLI signed in (gh auth login). Never overwrites an existing release.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

$version = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$tag = "v$version"

gh release view $tag *> $null
if ($LASTEXITCODE -eq 0) { throw "Release $tag already exists. Bump the version in Cargo.toml first." }

cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$exe = 'target\release\cc-launcher.exe'
# Tag the commit that was built. Without --target GitHub tags the default branch (main).
$commit = (git rev-parse HEAD).Trim()
git fetch -q origin
if (-not (git branch -r --contains $commit)) { throw 'Push this commit first; the release tag points at it.' }
gh release create $tag $exe --target $commit --title "cc-launcher $version" --notes "cc-launcher $version"
if ($LASTEXITCODE -ne 0) { throw 'gh release create failed' }
Write-Output "Published $tag"
