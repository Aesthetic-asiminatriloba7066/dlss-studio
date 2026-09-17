use std::sync::LazyLock;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use crate::core::pe::{inspect_pe, find_markers};
use regex::Regex;
#[cfg(windows)]
use windows::Win32::System::Registry::{
    RegOpenKeyExW, RegQueryValueExW, RegEnumKeyExW, RegCloseKey,
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY,
    REG_SAM_FLAGS, REG_VALUE_TYPE,
};
#[cfg(windows)]
use windows::core::{PCWSTR, PWSTR};

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn win32_read_reg_string(root: HKEY, subkey: &str, value_name: &str, sam: REG_SAM_FLAGS) -> Option<String> {
    let subkey_wide = to_wide(subkey);
    let val_wide = to_wide(value_name);
    let mut hkey = HKEY::default();

    unsafe {
        let res = RegOpenKeyExW(root, PCWSTR(subkey_wide.as_ptr()), 0, sam, &mut hkey);
        if res.is_err() || hkey.is_invalid() {
            return None;
        }

        let mut val_type = REG_VALUE_TYPE::default();
        let mut byte_len: u32 = 0;
        let query_res = RegQueryValueExW(
            hkey,
            PCWSTR(val_wide.as_ptr()),
            None,
            Some(&mut val_type),
            None,
            Some(&mut byte_len),
        );

        if query_res.is_err() || byte_len == 0 {
            let _ = RegCloseKey(hkey);
            return None;
        }

        let num_u16 = (byte_len as usize + 1) / 2;
        let mut buffer: Vec<u16> = vec![0u16; num_u16];
        let query_val = RegQueryValueExW(
            hkey,
            PCWSTR(val_wide.as_ptr()),
            None,
            Some(&mut val_type),
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut byte_len),
        );

        let _ = RegCloseKey(hkey);

        if query_val.is_ok() {
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            return String::from_utf16(&buffer[..len]).ok();
        }
    }
    None
}

#[cfg(windows)]
fn win32_enum_subkeys(root: HKEY, subkey: &str, sam: REG_SAM_FLAGS) -> Vec<String> {
    let mut results = Vec::new();
    let subkey_wide = to_wide(subkey);
    let mut hkey = HKEY::default();

    unsafe {
        let res = RegOpenKeyExW(root, PCWSTR(subkey_wide.as_ptr()), 0, sam, &mut hkey);
        if res.is_err() || hkey.is_invalid() {
            return results;
        }

        let mut index = 0u32;
        loop {
            let mut name_buf = [0u16; 260];
            let mut name_len = name_buf.len() as u32;
            let status = RegEnumKeyExW(
                hkey,
                index,
                PWSTR(name_buf.as_mut_ptr()),
                &mut name_len,
                None,
                PWSTR::null(),
                None,
                None,
            );

            if status.is_err() {
                break;
            }

            if let Ok(s) = String::from_utf16(&name_buf[..name_len as usize]) {
                results.push(s);
            }
            index += 1;
        }

        let _ = RegCloseKey(hkey);
    }
    results
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GameFileItem {
    pub rel: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GameExeOption {
    pub name: String,
    pub path: PathBuf,
    pub rel: String,
    pub api: String,
    pub bitness: u32,
    #[serde(default)]
    pub is_laa: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GameEntry {
    pub name: String,
    pub dir: PathBuf,
    pub exe_path: PathBuf,
    pub exe_rel: String,
    pub bitness: u32,
    #[serde(default)]
    pub is_laa: bool,
    pub api: String,
    pub dlss_version: Option<String>,
    pub has_frame_generation: bool,
    #[serde(default)]
    pub can_inject_fg: bool,
    pub optiscaler_installed: bool,
    pub optiscaler_presr: bool,
    pub optiscaler_passes: u32,
    pub mfg_unlock_installed: bool,
    pub has_backup: bool,
    pub launcher: String,
    pub poster: Option<String>,
    pub reshade_installed: bool,
    pub reshade_version: Option<String>,
    pub reshade_addon_support: bool,
    pub addon_installed: bool,
    pub installed_route: Option<String>,
    pub files: Vec<GameFileItem>,
    #[serde(default)]
    pub available_exes: Vec<GameExeOption>,
}

impl GameEntry {
    pub fn is_dlss5_patched(&self) -> bool {
        self.installed_route.is_some() || self.optiscaler_installed || self.reshade_installed
    }

    pub fn route_display_name(&self) -> &'static str {
        match self.installed_route.as_deref() {
            Some("feeder") => "Feeder · Neural Rendering",
            Some("native") => "Native D3D12",
            Some("optiscaler") => "OptiScaler",
            _ if self.optiscaler_installed => "OptiScaler",
            _ if self.reshade_installed => "ReShade",
            _ => "Vanilla",
        }
    }
}

pub fn short_version(v: &str) -> String {
    if let Some(stripped) = v.strip_suffix(".0") {
        stripped.to_string()
    } else {
        v.to_string()
    }
}

static RE_INSTALLER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(unins|setup|install|vcredist|vc_redist|dxsetup|dxwebsetup|oalinst|uninstall|crashreport|crashhandler|unitycrashhandler|unrealcefsubprocess|easyanticheat|eac|battleye|be_service|launcher|activation|patch|update|dotnetfx|touchup|rapidcrc|autorun|autoplay|quicksfv|readme|config|benchmark|report|helper|service|cleanup|modorganizer|redlauncher|skse\d*_loader|hlds |srcds |steamerrorreporter|dgvoodoocpl|dgvoodoo|reshade|optiscaler|dlss5-feed|specialk|skif|bg3modmanager|modmanager|vortex|fluffy|fomod)").unwrap()
});

static RE_NOT_GAME_DIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(steamapps|gamesave|gamesaves|workshop|downloading|shadercache|cache|caches|temp|tmp|backup|_dlss5_backup|reshade-shaders|host64|optiscaler|saves?|savegames?|redist|_?commonredist|__installer|installers?|setup|dlc|mods?|tools?|node_modules|\.git|bg3modmanager.*|.*modmanager.*|vortex.*|fluffy.*|modorganizer.*|trainers?|cheats?|sdk|patcher)$").unwrap()
});

static RE_NOT_GAME_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(redistributabl|steamworks common|directx|vcredist|proton|steam linux runtime|soundtrack|modmanager|mod manager|save editor|trainer|cheat engine|nexus mods|sdk )").unwrap()
});

static RE_CONTAINER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(games?|my ?games|steamlibrary|gog ?games|gog|epic ?games|epic|xbox ?games|origin ?games|repacks?|emulation)$").unwrap()
});

pub fn is_installer_or_helper(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower == "gamelaunchhelper.exe" || lower.starts_with("gamelaunchhelper") 
        || lower == "dlss5-feed-host64.exe" || lower.starts_with("dlss5-feed") || lower.contains("feed-host")
        || lower.starts_with("unitycrashhandler") || lower.contains("crashhandler") || lower.contains("crashreport")
        || lower == "unrealcefsubprocess.exe" || lower.contains("cefsubprocess") || lower.contains("webhelper")
        || lower.starts_with("dgvoodoo") || lower.starts_with("reshade") || lower.starts_with("optiscaler")
        || lower.starts_with("specialk") || lower == "skif.exe"
        || lower.starts_with("easyanticheat") || lower.starts_with("beservice") || lower.starts_with("start_protected_game")
        || lower == "python.exe" || lower == "pythonw.exe" || lower.starts_with("python.") || lower.starts_with("pythonw.")
        || lower == "zsync.exe" || lower == "zsyncmake.exe"
        || lower == "vrwebhelper.exe" || lower.starts_with("cef")
        || lower == "7za.exe" || lower == "7z.exe" || lower.starts_with("7z")
        || lower == "scc.exe" || lower == "unins000.exe" || lower.starts_with("unins")
        || lower.contains("prelauncher") || lower.contains("redprelauncher")
        || lower.contains("errorreporter") || lower.contains("crashreporter")
    {
        return true;
    }
    if lower.contains("installer") || lower.contains("uninstall") || lower.contains("crashreport") 
        || lower.contains("crashhandler") || lower.contains("vcredist") || lower.contains("dxsetup")
        || lower.contains("redist") || lower.contains("cleanup") || lower.contains("updater")
        || lower.contains("checker") || lower.contains("subprocess") || lower.contains("cefsharp")
        || lower.contains("browser") || lower.contains("crashmailer") || lower.contains("crashsender")
        || lower.contains("crash_report") || lower.contains("bugreport") || lower.contains("errorreport")
        || lower.contains("diagnostics") || lower.contains("benchmark")
        || lower.contains("prelauncher")
        || lower.contains("launcher")
    {
        return true;
    }
    RE_INSTALLER.is_match(&lower)
}

pub fn is_helper_or_tool_path(path: &Path) -> bool {
    for comp in path.components() {
        let s = comp.as_os_str().to_string_lossy().to_lowercase();
        if s == "host64" || s == "optiscaler" || s == "_dlss5_backup" || s == "reshade-shaders"
            || s == "crashreporter" || s == "crashreports" || s == "tools" || s == "tool"
            || s == "compiler" || s == "compilers" || s == "sdk" || s == "sdks"
            || s == "easyanticheat" || s == "battleye" || s == "anticheat"
            || s == "__installer" || s == "installer_resources" || s == "installers" || s == "installer"
            || s == "support" || s == "redist" || s == "_redist" || s == "commonredist" || s == "_commonredist"
            || s == "prerequisites" || s == "directx"
            || s == "launcher" || s == "launchers" {
            return true;
        }
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    is_installer_or_helper(name)
}

pub fn is_not_a_game_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "steamapps" | "gamesave" | "gamesaves" | "workshop" | "downloading" 
        | "shadercache" | "cache" | "caches" | "temp" | "tmp" | "backup" 
        | "_dlss5_backup" | "reshade-shaders" | "host64" | "optiscaler" | "save" | "saves" | "savegame" 
        | "savegames" | "redist" | "commonredist" | "_commonredist" 
        | "__installer" | "installer" | "installers" | "setup" | "dlc" | "mods" | "mod" 
        | "tools" | "tool" | "node_modules" | ".git" | "sdk" | "patcher" => return true,
        _ => {}
    }
    if lower.contains("modmanager") || lower.contains("vortex") || lower.contains("fluffy") || lower.contains("trainer") {
        return true;
    }
    RE_NOT_GAME_DIR.is_match(&lower)
}

pub fn is_not_a_game_title(name: &str) -> bool {
    let lower = name.to_lowercase();
    RE_NOT_GAME_TITLE.is_match(&lower)
}

pub fn is_library_container_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    RE_CONTAINER.is_match(&lower)
}

pub fn holds_game<P: AsRef<Path>>(dir: P, depth: usize) -> bool {
    let dir = dir.as_ref();
    let Ok(entries) = fs::read_dir(dir) else { return false; };
    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                let lower = fname.to_lowercase();
                if lower.ends_with(".exe") && !is_helper_or_tool_path(&path) {
                    return true;
                }
            }
        } else if path.is_dir() && depth > 0 {
            if let Some(dname) = path.file_name().and_then(|n| n.to_str()) {
                if !is_not_a_game_dir(dname) {
                    subdirs.push(path);
                }
            }
        }
    }

    if depth == 0 {
        return false;
    }

    for sub in subdirs {
        if holds_game(&sub, depth - 1) {
            return true;
        }
    }

    false
}

