#![allow(non_snake_case)]

use dioxus::prelude::*;
use base64::Engine;
use crate::core::scan::{scan_game_directory, scan_library_root, discover_all_launchers, discover_drive_roots, dedupe_games, GameEntry};
use crate::core::gpu::{detect_gpus, GpuInfo};

const BRAND_BADGE_PNG: &[u8] = include_bytes!("../../assets/brand-badge.png");


use crate::core::journal::{restore_game, read_history, append_history, HistoryRow};
use crate::core::install_guards::assert_game_closed;
use crate::core::compatibility::managed_mod_root;
use crate::core::state::{load_state, save_state, touch, ago, get_state_path, log_message, get_session_log};
use std::fs;


fn copy_to_clipboard(text: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let child = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$input | Set-Clipboard"])
            .stdin(std::process::Stdio::piped())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn();
        if let Ok(mut c) = child {
            if let Some(mut stdin) = c.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = c.wait();
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum AppStatus {
    Ready,
    DownloadingComponents,
    DownloadFailed(String),
    ScanningLibrary,
    ScanningFolder(String),
    ScanningLaunchers,
    FoundGames(usize),
}

fn format_status(lang: &str, status: &AppStatus) -> String {
    match status {
        AppStatus::Ready => crate::core::i18n::t(lang, "status_ready").to_string(),
        AppStatus::DownloadingComponents => crate::core::i18n::t(lang, "status_downloading_components").to_string(),
        AppStatus::DownloadFailed(_) => crate::core::i18n::t(lang, "status_download_failed").to_string(),
        AppStatus::ScanningLibrary => crate::core::i18n::t(lang, "status_scanning_library").to_string(),
        AppStatus::ScanningFolder(folder) => format!("{} {}...", crate::core::i18n::t(lang, "status_scanning_folder"), folder),
        AppStatus::ScanningLaunchers => crate::core::i18n::t(lang, "status_scanning_launchers").to_string(),
        AppStatus::FoundGames(count) => format!("{} ({} {})", crate::core::i18n::t(lang, "status_found_games"), count, crate::core::i18n::t(lang, "tab_games").to_lowercase()),
    }
}

fn clean_display_title(raw: &str) -> String {
    if raw.contains('_') && (raw.contains('.') || raw.contains("__")) {
        let base = raw.split('_').next().unwrap_or(raw);
        let name_part = base.split('.').last().unwrap_or(base);
        if !name_part.is_empty() {
            let mut spaced = String::new();
            let mut prev_is_lower = false;
            for ch in name_part.chars() {
                if ch.is_uppercase() && prev_is_lower {
                    spaced.push(' ');
                }
                prev_is_lower = ch.is_lowercase();
                spaced.push(ch);
            }
            return spaced;
        }
    }
    raw.replace('_', " ").replace('-', " ")
}

fn resolve_game_title(row: &HistoryRow, games: &[GameEntry]) -> String {
    if let Some(ref name) = row.game_name {
        if !name.trim().is_empty() {
            return name.clone();
        }
    }
    let row_p = std::path::Path::new(&row.dir);
    for g in games {
        let g_p = std::path::Path::new(&g.dir);
        if row_p == g_p || row.dir.eq_ignore_ascii_case(&g.dir.to_string_lossy()) || row_p.starts_with(g_p) || g_p.starts_with(row_p) {
            return g.name.clone();
        }
    }
    let manifests = [
        row_p.join("MicrosoftGame.config"),
        row_p.join("Content").join("MicrosoftGame.config"),
        row_p.join("appxmanifest.xml"),
    ];
    let display_name_re = regex::Regex::new(r#"(?i)DefaultDisplayName\s*=\s*"([^"]+)""#).unwrap();
    let display_name_tag_re = regex::Regex::new(r#"(?i)<DisplayName>\s*([^<]+)\s*</DisplayName>"#).unwrap();
    for m in &manifests {
        if let Ok(text) = std::fs::read_to_string(m) {
            if let Some(cap) = display_name_re.captures(&text).or_else(|| display_name_tag_re.captures(&text)) {
                let n = cap[1].trim();
                if !n.is_empty() && !n.starts_with("ms-resource:") {
                    return n.to_string();
                }
            }
        }
    }
    if let Some(fname) = row_p.file_name().and_then(|f| f.to_str()) {
        if fname.eq_ignore_ascii_case("content") || fname.eq_ignore_ascii_case("win64") || fname.eq_ignore_ascii_case("binaries") {
            if let Some(parent) = row_p.parent().and_then(|p| p.file_name()).and_then(|f| f.to_str()) {
                if parent.eq_ignore_ascii_case("binaries") {
                    if let Some(gp) = row_p.parent().and_then(|p| p.parent()).and_then(|p| p.file_name()).and_then(|f| f.to_str()) {
                        return clean_display_title(gp);
                    }
                }
                return clean_display_title(parent);
            }
        }
        return clean_display_title(fname);
    }
    row.dir.clone()
}

pub fn App() -> Element {
    let init_state = use_hook(|| {
        let mut state = load_state();
        let hidden = state.hidden.clone();
        state.cached_games.retain(|g| {
            let norm = crate::core::state::normalize_game_path(&g.dir);
            !hidden.iter().any(|h| crate::core::state::normalize_path_str(h) == norm)
        });
        let art_dir = crate::core::state::get_appdata_dir().join("art");
        let custom_names = state.custom_names.clone();
        let mut state_changed = false;
        for game in &mut state.cached_games {
            let norm = crate::core::state::normalize_game_path(&game.dir);
            if let Some(custom) = custom_names.get(&norm) {
                game.name = custom.clone();
            }
            let key = crate::core::steamart::key_for_dir(&game.dir);
            let cover_file = art_dir.join(format!("{}-cover.jpg", key));
            let hero_file = art_dir.join(format!("{}-hero.jpg", key));

            // Normalize any legacy dlss-art:// or raw path
            if let Some(ref p) = game.poster {
                let norm_p = crate::core::steamart::normalize_art_uri(p);
                if norm_p != *p {
                    game.poster = Some(norm_p);
                    state_changed = true;
                }
            }

            let needs_art = match game.poster {
                None => true,
                Some(ref p) => p.starts_with("data:image/"),
            };
            if needs_art {
                if cover_file.exists() && cover_file.metadata().map(|m| m.len() > 2000).unwrap_or(false) {
                    game.poster = Some(format!("http://dlss-art.localhost/art/{}-cover.jpg", key));
                    state_changed = true;
                } else if hero_file.exists() && hero_file.metadata().map(|m| m.len() > 2000).unwrap_or(false) {
                    game.poster = Some(format!("http://dlss-art.localhost/art/{}-hero.jpg", key));
                    state_changed = true;
                }
            }
        }
        if state_changed {
            let _ = crate::core::state::save_state(&state);
        }
        state
    });

    let init_theme = init_state.theme.clone();
    let init_lang = init_state.lang.clone();
    let init_group = init_state.group_games_by_store;
    let init_auto_scan = init_state.auto_scan_drives;
    let init_rust_theme = init_state.rust_theme;
    let init_run_in_bg = init_state.run_in_background;
    let init_folders = init_state.folders.clone();
    let init_addons = init_state.addons.clone();
    let init_custom_addons = init_state.addon_files.clone();
    let init_overlay_theme = init_state.overlay_theme.clone();
    let init_overlay_hotkey = init_state.overlay_hotkey.clone();
    let init_overlay_enabled = init_state.overlay_enabled;
    let init_custom_overlay_themes = init_state.custom_overlay_themes.clone();
    let init_cached_games = init_state.cached_games.clone();
    let init_recents = init_state.recents.clone();
    let init_hidden_count = init_state.hidden.len();

    let mut gpus = use_signal(|| vec![GpuInfo {
        name: "DirectX 12 Physical Device".to_string(),
        vendor_id: 0x10DE,
        device_id: 0x2684,
        dedicated_video_memory: 24 * 1024 * 1024 * 1024,
        is_rtx_40: true,
    }]);
    let mut games = use_signal(move || init_cached_games);
    let recents = use_signal(move || init_recents);
    let mut hidden_count = use_signal(move || init_hidden_count);
    let init_active_view = std::env::var("DLSS_TEST_VIEW").unwrap_or_else(|_| "home".to_string());
    let mut active_view = use_signal(move || init_active_view);
    let mut theme = use_signal(move || init_theme);
    let mut current_lang = use_signal(move || init_lang);
    let mut rust_theme = use_signal(move || init_rust_theme);
    let mut run_in_bg = use_signal(move || init_run_in_bg);
    let mut launch_at_startup = use_signal(|| crate::core::tray::is_startup_enabled());
    let mut lang_menu_open = use_signal(|| false);

    // Status bar state
    let mut status_text = use_signal(|| "Ready".to_string());
    let mut status_state = use_signal(|| {
        if crate::core::downloader::are_all_mandatory_components_cached() {
            AppStatus::Ready
        } else {
            AppStatus::DownloadingComponents
        }
    });
    let mut status_percent = use_signal(|| {
        if crate::core::downloader::are_all_mandatory_components_cached() {
            100.0f32
        } else {
            0.0f32
        }
    });

    let brand_badge_data_uri = use_hook(|| {
        let b64 = base64::engine::general_purpose::STANDARD.encode(BRAND_BADGE_PNG);
        format!("data:image/png;base64,{}", b64)
    });

    // Background System Tray message listener
    use_future(move || async move {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--background" || a == "--minimized") {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            crate::core::tray::trim_working_set();
        }
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            if crate::core::tray::take_show_window_request() {
                let win = dioxus::desktop::window();
                win.set_visible(true);
                win.set_minimized(false);
                win.set_focus();
            }
        }
    });

    // Automated background component downloader
    let mut component_download_status = use_signal(|| {
        if crate::core::downloader::are_all_mandatory_components_cached() {
            None
        } else {
            Some("Checking components...".to_string())
        }
    });
    let mut trigger_component_download = use_signal(|| 0usize);

    use_future(move || {
        let _ = *trigger_component_download.read();
        async move {
            if !crate::core::downloader::are_all_mandatory_components_cached() {
                status_state.set(AppStatus::DownloadingComponents);
                status_percent.set(0.0);
                let res = crate::core::downloader::ensure_all_mandatory_components_with_progress(|prog| {
                    status_percent.set(prog.percentage);
                    if prog.is_downloading {
                        let bytes_str = match prog.total_bytes {
                            Some(total) => format!(" ({} / {})", crate::core::downloader::format_bytes(prog.downloaded_bytes), crate::core::downloader::format_bytes(total)),
                            None => format!(" ({})", crate::core::downloader::format_bytes(prog.downloaded_bytes)),
                        };
                        component_download_status.set(Some(format!("{}{}", prog.message, bytes_str)));
                    } else {
                        component_download_status.set(None);
                    }
                }).await;

                if crate::core::downloader::are_all_mandatory_components_cached() {
                    status_percent.set(100.0);
                    component_download_status.set(None);
                    status_state.set(AppStatus::Ready);
                } else {
                    let err = res.err().unwrap_or_default();
                    status_percent.set(0.0);
                    component_download_status.set(Some(crate::core::i18n::t(&current_lang.read(), "status_download_failed").to_string()));
                    status_state.set(AppStatus::DownloadFailed(err));
                }
            } else {
                status_percent.set(100.0);
                component_download_status.set(None);
                status_state.set(AppStatus::Ready);
            }
        }
    });

    use_effect(move || {
        let t = theme.read().clone();
        let l = current_lang.read().clone();
        let rt = if *rust_theme.read() { "true" } else { "false" };
        let script = format!(
            "document.documentElement.setAttribute('data-theme', '{}'); document.documentElement.setAttribute('data-rust-theme', '{}'); document.documentElement.lang = '{}';",
            t, rt, l
        );
        let _ = dioxus::desktop::window().webview.evaluate_script(&script);
    });

    // Search and filters
    let mut search_query = use_signal(String::new);
    let mut filter_api = use_signal(|| "all".to_string());
    let mut filter_dlss = use_signal(|| "all".to_string());
    let mut filter_addon = use_signal(|| "all".to_string());

    // Sheet / Game detail modal state
    let init_sheet_open = std::env::var("DLSS_TEST_SHEET").is_ok();
    let mut sheet_open = use_signal(move || init_sheet_open);
    let mut sheet_game_idx = use_signal(|| if std::env::var("DLSS_TEST_SHEET").is_ok() { Some(0) } else { None });
    let mut is_editing_game_name = use_signal(|| false);
    let mut edit_game_name_val = use_signal(|| String::new());
    let mut backend_choice = use_signal(|| "reshade".to_string());
    let mut route_choice = use_signal(|| "native".to_string());
    let mut opti_pre_sr = use_signal(|| true);
    let mut opti_passes = use_signal(|| 3u32);
    let mut mfg_choice = use_signal(|| false);
    let mut mfg_multiplier = use_signal(|| 4u32);
    let mut job_lines = use_signal(|| vec!["Ready".to_string()]);
    let mut copy_toast = use_signal(|| false);
    let mut copy_toast_text = use_signal(|| "Copied to clipboard".to_string());

    // Settings state
    let mut group_games_by_store = use_signal(move || init_group);
    let mut auto_scan_drives = use_signal(move || init_auto_scan);
    let mut managed_folders = use_signal(move || init_folders);

    // start_overlay_bridge(); // Shelved for standalone stability
    log_message(&format!("@{{log_library_loaded|{}}}", games.read().len()));

    // Activity log for Home view
    let mut activity_log = use_signal(get_session_log);

    // Add-on management state
    let mut addons_active = use_signal(move || init_addons);
    let mut custom_addons = use_signal(move || init_custom_addons);
    let mut show_addon_dlg = use_signal(|| false);
    let mut dlg_path = use_signal(String::new);
    let mut dlg_file_info = use_signal(String::new);
    let mut dlg_name = use_signal(String::new);
    let mut dlg_desc = use_signal(String::new);
    let mut dlg_tag = use_signal(String::new);

    let _overlay_enabled = use_signal(move || init_overlay_enabled);
    let mut overlay_theme = use_signal(move || init_overlay_theme);
    let mut overlay_hotkey = use_signal(move || init_overlay_hotkey);
    let mut custom_overlay_themes = use_signal(move || init_custom_overlay_themes);
    let init_hotkey_open = std::env::var("DLSS_TEST_HOTKEY").is_ok();
    let mut overlay_hotkey_open = use_signal(move || init_hotkey_open);
    let mut overlay_create_open = use_signal(|| false);
    let init_preview_open = std::env::var("DLSS_TEST_PREVIEW").is_ok();
    let mut overlay_preview_open = use_signal(move || init_preview_open);
    let preview_theme_id = use_signal(|| "green".to_string());
    let mut custom_theme_name = use_signal(|| "My Theme".to_string());
    let mut custom_theme_color = use_signal(|| "#ff7a00".to_string());
    let mut is_busy = use_signal(|| false);

    // Interactive preview controls for the live RenoDX modal
    let mut prev_dlss_on = use_signal(|| true);
    let mut prev_structure = use_signal(|| 1.00f32);
    let mut prev_tone = use_signal(|| 1.00f32);
    let mut prev_char_mask = use_signal(|| true);
    let mut prev_char_structure = use_signal(|| 1.00f32);
    let mut prev_nr_style = use_signal(|| 0usize);
    let mut prev_overall_intensity = use_signal(|| 1.00f32);
    let mut prev_local_tone = use_signal(|| 0.00f32);
    let mut prev_diffuse_white = use_signal(|| 203.0f32);
    let mut prev_motion_x = use_signal(|| 1.00f32);
    let mut prev_motion_y = use_signal(|| 1.00f32);

    // History state
    let mut history_rows = use_signal(read_history);

    // Async GPU detection without blocking UI - runs exactly once on mount
    use_hook(|| {
        spawn(async move {
            if let Ok(detected) = tokio::task::spawn_blocking(detect_gpus).await {
                if !detected.is_empty() {
                    gpus.set(detected);
                }
            }
        });
    });

    // Async background library scan: completely non-blocking in background thread - runs exactly once on mount
    use_hook(|| {
        spawn(async move {
            static SCAN_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            use std::sync::atomic::Ordering;
            if SCAN_RUNNING.swap(true, Ordering::SeqCst) {
                return;
            }

            status_text.set("Scanning library in background...".to_string());
            status_state.set(AppStatus::ScanningLibrary);
            status_percent.set(30.0);
            log_message("@{log_scanning_background}");
            activity_log.set(get_session_log());

            let fresh = tokio::task::spawn_blocking(move || {
                let mut detected = discover_all_launchers();
                let state = load_state();
                for manual_path in &state.manual {
                    let p = std::path::Path::new(manual_path);
                    if p.exists() {
                        if let Some(game) = scan_game_directory(p) {
                            detected.push(game);
                        }
                    }
                }
                for folder_path in &state.folders {
                    let p = std::path::Path::new(folder_path);
                    if p.exists() {
                        detected.extend(scan_library_root(p));
                    }
                }
                if state.auto_scan_drives {
                    for root in discover_drive_roots(&state.excluded_roots) {
                        detected.extend(scan_library_root(&root));
                    }
                }
                let mut games = dedupe_games(detected);
                games.retain(|g| !state.is_hidden(&g.dir));
                games
            }).await.unwrap_or_default();

            SCAN_RUNNING.store(false, Ordering::SeqCst);

            if !fresh.is_empty() {
                games.set(fresh.clone());
                let mut s = load_state();
                s.cached_games = fresh.clone();
                let _ = save_state(&s);

                // Background Steam Art Resolver: resolve any games still missing posters
                let missing: Vec<(String, std::path::PathBuf)> = fresh.iter()
                    .filter(|g| g.poster.is_none())
                    .map(|g| (g.name.clone(), g.dir.clone()))
                    .collect();

                if !missing.is_empty() {
                    spawn(async move {
                        for (g_name, g_dir) in missing {
                            let art = crate::core::steamart::resolve_game_art(&g_name, &g_dir, None).await;
                            if let Some(cover) = art.cover_path {
                                let mut list = games.read().clone();
                                if let Some(item) = list.iter_mut().find(|x| x.dir == g_dir) {
                                    item.poster = Some(cover);
                                    games.set(list.clone());
                                    let mut s = load_state();
                                    s.cached_games = list;
                                    let _ = save_state(&s);
                                }
                            }
                        }
                    });
                }
            }

            let g_count = fresh.len();
            let dx12_count = fresh.iter().filter(|g| g.api == "DirectX 12").count();
            let mut sources = std::collections::HashSet::new();
            for g in &fresh {
                sources.insert(&g.launcher);
            }
            log_message(&format!("@{{log_found_games|{}|{}}}", g_count, sources.len()));
            log_message(&format!("@{{log_library_ready|{}|{}}}", g_count, dx12_count));
            activity_log.set(get_session_log());

            status_text.set("Ready".to_string());
            status_state.set(AppStatus::Ready);
            status_percent.set(100.0);
        });
    });

    let primary_gpu = gpus.read().first().cloned().unwrap_or_else(|| GpuInfo {
        name: "DirectX 12 Physical Device".to_string(),
        vendor_id: 0x10DE,
        device_id: 0x2684,
        dedicated_video_memory: 24 * 1024 * 1024 * 1024,
        is_rtx_40: true,
    });

    let selected_game = sheet_game_idx.read().and_then(|idx| games.read().get(idx).cloned());

    // Filter games list (lazy: only evaluate when games tab is active)
    let (visible_games, store_order): (Vec<(usize, GameEntry)>, Vec<String>) = if *active_view.read() == "games" {
        let q = search_query.read().to_lowercase();
        let api = filter_api.read().to_lowercase();
        let dlss = filter_dlss.read().clone();
        let addon = filter_addon.read().clone();

        let vg: Vec<(usize, GameEntry)> = games.read().iter().cloned().enumerate().filter(|(_, g)| {
            if !q.is_empty() && !g.name.to_lowercase().contains(&q) && !g.dir.to_string_lossy().to_lowercase().contains(&q) {
                return false;
            }
            if api != "all" {
                if api == "d3d12" && !g.api.contains("12") { return false; }
                if api == "d3d11" && !g.api.contains("11") { return false; }
                if api == "vulkan" && !g.api.to_lowercase().contains("vulkan") { return false; }
                if api == "d3d9" && !g.api.contains("9") { return false; }
                if api == "d3d8" && !g.api.contains("8") { return false; }
                if api == "opengl" && !g.api.to_lowercase().contains("opengl") { return false; }
            }
            if dlss == "ready" && g.dlss_version.is_none() && !g.api.contains("12") { return false; }
            if dlss == "has_dlss" && g.dlss_version.is_none() { return false; }
            if dlss == "absent" && g.dlss_version.is_some() { return false; }
            if dlss == "installed" && !g.has_backup { return false; }
            if addon == "installed" && !g.optiscaler_installed && !g.mfg_unlock_installed { return false; }

            true
        }).collect();

        let mut so = vec![
            "Steam".to_string(),
            "Xbox".to_string(),
            "Epic Games".to_string(),
            "GOG".to_string(),
            "Added by hand".to_string(),
            "My folders".to_string(),
        ];
        for (_, g) in &vg {
            if !so.contains(&g.launcher) && !g.launcher.is_empty() {
                so.push(g.launcher.clone());
            }
        }
        (vg, so)
    } else {
        (Vec::new(), Vec::new())
    };

    rsx! {
        div {
            class: "shell",
            onclick: move |_| {
                if *lang_menu_open.read() {
                    lang_menu_open.set(false);
                }
            },
            // ---------------- SIDEBAR ----------------
            aside {
                class: "sidebar",
                style: "cursor: default; user-select: none;",
                onmousedown: move |_| { dioxus::desktop::window().drag(); },
                ondoubleclick: move |_| { dioxus::desktop::window().toggle_maximized(); },
                div {
                    class: "brand",
                    id: "brand",
                    style: "cursor: default; user-select: none; display: flex; align-items: center; justify-content: center; width: 100%;",
                    onmousedown: move |_| { dioxus::desktop::window().drag(); },
                    ondoubleclick: move |_| { dioxus::desktop::window().toggle_maximized(); },
                    img {
                        class: "brand-rust-badge",
                        src: "{brand_badge_data_uri}",
                        alt: "DLSS 5 STUDIO",
                    }
                }

                nav {
                    class: "nav",
                    id: "nav",
                    onmousedown: move |e| e.stop_propagation(),
                    button {
                        class: if *active_view.read() == "home" { "nav-item active" } else { "nav-item" },
                        onclick: move |_| active_view.set("home".to_string()),
                        svg { view_box: "0 0 24 24",
                            path { d: "M3 10.5 12 3l9 7.5" }
                            path { d: "M5 9.5V20h14V9.5" }
                        }
                        span { "{crate::core::i18n::t(&current_lang.read(), \"nav_home\")}" }
                        i { class: "dot" }
                    }
                    button {
                        class: if *active_view.read() == "games" { "nav-item active" } else { "nav-item" },
                        onclick: move |_| active_view.set("games".to_string()),
                        svg { view_box: "0 0 24 24",
                            rect { x: "2", y: "7", width: "20", height: "11", rx: "4" }
                            path { d: "M7 11v3m-1.5-1.5h3M16 12h.01M18.5 14h.01" }
                        }
                        span { "{crate::core::i18n::t(&current_lang.read(), \"nav_games\")}" }
                        i { class: "dot" }
                    }
                    button {
                        class: if *active_view.read() == "addons" { "nav-item active" } else { "nav-item" },
                        onclick: move |_| active_view.set("addons".to_string()),
                        svg { view_box: "0 0 24 24",
                            path { d: "M10 4.5a1.9 1.9 0 1 1 3.8 0V6H17a1 1 0 0 1 1 1v3.2h1.5a1.9 1.9 0 1 1 0 3.8H18V17a1 1 0 0 1-1 1h-3.2v1.5a1.9 1.9 0 1 1-3.8 0V18H7a1 1 0 0 1-1-1v-3.2H4.5a1.9 1.9 0 1 1 0-3.8H6V7a1 1 0 0 1 1-1h3z" }
                        }
                        span { "{crate::core::i18n::t(&current_lang.read(), \"nav_addons\")}" }
                        i { class: "dot" }
                    }
                    button {
                        class: if *active_view.read() == "history" { "nav-item active" } else { "nav-item" },
                        onclick: move |_| {
                            history_rows.set(read_history());
                            active_view.set("history".to_string());
                        },
                        svg { view_box: "0 0 24 24",
                            path { d: "M3 12a9 9 0 1 0 3-6.7" }
                            path { d: "M3 4v5h5M12 7v5l3.5 2" }
                        }
                        span { "{crate::core::i18n::t(&current_lang.read(), \"nav_history\")}" }
                        i { class: "dot" }
                    }
                    button {
                        class: if *active_view.read() == "settings" { "nav-item active" } else { "nav-item" },
                        onclick: move |_| active_view.set("settings".to_string()),
                        svg { view_box: "0 0 24 24",
                            circle { cx: "12", cy: "12", r: "3.2" }
                            path { d: "M19.5 14.5a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.6-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.9.3H9a1.7 1.7 0 0 0 1-1.6V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.9V9a1.7 1.7 0 0 0 1.6 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.4 1z" }
                        }
                        span { "{crate::core::i18n::t(&current_lang.read(), \"nav_settings\")}" }
                        i { class: "dot" }
                    }
                    button {
                        class: if *active_view.read() == "about" { "nav-item active" } else { "nav-item" },
                        onclick: move |_| active_view.set("about".to_string()),
                        svg { view_box: "0 0 24 24",
                            circle { cx: "12", cy: "12", r: "9" }
                            path { d: "M12 17v-5M12 8h.01" }
                        }
                        span { "{crate::core::i18n::t(&current_lang.read(), \"nav_about\")}" }
                        i { class: "dot" }
                    }
                }

                div {
                    class: "status-card",
                    style: if matches!(*status_state.read(), AppStatus::DownloadFailed(_)) { "cursor: pointer;" } else { "" },
                    title: if matches!(*status_state.read(), AppStatus::DownloadFailed(_)) { crate::core::i18n::t(&current_lang.read(), "tooltip_retry_download") } else { "" },
                    onclick: move |_| {
                        if matches!(*status_state.read(), AppStatus::DownloadFailed(_)) {
                            *trigger_component_download.write() += 1;
                        }
                    },
                    onmousedown: move |e| e.stop_propagation(),
                    div { class: "status-line",
                        i {
                            class: match *status_state.read() {
                                AppStatus::DownloadingComponents => "live downloading",
                                AppStatus::DownloadFailed(_) => "live error",
                                _ => "live",
                            }
                        }
                        b { id: "statusText", "{format_status(&current_lang.read(), &status_state.read())}" }
                    }
                    p { class: "status-sub", "DLSS 5 Studio" }
                    p { class: "status-sub", id: "statusVersion", "v1.0" }
                    div { class: "bar",
                        i { id: "statusBar", style: "width: {status_percent.read()}%;" }
                    }
                }
            }

            // ---------------- MAIN WORKSPACE ----------------
            div { class: "main",
                // Toolbar
                div {
                    class: "toolbar",
                    style: "cursor: default; user-select: none;",
                    onmousedown: move |_| { dioxus::desktop::window().drag(); },
                    ondoubleclick: move |_| { dioxus::desktop::window().toggle_maximized(); },
                    div {
                        style: "flex: 1; height: 34px; cursor: default;",
                        onmousedown: move |_| { dioxus::desktop::window().drag(); },
                        ondoubleclick: move |_| { dioxus::desktop::window().toggle_maximized(); },
                    }
                    button {
                        id: "themeBtn",
                        class: "round",
                        title: if *theme.read() == "dark" { crate::core::i18n::t(&current_lang.read(), "tooltip_switch_light") } else { crate::core::i18n::t(&current_lang.read(), "tooltip_switch_dark") },
                        onmousedown: move |e| e.stop_propagation(),
                        onclick: move |e| {
                            e.stop_propagation();
                            let next = if *theme.read() == "dark" { "light" } else { "dark" };
                            theme.set(next.to_string());
                            let mut s = load_state();
                            s.theme = next.to_string();
                            let _ = save_state(&s);
                            let script = format!("document.documentElement.setAttribute('data-theme', '{}');", next);
                            let _ = dioxus::desktop::window().webview.evaluate_script(&script);
                        },
                        dangerous_inner_html: r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="4.2"/><path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M19.1 4.9l-1.8 1.8M6.7 17.3l-1.8 1.8"/></svg>"#
                    }
                    div {
                        class: "lang-wrap",
                        style: "position: relative;",
                        onmousedown: move |e| e.stop_propagation(),
                        button {
                            id: "langBtn",
                            class: "pill",
                            onmousedown: move |e| e.stop_propagation(),
                            onclick: move |e| {
                                e.stop_propagation();
                                lang_menu_open.toggle();
                            },
                            span { id: "langLabel", "{current_lang.read().to_uppercase()}" }
                            svg { view_box: "0 0 24 24", path { d: "M6 9l6 6 6-6" } }
                        }
                        if *lang_menu_open.read() {
                            div {
                                class: "lang-menu",
                                id: "langMenu",
                                style: "max-height: 380px; overflow-y: auto;",
                                onmousedown: move |e| e.stop_propagation(),
                                onclick: move |e| e.stop_propagation(),
                                for item in crate::core::i18n::SUPPORTED_LANGS {
                                    {
                                        let code = item.code;
                                        let is_active = *current_lang.read() == code;
                                        rsx! {
                                            button {
                                                class: if is_active { "lang-item active" } else { "lang-item" },
                                                onmousedown: move |e| e.stop_propagation(),
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    current_lang.set(code.to_string());
                                                    let mut s = load_state();
                                                    s.lang = code.to_string();
                                                    let _ = save_state(&s);
                                                    let script = format!("document.documentElement.lang = '{}';", code);
                                                    let _ = dioxus::desktop::window().webview.evaluate_script(&script);
                                                    lang_menu_open.set(false);
                                                },
                                                span { "{item.native}" }
                                                span { class: "code", "{item.code.to_uppercase()}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "win",
                        id: "winMin",
                        title: "{crate::core::i18n::t(&current_lang.read(), \"win_min\")}",
                        onmousedown: move |e| e.stop_propagation(),
                        onclick: move |e| {
                            e.stop_propagation();
                            dioxus::desktop::window().set_minimized(true);
                        },
                        dangerous_inner_html: r#"<svg viewBox="0 0 24 24"><path d="M6 12h12"/></svg>"#
                    }
                    button {
                        class: "win",
                        id: "winMax",
                        title: "{crate::core::i18n::t(&current_lang.read(), \"win_max\")}",
                        onmousedown: move |e| e.stop_propagation(),
                        onclick: move |e| {
                            e.stop_propagation();
                            dioxus::desktop::window().toggle_maximized();
                        },
                        dangerous_inner_html: r#"<svg viewBox="0 0 24 24"><rect x="5" y="5" width="14" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="2"/></svg>"#
                    }
                    button {
                        class: "win close",
                        id: "winClose",
                        title: "{crate::core::i18n::t(&current_lang.read(), \"win_close\")}",
                        onmousedown: move |e| e.stop_propagation(),
                        onclick: move |e| {
                            e.stop_propagation();
                            if *run_in_bg.read() {
                                dioxus::desktop::window().set_visible(false);
                                crate::core::tray::show_background_notification();
                                crate::core::tray::trim_working_set();
                            } else {
                                dioxus::desktop::window().close();
                                std::process::exit(0);
                            }
                        },
                        dangerous_inner_html: r#"<svg viewBox="0 0 24 24"><path d="M7 7l10 10M17 7L7 17"/></svg>"#
                    }
                }

                // ---------------- HOME VIEW ----------------
                if *active_view.read() == "home" {
                    section { class: "view active", id: "view-home",
                        div { class: "glass dropzone", id: "dropZone",
                            div { class: "dz-circle",
                                dangerous_inner_html: r#"<svg viewBox="0 0 24 24"><path d="M3 8a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>"#
                            }
                            h2 { "{crate::core::i18n::t(&current_lang.read(), \"drop_title\")}" }
                            p { class: "or", "{crate::core::i18n::t(&current_lang.read(), \"home_or\")}" }
                            button {
                                class: "glass-btn",
                                id: "browseBtn",
                                onclick: move |_| {
                                    spawn(async move {
                                        if let Some(handle) = rfd::AsyncFileDialog::new().set_title("Browse for game folder").pick_folder().await {
                                            let folder = handle.path().to_path_buf();
                                            let maybe_game = tokio::task::spawn_blocking({
                                                let folder = folder.clone();
                                                move || scan_game_directory(&folder)
                                            }).await.unwrap_or(None);

                                            if let Some(game) = maybe_game {
                                                let mut s = load_state();
                                                s.unhide_game(&game.dir);
                                                let f_str = folder.to_string_lossy().to_string();
                                                if !s.manual.contains(&f_str) {
                                                    s.manual.push(f_str.clone());
                                                }
                                                let mut current_games = games.read().clone();
                                                let idx = if let Some(pos) = current_games.iter().position(|g| g.dir == game.dir) {
                                                    current_games[pos] = game;
                                                    pos
                                                } else {
                                                    current_games.push(game);
                                                    current_games.len() - 1
                                                };
                                                s.cached_games = current_games.clone();
                                                let _ = save_state(&s);
                                                touch(&f_str);
                                                games.set(current_games);
                                                sheet_game_idx.set(Some(idx));
                                                sheet_open.set(true);
                                                active_view.set("games".to_string());
                                            }
                                        }
                                    });
                                },
                                "{crate::core::i18n::t(&current_lang.read(), \"home_browse_folder\")}"
                            }
                        }

                        div {
                            class: "section-head",
                            style: "cursor: default; user-select: none;",
                            onmousedown: move |_| { dioxus::desktop::window().drag(); },
                            ondoubleclick: move |_| { dioxus::desktop::window().toggle_maximized(); },
                            h3 { "{crate::core::i18n::t(&current_lang.read(), \"home_recent_games\")}" }
                            button {
                                class: "link",
                                onmousedown: move |e| e.stop_propagation(),
                                onclick: move |_| active_view.set("games".to_string()),
                                span { "{crate::core::i18n::t(&current_lang.read(), \"home_view_all\")}" }
                                svg { view_box: "0 0 24 24", path { d: "M9 6l6 6-6 6" } }
                            }
                        }
                        div { class: "recents", id: "recents",
                            {
                                let recent_list: Vec<(usize, GameEntry, String)> = recents.read().iter().filter_map(|r| {
                                    let time_ago = ago(r.at);
                                    let matched = games.read().iter().enumerate().find(|(_, g)| {
                                        g.dir.to_string_lossy().to_lowercase() == r.dir.to_lowercase()
                                    }).map(|(idx, g)| (idx, g.clone(), time_ago.clone()));
                                    matched
                                }).take(12).collect();

                                if recent_list.is_empty() {
                                    rsx! {
                                        p { class: "empty", "{crate::core::i18n::t(&current_lang.read(), \"home_recents_empty\")}" }
                                    }
                                } else {
                                    rsx! {
                                        for (orig_idx, game, time_str) in recent_list {
                                            {
                                                let idx = orig_idx;
                                                let g_name = game.name.clone();
                                                let g_dir = game.dir.clone();
                                                let is_ready = game.optiscaler_installed || game.mfg_unlock_installed;
                                                let poster_opt = game.poster.as_ref().map(|p| crate::core::steamart::normalize_art_uri(p));
                                                rsx! {
                                                    article {
                                                        class: "rcard",
                                                        tabindex: "0",
                                                        onclick: move |_| {
                                                            sheet_game_idx.set(Some(idx));
                                                            sheet_open.set(true);
                                                        },
                                                        div { class: "tools",
                                                            button {
                                                                class: "tool cover-tool",
                                                                title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_change_cover\")}",
                                                                onclick: {
                                                                    let dir = g_dir.clone();
                                                                    move |e: MouseEvent| {
                                                                        e.stop_propagation();
                                                                        pick_and_set_cover(&dir, games);
                                                                    }
                                                                },
                                                                svg {
                                                                    view_box: "0 0 24 24",
                                                                    style: "width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round;",
                                                                    rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                                                                    circle { cx: "8.5", cy: "8.5", r: "1.5" }
                                                                    polyline { points: "21 15 16 10 5 21" }
                                                                }
                                                            }
                                                            button {
                                                                class: "tool",
                                                                title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_open_explorer\")}",
                                                                onclick: {
                                                                    let dir = g_dir.clone();
                                                                    move |e: MouseEvent| {
                                                                        e.stop_propagation();
                                                                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                                                                    }
                                                                },
                                                                "📁"
                                                            }
                                                        }
                                                        button {
                                                            class: "card-center-play",
                                                            title: "{crate::core::i18n::t_param(&current_lang.read(), \"tooltip_launch_game\", &g_name)}",
                                                            onclick: {
                                                                let g = game.clone();
                                                                move |e: MouseEvent| {
                                                                    e.stop_propagation();
                                                                    launch_game(&g);
                                                                }
                                                            },
                                                            svg {
                                                                view_box: "0 0 24 24",
                                                                polygon { points: "6,4 20,12 6,20" }
                                                            }
                                                            span { "{crate::core::i18n::t(&current_lang.read(), \"home_play\")}" }
                                                        }
                                                        if let Some(ref p_url) = poster_opt {
                                                            img { src: "{p_url}", alt: "{g_name}" }
                                                        } else {
                                                            div { class: "initials", "{&g_name[0..2.min(g_name.len())]}" }
                                                        }
                                                        div { class: "meta",
                                                            div { class: "title", "{g_name}" }
                                                            div { class: "when",
                                                                span { class: "ago", "{time_str}" }
                                                                i { class: if is_ready { "pip on" } else { "pip" } }
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

                        div { class: "section-head",
                            h3 { "{crate::core::i18n::t(&current_lang.read(), \"home_activity\")}" }
                            div { class: "row",
                                button {
                                    class: "ghost sm",
                                    id: "copyLog",
                                    onclick: move |_| {
                                        let lang = current_lang.read().clone();
                                        let text = activity_log.read()
                                            .iter()
                                            .map(|line| {
                                                let l = line.as_str();
                                                if l.starts_with('[') && l.len() > 10 && l.chars().nth(9) == Some(']') {
                                                    format!("{}{}", &l[0..10], crate::core::i18n::format_log_entry(&lang, &l[10..]))
                                                } else {
                                                    crate::core::i18n::format_log_entry(&lang, l)
                                                }
                                            })
                                            .collect::<Vec<String>>()
                                            .join("\r\n");
                                        copy_to_clipboard(&text);
                                        copy_toast_text.set(crate::core::i18n::t(&current_lang.read(), "toast_activity_copied").to_string());
                                        copy_toast.set(true);
                                    },
                                    "{crate::core::i18n::t(&current_lang.read(), \"btn_copy_log\")}"
                                }
                                button {
                                    class: "ghost sm",
                                    id: "openLogFile",
                                    title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_open_logfile\")}",
                                    onclick: move |_| {
                                        let path = crate::core::logger::get_log_file_path();
                                        let mut cmd = std::process::Command::new("explorer");
                                        #[cfg(windows)]
                                        {
                                            use std::os::windows::process::CommandExt;
                                            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                                        }
                                        let _ = cmd.arg(&path).spawn();
                                        crate::core::logger::info("ui", &format!("Opened log file: {}", path.display()));
                                    },
                                    "{crate::core::i18n::t(&current_lang.read(), \"btn_open_log\")}"
                                }
                                button {
                                    class: "ghost sm",
                                    id: "clearLog",
                                    onclick: move |_| activity_log.set(Vec::new()),
                                    "{crate::core::i18n::t(&current_lang.read(), \"btn_clear_log\")}"
                                }
                            }
                        }
                        div { class: "glass log", id: "log",
                            for line in activity_log.read().iter() {
                                {
                                    let l = line.as_str();
                                    if l.starts_with('[') && l.len() > 10 && l.chars().nth(9) == Some(']') {
                                        let time_part = &l[0..10];
                                        let msg_part = &l[10..];
                                        let rendered_msg = crate::core::i18n::format_log_entry(&current_lang.read(), msg_part);
                                        rsx! {
                                            div { class: "log-line",
                                                span { class: "log-time", "{time_part}" }
                                                span { class: "log-msg", "{rendered_msg}" }
                                            }
                                        }
                                    } else {
                                        let rendered_l = crate::core::i18n::format_log_entry(&current_lang.read(), l);
                                        rsx! {
                                            div { class: "log-line",
                                                span { class: "log-msg", "{rendered_l}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---------------- GAMES VIEW ----------------
                if *active_view.read() == "games" {
                    section { class: "view active", id: "view-games",
                        div {
                            class: "section-head games-heading",
                            style: "cursor: default; user-select: none;",
                            onmousedown: move |_| { dioxus::desktop::window().drag(); },
                            ondoubleclick: move |_| { dioxus::desktop::window().toggle_maximized(); },
                            h3 {
                                style: "cursor: default;",
                                span { id: "gamesTitle", "{crate::core::i18n::t(&current_lang.read(), \"nav_games\")}" }
                                span { class: "count", id: "gamesCount", " {visible_games.len()} / {games.read().len()}" }
                            }
                            div {
                                class: "row",
                                onmousedown: move |e| e.stop_propagation(),
                                button {
                                     class: "ghost sm",
                                     id: "addGame",
                                     onclick: move |_| {
                                         spawn(async move {
                                             if let Some(handle) = rfd::AsyncFileDialog::new().set_title("Add one game").pick_folder().await {
                                                 let folder = handle.path().to_path_buf();
                                                 let maybe_game = tokio::task::spawn_blocking({
                                                     let folder = folder.clone();
                                                     move || scan_game_directory(&folder)
                                                 }).await.unwrap_or(None);

                                                 if let Some(game) = maybe_game {
                                                     let mut s = load_state();
                                                     s.unhide_game(&game.dir);
                                                     let f_str = folder.to_string_lossy().to_string();
                                                     if !s.manual.contains(&f_str) {
                                                         s.manual.push(f_str);
                                                     }
                                                     let mut current_games = games.read().clone();
                                                     current_games.push(game.clone());
                                                     let deduped = dedupe_games(current_games);
                                                     let idx = deduped.iter().position(|g| g.dir == game.dir).unwrap_or(0);
                                                     s.cached_games = deduped.clone();
                                                     let _ = save_state(&s);
                                                     games.set(deduped);
                                                     sheet_game_idx.set(Some(idx));
                                                     sheet_open.set(true);
                                                 }
                                             }
                                         });
                                     },
                                     "{crate::core::i18n::t(&current_lang.read(), \"add_game\")}"
                                 }
                                 button {
                                     class: "ghost sm",
                                     id: "addFolder",
                                     onclick: move |_| {
                                         spawn(async move {
                                             if let Some(handle) = rfd::AsyncFileDialog::new().set_title("Scan this folder for games").pick_folder().await {
                                                 let folder = handle.path().to_path_buf();
                                                 status_text.set("Scanning selected folder for games...".to_string());
                                                  status_state.set(AppStatus::ScanningFolder(folder.file_name().unwrap_or_default().to_string_lossy().to_string()));
                                                 status_percent.set(50.0);
                                                 let folder_games = tokio::task::spawn_blocking({
                                                     let folder = folder.clone();
                                                     move || scan_library_root(&folder)
                                                 }).await.unwrap_or_default();

                                                 let mut s = load_state();
                                                 for g in &folder_games {
                                                     s.unhide_game(&g.dir);
                                                 }
                                                 let f_str = folder.to_string_lossy().to_string();
                                                 if !s.folders.contains(&f_str) {
                                                     s.folders.push(f_str);
                                                     managed_folders.set(s.folders.clone());
                                                 }
                                                 let mut current_games = games.read().clone();
                                                 current_games.extend(folder_games);
                                                 let deduped = dedupe_games(current_games);
                                                 s.cached_games = deduped.clone();
                                                 let _ = save_state(&s);
                                                 games.set(deduped);
                                                 status_text.set("Ready".to_string());
                                                 status_state.set(AppStatus::Ready);
                                                 status_percent.set(100.0);
                                             }
                                         });
                                     },
                                     "{crate::core::i18n::t(&current_lang.read(), \"add_folder\")}"
                                 }
                                 button {
                                     class: "glass-btn sm",
                                     id: "rescan",
                                     onclick: move |_| {
                                         spawn(async move {
                                             status_text.set("Scanning launchers and custom folders...".to_string());
                                                status_state.set(AppStatus::ScanningLaunchers);
                                             status_percent.set(30.0);
                                             let deduped = tokio::task::spawn_blocking(move || {
                                                 let mut discovered = discover_all_launchers();
                                                 let s = load_state();
                                                 for folder in &s.folders {
                                                     discovered.extend(scan_library_root(folder));
                                                 }
                                                 for manual in &s.manual {
                                                     if let Some(g) = scan_game_directory(manual) {
                                                         discovered.push(g);
                                                     }
                                                 }
                                                 if s.auto_scan_drives {
                                                     for root in discover_drive_roots(&s.excluded_roots) {
                                                         discovered.extend(scan_library_root(&root));
                                                     }
                                                 }
                                                 let mut g_list = dedupe_games(discovered);
                                                 g_list.retain(|g| !s.is_hidden(&g.dir));
                                                 g_list
                                             }).await.unwrap_or_default();

                                            status_percent.set(100.0);
                                            status_text.set(format!("Found {} games across sources", deduped.len()));
                                                status_state.set(AppStatus::FoundGames(deduped.len()));
                                            games.set(deduped.clone());

                                            let mut s = load_state();
                                            s.cached_games = deduped;
                                            let _ = save_state(&s);
                                        });
                                    },
                                    "{crate::core::i18n::t(&current_lang.read(), \"rescan\")}"
                                }
                            }
                        }

                        div {
                            class: "game-filters",
                            role: "search",
                            onmousedown: move |e| e.stop_propagation(),
                            label { class: "game-search",
                                span { "{crate::core::i18n::t(&current_lang.read(), \"search_games\")}" }
                                input {
                                    id: "gameSearch",
                                    type: "search",
                                    placeholder: "{crate::core::i18n::t(&current_lang.read(), \"search_games\")}",
                                    value: "{search_query}",
                                    oninput: move |e| search_query.set(e.value())
                                }
                            }
                            label {
                                span { "{crate::core::i18n::t(&current_lang.read(), \"rendering_api\")}" }
                                select {
                                    id: "gameApi",
                                    value: "{filter_api}",
                                    onchange: move |e| filter_api.set(e.value()),
                                    option { value: "all", "{crate::core::i18n::t(&current_lang.read(), \"filter_all_apis\")}" }
                                    option { value: "d3d12", "DirectX 12" }
                                    option { value: "d3d11", "DirectX 11" }
                                    option { value: "vulkan", "Vulkan" }
                                    option { value: "d3d9", "DirectX 9" }
                                    option { value: "d3d8", "DirectX 8" }
                                    option { value: "opengl", "OpenGL" }
                                }
                            }
                            label {
                                span { "{crate::core::i18n::t(&current_lang.read(), \"dlss_status\")}" }
                                select {
                                    id: "gameDlss",
                                    value: "{filter_dlss}",
                                    onchange: move |e| filter_dlss.set(e.value()),
                                    option { value: "all", "{crate::core::i18n::t(&current_lang.read(), \"filter_all_status\")}" }
                                    option { value: "ready", "{crate::core::i18n::t(&current_lang.read(), \"filter_ready_dlss5\")}" }
                                    option { value: "has_dlss", "{crate::core::i18n::t(&current_lang.read(), \"filter_has_dlss\")}" }
                                    option { value: "absent", "{crate::core::i18n::t(&current_lang.read(), \"filter_no_dlss\")}" }
                                    option { value: "installed", "{crate::core::i18n::t(&current_lang.read(), \"filter_dlss5_installed\")}" }
                                }
                            }
                            label {
                                span { "{crate::core::i18n::t(&current_lang.read(), \"nav_addons\")}" }
                                select {
                                    id: "gameAddon",
                                    value: "{filter_addon}",
                                    onchange: move |e| filter_addon.set(e.value()),
                                    option { value: "all", "{crate::core::i18n::t(&current_lang.read(), \"filter_all\")}" }
                                    option { value: "installed", "{crate::core::i18n::t(&current_lang.read(), \"filter_installed\")}" }
                                }
                            }
                            button {
                                class: "ghost sm",
                                id: "clearGameFilters",
                                onclick: move |_| {
                                    search_query.set(String::new());
                                    filter_api.set("all".to_string());
                                    filter_dlss.set("all".to_string());
                                    filter_addon.set("all".to_string());
                                },
                                "{crate::core::i18n::t(&current_lang.read(), \"clear_filters\")}"
                            }
                        }

                        div { id: "groups",
                            if visible_games.is_empty() {
                                if games.read().is_empty() {
                                    div { class: "glass games-empty", style: "padding: 40px; text-align: center; color: var(--dim);",
                                        h4 { style: "font-size: 16px; margin-bottom: 8px; color: var(--text);", "{crate::core::i18n::t(&current_lang.read(), \"empty_library\")}" }
                                        p { "{crate::core::i18n::t(&current_lang.read(), \"empty_hint\")}" }
                                    }
                                } else {
                                    div { class: "glass games-empty", style: "padding: 40px; text-align: center; color: var(--dim);",
                                        h4 { style: "font-size: 16px; margin-bottom: 8px; color: var(--text);", "{crate::core::i18n::t(&current_lang.read(), \"games_no_matches\")}" }
                                        p { "{crate::core::i18n::t(&current_lang.read(), \"games_no_matches_hint\")}" }
                                    }
                                }
                            } else if *group_games_by_store.read() {
                                for store in &store_order {
                                    {
                                        let store_matches: Vec<(usize, GameEntry)> = visible_games.iter()
                                            .filter(|(_, g)| &g.launcher == store)
                                            .cloned()
                                            .collect();

                                        if store_matches.is_empty() {
                                            rsx! {}
                                        } else {
                                            let ready_count = store_matches.iter().filter(|(_, g)| g.dlss_version.is_some() || g.api.contains("12")).count();
                                            let store_display = match store.as_str() {
                                                "Added by hand" => crate::core::i18n::t(&current_lang.read(), "store_manual"),
                                                "My folders" => crate::core::i18n::t(&current_lang.read(), "store_folders"),
                                                s => s,
                                            };
                                            rsx! {
                                                section { class: "group",
                                                    div { class: "group-head",
                                                        h4 { "{store_display}" }
                                                        span { class: "count", "{store_matches.len()}" }
                                                        button {
                                                            class: if *filter_dlss.read() == "ready" { "ready filter-chip active" } else { "ready filter-chip" },
                                                            onclick: move |_| {
                                                                if *filter_dlss.read() == "ready" {
                                                                    filter_dlss.set("all".to_string());
                                                                } else {
                                                                    filter_dlss.set("ready".to_string());
                                                                }
                                                            },
                                                            "{crate::core::i18n::t(&current_lang.read(), \"filter_ready_dlss5\")} ({ready_count})"
                                                        }
                                                    }
                                                    div { class: "grid",
                                                        for (orig_idx, game) in store_matches {
                                                            {
                                                                let idx = orig_idx;
                                                                let g_name = game.name.clone();
                                                                let g_api = game.api.clone();
                                                                let g_dir = game.dir.clone();
                                                                let has_dlss = game.dlss_version.is_some();
                                                                let dlss_label = if let Some(ref v) = game.dlss_version {
                                                                    crate::core::scan::short_version(v)
                                                                } else {
                                                                    crate::core::i18n::t(&current_lang.read(), "badge_no_dlss").to_string()
                                                                };
                                                                let g_opti = game.optiscaler_installed;
                                                                let has_addon = game.mfg_unlock_installed || game.has_backup;
                                                                let is_dx12 = game.api == "DirectX 12";
                                                                let poster_opt = game.poster.as_ref().map(|p| crate::core::steamart::normalize_art_uri(p));
                                                                rsx! {
                                                                    article {
                                                                        class: if is_dx12 { "card dx12" } else { "card" },
                                                                        tabindex: "0",
                                                                        onclick: move |_| {
                                                                            sheet_game_idx.set(Some(idx));
                                                                            sheet_open.set(true);
                                                                        },
                                                                        div { class: "tools",
                                                                            button {
                                                                                class: "tool cover-tool",
                                                                                title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_change_cover\")}",
                                                                                onclick: {
                                                                                    let dir = g_dir.clone();
                                                                                    move |e: MouseEvent| {
                                                                                        e.stop_propagation();
                                                                                        pick_and_set_cover(&dir, games);
                                                                                    }
                                                                                },
                                                                                svg {
                                                                                    view_box: "0 0 24 24",
                                                                                    style: "width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round;",
                                                                                    rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                                                                                    circle { cx: "8.5", cy: "8.5", r: "1.5" }
                                                                                    polyline { points: "21 15 16 10 5 21" }
                                                                                }
                                                                            }
                                                                            button {
                                                                                class: "tool",
                                                                                title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_open_explorer\")}",
                                                                                onclick: {
                                                                                    let dir = g_dir.clone();
                                                                                    move |e: MouseEvent| {
                                                                                        e.stop_propagation();
                                                                                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                                                                                    }
                                                                                },
                                                                                "📂"
                                                                            }
                                                                            button {
                                                                                class: "tool",
                                                                                title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_hide_game\")}",
                                                                                onclick: {
                                                                                    let dir = g_dir.clone();
                                                                                    let name = g_name.clone();
                                                                                    move |e: MouseEvent| {
                                                                                        e.stop_propagation();
                                                                                        let mut s = load_state();
                                                                                        s.hide_game(&dir);
                                                                                        let _ = save_state(&s);
                                                                                        let mut cur = games.read().clone();
                                                                                        cur.retain(|g| !s.is_hidden(&g.dir));
                                                                                        games.set(cur);
                                                                                        hidden_count.set(s.hidden.len());
                                                                                        crate::core::logger::info("library", &format!("Hidden game from library: {} ({})", name, dir.display()));
                                                                                    }
                                                                                },
                                                                                "✕"
                                                                            }
                                                                        }
                                                                        button {
                                                                            class: "card-center-play",
                                                                            title: "{crate::core::i18n::t_param(&current_lang.read(), \"tooltip_launch_game\", &g_name)}",
                                                                            onclick: {
                                                                                let g = game.clone();
                                                                                move |e: MouseEvent| {
                                                                                    e.stop_propagation();
                                                                                    launch_game(&g);
                                                                                }
                                                                            },
                                                                            svg {
                                                                                view_box: "0 0 24 24",
                                                                                polygon { points: "6,4 20,12 6,20" }
                                                                            }
                                                                            span { "{crate::core::i18n::t(&current_lang.read(), \"home_play\")}" }
                                                                        }
                                                                        span { class: if is_dx12 { "badge dx12" } else { "badge" }, "{g_api}" }
                                                                        div { class: "poster",
                                                                            if let Some(ref p_url) = poster_opt {
                                                                                img { src: "{p_url}", alt: "{g_name}" }
                                                                            } else {
                                                                                div { class: "placeholder", "{&g_name[0..2.min(g_name.len())]}" }
                                                                            }
                                                                            div { class: "status",
                                                                                span { class: if has_dlss { "dot-s on" } else { "dot-s" } }
                                                                                "{dlss_label}"
                                                                                span { class: if g_opti || has_addon { "dot-s on" } else { "dot-s" }, style: "margin-inline-start:8px" }
                                                                                if g_opti { "OptiScaler" } else { "add-on" }
                                                                            }
                                                                        }
                                                                        div { class: "name", "{g_name}" }
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
                            } else {
                                // Flat alphabetical sort
                                div { class: "grid",
                                    for (orig_idx, game) in visible_games.iter() {
                                        {
                                            let idx = *orig_idx;
                                            let g_name = game.name.clone();
                                            let g_api = game.api.clone();
                                            let g_dir = game.dir.clone();
                                            let has_dlss = game.dlss_version.is_some();
                                            let dlss_label = if let Some(ref v) = game.dlss_version {
                                                crate::core::scan::short_version(v)
                                            } else {
                                                crate::core::i18n::t(&current_lang.read(), "badge_no_dlss").to_string()
                                            };
                                            let g_opti = game.optiscaler_installed;
                                            let has_addon = game.mfg_unlock_installed || game.has_backup;
                                            let is_dx12 = game.api == "DirectX 12";
                                            let poster_opt = game.poster.as_ref().map(|p| crate::core::steamart::normalize_art_uri(p));
                                            rsx! {
                                                article {
                                                    class: if is_dx12 { "card dx12" } else { "card" },
                                                    tabindex: "0",
                                                    onclick: move |_| {
                                                        sheet_game_idx.set(Some(idx));
                                                        sheet_open.set(true);
                                                    },
                                                    div { class: "tools",
                                                        button {
                                                            class: "tool cover-tool",
                                                            title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_change_cover\")}",
                                                            onclick: {
                                                                let dir = g_dir.clone();
                                                                move |e: MouseEvent| {
                                                                    e.stop_propagation();
                                                                    pick_and_set_cover(&dir, games);
                                                                }
                                                            },
                                                            svg {
                                                                view_box: "0 0 24 24",
                                                                style: "width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round;",
                                                                rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                                                                circle { cx: "8.5", cy: "8.5", r: "1.5" }
                                                                polyline { points: "21 15 16 10 5 21" }
                                                            }
                                                        }
                                                        button {
                                                            class: "tool",
                                                            title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_open_explorer\")}",
                                                            onclick: {
                                                                let dir = g_dir.clone();
                                                                move |e: MouseEvent| {
                                                                    e.stop_propagation();
                                                                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                                                                }
                                                            },
                                                            "📂"
                                                        }
                                                        button {
                                                            class: "tool",
                                                            title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_hide_game\")}",
                                                            onclick: {
                                                                let dir = g_dir.clone();
                                                                let name = g_name.clone();
                                                                move |e: MouseEvent| {
                                                                    e.stop_propagation();
                                                                    let mut s = load_state();
                                                                    s.hide_game(&dir);
                                                                    let _ = save_state(&s);
                                                                    let mut cur = games.read().clone();
                                                                    cur.retain(|g| !s.is_hidden(&g.dir));
                                                                    games.set(cur);
                                                                    hidden_count.set(s.hidden.len());
                                                                    crate::core::logger::info("library", &format!("Hidden game from library: {} ({})", name, dir.display()));
                                                                }
                                                            },
                                                            "✕"
                                                        }
                                                    }
                                                    button {
                                                        class: "card-center-play",
                                                        title: "{crate::core::i18n::t_param(&current_lang.read(), \"tooltip_launch_game\", &g_name)}",
                                                        onclick: {
                                                            let g = game.clone();
                                                            move |e: MouseEvent| {
                                                                e.stop_propagation();
                                                                launch_game(&g);
                                                            }
                                                        },
                                                        svg {
                                                            view_box: "0 0 24 24",
                                                            polygon { points: "6,4 20,12 6,20" }
                                                        }
                                                        span { "{crate::core::i18n::t(&current_lang.read(), \"home_play\")}" }
                                                    }
                                                    span { class: if is_dx12 { "badge dx12" } else { "badge" }, "{g_api}" }
                                                    div { class: "poster",
                                                        if let Some(ref p_url) = poster_opt {
                                                            img { src: "{p_url}", alt: "{g_name}" }
                                                        } else {
                                                            div { class: "placeholder", "{&g_name[0..2.min(g_name.len())]}" }
                                                        }
                                                        div { class: "status",
                                                            span { class: if has_dlss { "dot-s on" } else { "dot-s" } }
                                                            "{dlss_label}"
                                                            span { class: if g_opti || has_addon { "dot-s on" } else { "dot-s" }, style: "margin-inline-start:8px" }
                                                            if g_opti { "OptiScaler" } else { "add-on" }
                                                        }
                                                    }
                                                    div { class: "name", "{g_name}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---------------- ADD-ONS VIEW ----------------
                if *active_view.read() == "addons" {
                    section { class: "view active", id: "view-addons",
                        div { class: "section-head",
                            h3 { "{crate::core::i18n::t(&current_lang.read(), \"nav_addons\")}" }
                            button {
                                class: "ghost sm",
                                id: "addonAdd",
                                onclick: move |_| {
                                    spawn(async move {
                                        if let Some(handle) = rfd::AsyncFileDialog::new()
                                            .set_title("Add an add-on build")
                                            .add_filter("ReShade add-on", &["addon64", "addon"])
                                            .pick_file()
                                            .await
                                        {
                                            let file = handle.path().to_path_buf();
                                            let fname = file.file_name().and_then(|n| n.to_str()).unwrap_or("addon").to_string();
                                            let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
                                            let mb_size = format!("{:.2} MB", size as f64 / (1024.0 * 1024.0));
                                            let pe_ver = crate::core::pe::inspect_pe(&file).and_then(|p| p.version);
                                            let info_str = if let Some(ref v) = pe_ver {
                                                format!("{} · {} · {}", fname, v, mb_size)
                                            } else {
                                                format!("{} · {}", fname, mb_size)
                                            };
                                            let suggested_name = file.file_stem().and_then(|s| s.to_str()).unwrap_or(&fname).to_string();

                                            dlg_path.set(file.to_string_lossy().to_string());
                                            dlg_file_info.set(info_str);
                                            dlg_name.set(suggested_name);
                                            dlg_desc.set(String::new());
                                            dlg_tag.set(String::new());
                                            show_addon_dlg.set(true);
                                        }
                                    });
                                },
                                "{crate::core::i18n::t(&current_lang.read(), \"btn_add\")}"
                            }
                        }
                        p { class: "hint", "{crate::core::i18n::t(&current_lang.read(), \"addons_hint\")}" }
                        if let Some(ref status_msg) = *component_download_status.read() {
                            div { style: "margin-bottom: 16px; padding: 12px 16px; display: flex; flex-direction: column; gap: 8px; border: 1px solid rgba(249, 115, 22, 0.4); border-radius: 8px; background: rgba(249, 115, 22, 0.12);",
                                div { style: "display: flex; justify-content: space-between; align-items: center;",
                                    div { style: "font-size: 0.88rem; color: var(--text-primary); font-weight: 500;", "⚡ {status_msg}" }
                                    if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                        div { style: "font-size: 0.82rem; color: var(--accent); font-weight: 600;", "{status_percent.read():.0}%" }
                                    }
                                }
                                if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                    div { class: "bar", style: "margin-top: 0; height: 4px;",
                                        i { style: "width: {status_percent.read()}%;" }
                                    }
                                }
                            }
                        }
                        div { class: "addons", id: "addonList",
                            // 1. Mandatory Core: RenoDX v4.7
                            div { class: if crate::core::downloader::is_renodx_engine_cached() { "addon on" } else { "addon" },
                                div { class: "mark",
                                    if crate::core::downloader::is_renodx_engine_cached() {
                                        svg { style: "width:18px; height:18px; fill:currentColor;", view_box: "0 0 24 24",
                                            path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                        }
                                    }
                                }
                                div { class: "body",
                                    div { class: "t",
                                        "RenoDX v4.7 (Full ReShade Add-on Engine)"
                                        span { class: "tag accent", "{crate::core::i18n::t(&current_lang.read(), \"tag_mandatory_core\")}" }
                                    }
                                    div { class: "d", "renodx-dlss5.addon64 · {crate::core::i18n::t(&current_lang.read(), \"meta_sha256_verified\")} · 1.65 MB" }
                                    div { class: "dim", style: "font-size:0.75em; margin-top:3px; opacity:0.75; font-family:monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"label_source\")} {crate::core::downloader::RENODX_DLSS5_URL}"
                                    }
                                    div { class: "dim", style: "font-size:0.82em; margin-top:4px;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"addon_renodx_desc\")}"
                                    }
                                }
                                div { class: "addon-status-core", style: "display:flex; align-items:center; gap:8px;",
                                    if crate::core::downloader::is_renodx_engine_cached() {
                                        span { class: "tag accent", style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px;",
                                            "✓ {crate::core::i18n::t(&current_lang.read(), \"addon_core_badge\")}"
                                        }
                                    } else {
                                        button {
                                            class: "tag warn",
                                            style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px; cursor: pointer; border: none; background: rgba(245, 158, 11, 0.2); color: #f59e0b;",
                                            title: crate::core::i18n::t(&current_lang.read(), "tooltip_retry_download"),
                                            onclick: move |_| {
                                                *trigger_component_download.write() += 1;
                                            },
                                            if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                                "{status_percent.read():.0}% ..."
                                            } else {
                                                "⚡ {crate::core::i18n::t(&current_lang.read(), \"btn_retry_download\")}"
                                            }
                                        }
                                    }
                                }
                            }

                            // 2. Mandatory Core: RenoDX 4x MFG Unlock v0.9
                            div { class: if crate::core::downloader::is_mfg_addon_cached() { "addon on" } else { "addon" },
                                div { class: "mark",
                                    if crate::core::downloader::is_mfg_addon_cached() {
                                        svg { style: "width:18px; height:18px; fill:currentColor;", view_box: "0 0 24 24",
                                            path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                        }
                                    }
                                }
                                div { class: "body",
                                    div { class: "t",
                                        "RenoDX 4x MFG Unlock v0.9"
                                        span { class: "tag warn", "{crate::core::i18n::t(&current_lang.read(), \"tag_rtx40\")}" }
                                    }
                                    div { class: "d", "renodx-mfgunlock.addon64 · v0.9 · {crate::core::i18n::t(&current_lang.read(), \"meta_sha256_verified\")} · 528 KB" }
                                    div { class: "dim", style: "font-size:0.75em; margin-top:3px; opacity:0.75; font-family:monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"label_source\")} {crate::core::downloader::MFG_09_URL}"
                                    }
                                    div { class: "dim", style: "font-size:0.82em; margin-top:4px;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"addon_mfg_desc\")}"
                                    }
                                }
                                div { class: "addon-status-core", style: "display:flex; align-items:center; gap:8px;",
                                    if crate::core::downloader::is_mfg_addon_cached() {
                                        span { class: "tag accent", style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px;",
                                            "✓ {crate::core::i18n::t(&current_lang.read(), \"addon_core_badge\")}"
                                        }
                                    } else {
                                        button {
                                            class: "tag warn",
                                            style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px; cursor: pointer; border: none; background: rgba(245, 158, 11, 0.2); color: #f59e0b;",
                                            title: crate::core::i18n::t(&current_lang.read(), "tooltip_retry_download"),
                                            onclick: move |_| {
                                                *trigger_component_download.write() += 1;
                                            },
                                            if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                                "{status_percent.read():.0}% ..."
                                            } else {
                                                "⚡ {crate::core::i18n::t(&current_lang.read(), \"btn_retry_download\")}"
                                            }
                                        }
                                    }
                                }
                            }

                            // 3. DLSS 5 Feeder & Motion Shaders
                            div { class: if crate::core::downloader::is_feeder_cached() { "addon on" } else { "addon" },
                                div { class: "mark",
                                    if crate::core::downloader::is_feeder_cached() {
                                        svg { style: "width:18px; height:18px; fill:currentColor;", view_box: "0 0 24 24",
                                            path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                        }
                                    }
                                }
                                div { class: "body",
                                    div { class: "t",
                                        "DLSS 5 Feeder (Neural Pipeline Interceptor)"
                                        span { class: "tag", "{crate::core::i18n::t(&current_lang.read(), \"tag_universal_intercept\")}" }
                                    }
                                    div { class: "d", "dlss5-feed.addon64 + DLSS5_Feed.fx + vort_Motion.fx · {crate::core::i18n::t(&current_lang.read(), \"meta_sha256_verified\")} · 4.8 MB" }
                                    div { class: "dim", style: "font-size:0.75em; margin-top:3px; opacity:0.75; font-family:monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"label_source\")} {crate::core::downloader::FEEDER_ARCHIVE_URL}"
                                    }
                                    div { class: "dim", style: "font-size:0.82em; margin-top:4px;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"addon_feeder_desc\")}"
                                    }
                                }
                                div { class: "addon-status-core", style: "display:flex; align-items:center; gap:8px;",
                                    if crate::core::downloader::is_feeder_cached() {
                                        span { class: "tag accent", style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px;",
                                            "✓ {crate::core::i18n::t(&current_lang.read(), \"addon_core_badge\")}"
                                        }
                                    } else {
                                        button {
                                            class: "tag warn",
                                            style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px; cursor: pointer; border: none; background: rgba(245, 158, 11, 0.2); color: #f59e0b;",
                                            title: crate::core::i18n::t(&current_lang.read(), "tooltip_retry_download"),
                                            onclick: move |_| {
                                                *trigger_component_download.write() += 1;
                                            },
                                            if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                                "{status_percent.read():.0}% ..."
                                            } else {
                                                "⚡ {crate::core::i18n::t(&current_lang.read(), \"btn_retry_download\")}"
                                            }
                                        }
                                    }
                                }
                            }

                            // 4. Mandatory Core: OptiScaler DLSS-NR
                            div { class: if crate::core::downloader::is_optiscaler_cached() { "addon on" } else { "addon" },
                                div { class: "mark",
                                    if crate::core::downloader::is_optiscaler_cached() {
                                        svg { style: "width:18px; height:18px; fill:currentColor;", view_box: "0 0 24 24",
                                            path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                        }
                                    }
                                }
                                div { class: "body",
                                    div { class: "t",
                                        "OptiScaler DLSS-NR (Neural Reconstruction & Pre-SR)"
                                        span { class: "tag", "{crate::core::i18n::t(&current_lang.read(), \"tag_neural_reconstruction\")}" }
                                    }
                                    div { class: "d", "OptiScaler.dll + nvngx.ini · v0.7.8 · {crate::core::i18n::t(&current_lang.read(), \"meta_native_backend\")} · 18.4 MB" }
                                    div { class: "dim", style: "font-size:0.75em; margin-top:3px; opacity:0.75; font-family:monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"label_source\")} {crate::core::downloader::OPTISCALER_083_URL}"
                                    }
                                    div { class: "dim", style: "font-size:0.82em; margin-top:4px;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"addon_optiscaler_desc\")}"
                                    }
                                }
                                div { class: "addon-status-core", style: "display:flex; align-items:center; gap:8px;",
                                    if crate::core::downloader::is_optiscaler_cached() {
                                        span { class: "tag accent", style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px;",
                                            "✓ {crate::core::i18n::t(&current_lang.read(), \"addon_core_badge\")}"
                                        }
                                    } else {
                                        button {
                                            class: "tag warn",
                                            style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px; cursor: pointer; border: none; background: rgba(245, 158, 11, 0.2); color: #f59e0b;",
                                            title: crate::core::i18n::t(&current_lang.read(), "tooltip_retry_download"),
                                            onclick: move |_| {
                                                *trigger_component_download.write() += 1;
                                            },
                                            if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                                "{status_percent.read():.0}% ..."
                                            } else {
                                                "⚡ {crate::core::i18n::t(&current_lang.read(), \"btn_retry_download\")}"
                                            }
                                        }
                                    }
                                }
                            }

                            // 5. Mandatory Core: ReShade 6.8.0 Add-on Runtime
                            div { class: if crate::core::downloader::is_reshade_cached() { "addon on" } else { "addon" },
                                div { class: "mark",
                                    if crate::core::downloader::is_reshade_cached() {
                                        svg { style: "width:18px; height:18px; fill:currentColor;", view_box: "0 0 24 24",
                                            path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                        }
                                    }
                                }
                                div { class: "body",
                                    div { class: "t",
                                        "ReShade 6.8.0 (Add-on Host Runtime)"
                                        span { class: "tag", "{crate::core::i18n::t(&current_lang.read(), \"tag_graphics_hook\")}" }
                                    }
                                    div { class: "d", "ReShade64.dll · v6.8.0 · {crate::core::i18n::t(&current_lang.read(), \"meta_full_addon_support\")} · 5.3 MB" }
                                    div { class: "dim", style: "font-size:0.75em; margin-top:3px; opacity:0.75; font-family:monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"label_source\")} {crate::core::downloader::RESHADE_SETUP_URL}"
                                    }
                                    div { class: "dim", style: "font-size:0.82em; margin-top:4px;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"addon_reshade_desc\")}"
                                    }
                                }
                                div { class: "addon-status-core", style: "display:flex; align-items:center; gap:8px;",
                                    if crate::core::downloader::is_reshade_cached() {
                                        span { class: "tag accent", style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px;",
                                            "✓ {crate::core::i18n::t(&current_lang.read(), \"addon_core_badge\")}"
                                        }
                                    } else {
                                        button {
                                            class: "tag warn",
                                            style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px; cursor: pointer; border: none; background: rgba(245, 158, 11, 0.2); color: #f59e0b;",
                                            title: crate::core::i18n::t(&current_lang.read(), "tooltip_retry_download"),
                                            onclick: move |_| {
                                                *trigger_component_download.write() += 1;
                                            },
                                            if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                                "{status_percent.read():.0}% ..."
                                            } else {
                                                "⚡ {crate::core::i18n::t(&current_lang.read(), \"btn_retry_download\")}"
                                            }
                                        }
                                    }
                                }
                            }

                            // 6. Mandatory Core: NVIDIA Streamline Runtime v2.14.1
                            div { class: if crate::core::downloader::is_streamline_cached() { "addon on" } else { "addon" },
                                div { class: "mark",
                                    if crate::core::downloader::is_streamline_cached() {
                                        svg { style: "width:18px; height:18px; fill:currentColor;", view_box: "0 0 24 24",
                                            path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                        }
                                    }
                                }
                                div { class: "body",
                                    div { class: "t",
                                        "NVIDIA Streamline Runtime v2.14.1"
                                        span { class: "tag", "{crate::core::i18n::t(&current_lang.read(), \"tag_interposer\")}" }
                                    }
                                    div { class: "d", "sl.interposer.dll · v2.14.1 · {crate::core::i18n::t(&current_lang.read(), \"meta_streamline_interposer\")} · 1.2 MB" }
                                    div { class: "dim", style: "font-size:0.75em; margin-top:3px; opacity:0.75; font-family:monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"label_source\")} {crate::core::downloader::STREAMLINE_ZIP_URL}"
                                    }
                                    div { class: "dim", style: "font-size:0.82em; margin-top:4px;",
                                        "{crate::core::i18n::t(&current_lang.read(), \"addon_streamline_desc\")}"
                                    }
                                }
                                div { class: "addon-status-core", style: "display:flex; align-items:center; gap:8px;",
                                    if crate::core::downloader::is_streamline_cached() {
                                        span { class: "tag accent", style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px;",
                                            "✓ {crate::core::i18n::t(&current_lang.read(), \"addon_core_badge\")}"
                                        }
                                    } else {
                                        button {
                                            class: "tag warn",
                                            style: "font-weight:600; font-size:0.75rem; padding: 4px 10px; border-radius: 6px; letter-spacing: 0.5px; cursor: pointer; border: none; background: rgba(245, 158, 11, 0.2); color: #f59e0b;",
                                            title: crate::core::i18n::t(&current_lang.read(), "tooltip_retry_download"),
                                            onclick: move |_| {
                                                *trigger_component_download.write() += 1;
                                            },
                                            if matches!(*status_state.read(), AppStatus::DownloadingComponents) {
                                                "{status_percent.read():.0}% ..."
                                            } else {
                                                "⚡ {crate::core::i18n::t(&current_lang.read(), \"btn_retry_download\")}"
                                            }
                                        }
                                    }
                                }
                            }

                            // 4. Custom user-added add-ons
                            for custom in custom_addons.read().clone().into_iter() {
                                {
                                    let path_str = custom.path.clone();
                                    let is_act = addons_active.read().contains(&path_str);
                                    let name_display = custom.name.clone().unwrap_or_else(|| {
                                        std::path::Path::new(&path_str).file_name().and_then(|n| n.to_str()).unwrap_or("Custom Add-on").to_string()
                                    });
                                    let fname = std::path::Path::new(&path_str).file_name().and_then(|n| n.to_str()).unwrap_or("addon").to_string();
                                    let tag_display = custom.tag.clone();
                                    let desc_display = custom.description.clone();
                                    rsx! {
                                        div { key: "{path_str}", class: if is_act { "addon on" } else { "addon" },
                                            div { class: "mark",
                                                if is_act {
                                                    svg { style: "width:18px; height:18px; fill:currentColor;", view_box: "0 0 24 24",
                                                        path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                                    }
                                                }
                                            }
                                            div { class: "body",
                                                div { class: "t",
                                                    "{name_display}"
                                                    if let Some(ref t) = tag_display {
                                                        span { class: "tag", "{t}" }
                                                    } else {
                                                        span { class: "tag", "{crate::core::i18n::t(&current_lang.read(), \"tag_custom\")}" }
                                                    }
                                                }
                                                div { class: "d", "{fname} · {path_str}" }
                                                if let Some(ref d) = desc_display {
                                                    div { class: "dim", style: "font-size:0.82em; margin-top:4px;", "{d}" }
                                                }
                                            }
                                            button {
                                                class: "toggle",
                                                role: "switch",
                                                aria_checked: if is_act { "true" } else { "false" },
                                                title: if is_act { crate::core::i18n::t(&current_lang.read(), "tooltip_deactivate_addon") } else { crate::core::i18n::t(&current_lang.read(), "tooltip_activate_addon") },
                                                onclick: {
                                                    let p = path_str.clone();
                                                    move |_| {
                                                        let mut cur = addons_active.read().clone();
                                                        let was_on = cur.contains(&p);
                                                        if was_on {
                                                            cur.retain(|x| x != &p);
                                                        } else {
                                                            cur.push(p.clone());
                                                        }
                                                        addons_active.set(cur.clone());
                                                        let mut s = crate::core::state::load_state();
                                                        s.addons = cur;
                                                        let _ = crate::core::state::save_state(&s);
                                                        log_message(&format!("@{{log_addon_status|{}|@{}}}", p, if was_on { "log_addon_deactivated" } else { "log_addon_activated" }));
                                                        activity_log.set(get_session_log());
                                                    }
                                                },
                                                span { class: "knob" }
                                            }
                                            button {
                                                class: "drop",
                                                title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_remove_addon\")}",
                                                onclick: {
                                                    let p = path_str.clone();
                                                    move |_| {
                                                        let mut list = custom_addons.read().clone();
                                                        list.retain(|x| x.path != p);
                                                        custom_addons.set(list.clone());
                                                        let mut cur_act = addons_active.read().clone();
                                                        cur_act.retain(|x| x != &p);
                                                        addons_active.set(cur_act.clone());
                                                        let mut s = crate::core::state::load_state();
                                                        s.addon_files = list;
                                                        s.addons = cur_act;
                                                        let _ = crate::core::state::save_state(&s);
                                                        copy_toast_text.set(crate::core::i18n::t(&current_lang.read(), "toast_addon_removed").to_string());
                                                        copy_toast.set(true);
                                                    }
                                                },
                                                svg { style: "width:15px; height:15px; fill:currentColor;", view_box: "0 0 24 24",
                                                    path { d: "M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Add-on Dialog Modal
                        if *show_addon_dlg.read() {
                            div { class: "overlay", id: "dlgOverlay",
                                onclick: move |_| show_addon_dlg.set(false),
                                div {
                                    class: "dialog",
                                    onclick: move |e| e.stop_propagation(),
                                    h3 { "{crate::core::i18n::t(&current_lang.read(), \"dlg_add_addon_title\")}" }
                                    div { class: "dlg-file", id: "dlgFile", "{dlg_file_info.read()}" }
                                    label { class: "dlg-label", "{crate::core::i18n::t(&current_lang.read(), \"dlg_field_name\")}" }
                                    input {
                                        class: "dlg-input",
                                        id: "dlgName",
                                        r#type: "text",
                                        value: "{dlg_name.read()}",
                                        oninput: move |e| dlg_name.set(e.value())
                                    }
                                    label { class: "dlg-label", "{crate::core::i18n::t(&current_lang.read(), \"dlg_field_desc\")}" }
                                    textarea {
                                        class: "dlg-input",
                                        id: "dlgDesc",
                                        rows: "3",
                                        value: "{dlg_desc.read()}",
                                        oninput: move |e| dlg_desc.set(e.value())
                                    }
                                    div { class: "dlg-hint", "{crate::core::i18n::t(&current_lang.read(), \"dlg_hint_notes\")}" }
                                    label { class: "dlg-label", "{crate::core::i18n::t(&current_lang.read(), \"dlg_field_tag\")}" }
                                    input {
                                        class: "dlg-input",
                                        id: "dlgTag",
                                        r#type: "text",
                                        placeholder: "{crate::core::i18n::t(&current_lang.read(), \"dlg_placeholder_tag\")}",
                                        value: "{dlg_tag.read()}",
                                        oninput: move |e| dlg_tag.set(e.value())
                                    }
                                    div { class: "dlg-actions",
                                        button {
                                            class: "ghost sm",
                                            id: "dlgCancel",
                                            onclick: move |_| show_addon_dlg.set(false),
                                            "{crate::core::i18n::t(&current_lang.read(), \"btn_cancel\")}"
                                        }
                                        button {
                                            class: "primary sm",
                                            id: "dlgSave",
                                            onclick: move |_| {
                                                let p_val = dlg_path.read().clone();
                                                let n_val = dlg_name.read().trim().to_string();
                                                let d_val = dlg_desc.read().trim().to_string();
                                                let t_val = dlg_tag.read().trim().to_string();

                                                let entry = crate::core::state::AddonFileEntry {
                                                    path: p_val.clone(),
                                                    name: if n_val.is_empty() { None } else { Some(n_val) },
                                                    description: if d_val.is_empty() { None } else { Some(d_val) },
                                                    tag: if t_val.is_empty() { None } else { Some(t_val) },
                                                };

                                                let mut list = custom_addons.read().clone();
                                                list.retain(|x| x.path != p_val);
                                                list.push(entry);
                                                custom_addons.set(list.clone());

                                                let mut cur_act = addons_active.read().clone();
                                                if !cur_act.contains(&p_val) {
                                                    cur_act.push(p_val.clone());
                                                }
                                                addons_active.set(cur_act.clone());

                                                let mut s = crate::core::state::load_state();
                                                s.addon_files = list;
                                                s.addons = cur_act;
                                                let _ = crate::core::state::save_state(&s);

                                                show_addon_dlg.set(false);
                                                copy_toast_text.set(crate::core::i18n::t(&current_lang.read(), "toast_addon_registered").to_string());
                                                copy_toast.set(true);
                                                log_message(&format!("@{{log_addon_imported|{}}}", p_val));
                                                activity_log.set(get_session_log());
                                            },
                                            "{crate::core::i18n::t(&current_lang.read(), \"btn_save\")}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---------------- HISTORY VIEW ----------------
                if *active_view.read() == "history" {
                    section { class: "view active", id: "view-history",
                        div { class: "section-head",
                            h3 { "{crate::core::i18n::t(&current_lang.read(), \"nav_history\")}" }
                            button {
                                class: "ghost sm",
                                id: "copyHistory",
                                onclick: move |_| {
                                    let g_list = games.read().clone();
                                    let header_date = crate::core::i18n::t(&current_lang.read(), "col_date");
                                    let header_game = crate::core::i18n::t(&current_lang.read(), "col_game");
                                    let header_action = crate::core::i18n::t(&current_lang.read(), "col_action");
                                    let header_files = crate::core::i18n::t(&current_lang.read(), "col_replaced");
                                    let header_path = crate::core::i18n::t(&current_lang.read(), "col_path");
                                    let mut out = format!("{}\t{}\t{}\t{}\t{}\r\n", header_date, header_game, header_action, header_files, header_path);
                                    for r in history_rows.read().iter().rev() {
                                        let title = resolve_game_title(r, &g_list);
                                        let changes = if r.action == "restore" {
                                            crate::core::i18n::t(&current_lang.read(), "history_orig_restored").to_string()
                                        } else if r.action == "clean" {
                                            crate::core::i18n::t(&current_lang.read(), "history_mods_cleaned").to_string()
                                        } else {
                                            crate::core::i18n::t(&current_lang.read(), "history_changes_format")
                                                .replace("{replaced}", &r.replaced.to_string())
                                                .replace("{added}", &r.added.to_string())
                                        };
                                        let action_str = match r.action.as_str() {
                                            "install_native" => crate::core::i18n::t(&current_lang.read(), "history_action_install_native"),
                                            "install_feeder" => crate::core::i18n::t(&current_lang.read(), "history_action_install_feeder"),
                                            "install_optiscaler" => crate::core::i18n::t(&current_lang.read(), "history_action_install_optiscaler"),
                                            "install" => crate::core::i18n::t(&current_lang.read(), "history_action_install"),
                                            "restore" => crate::core::i18n::t(&current_lang.read(), "history_action_restore"),
                                            "clean" => crate::core::i18n::t(&current_lang.read(), "history_action_clean"),
                                            _ => r.action.as_str(),
                                        };
                                        out.push_str(&format!("{}\t{}\t{}\t{}\t{}\r\n", r.date, title, action_str, changes, r.dir));
                                    }
                                    copy_to_clipboard(&out);
                                    copy_toast_text.set(crate::core::i18n::t(&current_lang.read(), "toast_history_copied").to_string());
                                    copy_toast.set(true);
                                },
                                "{crate::core::i18n::t(&current_lang.read(), \"btn_copy_history\")}"
                            }
                        }
                        div { class: "glass pad", id: "history",
                            if history_rows.read().is_empty() {
                                p { class: "dim", "{crate::core::i18n::t(&current_lang.read(), \"history_empty\")}" }
                            } else {
                                div { class: "history-table-container",
                                    table { class: "history-table",
                                        thead {
                                            tr {
                                                th { class: "col-date", "{crate::core::i18n::t(&current_lang.read(), \"col_date\")}" }
                                                th { class: "col-game", "{crate::core::i18n::t(&current_lang.read(), \"col_game\")}" }
                                                th { class: "col-action", "{crate::core::i18n::t(&current_lang.read(), \"col_action\")}" }
                                                th { class: "col-changes", "{crate::core::i18n::t(&current_lang.read(), \"col_replaced\")}" }
                                                th { class: "col-path", "{crate::core::i18n::t(&current_lang.read(), \"col_path\")}" }
                                            }
                                        }
                                        tbody {
                                            for row in history_rows.read().iter().rev() {
                                                {
                                                    let title = resolve_game_title(row, &games.read());
                                                    let is_install = row.action.starts_with("install");
                                                    let r_date = if row.date.starts_with("SystemTime") {
                                                        crate::core::i18n::t(&current_lang.read(), "history_date_recently")
                                                    } else {
                                                        row.date.as_str()
                                                    };
                                                    let action_display = match row.action.as_str() {
                                                        "install_native" => crate::core::i18n::t(&current_lang.read(), "history_action_install_native"),
                                                        "install_feeder" => crate::core::i18n::t(&current_lang.read(), "history_action_install_feeder"),
                                                        "install_optiscaler" => crate::core::i18n::t(&current_lang.read(), "history_action_install_optiscaler"),
                                                        "install" => crate::core::i18n::t(&current_lang.read(), "history_action_install"),
                                                        "restore" => crate::core::i18n::t(&current_lang.read(), "history_action_restore"),
                                                        "clean" => crate::core::i18n::t(&current_lang.read(), "history_action_clean"),
                                                        _ => row.action.as_str(),
                                                    };
                                                    let changes_label = if row.action == "restore" {
                                                        crate::core::i18n::t(&current_lang.read(), "history_orig_restored").to_string()
                                                    } else if row.action == "clean" {
                                                        crate::core::i18n::t(&current_lang.read(), "history_mods_cleaned").to_string()
                                                    } else {
                                                        crate::core::i18n::t(&current_lang.read(), "history_changes_format")
                                                            .replace("{replaced}", &row.replaced.to_string())
                                                            .replace("{added}", &row.added.to_string())
                                                    };
                                                    rsx! {
                                                        tr {
                                                            td { class: "col-date", "{r_date}" }
                                                            td { class: "col-game", title: "{title}", "{title}" }
                                                            td { class: "col-action",
                                                                span { class: if is_install { "badge dx12" } else { "badge" }, "{action_display}" }
                                                            }
                                                            td { class: "col-changes", "{changes_label}" }
                                                            td { class: "col-path", title: "{row.dir}", "{row.dir}" }
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
                }

                // ---------------- SETTINGS VIEW ----------------
                if *active_view.read() == "settings" {
                    section { class: "view active", id: "view-settings",
                        div { class: "section-head",
                            h3 { "{crate::core::i18n::t(&current_lang.read(), \"nav_settings\")}" }
                        }
                        div { class: "glass pad", id: "settings",
                            // Group games by store
                            div { class: "set-row", style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                div {
                                    div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"settings_scanners\")}" }
                                    div { class: "v", style: "font-size:0.85em; color:var(--dim);", "{crate::core::i18n::t(&current_lang.read(), \"settings_scanners_desc\")}" }
                                }
                                button {
                                    class: "setting-switch",
                                    id: "setGroupGames",
                                    type: "button",
                                    role: "switch",
                                    "aria-checked": if *group_games_by_store.read() { "true" } else { "false" },
                                    onclick: move |_| {
                                        let next = !*group_games_by_store.read();
                                        group_games_by_store.set(next);
                                        let mut s = load_state();
                                        s.group_games_by_store = next;
                                        let _ = save_state(&s);
                                    },
                                    span { class: "knob" }
                                }
                            }

                            // Auto scan fixed drives
                            div { class: "set-row", style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                div {
                                    div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"settings_autoscan\")}" }
                                    div { class: "v", style: "font-size:0.85em; color:var(--dim);", "{crate::core::i18n::t(&current_lang.read(), \"settings_autoscan_desc\")}" }
                                }
                                button {
                                    class: "setting-switch",
                                    id: "setAutoScan",
                                    type: "button",
                                    role: "switch",
                                    "aria-checked": if *auto_scan_drives.read() { "true" } else { "false" },
                                    onclick: move |_| {
                                        let next = !*auto_scan_drives.read();
                                        auto_scan_drives.set(next);
                                        let mut s = load_state();
                                        s.auto_scan_drives = next;
                                        let _ = save_state(&s);
                                    },
                                    span { class: "knob" }
                                }
                            }

                            // Enable Rust Theme
                            div { class: "set-row", style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                div {
                                    div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"rust_theme_label\")}" }
                                    div { class: "v", style: "font-size:0.85em; color:var(--dim);", "{crate::core::i18n::t(&current_lang.read(), \"rust_theme_desc\")}" }
                                }
                                button {
                                    class: "setting-switch",
                                    id: "setRustTheme",
                                    type: "button",
                                    role: "switch",
                                    "aria-checked": if *rust_theme.read() { "true" } else { "false" },
                                    onclick: move |_| {
                                        let next = !*rust_theme.read();
                                        rust_theme.set(next);
                                        let mut s = load_state();
                                        s.rust_theme = next;
                                        let _ = save_state(&s);
                                        let script = format!("document.documentElement.setAttribute('data-rust-theme', '{}');", if next { "true" } else { "false" });
                                        let _ = dioxus::desktop::window().webview.evaluate_script(&script);
                                    },
                                    span { class: "knob" }
                                }
                            }

                            // Run in background
                            div { class: "set-row", style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                div {
                                    div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"tray_minimize_label\")}" }
                                    div { class: "v", style: "font-size:0.85em; color:var(--dim);", "{crate::core::i18n::t(&current_lang.read(), \"tray_minimize_desc\")}" }
                                }
                                button {
                                    class: "setting-switch",
                                    id: "setRunInBg",
                                    type: "button",
                                    role: "switch",
                                    "aria-checked": if *run_in_bg.read() { "true" } else { "false" },
                                    onclick: move |_| {
                                        let next = !*run_in_bg.read();
                                        run_in_bg.set(next);
                                        let mut s = load_state();
                                        s.run_in_background = next;
                                        let _ = save_state(&s);
                                    },
                                    span { class: "knob" }
                                }
                            }

                            // Launch on startup
                            div { class: "set-row", style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                div {
                                    div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"startup_boot_label\")}" }
                                    div { class: "v", style: "font-size:0.85em; color:var(--dim);", "{crate::core::i18n::t(&current_lang.read(), \"startup_boot_desc\")}" }
                                }
                                button {
                                    class: "setting-switch",
                                    id: "setStartup",
                                    type: "button",
                                    role: "switch",
                                    "aria-checked": if *launch_at_startup.read() { "true" } else { "false" },
                                    onclick: move |_| {
                                        let next = !*launch_at_startup.read();
                                        let _ = crate::core::tray::set_startup_enabled(next);
                                        launch_at_startup.set(crate::core::tray::is_startup_enabled());
                                    },
                                    span { class: "knob" }
                                }
                            }

                            // Managed folders list
                            div { class: "set-row", style: "padding:12px 0; border-bottom:1px solid var(--line);",
                                div { style: "display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;",
                                    div {
                                        div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"settings_managed_folders\")}" }
                                        div { class: "v", style: "font-size:0.85em; color:var(--dim);", "{crate::core::i18n::t(&current_lang.read(), \"settings_managed_folders_desc\")}" }
                                    }
                                    button {
                                        class: "ghost sm",
                                        id: "setAddFolder",
                                        title: "{crate::core::i18n::t(&current_lang.read(), \"settings_add_folder\")}",
                                        onclick: move |_| {
                                            spawn(async move {
                                                if let Some(handle) = rfd::AsyncFileDialog::new().set_title("Scan this folder for games").pick_folder().await {
                                                    let folder = handle.path().to_path_buf();
                                                    let folder_games = tokio::task::spawn_blocking({
                                                        let folder = folder.clone();
                                                        move || scan_library_root(&folder)
                                                    }).await.unwrap_or_default();

                                                    let mut s = load_state();
                                                    let f_str = folder.to_string_lossy().to_string();
                                                    if !s.folders.contains(&f_str) {
                                                        s.folders.push(f_str);
                                                        managed_folders.set(s.folders.clone());
                                                    }
                                                    let mut current_games = games.read().clone();
                                                    for g in folder_games {
                                                        if !current_games.iter().any(|existing| existing.dir == g.dir) {
                                                            current_games.push(g);
                                                        }
                                                    }
                                                    let deduped = dedupe_games(current_games);
                                                    s.cached_games = deduped.clone();
                                                    let _ = save_state(&s);
                                                    games.set(deduped);
                                                }
                                            });
                                        },
                                        "{crate::core::i18n::t(&current_lang.read(), \"btn_add\")}"
                                    }
                                }
                                if managed_folders.read().is_empty() {
                                    p { class: "dim", style: "font-size:0.85em;", "{crate::core::i18n::t(&current_lang.read(), \"settings_none_configured\")}" }
                                } else {
                                    div { class: "paths",
                                        for f_path in managed_folders.read().iter() {
                                            {
                                                let p_str = f_path.clone();
                                                rsx! {
                                                    div { class: "path-row", style: "display:flex; justify-content:space-between; align-items:center; padding:4px 0;",
                                                        span { style: "font-family:monospace; font-size:0.85em;", "{p_str}" }
                                                        button {
                                                            class: "drop",
                                                            title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_remove_folder\")}",
                                                            onclick: move |_| {
                                                                let mut s = load_state();
                                                                s.folders.retain(|f| f != &p_str);
                                                                managed_folders.set(s.folders.clone());
                                                                let mut cur_games = games.read().clone();
                                                                let p_buf = std::path::PathBuf::from(&p_str);
                                                                cur_games.retain(|g| !g.dir.starts_with(&p_buf));
                                                                s.cached_games = cur_games.clone();
                                                                let _ = save_state(&s);
                                                                games.set(cur_games);
                                                            },
                                                            "✕"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Hidden games management
                            if *hidden_count.read() > 0 {
                                div { class: "set-row", style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                    div {
                                        div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"settings_hidden_games\")}" }
                                        div { class: "v", style: "font-size:0.85em; color:var(--dim);", "{hidden_count.read()} {crate::core::i18n::t(&current_lang.read(), \"settings_hidden_count_suffix\")}" }
                                    }
                                    button {
                                        class: "ghost sm",
                                        id: "setUnhideAll",
                                        onclick: move |_| {
                                            let mut s = load_state();
                                            let count = s.hidden.len();
                                            s.unhide_all();
                                            let _ = save_state(&s);
                                            let discovered = discover_all_launchers();
                                            let deduped = dedupe_games(discovered);
                                            let mut current_s = load_state();
                                            current_s.cached_games = deduped.clone();
                                            let _ = save_state(&current_s);
                                            games.set(deduped);
                                            hidden_count.set(0);
                                            crate::core::logger::info("library", &format!("Restored {} hidden game(s)", count));
                                            copy_toast_text.set(format!("Restored {} hidden game(s)", count));
                                            copy_toast.set(true);
                                        },
                                        "{crate::core::i18n::t(&current_lang.read(), \"settings_unhide_all\")}"
                                    }
                                }
                            }

                            // Library configuration file & reset
                            div { class: "set-row", style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                div {
                                    div { class: "k", style: "font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"settings_lib_config\")}" }
                                    div { class: "v", style: "font-size:0.85em; font-family:monospace; color:var(--dim);", "{get_state_path().to_string_lossy()}" }
                                }
                                button {
                                    class: "ghost sm",
                                    id: "setReset",
                                    onclick: move |_| {
                                        let _ = fs::remove_file(get_state_path());
                                        managed_folders.set(Vec::new());
                                        games.set(Vec::new());
                                        copy_toast_text.set(crate::core::i18n::t(&current_lang.read(), "toast_config_reset").to_string());
                                        copy_toast.set(true);
                                    },
                                    "{crate::core::i18n::t(&current_lang.read(), \"settings_reset\")}"
                                }
                            }

                            // GPU diagnostics
                            div { style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                span { "{crate::core::i18n::t(&current_lang.read(), \"diag_primary_gpu\")}" }
                                b { "{primary_gpu.name}" }
                            }
                            div { style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0; border-bottom:1px solid var(--line);",
                                span { "{crate::core::i18n::t(&current_lang.read(), \"diag_vram\")}" }
                                b { "{primary_gpu.dedicated_video_memory / 1024 / 1024} MB" }
                            }
                            div { style: "display:flex; justify-content:space-between; align-items:center; padding:12px 0;",
                                span { "{crate::core::i18n::t(&current_lang.read(), \"diag_ada_arch\")}" }
                                if primary_gpu.is_rtx_40 {
                                    span { style: "color:var(--accent);font-weight:700;", "{crate::core::i18n::t(&current_lang.read(), \"diag_ada_supported\")}" }
                                } else {
                                    span { "{crate::core::i18n::t(&current_lang.read(), \"diag_ada_standard\")}" }
                                }
                            }
                        }
                    }
                }

                // ---------------- ABOUT VIEW ----------------
                if *active_view.read() == "about" {
                    section { class: "view active", id: "view-about",
                        div { class: "section-head",
                            h3 { "{crate::core::i18n::t(&current_lang.read(), \"nav_about\")}" }
                        }
                        div { class: "glass pad about",
                            p { b { "DLSS 5 Studio v1.0" } }
                            p { "{crate::core::i18n::t(&current_lang.read(), \"about_desc_main\")}" }
                            div { class: "about-links", style: "display:flex; flex-wrap:wrap; gap:10px; margin: 16px 0;",
                                a { class: "glass-btn", href: "https://github.com/bookamp/dlss-studio", target: "_blank", "{crate::core::i18n::t(&current_lang.read(), \"about_github_repo\")}" }
                                a { class: "glass-btn", href: "https://github.com/bookamp/dlss-studio/releases", target: "_blank", "{crate::core::i18n::t(&current_lang.read(), \"about_releases\")}" }
                                a { class: "glass-btn", href: "https://github.com/bookamp/dlss-studio/issues", target: "_blank", "{crate::core::i18n::t(&current_lang.read(), \"about_report_issue\")}" }
                            }
                            p { class: "dim", "{crate::core::i18n::t(&current_lang.read(), \"about_desc_sub\")}" }
                        }
                    }
                }
            }

            // ---------------- GAME DETAIL SHEET MODAL ----------------
            if *sheet_open.read() {
                if let Some(mut target_game) = selected_game {
                    {
                        let hero_p = crate::core::state::get_appdata_dir()
                            .join("art")
                            .join(format!("{}-hero.jpg", crate::core::steamart::key_for_dir(&target_game.dir)));
                        let hero_key = crate::core::steamart::key_for_dir(&target_game.dir);
                        let hero_url = if hero_p.exists() {
                            Some(format!("http://dlss-art.localhost/art/{}-hero.jpg", hero_key))
                        } else {
                            target_game.poster.as_ref().map(|p| crate::core::steamart::normalize_art_uri(p))
                        };
                        let cover_url = target_game.poster.as_ref().map(|p| crate::core::steamart::normalize_art_uri(p)).or(hero_url.clone());
                        let reshade_label = if target_game.reshade_installed {
                            let v_str = target_game.reshade_version.as_deref().unwrap_or("6.8.0");
                            let add_suffix = if target_game.reshade_addon_support { " + add-on" } else { "" };
                            format!("{}{}", v_str, add_suffix)
                        } else {
                            crate::core::i18n::t(&current_lang.read(), "val_not_installed").to_string()
                        };
                        let mut available_exes = crate::core::scan::discover_game_exes(&target_game.dir);
                        if available_exes.is_empty() && !target_game.available_exes.is_empty() {
                            available_exes = target_game.available_exes.clone();
                        }
                        available_exes.retain(|e| !crate::core::scan::is_helper_or_tool_path(&e.path));

                        let is_bad_exe = crate::core::scan::is_helper_or_tool_path(&target_game.exe_path)
                            || !target_game.exe_path.is_file()
                            || target_game.exe_path.as_os_str().is_empty();

                        if (is_bad_exe || !available_exes.iter().any(|e| e.path == target_game.exe_path)) && !available_exes.is_empty() {
                            target_game.exe_path = available_exes[0].path.clone();
                            target_game.exe_rel = available_exes[0].rel.clone();
                            target_game.api = available_exes[0].api.clone();
                            target_game.bitness = available_exes[0].bitness;
                            target_game.available_exes = available_exes.clone();

                            let dir = target_game.dir.clone();
                            let updated = target_game.clone();
                            let mut current_games = games.read().clone();
                            if let Some(pos) = current_games.iter().position(|g| g.dir == dir) {
                                current_games[pos] = updated;
                                let mut s = load_state();
                                s.cached_games = current_games.clone();
                                let _ = save_state(&s);
                                games.set(current_games);
                            }
                        }
                        let is_mod_added_dlss = if let Some(manifest) = crate::core::journal::read_manifest(&target_game.dir) {
                            manifest.added.iter().any(|a| {
                                let lower = a.to_lowercase();
                                lower.ends_with("nvngx_dlss.dll") || lower.ends_with("_nvngx.dll") || lower.ends_with("nvngx.dll")
                            })
                        } else {
                            false
                        };
                        let has_native_dlss = (target_game.dlss_version.is_some() || target_game.files.iter().any(|f| {
                            let lower = f.rel.to_lowercase();
                            lower.ends_with("nvngx_dlss.dll") || lower.ends_with("_nvngx.dll") || lower.ends_with("nvngx.dll")
                        })) && !is_mod_added_dlss;
                        let api_lower = target_game.api.to_lowercase();
                        let is_dx11 = api_lower.contains("11") || api_lower == "d3d11";
                        let is_vulkan = api_lower.contains("vulkan");
                        let is_dx12 = api_lower.contains("12") || api_lower == "d3d12";
                        target_game.can_inject_fg = target_game.bitness == 64 && has_native_dlss && !target_game.has_frame_generation && is_vulkan;
                        let opti_reason = crate::core::install_routes::check_opti_reason(&target_game);
                        let is_opti_disabled = opti_reason.is_some();
                        let ac_warning = crate::core::install_guards::has_anti_cheat(&target_game.dir);
                        let _has_dlss = target_game.dlss_version.is_some();
                        let is_native_dlss_supported = crate::core::install_routes::is_native_dlss_supported(&target_game);
                        let cur_backend = backend_choice.read().clone();
                        let effective_backend = if is_opti_disabled && cur_backend == "optiscaler" {
                            "reshade".to_string()
                        } else {
                            cur_backend
                        };
                        let cur_route = route_choice.read().clone();
                        let effective_route = if !is_native_dlss_supported || cur_route == "feeder" {
                            "feeder".to_string()
                        } else {
                            "native".to_string()
                        };
                        let is_fg_capable = crate::core::install_routes::is_frame_generation_supported(&target_game);
                        let show_mfg = is_fg_capable && (primary_gpu.is_rtx_40 || target_game.mfg_unlock_installed);
                        if !show_mfg && *mfg_choice.read() {
                            mfg_choice.set(false);
                        }
                        let show_pre_sr = effective_backend == "optiscaler";
                        let feeder_label = if is_dx11 {
                            crate::core::i18n::t(&current_lang.read(), "feeder_label_dx11")
                        } else if is_vulkan {
                            crate::core::i18n::t(&current_lang.read(), "feeder_label_vulkan")
                        } else if is_dx12 {
                            crate::core::i18n::t(&current_lang.read(), "feeder_label_dx12")
                        } else if api_lower.contains("opengl") {
                            crate::core::i18n::t(&current_lang.read(), "feeder_label_opengl")
                        } else {
                            crate::core::i18n::t(&current_lang.read(), "feeder_label_general")
                        };

                        rsx! {
                            div {
                                class: "overlay",
                                id: "overlay",
                                onclick: move |_| sheet_open.set(false),
                                div {
                                    class: "sheet",
                                    id: "sheet",
                                    onclick: move |e: MouseEvent| e.stop_propagation(),

                                    div { class: "hero",
                                        if let Some(ref h_url) = hero_url {
                                            img { src: "{h_url}", alt: "{target_game.name}" }
                                        }
                                        button {
                                            class: "close",
                                            id: "sheetClose",
                                            onclick: move |_| sheet_open.set(false),
                                            dangerous_inner_html: r#"<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>"#
                                        }
                                    }

                                    div { class: "sheet-body",
                                div { class: "head",
                                    div {
                                        class: "cover",
                                        style: "cursor: pointer;",
                                        title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_click_change_cover\")}",
                                        onclick: {
                                            let dir = target_game.dir.clone();
                                            move |e: MouseEvent| {
                                                e.stop_propagation();
                                                pick_and_set_cover(&dir, games);
                                            }
                                        },
                                        if let Some(ref p_url) = cover_url {
                                            img { src: "{p_url}", alt: "{target_game.name}" }
                                        } else {
                                            div { class: "placeholder", "{&target_game.name[0..2.min(target_game.name.len())]}" }
                                        }
                                    }
                                    div { class: "who",
                                        if !*is_editing_game_name.read() {
                                            div { class: "sheet-title-row",
                                                h3 { "{target_game.name}" }
                                                button {
                                                    class: "btn-rename-pencil",
                                                    title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_rename_game\")}",
                                                    onclick: {
                                                        let cur_name = target_game.name.clone();
                                                        move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            edit_game_name_val.set(cur_name.clone());
                                                            is_editing_game_name.set(true);
                                                        }
                                                    },
                                                    dangerous_inner_html: r#"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>"#
                                                }
                                            }
                                        } else {
                                            div { class: "sheet-title-edit-wrap",
                                                input {
                                                    r#type: "text",
                                                    class: "sheet-title-input",
                                                    value: "{edit_game_name_val.read()}",
                                                    autofocus: true,
                                                    oninput: move |e| edit_game_name_val.set(e.value()),
                                                    onkeydown: {
                                                        let dir = target_game.dir.clone();
                                                        let cur_idx = sheet_game_idx.read().unwrap_or(0);
                                                        move |e: KeyboardEvent| {
                                                            if e.key() == Key::Enter {
                                                                let val = edit_game_name_val.read().trim().to_string();
                                                                let mut s = load_state();
                                                                s.set_custom_name(&dir, &val);
                                                                let _ = save_state(&s);
                                                                let final_name = if val.is_empty() {
                                                                    crate::core::scan::scan_game_directory(&dir).map(|g| g.name).unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().to_string())
                                                                } else {
                                                                    val
                                                                };
                                                                let mut cur = games.read().clone();
                                                                if cur_idx < cur.len() {
                                                                    cur[cur_idx].name = final_name;
                                                                    s.cached_games = cur.clone();
                                                                    let _ = save_state(&s);
                                                                    games.set(cur);
                                                                }
                                                                is_editing_game_name.set(false);
                                                            } else if e.key() == Key::Escape {
                                                                is_editing_game_name.set(false);
                                                            }
                                                        }
                                                    }
                                                }
                                                button {
                                                    class: "sheet-title-btn save",
                                                    title: "{crate::core::i18n::t(&current_lang.read(), \"tooltip_save_name\")}",
                                                    onclick: {
                                                        let dir = target_game.dir.clone();
                                                        let cur_idx = sheet_game_idx.read().unwrap_or(0);
                                                        move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            let val = edit_game_name_val.read().trim().to_string();
                                                            let mut s = load_state();
                                                            s.set_custom_name(&dir, &val);
                                                            let _ = save_state(&s);
                                                            let final_name = if val.is_empty() {
                                                                crate::core::scan::scan_game_directory(&dir).map(|g| g.name).unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().to_string())
                                                            } else {
                                                                val
                                                            };
                                                            let mut cur = games.read().clone();
                                                            if cur_idx < cur.len() {
                                                                cur[cur_idx].name = final_name;
                                                                s.cached_games = cur.clone();
                                                                let _ = save_state(&s);
                                                                games.set(cur);
                                                            }
                                                            is_editing_game_name.set(false);
                                                        }
                                                    },
                                                    dangerous_inner_html: r#"<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>"#
                                                }
                                                button {
                                                    class: "sheet-title-btn cancel",
                                                    title: "{crate::core::i18n::t(&current_lang.read(), \"btn_cancel\")}",
                                                    onclick: move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        is_editing_game_name.set(false);
                                                    },
                                                    dangerous_inner_html: r#"<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>"#
                                                }
                                            }
                                        }
                                        div { class: "meta", "{target_game.launcher} • {target_game.api} • {target_game.bitness}-bit" }
                                        div {
                                            class: "path",
                                            style: "cursor:pointer;",
                                            onclick: {
                                                let dir = target_game.dir.clone();
                                                move |_| {
                                                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                                                }
                                            },
                                            "{target_game.dir.to_string_lossy()}"
                                        }
                                        if available_exes.len() > 1 {
                                            div { class: "exe-switch-row", style: "display:flex; align-items:center; gap:8px; margin:8px 0 10px 0;",
                                                span { style: "font-size:0.8em; color:var(--dim); font-weight:600;", "{crate::core::i18n::t(&current_lang.read(), \"sheet_executable\")}" }
                                                select {
                                                    class: "select sm",
                                                    style: "color-scheme:dark; background:#18181b; color:#f4f4f5; border:1px solid var(--line); border-radius:4px; padding:3px 8px; font-size:0.85em; cursor:pointer;",
                                                    value: "{target_game.exe_path.display()}",
                                                    onchange: {
                                                        let dir = target_game.dir.clone();
                                                        let exes = available_exes.clone();
                                                        move |e: Event<FormData>| {
                                                            let selected_path_str = e.value();
                                                            if let Some(opt) = exes.iter().find(|x| x.path.to_string_lossy() == selected_path_str) {
                                                                let mut current_games = games.read().clone();
                                                                if let Some(pos) = current_games.iter().position(|g| g.dir == dir) {
                                                                    current_games[pos].exe_path = opt.path.clone();
                                                                    current_games[pos].exe_rel = opt.rel.clone();
                                                                    current_games[pos].api = opt.api.clone();
                                                                    current_games[pos].bitness = opt.bitness;
                                                                    if current_games[pos].available_exes.is_empty() {
                                                                        current_games[pos].available_exes = exes.clone();
                                                                    }
                                                                    let is_mod_added_dlss = if let Some(manifest) = crate::core::journal::read_manifest(&current_games[pos].dir) {
                                                                        manifest.added.iter().any(|a| {
                                                                            let lower = a.to_lowercase();
                                                                            lower.ends_with("nvngx_dlss.dll") || lower.ends_with("_nvngx.dll") || lower.ends_with("nvngx.dll")
                                                                        })
                                                                    } else {
                                                                        false
                                                                    };
                                                                    let has_native_dlss = (current_games[pos].dlss_version.is_some() || current_games[pos].files.iter().any(|f| {
                                                                        let lower = f.rel.to_lowercase();
                                                                        lower.ends_with("nvngx_dlss.dll") || lower.ends_with("_nvngx.dll") || lower.ends_with("nvngx.dll")
                                                                    })) && !is_mod_added_dlss;
                                                                    let is_vulkan_opt = opt.api.to_lowercase().contains("vulkan");
                                                                    current_games[pos].can_inject_fg = opt.bitness == 64 && has_native_dlss && !current_games[pos].has_frame_generation && is_vulkan_opt;
                                                                    if !is_vulkan_opt {
                                                                        mfg_choice.set(false);
                                                                    }

                                                                    let mut s = load_state();
                                                                    s.cached_games = current_games.clone();
                                                                    let _ = save_state(&s);
                                                                    games.set(current_games);
                                                                }
                                                            }
                                                        }
                                                    },
                                                    for opt in &available_exes {
                                                        option {
                                                            style: "background:#18181b; color:#f4f4f5;",
                                                            value: "{opt.path.display()}",
                                                            selected: opt.path == target_game.exe_path,
                                                            "{opt.name} ({opt.api})"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        button {
                                            class: "btn-launch-hero",
                                            title: "{crate::core::i18n::t_param(&current_lang.read(), \"tooltip_launch_game\", &target_game.name)}",
                                            onclick: {
                                                let g = target_game.clone();
                                                move |e: MouseEvent| {
                                                    e.stop_propagation();
                                                    launch_game(&g);
                                                }
                                            },
                                            svg {
                                                view_box: "0 0 24 24",
                                                polygon { points: "6,4 20,12 6,20" }
                                            }
                                            span { "{crate::core::i18n::t(&current_lang.read(), \"sheet_launch_game\")}" }
                                        }
                                    }
                                }



                                // Specs Grid
                                div { class: "specs",
                                    div { class: "spec",
                                        span { class: "k", "{crate::core::i18n::t(&current_lang.read(), \"spec_target_exe\")}" }
                                        span { class: "v", "{target_game.exe_rel}" }
                                    }
                                    div { class: "spec",
                                        span { class: "k", "{crate::core::i18n::t(&current_lang.read(), \"spec_architecture\")}" }
                                        span { class: "v", "{target_game.bitness}-bit" }
                                    }
                                    div { class: "spec",
                                        span { class: "k", "{crate::core::i18n::t(&current_lang.read(), \"spec_rendering_api\")}" }
                                        span { class: "v on", "{target_game.api}" }
                                    }
                                    div { class: "spec",
                                        span { class: "k", "{crate::core::i18n::t(&current_lang.read(), \"spec_installed_backend\")}" }
                                        if target_game.installed_route.as_deref() == Some("optiscaler") || (target_game.optiscaler_installed && target_game.installed_route.is_none()) {
                                            span { class: "v on", "{crate::core::i18n::t(&current_lang.read(), \"opt_optiscaler_dlssnr\")}" }
                                        } else if target_game.installed_route.as_deref() == Some("native") {
                                            span { class: "v on", "{crate::core::i18n::t(&current_lang.read(), \"opt_native_dlss\")}" }
                                        } else if target_game.installed_route.as_deref() == Some("feeder") {
                                            span { class: "v on", "{crate::core::i18n::t(&current_lang.read(), \"opt_dlss5_feeder\")}" }
                                        } else if target_game.installed_route.is_some() || target_game.reshade_installed {
                                            span { class: "v on", "ReShade" }
                                        } else {
                                            span { class: "v", "{crate::core::i18n::t(&current_lang.read(), \"val_none\")}" }
                                        }
                                    }
                                    div { class: "spec",
                                        span { class: "k", "DLSS" }
                                        if let Some(ref v) = target_game.dlss_version {
                                            span { class: "v on", "{v}" }
                                        } else {
                                            span { class: "v", "{crate::core::i18n::t(&current_lang.read(), \"val_none\")}" }
                                        }
                                    }
                                    div { class: "spec",
                                        span { class: "k", "{crate::core::i18n::t(&current_lang.read(), \"spec_frame_generation\")}" }
                                        if target_game.has_frame_generation {
                                            span { class: "v on", "{crate::core::i18n::t(&current_lang.read(), \"val_fg_supported\")}" }
                                        } else {
                                            span { class: "v", "{crate::core::i18n::t(&current_lang.read(), \"val_fg_unsupported\")}" }
                                        }
                                    }
                                    div { class: "spec",
                                        span { class: "k", "{crate::core::i18n::t(&current_lang.read(), \"spec_addon\")}" }
                                        if target_game.addon_installed {
                                            span { class: "v on", "{crate::core::i18n::t(&current_lang.read(), \"filter_installed\")}" }
                                        } else {
                                            span { class: "v", "{crate::core::i18n::t(&current_lang.read(), \"val_not_present\")}" }
                                        }
                                    }
                                    div { class: "spec",
                                        span { class: "k", "ReShade" }
                                        if target_game.reshade_installed {
                                            span { class: "v on", "{reshade_label}" }
                                        } else {
                                            span { class: "v", "{reshade_label}" }
                                        }
                                    }
                                    if target_game.optiscaler_installed {
                                        div { class: "spec",
                                            span { class: "k", "{crate::core::i18n::t(&current_lang.read(), \"spec_presr_passes\")}" }
                                            span { class: "v", "{target_game.optiscaler_passes}x" }
                                        }
                                    }
                                }

                                // Backend Compatibility Check & Route Arbitration
                                if ac_warning {
                                    div {
                                        class: "emu-note anti-cheat-warning",
                                        b { "{crate::core::i18n::t(&current_lang.read(), \"sheet_anticheat_title\")}" }
                                        span { "{crate::core::i18n::t(&current_lang.read(), \"sheet_anticheat_desc\")}" }
                                    }
                                }

                                div { class: "install-options",
                                            label {
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"sheet_backend_label\")}" }
                                                select {
                                                    id: "backendChoice",
                                                    value: "{effective_backend}",
                                                    onchange: move |e| backend_choice.set(e.value()),
                                                    option { value: "reshade", "{crate::core::i18n::t(&current_lang.read(), \"backend_reshade_default\")}" }
                                                    option { value: "optiscaler", disabled: is_opti_disabled, "{crate::core::i18n::t(&current_lang.read(), \"opt_optiscaler_dlssnr\")}" }
                                                }
                                            }
                                            if effective_backend != "optiscaler" {
                                                label {
                                                    span { "{crate::core::i18n::t(&current_lang.read(), \"sheet_route_label\")}" }
                                                    select {
                                                        id: "routeChoice",
                                                        value: "{effective_route}",
                                                        onchange: move |e| route_choice.set(e.value()),
                                                        if is_native_dlss_supported {
                                                            option { value: "native", "{crate::core::i18n::t(&current_lang.read(), \"opt_native_dlss\")}" }
                                                        }
                                                        option { value: "feeder", "{feeder_label}" }
                                                    }
                                                }
                                            }
                                        }

                                        if let Some(ref reason) = opti_reason {
                                            div {
                                                class: "emu-note compatibility-warning",
                                                b { "{crate::core::i18n::t(&current_lang.read(), \"sheet_backend_restriction\")}" }
                                                span { "{reason.message()}" }
                                            }
                                        }

                                        if show_pre_sr {
                                            div { class: if *opti_pre_sr.read() { "sheet-feature-card on" } else { "sheet-feature-card" },
                                                input {
                                                    type: "checkbox",
                                                    id: "chkPreSr",
                                                    checked: *opti_pre_sr.read(),
                                                    onchange: move |e| opti_pre_sr.set(e.checked())
                                                }
                                                div { class: "body",
                                                    div { class: "t",
                                                        label {
                                                            r#for: "chkPreSr",
                                                            class: "t-left",
                                                            style: "cursor: pointer;",
                                                            span { "{crate::core::i18n::t(&current_lang.read(), \"feature_presr_title\")}" }
                                                            span { class: "tag accent", "{crate::core::i18n::t(&current_lang.read(), \"feature_presr_tag\")}" }
                                                        }
                                                        div { class: "passes-ctrl",
                                                            span { "{crate::core::i18n::t(&current_lang.read(), \"feature_presr_passes\")}" }
                                                            select {
                                                                class: "passes-select",
                                                                value: "{opti_passes}",
                                                                disabled: !*opti_pre_sr.read(),
                                                                onchange: move |e| opti_passes.set(e.value().parse::<u32>().unwrap_or(1)),
                                                                option { value: "1", "1x" }
                                                                option { value: "2", "2x" }
                                                                option { value: "3", "3x" }
                                                            }
                                                        }
                                                    }
                                                    div { class: "d", "{crate::core::i18n::t(&current_lang.read(), \"feature_presr_desc\")}" }
                                                }
                                            }
                                        }

                                        if show_mfg {
                                            div { class: if *mfg_choice.read() { "sheet-feature-card on" } else { "sheet-feature-card" },
                                                input {
                                                    type: "checkbox",
                                                    id: "chkMfg",
                                                    checked: *mfg_choice.read(),
                                                    onchange: move |e| mfg_choice.set(e.checked())
                                                }
                                                div { class: "body",
                                                    div { class: "t",
                                                        label {
                                                            r#for: "chkMfg",
                                                            class: "t-left",
                                                            style: "cursor: pointer;",
                                                            span { "{crate::core::i18n::t(&current_lang.read(), \"feature_mfg_title\")}" }
                                                            span { class: "tag warn", "{crate::core::i18n::t(&current_lang.read(), \"feature_mfg_tag\")}" }
                                                        }
                                                        div { class: "passes-ctrl",
                                                            span { "{crate::core::i18n::t(&current_lang.read(), \"sheet_mfg_multiplier\")}" }
                                                            select {
                                                                class: "passes-select",
                                                                value: "{mfg_multiplier}",
                                                                disabled: !*mfg_choice.read(),
                                                                onchange: move |e| mfg_multiplier.set(e.value().parse::<u32>().unwrap_or(4)),
                                                                option { value: "1", "1x (Auto)" }
                                                                option { value: "2", "2x" }
                                                                option { value: "3", "3x" }
                                                                option { value: "4", "4x (Default)" }
                                                            }
                                                        }
                                                    }
                                                    div { class: "d", "{crate::core::i18n::t(&current_lang.read(), \"feature_mfg_desc\")}" }
                                                }
                                            }
                                        } else if is_dx11 && !target_game.has_frame_generation {
                                            div {
                                                class: "emu-note",
                                                b { "4x Multi-Frame Generation: Requires Native DLSS-G on DirectX 11" }
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"feature_mfg_dx11_note\")}" }
                                            }
                                        } else if !target_game.has_frame_generation {
                                            div {
                                                class: "emu-note",
                                                b { "4x Multi-Frame Generation: Requires Native DLSS-G" }
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"feature_mfg_general_note\")}" }
                                            }
                                        } else if target_game.has_frame_generation && !primary_gpu.is_rtx_40 && !target_game.mfg_unlock_installed {
                                            div {
                                                class: "emu-note",
                                                b { "4x Multi-Frame Generation: Requires RTX 40-Series GPU" }
                                                span { "This game supports native Frame Generation, but 4x MFG unlock requires an Ada Lovelace (RTX 40-Series) GPU. (NVIDIA restricts MFG to RTX 50-Series; this mod unlocks it on RTX 40-Series cards)." }
                                            }
                                        }

                                        // Filelist
                                        if !target_game.files.is_empty() {
                                            div { class: "filelist",
                                                for f in &target_game.files {
                                                    div { class: "filerow",
                                                        span { class: "f", "{f.rel}" }
                                                        span { class: "v", "{f.version.as_deref().unwrap_or(\"-\")}" }
                                                    }
                                                }
                                            }
                                        }

                                        // Actions
                                        {
                                            let is_deployed = target_game.optiscaler_installed
                                                || target_game.addon_installed
                                                || target_game.installed_route.is_some()
                                                || target_game.has_backup;
                                            rsx! {
                                                div { class: "sheet-actions",
                                                    button {
                                                        class: "btn-install",
                                                        id: "doInstall",
                                                        disabled: *is_busy.read(),
                                                        onclick: {
                                                            let target_game = target_game.clone();
                                                            let cur_eff_backend = effective_backend.clone();
                                                            let cur_eff_route = effective_route.clone();
                                                            let opti_pre_sr_val = *opti_pre_sr.read();
                                                            let opti_passes_val = *opti_passes.read();
                                                            let mfg_choice_val = if cur_eff_backend == "optiscaler" && is_dx11 && !target_game.has_frame_generation {
                                                                false
                                                            } else {
                                                                *mfg_choice.read()
                                                            };
                                                            let mfg_multiplier_val = *mfg_multiplier.read();
                                                            move |_| {
                                                                if *is_busy.read() {
                                                                    return;
                                                                }
                                                                is_busy.set(true);

                                                                let target_game = target_game.clone();
                                                                let cur_eff_backend = cur_eff_backend.clone();
                                                                let cur_eff_route = cur_eff_route.clone();

                                                                dioxus::prelude::spawn(async move {
                                                                    let mut lines = Vec::new();
                                                                    lines.push(format!("--- installing DLSS 5 onto {} ---", target_game.name));
                                                                    job_lines.set(lines.clone());

                                                                    let game_dir = target_game.dir.clone();
                                                                    let exe_path = target_game.exe_path.clone();

                                                                    let closed_check = tokio::task::spawn_blocking({
                                                                        let gd = game_dir.clone();
                                                                        let ep = exe_path.clone();
                                                                        move || assert_game_closed(&gd, Some(&ep))
                                                                    }).await;

                                                                    if let Ok(Err(e)) = closed_check {
                                                                        lines.push(format!("[ERROR] {}", e));
                                                                        job_lines.set(lines);
                                                                        is_busy.set(false);
                                                                        return;
                                                                    }

                                                                    let mod_root = managed_mod_root(&game_dir, Some(&exe_path)).unwrap_or_else(|| game_dir.clone());
                                                                    lines.push(format!("[ROUTING] Target mod directory: {}", mod_root.display()));
                                                                    job_lines.set(lines.clone());

                                                                    let deploy_opts = crate::core::optiscaler::DeployOptions {
                                                                        game_name: Some(target_game.name.clone()),
                                                                        game_dir: target_game.dir.clone(),
                                                                        exe_path: target_game.exe_path.clone(),
                                                                        api: target_game.api.clone(),
                                                                        pre_sr: opti_pre_sr_val,
                                                                        passes: opti_passes_val,
                                                                        mfg_unlock: mfg_choice_val,
                                                                        mfg_multiplier: mfg_multiplier_val,
                                                                    };

                                                                    // Asynchronously fetch/verify Feeder components and latest MFG unlock if required
                                                                    let mut payloads = match crate::core::optiscaler::PayloadBundle::from_system() {
                                                                        Ok(p) => p,
                                                                        Err(e) => {
                                                                            lines.push(format!("[ERROR] System payload resolution failed: {}", e));
                                                                            job_lines.set(lines);
                                                                            is_busy.set(false);
                                                                            return;
                                                                        }
                                                                    };
                                                                    if cur_eff_backend != "optiscaler" && cur_eff_route == "feeder" {
                                                                        if payloads.feeder_components.is_none() {
                                                                            match crate::core::downloader::ensure_feeder_components(&mut lines).await {
                                                                                Ok(fc) => {
                                                                                    payloads.feeder_components = Some(fc);
                                                                                    job_lines.set(lines.clone());
                                                                                }
                                                                                Err(e) => {
                                                                                    lines.push(format!("[ERROR] Feeder component resolution failed: {}", e));
                                                                                    job_lines.set(lines);
                                                                                    is_busy.set(false);
                                                                                    return;
                                                                                }
                                                                            }
                                                                        }
                                                                        if mfg_choice_val && payloads.renodx_mfgunlock_addon.is_none() {
                                                                            match crate::core::downloader::ensure_mfg_v09_addon(&mut lines).await {
                                                                                Ok(addon_p) => {
                                                                                    payloads.renodx_mfgunlock_addon = Some(addon_p);
                                                                                    job_lines.set(lines.clone());
                                                                                }
                                                                                Err(e) => {
                                                                                    lines.push(format!("[WARN] MFG Unlock addon download: {}", e));
                                                                                    job_lines.set(lines.clone());
                                                                                }
                                                                            }
                                                                        }
                                                                    }

                                                                    // Perform deployment in blocking task
                                                                    let b_task = cur_eff_backend.clone();
                                                                    let r_task = cur_eff_route.clone();
                                                                    let result = tokio::task::spawn_blocking(move || {
                                                                        if b_task == "optiscaler" {
                                                                            crate::core::optiscaler::deploy_optiscaler(&deploy_opts)
                                                                        } else if r_task == "native" {
                                                                            crate::core::optiscaler::deploy_native_dlss5(&deploy_opts)
                                                                        } else {
                                                                            crate::core::optiscaler::deploy_feeder_with_bundle(&deploy_opts, &payloads)
                                                                        }
                                                                    }).await;

                                                                    match result {
                                                                        Ok(Ok(res)) => {
                                                                            lines.extend(res.log_lines);
                                                                            touch(&target_game.dir.to_string_lossy());

                                                                            let refreshed_opt = tokio::task::spawn_blocking({
                                                                                let gd = target_game.dir.clone();
                                                                                move || crate::core::scan::scan_game_directory(&gd)
                                                                            }).await.ok().flatten();

                                                                            let mut updated_game = target_game.clone();
                                                                            if let Some(mut refreshed) = refreshed_opt {
                                                                                refreshed.launcher = updated_game.launcher.clone();
                                                                                refreshed.exe_path = updated_game.exe_path.clone();
                                                                                refreshed.exe_rel = updated_game.exe_rel.clone();
                                                                                refreshed.api = updated_game.api.clone();
                                                                                refreshed.bitness = updated_game.bitness;
                                                                                if refreshed.poster.is_none() {
                                                                                    refreshed.poster = updated_game.poster.clone();
                                                                                }
                                                                                let s = load_state();
                                                                                if let Some(custom) = s.get_custom_name(&updated_game.dir) {
                                                                                    refreshed.name = custom.clone();
                                                                                } else if refreshed.name.is_empty() || refreshed.name == "Unknown" {
                                                                                    refreshed.name = updated_game.name.clone();
                                                                                }
                                                                                updated_game = refreshed;
                                                                            }

                                                                            if cur_eff_backend == "optiscaler" {
                                                                                updated_game.optiscaler_installed = true;
                                                                                updated_game.optiscaler_presr = opti_pre_sr_val;
                                                                                updated_game.optiscaler_passes = opti_passes_val;
                                                                                updated_game.mfg_unlock_installed = mfg_choice_val;
                                                                                updated_game.addon_installed = false;
                                                                                updated_game.reshade_installed = false;
                                                                                updated_game.installed_route = Some("optiscaler".to_string());
                                                                            } else if cur_eff_route == "native" {
                                                                                updated_game.optiscaler_installed = false;
                                                                                updated_game.optiscaler_presr = false;
                                                                                updated_game.optiscaler_passes = 1;
                                                                                updated_game.mfg_unlock_installed = mfg_choice_val;
                                                                                updated_game.addon_installed = true;
                                                                                updated_game.reshade_installed = true;
                                                                                updated_game.installed_route = Some("native".to_string());
                                                                            } else {
                                                                                updated_game.optiscaler_installed = false;
                                                                                updated_game.optiscaler_presr = false;
                                                                                updated_game.optiscaler_passes = 1;
                                                                                updated_game.mfg_unlock_installed = mfg_choice_val;
                                                                                updated_game.addon_installed = true;
                                                                                updated_game.reshade_installed = true;
                                                                                updated_game.installed_route = Some("feeder".to_string());
                                                                            }
                                                                            updated_game.has_backup = true;

                                                                            let mut current_games = games.read().clone();
                                                                            if let Some(pos) = current_games.iter().position(|g| g.dir == updated_game.dir) {
                                                                                current_games[pos] = updated_game.clone();
                                                                            }
                                                                            games.set(current_games.clone());

                                                                            let mut s = load_state();
                                                                            s.cached_games = current_games;
                                                                            let _ = save_state(&s);
                                                                        }
                                                                        Ok(Err(err)) => {
                                                                            lines.push(format!("[ERROR] Deployment failed: {}", err));
                                                                        }
                                                                        Err(join_err) => {
                                                                            lines.push(format!("[ERROR] Task execution failed: {}", join_err));
                                                                        }
                                                                    }
                                                                    job_lines.set(lines);
                                                                    is_busy.set(false);
                                                                });
                                                            }
                                                        },
                                                        {
                                                            if *is_busy.read() {
                                                                crate::core::i18n::t(&current_lang.read(), "btn_working")
                                                            } else if is_deployed {
                                                                crate::core::i18n::t(&current_lang.read(), "sheet_deploy")
                                                            } else {
                                                                crate::core::i18n::t(&current_lang.read(), "install")
                                                            }
                                                        }
                                                    }

                                                    if target_game.has_backup {
                                                        button {
                                                            class: "btn-restore",
                                                            id: "doRestore",
                                                            disabled: *is_busy.read(),
                                                            onclick: {
                                                                let target_game = target_game.clone();
                                                                move |_| {
                                                                    if *is_busy.read() {
                                                                        return;
                                                                    }
                                                                    is_busy.set(true);
                                                                    let target_game = target_game.clone();

                                                                    dioxus::prelude::spawn(async move {
                                                                        let mut lines = Vec::new();
                                                                        let gd = target_game.dir.clone();
                                                                        let ep = target_game.exe_path.clone();

                                                                        let closed_check = tokio::task::spawn_blocking({
                                                                            let gd = gd.clone();
                                                                            let ep = ep.clone();
                                                                            move || crate::core::install_guards::assert_game_closed(&gd, Some(&ep))
                                                                        }).await;

                                                                        if let Ok(Err(e)) = closed_check {
                                                                            lines.push(format!("[ERROR] {}", e));
                                                                            job_lines.set(lines);
                                                                            is_busy.set(false);
                                                                            return;
                                                                        }

                                                                        lines.push(format!("--- restoring {} ---", target_game.name));
                                                                        job_lines.set(lines.clone());

                                                                        let res = tokio::task::spawn_blocking({
                                                                            let gd = gd.clone();
                                                                            move || restore_game(&gd)
                                                                        }).await;

                                                                        match res {
                                                                            Ok(Ok(true)) => {
                                                                                lines.push("[RESTORE] Originals restored from _DLSS5_Backup".to_string());
                                                                                let _ = append_history(&HistoryRow {
                                                                                    date: crate::core::journal::now_timestamp_str(),
                                                                                    dir: target_game.dir.to_string_lossy().to_string(),
                                                                                    game_name: Some(target_game.name.clone()),
                                                                                    action: "restore".to_string(),
                                                                                    replaced: 0,
                                                                                    added: 0,
                                                                                });
                                                                                touch(&target_game.dir.to_string_lossy());

                                                                                let refreshed_opt = tokio::task::spawn_blocking({
                                                                                    let gd = gd.clone();
                                                                                    move || crate::core::scan::scan_game_directory(&gd)
                                                                                }).await.ok().flatten();

                                                                                let mut updated_game = target_game.clone();
                                                                                if let Some(mut refreshed) = refreshed_opt {
                                                                                    refreshed.launcher = updated_game.launcher.clone();
                                                                                    if refreshed.poster.is_none() {
                                                                                        refreshed.poster = updated_game.poster.clone();
                                                                                    }
                                                                                    let s = load_state();
                                                                                    if let Some(custom) = s.get_custom_name(&updated_game.dir) {
                                                                                        refreshed.name = custom.clone();
                                                                                    } else if refreshed.name.is_empty() || refreshed.name == "Unknown" {
                                                                                        refreshed.name = updated_game.name.clone();
                                                                                    }
                                                                                    updated_game = refreshed;
                                                                                } else {
                                                                                    updated_game.optiscaler_installed = false;
                                                                                    updated_game.optiscaler_presr = false;
                                                                                    updated_game.optiscaler_passes = 1;
                                                                                    updated_game.mfg_unlock_installed = false;
                                                                                    updated_game.reshade_installed = false;
                                                                                    updated_game.addon_installed = false;
                                                                                    updated_game.installed_route = None;
                                                                                    updated_game.has_backup = false;
                                                                                }

                                                                                let mut current_games = games.read().clone();
                                                                                if let Some(pos) = current_games.iter().position(|g| g.dir == updated_game.dir) {
                                                                                    current_games[pos] = updated_game.clone();
                                                                                }
                                                                                games.set(current_games.clone());

                                                                                let mut s = load_state();
                                                                                s.cached_games = current_games;
                                                                                let _ = save_state(&s);
                                                                            }
                                                                            _ => {
                                                                                lines.push("[RESTORE] No backup manifest found or error restoring".to_string());
                                                                            }
                                                                        }
                                                                        job_lines.set(lines);
                                                                        is_busy.set(false);
                                                                    });
                                                                }
                                                            },
                                                            "{crate::core::i18n::t(&current_lang.read(), \"restore\")}"
                                                        }
                                                    } else if is_deployed {
                                                        button {
                                                            class: "btn-restore",
                                                            id: "doClean",
                                                            disabled: *is_busy.read(),
                                                            onclick: {
                                                                let target_game = target_game.clone();
                                                                move |_| {
                                                                    if *is_busy.read() {
                                                                        return;
                                                                    }
                                                                    is_busy.set(true);
                                                                    let target_game = target_game.clone();

                                                                    dioxus::prelude::spawn(async move {
                                                                        let mut lines = Vec::new();
                                                                        let gd = target_game.dir.clone();
                                                                        let ep = target_game.exe_path.clone();

                                                                        let closed_check = tokio::task::spawn_blocking({
                                                                            let gd = gd.clone();
                                                                            let ep = ep.clone();
                                                                            move || crate::core::install_guards::assert_game_closed(&gd, Some(&ep))
                                                                        }).await;

                                                                        if let Ok(Err(e)) = closed_check {
                                                                            lines.push(format!("[ERROR] {}", e));
                                                                            job_lines.set(lines);
                                                                            is_busy.set(false);
                                                                            return;
                                                                        }

                                                                        lines.push(format!("--- removing mod files from {} ---", target_game.name));
                                                                        job_lines.set(lines.clone());

                                                                        let res = tokio::task::spawn_blocking({
                                                                            let gd = gd.clone();
                                                                            let ep = ep.clone();
                                                                            move || crate::core::journal::clean_untracked_mods_with_exe(&gd, Some(&ep))
                                                                        }).await;

                                                                        match res {
                                                                            Ok(Ok(removed)) => {
                                                                                for r in &removed {
                                                                                    lines.push(format!("[CLEAN] Removed {}", r));
                                                                                }
                                                                                lines.push(format!("[CLEAN] Purged {} mod item(s)", removed.len()));
                                                                                let _ = append_history(&HistoryRow {
                                                                                    date: crate::core::journal::now_timestamp_str(),
                                                                                    dir: target_game.dir.to_string_lossy().to_string(),
                                                                                    game_name: Some(target_game.name.clone()),
                                                                                    action: "clean".to_string(),
                                                                                    replaced: 0,
                                                                                    added: 0,
                                                                                });
                                                                                touch(&target_game.dir.to_string_lossy());

                                                                                let refreshed_opt = tokio::task::spawn_blocking({
                                                                                    let gd = gd.clone();
                                                                                    move || crate::core::scan::scan_game_directory(&gd)
                                                                                }).await.ok().flatten();

                                                                                let mut updated_game = target_game.clone();
                                                                                if let Some(mut refreshed) = refreshed_opt {
                                                                                    refreshed.launcher = updated_game.launcher.clone();
                                                                                    if refreshed.poster.is_none() {
                                                                                        refreshed.poster = updated_game.poster.clone();
                                                                                    }
                                                                                    let s = load_state();
                                                                                    if let Some(custom) = s.get_custom_name(&updated_game.dir) {
                                                                                        refreshed.name = custom.clone();
                                                                                    } else if refreshed.name.is_empty() || refreshed.name == "Unknown" {
                                                                                        refreshed.name = updated_game.name.clone();
                                                                                    }
                                                                                    updated_game = refreshed;
                                                                                } else {
                                                                                    updated_game.optiscaler_installed = false;
                                                                                    updated_game.reshade_installed = false;
                                                                                    updated_game.addon_installed = false;
                                                                                    updated_game.installed_route = None;
                                                                                    updated_game.has_backup = false;
                                                                                }

                                                                                let mut current_games = games.read().clone();
                                                                                if let Some(pos) = current_games.iter().position(|g| g.dir == updated_game.dir) {
                                                                                    current_games[pos] = updated_game.clone();
                                                                                }
                                                                                games.set(current_games.clone());

                                                                                let mut s = load_state();
                                                                                s.cached_games = current_games;
                                                                                let _ = save_state(&s);
                                                                            }
                                                                            Ok(Err(e)) => {
                                                                                lines.push(format!("[ERROR] Failed to clean mods: {}", e));
                                                                            }
                                                                            Err(je) => {
                                                                                lines.push(format!("[ERROR] Task execution failed: {}", je));
                                                                            }
                                                                        }
                                                                        job_lines.set(lines);
                                                                        is_busy.set(false);
                                                                    });
                                                                }
                                                            },
                                                            "{crate::core::i18n::t(&current_lang.read(), \"btn_clean_mods\")}"
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                // Terminal Log
                                div { class: "job-terminal", style: "margin-top:16px;",
                                    div { class: "job-head", style: "display:flex; justify-content:space-between; margin-bottom:6px;",
                                        span { class: "dim", "{crate::core::i18n::t(&current_lang.read(), \"btn_execution_log\")}" }
                                        button {
                                            class: "ghost sm",
                                            onclick: move |_| {
                                                let text = job_lines.read().join("\r\n");
                                                copy_to_clipboard(&text);
                                                copy_toast_text.set(crate::core::i18n::t(&current_lang.read(), "toast_activity_copied").to_string());
                                                copy_toast.set(true);
                                            },
                                            "{crate::core::i18n::t(&current_lang.read(), \"btn_copy_log\")}"
                                        }
                                    }
                                    div { class: "job-body", style: "font-family:monospace; font-size:0.85em; background:rgba(0,0,0,0.4); padding:10px; border-radius:6px; max-height:120px; overflow-y:auto;",
                                        for line in job_lines.read().iter() {
                                            div { "{line}" }
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

                        // ---------------- OVERLAY HOTKEY DIALOG ----------------
            if *overlay_hotkey_open.read() {
                div {
                    class: "overlay",
                    style: "display:flex; justify-content:center; align-items:center; z-index:9999;",
                    onclick: move |_| overlay_hotkey_open.set(false),
                    div {
                        class: "glass pad",
                        style: "width: 360px; max-width:90%; background: #181b20; border: 1px solid var(--line); border-radius: 8px; padding: 20px;",
                        onclick: move |e| e.stop_propagation(),
                        h4 { style: "margin-top:0; margin-bottom: 8px;", "{crate::core::i18n::t(&current_lang.read(), \"hotkey_dialog_title\")}" }
                        p { class: "hint", style: "margin-bottom:16px; font-size:0.85em;", "Select the shortcut to toggle HUD in-game:" }
                        div { style: "display:grid; grid-template-columns: 1fr 1fr; gap:8px; margin-bottom:16px;",
                            for hk in &["F8", "F9", "F10", "F11", "Ctrl+Shift+O", "Alt+`"] {
                                {
                                    let key_str = hk.to_string();
                                    let is_active = *overlay_hotkey.read() == key_str;
                                    rsx! {
                                        button {
                                            class: if is_active { "glass-btn" } else { "ghost sm" },
                                            style: "padding: 8px;",
                                            onclick: {
                                                let k = key_str.clone();
                                                move |_| {
                                                    overlay_hotkey.set(k.clone());
                                                    let mut s = load_state();
                                                    s.overlay_hotkey = k.clone();
                                                    let _ = save_state(&s);
                                                    crate::core::overlay_bridge::sync_overlay_preferences(&s);
                                                    overlay_hotkey_open.set(false);
                                                }
                                            },
                                            "{key_str}"
                                        }
                                    }
                                }
                            }
                        }
                        div { style: "text-align:right;",
                            button { class: "ghost sm", onclick: move |_| overlay_hotkey_open.set(false), "{crate::core::i18n::t(&current_lang.read(), \"btn_close\")}" }
                        }
                    }
                }
            }

            // ---------------- OVERLAY CREATE THEME DIALOG ----------------
            if *overlay_create_open.read() {
                div {
                    class: "overlay",
                    style: "display:flex; justify-content:center; align-items:center; z-index:9999;",
                    onclick: move |_| overlay_create_open.set(false),
                    div {
                        class: "glass pad",
                        style: "width: 400px; max-width:90%; background: #181b20; border: 1px solid var(--line); border-radius: 8px; padding: 24px;",
                        onclick: move |e| e.stop_propagation(),
                        h4 { style: "margin-top:0; margin-bottom: 6px;", "{crate::core::i18n::t(&current_lang.read(), \"theme_create_title\")}" }
                        p { class: "hint", style: "margin-bottom:16px; font-size:0.85em;", "{crate::core::i18n::t(&current_lang.read(), \"theme_create_hint\")}" }
                        div { style: "margin-bottom:12px;",
                            label { style: "display:block; font-size:0.85em; font-weight:600; margin-bottom:4px;", "{crate::core::i18n::t(&current_lang.read(), \"theme_create_name\")}" }
                            input {
                                style: "width:100%; padding:8px 12px; background:rgba(0,0,0,0.3); border:1px solid var(--line); border-radius:6px; color:var(--text); box-sizing:border-box;",
                                value: "{custom_theme_name.read()}",
                                oninput: move |e| custom_theme_name.set(e.value())
                            }
                        }
                        div { style: "margin-bottom:16px;",
                            label { style: "display:block; font-size:0.85em; font-weight:600; margin-bottom:4px;", "{crate::core::i18n::t(&current_lang.read(), \"theme_create_color\")}" }
                            div { style: "display:flex; gap:10px; align-items:center;",
                                input {
                                    type: "color",
                                    style: "width:44px; height:36px; padding:0; border:none; border-radius:4px; background:transparent; cursor:pointer;",
                                    value: "{custom_theme_color.read()}",
                                    oninput: move |e| custom_theme_color.set(e.value())
                                }
                                span { style: "font-family:monospace; font-weight:600;", "{custom_theme_color.read()}" }
                            }
                        }
                        // Real-time Swatch Preview
                        div {
                            style: "background:rgba(0,0,0,0.4); border-left: 4px solid {custom_theme_color.read()}; padding: 12px; border-radius: 4px; margin-bottom: 20px;",
                            div { style: "display:flex; justify-content:space-between; font-size:0.9em;",
                                b { "{custom_theme_name.read()}" }
                                span { style: "color:{custom_theme_color.read()}; font-family:monospace; font-weight:bold;", "120 FPS" }
                            }
                            span { class: "dim", style: "font-size:0.8em;", "{crate::core::i18n::t(&current_lang.read(), \"theme_preview_simulated\")}" }
                        }
                        div { style: "display:flex; justify-content:flex-end; gap:8px;",
                            button {
                                class: "ghost sm",
                                onclick: move |_| overlay_create_open.set(false),
                                "{crate::core::i18n::t(&current_lang.read(), \"btn_cancel\")}"
                            }
                            button {
                                class: "glass-btn sm",
                                onclick: move |_| {
                                    let name = if custom_theme_name.read().trim().is_empty() {
                                        "My Theme".to_string()
                                    } else {
                                        custom_theme_name.read().trim().to_string()
                                    };
                                    let color = custom_theme_color.read().clone();
                                    let new_id = format!("custom_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
                                    let new_theme = crate::core::state::CustomOverlayTheme {
                                        id: new_id.clone(),
                                        name,
                                        color,
                                    };
                                    let mut list = custom_overlay_themes.read().clone();
                                    list.push(new_theme);
                                    custom_overlay_themes.set(list.clone());
                                    overlay_theme.set(new_id.clone());
                                    let mut s = load_state();
                                    s.custom_overlay_themes = list;
                                    s.overlay_theme = new_id;
                                    let _ = save_state(&s);
                                    crate::core::overlay_bridge::sync_overlay_preferences(&s);
                                    overlay_create_open.set(false);
                                },
                                "{crate::core::i18n::t(&current_lang.read(), \"btn_save_apply\")}"
                            }
                        }
                    }
                }
            }

            // ---------------- OVERLAY INTERACTIVE PREVIEW MODAL ----------------
            if *overlay_preview_open.read() {
                {
                    let p_id = preview_theme_id.read().clone();
                    let custom_themes = custom_overlay_themes.read().clone();
                    let (p_name, p_accent, p_soft, p_back, p_bright, p_attr) = match p_id.as_str() {
                        "blue" | "azure" => ("Azure".to_string(), "#4aa8ee".to_string(), "#4aa8ee25".to_string(), "#0d1724".to_string(), "#91d1ff".to_string(), "blue".to_string()),
                        "purple" | "amethyst" => ("Amethyst".to_string(), "#b45dea".to_string(), "#b45dea25".to_string(), "#1b1127".to_string(), "#ddb0ff".to_string(), "purple".to_string()),
                        "green" | "emerald" => ("Emerald".to_string(), "#8bc400".to_string(), "#8bc40025".to_string(), "#111a10".to_string(), "#c2ec66".to_string(), "green".to_string()),
                        custom_id => {
                            if let Some(ct) = custom_themes.iter().find(|c| c.id == custom_id) {
                                (ct.name.clone(), ct.color.clone(), format!("{}25", ct.color), "#15181e".to_string(), ct.color.clone(), "custom".to_string())
                            } else {
                                ("Emerald".to_string(), "#8bc400".to_string(), "#8bc40025".to_string(), "#111a10".to_string(), "#c2ec66".to_string(), "green".to_string())
                            }
                        }
                    };

                    let p_id_apply = p_id.clone();

                    rsx! {
                        div {
                            class: "overlay",
                            style: "display:flex; justify-content:center; align-items:center; z-index:9999; overflow-y:auto; padding:20px 0;",
                            onclick: move |_| overlay_preview_open.set(false),
                            div {
                                class: "ol-dialog",
                                style: format!("--ol-accent:{}; --ol-soft:{}; --ol-back:{}; --ol-bright:{}; margin:auto;", p_accent, p_soft, p_back, p_bright),
                                "data-overlay-theme": "{p_attr}",
                                onclick: move |e| e.stop_propagation(),
                                div { class: "ol-dialog-head",
                                    h3 { "{crate::core::i18n::t(&current_lang.read(), \"preview_dialog_title\")} • {p_name}" }
                                    button { class: "ghost sm", onclick: move |_| overlay_preview_open.set(false), "✕" }
                                }
                                p { "Interactive RenoDX compact overlay in native resolution (534px). Toggle in-game with {overlay_hotkey.read()}." }
                                div { style: "display:flex; justify-content:center; margin: 16px 0; direction:ltr;",
                                    div { class: "ol-panel", style: "width: 534px; max-width: 100%; box-sizing: border-box;",
                                        header {
                                            div { class: "ol-eyebrow", "DLSS 5 STUDIO CONTROLS" }
                                            span { class: "ol-prototype", "{crate::core::i18n::t(&current_lang.read(), \"preview_interactive_tag\")}" }
                                        }
                                        section { class: "ol-master",
                                            label { class: "ol-check",
                                                input {
                                                    type: "checkbox",
                                                    checked: *prev_dlss_on.read(),
                                                    onchange: move |_| {
                                                        let cur = *prev_dlss_on.read();
                                                        prev_dlss_on.set(!cur);
                                                    }
                                                }
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_dlss_on\")}" }
                                            }
                                        }
                                        section {
                                            h4 { "{crate::core::i18n::t(&current_lang.read(), \"preview_global_controls\")}" }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_structure_intensity\")}" }
                                                input {
                                                    type: "range", min: "0", max: "2", step: "0.05",
                                                    value: "{prev_structure.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_structure.set(v); }
                                                }
                                                output { "{prev_structure.read():.2}" }
                                            }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_tone_intensity\")}" }
                                                input {
                                                    type: "range", min: "0", max: "2", step: "0.05",
                                                    value: "{prev_tone.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_tone.set(v); }
                                                }
                                                output { "{prev_tone.read():.2}" }
                                            }
                                        }
                                        section { class: "ol-group",
                                            label { class: "ol-check",
                                                input {
                                                    type: "checkbox",
                                                    checked: *prev_char_mask.read(),
                                                    onchange: move |_| {
                                                        let cur = *prev_char_mask.read();
                                                        prev_char_mask.set(!cur);
                                                    }
                                                }
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_char_mask\")}" }
                                            }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_char_structure\")}" }
                                                input {
                                                    type: "range", min: "0", max: "2", step: "0.05",
                                                    value: "{prev_char_structure.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_char_structure.set(v); }
                                                }
                                                output { "{prev_char_structure.read():.2}" }
                                            }
                                        }
                                        section {
                                            h4 { "{crate::core::i18n::t(&current_lang.read(), \"preview_nr_style\")}" }
                                            div { class: "ol-models",
                                                button {
                                                    class: if *prev_nr_style.read() == 0 { "ol-model selected" } else { "ol-model" },
                                                    onclick: move |_| prev_nr_style.set(0),
                                                    "{crate::core::i18n::t(&current_lang.read(), \"preview_model_a\")}"
                                                }
                                                button {
                                                    class: if *prev_nr_style.read() == 1 { "ol-model selected" } else { "ol-model" },
                                                    onclick: move |_| prev_nr_style.set(1),
                                                    "{crate::core::i18n::t(&current_lang.read(), \"preview_model_b\")}"
                                                }
                                                button {
                                                    class: if *prev_nr_style.read() == 2 { "ol-model selected" } else { "ol-model" },
                                                    onclick: move |_| prev_nr_style.set(2),
                                                    "{crate::core::i18n::t(&current_lang.read(), \"preview_model_c\")}"
                                                }
                                            }
                                        }
                                        section {
                                            h4 { "{crate::core::i18n::t(&current_lang.read(), \"preview_more_controls\")}" }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_overall_intensity\")}" }
                                                input {
                                                    type: "range", min: "0", max: "2", step: "0.05",
                                                    value: "{prev_overall_intensity.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_overall_intensity.set(v); }
                                                }
                                                output { "{prev_overall_intensity.read():.2}" }
                                            }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_local_tone\")}" }
                                                input {
                                                    type: "range", min: "0", max: "2", step: "0.05",
                                                    value: "{prev_local_tone.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_local_tone.set(v); }
                                                }
                                                output { "{prev_local_tone.read():.2}" }
                                            }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_diffuse_white\")}" }
                                                input {
                                                    type: "range", min: "80", max: "1000", step: "1",
                                                    value: "{prev_diffuse_white.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_diffuse_white.set(v); }
                                                }
                                                output { "{prev_diffuse_white.read():.0} nits" }
                                            }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_motion_x\")}" }
                                                input {
                                                    type: "range", min: "0", max: "2", step: "0.05",
                                                    value: "{prev_motion_x.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_motion_x.set(v); }
                                                }
                                                output { "{prev_motion_x.read():.2}" }
                                            }
                                            div { class: "ol-slider",
                                                span { "{crate::core::i18n::t(&current_lang.read(), \"preview_motion_y\")}" }
                                                input {
                                                    type: "range", min: "0", max: "2", step: "0.05",
                                                    value: "{prev_motion_y.read()}",
                                                    oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { prev_motion_y.set(v); }
                                                }
                                                output { "{prev_motion_y.read():.2}" }
                                            }
                                        }
                                        footer { "{crate::core::i18n::t(&current_lang.read(), \"preview_footer\")}" }
                                    }
                                }
                                div { style: "display:flex; justify-content:flex-end; gap:8px; margin-top:12px;",
                                    button {
                                        class: "ghost sm",
                                        onclick: move |_| overlay_preview_open.set(false),
                                        "{crate::core::i18n::t(&current_lang.read(), \"btn_close\")}"
                                    }
                                    button {
                                        class: "glass-btn sm",
                                        onclick: move |_| {
                                            overlay_theme.set(p_id_apply.clone());
                                            let mut s = load_state();
                                            s.overlay_theme = p_id_apply.clone();
                                            let _ = save_state(&s);
                                            crate::core::overlay_bridge::sync_overlay_preferences(&s);
                                            overlay_preview_open.set(false);
                                        },
                                        "{crate::core::i18n::t(&current_lang.read(), \"btn_apply_theme\")}"
                                    }
                                }
                            }
                        }
                    }
                }
            }


            // Toast feedback
            if *copy_toast.read() {
                div {
                    class: "copy-feedback",
                    style: "position:fixed; bottom:24px; right:24px; background:var(--accent); color:#fff; padding:8px 16px; border-radius:6px; z-index:99999;",
                    onclick: move |_| copy_toast.set(false),
                    "{copy_toast_text}"
                }
            }
        }
    }
}

pub fn launch_game(game: &GameEntry) {
    touch(&game.dir.to_string_lossy());
    log_message(&format!("@{{log_game_launched|{}}}", game.name));
    let target = if game.exe_path.exists() && game.exe_path.is_file() {
        game.exe_path.clone()
    } else {
        game.dir.clone()
    };

    if target.is_file() {
        let parent = target.parent().unwrap_or(&game.dir);
        let res = std::process::Command::new(&target)
            .current_dir(parent)
            .spawn();
        if res.is_err() {
            let _ = std::process::Command::new("explorer")
                .arg(&target)
                .spawn();
        }
    } else {
        let _ = std::process::Command::new("explorer")
            .arg(&game.dir)
            .spawn();
    }
}

pub fn pick_and_set_cover(dir: &std::path::Path, mut games: Signal<Vec<GameEntry>>) {
    let dir = dir.to_path_buf();
    spawn(async move {
        if let Some(handle) = rfd::AsyncFileDialog::new()
            .set_title("Select Cover Image")
            .add_filter("Images (*.jpg, *.png, *.webp)", &["jpg", "jpeg", "png", "webp", "bmp"])
            .pick_file()
            .await
        {
            let file_path = handle.path().to_path_buf();
            if let Ok(bytes) = std::fs::read(&file_path) {
                let art_dir = crate::core::state::get_appdata_dir().join("art");
                let _ = std::fs::create_dir_all(&art_dir);
                let key = crate::core::steamart::key_for_dir(&dir);
                let cover_file = art_dir.join(format!("{}-cover.jpg", key));
                let _ = std::fs::write(&cover_file, &bytes);
                let uri = format!("http://dlss-art.localhost/art/{}-cover.jpg", key);

                // Update games signal so UI refreshes immediately
                let mut cur = games.read().clone();
                for g in cur.iter_mut() {
                    if g.dir == dir {
                        g.poster = Some(uri.clone());
                    }
                }
                games.set(cur);

                // Update persistent cached_games in state
                let mut s = load_state();
                for g in s.cached_games.iter_mut() {
                    if g.dir == dir {
                        g.poster = Some(uri.clone());
                    }
                }
                let _ = save_state(&s);
                log_message(&format!("@{{log_custom_cover_updated|{}}}", dir.file_name().unwrap_or_default().to_string_lossy()));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_display_title_formats() {
        assert_eq!(clean_display_title("Cyberpunk_2077"), "Cyberpunk 2077");
        assert_eq!(clean_display_title("The-Witcher-3"), "The Witcher 3");
        assert_eq!(clean_display_title("Package.Name.GameTitle_1.0_x64"), "Game Title");
        assert_eq!(clean_display_title("Microsoft.FlightSimulator_1.37.19.0_x64__8wekyb3d8bbwe"), "Flight Simulator");
        assert_eq!(clean_display_title("SimpleTitle"), "SimpleTitle");
    }

    #[test]
    fn test_resolve_game_title_logic() {
        let games = vec![
            GameEntry {
                name: "Baldur's Gate 3".to_string(),
                dir: std::path::PathBuf::from("C:\\Games\\Baldurs Gate 3"),
                ..Default::default()
            }
        ];

        let row1 = HistoryRow {
            game_name: Some("Direct Name".to_string()),
            dir: "C:\\Games\\Unknown".to_string(),
            ..Default::default()
        };
        assert_eq!(resolve_game_title(&row1, &games), "Direct Name");

        let row2 = HistoryRow {
            game_name: None,
            dir: "C:\\Games\\Baldurs Gate 3".to_string(),
            ..Default::default()
        };
        assert_eq!(resolve_game_title(&row2, &games), "Baldur's Gate 3");

        let row3 = HistoryRow {
            game_name: None,
            dir: "C:\\Games\\Starfield\\Content".to_string(),
            ..Default::default()
        };
        assert_eq!(resolve_game_title(&row3, &[]), "Starfield");

        let row4 = HistoryRow {
            game_name: None,
            dir: "C:\\Games\\Cyberpunk\\Binaries\\Win64".to_string(),
            ..Default::default()
        };
        assert_eq!(resolve_game_title(&row4, &[]), "Cyberpunk");
    }

    #[test]
    fn test_app_virtual_dom_headless_render() {
        let mut dom = VirtualDom::new(App);
        dom.rebuild_in_place();
    }

    #[test]
    fn test_app_virtual_dom_all_views_and_modals() {
        let _state_lock = crate::core::state::STATE_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = load_state();
        let old_games = s.cached_games.clone();
        s.cached_games = vec![
            GameEntry {
                name: "Cyberpunk 2077".to_string(),
                dir: std::path::PathBuf::from("C:\\Games\\Cyberpunk 2077"),
                exe_path: std::path::PathBuf::from("C:\\Games\\Cyberpunk 2077\\bin\\x64\\Cyberpunk2077.exe"),
                launcher: "Steam".to_string(),
                bitness: 64,
                api: "DirectX 12".to_string(),
                dlss_version: Some("3.7.0.0".to_string()),
                has_frame_generation: true,
                can_inject_fg: false,
                ..Default::default()
            },
            GameEntry {
                name: "Baldur's Gate 3".to_string(),
                dir: std::path::PathBuf::from("C:\\Games\\Baldurs Gate 3"),
                exe_path: std::path::PathBuf::from("C:\\Games\\Baldurs Gate 3\\bin\\bg3_dx11.exe"),
                launcher: "GOG".to_string(),
                bitness: 64,
                api: "DirectX 11".to_string(),
                dlss_version: Some("2.4.2.0".to_string()),
                has_frame_generation: false,
                can_inject_fg: true,
                ..Default::default()
            },
        ];
        let _ = save_state(&s);

        // 1. Home view
        std::env::set_var("DLSS_TEST_VIEW", "home");
        let mut dom_home = VirtualDom::new(App);
        dom_home.rebuild_in_place();

        // 2. Games library view
        std::env::set_var("DLSS_TEST_VIEW", "games");
        let mut dom_games = VirtualDom::new(App);
        dom_games.rebuild_in_place();

        // 3. Add-ons view
        std::env::set_var("DLSS_TEST_VIEW", "addons");
        let mut dom_addons = VirtualDom::new(App);
        dom_addons.rebuild_in_place();

        // 4. History view
        std::env::set_var("DLSS_TEST_VIEW", "history");
        let mut dom_hist = VirtualDom::new(App);
        dom_hist.rebuild_in_place();

        // 5. Settings view
        std::env::set_var("DLSS_TEST_VIEW", "settings");
        let mut dom_sett = VirtualDom::new(App);
        dom_sett.rebuild_in_place();

        // 6. About view
        std::env::set_var("DLSS_TEST_VIEW", "about");
        let mut dom_about = VirtualDom::new(App);
        dom_about.rebuild_in_place();

        // 7. Game detail sheet modal
        std::env::set_var("DLSS_TEST_VIEW", "games");
        std::env::set_var("DLSS_TEST_SHEET", "1");
        let mut dom_sheet = VirtualDom::new(App);
        dom_sheet.rebuild_in_place();
        std::env::remove_var("DLSS_TEST_SHEET");

        // 8. RenoDX Overlay preview modal
        std::env::set_var("DLSS_TEST_PREVIEW", "1");
        let mut dom_prev = VirtualDom::new(App);
        dom_prev.rebuild_in_place();
        std::env::remove_var("DLSS_TEST_PREVIEW");

        // 9. Hotkey modal
        std::env::set_var("DLSS_TEST_HOTKEY", "1");
        let mut dom_hk = VirtualDom::new(App);
        dom_hk.rebuild_in_place();
        std::env::remove_var("DLSS_TEST_HOTKEY");

        // Clean up env and restore state
        std::env::remove_var("DLSS_TEST_VIEW");
        s.cached_games = old_games;
        let _ = save_state(&s);
    }
}

