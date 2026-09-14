#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use crate::core::journal::{ActiveManifest, ManifestItem, ManifestGame, save_manifest, append_history, HistoryRow};
use crate::core::mfg_unlock::configure_mfg_unlock_ini;

pub const RELEASE_VERSION: &str = "0.7.6-dlssnr";

#[derive(Debug, Clone)]
pub struct OptiScalerOptions {
    pub pre_sr: bool,
    pub passes: u32,
    pub mfg_unlock: bool,
    pub target_exe_name: String,
}

impl Default for OptiScalerOptions {
    fn default() -> Self {
        Self {
            pre_sr: true,
            passes: 3,
            mfg_unlock: false,
            target_exe_name: String::new(),
        }
    }
}

/// Updates or inserts a key/value pair within a specific [Section] of an INI file while preserving all comments and other formatting.
pub fn set_ini(text: &str, section: &str, key: &str, value: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.replace('\u{feff}', "").lines().map(|s| s.to_string()).collect();

    let header = format!("[{}]", section).to_lowercase();
    let start_idx = lines.iter().position(|l| l.trim().to_lowercase() == header);

    if let Some(start) = start_idx {
        let mut end = lines.len();
        for i in (start + 1)..lines.len() {
            let t = lines[i].trim();
            if t.starts_with('[') && t.ends_with(']') {
                end = i;
                break;
            }
        }
        let wanted = key.to_lowercase();
        let mut changed = false;
        for i in (start + 1)..end {
            let line = &lines[i];
            let trimmed = line.trim();
            if !trimmed.starts_with(';') && !trimmed.starts_with('#') {
                if let Some((k, _)) = trimmed.split_once('=') {
                    if k.trim().to_lowercase() == wanted {
                        lines[i] = format!("{}={}", key, value);
                        changed = true;
                        break;
                    }
                }
            }
        }
        if !changed {
            lines.insert(end, format!("{}={}", key, value));
        }
    } else {
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("[{}]", section));
        lines.push(format!("{}={}", key, value));
    }

    lines.join(newline)
}

pub fn get_ini(text: &str, section: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let header = format!("[{}]", section).to_lowercase();
    let start = lines.iter().position(|l| l.trim().to_lowercase() == header)?;

    let wanted = key.to_lowercase();
    for line in &lines[(start + 1)..] {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            break;
        }
        if trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim().to_lowercase() == wanted {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

pub fn generate_optiscaler_ini(
    base_text: &str,
    pre_sr: bool,
    passes: u32,
    external_mfg: bool,
    target_exe: Option<&str>,
) -> String {
    let mut text = base_text.to_string();

    text = set_ini(&text, "DlssNr", "Enabled", if pre_sr { "true" } else { "false" });
    text = set_ini(&text, "DlssNr", "RunBeforeSR", if pre_sr { "true" } else { "false" });
    text = set_ini(&text, "DlssNr", "Passes", &passes.to_string());
    text = set_ini(&text, "DlssNr", "ApplyAfterRR", "true");
    text = set_ini(&text, "Plugins", "LoadReshade", "false");
    text = set_ini(&text, "FrameGen", "External", if external_mfg { "true" } else { "false" });
    text = set_ini(&text, "DLSSG", "InterpolationCount", "auto");
    text = set_ini(&text, "DLSSG", "OverrideInterpolationCount", "auto");
    text = set_ini(&text, "Menu", "OverlayMenu", "true");
    text = set_ini(&text, "Menu", "ShortcutKey", "0x2D"); // INSERT key

    if let Some(exe) = target_exe {
        if !exe.is_empty() {
            text = set_ini(&text, "Init", "TargetProcessName", exe);
        }
    }

    text
}

pub fn configure_optiscaler_ini(base_text: &str, opts: &OptiScalerOptions) -> String {
    generate_optiscaler_ini(
        base_text,
        opts.pre_sr,
        opts.passes,
        opts.mfg_unlock,
        Some(&opts.target_exe_name),
    )
}

/// Locates the OptiScaler component directory containing OptiScaler.dll, OptiScaler.ini, and OptiScaler subfolder.
fn app_exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

/// Returns standard candidate directories to check for runtime component payloads.
fn get_component_roots() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(crate::core::downloader::get_components_root());
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        dirs.push(PathBuf::from(&local_appdata).join("dlss-5-studio").join("components"));
        dirs.push(PathBuf::from(&local_appdata).join("DLSS-Studio").join("components"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(&appdata).join("dlss-5-studio").join("components"));
        dirs.push(PathBuf::from(&appdata).join("DLSS-Studio").join("components"));
    }
    if let Some(exe) = app_exe_dir() {
        dirs.push(exe.join("components"));
    }
    dirs
}

/// Locates the OptiScaler component directory containing OptiScaler.dll, OptiScaler.ini, and OptiScaler subfolder.
pub fn find_optiscaler_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for root in get_component_roots() {
        candidates.push(root.join("OptiScaler-0.8.3-dlssnr"));
        candidates.push(root.join("OptiScaler-0.7.7-dlssnr"));
        candidates.push(root.join("OptiScaler-DLSSNR-v0.7.7"));
        candidates.push(root.join("OptiScaler-0.7.6-dlssnr"));
        candidates.push(root.join("OptiScaler-DLSSNR-v0.7.6"));
        candidates.push(root.join("OptiScaler-v0.7.6"));
        candidates.push(root.join("OptiScaler-0.6.2-dlssnr"));
        candidates.push(root.join("OptiScaler-0.2.0-dlssnr"));
    }
    if let Some(exe) = app_exe_dir() {
        candidates.push(exe.join("components").join("OptiScaler-0.8.3-dlssnr"));
        candidates.push(exe.join("components").join("OptiScaler-0.7.7-dlssnr"));
        candidates.push(exe.join("components").join("OptiScaler-DLSSNR-v0.7.7"));
        candidates.push(exe.join("payload").join("OptiScaler-0.7.7-dlssnr"));
        candidates.push(exe.join("payload").join("OptiScaler-DLSSNR-v0.7.7"));
        candidates.push(exe.join("components").join("OptiScaler-0.7.6-dlssnr"));
        candidates.push(exe.join("components").join("OptiScaler-DLSSNR-v0.7.6"));
        candidates.push(exe.join("payload").join("OptiScaler-0.7.6-dlssnr"));
        candidates.push(exe.join("payload").join("OptiScaler-DLSSNR-v0.7.6"));
        candidates.push(exe.join("components").join("OptiScaler-0.6.2-dlssnr"));
        candidates.push(exe.join("payload").join("OptiScaler-0.6.2-dlssnr"));
    }

    for c in candidates {
        if c.join("OptiScaler.dll").exists() {
            return Some(c);
        }
    }
    None
}

/// Locates ReShade64.dll payload
pub fn find_reshade64_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(PathBuf::from(r"payload\reshade-vulkan\ReShade64.dll"));
    candidates.push(PathBuf::from(r"payload\ReShade64.dll"));
    for root in get_component_roots() {
        candidates.push(root.join("reshade-vulkan").join("ReShade64.dll"));
        candidates.push(root.join("ReShade64.dll"));
    }
    if let Some(exe) = app_exe_dir() {
        for ancestor in exe.ancestors().take(4) {
            candidates.push(ancestor.join("payload").join("reshade-vulkan").join("ReShade64.dll"));
            candidates.push(ancestor.join("payload").join("ReShade64.dll"));
            candidates.push(ancestor.join("components").join("reshade-vulkan").join("ReShade64.dll"));
        }
    }

    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

/// Locates the RenoDX 4x MFG Unlock addon
pub fn find_mfg_addon_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for root in get_component_roots() {
        candidates.push(root.join("mfg-unlock-0.9").join("renodx-mfgunlock.addon64"));
        candidates.push(root.join("mfg-unlock-0.8").join("renodx-mfgunlock.addon64"));
        candidates.push(root.join("mfg-unlock-0.6.1").join("renodx-mfgunlock.addon64"));
        candidates.push(root.join("renodx-mfgunlock.addon64"));
    }
    if let Some(exe) = app_exe_dir() {
        candidates.push(exe.join("components").join("mfg-unlock-0.9").join("renodx-mfgunlock.addon64"));
        candidates.push(exe.join("components").join("mfg-unlock-0.8").join("renodx-mfgunlock.addon64"));
        candidates.push(exe.join("components").join("mfg-unlock-0.6.1").join("renodx-mfgunlock.addon64"));
        candidates.push(exe.join("payload").join("addons").join("renodx-mfgunlock.addon64"));
        candidates.push(exe.join("renodx-mfgunlock.addon64"));
    }

    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}


/// Locates the RenoDX v4.7 Integrated DLSS 5 Engine addon
pub fn find_renodx_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for root in get_component_roots() {
        candidates.push(root.join("renodx-dlss5").join("renodx-dlss5.addon64"));
        candidates.push(root.join("renodx-dlss5.addon64"));
        candidates.push(root.join("addons").join("renodx-dlss5.addon64"));
    }
    if let Some(exe) = app_exe_dir() {
        candidates.push(exe.join("components").join("renodx-dlss5").join("renodx-dlss5.addon64"));
        candidates.push(exe.join("payload").join("renodx-dlss5.addon64"));
        candidates.push(exe.join("addons").join("renodx-dlss5.addon64"));
    }

    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

/// Locates the modern Streamline nvngx_dlss.dll runtime (v3.7+/v3.10+)
pub fn find_dlss_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for root in get_component_roots() {
        candidates.push(root.join("streamline-2.14.1").join("streamline").join("nvngx_dlss.dll"));
        candidates.push(root.join("streamline").join("nvngx_dlss.dll"));
        candidates.push(root.join("nvngx_dlss.dll"));
    }
    if let Some(exe) = app_exe_dir() {
        candidates.push(exe.join("components").join("streamline-2.14.1").join("streamline").join("nvngx_dlss.dll"));
        candidates.push(exe.join("components").join("streamline").join("nvngx_dlss.dll"));
        candidates.push(exe.join("payload").join("streamline-2.14.1").join("streamline").join("nvngx_dlss.dll"));
        candidates.push(exe.join("payload").join("streamline").join("nvngx_dlss.dll"));
        candidates.push(exe.join("nvngx_dlss.dll"));
    }

    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

/// Locates the Streamline nvngx_dlssnr.dll runtime
pub fn find_dlssnr_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for root in get_component_roots() {
        candidates.push(root.join("streamline-2.14.1").join("streamline").join("nvngx_dlssnr.dll"));
        candidates.push(root.join("streamline").join("nvngx_dlssnr.dll"));
        candidates.push(root.join("nvngx_dlssnr.dll"));
    }
    if let Some(exe) = app_exe_dir() {
        candidates.push(exe.join("components").join("streamline-2.14.1").join("streamline").join("nvngx_dlssnr.dll"));
        candidates.push(exe.join("components").join("streamline").join("nvngx_dlssnr.dll"));
        candidates.push(exe.join("payload").join("streamline-2.14.1").join("streamline").join("nvngx_dlssnr.dll"));
        candidates.push(exe.join("payload").join("streamline").join("nvngx_dlssnr.dll"));
        candidates.push(exe.join("nvngx_dlssnr.dll"));
    }

    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

/// Locates Dashdogy's Universal RTXMFG v1.3.2 standalone DLL (RTXMFG.dll)
pub fn find_standalone_mfg_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(PathBuf::from(r"payload\mfg-standalone\RTXMFG.dll"));
    candidates.push(PathBuf::from(r"components\mfg-standalone\RTXMFG.dll"));
    for root in get_component_roots() {
        candidates.push(root.join("mfg-standalone").join("RTXMFG.dll"));
        candidates.push(root.join("RTXMFG.dll"));
    }
    if let Some(exe) = app_exe_dir() {
        for ancestor in exe.ancestors().take(4) {
            candidates.push(ancestor.join("payload").join("mfg-standalone").join("RTXMFG.dll"));
            candidates.push(ancestor.join("payload").join("RTXMFG.dll"));
            candidates.push(ancestor.join("components").join("mfg-standalone").join("RTXMFG.dll"));
            candidates.push(ancestor.join("RTXMFG.dll"));
        }
    }

    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

pub fn find_streamline_payload() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(PathBuf::from(r"payload\streamline-2.14.1\streamline"));
    candidates.push(PathBuf::from(r"payload\streamline"));
    for root in get_component_roots() {
        candidates.push(root.join("streamline-2.14.1").join("streamline"));
        candidates.push(root.join("streamline"));
        candidates.push(root.join("OptiScaler-0.7.7-dlssnr").join("OptiScaler").join("streamline"));
    }
    if let Some(exe) = app_exe_dir() {
        for ancestor in exe.ancestors().take(4) {
            candidates.push(ancestor.join("payload").join("streamline-2.14.1").join("streamline"));
            candidates.push(ancestor.join("payload").join("streamline"));
            candidates.push(ancestor.join("components").join("streamline-2.14.1").join("streamline"));
            candidates.push(ancestor.join("components").join("streamline"));
            candidates.push(ancestor.join("components").join("OptiScaler-0.7.7-dlssnr").join("OptiScaler").join("streamline"));
        }
    }

    for c in candidates {
        if c.join("sl.interposer.dll").exists() && c.join("sl.common.dll").exists() {
            return Some(c);
        }
    }
    None
}

/// Locates the open-source DLSS-NR forwarder (nvngx.dll_dlssnr.dll)
pub fn find_nvngx_snippet_payload(opti_dir: &Path) -> Option<PathBuf> {
    if opti_dir.join("nvngx.dll_dlssnr.dll").is_file() {
        return Some(opti_dir.join("nvngx.dll_dlssnr.dll"));
    }
    for root in get_component_roots() {
        let c1 = root.join("OptiScaler-0.7.7-dlssnr").join("nvngx.dll_dlssnr.dll");
        if c1.is_file() {
            return Some(c1);
        }
        let c2 = root.join("OptiScaler-0.2.0-dlssnr").join("nvngx.dll_dlssnr.dll");
        if c2.is_file() {
            return Some(c2);
        }
        let c3 = root.join("nvngx.dll_dlssnr.dll");
        if c3.is_file() {
            return Some(c3);
        }
    }
    if let Some(exe) = app_exe_dir() {
        for ancestor in exe.ancestors().take(4) {
            let c1 = ancestor.join("payload").join("OptiScaler-0.7.7-dlssnr").join("nvngx.dll_dlssnr.dll");
            if c1.is_file() {
                return Some(c1);
            }
            let c2 = ancestor.join("components").join("OptiScaler-0.7.7-dlssnr").join("nvngx.dll_dlssnr.dll");
            if c2.is_file() {
                return Some(c2);
            }
            let c3 = ancestor.join("payload").join("nvngx.dll_dlssnr.dll");
            if c3.is_file() {
                return Some(c3);
            }
        }
    }
    None
}

/// Fully decoupled, typed bundle of required runtime component binaries.
/// Enables 100% in-memory / temporary sandbox unit testing without machine-level dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadBundle {
    pub optiscaler_dll: PathBuf,
    pub optiscaler_ini: PathBuf,
    pub optiscaler_dir: Option<PathBuf>,
    pub nvngx_dlss_dll: Option<PathBuf>,
    pub nvngx_dlssnr_dll: Option<PathBuf>,
    pub nvngx_snippet_dll: Option<PathBuf>,
    pub rtxmfg_dll: Option<PathBuf>,
    pub reshade64_dll: Option<PathBuf>,
    pub renodx_dlss5_addon: Option<PathBuf>,
    pub renodx_mfgunlock_addon: Option<PathBuf>,
    pub streamline_dir: Option<PathBuf>,
    pub feeder_components: Option<crate::core::downloader::FeederComponents>,
}

