# DLSS 5 STUDIO ⚡ v1.0

> **A blisteringly fast, low-memory utility built in pure native Rust to enable and unlock DLSS, Neural Reconstruction, and 4x Frame Generation across your PC games while preserving pristine graphical fidelity.**
>
> _Supports all **GeForce RTX GPUs (20, 30, and 40-Series)** for DLSS upscaling and OptiScaler Pre-SR, with **exclusive 4x Multi-Frame Generation unlocking for RTX 40-Series cards**._

[![Version](https://img.shields.io/badge/version-1.0-orange.svg)](#)
[![Platform](<https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20(64--bit)-blue.svg>)](#)
[![Language](https://img.shields.io/badge/language-100%25%20Pure%20Rust-red.svg)](#)
[![i18n](https://img.shields.io/badge/i18n-14%20Languages-yellow.svg)](#)
[![Memory](https://img.shields.io/badge/RAM%20usage-~20%20MB-green.svg)](#)
[![Binary Size](https://img.shields.io/badge/Portable%20Exe-5.9%20MB-success.svg)](#)
[![License](https://img.shields.io/badge/license-MIT-purple.svg)](https://github.com/bookamp/dlss-studio/blob/main/LICENSE)

**DLSS 5 STUDIO** is a ground-up pure Rust desktop utility inspired by the sleek UI design and layout of the original [DLSS 5 Swapper](https://github.com/rakanki911/DLSS5-Swapper). Engineered from scratch for extreme speed and minimal resource usage, it enables gamers to inject and upgrade modern DLSS features, Neural Reconstruction, and OptiScaler Pre-SR across their game libraries—with **3x/4x Multi-Frame Generation unlocked specifically for GeForce RTX 40-Series GPUs**.

Built with [Dioxus](https://dioxuslabs.com/) and direct Win32 APIs, it eliminates heavy web-wrapper and Electron stacks—launching in under 200ms and consuming under 20 MB of RAM.

<p align="center">
  <img src="assets/preview-dashboard.jpg" alt="DLSS 5 Studio Dashboard Overview" width="850">
</p>

---

## 🌟 Key Features

### 1. ⚡ 4x Multi-Frame Generation (MFG) Unlock

- **Bypass RTX 50-Series Driver Locks**: NVIDIA officially restricts 3x and 4x Multi-Frame Generation in drivers to RTX 50-Series hardware. DLSS 5 STUDIO unlocks 3x and 4x multipliers on GeForce RTX 40-Series GPUs.
- **Native DLSS-G Interception**: Leverages RenoDX hook add-ons (`renodx-mfgunlock.addon64`) to intercept Streamline Frame Generation contracts on games with native Frame Generation code (`sl.dlss_g.dll`, `nvngx_dlssg.dll`).
- **Honest Hardware & API Gating**: Automatically detects whether a game's engine has native Frame Generation or only DLSS Super Resolution (e.g. _Baldur's Gate 3_), and strictly gates MFG availability on DirectX 11 executables (`bg3_dx11.exe`) where Streamline Frame Generation is unsupported.

<p align="center">
  <img src="assets/preview-cyberpunk-mfg.png" alt="Cyberpunk 2077 4x Multi-Frame Generation Unlock" width="620">
</p>

### 2. 🔬 OptiScaler DLSS-NR & Pre-SR Multipass

- **OptiScaler Neural Reconstruction Pipeline**: Route graphics through OptiScaler's open-source multi-vendor wrapper (`nvngx.dll` / `dxgi.dll`).
- **Pre-SR Multipass Clarity**: Enables multi-pass neural reconstruction (`1x`, `2x`, or `3x` passes) for dramatic clarity, sharpness, and temporal stability enhancements.
- **Universal RTXMFG Integration**: Pairs OptiScaler with standalone proxy hooks (`version.dll`) to allow simultaneous Frame Generation and Pre-SR multipass enhancements in supported 64-bit games.

<p align="center">
  <img src="assets/preview-bg3-presr.png" alt="Baldur's Gate 3 OptiScaler Pre-SR Multipass Clarity" width="620">
</p>

### 3. 🎯 Flexible Rendering Backends & Routes

- **ReShade Backend**:
  - **`Native DLSS (RenoDX)`**: For DirectX 12 games with native DLSS pipelines. Hooks into D3D12 NGX vtables and enables 4x MFG unlock.
  - **`DLSS 5 Feeder`**: Dedicated frame interception route for non-DLSS titles or games running on DirectX 11, Vulkan, OpenGL, or wrapper runtimes (`dlss5-feed.addon64`, `DLSS5_Feed.fx`, `vort_Motion.fx`).
- **OptiScaler Backend**:
  - **`OptiScaler DLSS-NR`**: Full neural reconstruction with Pre-SR multipass. Automatically restricted on titles lacking native depth and motion vectors.
- **Seamless Cross-Route Hot-Swapping**: Switch freely between ReShade (Native/Feeder) and OptiScaler with a single click. DLSS 5 STUDIO automatically unregisters Vulkan implicit layers, removes conflicting proxy DLLs, and deploys the new payload while carrying forward the original vanilla game backups.

### 4. 🚀 Universal Multi-Store Game Scanner

Scans and organizes your games automatically without manual configuration:

- **Steam**: Resolves library roots from `SteamPath` registry and `libraryfolders.vdf`, parses `appmanifest_<id>.acf`, and downloads official 600x900 vertical box art from Steam CDN.
- **Xbox Game Pass / Microsoft Store**: Queries `GamingServices` package repository and scans `XboxGames` drive roots. Parses GDK `MicrosoftGame.config` and `AppxManifest.xml` to bypass launcher wrappers (`gamelaunchhelper.exe`), resolves authentic 64-bit executables, and extracts high-resolution logos directly from package assets.
- **Epic Games Store**: Discovers installed titles by parsing `%PROGRAMDATA%\Epic\...\Manifests\*.item` manifests.
- **GOG Galaxy**: Inspects `GOG.com\Games` registry trees and `goggame-*.info` playtasks.
- **Custom Folders & Manual Executables**: Add any custom game folder or executable with custom title renaming and persistent artwork caching.

### 5. 🛡️ Bulletproof Backup, Rollback & Process Safety

- **Atomic Rollback Journals**: Every modification automatically creates a snapshot in `_DLSS5_Backup/originals/` before touching any game files.
- **Vanilla Backup Continuity**: Switching between routes carries forward the genuine unmodded game files through arbitrary successive swaps.
- **Restore Originals**: Restores authentic vanilla binaries with a single click and archives the backup manifest.
- **Clean Untracked Mods**: Purges leftover proxy DLLs (`dxgi.dll`, `OptiScaler.dll`, `ReShade64.dll`, `.addon64`) up to 4 directory levels deep without risking original game files.
- **Process Guarding**: Inspects running processes via native Win32 `Toolhelp32` snapshots, blocking mod deployment or restoration if the game is running.
- **Anti-Cheat Detection**: Detects EasyAntiCheat, BattlEye, and Vanguard, warning you before touching protected titles.

### 6. 🌍 Complete Multilingual Localization (14 Languages)

- **14 Supported Languages**: English, Deutsch (German), Español (Spanish), Français (French), Italiano (Italian), Português (Portuguese), Русский (Russian), 简体中文 (Simplified Chinese), 日本語 (Japanese), 한국어 (Korean), Polski (Polish), Türkçe (Turkish), العربية (Arabic with full RTL layout support), and हिन्दी (Hindi).
- **Reactive Dynamic Switching**: Instantly switch languages anytime from the header selector or Settings. All views, sheets, specs, action badges, and tooltips update in real-time with zero app restart.
- **Structured Activity Logging Engine**: Activity log entries use tokenized templates (`@{key|...}`), allowing the in-app terminal to dynamically translate logs into the selected language while keeping on-disk diagnostics (`dlss-studio.log`) in standard English for seamless GitHub issue reporting.
- **Localized History & Tooltips**: Fully translated modification history tables, dynamic change counts (`0 replaced, 5 added`), action badges, and localized play button tooltips (`Launch {game}`).

### 7. 🪶 100% Pure Rust Performance

| Metric                   | Traditional Web / Electron Apps | **DLSS 5 STUDIO**            | Advantage                      |
| :----------------------- | :------------------------------ | :--------------------------- | :----------------------------- |
| **Idle Memory (RAM)**    | 350 MB – 600 MB                 | **~20 MB**                   | **95% less RAM**               |
| **Executable Size**      | 120 MB – 250 MB                 | **5.9 MB**                   | **97% smaller**                |
| **Startup Time**         | 2.5s – 6.0s                     | **< 200ms**                  | **Instantaneous**              |
| **Window Dragging**      | Emulated / CSS Drag Regions     | **Native Win32 `HTCAPTION`** | Fluid Tao window management    |
| **Runtime Dependencies** | Node.js, Chromium, PowerShell   | **None (Pure Win32)**        | Standalone portable executable |

---

## 💻 System Requirements

- **Operating System**: Windows 10 (1903+) or Windows 11 (64-bit)
- **Graphics Card**:
  - Any DirectX 11, DirectX 12, or Vulkan compatible GPU.
  - _For DLSS Super Resolution_: NVIDIA GeForce RTX 20/30/40/50-Series.
  - _For 4x Multi-Frame Generation Unlock_: NVIDIA GeForce RTX 40-Series (Ada Lovelace) GPU.
- **Storage**: ~15 MB free space.

---

## 📦 Installation & Download

### Standalone Setup / Installer (Recommended)

- Run **`dlss-studio-setup.exe`** or install via **`dlss-studio.msi`** for standard Windows installation with Start Menu and Desktop shortcuts.

### Portable Executable

1. Download **`dlss-studio-portable.exe`** from the [Releases](https://github.com/bookamp/dlss-studio/releases) page.
2. Run from anywhere—no installation required.

---

## 🛠️ How to Use

1. **Launch DLSS 5 STUDIO**: Your installed games across Steam, Xbox Game Pass, Epic Games, and GOG will populate automatically.
2. **Select a Game**: Click on any game card to open its detail sheet.
3. **Choose Your Configuration**:
   - Select your **Rendering backend** (`ReShade` or `OptiScaler DLSS-NR`).
   - If using ReShade, select your **Installation route** (`Native DLSS (RenoDX)` or `DLSS 5 Feeder`).
   - If supported by your hardware and game engine, toggle **`Pre-SR Multipass`** (`1x`, `2x`, or `3x` passes) or **`Unlock 4x Multi-Frame Generation`**.
4. **Click "Install DLSS 5"** (ensuring the game is closed).
5. **Launch Your Game**: Launch via the "Launch Game" button or your regular launcher. To revert at any time, click **"Restore originals"**.

---

## ⚙️ Settings & System Tray

- **Run in Background**: Minimizes to the Windows System Notification Area (System Tray) when clicking the window close button (`X`).
- **Launch at Windows Startup**: Automatically registers in `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` to start in the background when Windows boots.
- **Industrial Rust Theme**: Toggle between sleek modern dark/light glassmorphism and textured Rust industrial metal finishes.

---

## 📚 Acknowledgements

- **DLSS 5 Swapper**: Original UI layout and desktop concept ([rakanki911/DLSS5-Swapper](https://github.com/rakanki911/DLSS5-Swapper)).
- **Frame Generation & HDR Mods**: Integrates runtime hooks developed by **Otis_Inf** and the **RenoDX** / **RTX40MFG-Unlock** project teams.
- **OptiScaler**: Open-source neural reconstruction wrapper developed by **cdozdil** ([OptiScaler](https://github.com/cdozdil/OptiScaler)).
- **Rust Ecosystem**: Built using [Dioxus](https://dioxuslabs.com/), [mimalloc](https://github.com/microsoft/mimalloc), [pelite](https://github.com/CasualX/pelite), and native Win32 APIs.

---

## 📜 License

This project is licensed under the MIT License. See [LICENSE](https://github.com/bookamp/dlss-studio/blob/main/LICENSE) for details.
