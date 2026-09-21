use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ManifestItem {
    pub rel: String,
    #[serde(default)]
    pub old_hash: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ManifestGame {
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub api: Option<String>,
    #[serde(default)]
    pub bitness: Option<u32>,
    #[serde(default)]
    pub api_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveManifest {
    #[serde(default = "default_manifest_version")]
    pub version: u32,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub game: Option<ManifestGame>,
    #[serde(default)]
    pub game_exe: Option<String>,
    #[serde(default)]
    pub backup_prefix: Option<String>,
    #[serde(default)]
    pub replaced: Vec<ManifestItem>,
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub added_dirs: Vec<String>,
    #[serde(default)]
    pub mfg_unlock: Option<bool>,
    #[serde(default)]
    pub mfg_multiplier: Option<u32>,
    #[serde(default)]
    pub nr_style_enabled: Option<bool>,
    #[serde(default)]
    pub nr_style: Option<usize>,
    #[serde(default)]
    pub opti_presr: Option<bool>,
    #[serde(default)]
    pub opti_passes: Option<u32>,
}

fn default_manifest_version() -> u32 { 1 }

impl Default for ActiveManifest {
    fn default() -> Self {
        Self {
            version: 1,
            date: format!("{:?}", std::time::SystemTime::now()),
            route: "optiscaler".to_string(),
            game: None,
            game_exe: Some(String::new()),
            backup_prefix: None,
            replaced: Vec::new(),
            added: Vec::new(),
            added_dirs: Vec::new(),
            mfg_unlock: None,
            mfg_multiplier: None,
            nr_style_enabled: None,
            nr_style: None,
            opti_presr: None,
            opti_passes: None,
        }
    }
}

fn strip_verbatim(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

pub fn backup_dir(game_dir: &Path) -> PathBuf {
    // 1. Direct subfolder
    let direct = game_dir.join("_DLSS5_Backup");
    if direct.exists() {
        return direct;
    }

    // 2. Canonicalized path (resolving NTFS Junctions / symlinks like Xbox WindowsApps -> Games\...\Content)
    if let Ok(canon) = fs::canonicalize(game_dir) {
        let norm = strip_verbatim(&canon);
        let c_direct = norm.join("_DLSS5_Backup");
        if c_direct.exists() {
            return c_direct;
        }
        if let Some(parent) = norm.parent() {
            let p_backup = parent.join("_DLSS5_Backup");
            if p_backup.exists() {
                return p_backup;
            }
        }
    }

    // 3. Parent of game_dir if game_dir is nested e.g. in "Content"
    if let Some(parent) = game_dir.parent() {
        let p_backup = parent.join("_DLSS5_Backup");
        if p_backup.exists() {
            return p_backup;
        }
    }

    direct
}

pub fn has_backup_available(game_dir: &Path) -> bool {
    let bdir = backup_dir(game_dir);
    bdir.join("manifest.json").exists() || bdir.join("pending-switch.json").exists()
}

pub fn read_manifest(game_dir: &Path) -> Option<ActiveManifest> {
    let bdir = backup_dir(game_dir);
    let manifest_file = bdir.join("manifest.json");
    if let Ok(bytes) = fs::read(&manifest_file) {
        if let Ok(m) = serde_json::from_slice::<ActiveManifest>(&bytes) {
            return Some(m);
        }
    }
    None
}

pub fn read_latest_done_manifest(game_dir: &Path) -> Option<ActiveManifest> {
    let bdir = backup_dir(game_dir);
    if let Ok(entries) = fs::read_dir(&bdir) {
        let mut done_files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("manifest.json.done-"))
                    .unwrap_or(false)
            })
            .collect();

        // Sort so latest timestamp is first
        done_files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        for p in done_files {
            if let Ok(bytes) = fs::read(&p) {
                if let Ok(m) = serde_json::from_slice::<ActiveManifest>(&bytes) {
                    return Some(m);
                }
            }
        }
    }

    None
}