impl PayloadBundle {
    pub fn from_system() -> Result<Self, String> {
        let opti_dir = find_optiscaler_payload()
            .ok_or_else(|| "OptiScaler payload not found on system. Please verify component payloads.".to_string())?;
        let optiscaler_dll = opti_dir.join("OptiScaler.dll");
        let optiscaler_ini = opti_dir.join("OptiScaler.ini");
        let optiscaler_dir = if opti_dir.join("OptiScaler").is_dir() {
            Some(opti_dir.join("OptiScaler"))
        } else {
            None
        };
        let nvngx_snippet_dll = find_nvngx_snippet_payload(&opti_dir);
        let nvngx_dlss_dll = find_dlss_payload();
        let nvngx_dlssnr_dll = find_dlssnr_payload();
        let rtxmfg_dll = find_standalone_mfg_payload();
        let reshade64_dll = find_reshade64_payload();
        let renodx_dlss5_addon = find_renodx_payload();
        let renodx_mfgunlock_addon = find_mfg_addon_payload();
        let streamline_dir = find_streamline_payload();
        let feeder_components = crate::core::downloader::find_local_feeder_components();

        Ok(Self {
            optiscaler_dll,
            optiscaler_ini,
            optiscaler_dir,
            nvngx_dlss_dll,
            nvngx_dlssnr_dll,
            nvngx_snippet_dll,
            rtxmfg_dll,
            reshade64_dll,
            renodx_dlss5_addon,
            renodx_mfgunlock_addon,
            streamline_dir,
            feeder_components,
        })
    }
}

/// Embedded ReShade Companion In-Game Overlay Add-on (dlss5-lab-overlay.addon64)
/// Enables 100% self-contained single-file portable execution without external loose files.
pub const EMBEDDED_OVERLAY_ADDON: &[u8] = include_bytes!("../../assets/dlss5-lab-overlay.addon64");

/// Verifies that a discovered overlay add-on binary is a valid 64-bit ReShade addon payload
fn is_native_overlay_addon(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    // Verified 64-bit ReShade companion addon binary payload
    metadata.len() >= 50_000
}

/// Locates or materializes the DLSS 5 Studio In-Game Overlay addon (dlss5-lab-overlay.addon64)
pub fn find_overlay_addon_payload() -> Option<PathBuf> {
    // 1. First priority: ensure AppData components contains a fresh, verified copy of EMBEDDED_OVERLAY_ADDON
    let appdata_comp = crate::core::state::get_appdata_dir().join("components");
    let target = appdata_comp.join("dlss5-lab-overlay.addon64");
    let target_needs_write = match fs::metadata(&target) {
        Ok(meta) => meta.len() != EMBEDDED_OVERLAY_ADDON.len() as u64,
        Err(_) => true,
    };
    if target_needs_write {
        let _ = fs::create_dir_all(&appdata_comp);
        let _ = fs::write(&target, EMBEDDED_OVERLAY_ADDON);
    }

    let mut candidates = Vec::new();

    // 2. Verified AppData target
    candidates.push(target);

    // 3. Workspace assets and build outputs
    candidates.push(PathBuf::from(r"assets\dlss5-lab-overlay.addon64"));
    candidates.push(PathBuf::from(r"target\release\dlss5_lab_overlay.dll"));
    candidates.push(PathBuf::from(r"target\release\dlss5-lab-overlay.addon64"));
    candidates.push(PathBuf::from(r"target\debug\dlss5_lab_overlay.dll"));
    candidates.push(PathBuf::from(r"target\debug\dlss5-lab-overlay.addon64"));

    // 4. Component roots and app executable dir
    for root in get_component_roots() {
        candidates.push(root.join("dlss5-lab-overlay.addon64"));
        candidates.push(root.join("overlay").join("dlss5-lab-overlay.addon64"));
    }
    if let Some(exe) = app_exe_dir() {
        candidates.push(exe.join("dlss5-lab-overlay.addon64"));
        candidates.push(exe.join("components").join("dlss5-lab-overlay.addon64"));
        candidates.push(exe.join("overlay").join("dlss5-lab-overlay.addon64"));
    }

    for c in candidates {
        if is_native_overlay_addon(&c) {
            return Some(c);
        }
    }

    None
}


#[derive(Debug, Clone)]
pub struct DeployOptions {
    pub game_name: Option<String>,
    pub game_dir: PathBuf,
    pub exe_path: PathBuf,
    pub api: String,
    pub pre_sr: bool,
    pub passes: u32,
    pub mfg_unlock: bool,
    pub mfg_multiplier: u32,
}

#[derive(Debug, Clone)]
pub struct DeployResult {
    pub success: bool,
    pub log_lines: Vec<String>,
    pub replaced: usize,
    pub added: usize,
}

fn is_known_mod_file(dest: &Path) -> bool {
    let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if name == "optiscaler.ini"
        || name == "optiscaler.log"
        || name == "optiscaler.dll"
        || name == "reshade.ini"
        || name == "reshade.log"
        || name == "reshade64.dll"
        || name == "reshade32.dll"
        || name == "reshade64.json"
        || name == "reshadegui.ini"
        || name == "reshadepreset.ini"
        || name == "nvngx.dll_dlssnr.dll"
        || name == "nvngx_dlssnr.dll"
        || name == "dlss5-feed.cfg"
        || name == "dlss5-feed.log"
        || name == "dlss5-feed.addon64"
        || name == "dlss5-feed.addon32"
        || name == "dlss5-lab-overlay.addon64"
        || name == "renodx-dlss5.addon64"
        || name == "renodx-mfgunlock.addon64"
        || name == "rtxmfg-universal.json"
        || name == "rtx40mfg-universal.json"
        || name.starts_with("rtxmfg-")
        || name.ends_with(".addon64")
        || name.ends_with(".addon32")
        || name.ends_with(".addon")
    {
        return true;
    }
    if dest.components().any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case("OptiScaler")) {
        return true;
    }
    let hook_names = ["dxgi.dll", "winmm.dll", "d3d12.dll", "d3d11.dll", "d3d9.dll", "opengl32.dll", "dinput8.dll", "version.dll"];
    if hook_names.contains(&name.as_str()) && dest.is_file() {
        return crate::core::pe::is_optiscaler_or_proxy(dest) || crate::core::pe::is_reshade_dll(dest).0;
    }
    false
}

fn is_stale_proxy_dll(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if crate::core::pe::is_optiscaler_or_proxy(path) || crate::core::pe::is_reshade_dll(path).0 {
        return true;
    }
    if let Ok(bytes) = fs::read(path) {
        let s = String::from_utf8_lossy(&bytes).to_lowercase();
        if s.contains("reshade") || s.contains("optiscaler") {
            return true;
        }
    }
    false
}

fn remove_stale_proxy_hooks(mod_root: &Path, current_hook: &str, log: &mut Vec<String>) {
    let check_hooks = ["winmm.dll", "version.dll", "dinput8.dll", "dxgi.dll", "d3d12.dll", "d3d11.dll", "d3d9.dll", "opengl32.dll"];
    for stale_name in check_hooks {
        if !stale_name.eq_ignore_ascii_case(current_hook) {
            let stale_p = mod_root.join(stale_name);
            if is_stale_proxy_dll(&stale_p) {
                if fs::remove_file(&stale_p).is_ok() {
                    log.push(format!("[CLEANUP] Removed obsolete proxy hook: {}", stale_name));
                }
            }
        }
    }
}

pub fn clean_conflicting_route_artifacts(
    target_route: &str,
    game_dir: &Path,
    mod_root: &Path,
    mfg_unlock: bool,
    api: &str,
    log: &mut Vec<String>,
) -> std::io::Result<()> {
    let mut unique_dirs: Vec<PathBuf> = Vec::new();
    if mod_root.is_dir() {
        unique_dirs.push(mod_root.to_path_buf());
    }
    if game_dir.is_dir() && !unique_dirs.iter().any(|d| d == game_dir) {
        unique_dirs.push(game_dir.to_path_buf());
    }

    // Check if there is an existing active manifest for a DIFFERENT route
    if let Some(prev_manifest) = crate::core::journal::read_manifest(game_dir) {
        if prev_manifest.route != target_route {
            log.push(format!("[SWAP] Switching route from '{}' to '{}' - cleaning prior route artifacts", prev_manifest.route, target_route));
            for rel in &prev_manifest.added {
                let target_file = crate::core::journal::resolve_target_path(game_dir, rel);
                if target_file.is_file() {
                    let fname = target_file.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                    if is_known_mod_file(&target_file)
                        || fname.ends_with(".addon64")
                        || fname.ends_with(".addon32")
                        || fname.ends_with(".addon")
                        || fname.ends_with(".ini")
                        || fname.ends_with(".cfg")
                        || fname.ends_with(".log")
                    {
                        if fs::remove_file(&target_file).is_ok() {
                            log.push(format!("[SWAP] Purged prior route file: {}", rel));
                        }
                    }
                }
            }
            for rel_dir in prev_manifest.added_dirs.iter().rev() {
                let target_d = crate::core::journal::resolve_target_path(game_dir, rel_dir);
                if target_d.is_dir() {
                    let lower = rel_dir.to_lowercase();
                    if lower.contains("reshade") || lower.contains("optiscaler") {
                        if fs::remove_dir_all(&target_d).is_ok() {
                            log.push(format!("[SWAP] Purged prior route directory: {}", rel_dir));
                        }
                    }
                }
            }
        }
    }

    if target_route == "optiscaler" {
        // 1. Unregister Vulkan layer
        if crate::core::vulkan_layer::unregister_vulkan_layer(game_dir).unwrap_or(false) {
            log.push("[SWAP] Unregistered Vulkan implicit layer for OptiScaler deployment".to_string());
        }

        // 2. Remove ReShade, RenoDX, and Feeder files and directories
        for dir in &unique_dirs {
            let reshade_shaders = dir.join("reshade-shaders");
            if reshade_shaders.is_dir() {
                if fs::remove_dir_all(&reshade_shaders).is_ok() {
                    log.push(format!("[SWAP] Removed conflicting reshade-shaders/ from {}", dir.display()));
                }
            }

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        let lower = fname.to_lowercase();
                        let is_reshade_artifact = lower == "reshade.ini"
                            || lower == "reshadepreset.ini"
                            || lower == "reshadegui.ini"
                            || lower == "reshade.log"
                            || lower == "reshade64.dll"
                            || lower == "reshade32.dll"
                            || lower == "reshade64.json"
                            || lower == "dlss5-feed.cfg"
                            || lower == "dlss5-feed.log"
                            || lower == "dlss5-feed.addon64"
                            || lower == "dlss5-feed.addon32"
                            || lower == "dlss5-lab-overlay.addon64"
                            || lower == "renodx-dlss5.addon64"
                            || lower == "renodx-mfgunlock.addon64"
                            || lower.ends_with(".addon64")
                            || lower.ends_with(".addon32")
                            || lower.ends_with(".addon");

                        let is_reshade_hook = (lower == "dxgi.dll" || lower == "d3d11.dll" || lower == "d3d12.dll" || lower == "d3d9.dll" || lower == "opengl32.dll")
                            && (crate::core::pe::is_reshade_dll(&path).0 || {
                                if let Ok(bytes) = fs::read(&path) {
                                    let s = String::from_utf8_lossy(&bytes).to_lowercase();
                                    s.contains("reshade")
                                } else {
                                    false
                                }
                            });

                        if is_reshade_artifact || is_reshade_hook {
                            if fs::remove_file(&path).is_ok() {
                                log.push(format!("[SWAP] Cleaned conflicting ReShade file: {}", fname));
                            }
                        }
                    }
                }
            }

            // Also clean standalone MFG artifacts if mfg_unlock is false
            if !mfg_unlock {
                let ver_p = dir.join("version.dll");
                if ver_p.is_file() && (is_stale_proxy_dll(&ver_p) || crate::core::pe::is_optiscaler_or_proxy(&ver_p)) {
                    if fs::remove_file(&ver_p).is_ok() {
                        log.push("[CLEANUP] Removed standalone MFG version.dll (MFG disabled)".to_string());
                    }
                }
                let _ = fs::remove_file(dir.join("RTXMFG-Universal.json"));
                let _ = fs::remove_file(dir.join("RTX40MFG-Universal.json"));
            }
        }
    } else {
        // target_route is "feeder" or "native"
        // 1. Purge OptiScaler files and directories
        for dir in &unique_dirs {
            let opti_dir = dir.join("OptiScaler");
            if opti_dir.is_dir() {
                if fs::remove_dir_all(&opti_dir).is_ok() {
                    log.push(format!("[SWAP] Removed conflicting OptiScaler/ directory from {}", dir.display()));
                }
            }

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        let lower = fname.to_lowercase();
                        let is_optiscaler_artifact = lower == "optiscaler.ini"
                            || lower == "optiscaler.log"
                            || lower == "optiscaler.dll"
                            || lower == "nvngx.dll_dlssnr.dll"
                            || lower == "nvngx_dlssnr.dll"
                            || lower == "rtxmfg-universal.json"
                            || lower == "rtx40mfg-universal.json"
                            || (lower.starts_with("rtxmfg-") && lower.ends_with(".json"));

                        let is_opti_or_mfg_hook = (lower == "dxgi.dll" || lower == "version.dll" || lower == "d3d12.dll" || lower == "d3d11.dll" || lower == "d3d9.dll")
                            && (crate::core::pe::is_optiscaler_or_proxy(&path) || {
                                if let Ok(bytes) = fs::read(&path) {
                                    let s = String::from_utf8_lossy(&bytes).to_lowercase();
                                    s.contains("optiscaler") || s.contains("rtxmfg")
                                } else {
                                    false
                                }
                            });

                        if is_optiscaler_artifact || is_opti_or_mfg_hook {
                            if fs::remove_file(&path).is_ok() {
                                log.push(format!("[SWAP] Cleaned conflicting OptiScaler file: {}", fname));
                            }
                        }
                    }
                }
            }

            if target_route == "native" {
                let _ = fs::remove_file(dir.join("dlss5-feed.cfg"));
                let _ = fs::remove_file(dir.join("dlss5-feed.log"));
                let _ = fs::remove_file(dir.join("dlss5-feed.addon64"));
                let _ = fs::remove_file(dir.join("dlss5-feed.addon32"));
                let _ = fs::remove_file(dir.join("ReShadePreset.ini"));
                let reshade_shaders = dir.join("reshade-shaders");
                if reshade_shaders.is_dir() {
                    let _ = fs::remove_dir_all(&reshade_shaders);
                }
            } else if target_route == "feeder" {
                let _ = fs::remove_file(dir.join("dlss5-lab-overlay.addon64"));
            }
        }

        let has_vulkan_target = api.to_lowercase().contains("vulkan")
            || mod_root.read_dir().map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    let p = e.path();
                    p.is_file()
                        && p.extension().map(|ext| ext.eq_ignore_ascii_case("exe")).unwrap_or(false)
                        && crate::core::scan::detect_api_for_exe(&p).map(|a| a.to_lowercase().contains("vulkan")).unwrap_or(false)
                })
            }).unwrap_or(false);

        if !has_vulkan_target {
            let _ = crate::core::vulkan_layer::unregister_vulkan_layer(game_dir);
        }
    }

    Ok(())
}

fn carry_forward_existing_backups(
    game_dir: &Path,
    backup_dir: &Path,
    manifest: &mut ActiveManifest,
    log: &mut Vec<String>,
) {
    if let Some(prev) = crate::core::journal::read_manifest(game_dir) {
        let prev_bdir = crate::core::journal::backup_dir(game_dir);
        for item in prev.replaced {
            let old_backup_file = if let Some(ref p) = prev.backup_prefix {
                prev_bdir.join(p).join(&item.rel)
            } else {
                prev_bdir.join(&item.rel)
            };
            if old_backup_file.is_file() && !crate::core::journal::is_proxy_hook(&old_backup_file) {
                let new_backup_file = backup_dir.join(&item.rel);
                if let Some(parent) = new_backup_file.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::copy(&old_backup_file, &new_backup_file).is_ok() {
                    if !manifest.replaced.iter().any(|r| r.rel == item.rel) {
                        manifest.replaced.push(item.clone());
                        log.push(format!("[BACKUP] Preserved original vanilla backup: {}", item.rel));
                    }
                }
            }
        }
    }
}

