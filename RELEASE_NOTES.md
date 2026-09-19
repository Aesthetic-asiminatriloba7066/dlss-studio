# DLSS 5 Studio v1.0.7 ⚡

> **Window drag modal event fix across all screens, theme-harmonious squircle accordion trigger, high-contrast Light Theme contrast & badges, 2-second copy toast auto-dismiss timer, and complete multilingual translation coverage.**

---

### 🚀 Highlights & Improvements

- **Window Drag Event Fix Across All Screens**:
  - Resolved an issue where clicks, toggles, and switches on **Add-ons**, **History**, **Settings**, and **About** screens failed to register.
  - Bounded window dragging strictly to designated header surfaces (`.toolbar`, `.brand`, and empty sidebar spacer) with native CSS drag regions (`-webkit-app-region: drag` and `no-drag !important`).
- **Theme-Harmonious Accordion Trigger (Detected Graphics Modules)**:
  - Replaced the mismatched circular glowing coin with a **20×20px rounded squircle (`border-radius: 6px`)** that mirrors the shape, scale, and left-alignment of the feature checkboxes directly above it (`chkNrStyle`, `chkMfg`).
  - Rendered with a razor-sharp 12×12px SVG chevron polyline (`stroke-width: 2.8px`) that smoothly rotates 90° on expand.
- **High-Contrast Light Theme Styling**:
  - Eliminated blurry yellow glow washes on white backgrounds.
  - Added dedicated high-contrast light theme colors for the chevron (`#9a3412` rust with `#b45309` border) and summary badges (NVIDIA, AMD, Streamline, OptiScaler).
- **Toast Feedback 2-Second Auto-Dismiss Timer**:
  - Added an automatic 2000ms dismiss timer with debounced multi-click reset and micro-animation for all copy actions across the application.
- **Complete Multilingual Translation & Terminal Status**:
  - Localized the execution log terminal status (`@{status_ready}`) and added complete translations across all 13 supported languages.
  - Cleaned up obsolete emulator references and refined modular engine runtime detection.

---

### 📦 Included Packages & Downloads

| File | Type | Description |
| :--- | :--- | :--- |
| **`dlss-studio-v1.0.7-setup.exe`** | Standalone Setup / Installer (Recommended) | Native Rust setup wizard with configurable install and data storage locations, in-place update detection, Start Menu & Desktop shortcuts, and Windows registration. |
| **`dlss-studio-v1.0.7-portable.exe`** | Portable Executable | Standalone self-contained executable. Run anywhere with no installation required. |

---

### 📜 Previous Releases

<details>
<summary><b>DLSS 5 Studio v1.0.6 — Route Advisory Engine & Deployment Reflection Release</b></summary>

- **Route Advisory & Force Override Engine**: Interactive advisory model with high incompatibility warnings and force override deployment.
- **Automatic Deployment State Reflection**: Restores UI controls from disk inspection.
- **Intelligent Optimal Path Auto-Selection**: Automatic optimal route selection for vanilla games.
- **Dynamic Executable Switching**: Route updates dynamically on executable switch for unpatched titles.

</details>

<details>
<summary><b>DLSS 5 Studio v1.0.5 — Graphics API Detection & Non-Game Filtering Release</b></summary>

- **Dedicated Interactive Uninstaller & Clean Directory Purge**: Dedicated uninstaller UI with temp trampoline worker pattern.
- **ReShade Framework Suite & Feeder Verification**: Bundled `DrawText.fxh` and `FontAtlas.png`.
- **Non-Game Executable Filtering**: Automatic filtering of `*config.exe`, `*settings.exe`, `*setup.exe`, etc.
- **DirectX 9 vs DirectX 10 UE3 Detection**: Fixed false positive detection on legacy Unreal Engine 3 games.
- **Relic Modular Rendering Recognition**: Supported modular render libraries (`spDx9.dll`, etc.).
- **True "Undetected" Fallback Labeling**: Replaced ambiguous DX11 fallbacks for bootstrap stubs.

</details>

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