pub fn prune_old_manifests(game_dir: &Path, max_keep: usize) -> usize {
    let bdir = backup_dir(game_dir);
    if !bdir.exists() {
        return 0;
    }

    let entries = match fs::read_dir(&bdir) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let mut done_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("manifest.json.done-"))
                .unwrap_or(false)
        })
        .collect();

    // Sort descending by file name (which includes epoch timestamp)
    done_files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    // Collect all referenced backup_prefix strings from retained / active manifests
    let mut referenced_prefixes = std::collections::HashSet::new();

    // 1. Active manifest
    if let Some(active) = read_manifest(game_dir) {
        if let Some(prefix) = active.backup_prefix {
            referenced_prefixes.insert(prefix);
        }
    }

    // 2. Pending switch manifest (if any)
    let pending_path = bdir.join("pending-switch.json");
    if let Ok(bytes) = fs::read(&pending_path) {
        if let Ok(pending) = serde_json::from_slice::<ActiveManifest>(&bytes) {
            if let Some(prefix) = pending.backup_prefix {
                referenced_prefixes.insert(prefix);
            }
        }
    }

    // 3. Top `max_keep` retained done manifests
    let retained_count = done_files.len().min(max_keep);
    for p in &done_files[..retained_count] {
        if let Ok(bytes) = fs::read(p) {
            if let Ok(m) = serde_json::from_slice::<ActiveManifest>(&bytes) {
                if let Some(prefix) = m.backup_prefix {
                    referenced_prefixes.insert(prefix);
                }
            }
        }
    }

    let mut pruned_count = 0;

    // Prune excess done manifests
    if done_files.len() > max_keep {
        for p in &done_files[max_keep..] {
            let prefix_opt = if let Ok(bytes) = fs::read(p) {
                serde_json::from_slice::<ActiveManifest>(&bytes)
                    .ok()
                    .and_then(|m| m.backup_prefix)
            } else {
                None
            };

            if fs::remove_file(p).is_ok() {
                pruned_count += 1;
                crate::core::logger::debug("journal", &format!("Pruned old archived manifest: {}", p.display()));

                // If this manifest had a backup_prefix that is NOT referenced anywhere else, clean it up
                if let Some(prefix) = prefix_opt {
                    if !referenced_prefixes.contains(&prefix) {
                        let prefix_path = bdir.join(&prefix);
                        if prefix_path.is_dir() {
                            let _ = fs::remove_dir_all(&prefix_path);
                            crate::core::logger::info("journal", &format!("Cleaned orphaned backup directory: {}", prefix_path.display()));
                        }
                    }
                }
            }
        }
    }

    // Also check the "originals" folder directly for any orphaned subdirectories
    let originals_dir = bdir.join("originals");
    if originals_dir.is_dir() {
        if let Ok(orig_entries) = fs::read_dir(&originals_dir) {
            for entry in orig_entries.flatten() {
                let sub_path = entry.path();
                if sub_path.is_dir() {
                    let rel_prefix = format!("originals/{}", entry.file_name().to_string_lossy());
                    if !referenced_prefixes.contains(&rel_prefix) {
                        let _ = fs::remove_dir_all(&sub_path);
                        crate::core::logger::info("journal", &format!("Cleaned unreferenced originals directory: {}", sub_path.display()));
                    }
                }
            }
        }
    }

    if pruned_count > 0 {
        crate::core::logger::info("journal", &format!("Pruned {} old archived manifests (retained top {}) for {}", pruned_count, max_keep, game_dir.display()));
    }

    pruned_count
}

pub fn save_manifest(game_dir: &Path, manifest: &ActiveManifest) -> std::io::Result<()> {
    crate::core::logger::info("journal", &format!("Saving active manifest for {}: route={}, replaced={}, added={}", game_dir.display(), manifest.route, manifest.replaced.len(), manifest.added.len()));
    let bdir = backup_dir(game_dir);
    fs::create_dir_all(&bdir)?;
    let bytes = serde_json::to_vec_pretty(manifest)?;
    fs::write(bdir.join("manifest.json"), bytes)?;
    let _ = prune_old_manifests(game_dir, 5);
    Ok(())
}

