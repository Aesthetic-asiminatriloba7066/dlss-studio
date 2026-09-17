# DLSS 5 Studio v1.0.2 ⚡

> **Bug fix and maintenance update resolving automatic artwork downloads for manually added games, nested folder title resolution, installer upgrade handling, legacy pre-DirectX 10 (D3D8 & D3D9) dgVoodoo 2 translation with 32-bit LAA support, and core runtime updates.**

---

### 🔧 Fixes & Improvements

- **Automatic Artwork Resolution for Manually Added Games & Folders**:
  - Fixed an issue where manually added game executables or custom folder scans did not automatically download vertical box art and banners from the Steam CDN.
  - **Smart Title Inference for Nested Folders**: Fixed games located in subfolders (e.g. `Cyberpunk 2077/bin/x64`) incorrectly displaying as `"x64"` or `"bin"` by climbing parent directories to resolve the actual game title.
  - **Title Normalization & Fuzzy Splitting**: Added boundary splitting for fused titles and numbers (e.g. `Cyberpunk2077` -> `Cyberpunk 2077`) so Steam artwork queries resolve successfully.
  - **On-Demand Artwork Fetch**: Added a "Fetch Artwork" action in the game detail sheet for manual titles that are missing artwork, with deduplication to prevent redundant network requests.

- **Installer Upgrade & Process Handling**:
  - Fixed installer behavior when upgrading an existing installation: now correctly checks the registry, switches the action to **"Update"**, and safely closes running `dlss-studio.exe` processes before copying files to prevent `ERROR_SHARING_VIOLATION` locks.
  - Preserves user data (`library.json`, custom folders, settings) across updates.

- **Runtime Component Updates**:
  - Updated bundled runtime components to their latest upstream releases:
    - **OptiScaler DLSS-NR v0.8.4** (`OptiScaler-NR-v0.8.4.zip`)
    - **DLSS 5 Feeder v1.16.0-beta.3** (`DLSS5-Feeder-1.16.0-beta.3.zip`)
    - **MFGAdaUnlock-RenoDx 1.0** (`renodx-mfgunlock.addon64`)

- **Legacy Pre-DirectX 10 (DirectX 8 & 9) dgVoodoo 2 Interop**:
  - Added automated dgVoodoo 2 translation for older pre-DirectX 10 titles (DirectX 8 and DirectX 9 via `d3d8.dll`, `d3d9.dll`, `dgVoodoo.conf`), wrapping legacy graphics calls into modern Direct3D 11 swapchains to route into the DLSS 5 Feeder pipeline (DirectX 11 was already supported natively).
  - Automatically handles PE architecture selection between 32-bit (x86) and 64-bit binaries, configuring the `dlss5-feed-host64.exe` inter-process bridge or `addon32` fallback.
  - Added PE Large Address Aware (LAA) header inspection and safe toggling for 32-bit executables, allowing classic games to address up to 4 GB of virtual memory and preventing out-of-memory crashes with modern upscalers and shaders.
  - Automated `dgVoodoo.conf` configuration (brief watermark confirmation, VRAM allocation, parser protection) and clean restoration during route switching or rollback.

- **Build & Packaging**:
  - Release executables and installer packages are now version-stamped (`dlss-studio-v1.0.2-setup.exe` and `dlss-studio-v1.0.2-portable.exe`).
  - Automated build scripts now clean up unversioned binaries from `target/release`.

---

### 📦 Included Packages & Downloads

| File | Type | Description |
| :--- | :--- | :--- |
| **`dlss-studio-v1.0.2-setup.exe`** | Standalone Setup / Installer (Recommended) | Native Rust setup wizard with configurable install and data storage locations, in-place update detection, Start Menu & Desktop shortcuts, and Windows registration. |
| **`dlss-studio-v1.0.2-portable.exe`** | Portable Executable | Standalone self-contained executable. Run anywhere with no installation required. |

---

### 📜 Previous Releases

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
