# build-release.ps1 — Build RxTerm release artifacts (MSI + EXE installer)
# Usage: .\build-release.ps1 -Version 0.2.0

param(
    [Parameter(Mandatory=$true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Write-Host "=== RxTerm Release Build v$Version ===" -ForegroundColor Cyan

# Update version in tauri.conf.json and package.json
$tauriConf = Join-Path $PSScriptRoot "src-tauri\tauri.conf.json"
$pkgJson   = Join-Path $PSScriptRoot "package.json"

$tauri = Get-Content $tauriConf -Raw | ConvertFrom-Json
$tauri.version = $Version
$tauri | ConvertTo-Json -Depth 10 | Set-Content $tauriConf -Encoding UTF8

$pkg = Get-Content $pkgJson -Raw | ConvertFrom-Json
$pkg.version = $Version
$pkg | ConvertTo-Json -Depth 10 | Set-Content $pkgJson -Encoding UTF8

Write-Host "  Updated version to $Version in tauri.conf.json and package.json" -ForegroundColor Gray

# Verify prerequisites
foreach ($cmd in @('node', 'npm', 'cargo', 'rustc')) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Write-Host "ERROR: '$cmd' not found in PATH." -ForegroundColor Red
        exit 1
    }
}

# Install frontend dependencies
Write-Host "`n[1/3] Installing frontend dependencies..." -ForegroundColor Yellow
npm ci
if ($LASTEXITCODE -ne 0) { Write-Host "npm ci failed" -ForegroundColor Red; exit 1 }

# Build frontend
Write-Host "`n[2/3] Building frontend..." -ForegroundColor Yellow
npm run build
if ($LASTEXITCODE -ne 0) { Write-Host "Frontend build failed" -ForegroundColor Red; exit 1 }

# Build Tauri (release mode)
Write-Host "`n[3/3] Building Tauri release bundle..." -ForegroundColor Yellow
npx tauri build
if ($LASTEXITCODE -ne 0) { Write-Host "Tauri build failed" -ForegroundColor Red; exit 1 }

# Locate artifacts and copy to versioned release folder
$bundleDir = Join-Path $PSScriptRoot "src-tauri\target\release\bundle"
$releaseDir = Join-Path $PSScriptRoot "release\v$Version"

if (-not (Test-Path $releaseDir)) {
    New-Item -ItemType Directory -Path $releaseDir | Out-Null
}

$msi = Get-ChildItem -Path "$bundleDir\msi\*.msi" -ErrorAction SilentlyContinue | Select-Object -First 1
$nsis = Get-ChildItem -Path "$bundleDir\nsis\*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1

$copied = 0
if ($msi) {
    $destName = "RxTerm_${Version}_x64.msi"
    Copy-Item -Path $msi.FullName -Destination (Join-Path $releaseDir $destName) -Force
    $copied++
    Write-Host "  MSI:  $(Join-Path $releaseDir $destName)" -ForegroundColor Cyan
}
if ($nsis) {
    $destName = "RxTerm_${Version}_x64-setup.exe"
    Copy-Item -Path $nsis.FullName -Destination (Join-Path $releaseDir $destName) -Force
    $copied++
    Write-Host "  EXE:  $(Join-Path $releaseDir $destName)" -ForegroundColor Cyan
}

Write-Host "`n=== Build Complete ===" -ForegroundColor Green
if ($copied -gt 0) {
    Write-Host "  Artifacts copied to: $releaseDir" -ForegroundColor Cyan
} else {
    Write-Host "  No artifacts found in: $bundleDir" -ForegroundColor Yellow
}
