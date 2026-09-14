# DLSS 5 Studio v1.0.0 — Initial Release ⚡

> **A blisteringly fast, ultra-low memory utility built in pure native Rust to enable and unlock DLSS, Neural Reconstruction, and 4x Frame Generation across your PC games while preserving pristine graphical fidelity.**

---

### ⚡ 4x Multi-Frame Generation (MFG) Unlock
NVIDIA officially restricts 3x and 4x Multi-Frame Generation in drivers to RTX 50-Series hardware. DLSS 5 Studio unlocks 3x and 4x multipliers on GeForce RTX 40-Series GPUs (Ada Lovelace).

- **Native DLSS-G Streamline Interception**: Leverages RenoDX hook add-ons (`renodx-mfgunlock.addon64`) to intercept Streamline Frame Generation contracts on games with native Frame Generation runtimes (`sl.dlss_g.dll`, `nvngx_dlssg.dll`).
- **Honest Hardware & API Gating**: Automatically inspects game binaries and engine exports. Games lacking native Frame Generation or running on DirectX 11 executables (e.g. `bg3_dx11.exe`) are cleanly gated to prevent instability.

### 🔬 OptiScaler DLSS-NR & Pre-SR Multipass Clarity
Route games through OptiScaler's open-source multi-vendor neural reconstruction wrapper (`nvngx.dll` / `dxgi.dll`).

- **Pre-SR Multipass Enhancement**: Enables multi-pass neural reconstruction (`1x`, `2x`, or `3x` passes) for dramatic sharpness, clarity, and temporal stability enhancements.
- **Universal RTXMFG Integration**: Pairs OptiScaler with standalone proxy hooks (`version.dll`) to allow simultaneous Frame Generation and Pre-SR multipass enhancements in supported 64-bit games.

### 🎯 Universal Multi-Store Game Scanner
Scans, detects, and organizes your game libraries automatically across all major PC launchers:

- **Steam**: Resolves library roots from `SteamPath` registry and `libraryfolders.vdf`, parses `appmanifest_<id>.acf`, and downloads official 600x900 vertical box art from Steam CDN.
- **Xbox Game Pass / Windows Store**: Queries `GamingServices` package repository and scans `XboxGames` drive roots. Parses GDK `MicrosoftGame.config` and `AppxManifest.xml` to bypass launcher wrappers (`gamelaunchhelper.exe`), resolves authentic 64-bit executables, and extracts high-resolution logos directly from package assets.
- **Epic Games Store & GOG Galaxy**: Discovers installed titles by parsing `%PROGRAMDATA%\Epic\...\Manifests\*.item` and `GOG.com\Games` registry trees with playtasks.
- **Custom Folders & Manual Executables**: Add any custom game folder or executable with custom title renaming and persistent artwork caching.

### 🔄 Seamless Cross-Route Hot-Swapping & Vanilla Safety
- **One-Click Hot-Swapping**: Switch freely between ReShade (Native / DLSS 5 Feeder) and OptiScaler without intermediate restore. The engine automatically unregisters Vulkan implicit layers, removes conflicting proxy DLLs, and deploys the new payload.
- **Vanilla Backup Continuity**: Switching between routes carries forward the genuine unmodded game files through arbitrary successive swaps.
- **Atomic Rollback Journals**: Every modification creates a snapshot in `_DLSS5_Backup/originals/` before touching any game files.
- **Deep Mod Cleaner**: Purges leftover proxy DLLs (`dxgi.dll`, `OptiScaler.dll`, `ReShade64.dll`, `.addon64`) up to 4 directory levels deep without risking original game files.
- **Running Process Guards & Anti-Cheat Detection**: Inspects running processes via native Win32 `Toolhelp32` snapshots, blocking mod deployment or restore while a game is active, and detects EasyAntiCheat, BattlEye, and Vanguard before touching protected titles.

### 🌍 Complete Multilingual Localization (14 Languages)
- **14 Supported Languages**: English, Deutsch (German), Español (Spanish), Français (French), Italiano (Italian), Português (Portuguese), Русский (Russian), 简体中文 (Simplified Chinese), 日本語 (Japanese), 한국어 (Korean), Polski (Polish), Türkçe (Turkish), العربية (Arabic with full RTL layout support), and हिन्दी (Hindi).
- **Reactive Dynamic Switching**: Instantly switch languages anytime from the header selector or Settings. All views, sheets, specs, action badges, and tooltips update in real-time with zero app restart.
- **Dynamic Activity Logging**: Activity log entries use tokenized templates (`@{key|...}`), allowing the in-app terminal to dynamically translate logs into the selected language while keeping on-disk diagnostics (`dlss-studio.log`) in standard English for seamless GitHub issue reporting.

### 📥 Resilient Component Downloader & Streaming Progress
- **Direct-to-Disk Streaming**: Automatically streams runtime packages (including the 144 MB NVIDIA Streamline package) directly to disk via temporary `.part` files with zero memory spikes.
- **Auto-Retry on Transient Resets**: 3-attempt exponential backoff retry for network resets, socket timeouts, and dropped connections.
- **Live Progress Animation**: Real-time byte tracking and fluid percentage progress bar animation in both the sidebar status card and the Add-ons manager.

### 🪶 100% Pure Rust Architecture
- **~20 MB RAM**: Consumes over 95% less memory than traditional Electron/Web wrappers.
- **<200ms Startup**: Instantaneous native cold launch with direct Win32 `HTCAPTION` window management.
- **Zero Runtime Dependencies**: No Node.js, Python, or Chromium runtimes required.

---

### 📦 Included Packages & Downloads

| File | Type | Description |
| :--- | :--- | :--- |
| **`dlss-studio-setup.exe`** | Standalone Setup / Installer (Recommended) | Modern native Rust setup wizard with configurable install and data storage locations, Start Menu & Desktop shortcuts, and Windows Add/Remove Programs registration. |
| **`dlss-studio-portable.exe`** | Portable Executable | Standalone self-contained executable. Run anywhere with no installation required. |

---

**Compatibility**: Windows 10 (1903+) or Windows 11 (64-bit) • NVIDIA GeForce RTX 20/30/40/50-Series (RTX 40-Series required for 4x Multi-Frame Generation).