pub fn resolve_target_path(game_dir: &Path, rel: &str) -> PathBuf {
    let direct = game_dir.join(rel);
    if direct.exists() {
        return direct;
    }
    // Check stripped Content prefix if game_dir is already Content or a junction to Content
    if let Ok(stripped) = Path::new(rel).strip_prefix("Content") {
        let sub = game_dir.join(stripped);
        if sub.exists() {
            return sub;
        }
    }
    if let Ok(stripped) = Path::new(rel).strip_prefix("content") {
        let sub = game_dir.join(stripped);
        if sub.exists() {
            return sub;
        }
    }
    // Check game_dir.join("Content").join(rel)
    let c_sub = game_dir.join("Content").join(rel);
    if c_sub.exists() {
        return c_sub;
    }

    if let Ok(canon) = fs::canonicalize(game_dir) {
        let norm = strip_verbatim(&canon);
        let c_direct = norm.join(rel);
        if c_direct.exists() {
            return c_direct;
        }
        if let Some(parent) = norm.parent() {
            let p_direct = parent.join(rel);
            if p_direct.exists() {
                return p_direct;
            }
        }
    }

    if let Ok(stripped) = Path::new(rel).strip_prefix("Content") {
        game_dir.join(stripped)
    } else {
        direct
    }
}

pub fn is_proxy_hook(path: &Path) -> bool {
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    let hook_names = ["dxgi.dll", "winmm.dll", "d3d12.dll", "d3d11.dll", "d3d9.dll", "d3d8.dll", "opengl32.dll", "dinput8.dll", "version.dll"];
    if hook_names.contains(&fname.as_str()) {
        return crate::core::pe::is_optiscaler_or_proxy(path) || crate::core::pe::is_reshade_dll(path).0;
    }
    false
}

pub fn clean_untracked_mods(game_dir: &Path) -> std::io::Result<Vec<String>> {
    clean_untracked_mods_with_exe(game_dir, None)
}

