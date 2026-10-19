# Builds the release zip: dist/<name>-v<version>-windows-x64.zip (plus a .sha256 file).
#
#   ./scripts/package.ps1            build in release mode, then package
#   ./scripts/package.ps1 -NoBuild   package what's already in target/release
#
# The zip is everything the meter needs: the program, the game data (downloaded fresh
# from its GPL-3.0 sources by packet-lab, with that licence beside it), the docs and the
# licences. Npcap isn't included; its licence doesn't allow that, and users install it once.
#
#   -GameData <folder>   use an existing game data folder instead of downloading one

param([switch]$NoBuild, [string]$GameData)

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

Copy-Item (Join-Path $Root 'target/release/glass-cannon.exe') $stage
Copy-Item README.md, LICENSE, THIRD_PARTY.md $stage

# Game data: skill, class and mob names and icons, next to the exe where the meter looks.
$data = Join-Path $stage 'gamedata'
if ($GameData) {
  Copy-Item -Recurse $GameData $data
} else {
  & (Join-Path $Root 'target/release/packet-lab.exe') update-data --out $data
  if ($LASTEXITCODE -ne 0) { throw 'downloading the game data failed' }
}
foreach ($f in 'skills.json', 'npcs.json') {
  if (-not (Test-Path (Join-Path $data $f))) { throw "game data is missing $f" }
}
Copy-Item (Join-Path $Root 'third_party/GPL-3.0.txt') (Join-Path $data 'LICENSE-GPL-3.0.txt')
$skills = Get-Content -Raw (Join-Path $data 'skills.json') | ConvertFrom-Json
$npcs = Get-Content -Raw (Join-Path $data 'npcs.json') | ConvertFrom-Json
@(
  'Game data for Glass Cannon: skill, class and mob names and icons.'
  ''
  'This folder is not covered by Glass Cannon''s MIT licence. It comes from two GPL-3.0'
  'projects and stays under that licence (LICENSE-GPL-3.0.txt). The names and artwork'
  'themselves are from Aion 2 and belong to NCSOFT.'
  ''
  "- NOIA2, https://github.com/ZDYoung0519/NOIA2, commit $($skills.source.commit)"
  '  skill names and icons, class icons, boss lists'
  "- RATmeter, https://github.com/Kuroukihime/AIon2-Dps-Meter, commit $($npcs.source.mob_names.commit)"
  '  English mob names'
  ''
  'The files are the projects'' data as published, reformatted into skills.json and'
  'npcs.json by packet-lab update-data (in the Glass Cannon repository).'
) | Set-Content -Encoding utf8 (Join-Path $data 'SOURCES.txt')

# Every Rust crate built into the meter, with its licence.
$lines = cargo tree -p glass-cannon -e normal,build --prefix none --locked --format '{p}  {l}  {r}' |
  ForEach-Object { $_ -replace ' \(\*\)$', '' } |
  Where-Object { $_ -and $_ -notmatch '^(glass-cannon|meter-core) ' } |
  Sort-Object -Unique
if ($LASTEXITCODE -ne 0) { throw 'cargo tree failed' }
@(
  "Rust crates used by $Name v$version (name, version, licence, source)."
  'Generated from Cargo.lock by scripts/package.ps1; some are only used at build time.'
  ''
) + $lines | Set-Content -Encoding utf8 (Join-Path $stage 'THIRD-PARTY-CRATES.txt')

$zip = Join-Path $dist "$stem.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
# Entries are added one by one with forward slashes: Windows PowerShell 5.1's zip
# helpers write backslash paths, which other unzip tools mishandle.
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$base = (Resolve-Path $stage).Path.TrimEnd('\') + '\'
$archive = [System.IO.Compression.ZipFile]::Open($zip, 'Create')
try {
  Get-ChildItem -Recurse -File $stage | Sort-Object FullName | ForEach-Object {
    $entry = $_.FullName.Substring($base.Length).Replace('\', '/')
    [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $_.FullName, $entry, 'Optimal')
  }
} finally {
  $archive.Dispose()
}
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
# One LF-terminated line, so `sha256sum -c` accepts it.
[System.IO.File]::WriteAllText("$zip.sha256", "$hash  $stem.zip`n")

Write-Host "packaged $zip"
Write-Host "sha256 $hash"
