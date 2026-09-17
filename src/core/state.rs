use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanCacheEntry {
    pub dir: String,
    pub ok: bool,
    pub installable: bool,
    pub api: Option<String>,
    pub bitness: Option<u32>,
    pub dx12: bool,
    pub exe: Option<String>,
    pub reason: Option<String>,
    pub dlss: Option<String>,
    #[serde(rename = "hasDlss")]
    pub has_dlss: bool,
    pub addon: bool,
    pub optiscaler: bool,
    pub reshade: Option<String>,
    #[serde(rename = "scannedAt")]
    pub scanned_at: u64,
    pub rules: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddonFileEntry {
    pub path: String,
    pub name: Option<String>,
    pub tag: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

fn default_addons() -> Vec<String> {
    vec![
        "builtin:renodx".to_string(),
        "builtin:mfgunlock".to_string(),
        "builtin:feeder".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentEntry {
    pub dir: String,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomOverlayTheme {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppState {
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default, rename = "excludedRoots")]
    pub excluded_roots: Vec<String>,
    #[serde(default)]
    pub manual: Vec<String>,
    #[serde(default)]
    pub posters: HashMap<String, String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub scans: HashMap<String, ScanCacheEntry>,
    #[serde(default)]
    pub recents: Vec<RecentEntry>,
    #[serde(default, rename = "apiOverrides")]
    pub api_overrides: HashMap<String, String>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_lang")]
    pub lang: String,
    #[serde(default = "default_true", rename = "groupGamesByStore")]
    pub group_games_by_store: bool,
    #[serde(default, rename = "autoScanDrives")]
    pub auto_scan_drives: bool,
    #[serde(default = "default_true", rename = "rustTheme")]
    pub rust_theme: bool,
    #[serde(default = "default_run_in_background", rename = "runInBackground")]
    pub run_in_background: bool,
    #[serde(default, rename = "cachedGames")]
    pub cached_games: Vec<crate::core::scan::GameEntry>,
    #[serde(default = "default_addons")]
    pub addons: Vec<String>,
    #[serde(default, rename = "addonFiles")]
    pub addon_files: Vec<AddonFileEntry>,
    #[serde(default = "default_false", rename = "overlayEnabled")]
    pub overlay_enabled: bool,
    #[serde(default = "default_overlay_theme", rename = "overlayTheme")]
    pub overlay_theme: String,
    #[serde(default = "default_overlay_hotkey", rename = "overlayHotkey")]
    pub overlay_hotkey: String,
    #[serde(default, rename = "customOverlayThemes")]
    pub custom_overlay_themes: Vec<CustomOverlayTheme>,
    #[serde(default, rename = "customNames")]
    pub custom_names: HashMap<String, String>,
}

pub fn is_portable_executable() -> bool {
    if let Ok(exe) = std::env::current_exe() {
        let name = exe.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        if name.contains("portable") {
            return true;
        }
    }
    false
}

pub fn default_run_in_background() -> bool {
    !is_portable_executable()
}

fn default_theme() -> String { "dark".to_string() }
fn default_lang() -> String { "en".to_string() }
fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_overlay_theme() -> String { "green".to_string() }
fn default_overlay_hotkey() -> String { "F8".to_string() }

impl Default for AppState {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            excluded_roots: Vec::new(),
            manual: Vec::new(),
            posters: HashMap::new(),
            hidden: Vec::new(),
            scans: HashMap::new(),
            recents: Vec::new(),
            api_overrides: HashMap::new(),
            theme: default_theme(),
            lang: default_lang(),
            group_games_by_store: true,
            auto_scan_drives: false,
            rust_theme: true,
            run_in_background: default_run_in_background(),
            cached_games: Vec::new(),
            addons: default_addons(),
            addon_files: Vec::new(),
            overlay_enabled: false,
            overlay_theme: default_overlay_theme(),
            overlay_hotkey: default_overlay_hotkey(),
            custom_overlay_themes: Vec::new(),
            custom_names: HashMap::new(),
        }
    }
}

pub fn normalize_path_str(p: &str) -> String {
    p.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

pub fn normalize_game_path(p: &std::path::Path) -> String {
    normalize_path_str(&p.to_string_lossy())
}

impl AppState {
    pub fn get_custom_name(&self, dir: &std::path::Path) -> Option<&String> {
        let norm = normalize_game_path(dir);
        self.custom_names.get(&norm)
    }

    pub fn set_custom_name(&mut self, dir: &std::path::Path, name: &str) {
        let norm = normalize_game_path(dir);
        let trimmed = name.trim();
        if trimmed.is_empty() {
            self.custom_names.remove(&norm);
        } else {
            self.custom_names.insert(norm, trimmed.to_string());
        }
    }

    pub fn remove_custom_name(&mut self, dir: &std::path::Path) {
        let norm = normalize_game_path(dir);
        self.custom_names.remove(&norm);
    }

    pub fn is_hidden(&self, dir: &std::path::Path) -> bool {
        let norm = normalize_game_path(dir);
        self.hidden.iter().any(|h| normalize_path_str(h) == norm)
    }

    pub fn hide_game(&mut self, dir: &std::path::Path) {
        let norm = normalize_game_path(dir);
        if !self.hidden.iter().any(|h| normalize_path_str(h) == norm) {
            self.hidden.push(dir.to_string_lossy().to_string());
        }
        self.manual.retain(|m| normalize_path_str(m) != norm);
        self.cached_games.retain(|g| normalize_game_path(&g.dir) != norm);
    }

    pub fn unhide_game(&mut self, dir: &std::path::Path) {
        let norm = normalize_game_path(dir);
        self.hidden.retain(|h| normalize_path_str(h) != norm);
    }

    pub fn unhide_all(&mut self) {
        self.hidden.clear();
    }
}

pub fn key_for(dir: &str) -> String {
    let lower = dir.trim().to_lowercase().replace('/', "\\");
    let mut hasher = Sha256::new();
    hasher.update(lower.as_bytes());
    let res = hex::encode(hasher.finalize());
    res[..16].to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageConfig {
    pub data_dir: String,
}

pub fn is_path_protected<P: AsRef<Path>>(path: P) -> bool {
    let path_str = path.as_ref().to_string_lossy().to_lowercase().replace('/', "\\");
    if let Ok(pf) = std::env::var("ProgramFiles") {
        let pf_lower = pf.to_lowercase().replace('/', "\\");
        if path_str.starts_with(&pf_lower) {
            return true;
        }
    } else if path_str.starts_with(r"c:\program files") {
        return true;
    }

    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        let pf86_lower = pf86.to_lowercase().replace('/', "\\");
        if path_str.starts_with(&pf86_lower) {
            return true;
        }
    } else if path_str.starts_with(r"c:\program files (x86)") {
        return true;
    }

    if let Ok(windir) = std::env::var("SystemRoot") {
        let win_lower = windir.to_lowercase().replace('/', "\\");
        if path_str.starts_with(&win_lower) {
            return true;
        }
    } else if path_str.starts_with(r"c:\windows") {
        return true;
    }

    if path_str.contains(r"\windowsapps") {
        return true;
    }

    false
}

pub fn resolve_appdata_dir_internal(
    exe_name: &str,
    exe_dir: Option<&Path>,
    storage_json_content: Option<&str>,
    env_programdata: Option<&str>,
    env_appdata: Option<&str>,
) -> PathBuf {
    // 1. Portable build / executable check
    let lower_name = exe_name.to_lowercase();
    if lower_name.contains("portable") {
        if let Some(appdata) = env_appdata {
            return PathBuf::from(appdata).join("dlss-5-studio");
        } else {
            return PathBuf::from(".").join(".appdata");
        }
    }

    // 2. storage.json in executable directory
    if let Some(content) = storage_json_content {
        if let Ok(cfg) = serde_json::from_str::<StorageConfig>(content) {
            let p = PathBuf::from(cfg.data_dir.trim());
            if !p.as_os_str().is_empty() {
                return p;
            }
        }
    }

    // 3. If installed in an unprotected location (e.g. D:\Games\DLSS 5 Studio or C:\Games\...), default to <exe_dir>\Data
    if let Some(dir) = exe_dir {
        if !is_path_protected(dir) {
            return dir.join("data");
        }
    }

    // 4. If installed in a protected location (Program Files / Windows), default to ProgramData
    if let Some(pd) = env_programdata {
        return PathBuf::from(pd).join("dlss-5-studio");
    }

    // 5. Ultimate fallback
    if let Some(appdata) = env_appdata {
        PathBuf::from(appdata).join("dlss-5-studio")
    } else {
        PathBuf::from(".").join(".appdata")
    }
}

pub fn get_appdata_dir() -> PathBuf {
    let curr_exe = std::env::current_exe().ok();
    let exe_name = curr_exe
        .as_ref()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default();
    let exe_dir = curr_exe.as_ref().and_then(|p| p.parent());

    let storage_json = exe_dir.and_then(|d| fs::read_to_string(d.join("storage.json")).ok());
    let prog_data = std::env::var("ProgramData").ok();
    let app_data = std::env::var("APPDATA").ok();

    let resolved = resolve_appdata_dir_internal(
        &exe_name,
        exe_dir,
        storage_json.as_deref(),
        prog_data.as_deref(),
        app_data.as_deref(),
    );

    let _ = fs::create_dir_all(&resolved);
    resolved
}

pub fn get_state_path() -> PathBuf {
    get_appdata_dir().join("library.json")
}

pub fn load_state() -> AppState {
    let path = get_state_path();
    if !path.exists() {
        // First run: initialize default state without altering system startup registry
        let default_st = AppState::default();
        let _ = save_state(&default_st);
        return default_st;
    }
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(state) = serde_json::from_str::<AppState>(&content) {
            return state;
        }
    }
    AppState::default()
}

pub fn save_state(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn touch(dir: &str) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    let mut state = load_state();
    state.recents.retain(|r| r.dir.to_lowercase() != dir.to_lowercase());
    state.recents.insert(0, RecentEntry { dir: dir.to_string(), at: now });
    if state.recents.len() > 12 {
        state.recents.truncate(12);
    }
    let _ = save_state(&state);
}

pub fn ago(ts_ms: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    let diff_s = if now_ms > ts_ms { (now_ms - ts_ms) / 1000 } else { 0 };
    if diff_s < 60 {
        "just now".to_string()
    } else if diff_s < 3600 {
        format!("{}m ago", diff_s / 60)
    } else if diff_s < 86400 {
        format!("{}h ago", diff_s / 3600)
    } else if diff_s < 172800 {
        "yesterday".to_string()
    } else {
        format!("{}d ago", diff_s / 86400)
    }
}

use std::sync::Mutex;
static SESSION_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
pub static STATE_TEST_MUTEX: Mutex<()> = Mutex::new(());

pub fn log_message(msg: &str) {
    let disk_msg = crate::core::i18n::format_log_entry("en", msg);
    crate::core::logger::info("ui", &disk_msg);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sec_in_day = now % 86400;
    let hours = sec_in_day / 3600;
    let minutes = (sec_in_day % 3600) / 60;
    let seconds = sec_in_day % 60;
    let line = format!("[{:02}:{:02}:{:02}] {}", hours, minutes, seconds, msg);
    if let Ok(mut lock) = SESSION_LOG.lock() {
        lock.push(line);
        if lock.len() > 120 {
            lock.remove(0);
        }
    }
}

pub fn get_session_log() -> Vec<String> {
    SESSION_LOG.lock().map(|l| l.clone()).unwrap_or_default()
}


pub fn is_addon_active(state: &AppState, id: &str) -> bool {
    // Base add-ons are mandatory core engine components and always active
    if id == "builtin:renodx" || id == "builtin:mfgunlock" || id == "builtin:feeder" {
        return true;
    }
    // Overlay is shelved
    if id == "builtin:overlay" {
        return false;
    }
    state.addons.iter().any(|a| a == id)
}

pub fn toggle_addon_in_state(state: &mut AppState, id: &str, active: bool) {
    // Base add-ons are mandatory and cannot be deactivated
    if id == "builtin:renodx" || id == "builtin:mfgunlock" || id == "builtin:feeder" {
        if !state.addons.iter().any(|a| a == id) {
            state.addons.push(id.to_string());
        }
        return;
    }
    // Overlay cannot be enabled
    if id == "builtin:overlay" {
        return;
    }
    if active {
        if !state.addons.iter().any(|a| a == id) {
            state.addons.push(id.to_string());
        }
    } else {
        state.addons.retain(|a| a != id);
    }
}

pub fn add_custom_addon(state: &mut AppState, entry: AddonFileEntry) {
    state.addon_files.retain(|e| e.path != entry.path);
    if !state.addons.contains(&entry.path) {
        state.addons.push(entry.path.clone());
    }
    state.addon_files.push(entry);
}

pub fn remove_custom_addon(state: &mut AppState, path: &str) {
    state.addon_files.retain(|e| e.path != path);
    state.addons.retain(|a| a != path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_theme_default_and_serde() {
        let default_state = AppState::default();
        assert!(default_state.rust_theme, "Rust theme should be enabled by default");

        let json = serde_json::to_string(&default_state).unwrap();
        assert!(json.contains(r#""rustTheme":true"#));

        // Test missing key defaults to true
        let partial_json = r#"{"folders":[],"theme":"dark"}"#;
        let parsed: AppState = serde_json::from_str(partial_json).unwrap();
        assert!(parsed.rust_theme, "Missing rustTheme in JSON must default to true");

        // Test explicit false is preserved
        let false_json = r#"{"folders":[],"rustTheme":false}"#;
        let parsed_false: AppState = serde_json::from_str(false_json).unwrap();
        assert!(!parsed_false.rust_theme, "Explicit false rustTheme must be preserved");
    }

    #[test]
    fn test_run_in_background_serde() {
        let default_state = AppState::default();
        assert!(default_state.run_in_background, "Run in background should default to true");

        let json = serde_json::to_string(&default_state).unwrap();
        assert!(json.contains(r#""runInBackground":true"#));

        let partial = r#"{"folders":[],"theme":"dark"}"#;
        let parsed: AppState = serde_json::from_str(partial).unwrap();
        assert!(parsed.run_in_background, "Missing runInBackground must default to true");

        let false_json = r#"{"runInBackground":false}"#;
        let parsed_false: AppState = serde_json::from_str(false_json).unwrap();
        assert!(!parsed_false.run_in_background, "Explicit false runInBackground must be preserved");
    }

    #[test]
    fn test_hidden_games_persistence_and_normalization() {
        let mut state = AppState::default();
        let game_dir = std::path::PathBuf::from(r"E:\GOG Games\Being a DIK");
        let game = crate::core::scan::GameEntry {
            name: "Being a DIK".to_string(),
            dir: game_dir.clone(),
            exe_path: game_dir.join("Being a DIK.exe"),
            exe_rel: "Being a DIK.exe".to_string(),
            bitness: 64,
            api: "DirectX 11".to_string(),
            dlss_version: None,
            has_frame_generation: false,
            can_inject_fg: false,
            optiscaler_installed: false,
            optiscaler_presr: false,
            optiscaler_passes: 1,
            mfg_unlock_installed: false,
            has_backup: false,
            launcher: "GOG".to_string(),
            poster: None,
            reshade_installed: false,
            reshade_version: None,
            reshade_addon_support: false,
            addon_installed: false,
            installed_route: None,
            files: Vec::new(),
            available_exes: Vec::new(),
            is_laa: true,
            nr_style: 0,
        };
        state.cached_games.push(game.clone());
        assert!(!state.is_hidden(&game_dir));

        state.hide_game(&game_dir);
        assert!(state.is_hidden(&game_dir));
        // Test case and slash insensitivity
        assert!(state.is_hidden(std::path::Path::new("e:/gog games/being a dik/")));
        // Cached games should no longer contain it
        assert_eq!(state.cached_games.len(), 0);

        // Unhide
        state.unhide_game(&game_dir);
        assert!(!state.is_hidden(&game_dir));
    }

    #[test]
    fn test_base_addons_mandatory_and_custom_addons() {
        let mut state = AppState::default();
        // Base add-ons are always active
        assert!(is_addon_active(&state, "builtin:renodx"));
        assert!(is_addon_active(&state, "builtin:mfgunlock"));
        assert!(is_addon_active(&state, "builtin:feeder"));
        assert!(!is_addon_active(&state, "builtin:overlay"), "Overlay must be inactive");

        // Attempting to toggle off a base add-on is ignored
        toggle_addon_in_state(&mut state, "builtin:renodx", false);
        assert!(is_addon_active(&state, "builtin:renodx"), "Base add-ons cannot be deactivated");

        // Custom add-ons can be toggled
        let custom_id = "C:\\Mods\\reshade\\my_addon.addon64";
        assert!(!is_addon_active(&state, custom_id));
        toggle_addon_in_state(&mut state, custom_id, true);
        assert!(is_addon_active(&state, custom_id));
        toggle_addon_in_state(&mut state, custom_id, false);
        assert!(!is_addon_active(&state, custom_id));
    }

    #[test]
    fn test_custom_names_persistence() {
        let mut state = AppState::default();
        let game_dir = std::path::Path::new(r"E:\Games\CustomGame");

        // Initially none
        assert_eq!(state.get_custom_name(game_dir), None);

        // Set custom name
        state.set_custom_name(game_dir, "My Custom Game Title");
        assert_eq!(state.get_custom_name(game_dir), Some(&"My Custom Game Title".to_string()));

        // Check path normalization (forward slashes and trailing slash)
        let alt_dir = std::path::Path::new("e:/games/customgame/");
        assert_eq!(state.get_custom_name(alt_dir), Some(&"My Custom Game Title".to_string()));

        // Test serialization and deserialization
        let json = serde_json::to_string(&state).expect("serialize");
        let loaded: AppState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(loaded.get_custom_name(game_dir), Some(&"My Custom Game Title".to_string()));

        // Empty string resets / removes
        state.set_custom_name(game_dir, "   ");
        assert_eq!(state.get_custom_name(game_dir), None);

        // Remove method
        state.set_custom_name(game_dir, "Temporary");
        state.remove_custom_name(game_dir);
        assert_eq!(state.get_custom_name(game_dir), None);
    }

    #[test]
    fn test_fresh_state_defaults() {
        let state = AppState::default();
        assert!(state.run_in_background);
        assert!(state.rust_theme);
        assert!(!state.auto_scan_drives);
        assert!(state.cached_games.is_empty());
    }

    #[test]
    fn test_is_path_protected_detection() {
        assert!(super::is_path_protected(r"C:\Program Files\DLSS 5 Studio"));
        assert!(super::is_path_protected(r"C:\Program Files (x86)\DLSS 5 Studio"));
        assert!(super::is_path_protected("C:/Program Files/DLSS 5 Studio"));
        assert!(super::is_path_protected(r"C:\Windows\System32"));
        assert!(super::is_path_protected(r"C:\Program Files\WindowsApps\Game"));

        // Unprotected directories
        assert!(!super::is_path_protected(r"D:\Games\DLSS 5 Studio"));
        assert!(!super::is_path_protected(r"C:\Games\DLSS 5 Studio"));
        assert!(!super::is_path_protected(r"E:\Tools\ModManager"));
    }

    #[test]
    fn test_resolve_appdata_dir_portable_priority() {
        let res = super::resolve_appdata_dir_internal(
            "dlss-studio-portable.exe",
            Some(std::path::Path::new(r"D:\Games\DLSS 5 Studio")),
            Some(r#"{"data_dir": "E:\\CustomStorage"}"#),
            Some(r"C:\ProgramData"),
            Some(r"C:\Users\Tester\AppData\Roaming"),
        );
        assert_eq!(res, std::path::PathBuf::from(r"C:\Users\Tester\AppData\Roaming\dlss-5-studio"));
    }

    #[test]
    fn test_resolve_appdata_dir_storage_json_override() {
        let res = super::resolve_appdata_dir_internal(
            "dlss-studio.exe",
            Some(std::path::Path::new(r"C:\Program Files\DLSS 5 Studio")),
            Some(r#"{"data_dir": "D:\\MyCustomStorage"}"#),
            Some(r"C:\ProgramData"),
            Some(r"C:\Users\Tester\AppData\Roaming"),
        );
        assert_eq!(res, std::path::PathBuf::from(r"D:\MyCustomStorage"));
    }

    #[test]
    fn test_resolve_appdata_dir_unprotected_install_defaults_to_data() {
        let res = super::resolve_appdata_dir_internal(
            "dlss-studio.exe",
            Some(std::path::Path::new(r"D:\Games\DLSS 5 Studio")),
            None,
            Some(r"C:\ProgramData"),
            Some(r"C:\Users\Tester\AppData\Roaming"),
        );
        assert_eq!(res, std::path::PathBuf::from(r"D:\Games\DLSS 5 Studio\data"));
    }

    #[test]
    fn test_resolve_appdata_dir_protected_install_defaults_to_programdata() {
        let res = super::resolve_appdata_dir_internal(
            "dlss-studio.exe",
            Some(std::path::Path::new(r"C:\Program Files\DLSS 5 Studio")),
            None,
            Some(r"C:\ProgramData"),
            Some(r"C:\Users\Tester\AppData\Roaming"),
        );
        assert_eq!(res, std::path::PathBuf::from(r"C:\ProgramData\dlss-5-studio"));
    }
}