fn track_and_copy(
    manifest: &mut ActiveManifest,
    game_dir: &Path,
    backup_dir: &Path,
    src: &Path,
    dest: &Path,
    kind: &str,
    log: &mut Vec<String>,
) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
            let rel_p = parent.strip_prefix(game_dir).unwrap_or(parent).to_string_lossy().to_string();
            if !rel_p.is_empty() && !manifest.added_dirs.contains(&rel_p) {
                manifest.added_dirs.push(rel_p);
            }
        }
    }

    let rel = dest.strip_prefix(game_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| dest.file_name().unwrap_or_default().to_string_lossy().to_string());

    if dest.exists() {
        if is_known_mod_file(dest) {
            if !manifest.added.contains(&rel) && !manifest.replaced.iter().any(|r| r.rel == rel) {
                manifest.added.push(rel.clone());
            }
        } else if !manifest.added.contains(&rel) && !manifest.replaced.iter().any(|r| r.rel == rel) {
            let backup_dest = backup_dir.join(&rel);
            if let Some(p) = backup_dest.parent() {
                fs::create_dir_all(p)?;
            }
            let _ = fs::copy(dest, &backup_dest);
            manifest.replaced.push(ManifestItem {
                rel: rel.clone(),
                old_hash: None,
                kind: Some(kind.to_string()),
            });
            log.push(format!("[BACKUP] Saved existing {} to backup", rel));
        }
    } else {
        if !manifest.added.contains(&rel) && !manifest.replaced.iter().any(|r| r.rel == rel) {
            manifest.added.push(rel.clone());
        }
    }

    fs::copy(src, dest)?;
    log.push(format!("[COPY] Deployed {}", rel));
    Ok(())
}

fn track_and_write(
    manifest: &mut ActiveManifest,
    game_dir: &Path,
    backup_dir: &Path,
    dest: &Path,
    content: &str,
    kind: &str,
    log: &mut Vec<String>,
) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let rel = dest.strip_prefix(game_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| dest.file_name().unwrap_or_default().to_string_lossy().to_string());

    if dest.exists() {
        if is_known_mod_file(dest) {
            if !manifest.added.contains(&rel) && !manifest.replaced.iter().any(|r| r.rel == rel) {
                manifest.added.push(rel.clone());
            }
        } else if !manifest.added.contains(&rel) && !manifest.replaced.iter().any(|r| r.rel == rel) {
            let backup_dest = backup_dir.join(&rel);
            if let Some(p) = backup_dest.parent() {
                fs::create_dir_all(p)?;
            }
            let _ = fs::copy(dest, &backup_dest);
            manifest.replaced.push(ManifestItem {
                rel: rel.clone(),
                old_hash: None,
                kind: Some(kind.to_string()),
            });
            log.push(format!("[BACKUP] Saved existing {} to backup", rel));
        }
    } else {
        if !manifest.added.contains(&rel) && !manifest.replaced.iter().any(|r| r.rel == rel) {
            manifest.added.push(rel.clone());
        }
    }

    fs::write(dest, content)?;
    log.push(format!("[WRITE] Configured {}", rel));
    Ok(())
}

/// Deploys Pure OptiScaler Pre-SR and Standalone 4x MFG (Universal RTXMFG v1.3.2) using provided payload bundle.
/// STRICTLY ZERO ReShade or add-on files are copied or referenced in this route.
pub fn deploy_optiscaler_with_bundle(opts: &DeployOptions, payloads: &PayloadBundle) -> Result<DeployResult, String> {
    let mut log = Vec::new();
    let mod_root = crate::core::compatibility::managed_mod_root(&opts.game_dir, Some(&opts.exe_path))
        .unwrap_or_else(|| opts.exe_path.parent().unwrap_or(&opts.game_dir).to_path_buf());

    log.push(format!("[ROUTING] Target installation directory: {}", mod_root.display()));

    crate::core::install_guards::assert_game_closed(&opts.game_dir, Some(&opts.exe_path))
        .map_err(|e| format!("Cannot deploy while game is running: {}", e))?;

    clean_conflicting_route_artifacts("optiscaler", &opts.game_dir, &mod_root, opts.mfg_unlock, &opts.api, &mut log)
        .map_err(|e| format!("Failed to clean conflicting route artifacts: {}", e))?;

    let hook_dll = if opts.api.to_lowercase().contains("9") {
        "d3d9.dll"
    } else {
        "dxgi.dll"
    };

    remove_stale_proxy_hooks(&mod_root, hook_dll, &mut log);

    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let prefix = format!("originals/{}", ts);
    let backup_dir = opts.game_dir.join("_DLSS5_Backup").join(&prefix);

    let exe_rel = opts.exe_path.strip_prefix(&opts.game_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| opts.exe_path.file_name().unwrap_or_default().to_string_lossy().to_string());

    let mut manifest = ActiveManifest {
        version: 1,
        date: crate::core::journal::now_timestamp_str(),
        route: "optiscaler".to_string(),
        game: Some(ManifestGame {
            dir: Some(opts.game_dir.to_string_lossy().to_string()),
            exe: Some(exe_rel.clone()),
            api: Some(if opts.api.to_lowercase().contains("vulkan") { "vulkan".to_string() } else { "dxgi".to_string() }),
            bitness: Some(64),
            api_label: Some(opts.api.clone()),
        }),
        game_exe: Some(exe_rel.clone()),
        backup_prefix: Some(prefix),
        replaced: Vec::new(),
        added: Vec::new(),
        added_dirs: Vec::new(),
    };

    carry_forward_existing_backups(&opts.game_dir, &backup_dir, &mut manifest, &mut log);

    // 1. Copy OptiScaler.dll as the hook DLL (dxgi.dll or d3d9.dll)
    if !payloads.optiscaler_dll.is_file() {
        return Err(format!("Missing OptiScaler.dll: {}", payloads.optiscaler_dll.display()));
    }
    track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, &payloads.optiscaler_dll, &mod_root.join(hook_dll), "optiscaler", &mut log)
        .map_err(|e| format!("Failed to copy hook DLL: {}", e))?;

    // 2. Copy nvngx_dlssnr.dll and nvngx.dll_dlssnr.dll if present
    if let Some(dlssnr_src) = &payloads.nvngx_dlssnr_dll {
        if dlssnr_src.is_file() {
            track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, dlssnr_src, &mod_root.join("nvngx_dlssnr.dll"), "runtime", &mut log)
                .map_err(|e| format!("Failed to copy nvngx_dlssnr.dll: {}", e))?;
        }
    }
    if let Some(snippet_src) = &payloads.nvngx_snippet_dll {
        if snippet_src.is_file() {
            track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, snippet_src, &mod_root.join("nvngx.dll_dlssnr.dll"), "runtime", &mut log)
                .map_err(|e| format!("Failed to copy nvngx.dll_dlssnr.dll: {}", e))?;
        }
    }

    // 3. Copy OptiScaler subfolder if present
    if let Some(opti_sub) = &payloads.optiscaler_dir {
        if opti_sub.is_dir() {
            let parent_dir = opti_sub.parent().unwrap_or(opti_sub);
            for entry in walkdir::WalkDir::new(opti_sub).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let sub_rel = entry.path().strip_prefix(parent_dir).unwrap_or_else(|_| Path::new("OptiScaler"));
                    let dest = mod_root.join(sub_rel);
                    track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, entry.path(), &dest, "optiscaler", &mut log)
                        .map_err(|e| format!("Failed to copy {}: {}", sub_rel.display(), e))?;
                }
            }
        }
    }

    // 4. Configure and write OptiScaler.ini (pure OptiScaler Pre-SR, zero ReShade)
    let base_ini_text = fs::read_to_string(&payloads.optiscaler_ini).unwrap_or_default();
    let configured_ini = configure_optiscaler_ini(&base_ini_text, &OptiScalerOptions {
        pre_sr: opts.pre_sr,
        passes: opts.passes,
        mfg_unlock: opts.mfg_unlock,
        target_exe_name: opts.exe_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
    });
    track_and_write(&mut manifest, &opts.game_dir, &backup_dir, &mod_root.join("OptiScaler.ini"), &configured_ini, "config", &mut log)
        .map_err(|e| format!("Failed to write OptiScaler.ini: {}", e))?;

    // 5. Deploy Standalone 4x MFG (Universal RTXMFG v1.3.2) as version.dll
    // Coexists with OptiScaler via [FrameGen] External=true as documented in RTX40-MFG.md:
    // OptiScaler yields Streamline hooks and passes FrameGen to Dashdogy's RTXMFG.
    if opts.mfg_unlock {
        if let Some(mfg_dll) = &payloads.rtxmfg_dll {
            if mfg_dll.is_file() {
                let version_dest = mod_root.join("version.dll");
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, mfg_dll, &version_dest, "mfg", &mut log)
                    .map_err(|e| format!("Failed to deploy standalone RTXMFG as version.dll: {}", e))?;
                log.push(format!("[MFG] Standalone Universal RTXMFG v1.3.2 deployed as version.dll ({}x multiplier active)", opts.mfg_multiplier));

                let mult_key = if opts.mfg_multiplier >= 2 { opts.mfg_multiplier - 1 } else { 1 };
                let mfg_config = format!(
                    "{{\n  \"mode\": \"fixed\",\n  \"multiplier\": {},\n  \"dlssgPreset\": 2,\n  \"followGame\": false,\n  \"dynamicTargetFrameRate\": 0,\n  \"dynamicExperimental56\": false,\n  \"selectiveOtaDlssgWrapper\": false\n}}\n",
                    mult_key
                );
                let json_dest1 = mod_root.join("RTXMFG-Universal.json");
                track_and_write(&mut manifest, &opts.game_dir, &backup_dir, &json_dest1, &mfg_config, "mfg_config", &mut log)
                    .map_err(|e| format!("Failed to write RTXMFG-Universal.json: {}", e))?;
                let json_dest2 = mod_root.join("RTX40MFG-Universal.json");
                track_and_write(&mut manifest, &opts.game_dir, &backup_dir, &json_dest2, &mfg_config, "mfg_config", &mut log)
                    .map_err(|e| format!("Failed to write RTX40MFG-Universal.json: {}", e))?;
                log.push(format!("[MFG] Configured RTXMFG-Universal.json (multiplier: {}x, fixed mode)", opts.mfg_multiplier));
            } else {
                log.push("[WARNING] RTXMFG.dll not found on system - skipping standalone MFG".to_string());
            }
        } else {
            log.push("[WARNING] Standalone RTXMFG payload unavailable - skipping standalone MFG".to_string());
        }
    }

    // 6. Deploy verified Streamline 2.14.1 stack if game has Streamline
    // Unifies local Streamline with driver OTA to permanently eliminate the 0xC0000005 crash in sl.reflex (190_E658703.dll)
    if mod_root.join("sl.interposer.dll").exists() {
        if let Some(streamline_src) = &payloads.streamline_dir {
            if streamline_src.is_dir() {
                let sl_files = [
                    "sl.interposer.dll",
                    "sl.common.dll",
                    "sl.dlss_g.dll",
                    "sl.reflex.dll",
                    "sl.pcl.dll",
                    "nvngx_dlssg.dll",
                ];
                for f in &sl_files {
                    let src_file = streamline_src.join(f);
                    if src_file.is_file() {
                        let dest_file = mod_root.join(f);
                        track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, &src_file, &dest_file, "streamline", &mut log)
                            .map_err(|e| format!("Failed to deploy Streamline runtime {}: {}", f, e))?;
                    }
                }
                log.push("[STREAMLINE] Deployed verified Streamline 2.14.1 stack to eliminate OTA ABI version conflicts".to_string());
            }
        }
    }

    // 6. Save manifest and history
    save_manifest(&opts.game_dir, &manifest)
        .map_err(|e| format!("Failed to save manifest: {}", e))?;

    let replaced = manifest.replaced.len();
    let added = manifest.added.len();

    let _ = append_history(&HistoryRow {
        date: crate::core::journal::now_timestamp_str(),
        dir: opts.game_dir.to_string_lossy().to_string(),
        game_name: opts.game_name.clone(),
        action: "install".to_string(),
        replaced,
        added,
    });

    log.push(format!("[COMPLETE] Successfully installed OptiScaler Pre-SR (Passes: {}, MFG: {})! {} files replaced, {} added.",
        opts.passes,
        if opts.mfg_unlock { "4x standalone" } else { "disabled" },
        replaced,
        added
    ));

    Ok(DeployResult {
        success: true,
        log_lines: log,
        replaced,
        added,
    })
}

/// Fully deploys Pure OptiScaler Pre-SR and Standalone 4x MFG using system-resolved payloads.
pub fn deploy_optiscaler(opts: &DeployOptions) -> Result<DeployResult, String> {
    let payloads = PayloadBundle::from_system()?;
    deploy_optiscaler_with_bundle(opts, &payloads)
}

