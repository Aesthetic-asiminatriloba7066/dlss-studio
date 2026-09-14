# DLSS 5 Studio v1.0.1 — Hotfix Release ⚡

> **Hotfix release restoring Neural Rendering on the DLSS 5 Feeder route and improving out-of-the-box installation defaults.**

---

### 🔧 Fixes & Improvements

- **DLSS 5 Feeder Neural Rendering**:
  - Restored `NeuralUplift=1` in `ReShade.ini` during Feeder route deployments.
  - Resolves an issue where Neural Reconstruction (Feature 18) was initialized in a disabled state at launch, enabling seamless DLSS-NR execution across non-DX12 titles (such as DirectX 11 executables like `bg3_dx11.exe`).
- **Setup & Installation Defaults**:
  - Defaulted install directory to `C:\DLSS 5 Studio` and data directory to `C:\DLSS 5 Studio\data` for smooth, permission-friendly installs on standard user accounts without requiring elevation prompts.
  - Added real-time visual warning banners in the setup wizard when protected system directories (`Program Files`) are manually selected.
- **Documentation & Upstream Attribution**:
  - Updated OptiScaler attribution in `README.md` to credit `wilsjo2/OptiScaler-DLSSNR-PreSR-Multipass` for the Pre-SR Multipass and DLSS-NR runtime implementation.

---

### 📦 Included Packages & Downloads

| File | Type | Description |
| :--- | :--- | :--- |
| **`dlss-studio-setup.exe`** | Standalone Setup / Installer (Recommended) | Native Rust setup wizard with configurable install and data storage locations, Start Menu & Desktop shortcuts, and Windows registration. |
| **`dlss-studio-portable.exe`** | Portable Executable | Standalone self-contained executable. Run anywhere with no installation required. |

---

**Compatibility**: Windows 10 (1903+) or Windows 11 (64-bit) • NVIDIA GeForce RTX 20/30/40/50-Series (RTX 40-Series required for 4x Multi-Frame Generation).