pub fn clean_untracked_mods_with_exe(game_dir: &Path, exe_path: Option<&Path>) -> std::io::Result<Vec<String>> {
    let mut removed = Vec::new();
    let mut target_dirs = vec![game_dir.to_path_buf()];

    let detected_exe = if exe_path.is_none() {
        crate::core::scan::scan_game_directory(game_dir).map(|g| g.exe_path)
    } else {
        None
    };
    let effective_exe = exe_path.or(detected_exe.as_deref());

    if let Some(ep) = effective_exe {
        if let Some(parent) = ep.parent() {
            target_dirs.push(parent.to_path_buf());
        }
    }
    if let Some(mod_root) = crate::core::compatibility::managed_mod_root(game_dir, effective_exe) {
        target_dirs.push(mod_root);
    }

    if let Err(e) = crate::core::install_guards::assert_game_closed(game_dir, effective_exe) {
        crate::core::logger::warn("journal", &format!("Cannot clean untracked mods while game is running: {}", e));
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, e));
    }

    if let Ok(canon) = fs::canonicalize(game_dir) {
        let norm = strip_verbatim(&canon);
        if norm != game_dir {
            target_dirs.push(norm.clone());
        }
        if let Some(parent) = norm.parent() {
            let fname = norm.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if fname.eq_ignore_ascii_case("content") || fname.eq_ignore_ascii_case("win64") || fname.eq_ignore_ascii_case("binaries") {
                target_dirs.push(parent.to_path_buf());
            }
        }
    }
    if game_dir.join("Content").is_dir() {
        target_dirs.push(game_dir.join("Content"));
    }

    // Recursively discover subdirectories up to depth 4 to catch nested executable folders (e.g. bin\x64, Binaries\Win64)
    for entry in walkdir::WalkDir::new(game_dir).max_depth(4).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_dir() {
            let fname = entry.file_name().to_string_lossy();
            if !fname.starts_with('.') && fname != "_DLSS5_Backup" && fname != "node_modules" {
                target_dirs.push(entry.path().to_path_buf());
            }
        }
    }

    // Deduplicate target dirs
    let mut unique_dirs = Vec::new();
    for d in target_dirs {
        if !unique_dirs.contains(&d) && d.is_dir() {
            unique_dirs.push(d);
        }
    }

    // Also purge files listed in any active or recent manifest
    if let Some(manifest) = read_manifest(game_dir).or_else(|| read_latest_done_manifest(game_dir)) {
        for rel in &manifest.added {
            let target_file = resolve_target_path(game_dir, rel);
            if target_file.exists() && fs::remove_file(&target_file).is_ok() {
                removed.push(rel.clone());
            }
        }
        for rel in manifest.added_dirs.iter().rev() {
            let target_dir = resolve_target_path(game_dir, rel);
            if target_dir.exists() && fs::remove_dir_all(&target_dir).is_ok() {
                removed.push(format!("{}/", rel));
            }
        }
    }

    for dir in unique_dirs {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let fname = entry.file_name().to_string_lossy().to_string();
                let lower = fname.to_lowercase();

                if lower == "optiscaler.ini"
                    || lower == "optiscaler.log"
                    || lower == "optiscaler.dll"
                    || lower == "reshade.ini"
                    || lower == "reshade.log"
                    || lower == "reshade64.dll"
                    || lower == "reshade32.dll"
                    || lower == "reshade64.json"
                    || lower == "reshadegui.ini"
                    || lower == "reshadepreset.ini"
                    || lower == "dlss5-feed.cfg"
                    || lower == "dlss5-feed.log"
                    || lower == "dlss5-feed.addon64"
                    || lower == "dlss5-feed.addon32"
                    || lower == "dlss5-feed-host64.exe"
                    || lower == "dlss5-lab-overlay.addon64"
                    || lower == "renodx-dlss5.addon64"
                    || lower == "renodx-mfgunlock.addon64"
                    || lower == "dgvoodoo.conf"
                    || lower == "dgvoodoo.log"
                    || lower == "nvngx.dll_dlssnr.dll"
                    || lower == "nvngx_dlssnr.dll"
                    || lower == "rtxmfg-universal.json"
                    || lower.starts_with("rtxmfg-")
                    || lower.ends_with(".addon64")
                    || lower.ends_with(".addon32")
                    || lower.ends_with(".addon")
                {
                    match fs::remove_file(&path) {
                        Ok(_) => {
                            removed.push(fname.clone());
                        }
                        Err(e) => {
                            crate::core::logger::warn("journal", &format!("Failed to remove mod file {}: {}", path.display(), e));
                            return Err(std::io::Error::new(e.kind(), format!("Failed to remove {}: {}. Is the game or launcher still running?", fname, e)));
                        }
                    }
                } else if (lower == "optiscaler" || lower == "host64") && path.is_dir() {
                    if let Err(e) = fs::remove_dir_all(&path) {
                        return Err(std::io::Error::new(e.kind(), format!("Failed to remove {} directory: {}. Is the game running?", fname, e)));
                    }
                    removed.push(format!("{}/", fname));
                } else if lower == "reshade-shaders" && path.is_dir() {
                    if let Err(e) = fs::remove_dir_all(&path) {
                        return Err(std::io::Error::new(e.kind(), format!("Failed to remove reshade-shaders directory: {}. Is the game running?", e)));
                    }
                    removed.push("reshade-shaders/".to_string());
                } else if lower == "dxgi.dll" || lower == "winmm.dll" || lower == "d3d12.dll" || lower == "d3d11.dll" || lower == "d3d9.dll" || lower == "d3d8.dll" || lower == "dinput8.dll" || lower == "version.dll" {
                    if is_proxy_hook(&path) {
                        match fs::remove_file(&path) {
                            Ok(_) => {
                                removed.push(fname.clone());
                            }
                            Err(e) => {
                                crate::core::logger::warn("journal", &format!("Failed to remove proxy hook {}: {}", path.display(), e));
                                return Err(std::io::Error::new(e.kind(), format!("Failed to remove {}: {}. Is the game or launcher still running?", fname, e)));
                            }
                        }
                    }
                }
            }
        }
    }

    // If active manifest exists, archive it so it doesn't leave active state
    let bdir = backup_dir(game_dir);
    let manifest_path = bdir.join("manifest.json");
    if manifest_path.exists() {
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let archive_name = format!("manifest.json.done-{}", ts);
        let _ = fs::rename(&manifest_path, bdir.join(&archive_name));
        let _ = prune_old_manifests(game_dir, 5);
    }

    if !removed.is_empty() {
        crate::core::logger::info("journal", &format!("Cleaned untracked mod files for {}: {:?}", game_dir.display(), removed));
    }

    Ok(removed)
}