/// Deploys Native DLSS 5 route (ReShade + RenoDX + ReShade 4x MFG Unlock).
/// STRICTLY ZERO OptiScaler files are deployed in this route.
pub fn deploy_native_dlss5_with_bundle(opts: &DeployOptions, payloads: &PayloadBundle) -> Result<DeployResult, String> {
    let mut log = Vec::new();
    let mod_root = crate::core::compatibility::managed_mod_root(&opts.game_dir, Some(&opts.exe_path))
        .unwrap_or_else(|| opts.exe_path.parent().unwrap_or(&opts.game_dir).to_path_buf());

    log.push(format!("[ROUTING] Native DLSS (RenoDX) target mod directory: {}", mod_root.display()));

    crate::core::install_guards::assert_game_closed(&opts.game_dir, Some(&opts.exe_path))
        .map_err(|e| format!("Cannot deploy while game is running: {}", e))?;

    clean_conflicting_route_artifacts("native", &opts.game_dir, &mod_root, opts.mfg_unlock, &opts.api, &mut log)
        .map_err(|e| format!("Failed to clean conflicting route artifacts: {}", e))?;

    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let prefix = format!("originals/{}", ts);
    let backup_dir = opts.game_dir.join("_DLSS5_Backup").join(&prefix);

    let exe_rel = opts.exe_path.strip_prefix(&opts.game_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| opts.exe_path.file_name().unwrap_or_default().to_string_lossy().to_string());

    let mut manifest = ActiveManifest {
        version: 1,
        date: crate::core::journal::now_timestamp_str(),
        route: "native".to_string(),
        game: Some(ManifestGame {
            dir: Some(opts.game_dir.to_string_lossy().to_string()),
            exe: Some(exe_rel.clone()),
            api: Some(if opts.api.to_lowercase().contains("vulkan") { "vulkan".to_string() } else { "dxgi".to_string() }),
            bitness: Some(64),
            api_label: Some(opts.api.clone()),
        }),
        game_exe: Some(exe_rel.clone()),
        backup_prefix: Some(prefix),
        replaced: Vec::new(),
        added: Vec::new(),
        added_dirs: Vec::new(),
    };

    carry_forward_existing_backups(&opts.game_dir, &backup_dir, &mut manifest, &mut log);

    let hook_dll = if opts.api.to_lowercase().contains("9") {
        "d3d9.dll"
    } else if opts.api.to_lowercase().contains("opengl") {
        "opengl32.dll"
    } else {
        "dxgi.dll"
    };

    remove_stale_proxy_hooks(&mod_root, hook_dll, &mut log);

    // 1. Deploy ReShade64.dll as hook DLL
    if let Some(reshade_src) = &payloads.reshade64_dll {
        if reshade_src.is_file() {
            let dest_hook = mod_root.join(hook_dll);
            track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, reshade_src, &dest_hook, "reshade", &mut log)
                .map_err(|e| format!("Failed to deploy {}: {}", hook_dll, e))?;
            log.push(format!("[HOOK] ReShade deployed as {} for RenoDX add-on execution", hook_dll));
        } else {
            return Err(format!("ReShade64.dll payload file missing: {}", reshade_src.display()));
        }
    } else {
        return Err("ReShade64.dll payload not found on system".to_string());
    }

    let state = crate::core::state::load_state();
    let renodx_active = crate::core::state::is_addon_active(&state, "builtin:renodx");
    let mfg_active = crate::core::state::is_addon_active(&state, "builtin:mfgunlock");

    // 2. Deploy RenoDX v4.7 DLSS 5 add-on if active
    let mut deployed_addon_stems: Vec<String> = Vec::new();
    if renodx_active {
        if let Some(renodx_src) = &payloads.renodx_dlss5_addon {
            if renodx_src.is_file() {
                let dest = mod_root.join("renodx-dlss5.addon64");
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, renodx_src, &dest, "addon", &mut log)
                    .map_err(|e| format!("Failed to copy renodx-dlss5.addon64: {}", e))?;
                deployed_addon_stems.push("renodx-dlss5".to_string());
                log.push("[ADDON] renodx-dlss5.addon64 deployed (RenoDX v4.7 Integrated DLSS 5 Engine)".to_string());
            }
        }

        // Deploy nvngx_dlssnr.dll required by RenoDX DLSS 5 Neural Rendering engine
        if let Some(dlssnr_src) = &payloads.nvngx_dlssnr_dll {
            if dlssnr_src.is_file() {
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, dlssnr_src, &mod_root.join("nvngx_dlssnr.dll"), "runtime", &mut log)
                    .map_err(|e| format!("Failed to copy nvngx_dlssnr.dll: {}", e))?;
                log.push("[RUNTIME] nvngx_dlssnr.dll deployed for RenoDX DLSS 5 Neural Rendering".to_string());
            }
        }
    }

    // Note: DLSS 5 In-Game Overlay add-on is shelved for now - strictly zero overlay deployment

    // 3. Deploy ReShade RenoDX 4x MFG Unlock if requested and active
    if opts.mfg_unlock && mfg_active {
        if let Some(mfg_src) = &payloads.renodx_mfgunlock_addon {
            if mfg_src.is_file() {
                let dest = mod_root.join("renodx-mfgunlock.addon64");
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, mfg_src, &dest, "addon", &mut log)
                    .map_err(|e| format!("Failed to copy renodx-mfgunlock.addon64: {}", e))?;
                deployed_addon_stems.push("renodx-mfgunlock".to_string());
                log.push("[MFG] renodx-mfgunlock.addon64 deployed (ReShade 4x MFG Unlock)".to_string());
            }
        }
    }

    // 3b. Deploy active user-imported custom add-ons
    for custom in &state.addon_files {
        if state.addons.contains(&custom.path) {
            let custom_path = PathBuf::from(&custom.path);
            if custom_path.is_file() {
                let file_name = custom_path.file_name().and_then(|n| n.to_str()).unwrap_or("custom.addon64");
                let dest = mod_root.join(file_name);
                if track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, &custom_path, &dest, "addon", &mut log).is_ok() {
                    let stem = custom_path.file_stem().and_then(|s| s.to_str()).unwrap_or(file_name).to_string();
                    deployed_addon_stems.push(stem);
                    let display_name = custom.name.as_deref().unwrap_or(file_name);
                    log.push(format!("[ADDON] {} deployed ({})", file_name, display_name));
                }
            }
        }
    }

    // 4. Configure ReShade.ini
    let reshade_ini_path = mod_root.join("ReShade.ini");
    let existing_reshade_ini = fs::read_to_string(&reshade_ini_path).unwrap_or_default();
    let mut configured_reshade_ini = if opts.mfg_unlock {
        configure_mfg_unlock_ini(&existing_reshade_ini, Some(opts.mfg_multiplier))
    } else {
        existing_reshade_ini
    };
    configured_reshade_ini = set_ini(&configured_reshade_ini, "INPUT", "KeyOverlay", "36,0,0,0");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "OVERLAY", "TutorialProgress", "4");

    if let Some(disabled) = get_ini(&configured_reshade_ini, "ADDON", "DisabledAddons") {
        let kept: Vec<&str> = disabled.split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !deployed_addon_stems.iter().any(|d| d.eq_ignore_ascii_case(s)))
            .collect();
        configured_reshade_ini = set_ini(&configured_reshade_ini, "ADDON", "DisabledAddons", &kept.join(","));
    }

    track_and_write(&mut manifest, &opts.game_dir, &backup_dir, &reshade_ini_path, &configured_reshade_ini, "config", &mut log)
        .map_err(|e| format!("Failed to write ReShade.ini: {}", e))?;

    // 5. Save manifest and history
    save_manifest(&opts.game_dir, &manifest)
        .map_err(|e| format!("Failed to save manifest: {}", e))?;

    let replaced = manifest.replaced.len();
    let added = manifest.added.len();

    let _ = append_history(&HistoryRow {
        date: crate::core::journal::now_timestamp_str(),
        dir: opts.game_dir.to_string_lossy().to_string(),
        game_name: opts.game_name.clone(),
        action: "install_native".to_string(),
        replaced,
        added,
    });

    let mfg_desc = if opts.mfg_unlock {
        format!("{}x companion add-on", opts.mfg_multiplier)
    } else {
        "disabled".to_string()
    };
    log.push(format!("[COMPLETE] Successfully installed ReShade + RenoDX (MFG: {})! {} files replaced, {} added.",
        mfg_desc,
        replaced,
        added
    ));

    Ok(DeployResult {
        success: true,
        log_lines: log,
        replaced,
        added,
    })
}

/// Fully deploys ReShade + RenoDX route using system-resolved payloads.
pub fn deploy_native_dlss5(opts: &DeployOptions) -> Result<DeployResult, String> {
    let payloads = PayloadBundle::from_system()?;
    deploy_native_dlss5_with_bundle(opts, &payloads)
}

/// Formats or updates ReShadePreset.ini for DLSS5-Feeder route.
/// In ReShade preset syntax, Techniques= and TechniqueSorting= MUST live at the root of the file
/// before ANY section header (e.g. [DLSS5_Feed.fx]). Placing them under [ReShadePreset.ini] causes
/// ReShade to parse 0 active techniques and skips execution of DLSS5_Feed.fx and vort_Motion.fx.
pub fn configure_feeder_preset(existing: &str) -> String {
    let required_techs = ["vort_MotionEffects@vort_Motion.fx", "DLSS5_Feed@DLSS5_Feed.fx"];

    if existing.trim().is_empty() {
        return format!(
            "Techniques={}\nTechniqueSorting={}\nPreprocessorDefinitions=DLSS5_MV_PROVIDER=2\n\n[DLSS5_Feed.fx]\nPreprocessorDefinitions=DLSS5_MV_PROVIDER=2\n",
            required_techs.join(","),
            required_techs.join(",")
        );
    }

    // Strip legacy erroneous [ReShadePreset.ini] header if present
    let raw_lines: Vec<&str> = existing
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case("[ReShadePreset.ini]"))
        .collect();

    // Partition root lines (before the first [section]) and section lines
    let mut root_lines = Vec::new();
    let mut section_lines = Vec::new();
    let mut in_section = false;

    for line in raw_lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = true;
        }
        if in_section {
            section_lines.push(line);
        } else {
            root_lines.push(line);
        }
    }

    let mut techniques: Vec<String> = required_techs.iter().map(|s| s.to_string()).collect();
    let mut sorting: Vec<String> = required_techs.iter().map(|s| s.to_string()).collect();
    let mut preprocessors = "DLSS5_MV_PROVIDER=2".to_string();
    let mut other_root = Vec::new();

    for line in root_lines {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            if key.eq_ignore_ascii_case("Techniques") {
                for t in val.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if !techniques.iter().any(|req| req.eq_ignore_ascii_case(t)) {
                        techniques.push(t.to_string());
                    }
                }
            } else if key.eq_ignore_ascii_case("TechniqueSorting") {
                for t in val.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if !sorting.iter().any(|req| req.eq_ignore_ascii_case(t)) {
                        sorting.push(t.to_string());
                    }
                }
            } else if key.eq_ignore_ascii_case("PreprocessorDefinitions") {
                if !val.contains("DLSS5_MV_PROVIDER") {
                    preprocessors = format!("{},{}", val, preprocessors);
                } else {
                    preprocessors = val.to_string();
                }
            } else {
                other_root.push(line.to_string());
            }
        } else if !trimmed.is_empty() {
            other_root.push(line.to_string());
        }
    }

    let mut out = String::new();
    out.push_str(&format!("Techniques={}\n", techniques.join(",")));
    out.push_str(&format!("TechniqueSorting={}\n", sorting.join(",")));
    out.push_str(&format!("PreprocessorDefinitions={}\n", preprocessors));
    for line in other_root {
        out.push_str(&line);
        out.push('\n');
    }
    if !section_lines.is_empty() {
        out.push('\n');
        for line in section_lines {
            out.push_str(line);
            out.push('\n');
        }
    }

    if !out.contains("[DLSS5_Feed.fx]") {
        out.push_str("\n[DLSS5_Feed.fx]\nPreprocessorDefinitions=DLSS5_MV_PROVIDER=2\n");
    }

    out
}

/// Deploys DLSS5-Feeder route using provided payload bundle.
/// STRICTLY ZERO Pre-SR, ZERO MFG, ZERO OptiScaler files are deployed in this route.
pub fn deploy_feeder_with_bundle(opts: &DeployOptions, payloads: &PayloadBundle) -> Result<DeployResult, String> {
    let mut log = Vec::new();
    let mod_root = crate::core::compatibility::managed_mod_root(&opts.game_dir, Some(&opts.exe_path))
        .unwrap_or_else(|| opts.exe_path.parent().unwrap_or(&opts.game_dir).to_path_buf());

    log.push(format!("[ROUTING] DLSS5-Feeder target mod directory: {}", mod_root.display()));

    crate::core::install_guards::assert_game_closed(&opts.game_dir, Some(&opts.exe_path))
        .map_err(|e| format!("Cannot deploy while game is running: {}", e))?;

    clean_conflicting_route_artifacts("feeder", &opts.game_dir, &mod_root, opts.mfg_unlock, &opts.api, &mut log)
        .map_err(|e| format!("Failed to clean conflicting route artifacts: {}", e))?;

    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let prefix = format!("originals/{}", ts);
    let backup_dir = opts.game_dir.join("_DLSS5_Backup").join(&prefix);

    let exe_rel = opts.exe_path.strip_prefix(&opts.game_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| opts.exe_path.file_name().unwrap_or_default().to_string_lossy().to_string());

    let mut manifest = ActiveManifest {
        version: 1,
        date: crate::core::journal::now_timestamp_str(),
        route: "feeder".to_string(),
        game: Some(ManifestGame {
            dir: Some(opts.game_dir.to_string_lossy().to_string()),
            exe: Some(exe_rel.clone()),
            api: Some(if opts.api.to_lowercase().contains("vulkan") { "vulkan".to_string() } else { "dxgi".to_string() }),
            bitness: Some(64),
            api_label: Some(opts.api.clone()),
        }),
        game_exe: Some(exe_rel.clone()),
        backup_prefix: Some(prefix),
        replaced: Vec::new(),
        added: Vec::new(),
        added_dirs: Vec::new(),
    };

    carry_forward_existing_backups(&opts.game_dir, &backup_dir, &mut manifest, &mut log);

    let hook_dll = if opts.api.to_lowercase().contains("9") {
        "d3d9.dll"
    } else if opts.api.to_lowercase().contains("opengl") {
        "opengl32.dll"
    } else {
        "dxgi.dll"
    };

    remove_stale_proxy_hooks(&mod_root, hook_dll, &mut log);

    // If game is Vulkan or contains any Vulkan executables, register the Vulkan implicit layer
    let has_vulkan_target = opts.api.to_lowercase().contains("vulkan")
        || mod_root.read_dir().map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let p = e.path();
                p.is_file()
                    && p.extension().map(|ext| ext.eq_ignore_ascii_case("exe")).unwrap_or(false)
                    && crate::core::scan::detect_api_for_exe(&p).map(|a| a.to_lowercase().contains("vulkan")).unwrap_or(false)
            })
        }).unwrap_or(false);

    if has_vulkan_target {
        let vk_dir = payloads.feeder_components.as_ref().and_then(|fc| fc.vk_layer_dir.as_deref());
        match crate::core::vulkan_layer::register_vulkan_layer(&opts.game_dir, vk_dir, payloads.reshade64_dll.as_deref()) {
            Ok(reg_p) => log.push(format!("[VULKAN] Registered Vulkan implicit layer ({})", reg_p.display())),
            Err(e) => log.push(format!("[WARN] Vulkan layer registration: {}", e)),
        }
    }

    // 1. Deploy ReShade64.dll as hook DLL
    if let Some(reshade_src) = &payloads.reshade64_dll {
        if reshade_src.is_file() {
            let dest_hook = mod_root.join(hook_dll);
            track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, reshade_src, &dest_hook, "reshade", &mut log)
                .map_err(|e| format!("Failed to deploy {}: {}", hook_dll, e))?;
            log.push(format!("[FEEDER] ReShade deployed as {} for frame/depth buffer capture", hook_dll));
        } else {
            return Err(format!("ReShade64.dll payload file missing: {}", reshade_src.display()));
        }
    } else {
        return Err("ReShade64.dll payload not found on system".to_string());
    }

    let state = crate::core::state::load_state();
    let renodx_active = crate::core::state::is_addon_active(&state, "builtin:renodx");
    let mfg_active = crate::core::state::is_addon_active(&state, "builtin:mfgunlock");

    let mut deployed_addon_stems: Vec<String> = Vec::new();

    // 2. Deploy dlss5-feed addon & cfg if available
    let bitness = crate::core::pe::inspect_pe(&opts.exe_path).map(|p| p.bitness).unwrap_or(64);
    if let Some(fc) = &payloads.feeder_components {
            let addon_src = if bitness == 32 {
                fc.addon32.as_ref().unwrap_or(&fc.addon64)
            } else {
                &fc.addon64
            };
            if addon_src.is_file() {
                let dest = mod_root.join(if bitness == 32 { "dlss5-feed.addon32" } else { "dlss5-feed.addon64" });
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, addon_src, &dest, "feeder", &mut log)
                    .map_err(|e| format!("Failed to copy dlss5-feed addon: {}", e))?;
                deployed_addon_stems.push("dlss5-feed".to_string());
                log.push("[FEEDER] dlss5-feed addon deployed for frame, depth & optical flow capture".to_string());
            }

            // Deploy reshade-shaders tree
            if fc.shader_dir.is_dir() {
                let target_root = mod_root.join("reshade-shaders");
                for entry in walkdir::WalkDir::new(&fc.shader_dir).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_file() {
                        if let Ok(rel) = entry.path().strip_prefix(&fc.shader_dir) {
                            let dest = target_root.join(rel);
                            track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, entry.path(), &dest, "shader", &mut log)
                                .map_err(|e| format!("Failed to copy shader {}: {}", rel.display(), e))?;
                        }
                    }
                }
                log.push("[SHADERS] Deployed DLSS5-Feeder optical flow and depth shaders".to_string());
            }
        }

    // Deploy dlss5-feed.cfg
    let cfg_content = "enabled=1\nmode=2\nhdr=-1\ndepth_inverted=-1\nflags=-1\nreset_every=0\nwarmup_rebuild=180\nrebuild=0\nlog_frames=3\ncreate_delay=60\npreset=0\nwork_resolution=100\nmv_scale_x=1.000\nmv_scale_y=1.000\nhost_window=0\nasync_home=1\n";
    let cfg_path = mod_root.join("dlss5-feed.cfg");
    track_and_write(&mut manifest, &opts.game_dir, &backup_dir, &cfg_path, cfg_content, "config", &mut log)
        .map_err(|e| format!("Failed to write dlss5-feed.cfg: {}", e))?;

    // Deploy ReShadePreset.ini configured for Feeder shaders.
    // In ReShade preset syntax, Techniques= and TechniqueSorting= MUST live at the root of the file
    // before any section header, otherwise ReShade parses 0 active techniques and skips execution.
    let preset_path = mod_root.join("ReShadePreset.ini");
    let existing_preset = fs::read_to_string(&preset_path).unwrap_or_default();
    let preset_content = configure_feeder_preset(&existing_preset);
    track_and_write(&mut manifest, &opts.game_dir, &backup_dir, &preset_path, &preset_content, "config", &mut log)
        .map_err(|e| format!("Failed to write ReShadePreset.ini: {}", e))?;

    // 3. Deploy RenoDX DLSS 5 Engine & Neural Rendering runtime
    if renodx_active {
        if let Some(renodx_src) = &payloads.renodx_dlss5_addon {
            if renodx_src.is_file() {
                let dest = mod_root.join("renodx-dlss5.addon64");
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, renodx_src, &dest, "addon", &mut log)
                    .map_err(|e| format!("Failed to copy renodx-dlss5.addon64: {}", e))?;
                deployed_addon_stems.push("renodx-dlss5".to_string());
                log.push("[FEEDER-DLSS5] renodx-dlss5.addon64 deployed for Streamline Neural Rendering".to_string());
            }
        }

        if let Some(dlssnr_src) = &payloads.nvngx_dlssnr_dll {
            if dlssnr_src.is_file() {
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, dlssnr_src, &mod_root.join("nvngx_dlssnr.dll"), "runtime", &mut log)
                    .map_err(|e| format!("Failed to copy nvngx_dlssnr.dll: {}", e))?;
                log.push("[RUNTIME] nvngx_dlssnr.dll deployed for Streamline Feeder Neural Rendering".to_string());
            }
        }
    }

    // Deploy / upgrade modern nvngx_dlss.dll runtime so dlss5-feed D3D12 session can initialize
    // NGX Super Sampling and CreateFeature without error.
    if bitness == 64 {
        if let Some(dlss_src) = &payloads.nvngx_dlss_dll {
            if dlss_src.is_file() {
                let target_path = if mod_root.join("nvngx_dlss.dll").is_file() {
                    mod_root.join("nvngx_dlss.dll")
                } else if opts.game_dir.join("nvngx_dlss.dll").is_file() {
                    opts.game_dir.join("nvngx_dlss.dll")
                } else {
                    mod_root.join("nvngx_dlss.dll")
                };

                let existing_ver = crate::core::pe::inspect_pe(&target_path).and_then(|p| p.version);
                let payload_ver = crate::core::pe::inspect_pe(dlss_src).and_then(|p| p.version);
                let should_copy = match (existing_ver, payload_ver) {
                    (Some(e), Some(p)) => e != p,
                    _ => true,
                };

                if should_copy {
                    track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, dlss_src, &target_path, "runtime", &mut log)
                        .map_err(|e| format!("Failed to deploy/upgrade nvngx_dlss.dll: {}", e))?;
                    log.push("[RUNTIME] Deployed/upgraded modern nvngx_dlss.dll for Streamline Feeder Super Sampling".to_string());
                }
            }
        }
    }

    // 4. Deploy Streamline Feeder addons when MFG is enabled (supported on 64-bit titles with native DLSS-G, DX12/DXGI, or Vulkan)
    let api_lower = opts.api.to_lowercase();
    let is_dx11 = api_lower.contains("11") || api_lower == "d3d11";
    let is_vulkan = api_lower.contains("vulkan");
    let is_dx12 = api_lower.contains("12") || api_lower.contains("d3d12") || api_lower.contains("dxgi");
    let has_native_dlssg = mod_root.join("nvngx_dlssg.dll").is_file()
        || mod_root.join("sl.dlss_g.dll").is_file()
        || opts.game_dir.join("nvngx_dlssg.dll").is_file();
    let api_supports_fg = bitness == 64 && (has_native_dlssg || is_vulkan || is_dx12) && !is_dx11;
    if opts.mfg_unlock && api_supports_fg && mfg_active {
        if let Some(mfg_src) = &payloads.renodx_mfgunlock_addon {
            if mfg_src.is_file() {
                let dest = mod_root.join("renodx-mfgunlock.addon64");
                track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, mfg_src, &dest, "addon", &mut log)
                    .map_err(|e| format!("Failed to copy renodx-mfgunlock.addon64: {}", e))?;
                deployed_addon_stems.push("renodx-mfgunlock".to_string());
                log.push(format!("[FEEDER-MFG] renodx-mfgunlock.addon64 deployed (Streamline {}x MFG Unlock)", opts.mfg_multiplier));
            }
        }
    }

    // 4b. Deploy active user-imported custom add-ons
    for custom in &state.addon_files {
        if state.addons.contains(&custom.path) {
            let custom_path = PathBuf::from(&custom.path);
            if custom_path.is_file() {
                let file_name = custom_path.file_name().and_then(|n| n.to_str()).unwrap_or("custom.addon64");
                let dest = mod_root.join(file_name);
                if track_and_copy(&mut manifest, &opts.game_dir, &backup_dir, &custom_path, &dest, "addon", &mut log).is_ok() {
                    let stem = custom_path.file_stem().and_then(|s| s.to_str()).unwrap_or(file_name).to_string();
                    deployed_addon_stems.push(stem);
                    let display_name = custom.name.as_deref().unwrap_or(file_name);
                    log.push(format!("[ADDON] {} deployed ({})", file_name, display_name));
                }
            }
        }
    }

    // 5. Configure ReShade.ini for Feeder
    let reshade_ini_path = mod_root.join("ReShade.ini");
    let existing_reshade_ini = fs::read_to_string(&reshade_ini_path).unwrap_or_default();
    let mut configured_reshade_ini = if opts.mfg_unlock && api_supports_fg {
        configure_mfg_unlock_ini(&existing_reshade_ini, Some(opts.mfg_multiplier))
    } else {
        existing_reshade_ini
    };
    configured_reshade_ini = set_ini(&configured_reshade_ini, "INPUT", "KeyOverlay", "36,0,0,0");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "OVERLAY", "TutorialProgress", "4");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "GENERAL", "EffectSearchPaths", ".\\reshade-shaders\\Shaders\\**");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "GENERAL", "TextureSearchPaths", ".\\reshade-shaders\\Textures\\**");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "GENERAL", "PresetPath", ".\\ReShadePreset.ini");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "GENERAL", "StartupPresetPath", "");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "GENERAL", "NoReloadOnInit", "0");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "GENERAL", "PreprocessorDefinitions", "DLSS5_MV_PROVIDER=2");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "ADDON", "AddonPath", ".\\");
    // EnableHooks=1 allows RenoDX to hook swapchain and direct presentation paths when native D3D12 NGX
    // is absent or bypassed (such as under DX11, Vulkan, or Feeder routes per AGENTS.md).
    configured_reshade_ini = set_ini(&configured_reshade_ini, "RenoDX.DLSS5", "EnableHooks", "1");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "RenoDX.DLSS5", "NeuralUplift", "0");
    configured_reshade_ini = set_ini(&configured_reshade_ini, "RenoDX.DLSS5", "NREnableUpscaling", "0");

    if let Some(disabled) = get_ini(&configured_reshade_ini, "ADDON", "DisabledAddons") {
        let kept: Vec<&str> = disabled.split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !deployed_addon_stems.iter().any(|d| d.eq_ignore_ascii_case(s)))
            .collect();
        configured_reshade_ini = set_ini(&configured_reshade_ini, "ADDON", "DisabledAddons", &kept.join(","));
    }

    track_and_write(&mut manifest, &opts.game_dir, &backup_dir, &reshade_ini_path, &configured_reshade_ini, "config", &mut log)
        .map_err(|e| format!("Failed to write ReShade.ini: {}", e))?;

    // 3. Save manifest and history
    save_manifest(&opts.game_dir, &manifest)
        .map_err(|e| format!("Failed to save manifest: {}", e))?;

    let replaced = manifest.replaced.len();
    let added = manifest.added.len();

    let _ = append_history(&HistoryRow {
        date: crate::core::journal::now_timestamp_str(),
        dir: opts.game_dir.to_string_lossy().to_string(),
        game_name: opts.game_name.clone(),
        action: "install_feeder".to_string(),
        replaced,
        added,
    });

    let mfg_desc = if opts.mfg_unlock {
        format!("{}x Streamline Feeder companion add-on", opts.mfg_multiplier)
    } else {
        "disabled".to_string()
    };
    log.push(format!("[COMPLETE] Successfully installed DLSS5-Feeder (MFG: {})! {} files replaced, {} added.",
        mfg_desc,
        replaced,
        added
    ));

    Ok(DeployResult {
        success: true,
        log_lines: log,
        replaced,
        added,
    })
}

