# Changelog

All notable changes to **DLSS 5 Studio** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.0.1] - 2026-09-14

### Fixed
- **DLSS 5 Feeder Neural Rendering**: Restored `NeuralUplift=1` in `ReShade.ini` for the DLSS 5 Feeder route. Resolves an issue where Neural Reconstruction (Feature 18) was initialized as disabled at startup, enabling seamless DLSS-NR execution across non-DX12 titles (e.g. DirectX 11 executables like `bg3_dx11.exe`).
- **Setup Defaults**: Defaulted installation path to `C:\DLSS 5 Studio` and data directory to `C:\DLSS 5 Studio\data` for friction-free un-elevated installs on standard Windows user accounts, with dynamic warning banners when protected system directories (`Program Files`) are selected.

### Changed
- **OptiScaler Attribution**: Updated README documentation and repository links to accurately credit `wilsjo2/OptiScaler-DLSSNR-PreSR-Multipass` for the Pre-SR Multipass and DLSS-NR runtime implementation.
- **Publish Scripting**: Added `-NoTag` switch to release automation tooling for seamless non-release documentation synchronization.

---

## [1.0.0] - 2026-09-13

### Added
- **Pure Rust Native Architecture**: Ground-up lightweight implementation using Dioxus v0.6 and direct Win32 APIs (~20 MB idle RAM, <200ms cold startup, 5.9 MB standalone binary).
- **4x Multi-Frame Generation Unlock**: Ada Lovelace frame generation multiplier unlocking 3x and 4x multipliers on GeForce RTX 40-Series GPUs via RenoDX Streamline hook companion bridge.
- **OptiScaler DLSS-NR & Pre-SR Multipass**: Integrated multi-pass neural reconstruction (1x/2x/3x passes) and universal RTXMFG proxy injection (`version.dll`).
- **Flexible Rendering Backends**:
  - ReShade Native DLSS (RenoDX) for DirectX 12 games.
  - DLSS 5 Feeder route for non-DLSS or non-DX12 titles (DirectX 11, Vulkan, OpenGL).
  - OptiScaler DLSS-NR for direct neural reconstruction.
- **Universal Multi-Store Game Scanner**: Automatic library scanning across Steam, Xbox Game Pass / Windows Store, Epic Games Store, and GOG Galaxy.
- **Atomic Journaling & Hot-Swapping**: One-click cross-route swapping with vanilla backup continuity (`_DLSS5_Backup/originals/`) and untracked mod cleaning.
- **Running Game Guard & Anti-Cheat Protection**: Win32 process inspection preventing file operations while games are running, with EasyAntiCheat, BattlEye, and Vanguard detection.
- **Complete Localization**: Full dynamic translation support across 14 languages with RTL support for Arabic.
- **Standalone Setup Installer & Portable Binary**: Dedicated WiX v4 installer and portable executable releases.