pub fn restore_game(game_dir: &Path) -> std::io::Result<bool> {
    crate::core::logger::info("restore", &format!("Initiating restore for game: {}", game_dir.display()));
    let manifest = match read_manifest(game_dir) {
        Some(m) => m,
        None => {
            // Fallback: clean untracked mods if no manifest is found
            let cleaned = clean_untracked_mods(game_dir)?;
            crate::core::logger::info("restore", &format!("No active manifest found for {}; cleaned untracked mods: {:?}", game_dir.display(), cleaned));
            return Ok(!cleaned.is_empty());
        }
    };

    let exe_opt = manifest.game_exe.as_ref()
        .or_else(|| manifest.game.as_ref().and_then(|g| g.exe.as_ref()))
        .map(|e| game_dir.join(e));

    if let Err(e) = crate::core::install_guards::assert_game_closed(game_dir, exe_opt.as_deref()) {
        crate::core::logger::warn("restore", &format!("Cannot restore while game is running: {}", e));
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, e));
    }

    let bdir = backup_dir(game_dir);

    for item in &manifest.replaced {
        let backup_file = if let Some(ref p) = manifest.backup_prefix {
            bdir.join(p).join(&item.rel)
        } else {
            bdir.join(&item.rel)
        };
        let target_file = resolve_target_path(game_dir, &item.rel);
        if backup_file.exists() {
            // Safety check: if the backed-up file was actually a proxy hook or mod file
            // that was mistakenly copied to backup during a prior dirty install, NEVER restore it!
            if is_proxy_hook(&backup_file) {
                crate::core::logger::warn("restore", &format!("Skipping corrupted backup file (is proxy hook): {}", backup_file.display()));
                if target_file.exists() {
                    let _ = fs::remove_file(&target_file);
                }
                continue;
            }

            if let Some(parent) = target_file.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&backup_file, &target_file)?;
            crate::core::logger::debug("restore", &format!("Restored original file: {}", target_file.display()));
        }
    }

    for rel in &manifest.added {
        let target_file = resolve_target_path(game_dir, rel);
        if target_file.exists() {
            let _ = fs::remove_file(&target_file);
            crate::core::logger::debug("restore", &format!("Removed mod file: {}", target_file.display()));
        }
    }

    for rel in manifest.added_dirs.iter().rev() {
        let target_dir = resolve_target_path(game_dir, rel);
        if target_dir.exists() {
            let _ = fs::remove_dir_all(&target_dir);
            crate::core::logger::debug("restore", &format!("Removed mod directory: {}", target_dir.display()));
        }
    }

    // Also purge known leftover injected mod files
    let exe_opt = manifest.game_exe.as_ref()
        .or_else(|| manifest.game.as_ref().and_then(|g| g.exe.as_ref()))
        .map(|e| game_dir.join(e));
    let _ = clean_untracked_mods_with_exe(game_dir, exe_opt.as_deref());
    let _ = crate::core::vulkan_layer::unregister_vulkan_layer(game_dir);

    let manifest_path = bdir.join("manifest.json");
    if manifest_path.exists() {
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let archive_name = format!("manifest.json.done-{}", ts);
        let _ = fs::rename(&manifest_path, bdir.join(&archive_name));
        crate::core::logger::info("restore", &format!("Archived manifest to: {}", archive_name));
        let _ = prune_old_manifests(game_dir, 5);
    }

    crate::core::logger::info("restore", &format!("Restore successfully completed for {}", game_dir.display()));

    Ok(true)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryRow {
    pub date: String,
    pub dir: String,
    #[serde(default)]
    pub game_name: Option<String>,
    pub action: String,
    pub replaced: usize,
    pub added: usize,
}