/// Fully deploys DLSS5-Feeder route using system-resolved payloads.
pub fn deploy_feeder(opts: &DeployOptions) -> Result<DeployResult, String> {
    let mut payloads = PayloadBundle::from_system()?;
    if payloads.feeder_components.is_none() {
        let mut download_log = Vec::new();
        let fc = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                tokio::task::block_in_place(|| {
                    handle.block_on(crate::core::downloader::ensure_feeder_components(&mut download_log))
                })
            }
            Err(_) => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("Tokio runtime error: {}", e))?;
                rt.block_on(crate::core::downloader::ensure_feeder_components(&mut download_log))
            }
        }?;
        payloads.feeder_components = Some(fc);
    }
    deploy_feeder_with_bundle(opts, &payloads)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub use crate::core::state::STATE_TEST_MUTEX;

    #[test]
    fn test_set_ini() {
        let base = "[DlssNr]\nRunBeforeSR=false\nPasses=1\n";
        let updated = set_ini(base, "DlssNr", "RunBeforeSR", "true");
        assert!(updated.contains("RunBeforeSR=true"));
        let updated2 = set_ini(&updated, "DlssNr", "Passes", "3");
        assert!(updated2.contains("Passes=3"));
    }

    #[test]
    fn test_find_overlay_addon_payload() {
        let p = find_overlay_addon_payload();
        assert!(p.is_some(), "In-game overlay addon payload must always be discoverable or materialized");
        let path = p.unwrap();
        assert!(path.exists(), "Materialized overlay addon must exist on disk");

        // Verify it is a valid 64-bit ReShade addon payload ready for deployment
        let metadata = fs::metadata(&path).expect("metadata must be readable");
        assert!(
            metadata.len() >= 50_000,
            "Overlay addon must be valid 64-bit payload, found size: {}",
            metadata.len()
        );
    }


    #[test]
    fn test_find_payloads() {
        if find_optiscaler_payload().is_none() {
            println!("OptiScaler payload not installed on this runner; skipping payload discovery test.");
            return;
        }
        assert!(find_optiscaler_payload().is_some(), "OptiScaler payload should be found");
        assert!(find_reshade64_payload().is_some(), "ReShade64 payload should be found");
        assert!(find_mfg_addon_payload().is_some(), "MFG addon payload should be found");
        assert!(find_dlssnr_payload().is_some(), "DLSS-NR payload should be found");
        assert!(find_standalone_mfg_payload().is_some(), "Standalone RTXMFG payload should be found");
    }
    #[test]
    fn test_deploy_and_restore() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        if find_optiscaler_payload().is_none() {
            println!("OptiScaler payload not installed on this runner; skipping deploy and restore test.");
            return;
        }
        let temp_dir = std::env::temp_dir().join(format!("dlss_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let content_dir = temp_dir.join("Content");
        fs::create_dir_all(&content_dir).unwrap();

        let orig_dxgi = content_dir.join("dxgi.dll");
        fs::write(&orig_dxgi, b"ORIGINAL_DXGI").unwrap();

        let exe_path = content_dir.join("Resonance.exe");
        fs::write(&exe_path, b"DUMMY_EXE").unwrap();

        let opts = DeployOptions {
            game_name: Some("Test Game".to_string()),
            game_dir: content_dir.clone(),
            exe_path: exe_path.clone(),
            api: "dxgi".to_string(),
            pre_sr: true,
            passes: 2,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_optiscaler(&opts).expect("deploy should succeed");
        assert!(res.success);
        assert_eq!(res.replaced, 1);
        assert!(res.added >= 2);

        // Verify deployed files
        assert!(orig_dxgi.exists());
        assert_ne!(fs::read(&orig_dxgi).unwrap(), b"ORIGINAL_DXGI");

        let opti_ini = fs::read_to_string(content_dir.join("OptiScaler.ini")).unwrap();
        assert!(opti_ini.contains("RunBeforeSR=true"));
        assert!(opti_ini.contains("Passes=2"));
        assert!(opti_ini.contains("LoadReshade=false"));
        assert!(opti_ini.contains("TargetProcessName=Resonance.exe"));

        // Standalone MFG as version.dll
        assert!(content_dir.join("version.dll").exists());
        // Pure OptiScaler - zero ReShade files
        assert!(!content_dir.join("ReShade64.dll").exists());
        assert!(!content_dir.join("renodx-mfgunlock.addon64").exists());
        assert!(content_dir.join("_DLSS5_Backup").join("manifest.json").exists());

        // Now test restore
        let rest = crate::core::journal::restore_game(&content_dir).expect("restore should succeed");
        assert!(rest);

        // Verify clean state
        assert_eq!(fs::read(&orig_dxgi).unwrap(), b"ORIGINAL_DXGI");
        assert!(!content_dir.join("version.dll").exists());
        assert!(!content_dir.join("ReShade64.dll").exists());
        assert!(!content_dir.join("renodx-mfgunlock.addon64").exists());
        assert!(!content_dir.join("_DLSS5_Backup").join("manifest.json").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_addon_state_helpers() {
        use crate::core::state::*;
        let mut state = AppState::default();
        assert!(is_addon_active(&state, "builtin:renodx"));
        assert!(is_addon_active(&state, "builtin:mfgunlock"));
        assert!(is_addon_active(&state, "builtin:feeder"));
        assert!(!is_addon_active(&state, "builtin:overlay"));

        // Mandatory base add-ons cannot be deactivated
        toggle_addon_in_state(&mut state, "builtin:renodx", false);
        assert!(is_addon_active(&state, "builtin:renodx"), "Base add-ons are mandatory");

        let custom = AddonFileEntry {
            path: "C:\\mods\\my_addon.addon64".to_string(),
            name: Some("My Custom Addon".to_string()),
            tag: Some("HDR".to_string()),
            description: Some("Custom HDR grading".to_string()),
        };
        add_custom_addon(&mut state, custom);
        assert_eq!(state.addon_files.len(), 1);
        assert!(is_addon_active(&state, "C:\\mods\\my_addon.addon64"));

        toggle_addon_in_state(&mut state, "C:\\mods\\my_addon.addon64", false);
        assert!(!is_addon_active(&state, "C:\\mods\\my_addon.addon64"));
        toggle_addon_in_state(&mut state, "C:\\mods\\my_addon.addon64", true);
        assert!(is_addon_active(&state, "C:\\mods\\my_addon.addon64"));

        remove_custom_addon(&mut state, "C:\\mods\\my_addon.addon64");
        assert_eq!(state.addon_files.len(), 0);
        assert!(!is_addon_active(&state, "C:\\mods\\my_addon.addon64"));
    }

    #[test]
    fn test_find_renodx_payload() {
        if find_renodx_payload().is_none() {
            println!("RenoDX payload not installed on this runner; skipping test.");
            return;
        }
        assert!(find_renodx_payload().is_some(), "RenoDX v4.7 payload should be found");
    }

    #[test]
    fn test_deploy_respects_addon_toggles() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        if find_optiscaler_payload().is_none() || find_renodx_payload().is_none() {
            println!("OptiScaler or RenoDX payload not installed on this runner; skipping addon toggle test.");
            return;
        }
        let temp_dir = std::env::temp_dir().join(format!("dlss_addon_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let content_dir = temp_dir.join("Content");
        fs::create_dir_all(&content_dir).unwrap();

        let orig_dxgi = content_dir.join("dxgi.dll");
        fs::write(&orig_dxgi, b"ORIGINAL_DXGI").unwrap();

        let exe_path = content_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_EXE").unwrap();

        // Create a custom addon payload file to test custom addon toggles
        let custom_addon_file = temp_dir.join("test_custom.addon64");
        fs::write(&custom_addon_file, b"CUSTOM_ADDON_PAYLOAD").unwrap();

        let opts = DeployOptions {
            game_name: Some("Addon Test Game".to_string()),
            game_dir: content_dir.clone(),
            exe_path: exe_path.clone(),
            api: "dxgi".to_string(),
            pre_sr: true,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        // 1. Deploy with custom addon registered and enabled
        let mut state = crate::core::state::load_state();
        let custom_entry = crate::core::state::AddonFileEntry {
            path: custom_addon_file.to_string_lossy().to_string(),
            name: Some("Test Custom".to_string()),
            tag: Some("Test".to_string()),
            description: None,
        };
        crate::core::state::add_custom_addon(&mut state, custom_entry);
        let _ = crate::core::state::save_state(&state);

        let res = deploy_native_dlss5(&opts).expect("deploy should succeed");
        assert!(res.success);
        // Base add-on is mandatory and always deployed
        assert!(content_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 should be deployed as mandatory base");
        // Enabled custom addon is deployed
        assert!(content_dir.join("test_custom.addon64").exists(), "custom addon should be deployed when active");

        // Restore
        let rest_res = crate::core::journal::restore_game(&content_dir).expect("restore should succeed");
        assert!(rest_res);
        assert!(!content_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 should be deleted on restore");
        assert!(!content_dir.join("test_custom.addon64").exists(), "custom addon should be deleted on restore");

        // 2. Deploy with custom addon deactivated
        crate::core::state::toggle_addon_in_state(&mut state, &custom_addon_file.to_string_lossy(), false);
        let _ = crate::core::state::save_state(&state);

        let res2 = deploy_native_dlss5(&opts).expect("deploy should succeed");
        assert!(res2.success);
        assert!(content_dir.join("renodx-dlss5.addon64").exists(), "mandatory base addon is still deployed");
        assert!(!content_dir.join("test_custom.addon64").exists(), "custom addon should NOT be deployed when deactivated");

        // Clean up custom addon from state
        crate::core::state::remove_custom_addon(&mut state, &custom_addon_file.to_string_lossy());
        let _ = crate::core::state::save_state(&state);
    }

    #[test]
    fn test_native_dlss5_nested_deploy_and_clean_lifecycle() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        if find_optiscaler_payload().is_none() || find_renodx_payload().is_none() {
            println!("OptiScaler or RenoDX payload not installed on this runner; skipping test.");
            return;
        }
        let orig_state = crate::core::state::load_state();
        let mut state = orig_state.clone();
        if !state.addons.iter().any(|a| a == "builtin:mfgunlock") {
            state.addons.push("builtin:mfgunlock".to_string());
        }
        if !state.addons.iter().any(|a| a == "builtin:renodx") {
            state.addons.push("builtin:renodx".to_string());
        }
        let _ = crate::core::state::save_state(&state);

        let temp_dir = std::env::temp_dir().join(format!("dlss_nested_lifecycle_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("CyberGame.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..117].copy_from_slice(b"D3D12CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();

        let orig_dlss = bin_dir.join("nvngx_dlss.dll");
        fs::write(&orig_dlss, b"ORIGINAL_DLSS_BINARY").unwrap();
        fs::write(bin_dir.join("D3D12Core.dll"), b"core").unwrap();

        let opts = DeployOptions {
            game_name: Some("CyberGame".to_string()),
            game_dir: temp_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        // 1. Deploy Native DLSS 5
        let res = deploy_native_dlss5(&opts).expect("deploy_native_dlss5 should succeed");
        assert!(res.success);

        assert!(bin_dir.join("dxgi.dll").exists(), "dxgi.dll hook should be deployed in nested bin/x64");
        assert!(bin_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 should be in bin/x64");
        assert!(bin_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 should be in bin/x64");
        assert!(bin_dir.join("ReShade.ini").exists(), "ReShade.ini should be in bin/x64");

        // 2. Rescan directory: must recognize ReShade & Add-on
        let scanned = crate::core::scan::scan_game_directory(&temp_dir).expect("game should be recognized");
        assert!(scanned.reshade_installed, "Reshade must be detected after deploy");
        assert!(scanned.addon_installed, "Add-on must be detected after deploy");
        assert_eq!(scanned.installed_route, Some("native".to_string()));

        // 3. Clean untracked mods with exe
        let removed = crate::core::journal::clean_untracked_mods_with_exe(&temp_dir, Some(&exe_path)).expect("clean should succeed");
        assert!(removed.iter().any(|r| r.contains("dxgi.dll")), "Cleaned files must include dxgi.dll: {:?}", removed);
        assert!(removed.iter().any(|r| r.contains("renodx-dlss5.addon64")), "Cleaned files must include renodx-dlss5.addon64: {:?}", removed);
        assert!(removed.iter().any(|r| r.contains("renodx-mfgunlock.addon64")), "Cleaned files must include renodx-mfgunlock.addon64: {:?}", removed);

        // 4. Verify nested directory is completely clean of mod files, but original game files remain intact!
        assert!(!bin_dir.join("dxgi.dll").exists(), "dxgi.dll must be purged from bin/x64");
        assert!(!bin_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 must be purged from bin/x64");
        assert!(!bin_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must be purged from bin/x64");
        assert!(!bin_dir.join("ReShade.ini").exists(), "ReShade.ini must be purged from bin/x64");
        assert!(bin_dir.join("CyberGame.exe").exists(), "Original game executable must not be deleted");
        assert_eq!(fs::read(&orig_dlss).unwrap(), b"ORIGINAL_DLSS_BINARY", "Original DLSS file must remain untouched");

        // 5. Rescan directory from disk: must report NOT INSTALLED across app restarts
        let after_clean = crate::core::scan::scan_game_directory(&temp_dir).expect("game should be recognized after clean");
        assert!(!after_clean.reshade_installed, "ReShade must be false after clean");
        assert!(!after_clean.addon_installed, "Add-on must be false after clean");
        assert_eq!(after_clean.installed_route, None, "Installed route must be None after clean");
        let _ = crate::core::state::save_state(&orig_state);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_prevent_reshade_hook_corrupted_into_backup() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        if find_optiscaler_payload().is_none() || find_renodx_payload().is_none() {
            println!("OptiScaler or RenoDX payload not installed on this runner; skipping test.");
            return;
        }
        let temp_dir = std::env::temp_dir().join(format!("dlss_dirty_backup_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("Game.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..117].copy_from_slice(b"D3D12CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();
        fs::write(bin_dir.join("D3D12Core.dll"), b"core").unwrap();

        // Simulate pre-existing ReShade dxgi.dll sitting in the directory
        let pre_existing_dxgi = bin_dir.join("dxgi.dll");
        let mut reshade_bytes = vec![0u8; 50000];
        reshade_bytes[20000..20007].copy_from_slice(b"ReShade");
        fs::write(&pre_existing_dxgi, &reshade_bytes).unwrap();

        // Verify is_known_mod_file detects it
        assert!(is_known_mod_file(&pre_existing_dxgi), "is_known_mod_file must recognize pre-existing ReShade dxgi.dll");

        let opts = DeployOptions {
            game_name: Some("Game".to_string()),
            game_dir: temp_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        let res = deploy_native_dlss5(&opts).expect("deploy should succeed");
        assert!(res.success);

        // Crucial verification: pre-existing ReShade dxgi.dll must NEVER have been backed up as an "original file"!
        let backup_dxgi = temp_dir.join("_DLSS5_Backup").join("bin").join("x64").join("dxgi.dll");
        assert!(!backup_dxgi.exists(), "ReShade hook must never be copied to _DLSS5_Backup as an original file");

        let manifest = crate::core::journal::read_manifest(&temp_dir).expect("manifest should exist");
        assert!(!manifest.replaced.iter().any(|r| r.rel.contains("dxgi.dll")), "manifest.replaced must not contain dxgi.dll");

        // Now restore: must cleanly wipe the mod and leave no residual ReShade
        let restored = crate::core::journal::restore_game(&temp_dir).expect("restore should succeed");
        assert!(restored);

        assert!(!bin_dir.join("dxgi.dll").exists(), "Restoring must not leave or restore ReShade hook");

        let scanned = crate::core::scan::scan_game_directory(&temp_dir).expect("scan should succeed");
        assert!(!scanned.reshade_installed, "ReShade must be false after restore");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_overlay_addon_strictly_not_deployed_while_shelved() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        if find_optiscaler_payload().is_none() || find_renodx_payload().is_none() {
            println!("OptiScaler or RenoDX payload not installed on this runner; skipping test.");
            return;
        }

        let temp_dir = std::env::temp_dir().join(format!("dlss_overlay_shelved_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("CyberGame.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..117].copy_from_slice(b"D3D12CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();
        fs::write(bin_dir.join("D3D12Core.dll"), b"core").unwrap();

        let orig_state = crate::core::state::load_state();

        let opts = DeployOptions {
            game_name: Some("CyberGame".to_string()),
            game_dir: temp_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        let res = deploy_native_dlss5(&opts).expect("deploy_native_dlss5 should succeed");
        assert!(res.success);

        // Verify dlss5-lab-overlay.addon64 is strictly NOT deployed while shelved
        let deployed_overlay = bin_dir.join("dlss5-lab-overlay.addon64");
        assert!(!deployed_overlay.exists(), "dlss5-lab-overlay.addon64 must NEVER be deployed while shelved");

        let manifest = crate::core::journal::read_manifest(&temp_dir).expect("manifest should exist");
        assert!(!manifest.added.iter().any(|a| a.contains("dlss5-lab-overlay.addon64")), "manifest.added must NOT contain dlss5-lab-overlay.addon64");

        // Clean up
        let _ = crate::core::journal::clean_untracked_mods_with_exe(&temp_dir, Some(&exe_path));
        let _ = crate::core::state::save_state(&orig_state);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_overlay_addon_clean_and_restore_lifecycle() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        let temp_dir = std::env::temp_dir().join(format!("dlss_overlay_lifecycle_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("CyberGame.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..117].copy_from_slice(b"D3D12CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();
        fs::write(bin_dir.join("D3D12Core.dll"), b"core").unwrap();

        // Place pre-existing overlay file in bin directory
        fs::write(bin_dir.join("dlss5-lab-overlay.addon64"), b"OLD_OVERLAY_BYTES").unwrap();
        assert!(bin_dir.join("dlss5-lab-overlay.addon64").exists(), "Simulated overlay addon must exist before clean");

        // 1. Clean untracked mods
        let removed = crate::core::journal::clean_untracked_mods_with_exe(&temp_dir, Some(&exe_path)).expect("clean should succeed");
        assert!(removed.iter().any(|r| r.contains("dlss5-lab-overlay.addon64")), "Removed files must include dlss5-lab-overlay.addon64: {:?}", removed);

        // 2. Verify complete deletion
        assert!(!bin_dir.join("dlss5-lab-overlay.addon64").exists(), "dlss5-lab-overlay.addon64 must be purged from disk");

        // 3. Rescan: addon_installed must be false
        let scanned = crate::core::scan::scan_game_directory(&temp_dir).expect("scan should succeed");
        assert!(!scanned.addon_installed, "addon_installed must be false after clean");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_deploy_preserves_reshade_defaults() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        if find_optiscaler_payload().is_none() || find_renodx_payload().is_none() {
            println!("OptiScaler or RenoDX payload not installed on this runner; skipping test.");
            return;
        }
        let temp_dir = std::env::temp_dir().join(format!("dlss_font_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("FontGame.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..117].copy_from_slice(b"D3D12CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();
        fs::write(bin_dir.join("D3D12Core.dll"), b"core").unwrap();

        let opts = DeployOptions {
            game_name: Some("FontGame".to_string()),
            game_dir: temp_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_native_dlss5(&opts).expect("deploy should succeed");
        assert!(res.success);

        let reshade_ini_path = bin_dir.join("ReShade.ini");
        assert!(reshade_ini_path.exists(), "ReShade.ini must be deployed");

        let ini_content = fs::read_to_string(&reshade_ini_path).unwrap();
        let font = get_ini(&ini_content, "STYLE", "Font");

        // Verify ReShade.ini does NOT pollute the global font setting
        assert_eq!(font, None);

        let _ = crate::core::journal::clean_untracked_mods_with_exe(&temp_dir, Some(&exe_path));
        let _ = fs::remove_dir_all(&temp_dir);
    }

    impl PayloadBundle {
        pub fn create_mock(temp: &Path) -> Self {
            let opti_dir = temp.join("mock_opti_payload");
            fs::create_dir_all(opti_dir.join("OptiScaler")).unwrap();
            let optiscaler_dll = opti_dir.join("OptiScaler.dll");
            fs::write(&optiscaler_dll, b"MOCK_OPTISCALER_PE_BYTES").unwrap();
            let optiscaler_ini = opti_dir.join("OptiScaler.ini");
            fs::write(&optiscaler_ini, b"[DlssNr]\nEnabled=false\n\n[Plugins]\nLoadReshade=true\n\n[FrameGen]\nExternal=false\n").unwrap();

            let nr_dll = temp.join("nvngx_dlssnr.dll");
            fs::write(&nr_dll, b"MOCK_DLSSNR_PE_BYTES").unwrap();

            let rtxmfg = temp.join("RTXMFG.dll");
            fs::write(&rtxmfg, b"MOCK_RTXMFG_PE_BYTES").unwrap();

            let reshade = temp.join("ReShade64.dll");
            fs::write(&reshade, b"MOCK_RESHADE_PE_BYTES").unwrap();

            let renodx = temp.join("renodx-dlss5.addon64");
            fs::write(&renodx, b"MOCK_RENODX_PE_BYTES").unwrap();

            let mfgunlock = temp.join("renodx-mfgunlock.addon64");
            fs::write(&mfgunlock, b"MOCK_MFGUNLOCK_PE_BYTES").unwrap();

            let snippet = temp.join("nvngx.dll_dlssnr.dll");
            fs::write(&snippet, b"MOCK_SNIPPET_PE_BYTES").unwrap();

            let streamline_dir = temp.join("mock_streamline");
            fs::create_dir_all(&streamline_dir).unwrap();
            fs::write(streamline_dir.join("sl.interposer.dll"), b"MOCK_SL_INTERPOSER").unwrap();
            fs::write(streamline_dir.join("sl.common.dll"), b"MOCK_SL_COMMON").unwrap();
            fs::write(streamline_dir.join("sl.dlss_g.dll"), b"MOCK_SL_DLSSG").unwrap();
            fs::write(streamline_dir.join("sl.reflex.dll"), b"MOCK_SL_REFLEX").unwrap();
            fs::write(streamline_dir.join("sl.pcl.dll"), b"MOCK_SL_PCL").unwrap();
            fs::write(streamline_dir.join("nvngx_dlssg.dll"), b"MOCK_NVNGX_DLSSG").unwrap();

            let feeder_dir = temp.join("mock_feeder");
            let feeder_shaders = feeder_dir.join("feeder-shaders");
            fs::create_dir_all(feeder_shaders.join("Shaders")).unwrap();
            fs::write(feeder_shaders.join("Shaders").join("DLSS5_Feed.fx"), b"// mock feed").unwrap();
            fs::write(feeder_shaders.join("Shaders").join("vort_Motion.fx"), b"// mock motion").unwrap();
            let mock_addon64 = feeder_dir.join("dlss5-feed.addon64");
            fs::write(&mock_addon64, b"MOCK_FEEDER_ADDON_64").unwrap();

            let mock_feeder = crate::core::downloader::FeederComponents {
                addon64: mock_addon64,
                addon32: None,
                host64: None,
                shader_dir: feeder_shaders,
                vk_layer_dir: None,
            };

            let dlss_dll = temp.join("nvngx_dlss.dll");
            fs::write(&dlss_dll, b"MOCK_DLSS_PE_BYTES").unwrap();

            Self {
                optiscaler_dll,
                optiscaler_ini,
                optiscaler_dir: Some(opti_dir.join("OptiScaler")),
                nvngx_dlss_dll: Some(dlss_dll),
                nvngx_dlssnr_dll: Some(nr_dll),
                nvngx_snippet_dll: Some(snippet),
                rtxmfg_dll: Some(rtxmfg),
                reshade64_dll: Some(reshade),
                renodx_dlss5_addon: Some(renodx),
                renodx_mfgunlock_addon: Some(mfgunlock),
                feeder_components: Some(mock_feeder),
                streamline_dir: Some(streamline_dir),
            }
        }
    }

    #[test]
    fn test_optiscaler_route_deploys_expected_files_and_strictly_excludes_reshade() {
        let temp_dir = std::env::temp_dir().join(format!("test_pure_opti_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        let bin_dir = game_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Pure Opti Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: true,
            passes: 2,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_optiscaler_with_bundle(&opts, &payloads).expect("deploy_optiscaler_with_bundle must succeed");
        assert!(res.success);

        // 1. Positive assertions: Pure OptiScaler and Standalone RTXMFG must exist
        assert!(bin_dir.join("dxgi.dll").exists(), "OptiScaler dxgi.dll must be deployed");
        assert!(bin_dir.join("version.dll").exists(), "Standalone RTXMFG version.dll must be deployed");
        assert!(bin_dir.join("OptiScaler.ini").exists(), "OptiScaler.ini must be deployed");
        assert!(bin_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be deployed");
        assert!(bin_dir.join("nvngx.dll_dlssnr.dll").exists(), "nvngx.dll_dlssnr.dll must be deployed");

        // 2. Strict Negative assertions: ZERO ReShade or add-on files allowed
        assert!(!bin_dir.join("ReShade64.dll").exists(), "ReShade64.dll must NEVER be deployed in OptiScaler route");
        assert!(!bin_dir.join("ReShade.ini").exists(), "ReShade.ini must NEVER be deployed in OptiScaler route");
        assert!(!bin_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 must NEVER be deployed in OptiScaler route");
        assert!(!bin_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must NEVER be deployed in OptiScaler route");
        assert!(!bin_dir.join("dlss5-lab-overlay.addon64").exists(), "overlay addon must NEVER be deployed in OptiScaler route");

        // 3. Ini inspection: LoadReshade must be false, External must be true, PreSR configured
        let ini_content = fs::read_to_string(bin_dir.join("OptiScaler.ini")).unwrap();
        assert_eq!(get_ini(&ini_content, "Plugins", "LoadReshade"), Some("false".to_string()));
        assert_eq!(get_ini(&ini_content, "FrameGen", "External"), Some("true".to_string()));
        assert_eq!(get_ini(&ini_content, "DlssNr", "Enabled"), Some("true".to_string()));
        assert_eq!(get_ini(&ini_content, "DlssNr", "RunBeforeSR"), Some("true".to_string()));
        assert_eq!(get_ini(&ini_content, "DlssNr", "Passes"), Some("2".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_optiscaler_route_without_mfg() {
        let temp_dir = std::env::temp_dir().join(format!("test_opti_no_mfg_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Opti No MFG".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "dxgi".to_string(),
            pre_sr: true,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        let res = deploy_optiscaler_with_bundle(&opts, &payloads).expect("deploy must succeed");
        assert!(res.success);

        // version.dll must NOT be deployed
        assert!(!game_dir.join("version.dll").exists(), "version.dll must not be deployed when mfg_unlock is false");

        // External FG must be false
        let ini_content = fs::read_to_string(game_dir.join("OptiScaler.ini")).unwrap();
        assert_eq!(get_ini(&ini_content, "FrameGen", "External"), Some("false".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_streamline_game_deploys_version_dll_with_external_framegen() {
        let temp_dir = std::env::temp_dir().join(format!("test_streamline_mfg_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("Cyberpunk2077");
        let bin_dir = game_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("Cyberpunk2077.exe");
        fs::write(&exe_path, b"DUMMY_CYBERPUNK_EXE").unwrap();

        // Simulate native Streamline files present in Cyberpunk 2077
        fs::write(bin_dir.join("sl.interposer.dll"), b"MOCK_STREAMLINE_INTERPOSER").unwrap();
        fs::write(bin_dir.join("sl.common.dll"), b"MOCK_STREAMLINE_COMMON").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Cyberpunk 2077".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: true,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_optiscaler_with_bundle(&opts, &payloads).expect("deploy must succeed");
        assert!(res.success);

        // Positive assertions: OptiScaler and DLSS-NR files deployed
        assert!(bin_dir.join("dxgi.dll").exists(), "OptiScaler dxgi.dll must be deployed");
        assert!(bin_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be deployed");
        assert!(bin_dir.join("nvngx.dll_dlssnr.dll").exists(), "nvngx.dll_dlssnr.dll must be deployed");
        assert!(bin_dir.join("OptiScaler.ini").exists(), "OptiScaler.ini must be deployed");

        // RTX40-MFG doc adherence: version.dll, RTXMFG-Universal.json, and RTX40MFG-Universal.json must be deployed
        assert!(
            bin_dir.join("version.dll").exists(),
            "Standalone RTXMFG version.dll must be deployed to provide 4x MFG unlock"
        );
        assert!(
            bin_dir.join("RTXMFG-Universal.json").exists(),
            "RTXMFG-Universal.json configuration must be deployed"
        );
        assert!(
            bin_dir.join("RTX40MFG-Universal.json").exists(),
            "RTX40MFG-Universal.json configuration must be deployed"
        );

        // Streamline 2.14.1 stack deployed and replaces old local DLLs
        assert_eq!(fs::read(bin_dir.join("sl.interposer.dll")).unwrap(), b"MOCK_SL_INTERPOSER");
        assert_eq!(fs::read(bin_dir.join("sl.common.dll")).unwrap(), b"MOCK_SL_COMMON");
        assert_eq!(fs::read(bin_dir.join("sl.reflex.dll")).unwrap(), b"MOCK_SL_REFLEX");

        let ini_content = fs::read_to_string(bin_dir.join("OptiScaler.ini")).unwrap();
        assert_eq!(get_ini(&ini_content, "FrameGen", "External"), Some("true".to_string()));
        assert_eq!(get_ini(&ini_content, "DLSSG", "InterpolationCount"), Some("auto".to_string()));
        assert_eq!(get_ini(&ini_content, "DLSSG", "OverrideInterpolationCount"), Some("auto".to_string()));

        // Restore backup via journal
        let restored = crate::core::journal::restore_game(&game_dir).expect("restore must succeed");
        assert!(restored);

        // Original Streamline files must be restored
        assert_eq!(fs::read(bin_dir.join("sl.interposer.dll")).unwrap(), b"MOCK_STREAMLINE_INTERPOSER");
        assert_eq!(fs::read(bin_dir.join("sl.common.dll")).unwrap(), b"MOCK_STREAMLINE_COMMON");
        assert!(!bin_dir.join("sl.reflex.dll").exists(), "sl.reflex.dll must be wiped on restore");
        assert!(!bin_dir.join("version.dll").exists(), "version.dll must be wiped on restore");
        assert!(!bin_dir.join("OptiScaler.ini").exists(), "OptiScaler.ini must be wiped on restore");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_reshade_route_deploys_expected_files_and_strictly_excludes_optiscaler() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_pure_reshade_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("ReShade Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_native_dlss5_with_bundle(&opts, &payloads).expect("deploy_native_dlss5_with_bundle must succeed");
        assert!(res.success);

        // 1. Positive assertions: ReShade + RenoDX + ReShade 4x MFG unlock addon must exist
        assert!(game_dir.join("dxgi.dll").exists(), "ReShade dxgi.dll must be deployed");
        assert!(game_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 must be deployed");
        assert!(game_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be deployed for RenoDX DLSS 5 Neural Rendering");
        assert!(game_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must be deployed");
        assert!(game_dir.join("ReShade.ini").exists(), "ReShade.ini must be deployed");

        // 2. Strict Negative assertions: ZERO OptiScaler files allowed
        assert!(!game_dir.join("OptiScaler.ini").exists(), "OptiScaler.ini must NEVER be deployed in ReShade route");
        assert!(!game_dir.join("OptiScaler").exists(), "OptiScaler directory must NEVER be deployed in ReShade route");
        assert!(!game_dir.join("version.dll").exists(), "Standalone RTXMFG version.dll must NEVER be deployed in ReShade route");
        assert!(!game_dir.join("nvngx.dll_dlssnr.dll").exists(), "nvngx.dll_dlssnr.dll must NEVER be in ReShade route");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_reshade_route_mfg_disabled() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_pure_reshade_no_mfg_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("ReShade No MFG".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        let res = deploy_native_dlss5_with_bundle(&opts, &payloads).expect("deploy must succeed");
        assert!(res.success);

        assert!(!game_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must NOT be deployed when mfg_unlock is false");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_feeder_route_deploys_only_feeder_and_strictly_excludes_mfg_and_presr() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_feeder_pure_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Feeder Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "dxgi".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        let res = deploy_feeder_with_bundle(&opts, &payloads).expect("deploy_feeder_with_bundle must succeed");
        assert!(res.success);

        // 1. Positive assertions: Feeder + Shaders + DLSS-5 Neural Rendering
        assert!(game_dir.join("dxgi.dll").exists(), "ReShade dxgi.dll must be deployed");
        assert!(game_dir.join("ReShade.ini").exists(), "ReShade.ini must be deployed");
        assert!(game_dir.join("dlss5-feed.addon64").exists(), "dlss5-feed.addon64 must be deployed");
        assert!(game_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 must be deployed for neural rendering");
        assert!(game_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be deployed for neural rendering");
        assert!(game_dir.join("nvngx_dlss.dll").exists(), "nvngx_dlss.dll must be deployed for Streamline Feeder Super Sampling");

        // 2. Strict Negative assertions: NO MFG (when mfg_unlock: false), NO Pre-SR, NO OptiScaler
        assert!(!game_dir.join("version.dll").exists(), "version.dll must NOT be in Feeder route");
        assert!(!game_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must NOT be in Feeder route when mfg_unlock is false");
        assert!(!game_dir.join("OptiScaler.ini").exists(), "OptiScaler.ini must NOT be in Feeder route");
        assert!(!game_dir.join("OptiScaler").exists(), "OptiScaler dir must NOT be in Feeder route");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_feeder_route_deploys_mfg_and_dlssnr_when_mfg_unlock_enabled() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_feeder_mfg_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Feeder MFG Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "dxgi".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_feeder_with_bundle(&opts, &payloads).expect("deploy_feeder_with_bundle must succeed");
        assert!(res.success);

        // Positive assertions: ReShade + RenoDX MFG + RenoDX DLSS-5 + nvngx_dlssnr must exist
        assert!(game_dir.join("dxgi.dll").exists(), "ReShade dxgi.dll must be deployed");
        assert!(game_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must be deployed on Feeder with MFG");
        assert!(game_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 must be deployed on Feeder with MFG");
        assert!(game_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be deployed on Feeder with MFG");
        assert!(game_dir.join("ReShade.ini").exists(), "ReShade.ini must be deployed");

        let ini_content = fs::read_to_string(game_dir.join("ReShade.ini")).unwrap();
        assert!(ini_content.contains("[RenoDX.MFGUnlock]"), "[RenoDX.MFGUnlock] section must be written");
        assert!(ini_content.contains("ForceMultiplier=4"), "ForceMultiplier=4 must be written for 4x frame generation");
        assert!(ini_content.contains("EnableHooks=1"), "ReShade.ini in Feeder mode specifies EnableHooks=1 for swapchain and direct presentation interception");

        // Verify root preset formatting
        let preset_content = fs::read_to_string(game_dir.join("ReShadePreset.ini")).unwrap();
        assert!(preset_content.starts_with("Techniques=vort_MotionEffects@vort_Motion.fx,DLSS5_Feed@DLSS5_Feed.fx"), "Techniques must be at root of ReShadePreset.ini");
        assert!(!preset_content.starts_with("[ReShadePreset.ini]"), "ReShadePreset.ini must NOT contain a section header at line 1");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_feeder_route_dx11_configures_preset_root_and_enables_mfg() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_feeder_dx11_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game_DX11.exe");
        fs::write(&exe_path, b"DUMMY_DX11_GAME_EXE").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("DirectX 11 Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 11".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true, // User requested MFG in UI or options
            mfg_multiplier: 4,
        };

        let res = deploy_feeder_with_bundle(&opts, &payloads).expect("deploy_feeder_with_bundle must succeed for DX11");
        assert!(res.success);

        // 1. Positive assertion: ReShade + RenoDX MFG + RenoDX DLSS-5 + nvngx_dlssnr must exist
        assert!(game_dir.join("dxgi.dll").exists(), "ReShade dxgi.dll must be deployed for DX11");
        assert!(game_dir.join("dlss5-feed.addon64").exists(), "dlss5-feed.addon64 must be deployed for DX11");
        assert!(game_dir.join("renodx-dlss5.addon64").exists(), "renodx-dlss5.addon64 must be deployed for DX11");
        assert!(game_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be deployed for DX11");
        assert!(game_dir.join("nvngx_dlss.dll").exists(), "nvngx_dlss.dll must be deployed for DX11");
        assert!(game_dir.join("ReShade.ini").exists(), "ReShade.ini must be deployed for DX11");

        // 2. Gating assertion: renodx-mfgunlock.addon64 must NOT be deployed on DX11
        assert!(!game_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must NOT be deployed on DX11 games");

        // 3. ReShade.ini content verification
        let ini_content = fs::read_to_string(game_dir.join("ReShade.ini")).unwrap();
        assert!(ini_content.contains("EnableHooks=1"), "ReShade.ini specifies EnableHooks=1 for swapchain and direct presentation interception");
        assert!(!ini_content.contains("[RenoDX.MFGUnlock]"), "[RenoDX.MFGUnlock] section must NOT be written for DX11 titles without native DLSS-G");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_feeder_route_vulkan_uses_dxgi_and_cleans_stale_winmm() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_feeder_vulkan_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        let bin_dir = game_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        // Place a stale proxy winmm.dll left behind by an older deployment
        let stale_winmm = bin_dir.join("winmm.dll");
        fs::write(&stale_winmm, b"MOCK_RESHADE_PE_BYTES").unwrap();
        assert!(stale_winmm.exists());

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Vulkan Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "Vulkan".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_feeder_with_bundle(&opts, &payloads).expect("deploy_feeder_with_bundle must succeed");
        assert!(res.success);

        // Positive assertion: dxgi.dll MUST be used for Vulkan
        assert!(bin_dir.join("dxgi.dll").exists(), "dxgi.dll must be deployed for Vulkan game");
        assert!(bin_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must be deployed for Vulkan game with MFG unlock");
        let ini_content = fs::read_to_string(bin_dir.join("ReShade.ini")).unwrap();
        assert!(ini_content.contains("[RenoDX.MFGUnlock]"), "[RenoDX.MFGUnlock] must be written for Vulkan game with MFG unlock");

        // Negative assertion: winmm.dll MUST be deleted and cleaned up
        assert!(!bin_dir.join("winmm.dll").exists(), "Obsolete winmm.dll proxy must be deleted so timeGetTime is not intercepted");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_route_clean_and_rollback() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_rollback_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let orig_dxgi = game_dir.join("dxgi.dll");
        fs::write(&orig_dxgi, b"GENUINE_GAME_DXGI").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Rollback Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "dxgi".to_string(),
            pre_sr: true,
            passes: 2,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        // Deploy Route 1
        let res = deploy_optiscaler_with_bundle(&opts, &payloads).expect("deploy must succeed");
        assert!(res.success);
        assert_eq!(fs::read(&orig_dxgi).unwrap(), b"MOCK_OPTISCALER_PE_BYTES");

        // Restore backup via journal
        let restored = crate::core::journal::restore_game(&game_dir).expect("restore must succeed");
        assert!(restored);

        // Original file must be restored
        assert_eq!(fs::read(&orig_dxgi).unwrap(), b"GENUINE_GAME_DXGI");
        assert!(!game_dir.join("version.dll").exists(), "version.dll must be wiped on restore");
        assert!(!game_dir.join("OptiScaler.ini").exists(), "OptiScaler.ini must be wiped on restore");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_switching_routes_cleans_previous_artifacts() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_switch_routes_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..117].copy_from_slice(b"D3D12CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        // 1. Deploy Route 1 (OptiScaler)
        let opti_opts = DeployOptions {
            game_name: Some("Switch Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: true,
            passes: 2,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };
        deploy_optiscaler_with_bundle(&opti_opts, &payloads).unwrap();
        assert!(game_dir.join("OptiScaler.ini").exists());
        assert!(game_dir.join("version.dll").exists());

        // 2. Clean
        let removed = crate::core::journal::clean_untracked_mods_with_exe(&game_dir, Some(&exe_path)).unwrap();
        assert!(removed.iter().any(|r| r.contains("OptiScaler.ini")));

        // 3. Deploy Route 2 (ReShade + RenoDX)
        let reshade_opts = DeployOptions {
            game_name: Some("Switch Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 12".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };
        deploy_native_dlss5_with_bundle(&reshade_opts, &payloads).unwrap();

        // Must have ReShade, but strictly NO OptiScaler residue
        assert!(game_dir.join("renodx-dlss5.addon64").exists());
        assert!(game_dir.join("ReShade.ini").exists());
        assert!(!game_dir.join("OptiScaler.ini").exists());
        assert!(!game_dir.join("version.dll").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_deploy_fails_cleanly_on_missing_payload() {
        let temp_dir = std::env::temp_dir().join(format!("test_fail_cleanly_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        let mut payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));
        payloads.optiscaler_dll = temp_dir.join("non_existent_optiscaler.dll");

        let opts = DeployOptions {
            game_name: Some("Fail Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "dxgi".to_string(),
            pre_sr: true,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res = deploy_optiscaler_with_bundle(&opts, &payloads);
        assert!(res.is_err(), "Must return Err when critical payload is missing");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ini_parser_handles_malformed_input() {
        let malformed = "just a random line\nno_equal_sign\n[DlssNr]\n=empty_key\nEnabled=true\n\n[SectionWithoutValue]\nKey=";
        assert_eq!(get_ini(malformed, "DlssNr", "Enabled"), Some("true".to_string()));
        assert_eq!(get_ini(malformed, "SectionWithoutValue", "Key"), Some("".to_string()));
        assert_eq!(get_ini(malformed, "NonExistent", "Key"), None);

        let updated = set_ini(malformed, "DlssNr", "Passes", "3");
        assert_eq!(get_ini(&updated, "DlssNr", "Passes"), Some("3".to_string()));

        let added_section = set_ini("", "NewSection", "NewKey", "NewVal");
        assert_eq!(get_ini(&added_section, "NewSection", "NewKey"), Some("NewVal".to_string()));
    }

    #[test]
    fn test_generate_optiscaler_ini_all_combinations() {
        let base = "[DlssNr]\nEnabled=false\n\n[Plugins]\nLoadReshade=true\n";

        // Case A: PreSR true, MFG true, exe provided
        let res_a = generate_optiscaler_ini(base, true, 3, true, Some("CyberGame.exe"));
        assert_eq!(get_ini(&res_a, "DlssNr", "Enabled"), Some("true".to_string()));
        assert_eq!(get_ini(&res_a, "DlssNr", "RunBeforeSR"), Some("true".to_string()));
        assert_eq!(get_ini(&res_a, "DlssNr", "Passes"), Some("3".to_string()));
        assert_eq!(get_ini(&res_a, "DlssNr", "ApplyAfterRR"), Some("true".to_string()));
        assert_eq!(get_ini(&res_a, "Plugins", "LoadReshade"), Some("false".to_string()));
        assert_eq!(get_ini(&res_a, "FrameGen", "External"), Some("true".to_string()));
        assert_eq!(get_ini(&res_a, "Menu", "ShortcutKey"), Some("0x2D".to_string()));
        assert_eq!(get_ini(&res_a, "Init", "TargetProcessName"), Some("CyberGame.exe".to_string()));

        // Case B: PreSR false, MFG false, no exe
        let res_b = generate_optiscaler_ini(base, false, 1, false, None);
        assert_eq!(get_ini(&res_b, "DlssNr", "Enabled"), Some("false".to_string()));
        assert_eq!(get_ini(&res_b, "DlssNr", "RunBeforeSR"), Some("false".to_string()));
        assert_eq!(get_ini(&res_b, "DlssNr", "Passes"), Some("1".to_string()));
        assert_eq!(get_ini(&res_b, "Plugins", "LoadReshade"), Some("false".to_string()));
        assert_eq!(get_ini(&res_b, "FrameGen", "External"), Some("false".to_string()));
    }

    #[test]
    fn test_payload_bundle_from_system_finds_streamline() {
        let bundle = match PayloadBundle::from_system() {
            Ok(b) => b,
            Err(_) => {
                println!("PayloadBundle not installed on this runner; skipping test.");
                return;
            }
        };
        assert!(bundle.streamline_dir.is_some(), "Streamline payload directory must be found on system");
        let dir = bundle.streamline_dir.unwrap();
        assert!(dir.join("sl.interposer.dll").exists());
        assert!(dir.join("sl.common.dll").exists());
        assert!(dir.join("sl.reflex.dll").exists());
        assert!(dir.join("sl.dlss_g.dll").exists());
        assert!(dir.join("nvngx_dlssg.dll").exists());

        assert!(bundle.nvngx_snippet_dll.is_some(), "nvngx.dll_dlssnr.dll forwarder must be found on system");
        let snippet = bundle.nvngx_snippet_dll.unwrap();
        assert!(snippet.is_file(), "nvngx.dll_dlssnr.dll must be a valid file");
    }

    #[test]
    fn test_feeder_preset_root_technique_formatting() {
        // 1. Fresh empty preset
        let fresh = configure_feeder_preset("");
        let first_line = fresh.lines().next().unwrap_or("");
        assert!(
            first_line.starts_with("Techniques=vort_MotionEffects@vort_Motion.fx,DLSS5_Feed@DLSS5_Feed.fx"),
            "First line of preset must be Techniques= and NOT a section header"
        );
        assert!(!fresh.starts_with('['), "Preset must not start with a section header");
        assert!(fresh.contains("TechniqueSorting=vort_MotionEffects@vort_Motion.fx,DLSS5_Feed@DLSS5_Feed.fx"));
        assert!(fresh.contains("[DLSS5_Feed.fx]"));
        assert!(fresh.contains("PreprocessorDefinitions=DLSS5_MV_PROVIDER=2"));

        // 2. Legacy preset with erroneous [ReShadePreset.ini] section header at line 1
        let legacy = "[ReShadePreset.ini]\nTechniques=vort_MotionEffects@vort_Motion.fx\nTechniqueSorting=vort_MotionEffects@vort_Motion.fx\n";
        let fixed = configure_feeder_preset(legacy);
        assert!(!fixed.contains("[ReShadePreset.ini]"), "Erroneous section header must be stripped");
        assert!(fixed.starts_with("Techniques=vort_MotionEffects@vort_Motion.fx,DLSS5_Feed@DLSS5_Feed.fx"));

        // 3. Preset with existing user techniques and other section
        let existing = "Techniques=CAS@CAS.fx,SMAA@SMAA.fx\nTechniqueSorting=CAS@CAS.fx,SMAA@SMAA.fx\n\n[CAS.fx]\nContrast=0.5\n";
        let merged = configure_feeder_preset(existing);
        assert!(merged.starts_with("Techniques=vort_MotionEffects@vort_Motion.fx,DLSS5_Feed@DLSS5_Feed.fx,CAS@CAS.fx,SMAA@SMAA.fx"));
        assert!(merged.contains("TechniqueSorting=vort_MotionEffects@vort_Motion.fx,DLSS5_Feed@DLSS5_Feed.fx,CAS@CAS.fx,SMAA@SMAA.fx"));
        assert!(merged.contains("[CAS.fx]\nContrast=0.5"));
        assert!(merged.contains("[DLSS5_Feed.fx]"));
    }

    #[test]
    fn test_feeder_upgrades_outdated_local_dlss_dll_and_restores_original() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_dlss_upgrade_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        let bin_dir = game_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        // Simulate game shipping with an ancient DLSS 2.4.2.0 DLL
        let original_dlss_bytes = b"ANCIENT_DLSS_2_4_2_0_BYTES";
        let local_dlss = bin_dir.join("nvngx_dlss.dll");
        fs::write(&local_dlss, original_dlss_bytes).unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("Upgrade Test Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 11".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        // 1. Deploy Feeder route
        let res = deploy_feeder_with_bundle(&opts, &payloads).expect("deploy must succeed");
        assert!(res.success);

        // nvngx_dlss.dll must now be upgraded to the payload modern version
        let deployed_bytes = fs::read(&local_dlss).unwrap();
        assert_eq!(deployed_bytes, b"MOCK_DLSS_PE_BYTES", "Local DLSS DLL must be upgraded to modern payload bytes");

        // nvngx_dlssnr.dll must also be deployed
        assert!(bin_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be deployed beside executable");

        // 2. Rollback via journal restore_game
        let restored = crate::core::journal::restore_game(&game_dir).expect("restore must succeed");
        assert!(restored, "restore_game must report success");

        // Local nvngx_dlss.dll must be restored to its exact original bytes
        let restored_bytes = fs::read(&local_dlss).unwrap();
        assert_eq!(restored_bytes, original_dlss_bytes, "Original DLSS DLL must be restored byte-for-byte");

        // Added files must be cleaned
        assert!(!bin_dir.join("dxgi.dll").exists(), "dxgi.dll hook must be removed on rollback");
        assert!(!bin_dir.join("dlss5-feed.addon64").exists(), "dlss5-feed.addon64 must be removed on rollback");
        assert!(!bin_dir.join("nvngx_dlssnr.dll").exists(), "nvngx_dlssnr.dll must be removed on rollback");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_deploy_opengl_feeder_deploys_opengl32_and_cleans_stale_dxgi() {
        let _guard = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_deploy_opengl_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("game");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"DUMMY_GAME_EXE").unwrap();

        // Simulate a stale dxgi.dll left over from earlier misclassified deploy
        let stale_dxgi = game_dir.join("dxgi.dll");
        fs::write(&stale_dxgi, b"MOCK_RESHADE_BYTES").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let opts = DeployOptions {
            game_name: Some("OpenGL Test Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "OpenGL".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        // Deploy Feeder route for OpenGL game
        let res = deploy_feeder_with_bundle(&opts, &payloads).expect("deploy must succeed");
        assert!(res.success);

        // 1. opengl32.dll must be deployed as hook
        let hook_path = game_dir.join("opengl32.dll");
        assert!(hook_path.exists(), "opengl32.dll must be deployed for OpenGL titles");

        // 2. Stale dxgi.dll must be cleaned
        assert!(!stale_dxgi.exists(), "Stale dxgi.dll must be removed when switching to opengl32.dll");

        // 3. Rollback via journal restore_game
        let restored = crate::core::journal::restore_game(&game_dir).expect("restore must succeed");
        assert!(restored, "restore_game must report success");
        assert!(!hook_path.exists(), "opengl32.dll must be removed on rollback");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_hot_swap_between_feeder_and_optiscaler_without_intermediate_restore() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_hotswap_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("BaldursGate3");
        let bin_dir = game_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("bg3.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..110].copy_from_slice(b"vkCreateIn");
        fs::write(&exe_path, &exe_bytes).unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        // Step 1: Deploy Feeder route with MFG on Vulkan
        let feeder_opts = DeployOptions {
            game_name: Some("Baldurs Gate 3".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "Vulkan".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res1 = deploy_feeder_with_bundle(&feeder_opts, &payloads).expect("deploy feeder must succeed");
        assert!(res1.success);

        assert!(bin_dir.join("dxgi.dll").exists());
        assert!(bin_dir.join("ReShade.ini").exists());
        assert!(bin_dir.join("dlss5-feed.addon64").exists());
        assert!(bin_dir.join("renodx-mfgunlock.addon64").exists());
        assert!(bin_dir.join("reshade-shaders").exists());
        assert!(crate::core::vulkan_layer::is_game_registered(&game_dir));

        // Step 2: Directly hot-swap to OptiScaler (WITHOUT calling restore_game!)
        let opti_opts = DeployOptions {
            game_name: Some("Baldurs Gate 3".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "Vulkan".to_string(),
            pre_sr: true,
            passes: 2,
            mfg_unlock: true,
            mfg_multiplier: 4,
        };

        let res2 = deploy_optiscaler_with_bundle(&opti_opts, &payloads).expect("deploy optiscaler must succeed");
        assert!(res2.success);

        // OptiScaler files MUST exist
        assert!(bin_dir.join("dxgi.dll").exists());
        assert!(bin_dir.join("OptiScaler.ini").exists());
        assert!(bin_dir.join("version.dll").exists());
        assert!(bin_dir.join("RTXMFG-Universal.json").exists());

        // ReShade and Feeder artifacts MUST be purged
        assert!(!bin_dir.join("ReShade.ini").exists(), "ReShade.ini must be purged when hot-swapping to OptiScaler");
        assert!(!bin_dir.join("ReShadePreset.ini").exists(), "ReShadePreset.ini must be purged");
        assert!(!bin_dir.join("reshadegui.ini").exists(), "reshadegui.ini must be purged");
        assert!(!bin_dir.join("dlss5-feed.addon64").exists(), "dlss5-feed.addon64 must be purged");
        assert!(!bin_dir.join("renodx-mfgunlock.addon64").exists(), "renodx-mfgunlock.addon64 must be purged");
        assert!(!bin_dir.join("reshade-shaders").exists(), "reshade-shaders directory must be purged");
        // Vulkan layer MUST be unregistered so ReShade is NOT loaded alongside OptiScaler!
        assert!(!crate::core::vulkan_layer::is_game_registered(&game_dir), "Vulkan layer must be unregistered when hot-swapping to OptiScaler");

        // Step 3: Directly hot-swap back to Feeder (WITHOUT calling restore_game!)
        let res3 = deploy_feeder_with_bundle(&feeder_opts, &payloads).expect("deploy feeder 2 must succeed");
        assert!(res3.success);

        // Feeder files MUST exist and Vulkan layer re-registered
        assert!(bin_dir.join("ReShade.ini").exists());
        assert!(bin_dir.join("renodx-mfgunlock.addon64").exists());
        assert!(crate::core::vulkan_layer::is_game_registered(&game_dir));

        // OptiScaler files MUST be purged
        assert!(!bin_dir.join("OptiScaler.ini").exists(), "OptiScaler.ini must be purged when hot-swapping to Feeder");
        assert!(!bin_dir.join("version.dll").exists(), "version.dll must be purged when hot-swapping to Feeder");
        assert!(!bin_dir.join("RTXMFG-Universal.json").exists(), "RTXMFG-Universal.json must be purged");

        // Step 4: Full Restore
        let restored = crate::core::journal::restore_game(&game_dir).expect("restore must succeed");
        assert!(restored);
        assert!(!bin_dir.join("dxgi.dll").exists());
        assert!(!bin_dir.join("ReShade.ini").exists());
        assert!(!bin_dir.join("OptiScaler.ini").exists());
        assert!(!crate::core::vulkan_layer::is_game_registered(&game_dir));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_hot_swap_preserves_original_game_files_through_multiple_swaps() {
        let _state_lock = STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!("test_hotswap_preserve_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp_dir.join("GameDir");
        fs::create_dir_all(&game_dir).unwrap();

        let exe_path = game_dir.join("Game.exe");
        fs::write(&exe_path, b"ORIGINAL_EXE_PAYLOAD").unwrap();

        // Vanilla genuine dxgi.dll
        let genuine_dxgi = game_dir.join("dxgi.dll");
        fs::write(&genuine_dxgi, b"GENUINE_GAME_DXGI_CONTENT_12345").unwrap();

        let payloads = PayloadBundle::create_mock(&temp_dir.join("payloads"));

        let feeder_opts = DeployOptions {
            game_name: Some("Preserve Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 11".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };

        // Deploy Feeder: replaces genuine dxgi.dll with ReShade
        deploy_feeder_with_bundle(&feeder_opts, &payloads).unwrap();
        assert_ne!(fs::read(&genuine_dxgi).unwrap(), b"GENUINE_GAME_DXGI_CONTENT_12345");

        // Swap directly to OptiScaler: replaces dxgi.dll with OptiScaler
        let opti_opts = DeployOptions {
            game_name: Some("Preserve Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 11".to_string(),
            pre_sr: true,
            passes: 2,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };
        deploy_optiscaler_with_bundle(&opti_opts, &payloads).unwrap();

        // Swap directly to Native DLSS 5
        let native_opts = DeployOptions {
            game_name: Some("Preserve Game".to_string()),
            game_dir: game_dir.clone(),
            exe_path: exe_path.clone(),
            api: "DirectX 11".to_string(),
            pre_sr: false,
            passes: 1,
            mfg_unlock: false,
            mfg_multiplier: 1,
        };
        deploy_native_dlss5_with_bundle(&native_opts, &payloads).unwrap();

        // Now restore: genuine dxgi.dll must be 100% restored
        let restored = crate::core::journal::restore_game(&game_dir).expect("restore must succeed");
        assert!(restored);
        assert_eq!(fs::read(&genuine_dxgi).unwrap(), b"GENUINE_GAME_DXGI_CONTENT_12345", "Original vanilla file must be completely preserved across multiple hot-swaps");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}


