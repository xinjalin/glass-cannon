# Builds the release zip: dist/<name>-v<version>-windows-x64.zip (plus a .sha256 file).
#
#   ./scripts/package.ps1            build in release mode, then package
#   ./scripts/package.ps1 -NoBuild   package what's already in target/release
#
# The zip holds the two programs, the docs and the licences. Game data is not included:
# its sources are GPL-3.0 projects, so the meter downloads it on first start instead.
# Npcap isn't included either; its licence doesn't allow that, and users install it once.

param([switch]$NoBuild)

$ErrorActionPreference = 'Stop'
$Name = 'glass-cannon'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$meta = cargo metadata --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$version = ($meta.packages | Where-Object { $_.name -eq 'glass-cannon' }).version

if (-not $NoBuild) {
  cargo build --release --locked --workspace
  if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}

$stem = "$Name-v$version-windows-x64"
$dist = Join-Path $Root 'dist'
$stage = Join-Path $dist $stem
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

foreach ($exe in 'glass-cannon.exe', 'packet-lab.exe') {
  Copy-Item (Join-Path $Root "target/release/$exe") $stage
}
Copy-Item README.md, LICENSE, THIRD_PARTY.md $stage

# Every Rust crate compiled into the programs, with its licence.
$ours = 'glass-cannon', 'meter-core', 'packet-lab'
$lines = $meta.packages |
  Where-Object { $ours -notcontains $_.name } |
  Sort-Object name, version |
  ForEach-Object { '{0} {1}  {2}  {3}' -f $_.name, $_.version, $_.license, $_.repository }
@(
  "Rust crates used by $Name v$version (name, version, licence, source)."
  'Generated from Cargo.lock by scripts/package.ps1; some are only used at build time.'
  ''
) + $lines | Set-Content -Encoding utf8 (Join-Path $stage 'THIRD-PARTY-CRATES.txt')

$zip = Join-Path $dist "$stem.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
"$hash  $stem.zip" | Set-Content -Encoding ascii "$zip.sha256"

Write-Host "packaged $zip"
Write-Host "sha256 $hash"
