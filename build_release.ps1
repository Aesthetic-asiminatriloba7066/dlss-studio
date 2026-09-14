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

Write-Host "1. Building Main Application Binary..." -ForegroundColor Yellow
cargo build --release --bin dlss-studio
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo release build failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

$mainExe = Join-Path $PSScriptRoot "target\release\dlss-studio.exe"
$portableExe = Join-Path $PSScriptRoot "target\release\dlss-studio-portable.exe"
Copy-Item $mainExe $portableExe -Force
Write-Host "Created portable binary: dlss-studio-portable.exe ($([math]::Round((Get-Item $portableExe).Length / 1MB, 2)) MB)" -ForegroundColor Green

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host " Building Native Setup Executable       " -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

cargo build --release --bin dlss-studio-setup
if ($LASTEXITCODE -ne 0) {
    Write-Error "Native setup binary build failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

$setupExe = Join-Path $PSScriptRoot "target\release\dlss-studio-setup.exe"
$setupItem = Get-Item $setupExe
$setupMb = [math]::Round($setupItem.Length / 1MB, 2)

# Post-build cleanup: Remove everything in target\release except the two required deliverables
Write-Host "`n3. Cleaning up target\release (keeping only portable and setup executables)..." -ForegroundColor Yellow
$releaseDir = Join-Path $PSScriptRoot "target\release"
Get-ChildItem -Path $releaseDir -Force | Where-Object {
    $_.Name -ne "dlss-studio-portable.exe" -and $_.Name -ne "dlss-studio-setup.exe"
} | ForEach-Object {
    Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Removed: $($_.Name)" -ForegroundColor DarkGray
}

Write-Host "`n========================================================" -ForegroundColor Green
Write-Host " [SUCCESS] Clean Deliverables Ready                    " -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Green
Write-Host "Setup Wizard: $setupExe ($setupMb MB)" -ForegroundColor Cyan
Write-Host "Portable Exe: $portableExe ($([math]::Round((Get-Item $portableExe).Length / 1MB, 2)) MB)" -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Green



