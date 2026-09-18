# DLSS 5 Studio v1.0.6 ⚡

> **Route advisory engine, high-contrast incompatibility warnings, unrestricted force override deployment, automatic deployment state reflection, and intelligent optimal path selection.**

---

### 🚀 Highlights & Improvements

- **Route Advisory & Force Override Engine**:
  - Replaced hard-blocking route restrictions with an interactive advisory model.
  - All backends (**ReShade**, **OptiScaler**) and routes (**DLSS 5 Feeder**, **Native DLSS**) remain fully selectable for every game in your library.
  - Selecting an incompatible combination displays a prominent crimson alert card (**`🚨 High Incompatibility Warning`**) outlining exact technical reasons and recommended alternatives.
  - Advanced users can bypass warnings at any time using the crimson **`Deploy Anyway (Force Override) ⚠️`** action.
- **Automatic Deployment State Reflection**:
  - Opening the game detail sheet now queries active game installations on disk and faithfully restores all UI controls (**Backend**, **Route**, **Pre-SR**, **Passes**, **4x MFG**, and **Neural Rendering Style**) to reflect what is actually deployed instead of resetting to defaults.
- **Intelligent Optimal Path Auto-Selection (Vanilla Games)**:
  - Opening an unmodded game automatically selects the optimal compatible route based on its detected graphics API and bitness:
    - **DirectX 11, Vulkan, OpenGL, DirectX 9, or non-DLSS**: Automatically defaults to **DLSS 5 Feeder** (no false warnings).
    - **64-Bit DirectX 12 with native DLSS**: Defaults to **Native DLSS**.
    - **4x Multi-Frame Generation**: Defaults to off for vanilla titles.
- **Dynamic Executable Switching (Unpatched Games Only)**:
  - Switching between game executables in the dropdown automatically updates the route to match the chosen executable's optimal API path if the title is unpatched. If an active deployment already exists, settings are strictly preserved.

---

### 📦 Included Packages & Downloads

| File | Type | Description |
| :--- | :--- | :--- |
| **`dlss-studio-v1.0.6-setup.exe`** | Standalone Setup / Installer (Recommended) | Native Rust setup wizard with configurable install and data storage locations, in-place update detection, Start Menu & Desktop shortcuts, and Windows registration. |
| **`dlss-studio-v1.0.6-portable.exe`** | Portable Executable | Standalone self-contained executable. Run anywhere with no installation required. |

---

### 📜 Previous Releases

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