pub fn history_path() -> PathBuf {
    crate::core::state::get_appdata_dir().join("history.json")
}

pub fn read_history() -> Vec<HistoryRow> {
    let path = history_path();
    if let Ok(bytes) = fs::read(&path) {
        let rows: Vec<HistoryRow> = serde_json::from_slice(&bytes).unwrap_or_default();
        rows.into_iter()
            .filter(|r| !r.dir.contains("dlss_test_") && !r.dir.contains("dlss_addon_test_"))
            .collect()
    } else {
        Vec::new()
    }
}

pub fn append_history(row: &HistoryRow) -> std::io::Result<()> {
    #[cfg(test)]
    {
        let _ = row;
        return Ok(());
    }
    #[cfg(not(test))]
    {
        if row.dir.contains("dlss_test_") || row.dir.contains("dlss_addon_test_") {
            return Ok(());
        }
        let path = history_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut history = read_history();
        history.push(row.clone());
        let bytes = serde_json::to_vec_pretty(&history)?;
        fs::write(path, bytes)?;
        Ok(())
    }
}

pub fn now_timestamp_str() -> String {
    let now = std::time::SystemTime::now();
    if let Ok(dur) = now.duration_since(std::time::UNIX_EPOCH) {
        let secs = dur.as_secs();
        let days = secs / 86400;
        let day_secs = secs % 86400;
        let hours = day_secs / 3600;
        let mins = (day_secs % 3600) / 60;
        let mut y = 1970;
        let mut d = days;
        loop {
            let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
            let ydays = if leap { 366 } else { 365 };
            if d < ydays { break; }
            d -= ydays;
            y += 1;
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let mdays = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut m = 1;
        for (idx, &dim) in mdays.iter().enumerate() {
            if d < dim { m = idx + 1; break; }
            d -= dim;
        }
        format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", y, m, d + 1, hours, mins)
    } else {
        "Recently".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_backup_available_and_read_done_manifest() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_journal_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let bdir = temp_dir.join("_DLSS5_Backup");
        fs::create_dir_all(&bdir).unwrap();

        assert!(!has_backup_available(&temp_dir));
        assert!(read_manifest(&temp_dir).is_none());

        // Create an archived manifest
        let manifest = ActiveManifest {
            route: "optiscaler".to_string(),
            ..Default::default()
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        fs::write(bdir.join("manifest.json.done-123456789"), &bytes).unwrap();

        // Archived manifest represents a completed restore; should NOT trigger active backup or active manifest
        assert!(!has_backup_available(&temp_dir), "Archived manifest must not trigger has_backup_available");
        assert!(read_manifest(&temp_dir).is_none(), "read_manifest must not fall back to archived manifest");

        // Can still be inspected via read_latest_done_manifest
        let read = read_latest_done_manifest(&temp_dir);
        assert!(read.is_some(), "read_latest_done_manifest should find archived manifest");
        assert_eq!(read.unwrap().route, "optiscaler");

        // When active manifest.json is present, both return true/Some
        fs::write(bdir.join("manifest.json"), &bytes).unwrap();
        assert!(has_backup_available(&temp_dir));
        assert!(read_manifest(&temp_dir).is_some());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_clean_untracked_mods() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_clean_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        fs::create_dir_all(&temp_dir).unwrap();

        let opti_ini = temp_dir.join("OptiScaler.ini");
        let reshade_dll = temp_dir.join("ReShade64.dll");
        let addon = temp_dir.join("renodx-mfgunlock.addon64");
        let safe_file = temp_dir.join("Game.exe");
        let proxy_dxgi = temp_dir.join("dxgi.dll");

        fs::write(&opti_ini, b"ini").unwrap();
        fs::write(&reshade_dll, b"dll").unwrap();
        fs::write(&addon, b"addon").unwrap();
        fs::write(&safe_file, b"exe").unwrap();

        let mut proxy_bytes = vec![0u8; 100_000];
        proxy_bytes[50_000..50_010].copy_from_slice(b"OptiScaler");
        fs::write(&proxy_dxgi, proxy_bytes).unwrap();

        let removed = clean_untracked_mods(&temp_dir).unwrap();
        assert_eq!(removed.len(), 4);
        assert!(!opti_ini.exists());
        assert!(!reshade_dll.exists());
        assert!(!addon.exists());
        assert!(!proxy_dxgi.exists(), "Proxy dxgi.dll must be removed");
        assert!(safe_file.exists(), "Original game executable must not be deleted");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_clean_untracked_mods_nested_reshade() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_clean_nested_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        let nested_dir = temp_dir.join("bin").join("x64");
        fs::create_dir_all(&nested_dir).unwrap();

        let nested_exe = nested_dir.join("MockCyberpunk2077.exe");
        let reshade_dxgi = nested_dir.join("dxgi.dll");
        let renodx_addon = nested_dir.join("renodx-dlss5.addon64");
        let mfg_addon = nested_dir.join("renodx-mfgunlock.addon64");
        let reshade_ini = nested_dir.join("ReShade.ini");

        fs::write(&nested_exe, b"MZ dummy exe").unwrap();
        fs::write(&renodx_addon, b"addon").unwrap();
        fs::write(&mfg_addon, b"mfg").unwrap();
        fs::write(&reshade_ini, b"[INPUT]\nKeyOverlay=36").unwrap();

        let mut reshade_bytes = vec![0u8; 100_000];
        reshade_bytes[50_000..50_007].copy_from_slice(b"ReShade");
        fs::write(&reshade_dxgi, reshade_bytes).unwrap();

        let removed = clean_untracked_mods_with_exe(&temp_dir, Some(&nested_exe)).unwrap();
        assert!(removed.contains(&"dxgi.dll".to_string()));
        assert!(removed.contains(&"renodx-dlss5.addon64".to_string()));
        assert!(removed.contains(&"renodx-mfgunlock.addon64".to_string()));
        assert!(removed.contains(&"ReShade.ini".to_string()));

        assert!(!reshade_dxgi.exists(), "Nested dxgi.dll must be cleaned");
        assert!(!renodx_addon.exists(), "Nested renodx-dlss5.addon64 must be cleaned");
        assert!(!mfg_addon.exists(), "Nested renodx-mfgunlock.addon64 must be cleaned");
        assert!(!reshade_ini.exists(), "Nested ReShade.ini must be cleaned");
        assert!(nested_exe.exists(), "Game executable must remain intact");

        let _ = fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_running_game_guard_prevents_clean_and_restore() {
        let procs = crate::core::install_guards::get_running_processes();
        if procs.is_empty() {
            return;
        }
        let running_proc_name = &procs[0].name;

        let temp_dir = std::env::temp_dir().join(format!("dlss_guard_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        fs::create_dir_all(&temp_dir).unwrap();

        let running_dummy_exe = temp_dir.join(running_proc_name);
        fs::write(&running_dummy_exe, b"MZ").unwrap();

        // 1. clean_untracked_mods_with_exe must reject cleaning while target process is running
        let clean_res = clean_untracked_mods_with_exe(&temp_dir, Some(&running_dummy_exe));
        assert!(clean_res.is_err(), "Cleaning must be blocked when game process is active");
        let err_str = clean_res.unwrap_err().to_string();
        assert!(err_str.contains("Close the game"), "Error must tell user to close the game first: {}", err_str);

        // 2. restore_game must also reject restoring while target process is running
        let manifest = ActiveManifest {
            game_exe: Some(running_proc_name.clone()),
            ..Default::default()
        };
        let bdir = backup_dir(&temp_dir);
        fs::create_dir_all(&bdir).unwrap();
        fs::write(bdir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();

        let restore_res = restore_game(&temp_dir);
        assert!(restore_res.is_err(), "Restore must be blocked when game process is active");
        let rest_err = restore_res.unwrap_err().to_string();
        assert!(rest_err.contains("Close the game"), "Error must tell user to close the game first: {}", rest_err);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_history_and_manifest_serialization() {
        let temp = std::env::temp_dir().join(format!("test_journal_ser_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();

        // Manifest path
        let m_path = backup_dir(&temp).join("manifest.json");
        assert!(m_path.ends_with("manifest.json"));

        // Read manifest on non-existent file
        assert!(read_manifest(&temp).is_none());

        // Read manifest with invalid JSON
        fs::create_dir_all(backup_dir(&temp)).unwrap();
        fs::write(&m_path, b"invalid-json").unwrap();
        assert!(read_manifest(&temp).is_none());

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_prune_old_manifests_keeps_last_5_and_cleans_orphaned_originals() {
        let temp = std::env::temp_dir().join(format!("dlss_prune_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let bdir = temp.join("_DLSS5_Backup");
        fs::create_dir_all(&bdir).unwrap();

        // Create 8 archived done manifests (ts 100 to 800)
        for i in 1..=8 {
            let ts = 1000 + i * 100;
            let prefix = format!("originals/{}", ts);
            let orig_dir = bdir.join(&prefix);
            fs::create_dir_all(&orig_dir).unwrap();
            fs::write(orig_dir.join("dxgi.dll"), b"mock-orig").unwrap();

            let manifest = ActiveManifest {
                backup_prefix: Some(prefix),
                ..Default::default()
            };
            let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
            fs::write(bdir.join(format!("manifest.json.done-{}", ts)), bytes).unwrap();
        }

        // Active manifest references originals/1800
        let active_manifest = ActiveManifest {
            backup_prefix: Some("originals/1800".to_string()),
            ..Default::default()
        };
        let active_bytes = serde_json::to_vec_pretty(&active_manifest).unwrap();
        fs::write(bdir.join("manifest.json"), active_bytes).unwrap();

        // Run pruner keeping top 5
        let pruned = prune_old_manifests(&temp, 5);
        assert_eq!(pruned, 3, "Must prune 3 older manifests out of 8");

        // Verify remaining done manifests: 1800, 1700, 1600, 1500, 1400 should exist
        assert!(bdir.join("manifest.json.done-1800").exists());
        assert!(bdir.join("manifest.json.done-1700").exists());
        assert!(bdir.join("manifest.json.done-1600").exists());
        assert!(bdir.join("manifest.json.done-1500").exists());
        assert!(bdir.join("manifest.json.done-1400").exists());

        // 1300, 1200, 1100 must be deleted
        assert!(!bdir.join("manifest.json.done-1300").exists());
        assert!(!bdir.join("manifest.json.done-1200").exists());
        assert!(!bdir.join("manifest.json.done-1100").exists());

        // Verify orphaned originals/1300, 1200, 1100 were cleaned up
        assert!(!bdir.join("originals/1300").exists());
        assert!(!bdir.join("originals/1200").exists());
        assert!(!bdir.join("originals/1100").exists());

        // Retained originals/1400 to 1800 must still exist
        assert!(bdir.join("originals/1400").exists());
        assert!(bdir.join("originals/1800").exists());

        let _ = fs::remove_dir_all(&temp);
    }
}

