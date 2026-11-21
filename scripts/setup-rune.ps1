# Fetches the pinned Rune revision the app builds against. Rune is pre-release
# and needs its vendored dependency patches, so the checkout must be sparse
# (crates + vendor only) rather than the full repository.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $root "vendor\rune"
$rev = "b39e1b4d326e323e971b86b8f105d3df53d4642c"

if (Test-Path (Join-Path $dir "Cargo.toml")) {
    Write-Output "vendor/rune is already present"
    exit 0
}
New-Item -ItemType Directory -Force (Split-Path $dir) | Out-Null
git init $dir
git -C $dir remote add origin https://github.com/anomalyco/rune
git -C $dir sparse-checkout init --cone
git -C $dir sparse-checkout set crates vendor
git -C $dir fetch --depth 1 origin $rev
git -C $dir checkout FETCH_HEAD
Write-Output "vendor/rune at $rev"
