# DLSS 5 Studio v1.0.4 ⚡

> **Graphics API detection accuracy enhancements, non-game configuration and utility filtering, Relic modular rendering engine support, and true "Undetected" fallback labeling.**

---

### 🔧 Fixes & Improvements

- **Dedicated Interactive Uninstaller & Clean Directory Purge**:
  - Added dedicated `UninstallApp` UI with confirmation screen, live progress bar, and completion screen.
  - Implemented temp trampoline worker pattern: launches `%TEMP%\dlss_studio_uninstall.exe` and terminates the launcher so the entire installation directory can be completely purged without Windows file-locking conflicts.
  - Checkbox: *"Also remove downloaded models, cache, and preferences (%APPDATA%\dlss-5-studio)"* (checked by default; leaves original game backups inside game folders untouched).
  - Routed setup and uninstaller WebView2 data directories to `%TEMP%`, completely preventing `.WebView2` folders from cluttering the installation folder.
- **ReShade Framework Suite & Feeder Verification (`DrawText.fxh`)**:
  - Bundled `DrawText.fxh`, `FontAtlas.png`, and the slim ReShade framework headers suite into `feeder-shaders\`.
  - Ensures upstream verification script (`Verify-DLSS5Feeder.ps1`) passes with 0 errors and 0 warnings.
- **Non-Game Executable & Helper Filtering**:
  - Expanded `is_installer_or_helper` to automatically filter configuration utilities, settings tools, registration, support, and setup binaries (`*config.exe`, `*settings.exe`, `*setup.exe`, `*activation*.exe`, `*autorun*.exe`, `*registration*.exe`, `*support*.exe`) from game directories and executable selection dropdowns (e.g. `MassEffect2Config.exe`).
- **DirectX 9 vs DirectX 10 Detection & Unreal Engine 3 Marker Priority**:
  - Resolved false positive DirectX 10 / DirectX 11 detection on legacy Unreal Engine 3 games (such as *Mass Effect 2* `ME2Game.exe`): prioritized active `d3d9.dll` and `Direct3DCreate9` / `Direct3DCreate9Ex` imports over dormant Direct3D 10 engine headers.
- **Relic Modular Rendering Engine Recognition**:
  - Added support for modular sibling graphics libraries in Relic engine games (such as *Warhammer 40,000: Dawn of War Definitive Edition* and *Company of Heroes*) recognizing `spDx9.dll`, `GraphicsDx9.dll`, etc. while preventing auxiliary DXGI video player helpers from misleading 3D API detection.
- **True "Undetected" Fallback Labeling**:
  - Replaced ambiguous `"DirectX 11"` fallback labeling with `"Undetected"` for executables lacking any 3D graphics imports, PE markers, or graphics sibling modules (such as launcher stubs and bootstrap wrappers like `MassEffect2.exe`).
- **Candidate Scoring & Primary Game Executable Selection**:
  - Upgraded heuristic executable scoring to heavily prioritize real game binaries with verified 3D graphics imports (+5,000 pts) and substantial PE code size (+4,000 pts for files > 5 MB), while penalizing stubs (< 1 MB without graphics calls) by -5,000 pts.

---

### 📦 Included Packages & Downloads

| File | Type | Description |
| :--- | :--- | :--- |
| **`dlss-studio-v1.0.4-setup.exe`** | Standalone Setup / Installer (Recommended) | Native Rust setup wizard with configurable install and data storage locations, in-place update detection, Start Menu & Desktop shortcuts, and Windows registration. |
| **`dlss-studio-v1.0.4-portable.exe`** | Portable Executable | Standalone self-contained executable. Run anywhere with no installation required. |

---

### 📜 Previous Releases

<details>
<summary><b>DLSS 5 Studio v1.0.2 — Maintenance & dgVoodoo 2 Interop Release</b></summary>

- **Automatic Artwork Resolution for Manually Added Games & Folders**: Smart title inference for nested folders and boundary splitting for fused titles.
- **Installer Upgrade & Process Handling**: Registry check, in-place update mode, and graceful process shutdown.
- **Runtime Component Updates**: OptiScaler DLSS-NR v0.8.4, DLSS 5 Feeder v1.16.0-beta.3, MFGAdaUnlock-RenoDx 1.0.
- **Legacy Pre-DirectX 10 (DirectX 8 & 9) dgVoodoo 2 Interop**: Automated dgVoodoo 2 translation, 32-bit Large Address Aware (LAA) inspection and toggling.
- **Versioned Deliverables**: Version-stamped setup and portable executables.

</details>

<details>
<summary><b>DLSS 5 Studio v1.0.1 — Hotfix Release</b></summary>

> Hotfix release restoring Neural Rendering on the DLSS 5 Feeder route and improving out-of-the-box installation defaults.

- **DLSS 5 Feeder Neural Rendering**:
  - Restored `NeuralUplift=1` in `ReShade.ini` during Feeder route deployments.
  - Resolves an issue where Neural Reconstruction (Feature 18) was initialized in a disabled state at launch, enabling seamless DLSS-NR execution across non-DX12 titles (such as DirectX 11 executables like `bg3_dx11.exe`).
- **Setup & Installation Defaults**:
  - Defaulted install directory to `C:\DLSS 5 Studio` and data directory to `C:\DLSS 5 Studio\data` for smooth, permission-friendly installs on standard user accounts without requiring elevation prompts.
  - Added real-time visual warning banners in the setup wizard when protected system directories (`Program Files`) are manually selected.
- **Documentation & Upstream Attribution**:
  - Updated OptiScaler attribution in `README.md` to credit `wilsjo2/OptiScaler-DLSSNR-PreSR-Multipass` for the Pre-SR Multipass and DLSS-NR runtime implementation.

</details>

---

**Compatibility**: Windows 10 (1903+) or Windows 11 (64-bit) • NVIDIA GeForce RTX 20/30/40/50-Series (RTX 40-Series required for 4x Multi-Frame Generation).