pub fn api_from_names(imports: &[String]) -> Option<String> {
    let has = |n: &str| imports.iter().any(|i| i == n || i.ends_with(&format!("\\{}", n)) || i.ends_with(&format!("/{}", n)));
    if has("d3d12.dll") {
        return Some("DirectX 12".to_string());
    }
    if has("vulkan-1.dll") {
        return Some("Vulkan".to_string());
    }
    if has("d3d11.dll") {
        return Some("DirectX 11".to_string());
    }
    if has("d3d10.dll") || has("d3d10_1.dll") {
        return Some("DirectX 10".to_string());
    }
    if has("dxgi.dll") {
        return Some("DirectX (DXGI)".to_string());
    }
    if has("d3d9.dll") {
        return Some("DirectX 9".to_string());
    }
    if has("d3d8.dll") {
        return Some("DirectX 8".to_string());
    }
    None
}

pub fn api_from_markers(path: &Path) -> Option<String> {
    let markers = &[
        "D3D12CreateDevice", "D3D12SDKPath", "D3D12SDKVersion",
        "D3D11CreateDevice", "D3D10CreateDevice",
        "Direct3DCreate9", "Direct3DCreate8", "CreateDXGIFactory", "vkCreateInstance", "wglCreateContext"
    ];
    let found = find_markers(path, markers);
    if found.iter().any(|m| m == "D3D12CreateDevice" || m == "D3D12SDKPath" || m == "D3D12SDKVersion") {
        return Some("DirectX 12".to_string());
    }
    if found.iter().any(|m| m == "vkCreateInstance") {
        return Some("Vulkan".to_string());
    }
    if found.iter().any(|m| m == "D3D11CreateDevice") {
        return Some("DirectX 11".to_string());
    }
    if found.iter().any(|m| m == "D3D10CreateDevice") {
        return Some("DirectX 10".to_string());
    }
    if found.iter().any(|m| m == "CreateDXGIFactory") {
        return Some("DirectX (DXGI)".to_string());
    }
    if found.iter().any(|m| m == "Direct3DCreate9") {
        return Some("DirectX 9".to_string());
    }
    if found.iter().any(|m| m == "Direct3DCreate8") {
        return Some("DirectX 8".to_string());
    }
    if found.iter().any(|m| m == "wglCreateContext") {
        return Some("OpenGL".to_string());
    }
    None
}

pub fn detect_renpy_api(dir: &Path) -> Option<String> {
    let is_renpy = dir.join("renpy").is_dir()
        || dir.join("lib").join("windows-x86_64").join("librenpython.dll").exists()
        || dir.join("lib").join("windows-i686").join("librenpython.dll").exists()
        || dir.join("librenpython.dll").exists();

    if !is_renpy {
        return None;
    }

    // Inspect log.txt in game directory if available
    let log_path = dir.join("log.txt");
    if let Ok(content) = fs::read_to_string(&log_path) {
        let c_lower = content.to_lowercase();
        if c_lower.contains("angle2") || c_lower.contains("directx 11") || c_lower.contains("d3d11") {
            return Some("DirectX 11".to_string());
        }
        if c_lower.contains("angle") || c_lower.contains("directx 9") || c_lower.contains("d3d9") {
            return Some("DirectX 9".to_string());
        }
    }

    // Ren'Py's default renderer on Windows is gl2 (Desktop OpenGL)
    Some("OpenGL".to_string())
}

fn detect_sibling_api(dir: &Path) -> Option<String> {
    if let Some(api) = detect_renpy_api(dir) {
        return Some(api);
    }
    if dir.join("D3D12Core.dll").exists() || dir.join("D3D12").join("D3D12Core.dll").exists() {
        return Some("DirectX 12".to_string());
    }

    let Ok(entries) = fs::read_dir(dir) else { return None; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let is_dll = path.extension().map(|e| e.to_string_lossy().eq_ignore_ascii_case("dll")).unwrap_or(false);
            if !is_dll {
                continue;
            }
            let fname = entry.file_name().to_string_lossy().to_lowercase();
            // Skip generic middleware DLLs
            if fname.starts_with("sdl") || fname.starts_with("bink") || fname.starts_with("fmod") 
                || fname.starts_with("libxess") || fname.starts_with("nvngx") || fname.starts_with("amd_") {
                continue;
            }
            if fname.contains("dx12") || fname.contains("d3d12") {
                return Some("DirectX 12".to_string());
            }
            if fname.contains("vulkan") {
                return Some("Vulkan".to_string());
            }
            if fname.contains("dx11") || fname.contains("d3d11") {
                return Some("DirectX 11".to_string());
            }
        }
    }
    None
}
pub fn detect_api_for_exe(path: &Path) -> Option<String> {
    let imports = crate::core::pe::inspect_pe(path).map(|p| p.imports).unwrap_or_default();
    detect_api(path, &imports)
}

