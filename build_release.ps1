<#
.SYNOPSIS
    Builds the standalone portable executable and pure native Rust setup installer for DLSS 5 Studio.
#>

$ErrorActionPreference = "Stop"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host " Building DLSS 5 Studio (Release)       " -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Ensure any running background instance is closed so binary can be overwritten
Stop-Process -Name "dlss-studio" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "dlss-studio-portable" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "dlss-studio-setup" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "dlss5-swapper-rust" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "dlss5-swapper-rust-portable" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "dlss5-swapper-setup" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 200

# Extract version dynamically from Cargo.toml
$cargoToml = Get-Content (Join-Path $PSScriptRoot "Cargo.toml") -Raw
if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
    $version = $matches[1]
} else {
    $version = "1.0.0"
}
$versionedPortableName = "dlss-studio-v$version-portable.exe"
$versionedSetupName = "dlss-studio-v$version-setup.exe"
$versionedPortableExe = Join-Path $PSScriptRoot "target\release\$versionedPortableName"
$versionedSetupExe = Join-Path $PSScriptRoot "target\release\$versionedSetupName"

Write-Host "Target Version: v$version" -ForegroundColor Cyan
Write-Host "Deliverables: $versionedSetupName | $versionedPortableName`n" -ForegroundColor Cyan

Write-Host "1. Building Main Application Binary..." -ForegroundColor Yellow
cargo build --release --bin dlss-studio
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo release build failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

$mainExe = Join-Path $PSScriptRoot "target\release\dlss-studio.exe"
Copy-Item $mainExe $versionedPortableExe -Force
Write-Host "Created portable binary: $versionedPortableName ($([math]::Round((Get-Item $versionedPortableExe).Length / 1MB, 2)) MB)" -ForegroundColor Green

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host " Building Native Setup Executable       " -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

cargo build --release --bin dlss-studio-setup
if ($LASTEXITCODE -ne 0) {
    Write-Error "Native setup binary build failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

$setupExe = Join-Path $PSScriptRoot "target\release\dlss-studio-setup.exe"
Copy-Item $setupExe $versionedSetupExe -Force
$setupItem = Get-Item $versionedSetupExe
$setupMb = [math]::Round($setupItem.Length / 1MB, 2)

# Post-build cleanup: Remove everything in target\release except the two versioned deliverables
Write-Host "`n3. Cleaning up target\release (keeping only versioned deliverables)..." -ForegroundColor Yellow
$releaseDir = Join-Path $PSScriptRoot "target\release"
$keepFiles = @(
    $versionedSetupName,
    $versionedPortableName
)
Get-ChildItem -Path $releaseDir -Force | Where-Object {
    $_.Name -notin $keepFiles
} | ForEach-Object {
    Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Removed: $($_.Name)" -ForegroundColor DarkGray
}

Write-Host "`n========================================================" -ForegroundColor Green
Write-Host " [SUCCESS] Clean Deliverables Ready (v$version)        " -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Green
Write-Host "Setup Wizard: $versionedSetupExe ($setupMb MB)" -ForegroundColor Cyan
Write-Host "Portable Exe: $versionedPortableExe ($([math]::Round((Get-Item $versionedPortableExe).Length / 1MB, 2)) MB)" -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Green



