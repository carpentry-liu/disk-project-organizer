$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all
    cargo build --release
    $dist = Join-Path $projectRoot 'dist'
    New-Item -ItemType Directory -Path $dist -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\disk-project-organizer.exe') -Destination (Join-Path $dist 'disk-project-organizer.exe') -Force
    Write-Host "Built: $dist\disk-project-organizer.exe"
}
finally {
    Pop-Location
}