pub fn detect_api(path: &Path, imports: &[String]) -> Option<String> {
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if fname.contains("dx12") || fname.contains("d3d12") {
        return Some("DirectX 12".to_string());
    }
    if fname.contains("dx11") || fname.contains("d3d11") {
        return Some("DirectX 11".to_string());
    }
    if fname.contains("vulkan") {
        return Some("Vulkan".to_string());
    }

    if let Some(parent) = path.parent() {
        if let Some(api) = detect_renpy_api(parent) {
            return Some(api);
        }
    }
    if imports.iter().any(|i| i.to_lowercase().contains("librenpython")) {
        if let Some(parent) = path.parent() {
            if let Some(api) = detect_renpy_api(parent) {
                return Some(api);
            }
        }
        return Some("OpenGL".to_string());
    }

    if let Some(api) = api_from_names(imports) {
        return Some(api);
    }
    if let Some(api) = api_from_markers(path) {
        return Some(api);
    }
    if imports.iter().any(|i| i == "opengl32.dll" || i.ends_with("\\opengl32.dll") || i.ends_with("/opengl32.dll")) {
        return Some("OpenGL".to_string());
    }
    if let Some(parent) = path.parent() {
        // Check imported sibling DLLs (Control's d3d_rmdwin10_f.dll, Ren'Py's librenpython.dll)
        // Skip generic third-party middleware (SDL, Bink, audio engines, upscalers, store SDKs)
        for name in imports.iter().take(80) {
            let n_lower = name.to_lowercase();
            if n_lower.starts_with("sdl") || n_lower.starts_with("bink") || n_lower.starts_with("fmod") 
                || n_lower.starts_with("libxess") || n_lower.starts_with("nvngx") || n_lower.starts_with("amd_")
                || n_lower.starts_with("galaxy") || n_lower.starts_with("discord") || n_lower.starts_with("steam")
                || n_lower.starts_with("party") || n_lower.starts_with("playfab") {
                continue;
            }
            let sib_path = parent.join(name);
            if sib_path.is_file() {
                if let Some(sib_pe) = inspect_pe(&sib_path) {
                    if let Some(api) = api_from_names(&sib_pe.imports) {
                        return Some(api);
                    }
                    if let Some(api) = api_from_markers(&sib_path) {
                        return Some(api);
                    }
                }
            }
        }
        if let Some(api) = detect_sibling_api(parent) {
            return Some(api);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct XboxExe {
    pub path: PathBuf,
    pub rel: String,
    pub name: String,
    pub bitness: u32,
}

pub fn xbox_executables(game_dir: &Path) -> Vec<XboxExe> {
    let mut configs = Vec::new();
    let root_cfg = game_dir.join("MicrosoftGame.config");
    if root_cfg.exists() {
        configs.push(root_cfg);
    }
    let content_cfg = game_dir.join("Content").join("MicrosoftGame.config");
    if content_cfg.exists() {
        configs.push(content_cfg);
    }

    let mut found = Vec::new();
    let exe_tag_re = Regex::new(r#"(?i)<Executable\b([^>]*)/?>"#).unwrap();
    let name_attr_re = Regex::new(r#"(?i)\bName\s*=\s*"([^"]+)""#).unwrap();
    let arch_attr_re = Regex::new(r#"(?i)\bArchitecture\s*=\s*"([^"]+)""#).unwrap();
    let proc_arch_re = Regex::new(r#"(?i)<ProcessorArchitecture>\s*([^<]+)\s*</ProcessorArchitecture>"#).unwrap();

    for config in configs {
        let Ok(text) = fs::read_to_string(&config) else { continue; };
        let cfg_dir = config.parent().unwrap_or(game_dir);

        let mut default_bitness = 64;
        if let Some(caps) = proc_arch_re.captures(&text) {
            let arch = caps.get(1).map(|m| m.as_str().trim().to_lowercase()).unwrap_or_default();
            if arch == "x86" {
                default_bitness = 32;
            }
        }

        for cap in exe_tag_re.captures_iter(&text) {
            let tag_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = if let Some(nc) = name_attr_re.captures(tag_str) {
                nc.get(1).map(|m| m.as_str().trim()).unwrap_or("")
            } else {
                continue;
            };

            let base_name = Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or(name);
            if base_name.eq_ignore_ascii_case("gamelaunchhelper.exe") || base_name.to_lowercase().starts_with("gamelaunchhelper") {
                continue;
            }

            let bitness = if let Some(ac) = arch_attr_re.captures(tag_str) {
                let arch = ac.get(1).map(|m| m.as_str().trim().to_lowercase()).unwrap_or_default();
                if arch == "x86" { 32 } else { 64 }
            } else {
                default_bitness
            };

            let full = cfg_dir.join(name.replace('/', "\\"));
            let rel = full.strip_prefix(game_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| base_name.to_string());

            found.push(XboxExe {
                path: full,
                rel,
                name: base_name.to_string(),
                bitness,
            });
        }
    }
    found
}

/// Discovers officially declared game executables from Xbox configs (MicrosoftGame.config, appxmanifest.xml)
/// and GOG manifests (goggame-*.info).
pub fn declared_executables(dir: &Path) -> (Vec<XboxExe>, Option<String>) {
    let mut found = xbox_executables(dir);
    let mut launcher = if !found.is_empty() {
        Some("Xbox".to_string())
    } else {
        None
    };

    // GOG manifests: goggame-*.info
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("goggame-") && name.ends_with(".info") {
                if launcher.is_none() {
                    launcher = Some("GOG".to_string());
                }
                if let Ok(text) = fs::read_to_string(entry.path()) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(tasks) = val.get("playTasks").and_then(|v| v.as_array()) {
                            for task in tasks {
                                if let Some(cat) = task.get("category").and_then(|c| c.as_str()) {
                                    if cat != "game" {
                                        continue;
                                    }
                                }
                                if let Some(path_str) = task.get("path").and_then(|p| p.as_str()) {
                                    let full = dir.join(path_str.replace('/', "\\"));
                                    if full.is_file() {
                                        let file_name = full.file_name().unwrap_or_default().to_string_lossy().to_string();
                                        if !is_installer_or_helper(&file_name) && !found.iter().any(|x| x.path == full) {
                                            let pe_opt = inspect_pe(&full);
                                            let bitness = pe_opt.as_ref().map(|p| p.bitness).unwrap_or(64);
                                            let rel = full.strip_prefix(dir)
                                                .map(|p| p.to_string_lossy().to_string())
                                                .unwrap_or_else(|_| path_str.to_string());
                                            found.push(XboxExe {
                                                path: full,
                                                rel,
                                                name: file_name,
                                                bitness,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    (found, launcher)
}

#[derive(Debug, Clone)]
struct Candidate {
    path: PathBuf,
    rel: String,
    name: String,
    size: u64,
    depth: usize,
    bitness: u32,
    is_laa: bool,
    api: String,
    declared: bool,
    has_sibling_dlss: bool,
    is_dx12: bool,
}

fn playable_role_score(name: &str, rel: &str) -> i64 {
    let lower = format!("{} {}", name, rel).to_lowercase();
    let mut score = 0;
    // Prioritize actual Unreal Engine gameplay shipping binaries over root bootstrap wrappers
    if lower.contains("shipping.exe") || lower.ends_with("-shipping.exe") || lower.ends_with("_shipping.exe") || lower.contains("win64-shipping") || lower.contains("wingdk-shipping") {
        score += 15000;
    }
    if lower.contains("singleplayer") || lower.ends_with("sp.exe") || lower.contains("_sp") {
        score += 400;
    }
    if lower.contains("multiplayer") || lower.ends_with("mp.exe") || lower.contains("_mp") {
        score -= 400;
    }
    // Deprioritize legacy 32-bit fallback executables when 64-bit binaries exist
    if lower.contains("-32") || lower.contains("_32") || lower.contains("32bit") || lower.contains("win32") {
        score -= 2000;
    }
    score
}

fn candidate_score(c: &Candidate, dir_name: &str) -> i64 {
    let mut score = 0;
    if c.declared {
        score += 20000;
    }
    if c.has_sibling_dlss {
        score += 10000;
    }
    if c.is_dx12 {
        score += 5000;
    }
    if c.bitness == 64 {
        score += 1000;
    }
    // Prefer executables closer to root directory
    if c.depth <= 1 {
        score += 3000;
    }
    // Boost executables matching the game folder name (e.g. BeingADIK.exe vs "Being a DIK")
    let norm_dir: String = dir_name.chars().filter(|ch| ch.is_alphanumeric()).flat_map(|ch| ch.to_lowercase()).collect();
    let norm_name: String = c.name.chars().filter(|ch| ch.is_alphanumeric()).flat_map(|ch| ch.to_lowercase()).collect();
    if !norm_dir.is_empty() && (norm_name.starts_with(&norm_dir) || norm_dir.starts_with(&norm_name)) {
        score += 8000;
    }
    score += playable_role_score(&c.name, &c.rel);
    score
}

pub fn extract_xbox_metadata(dir: &Path) -> (Option<String>, Option<String>) {
    let display_name_re = Regex::new(r#"(?i)DefaultDisplayName\s*=\s*"([^"]+)""#).unwrap();
    let display_name_tag_re = Regex::new(r#"(?i)<DisplayName>\s*([^<]+)\s*</DisplayName>"#).unwrap();
    let splash_re = Regex::new(r#"(?i)SplashScreenImage\s*=\s*"([^"]+)""#).unwrap();
    let logo_re = Regex::new(r#"(?i)Square150x150Logo\s*=\s*"([^"]+)""#).unwrap();
    let store_logo_re = Regex::new(r#"(?i)StoreLogo\s*=\s*"([^"]+)""#).unwrap();

    let mut resolved_name = None;
    let mut resolved_poster = None;

    let configs = [
        dir.join("MicrosoftGame.config"),
        dir.join("Content").join("MicrosoftGame.config"),
        dir.join("appxmanifest.xml"),
        dir.join("AppxManifest.xml"),
    ];

    for cfg in &configs {
        if let Ok(cfg_text) = fs::read_to_string(cfg) {
            if resolved_name.is_none() {
                if let Some(cap) = display_name_re.captures(&cfg_text).or_else(|| display_name_tag_re.captures(&cfg_text)) {
                    let name = cap[1].trim();
                    if !name.is_empty() && !name.starts_with("ms-resource:") {
                        resolved_name = Some(name.to_string());
                    }
                }
            }

            if resolved_poster.is_none() {
                if let Some(cap) = splash_re.captures(&cfg_text).or_else(|| logo_re.captures(&cfg_text)).or_else(|| store_logo_re.captures(&cfg_text)) {
                    let rel_img = cap[1].replace('/', "\\");
                    let img_path = dir.join(&rel_img);
                    if img_path.exists() {
                        resolved_poster = crate::core::steamart::file_to_art_uri(&img_path);
                    }
                }
            }

            if resolved_name.is_some() && resolved_poster.is_some() {
                break;
            }
        }
    }

    (resolved_name, resolved_poster)
}

pub fn is_generic_folder_name(s: &str) -> bool {
    let lower = s.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "bin" | "x64" | "x86" | "win64" | "win32" | "binaries" | "retail" | "release" | "shipping" | "game" | "client"
    )
}

pub fn infer_game_name(dir: &Path, exe_path: &Path, xbox_name: Option<String>) -> String {
    if let Some(name) = xbox_name {
        if !name.trim().is_empty() {
            return name;
        }
    }

    let mut candidate_dir = dir;
    let mut resolved_name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();

    while is_generic_folder_name(&resolved_name) {
        if let Some(parent) = candidate_dir.parent() {
            if let Some(p_name) = parent.file_name() {
                resolved_name = p_name.to_string_lossy().to_string();
                candidate_dir = parent;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if is_generic_folder_name(&resolved_name) || resolved_name.trim().is_empty() {
        let exe_stem = exe_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        if !exe_stem.is_empty() {
            return exe_stem;
        }
    }

    if resolved_name.trim().is_empty() {
        "Unknown Game".to_string()
    } else {
        resolved_name
    }
}

pub fn scan_game_directory<P: AsRef<Path>>(dir: P) -> Option<GameEntry> {
    let dir = dir.as_ref();
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    let mut dlss_files: Vec<(PathBuf, Option<String>)> = Vec::new();
    let mut has_fg = false;
    let mut mfg_addon = false;
    let mut addon_installed = false;
    let mut optiscaler_installed = false;
    let mut optiscaler_presr = false;
    let mut optiscaler_passes = 1;
    let mut poster = None;

    let (declared, declared_launcher) = declared_executables(dir);
    let mut candidates: Vec<Candidate> = Vec::new();

    let skip_dirs = [
        "_dlss5_backup", "reshade-shaders", "optiscaler", "host64", "paks", "movies", "saves", "logs",
        "node_modules", ".git", "data", "audio", "sound", "sounds", "music",
        "textures", "cinematics", "localization", "streamingassets", "shaders",
        "__installer", "installer_resources", "installers", "installer", "support", "redist", "_redist", "commonredist", "_commonredist", "prerequisites",
        "cache", "caches", "shadercache", "soundbanks", "soundbank", "video", "videos", "datas", "fonts", "font",
        "renpy", "crashreporter", "crashreports", "tools", "tool", "compiler", "compilers", "easyanticheat", "battleye",
        "launcher", "launchers"
    ];

    for entry in WalkDir::new(dir)
        .max_depth(5)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                !skip_dirs.contains(&name.as_str())
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            let is_inside_mod_dir = path.components().any(|c| {
                let s = c.as_os_str().to_string_lossy().to_lowercase();
                s == "optiscaler" || s == "_dlss5_backup" || s == "reshade-shaders"
            });
            let file_name = entry.file_name().to_string_lossy().to_lowercase();
            if file_name.ends_with(".exe") && !is_helper_or_tool_path(path) {
                let t_exe = std::time::Instant::now();
                let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                let depth = entry.depth();
                let is_declared = declared.iter().any(|x| x.path == path);
                let declared_match = declared.iter().find(|x| x.path == path);

                let pe_opt = inspect_pe(path);
                let bitness = if let Some(ref pe) = pe_opt {
                    pe.bitness
                } else if let Some(dm) = declared_match {
                    dm.bitness
                } else {
                    64
                };

                let parent_dir = path.parent();
                let has_sibling_dlss = parent_dir.map(|p| p.join("nvngx_dlss.dll").exists()).unwrap_or(false);

                let detected_api = if let Some(ref pe) = pe_opt {
                    detect_api(path, &pe.imports)
                } else if let Some(parent) = parent_dir {
                    detect_sibling_api(parent)
                } else {
                    None
                };

                let api = match detected_api {
                    Some(a) => a,
                    None => {
                        if let Some(parent) = parent_dir {
                            if let Some(renpy_api) = detect_renpy_api(parent) {
                                renpy_api
                            } else if is_declared || depth <= 1 {
                                "DirectX 11".to_string()
                            } else {
                                println!("EXE REJECTED: {} took {:?}", path.display(), t_exe.elapsed());
                                continue;
                            }
                        } else if is_declared || depth <= 1 {
                            "DirectX 11".to_string()
                        } else {
                            println!("EXE REJECTED: {} took {:?}", path.display(), t_exe.elapsed());
                            continue;
                        }
                    }
                };
                println!("EXE ACCEPTED: {} took {:?}", path.display(), t_exe.elapsed());

                let is_dx12 = api == "DirectX 12";
                let rel = path.strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| entry.file_name().to_string_lossy().to_string());

                let is_laa = pe_opt.as_ref().map(|p| p.is_laa).unwrap_or(bitness == 64);
                candidates.push(Candidate {
                    path: path.to_path_buf(),
                    rel,
                    name: entry.file_name().to_string_lossy().to_string(),
                    size,
                    depth,
                    bitness,
                    is_laa,
                    api,
                    declared: is_declared,
                    has_sibling_dlss,
                    is_dx12,
                });
            } else if file_name.ends_with(".addon64") || file_name.ends_with(".addon32") || file_name.ends_with(".addon") {
                addon_installed = true;
                if file_name.contains("mfgunlock") {
                    mfg_addon = true;
                }
            } else if (file_name == "nvngx_dlss.dll" || file_name == "_nvngx.dll" || file_name == "nvngx.dll" || file_name == "nvngx_dlssnr.dll") && !is_inside_mod_dir {
                let pe_opt = inspect_pe(path);
                dlss_files.push((path.to_path_buf(), pe_opt.and_then(|p| p.version)));
            } else if (file_name == "nvngx_dlssg.dll" || file_name == "sl.dlss_g.dll" || file_name == "sl.dlss.dll" || file_name.contains("framegeneration_dx12") || file_name == "fgvk.dll") && !is_inside_mod_dir {
                has_fg = true;
                let pe_opt = inspect_pe(path);
                dlss_files.push((path.to_path_buf(), pe_opt.and_then(|p| p.version)));
            } else if file_name == "optiscaler.dll" || file_name == "optiscaler.ini" {
                optiscaler_installed = true;
                if file_name == "optiscaler.ini" {
                    if let Ok(ini_text) = fs::read_to_string(path) {
                        for line in ini_text.lines() {
                            let trimmed = line.trim();
                            if trimmed.starts_with("RunBeforeSR") && trimmed.contains("true") {
                                optiscaler_presr = true;
                            } else if trimmed.starts_with("PreSRMultipassCount") {
                                if let Some(val) = trimmed.split('=').nth(1) {
                                    if let Ok(p) = val.trim().parse::<u32>() {
                                        optiscaler_passes = p;
                                    }
                                }
                            }
                        }
                    }
                }
            } else if (file_name.contains("library_600x900") || file_name == "cover.jpg" || file_name == "poster.jpg") && poster.is_none() {
                poster = crate::core::steamart::file_to_art_uri(&path);
            }
        }
    }

    // Add manifest-declared executables that were not visited or had inspect_pe fail
    for d in &declared {
        if !d.path.exists() {
            continue;
        }
        if !candidates.iter().any(|c| c.path == d.path) {
            let size = fs::metadata(&d.path).map(|m| m.len()).unwrap_or(0);
            let parent_dir = d.path.parent();
            let has_sibling_dlss = parent_dir.map(|p| p.join("nvngx_dlss.dll").exists()).unwrap_or(false);
            let sibling_api = parent_dir.and_then(detect_sibling_api);
            let api = sibling_api.unwrap_or_else(|| "DirectX 11".to_string());
            let is_dx12 = api == "DirectX 12";

            let pe_d = crate::core::pe::inspect_pe(&d.path);
            let is_laa = pe_d.as_ref().map(|p| p.is_laa).unwrap_or(d.bitness == 64);

            candidates.push(Candidate {
                path: d.path.clone(),
                rel: d.rel.clone(),
                name: d.name.clone(),
                size,
                depth: d.rel.split(['/', '\\']).count().saturating_sub(1),
                bitness: d.bitness,
                is_laa,
                api,
                declared: true,
                has_sibling_dlss,
                is_dx12,
            });
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // If an Unreal Engine shipping binary exists in subdirectories (*-Shipping.exe),
    // filter out root-level dummy bootstrap wrappers (depth <= 1).
    let has_shipping_exe = candidates.iter().any(|c| {
        let l = c.name.to_lowercase();
        l.contains("shipping.exe") || l.ends_with("-shipping.exe") || l.ends_with("_shipping.exe")
    });
    if has_shipping_exe {
        candidates.retain(|c| {
            let l = c.name.to_lowercase();
            let is_shipping = l.contains("shipping.exe") || l.ends_with("-shipping.exe") || l.ends_with("_shipping.exe");
            is_shipping || c.depth > 1
        });
    }

    let dir_name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
    candidates.sort_by(|a, b| {
        let a_score = candidate_score(a, &dir_name);
        let b_score = candidate_score(b, &dir_name);
        b_score.cmp(&a_score)
            .then_with(|| a.depth.cmp(&b.depth))
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| a.rel.cmp(&b.rel))
    });

    let available_exes: Vec<GameExeOption> = candidates.iter().map(|c| GameExeOption {
        name: c.name.clone(),
        path: c.path.clone(),
        rel: c.rel.clone(),
        api: c.api.clone(),
        bitness: c.bitness,
        is_laa: c.is_laa,
    }).collect();

    let chosen = candidates.remove(0);

    // Check ReShade hooks
    let mut reshade_installed = false;
    let mut reshade_version = None;
    let mut reshade_addon_support = false;

    let search_dirs = [chosen.path.parent(), Some(dir)];
    let hook_names = ["dxgi.dll", "d3d12.dll", "d3d11.dll", "d3d9.dll", "opengl32.dll", "dinput8.dll"];
    for s_dir in search_dirs.into_iter().flatten() {
        for hook in &hook_names {
            let hook_path = s_dir.join(hook);
            if hook_path.is_file() {
                let (mentions, ver, addon_sup) = crate::core::pe::is_reshade_dll(&hook_path);
                if mentions {
                    reshade_installed = true;
                    reshade_version = ver;
                    reshade_addon_support = addon_sup;
                    break;
                }
            }
        }
        if reshade_installed {
            break;
        }
    }

    // Check backup manifest
    let has_backup = crate::core::journal::has_backup_available(dir);

    let mut installed_route = None;
    if let Some(manifest) = crate::core::journal::read_manifest(dir) {
        if !manifest.route.is_empty() {
            installed_route = Some(manifest.route.clone());
            if manifest.route == "feeder" {
                addon_installed = true;
                reshade_installed = true;
            } else if manifest.route == "optiscaler" {
                optiscaler_installed = true;
            }
        }
    }

    // GDK / MicrosoftGame.config fallback:
    // If Xbox declared and api is DirectX 12 without explicit D3D12 SDK, label as DirectX 11/12
    let api = if chosen.declared && chosen.api == "DirectX 12" {
        let has_d3d12_sdk = chosen.path.parent().map(|p| p.join("D3D12Core.dll").exists() || p.join("D3D12").join("D3D12Core.dll").exists()).unwrap_or(false);
        if !has_d3d12_sdk {
            "DirectX 11/12".to_string()
        } else {
            chosen.api.clone()
        }
    } else {
        chosen.api.clone()
    };

    let chosen_dir = chosen.path.parent();
    dlss_files.sort_by(|a, b| {
        let a_same_dir = chosen_dir.map(|p| p == a.0.parent().unwrap_or(p)).unwrap_or(false);
        let b_same_dir = chosen_dir.map(|p| p == b.0.parent().unwrap_or(p)).unwrap_or(false);
        let a_is_primary = a.0.file_name().map(|n| n.to_string_lossy().to_lowercase() == "nvngx_dlss.dll").unwrap_or(false);
        let b_is_primary = b.0.file_name().map(|n| n.to_string_lossy().to_lowercase() == "nvngx_dlss.dll").unwrap_or(false);
        (b_same_dir as u8).cmp(&(a_same_dir as u8))
            .then_with(|| (b_is_primary as u8).cmp(&(a_is_primary as u8)))
            .then_with(|| (b.1.is_some() as u8).cmp(&(a.1.is_some() as u8)))
    });
    let sr_file = dlss_files.iter().find(|(p, _)| {
        let n = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        n == "nvngx_dlss.dll" || n == "_nvngx.dll" || n == "nvngx.dll"
    });
    let dlss_version = sr_file.and_then(|f| f.1.clone());

    let mut files: Vec<GameFileItem> = Vec::new();
    for (f_path, ver) in &dlss_files {
        let rel = f_path.strip_prefix(dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| f_path.file_name().unwrap_or_default().to_string_lossy().to_string());
        if !files.iter().any(|existing| existing.rel == rel) {
            files.push(GameFileItem { rel, version: ver.clone() });
        }
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));

    if poster.is_none() {
        let art_dir = crate::core::state::get_appdata_dir().join("art");
        let key = crate::core::steamart::key_for_dir(dir);
        let cover_file = art_dir.join(format!("{}-cover.jpg", key));
        let hero_file = art_dir.join(format!("{}-hero.jpg", key));
        if cover_file.exists() && cover_file.metadata().map(|m| m.len() > 2000).unwrap_or(false) {
            poster = Some(format!("http://dlss-art.localhost/art/{}-cover.jpg", key));
        } else if hero_file.exists() && hero_file.metadata().map(|m| m.len() > 2000).unwrap_or(false) {
            poster = Some(format!("http://dlss-art.localhost/art/{}-hero.jpg", key));
        }
    }

    let (xbox_name, xbox_poster) = extract_xbox_metadata(dir);
    if poster.is_none() {
        poster = xbox_poster;
    }

    let is_xbox_dir = dir.join("MicrosoftGame.config").exists()
        || dir.join("Content").join("MicrosoftGame.config").exists()
        || dir.join("AppxManifest.xml").exists()
        || dir.join("appxmanifest.xml").exists()
        || dir.to_string_lossy().to_lowercase().contains("xboxgames")
        || dir.to_string_lossy().to_lowercase().contains("windowsapps");
    let name = infer_game_name(dir, &chosen.path, xbox_name);

    crate::core::logger::debug("scan", &format!(
        "Game scanned '{}': exe={}, bitness={}-bit, api={}, dlss={:?}, fg={}, optiscaler={}, backup={}",
        name, chosen.rel, chosen.bitness, api, dlss_version, has_fg, optiscaler_installed, has_backup
    ));

    let detected_launcher = if let Some(dl) = declared_launcher {
        dl
    } else if is_xbox_dir {
        "Xbox".to_string()
    } else if dir.join("steam_appid.txt").exists()
        || dir.to_string_lossy().to_lowercase().contains("steamapps")
    {
        "Steam".to_string()
    } else if dir.join(".egstore").exists()
        || dir.to_string_lossy().to_lowercase().contains("epic games")
    {
        "Epic Games".to_string()
    } else if fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.file_name().to_string_lossy().to_lowercase().starts_with("goggame-")
            })
        })
        .unwrap_or(false)
        || dir.to_string_lossy().to_lowercase().contains("gog")
    {
        "GOG".to_string()
    } else {
        "Added by hand".to_string()
    };

    let is_mod_added_dlss = if let Some(manifest) = crate::core::journal::read_manifest(dir) {
        if let Some((sr_p, _)) = &sr_file {
            let rel = sr_p.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| sr_p.file_name().unwrap_or_default().to_string_lossy().to_string());
            manifest.added.iter().any(|a| a.eq_ignore_ascii_case(&rel))
        } else {
            false
        }
    } else {
        false
    };
    let has_native_dlss = sr_file.is_some() && !is_mod_added_dlss;
    let is_vulkan = chosen.api.to_lowercase().contains("vulkan");
    let can_inject_fg = chosen.bitness == 64 && has_native_dlss && !has_fg && is_vulkan;

    Some(GameEntry {
        name,
        dir: dir.to_path_buf(),
        exe_path: chosen.path,
        exe_rel: chosen.rel,
        bitness: chosen.bitness,
        is_laa: chosen.is_laa,
        api,
        dlss_version,
        has_frame_generation: has_fg,
        can_inject_fg,
        optiscaler_installed,
        optiscaler_presr,
        optiscaler_passes,
        mfg_unlock_installed: mfg_addon,
        has_backup,
        launcher: detected_launcher,
        poster,
        reshade_installed,
        reshade_version,
        reshade_addon_support,
        addon_installed,
        installed_route,
        files,
        available_exes,
    })
}

/// Helper to discover all valid game executables in a game directory on demand.
pub fn discover_game_exes(dir: &Path) -> Vec<GameExeOption> {
    let mut exes = Vec::new();
    let (declared, _) = declared_executables(dir);
    for d in &declared {
        let pe_opt = inspect_pe(&d.path);
        let bitness = pe_opt.as_ref().map(|p| p.bitness).unwrap_or(d.bitness);
        let detected_api = if let Some(ref pe) = pe_opt {
            detect_api(&d.path, &pe.imports)
        } else if let Some(api) = detect_api(&d.path, &[]) {
            Some(api)
        } else if let Some(parent) = d.path.parent() {
            detect_sibling_api(parent)
        } else {
            None
        };
        let is_laa = pe_opt.as_ref().map(|p| p.is_laa).unwrap_or(bitness == 64);
        let api = detected_api.unwrap_or_else(|| "DirectX 11".to_string());
        exes.push(GameExeOption {
            name: d.name.clone(),
            path: d.path.clone(),
            rel: d.rel.clone(),
            api,
            bitness,
            is_laa,
        });
    }

    let skip_dirs = [
        "_dlss5_backup", "reshade-shaders", "optiscaler", "host64", "paks", "movies", "saves", "logs",
        "node_modules", ".git", "data", "audio", "sound", "sounds", "music",
        "textures", "cinematics", "localization", "streamingassets", "shaders",
        "__installer", "installer_resources", "installers", "installer", "support", "redist", "_redist", "commonredist", "_commonredist", "prerequisites",
        "cache", "caches", "shadercache", "soundbanks", "soundbank", "video", "videos", "datas", "fonts", "font",
        "renpy", "crashreporter", "crashreports", "tools", "tool", "compiler", "compilers", "easyanticheat", "battleye",
        "launcher", "launchers"
    ];

    for entry in WalkDir::new(dir)
        .max_depth(5)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                !skip_dirs.contains(&name.as_str())
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            let file_name = entry.file_name().to_string_lossy().to_lowercase();
            if file_name.ends_with(".exe") && !is_helper_or_tool_path(path) {
                if exes.iter().any(|e| e.path == path) {
                    continue;
                }
                let pe_opt = inspect_pe(path);
                let bitness = pe_opt.as_ref().map(|p| p.bitness).unwrap_or(64);
                let is_laa = pe_opt.as_ref().map(|p| p.is_laa).unwrap_or(bitness == 64);
                let detected_api = if let Some(ref pe) = pe_opt {
                    detect_api(path, &pe.imports)
                } else if let Some(api) = detect_api(path, &[]) {
                    Some(api)
                } else if let Some(parent) = path.parent() {
                    detect_sibling_api(parent)
                } else {
                    None
                };

                let api = detected_api.unwrap_or_else(|| "DirectX 11".to_string());
                let rel = path.strip_prefix(dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| entry.file_name().to_string_lossy().to_string());
                exes.push(GameExeOption {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: path.to_path_buf(),
                    rel,
                    api,
                    bitness,
                    is_laa,
                });
            }
        }
    }

    // If an Unreal Engine shipping binary exists in subdirectories (*-Shipping.exe),
    // filter out root-level dummy bootstrap wrappers (depth <= 1).
    let has_shipping_exe = exes.iter().any(|e| {
        let l = e.name.to_lowercase();
        l.contains("shipping.exe") || l.ends_with("-shipping.exe") || l.ends_with("_shipping.exe")
    });
    if has_shipping_exe {
        exes.retain(|e| {
            let l = e.name.to_lowercase();
            let is_shipping = l.contains("shipping.exe") || l.ends_with("-shipping.exe") || l.ends_with("_shipping.exe");
            let depth = e.rel.matches(['/', '\\']).count();
            is_shipping || depth > 1
        });
    }

    let dir_name: String = dir.file_name().unwrap_or_default().to_string_lossy().chars().filter(|ch| ch.is_alphanumeric()).flat_map(|ch| ch.to_lowercase()).collect();
    exes.sort_by(|a, b| {
        let a_is_shipping = a.name.to_lowercase().contains("shipping.exe");
        let b_is_shipping = b.name.to_lowercase().contains("shipping.exe");
        let a_decl_pos = declared.iter().position(|d| d.path == a.path).unwrap_or(usize::MAX);
        let b_decl_pos = declared.iter().position(|d| d.path == b.path).unwrap_or(usize::MAX);
        let a_is_32 = a.name.to_lowercase().contains("-32") || a.name.to_lowercase().contains("_32") || a.name.to_lowercase().contains("32bit");
        let b_is_32 = b.name.to_lowercase().contains("-32") || b.name.to_lowercase().contains("_32") || b.name.to_lowercase().contains("32bit");
        let a_norm: String = a.name.chars().filter(|ch| ch.is_alphanumeric()).flat_map(|ch| ch.to_lowercase()).collect();
        let b_norm: String = b.name.chars().filter(|ch| ch.is_alphanumeric()).flat_map(|ch| ch.to_lowercase()).collect();
        let a_match = !dir_name.is_empty() && (a_norm.starts_with(&dir_name) || dir_name.starts_with(&a_norm));
        let b_match = !dir_name.is_empty() && (b_norm.starts_with(&dir_name) || dir_name.starts_with(&b_norm));

        (b_is_shipping as u8).cmp(&(a_is_shipping as u8))
            .then_with(|| a_decl_pos.cmp(&b_decl_pos))
            .then_with(|| (a_is_32 as u8).cmp(&(b_is_32 as u8)))
            .then_with(|| (b_match as u8).cmp(&(a_match as u8)))
            .then_with(|| a.rel.matches(['/', '\\']).count().cmp(&b.rel.matches(['/', '\\']).count()))
            .then_with(|| b.bitness.cmp(&a.bitness))
            .then_with(|| a.name.cmp(&b.name))
    });
    exes
}

pub fn scan_library_root<P: AsRef<Path>>(root: P) -> Vec<GameEntry> {
    let root = root.as_ref();
    let mut games = Vec::new();
    if !root.exists() || !root.is_dir() {
        return games;
    }

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name_str) = path.file_name().and_then(|n| n.to_str()) {
                    if is_not_a_game_dir(name_str) || is_not_a_game_title(name_str) {
                        continue;
                    }
                    if is_library_container_name(name_str) {
                        games.extend(scan_library_root(&path));
                        continue;
                    }
                    let t_folder = std::time::Instant::now();
                    let holds = holds_game(&path, 3);
                    println!("FOLDER: {} => holds={}, took={:?}", path.display(), holds, t_folder.elapsed());
                    if holds {
                        let t_scan = std::time::Instant::now();
                        if let Some(mut game) = scan_game_directory(&path) {
                            println!("SCANNED GAME: {} => took={:?}", game.name, t_scan.elapsed());
                            game.launcher = "My folders".to_string();
                            games.push(game);
                        }
                    }
                }
            }
        }
    }
    games
}

pub fn dedupe_games(games: Vec<GameEntry>) -> Vec<GameEntry> {
    let mut map: std::collections::HashMap<String, GameEntry> = std::collections::HashMap::new();
    for game in games {
        let key = game.dir.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
        if let Some(existing) = map.get(&key) {
            let existing_is_generic = existing.launcher.starts_with("My folders") || existing.launcher == "Added by hand";
            let new_is_launcher = !game.launcher.starts_with("My folders") && game.launcher != "Added by hand";
            if existing_is_generic && new_is_launcher {
                map.insert(key, game);
            }
        } else {
            map.insert(key, game);
        }
    }
    let mut list: Vec<GameEntry> = map.into_values().collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

pub fn discover_steam() -> Vec<GameEntry> {
    let mut games = Vec::new();
    #[cfg(windows)]
    let steam_path = win32_read_reg_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath", KEY_READ);
    #[cfg(not(windows))]
    let steam_path: Option<String> = None;

    let Some(steam_root) = steam_path else {
        return games;
    };

    let norm_root = crate::core::state::normalize_path_str(&steam_root);
    let mut libraries = vec![PathBuf::from(&steam_root)];
    let mut seen_libs = std::collections::HashSet::new();
    seen_libs.insert(norm_root);

    let vdf_path = PathBuf::from(&steam_root).join("steamapps").join("libraryfolders.vdf");
    if let Ok(vdf_content) = fs::read_to_string(&vdf_path) {
        let re = Regex::new(r#""path"\s+"([^"]+)""#).unwrap();
        for cap in re.captures_iter(&vdf_content) {
            let lib = cap[1].replace(r"\\", r"\");
            let norm_lib = crate::core::state::normalize_path_str(&lib);
            if seen_libs.insert(norm_lib) {
                libraries.push(PathBuf::from(lib));
            }
        }
    }

    let mut seen_dirs = std::collections::HashSet::new();
    for lib in libraries {
        let apps_dir = lib.join("steamapps");
        let Ok(entries) = fs::read_dir(&apps_dir) else { continue; };
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with("appmanifest_") && fname.ends_with(".acf") {
                if let Ok(acf_text) = fs::read_to_string(entry.path()) {
                    let appid = Regex::new(r#""appid"\s+"([^"]+)""#).ok().and_then(|r| r.captures(&acf_text).map(|c| c[1].to_string()));
                    let installdir = Regex::new(r#""installdir"\s+"([^"]+)""#).ok().and_then(|r| r.captures(&acf_text).map(|c| c[1].to_string()));
                    let gname = Regex::new(r#""name"\s+"([^"]+)""#).ok().and_then(|r| r.captures(&acf_text).map(|c| c[1].to_string()));

                    if let (Some(aid), Some(idir)) = (appid, installdir) {
                        if let Some(ref title) = gname {
                            if is_not_a_game_title(title) {
                                continue;
                            }
                        }
                        let game_dir = apps_dir.join("common").join(&idir);
                        let norm = crate::core::state::normalize_game_path(&game_dir);
                        if !seen_dirs.insert(norm) {
                            continue;
                        }
                        if game_dir.exists() {
                            if let Some(mut game) = scan_game_directory(&game_dir) {
                                game.launcher = "Steam".to_string();
                                if let Some(real_name) = gname {
                                    game.name = real_name;
                                }
                                let poster_path = PathBuf::from(&steam_root).join("appcache").join("librarycache").join(&aid).join("library_600x900.jpg");
                                if poster_path.exists() {
                                    game.poster = crate::core::steamart::file_to_art_uri(&poster_path);
                                }
                                games.push(game);
                            }
                        }
                    }
                }
            }
        }
    }
    games
}

pub fn discover_gog() -> Vec<GameEntry> {
    let mut games = Vec::new();
    #[cfg(windows)]
    {
        let subkeys = win32_enum_subkeys(HKEY_LOCAL_MACHINE, r"SOFTWARE\GOG.com\Games", KEY_READ | KEY_WOW64_32KEY);
        let mut seen_paths = std::collections::HashSet::new();
        for game_id in subkeys {
            let subkey_path = format!(r"SOFTWARE\GOG.com\Games\{}", game_id);
            if let Some(path_str) = win32_read_reg_string(HKEY_LOCAL_MACHINE, &subkey_path, "path", KEY_READ | KEY_WOW64_32KEY) {
                let norm = crate::core::state::normalize_path_str(&path_str);
                if !seen_paths.insert(norm) {
                    continue;
                }
                let gdir = PathBuf::from(path_str.trim());
                if gdir.exists() {
                    if let Some(mut game) = scan_game_directory(&gdir) {
                        game.launcher = "GOG".to_string();
                        games.push(game);
                    }
                }
            }
        }
    }
    games
}

pub fn discover_epic() -> Vec<GameEntry> {
    let mut games = Vec::new();
    let manifests = PathBuf::from(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let mut seen_paths = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir(&manifests) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "item").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(loc) = val.get("InstallLocation").and_then(|v| v.as_str()) {
                            let norm = crate::core::state::normalize_path_str(loc);
                            if !seen_paths.insert(norm) {
                                continue;
                            }
                            let gdir = PathBuf::from(loc);
                            if gdir.exists() {
                                if let Some(mut game) = scan_game_directory(&gdir) {
                                    game.launcher = "Epic Games".to_string();
                                    if let Some(dname) = val.get("DisplayName").and_then(|v| v.as_str()) {
                                        game.name = dname.to_string();
                                    }
                                    games.push(game);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    games
}

pub fn discover_xbox() -> Vec<GameEntry> {
    let mut games = Vec::new();
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    {
        // 1. Query HKLM\SOFTWARE\Microsoft\GamingServices\PackageRepository\Root
        let root_subs = win32_enum_subkeys(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\GamingServices\PackageRepository\Root", KEY_READ);
        for sub in root_subs {
            let sub_path = format!(r"SOFTWARE\Microsoft\GamingServices\PackageRepository\Root\{}", sub);
            let child_subs = win32_enum_subkeys(HKEY_LOCAL_MACHINE, &sub_path, KEY_READ);
            for child in child_subs {
                let pkg_key = format!(r"{}\{}", sub_path, child);
                if let Some(raw_root) = win32_read_reg_string(HKEY_LOCAL_MACHINE, &pkg_key, "Root", KEY_READ) {
                    let clean = raw_root.trim_start_matches(r"\\?\").trim_end_matches('\\').trim_end_matches('/');
                    let p = PathBuf::from(clean);
                    if p.exists() && p.is_dir() && !candidate_dirs.contains(&p) {
                        candidate_dirs.push(p);
                    }
                }
            }
        }
    }

    // 2. Scan standard XboxGames folders on all fixed drives
    for drive in get_fixed_drives() {
        let xgames = drive.join("XboxGames");
        if xgames.is_dir() {
            if let Ok(entries) = fs::read_dir(&xgames) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() && !candidate_dirs.contains(&p) {
                        candidate_dirs.push(p);
                    }
                }
            }
        }
    }

    // 3. Scan candidate directories
    for dir in candidate_dirs {
        if let Some(mut game) = scan_game_directory(&dir) {
            game.launcher = "Xbox".to_string();

            let (xbox_name, xbox_poster) = extract_xbox_metadata(&dir);
            if let Some(name) = xbox_name {
                game.name = name;
            }
            if game.poster.is_none() {
                game.poster = xbox_poster;
            }

            games.push(game);
        }
    }

    games
}

pub fn discover_all_launchers() -> Vec<GameEntry> {
    crate::core::logger::info("scan", "Starting discovery across all launchers (Steam, GOG, Epic, Xbox)...");
    let t0 = std::time::Instant::now();
    let mut all = Vec::new();

    let steam_games = discover_steam();
    crate::core::logger::info("scan", &format!("Steam discovery completed: {} games found", steam_games.len()));
    all.extend(steam_games);

    let gog_games = discover_gog();
    crate::core::logger::info("scan", &format!("GOG discovery completed: {} games found", gog_games.len()));
    all.extend(gog_games);

    let epic_games = discover_epic();
    crate::core::logger::info("scan", &format!("Epic Games discovery completed: {} games found", epic_games.len()));
    all.extend(epic_games);

    let xbox_games = discover_xbox();
    crate::core::logger::info("scan", &format!("Xbox Game Pass discovery completed: {} games found", xbox_games.len()));
    all.extend(xbox_games);

    crate::core::logger::info("scan", &format!("All launchers discovery finished in {:?}: {} total games found", t0.elapsed(), all.len()));
    all
}

pub fn get_fixed_drives() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetLogicalDrives() -> u32;
            fn GetDriveTypeW(lpRootPathName: *const u16) -> u32;
        }

        const DRIVE_FIXED: u32 = 3;
        let mut drives = Vec::new();
        let mask = unsafe { GetLogicalDrives() };

        for i in 2..26 { // Start at C (index 2: 'C' - 'A')
            if (mask & (1 << i)) != 0 {
                let letter = (b'A' + i as u8) as char;
                let root_str = format!("{}:\\", letter);
                let wide: Vec<u16> = root_str.encode_utf16().chain(std::iter::once(0)).collect();
                let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
                if drive_type == DRIVE_FIXED {
                    drives.push(PathBuf::from(root_str));
                }
            }
        }
        drives
    }

    #[cfg(not(windows))]
    {
        vec![PathBuf::from("/")]
    }
}

pub fn is_inside<P1: AsRef<Path>, P2: AsRef<Path>>(file: P1, root: P2) -> bool {
    let candidate = file.as_ref().to_string_lossy().to_lowercase().replace('/', "\\");
    let parent = root.as_ref().to_string_lossy().to_lowercase().replace('/', "\\");
    candidate == parent || candidate.starts_with(&format!("{}\\", parent.trim_end_matches('\\')))
}

const SYSTEM_DIRS: &[&str] = &[
    "windows", "winnt", "program files", "program files (x86)", "programdata",
    "users", "$recycle.bin", "system volume information", "recovery", "perflogs",
    "config.msi", "documents and settings", "msocache", "intel", "amd", "nvidia",
    "drivers", "temp", "tmp", "$windows.~bt", "$windows.~ws", "onedrivetemp",
    "inetpub", "node_modules"
];

pub fn discover_drive_roots(excluded_roots: &[String]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let library_name_re = Regex::new(r"(?i)^(games?|my ?games|steamlibrary|gog ?games|epic ?games|xbox ?games|origin ?games|repacks?|emulation)$").unwrap();

    for drive in get_fixed_drives() {
        let Ok(entries) = fs::read_dir(&drive) else { continue; };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let Some(name_str) = path.file_name().and_then(|n| n.to_str()) else { continue; };
            let name_lower = name_str.to_lowercase();
            if SYSTEM_DIRS.contains(&name_lower.as_str()) {
                continue;
            }

            if library_name_re.is_match(&name_lower) {
                if !excluded_roots.iter().any(|ex| is_inside(&path, ex)) {
                    roots.push(path);
                }
                continue;
            }

            if let Ok(kids) = fs::read_dir(&path) {
                let kid_dirs: Vec<PathBuf> = kids.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
                if kid_dirs.len() >= 2 && kid_dirs.len() <= 300 {
                    let gameish = kid_dirs.iter().filter(|k| holds_game(k, 2)).count();
                    if gameish >= 2 && gameish * 2 >= kid_dirs.len() {
                        if !excluded_roots.iter().any(|ex| is_inside(&path, ex)) {
                            roots.push(path);
                        }
                    }
                }
            }
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_version() {
        assert_eq!(short_version("2.4.2.0"), "2.4.2");
        assert_eq!(short_version("310.1.0.0"), "310.1.0");
        assert_eq!(short_version("310.6.0.0"), "310.6.0");
        assert_eq!(short_version("3.7.10"), "3.7.10");
    }

    #[test]
    fn test_scan_synthetic_game_directory() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_scan_synthetic_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin").join("x64");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("CyberGame.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..117].copy_from_slice(b"D3D12CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();

        fs::write(bin_dir.join("D3D12Core.dll"), b"core").unwrap();
        fs::write(bin_dir.join("nvngx_dlss.dll"), b"dlss").unwrap();
        fs::write(bin_dir.join("nvngx_dlssg.dll"), b"framegen").unwrap();

        let g = scan_game_directory(&temp_dir).expect("Synthetic game must be scanned");
        assert_eq!(g.api, "DirectX 12");
        assert!(g.has_frame_generation, "Frame generation must be detected from nvngx_dlssg.dll");
        assert_eq!(g.exe_rel, "bin\\x64\\CyberGame.exe");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_dlss2_without_native_fg_marks_can_inject_fg() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_scan_dlss2_only_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let exe_path = bin_dir.join("bg3.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..119].copy_from_slice(b"vkCreateInstance\x00\x00\x00");
        fs::write(&exe_path, &exe_bytes).unwrap();

        fs::write(bin_dir.join("vulkan-1.dll"), b"vulkan").unwrap();
        fs::write(bin_dir.join("nvngx_dlss.dll"), b"dlss").unwrap();

        let g = scan_game_directory(&temp_dir).expect("Synthetic Vulkan game must be scanned");
        assert_eq!(g.api, "Vulkan");
        assert!(!g.has_frame_generation, "Native frame generation is false");
        assert!(g.can_inject_fg, "can_inject_fg must be true for 64-bit Vulkan game with DLSS 2");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_ignores_optiscaler_subdirectories_for_fg() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_scan_opti_ignore_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bin_dir = temp_dir.join("bin");
        let opti_streamline = bin_dir.join("OptiScaler").join("streamline");
        fs::create_dir_all(&opti_streamline).unwrap();

        let exe_path = bin_dir.join("game.exe");
        let mut exe_bytes = vec![0u8; 10000];
        exe_bytes[100..119].copy_from_slice(b"D3D11CreateDevice\x00\x00");
        fs::write(&exe_path, &exe_bytes).unwrap();

        fs::write(bin_dir.join("d3d11.dll"), b"dx11").unwrap();
        fs::write(bin_dir.join("nvngx_dlss.dll"), b"dlss").unwrap();
        // OptiScaler bundled streamline files
        fs::write(opti_streamline.join("nvngx_dlssg.dll"), b"bundled_dlssg").unwrap();
        fs::write(opti_streamline.join("sl.dlss_g.dll"), b"bundled_sl_dlssg").unwrap();

        let g = scan_game_directory(&temp_dir).expect("Synthetic DX11 game must be scanned");
        assert_eq!(g.api, "DirectX 11");
        assert!(!g.has_frame_generation, "OptiScaler streamline folder must NOT flag has_frame_generation=true");
        assert!(!g.can_inject_fg, "can_inject_fg must be false for 64-bit DX11 game without native DLSS-G");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_bg3_real() {
        let path = Path::new(r"E:\Games\GoG\Baldurs Gate 3\bin\bg3.exe");
        if path.exists() {
            let pe = inspect_pe(path).unwrap();
            let api = detect_api(path, &pe.imports).expect("bg3.exe must detect an API");
            assert_eq!(api, "Vulkan", "bg3.exe must be detected as Vulkan, not DirectX 12");

            let fake_bg3 = GameEntry {
                name: "Baldur's Gate 3".to_string(),
                dir: PathBuf::from(r"E:\Games\GoG\Baldurs Gate 3"),
                exe_path: path.to_path_buf(),
                exe_rel: "bin\\bg3.exe".to_string(),
                bitness: 64,
                api: api.clone(),
                dlss_version: Some("2.4.2".to_string()),
                has_frame_generation: false,
                can_inject_fg: true,
                optiscaler_installed: false,
                optiscaler_presr: false,
                optiscaler_passes: 1,
                reshade_installed: false,
                reshade_version: None,
                reshade_addon_support: false,
                addon_installed: false,
                installed_route: None,
                mfg_unlock_installed: false,
                has_backup: false,
                launcher: "GOG".to_string(),
                poster: None,
                files: Vec::new(),
                available_exes: Vec::new(),
                is_laa: true,
            };
            assert!(!crate::core::install_routes::is_native_dlss_supported(&fake_bg3), "Vulkan game must NOT support Native DLSS (RenoDX)");
            assert!(fake_bg3.can_inject_fg, "BG3 Vulkan must support frame generation injection");
            assert!(crate::core::install_routes::is_frame_generation_supported(&fake_bg3), "BG3 Vulkan must be recognized as frame generation supported");
            let routes = crate::core::install_routes::routes_for(&fake_bg3);
            assert!(!routes.contains(&crate::core::install_routes::InstallRoute::Native), "Routes must NOT contain Native for BG3");
            assert!(routes.contains(&crate::core::install_routes::InstallRoute::Feeder), "Routes must contain Feeder for BG3");

            // Verify bg3_dx11.exe behaves as DirectX 11 and rejects frame generation injection
            let dx11_path = Path::new(r"E:\Games\GoG\Baldurs Gate 3\bin\bg3_dx11.exe");
            if dx11_path.exists() {
                let dx11_pe = inspect_pe(dx11_path).unwrap();
                let dx11_api = detect_api(dx11_path, &dx11_pe.imports).expect("bg3_dx11.exe must detect an API");
                assert_eq!(dx11_api, "DirectX 11", "bg3_dx11.exe must detect as DirectX 11");

                let fake_bg3_dx11 = GameEntry {
                    api: dx11_api,
                    can_inject_fg: false,
                    ..fake_bg3.clone()
                };
                assert!(!fake_bg3_dx11.can_inject_fg, "BG3 DX11 must NOT support frame generation injection");
                assert!(!crate::core::install_routes::is_frame_generation_supported(&fake_bg3_dx11), "BG3 DX11 must reject frame generation");
            }
        }
    }

    #[test]
    fn test_read_done_manifest_synthetic() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_manifest_synthetic_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bdir = temp_dir.join("_DLSS5_Backup");
        fs::create_dir_all(&bdir).unwrap();

        let manifest = crate::core::journal::ActiveManifest {
            version: 1,
            date: "2026-09-11 12:00 UTC".to_string(),
            route: "feeder".to_string(),
            game: Some(crate::core::journal::ManifestGame {
                dir: Some(temp_dir.to_string_lossy().to_string()),
                exe: Some("Content\\game.exe".to_string()),
                api: Some("dxgi".to_string()),
                bitness: Some(64),
                api_label: Some("DirectX 11/12".to_string()),
            }),

            game_exe: Some("Content\\game.exe".to_string()),
            backup_prefix: Some("originals/test-uuid".to_string()),
            replaced: Vec::new(),
            added: vec!["Content\\dxgi.dll".to_string()],
            added_dirs: Vec::new(),
        };

        let bytes = serde_json::to_vec(&manifest).unwrap();
        fs::write(bdir.join("manifest.json.done-1789100000000"), bytes).unwrap();

        let m = crate::core::journal::read_latest_done_manifest(&temp_dir).expect("Done manifest must deserialize");
        assert_eq!(m.route, "feeder");
        assert_eq!(m.backup_prefix.as_deref(), Some("originals/test-uuid"));
        assert!(m.added.contains(&"Content\\dxgi.dll".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_library_scan_synthetic() {
        let temp_root = std::env::temp_dir().join(format!("dlss_lib_synthetic_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let game1_dir = temp_root.join("GameOne");
        let game2_dir = temp_root.join("GameTwo");
        fs::create_dir_all(&game1_dir).unwrap();
        fs::create_dir_all(&game2_dir).unwrap();

        let mut exe1 = vec![0u8; 8000];
        exe1[50..67].copy_from_slice(b"D3D12CreateDevice");
        fs::write(game1_dir.join("Game1.exe"), exe1).unwrap();
        fs::write(game1_dir.join("D3D12Core.dll"), b"d3d12").unwrap();

        let mut exe2 = vec![0u8; 8000];
        exe2[50..67].copy_from_slice(b"D3D11CreateDevice");
        fs::write(game2_dir.join("Game2.exe"), exe2).unwrap();
        fs::write(game2_dir.join("d3d11.dll"), b"d3d11").unwrap();

        let games = scan_library_root(&temp_root);

        assert_eq!(games.len(), 2, "Both synthetic games in library must be detected");

        let _ = fs::remove_dir_all(&temp_root);
    }


    #[test]
    fn test_discover_xbox() {
        let xbox_games = discover_xbox();
        println!("Discovered Xbox games count: {}", xbox_games.len());
        for g in &xbox_games {
            println!(" - Xbox game: {} ({}) => API: {}, DLSS: {:?}, optiscaler: {}, route: {:?}, backup: {}",
                g.name, g.dir.display(), g.api, g.dlss_version, g.optiscaler_installed, g.installed_route, g.has_backup);
        }
        // If Xbox games are installed on the machine, verify they are found
        assert!(xbox_games.iter().all(|g| g.launcher == "Xbox"));
    }

    #[test]
    fn test_discover_game_exes_finds_multiple_binaries() {
        let temp_dir = std::env::temp_dir().join(format!("test_multi_exe_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let bin_dir = temp_dir.join("bin");
        let _ = fs::create_dir_all(&bin_dir);

        let mut dx11_exe = vec![0u8; 8000];
        dx11_exe[50..67].copy_from_slice(b"D3D11CreateDevice");
        fs::write(bin_dir.join("bg3_dx11.exe"), dx11_exe).unwrap();

        fs::write(bin_dir.join("bg3.exe"), b"vulkan executable dummy").unwrap();
        fs::write(bin_dir.join("vulkan-1.dll"), b"vulkan").unwrap();

        let exes = discover_game_exes(&temp_dir);
        assert_eq!(exes.len(), 2, "Must discover both bg3.exe and bg3_dx11.exe");
        assert!(exes.iter().any(|e| e.name == "bg3.exe" && e.api == "Vulkan"));
        assert!(exes.iter().any(|e| e.name == "bg3_dx11.exe" && e.api.contains("DirectX 11")));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_discover_game_exes_filters_helpers_prelauncher_and_tools() {
        let temp_dir = std::env::temp_dir().join(format!("test_cp2077_filter_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let bin_x64 = temp_dir.join("bin").join("x64");
        let crash_dir = bin_x64.join("CrashReporter");
        let tools_dir = temp_dir.join("engine").join("tools");
        fs::create_dir_all(&crash_dir).unwrap();
        fs::create_dir_all(&tools_dir).unwrap();

        // Write root prelauncher, tool archiver, and script compiler
        fs::write(temp_dir.join("REDprelauncher.exe"), b"dummy prelauncher").unwrap();
        fs::write(crash_dir.join("7za.exe"), b"dummy 7za tool").unwrap();
        fs::write(tools_dir.join("scc.exe"), b"dummy scc compiler").unwrap();

        // Write actual game binary with sibling d3d12
        let mut cp_exe = vec![0u8; 8000];
        cp_exe[50..67].copy_from_slice(b"D3D12CreateDevice");
        fs::write(bin_x64.join("Cyberpunk2077.exe"), cp_exe).unwrap();
        fs::write(bin_x64.join("d3d12.dll"), b"d3d12").unwrap();

        let exes = discover_game_exes(&temp_dir);
        assert_eq!(exes.len(), 1, "Must filter REDprelauncher, 7za, and scc, returning ONLY Cyberpunk2077.exe");
        assert_eq!(exes[0].name, "Cyberpunk2077.exe");

        let game = scan_game_directory(&temp_dir).expect("Must scan game directory");
        assert_eq!(game.exe_path.file_name().unwrap(), "Cyberpunk2077.exe");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_discover_game_exes_filters_larilauncher_and_launcher_dirs() {
        let temp_dir = std::env::temp_dir().join(format!("test_bg3_filter_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let bin_dir = temp_dir.join("bin");
        let launcher_dir = temp_dir.join("Launcher");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&launcher_dir).unwrap();

        // Write real executables in bin/
        let mut dx11_exe = vec![0u8; 8000];
        dx11_exe[50..67].copy_from_slice(b"D3D11CreateDevice");
        fs::write(bin_dir.join("bg3_dx11.exe"), dx11_exe).unwrap();
        fs::write(bin_dir.join("bg3.exe"), b"vulkan executable dummy").unwrap();
        fs::write(bin_dir.join("vulkan-1.dll"), b"vulkan").unwrap();

        // Write LariLauncher in Launcher/
        fs::write(launcher_dir.join("LariLauncher.exe"), b"larian launcher dummy").unwrap();

        let exes = discover_game_exes(&temp_dir);
        assert_eq!(exes.len(), 2, "Must discover ONLY bg3.exe and bg3_dx11.exe, rejecting LariLauncher.exe");
        assert!(!exes.iter().any(|e| e.name.to_lowercase().contains("launcher")));
        assert!(exes.iter().any(|e| e.name == "bg3.exe"));
        assert!(exes.iter().any(|e| e.name == "bg3_dx11.exe"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_discover_game_exes_filters_unreal_root_stub_when_shipping_binary_exists() {
        let temp_dir = std::env::temp_dir().join(format!("test_ue_shipping_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let binaries_wingdk = temp_dir.join("Mixtape").join("Binaries").join("WinGDK");
        fs::create_dir_all(&binaries_wingdk).unwrap();

        // Write root dummy wrapper
        fs::write(temp_dir.join("Mixtape.exe"), b"root bootstrap stub").unwrap();

        // Write shipping binary
        let mut shipping_exe = vec![0u8; 8000];
        shipping_exe[50..67].copy_from_slice(b"D3D11CreateDevice");
        fs::write(binaries_wingdk.join("Mixtape-WinGDK-Shipping.exe"), shipping_exe).unwrap();
        fs::write(binaries_wingdk.join("d3d11.dll"), b"d3d11").unwrap();

        let exes = discover_game_exes(&temp_dir);
        assert_eq!(exes.len(), 1, "Must filter root Mixtape.exe stub and return ONLY Mixtape-WinGDK-Shipping.exe");
        assert_eq!(exes[0].name, "Mixtape-WinGDK-Shipping.exe");

        let game = scan_game_directory(&temp_dir).expect("Must scan game directory");
        assert_eq!(game.exe_path.file_name().unwrap(), "Mixtape-WinGDK-Shipping.exe");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_gog_info_playtasks_and_python_filtering() {
        let temp_dir = std::env::temp_dir().join(format!("test_gog_dik_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp_dir).unwrap();

        let lib_x64 = temp_dir.join("lib").join("windows-x86_64");
        let lib_x86 = temp_dir.join("lib").join("windows-i686");
        fs::create_dir_all(&lib_x64).unwrap();
        fs::create_dir_all(&lib_x86).unwrap();

        // Write python runtime helpers in lib
        fs::write(lib_x64.join("python.exe"), b"dummy python interpreter").unwrap();
        fs::write(lib_x64.join("pythonw.exe"), b"dummy pythonw interpreter").unwrap();
        fs::write(lib_x86.join("python.exe"), b"dummy python interpreter 32").unwrap();
        fs::write(lib_x86.join("pythonw.exe"), b"dummy pythonw interpreter 32").unwrap();

        // Write actual root game executables
        fs::write(temp_dir.join("BeingADIK.exe"), b"renpy game binary 64").unwrap();
        fs::write(temp_dir.join("BeingADIK-32.exe"), b"renpy game binary 32").unwrap();

        // Write GOG info manifest
        let gog_json = r#"{
            "gameId": "1181224050",
            "name": "Being a DIK - Season 1",
            "playTasks": [
                {
                    "category": "game",
                    "isPrimary": true,
                    "name": "Being a DIK",
                    "path": "BeingADIK.exe",
                    "type": "FileTask"
                },
                {
                    "category": "game",
                    "name": "Being a DIK 32-bit",
                    "path": "BeingADIK-32.exe",
                    "type": "FileTask"
                }
            ]
        }"#;
        fs::write(temp_dir.join("goggame-1181224050.info"), gog_json).unwrap();

        let game = scan_game_directory(&temp_dir).expect("Game directory must be recognized");
        assert_eq!(game.launcher, "GOG");
        assert_eq!(game.exe_rel, "BeingADIK.exe", "Primary executable must be BeingADIK.exe, not python.exe");
        assert_eq!(game.available_exes.len(), 2, "Available exes must only contain the 2 real game binaries");
        assert_eq!(game.available_exes[0].name, "BeingADIK.exe");
        assert_eq!(game.available_exes[1].name, "BeingADIK-32.exe");
        assert!(!game.available_exes.iter().any(|e| e.name.contains("python")), "Python helpers must be strictly excluded");

        let exes = discover_game_exes(&temp_dir);
        assert_eq!(exes.len(), 2);
        assert_eq!(exes[0].name, "BeingADIK.exe");
        assert_eq!(exes[1].name, "BeingADIK-32.exe");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_dynamic_d3d11_with_opengl32_import_resolves_to_directx11() {
        let temp_dir = std::env::temp_dir().join(format!("test_d3d11_gl_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp_dir).unwrap();
        let exe_path = temp_dir.join("Game.exe");
        let mut exe_bytes = vec![0u8; 4096];
        exe_bytes[100..117].copy_from_slice(b"D3D11CreateDevice");
        fs::write(&exe_path, &exe_bytes).unwrap();

        let imports = vec!["opengl32.dll".to_string(), "kernel32.dll".to_string()];
        let api = detect_api(&exe_path, &imports);
        assert_eq!(api, Some("DirectX 11".to_string()), "Games importing opengl32 but containing D3D11CreateDevice must resolve to DirectX 11");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_is_inside_and_path_helpers() {
        assert!(is_inside("C:\\Games\\Cyberpunk\\bin\\Cyberpunk2077.exe", "C:\\Games\\Cyberpunk"));
        assert!(is_inside("C:/Games/Cyberpunk/bin/Cyberpunk2077.exe", "C:\\Games\\Cyberpunk"));
        assert!(is_inside("C:\\Games\\Cyberpunk", "C:\\Games\\Cyberpunk"));
        assert!(!is_inside("C:\\OtherGames\\Cyberpunk", "C:\\Games\\Cyberpunk"));
    }

    #[test]
    fn test_dedupe_games_logic() {
        let g1 = GameEntry {
            name: "Game 1".to_string(),
            dir: PathBuf::from("C:\\Games\\Game1"),
            launcher: "Steam".to_string(),
            ..Default::default()
        };
        let g2 = GameEntry {
            name: "Game 1 (Duplicate)".to_string(),
            dir: PathBuf::from("c:/games/game1/"),
            launcher: "My folders".to_string(),
            ..Default::default()
        };
        let g3 = GameEntry {
            name: "Game 2".to_string(),
            dir: PathBuf::from("C:\\Games\\Game2"),
            launcher: "Xbox".to_string(),
            ..Default::default()
        };

        let deduped = dedupe_games(vec![g1, g2, g3]);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].name, "Game 1");
        assert_eq!(deduped[1].name, "Game 2");
    }

    #[test]
    fn test_detect_api_all_graphics_backends() {
        let temp = std::env::temp_dir().join(format!("test_api_detect_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let exe = temp.join("Dummy.exe");
        fs::write(&exe, b"dummy binary").unwrap();

        assert_eq!(detect_api(&exe, &["d3d12.dll".to_string()]), Some("DirectX 12".to_string()));
        assert_eq!(detect_api(&exe, &["d3d11.dll".to_string()]), Some("DirectX 11".to_string()));
        assert_eq!(detect_api(&exe, &["d3d10.dll".to_string()]), Some("DirectX 10".to_string()));
        assert_eq!(detect_api(&exe, &["d3d9.dll".to_string()]), Some("DirectX 9".to_string()));
        assert_eq!(detect_api(&exe, &["d3d8.dll".to_string()]), Some("DirectX 8".to_string()));
        assert_eq!(detect_api(&exe, &["vulkan-1.dll".to_string()]), Some("Vulkan".to_string()));
        assert_eq!(detect_api(&exe, &["opengl32.dll".to_string()]), Some("OpenGL".to_string()));
        assert_eq!(detect_api(&exe, &["kernel32.dll".to_string()]), None);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_xbox_gdk_display_name_extraction() {
        let temp = std::env::temp_dir().join(format!("test_xbox_gdk_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();

        // Create dummy executable
        let exe = temp.join("Resonance.exe");
        fs::write(&exe, b"MZ\x90\x00dummy_pe").unwrap();

        // Create MicrosoftGame.config
        let config_content = r#"<?xml version="1.0" encoding="utf-8"?>
<Game configVersion="1">
    <ExecutableList>
        <Executable Name="Resonance.exe" Id="Game" Alias="Resonance.exe"/>
    </ExecutableList>
    <ShellVisuals DefaultDisplayName="Resonance: A Plague Tale Legacy"
                  PublisherDisplayName="Focus Home Interactive SA"
                  Description="Resonance: A Plague Tale Legacy"/>
</Game>
"#;
        fs::write(temp.join("MicrosoftGame.config"), config_content).unwrap();

        let scanned = scan_game_directory(&temp).expect("Game directory should be recognized");
        assert_eq!(scanned.name, "Resonance: A Plague Tale Legacy", "scan_game_directory must extract the real DefaultDisplayName from MicrosoftGame.config");
        assert_eq!(scanned.launcher, "Xbox", "Detected launcher must be Xbox when MicrosoftGame.config is present");

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_detect_api_renpy_opengl_and_angle_dx11() {
        let temp = std::env::temp_dir().join(format!("test_renpy_api_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(temp.join("renpy")).unwrap();
        fs::create_dir_all(temp.join("lib").join("windows-x86_64")).unwrap();
        fs::write(temp.join("lib").join("windows-x86_64").join("librenpython.dll"), b"MZ\x90dummy").unwrap();

        let exe = temp.join("Game.exe");
        fs::write(&exe, b"MZ\x90dummy").unwrap();

        // 1. By default, Ren'Py resolves to OpenGL
        let api_default = detect_api(&exe, &["librenpython.dll".to_string()]);
        assert_eq!(api_default, Some("OpenGL".to_string()));

        // 2. If log.txt indicates angle2, resolves to DirectX 11
        fs::write(temp.join("log.txt"), "Initializing angle2 renderer:\nDirectX 11").unwrap();
        let api_angle = detect_api(&exe, &["librenpython.dll".to_string()]);
        assert_eq!(api_angle, Some("DirectX 11".to_string()));

        // 3. If log.txt indicates gl2, resolves to OpenGL
        fs::write(temp.join("log.txt"), "Initializing gl2 renderer:\nVendor: NVIDIA\nRenderer: RTX 4090").unwrap();
        let api_gl2 = detect_api(&exe, &["librenpython.dll".to_string()]);
        assert_eq!(api_gl2, Some("OpenGL".to_string()));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_get_fixed_drives_non_empty() {
        let drives = get_fixed_drives();
        assert!(!drives.is_empty(), "Should discover at least one fixed drive on Windows");
        assert!(drives.iter().any(|d| d.to_string_lossy().to_uppercase().starts_with("C:")));
    }

    #[test]
    fn test_discover_game_exes_filters_dlss5_feed_host64_and_mod_directories() {
        let temp_dir = std::env::temp_dir().join(format!("test_filter_host64_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let host64_dir = temp_dir.join("host64");
        let installer_dir = temp_dir.join("__installer");
        fs::create_dir_all(&host64_dir).unwrap();
        fs::create_dir_all(&installer_dir).unwrap();

        // Write real game exe
        let mut game_exe = vec![0u8; 8000];
        game_exe[50..67].copy_from_slice(b"D3D11CreateDevice");
        fs::write(temp_dir.join("Dead Space.exe"), game_exe).unwrap();

        // Write non-game executables
        fs::write(host64_dir.join("dlss5-feed-host64.exe"), b"feeder helper binary").unwrap();
        fs::write(temp_dir.join("dlss5-feed-host64.exe"), b"root feeder helper binary").unwrap();
        fs::write(installer_dir.join("Touchup.exe"), b"installer tool").unwrap();

        let exes = discover_game_exes(&temp_dir);
        assert_eq!(exes.len(), 1, "Must only return Dead Space.exe, strictly filtering host64, installer, and dlss5-feed-host64");
        assert_eq!(exes[0].name, "Dead Space.exe");

        let game = scan_game_directory(&temp_dir).expect("Must scan game directory");
        assert_eq!(game.exe_path.file_name().unwrap(), "Dead Space.exe");
        assert_eq!(game.available_exes.len(), 1);
        assert_eq!(game.available_exes[0].name, "Dead Space.exe");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_game_entry_dlss5_patched_and_route_display() {
        let mut entry = GameEntry::default();
        assert!(!entry.is_dlss5_patched());
        assert_eq!(entry.route_display_name(), "Vanilla");

        entry.installed_route = Some("feeder".to_string());
        assert!(entry.is_dlss5_patched());
        assert_eq!(entry.route_display_name(), "Feeder · Neural Rendering");

        entry.installed_route = Some("native".to_string());
        assert!(entry.is_dlss5_patched());
        assert_eq!(entry.route_display_name(), "Native D3D12");

        entry.installed_route = Some("optiscaler".to_string());
        assert!(entry.is_dlss5_patched());
        assert_eq!(entry.route_display_name(), "OptiScaler");

        entry.installed_route = None;
        entry.optiscaler_installed = true;
        assert!(entry.is_dlss5_patched());
        assert_eq!(entry.route_display_name(), "OptiScaler");

        entry.optiscaler_installed = false;
        entry.reshade_installed = true;
        assert!(entry.is_dlss5_patched());
        assert_eq!(entry.route_display_name(), "ReShade");

        entry.reshade_installed = false;
        assert!(!entry.is_dlss5_patched());
        assert_eq!(entry.route_display_name(), "Vanilla");
    }

    #[test]
    fn test_infer_game_name_nested_generic_folder() {
        let p = Path::new(r"C:\Games\Cyberpunk 2077\bin\x64");
        let exe = Path::new(r"C:\Games\Cyberpunk 2077\bin\x64\Cyberpunk2077.exe");
        let name = infer_game_name(p, exe, None);
        assert_eq!(name, "Cyberpunk 2077");

        let p_root = Path::new(r"D:\SteamLibrary\steamapps\common\Baldurs Gate 3");
        let exe_bg3 = Path::new(r"D:\SteamLibrary\steamapps\common\Baldurs Gate 3\bin\bg3.exe");
        let name_bg3 = infer_game_name(p_root, exe_bg3, None);
        assert_eq!(name_bg3, "Baldurs Gate 3");

        let p_generic = Path::new(r"C:\Random\shipping");
        let exe_stub = Path::new(r"C:\Random\shipping\Starfield.exe");
        let name_stem = infer_game_name(p_generic, exe_stub, None);
        assert_eq!(name_stem, "Random");
    }
}


