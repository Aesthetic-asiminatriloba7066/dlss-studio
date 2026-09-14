#![windows_subsystem = "windows"]

use base64::Engine;
use dioxus::desktop::tao::window::Icon as TaoIcon;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

const CREATE_NO_WINDOW: u32 = 0x08000000;

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/installer_payload.bin"));
static APP_ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

#[derive(Clone, Copy, PartialEq)]
enum SetupPhase {
    Config,
    Installing,
    Complete,
    Error,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--uninstall") {
        let silent = args.iter().any(|a| a == "--silent" || a == "/qn" || a == "-s");
        perform_native_uninstall(silent);
        return;
    }

    let icon = TaoIcon::from_rgba(include_bytes!("../../assets/icon_64.rgba").to_vec(), 64, 64).ok();

    let mut window = WindowBuilder::new()
        .with_title("DLSS 5 Studio Setup")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_inner_size(LogicalSize::new(780.0, 560.0));

    if let Some(ic) = icon {
        window = window.with_window_icon(Some(ic));
    }

    let cfg = Config::new()
        .with_window(window)
        .with_custom_head(
            r#"<meta charset="utf-8" />
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; user-select: none; }
  body {
    background: transparent;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, "Helvetica Neue", sans-serif;
    color: #e5e7eb;
    overflow: hidden;
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }
  .app-bg-wrapper {
    position: fixed;
    inset: 0;
    z-index: 0;
    -webkit-app-region: drag;
    background:
      radial-gradient(ellipse at 50% 30%, rgba(45, 25, 16, 0.75) 0%, rgba(14, 10, 8, 0.94) 80%),
      linear-gradient(135deg, #18110c 0%, #0d0907 100%);
  }
  .app-bg-wrapper::after {
    content: "";
    position: absolute;
    inset: 0;
    opacity: 0.18;
    background-image: radial-gradient(#d97706 0.75px, transparent 0.75px);
    background-size: 16px 16px;
    pointer-events: none;
  }
  .setup-card {
    position: relative;
    z-index: 1;
    width: 620px;
    max-width: 90vw;
    max-height: 94vh;
    overflow-y: auto;
    background: rgba(22, 16, 13, 0.92);
    border: 1px solid rgba(217, 119, 6, 0.65);
    border-radius: 18px;
    padding: 28px 36px;
    box-shadow:
      0 0 28px rgba(217, 119, 6, 0.38),
      0 0 50px rgba(217, 119, 6, 0.16),
      0 24px 60px rgba(0, 0, 0, 0.9),
      inset 0 0 1px rgba(251, 191, 36, 0.45);
    backdrop-filter: blur(28px);
    -webkit-backdrop-filter: blur(28px);
  }
  .drag-header {
    -webkit-app-region: drag;
    display: flex;
    align-items: center;
    margin-bottom: 24px;
  }
  .no-drag {
    -webkit-app-region: no-drag;
  }
  .title-group {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .app-badge {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    object-fit: cover;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
    border: 1px solid rgba(217, 119, 6, 0.4);
    flex-shrink: 0;
  }
  .app-title {
    font-size: 21px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #f9fafb;
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.6);
  }
  .btn-close {
    position: fixed;
    top: 14px;
    right: 14px;
    z-index: 100;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: #9ca3af;
    background: rgba(30, 20, 16, 0.6);
    border: 1px solid rgba(217, 119, 6, 0.25);
    transition: all 0.15s ease;
  }
  .btn-close:hover {
    color: #fee2e2;
    background: rgba(239, 68, 68, 0.85);
    border-color: rgba(239, 68, 68, 1);
  }
  .section-label {
    font-size: 13px;
    font-weight: 600;
    color: #d1d5db;
    margin-bottom: 8px;
    letter-spacing: 0.02em;
  }
  .path-row {
    display: flex;
    gap: 10px;
    margin-bottom: 20px;
  }
  .path-input {
    flex: 1;
    background: rgba(14, 10, 8, 0.85);
    border: 1px solid rgba(217, 119, 6, 0.35);
    border-radius: 8px;
    padding: 9px 14px;
    color: #f3f4f6;
    font-size: 13px;
    outline: none;
    transition: border-color 0.2s;
  }
  .path-input:focus {
    border-color: #d97706;
  }
  .path-input[readonly] {
    cursor: default;
  }
  .path-input-warning {
    border-color: rgba(245, 158, 11, 0.75) !important;
    box-shadow: 0 0 10px rgba(245, 158, 11, 0.25);
  }
  .btn-browse {
    background: rgba(45, 25, 16, 0.8);
    border: 1px solid rgba(217, 119, 6, 0.4);
    border-radius: 8px;
    padding: 0 16px;
    color: #f3f4f6;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }
  .btn-browse:hover {
    background: rgba(217, 119, 6, 0.25);
    border-color: #d97706;
  }
  .prefs-group {
    background: rgba(14, 10, 8, 0.6);
    border: 1px solid rgba(217, 119, 6, 0.2);
    border-radius: 10px;
    padding: 10px 14px;
    margin-bottom: 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .pref-item {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
    font-size: 13px;
    color: #e5e7eb;
  }
  .chk-box {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    border: 1px solid rgba(217, 119, 6, 0.45);
    background: rgba(14, 10, 8, 0.9);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
  }
  .chk-box.checked {
    background: #d97706;
    border-color: #f59e0b;
  }
  .chk-icon {
    font-size: 11px;
    color: #ffffff;
    display: none;
  }
  .chk-box.checked .chk-icon {
    display: block;
  }
  .advanced-toggle-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 16px;
    cursor: pointer;
    user-select: none;
    width: fit-content;
    padding: 2px 4px;
    border-radius: 4px;
    transition: opacity 0.2s ease;
  }
  .advanced-toggle-row:hover .advanced-toggle-label {
    color: #f59e0b;
  }
  .advanced-chevron {
    font-size: 13px;
    color: #d97706;
    display: inline-block;
    transition: transform 0.2s ease;
  }
  .advanced-chevron.open {
    transform: rotate(90deg);
  }
  .advanced-toggle-label {
    font-size: 13px;
    font-weight: 600;
    color: #d1d5db;
    letter-spacing: 0.02em;
    transition: color 0.2s;
  }
  .advanced-panel {
    background: rgba(14, 10, 8, 0.55);
    border: 1px solid rgba(217, 119, 6, 0.25);
    border-radius: 10px;
    padding: 12px 14px;
    margin-bottom: 20px;
  }
  .helper-text {
    font-size: 11px;
    color: #9ca3af;
    margin-top: 4px;
  }
  .warning-banner {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    background: rgba(245, 158, 11, 0.1);
    border: 1px solid rgba(245, 158, 11, 0.45);
    border-radius: 8px;
    padding: 8px 12px;
    margin-top: 10px;
  }
  .warning-icon {
    font-size: 15px;
    color: #f59e0b;
    line-height: 1.2;
    flex-shrink: 0;
  }
  .warning-msg {
    font-size: 12px;
    line-height: 1.4;
    color: #fbbf24;
  }
  .actions-row {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
  }
  .btn-install {
    background: linear-gradient(135deg, #d97706 0%, #b45309 100%);
    border: 1px solid #f59e0b;
    border-radius: 8px;
    padding: 10px 24px;
    color: #ffffff;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 4px 14px rgba(217, 119, 6, 0.35);
    transition: all 0.2s;
  }
  .btn-install:hover {
    background: linear-gradient(135deg, #f59e0b 0%, #d97706 100%);
    box-shadow: 0 6px 20px rgba(217, 119, 6, 0.5);
    transform: translateY(-1px);
  }
  .btn-cancel {
    background: transparent;
    border: 1px solid rgba(156, 163, 175, 0.3);
    border-radius: 8px;
    padding: 10px 20px;
    color: #9ca3af;
    font-size: 14px;
    cursor: pointer;
    transition: all 0.2s;
  }
  .btn-cancel:hover {
    color: #f3f4f6;
    border-color: rgba(209, 213, 219, 0.6);
  }
  .progress-wrap {
    margin: 28px 0;
  }
  .progress-bar-bg {
    width: 100%;
    height: 8px;
    background: rgba(14, 10, 8, 0.9);
    border-radius: 4px;
    overflow: hidden;
    border: 1px solid rgba(217, 119, 6, 0.2);
    margin-bottom: 10px;
  }
  .progress-bar-fill {
    height: 100%;
    background: linear-gradient(90deg, #d97706, #fbbf24);
    box-shadow: 0 0 10px rgba(245, 158, 11, 0.5);
    transition: width 0.3s ease;
  }
  .status-text {
    font-size: 13px;
    color: #d1d5db;
    text-align: center;
  }
</style>
"#.to_string(),
        );

    LaunchBuilder::desktop().with_cfg(cfg).launch(SetupApp);
}

fn is_protected_directory<P: AsRef<Path>>(path: P) -> bool {
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

fn compute_default_storage_path(install_dir: &Path) -> String {
    if is_protected_directory(install_dir) {
        if let Ok(pd) = std::env::var("ProgramData") {
            format!("{}\\dlss-5-studio", pd)
        } else {
            r"C:\ProgramData\dlss-5-studio".to_string()
        }
    } else {
        install_dir.join("data").to_string_lossy().to_string()
    }
}

#[component]
fn SetupApp() -> Element {
    let mut phase = use_signal(|| SetupPhase::Config);
    let mut progress = use_signal(|| 0);
    let mut status_msg = use_signal(|| "Preparing setup...".to_string());
    let mut error_msg = use_signal(|| String::new());

    let default_path = r"C:\DLSS 5 Studio".to_string();
    let initial_storage = compute_default_storage_path(Path::new(&default_path));

    let mut install_path = use_signal(move || default_path);
    let mut storage_path = use_signal(move || initial_storage);
    let mut storage_manually_customized = use_signal(|| false);
    let mut show_advanced = use_signal(|| false);
    let mut startup_on_boot = use_signal(|| true);
    let mut run_in_background = use_signal(|| true);
    let mut create_desktop_shortcut = use_signal(|| true);

    let icon_data_uri = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(APP_ICON_PNG)
    );

    rsx! {
        div { class: "app-bg-wrapper" }
        button {
            class: "btn-close no-drag",
            title: "Exit Setup",
            onclick: move |_| {
                dioxus::desktop::window().close();
            },
            svg { style: "width: 16px; height: 16px; fill: currentColor;", view_box: "0 0 24 24",
                path { d: "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" }
            }
        }
        div { class: "setup-card",
            div { class: "drag-header",
                div { class: "title-group",
                    img { class: "app-badge", src: "{icon_data_uri}" }
                    span { class: "app-title", "DLSS 5 STUDIO" }
                }
            }

            match phase() {
                SetupPhase::Config => rsx! {
                    div {
                        // Installation Location
                        div { class: "section-label", "Installation Location" }
                        div { class: "path-row", style: if is_protected_directory(Path::new(&install_path())) { "margin-bottom: 6px;" } else { "margin-bottom: 20px;" },
                            input {
                                class: if is_protected_directory(Path::new(&install_path())) { "path-input path-input-warning no-drag" } else { "path-input no-drag" },
                                r#type: "text",
                                value: "{install_path()}",
                                readonly: true,
                            }
                            button {
                                class: "btn-browse no-drag",
                                onclick: move |_| {
                                    spawn(async move {
                                        if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                                            let path = folder.path();
                                            let chosen = if path.file_name().map(|n| n.to_string_lossy().to_lowercase()) == Some("dlss 5 studio".to_string()) {
                                                path.to_path_buf()
                                            } else {
                                                path.join("DLSS 5 Studio")
                                            };
                                            let chosen_str = chosen.to_string_lossy().to_string();
                                            install_path.set(chosen_str);

                                            if !storage_manually_customized() {
                                                storage_path.set(compute_default_storage_path(&chosen));
                                            }
                                        }
                                    });
                                },
                                "Browse..."
                            }
                        }

                        if is_protected_directory(Path::new(&install_path())) {
                            div { class: "warning-banner", style: "margin-top: 0; margin-bottom: 20px;",
                                span { class: "warning-icon", "⚠" }
                                span { class: "warning-msg",
                                    "Warning: This location is protected by Windows. DLSS Studio will require administrator privileges to install and write to this directory."
                                }
                            }
                        }

                        // System Preferences
                        div { class: "section-label", "System Preferences" }
                        div { class: "prefs-group no-drag",
                            div {
                                class: "pref-item",
                                onclick: move |_| startup_on_boot.set(!startup_on_boot()),
                                div { class: if startup_on_boot() { "chk-box checked" } else { "chk-box" },
                                    span { class: "chk-icon", "✓" }
                                }
                                span { "Launch automatically when Windows starts" }
                            }
                            div {
                                class: "pref-item",
                                onclick: move |_| run_in_background.set(!run_in_background()),
                                div { class: if run_in_background() { "chk-box checked" } else { "chk-box" },
                                    span { class: "chk-icon", "✓" }
                                }
                                span { "Keep running in the background when closed" }
                            }
                            div {
                                class: "pref-item",
                                onclick: move |_| create_desktop_shortcut.set(!create_desktop_shortcut()),
                                div { class: if create_desktop_shortcut() { "chk-box checked" } else { "chk-box" },
                                    span { class: "chk-icon", "✓" }
                                }
                                span { "Create desktop shortcut" }
                            }
                        }

                        // Advanced Options expandable toggle
                        div {
                            class: "advanced-toggle-row no-drag",
                            onclick: move |_| show_advanced.set(!show_advanced()),
                            span {
                                class: if show_advanced() { "advanced-chevron open" } else { "advanced-chevron" },
                                "▸"
                            }
                            span { class: "advanced-toggle-label", "Advanced Options" }
                        }

                        if show_advanced() {
                            div { class: "advanced-panel no-drag",
                                div { class: "section-label", "Data & Backups Storage Location" }
                                div { class: "path-row", style: "margin-bottom: 6px;",
                                    input {
                                        class: if is_protected_directory(Path::new(&storage_path())) { "path-input path-input-warning no-drag" } else { "path-input no-drag" },
                                        r#type: "text",
                                        value: "{storage_path()}",
                                        readonly: true,
                                    }
                                    button {
                                        class: "btn-browse no-drag",
                                        onclick: move |_| {
                                            spawn(async move {
                                                if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                                                    let path = folder.path().to_string_lossy().to_string();
                                                    storage_path.set(path);
                                                    storage_manually_customized.set(true);
                                                }
                                            });
                                        },
                                        "Browse..."
                                    }
                                }
                                div { class: "helper-text", "Stores downloaded DLSS/MFG models, shaders, and original game backups" }

                                if is_protected_directory(Path::new(&storage_path())) {
                                    div { class: "warning-banner",
                                        span { class: "warning-icon", "⚠" }
                                        span { class: "warning-msg",
                                            "Warning: This location is protected by Windows. DLSS Studio will require administrator privileges to save models and game backups here."
                                        }
                                    }
                                }
                            }
                        }

                        // Actions
                        div { class: "actions-row no-drag",
                            button {
                                class: "btn-install",
                                onclick: move |_| {
                                    phase.set(SetupPhase::Installing);
                                    let target_folder = install_path();
                                    let target_storage = storage_path();
                                    let s_boot = startup_on_boot();
                                    let b_run = run_in_background();
                                    let d_shortcut = create_desktop_shortcut();

                                    spawn(async move {
                                        progress.set(15);
                                        status_msg.set("Preparing target installation directory...".to_string());
                                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                                        progress.set(45);
                                        status_msg.set("Extracting DLSS 5 Studio application...".to_string());

                                        let target_folder_clone = target_folder.clone();
                                        let target_storage_clone = target_storage.clone();
                                        let result = tokio::task::spawn_blocking(move || {
                                            run_installation_pipeline(target_folder_clone, target_storage_clone, s_boot, b_run, d_shortcut)
                                        }).await.unwrap_or(Err("Installation task panicked".to_string()));

                                        match result {
                                            Ok(_) => {
                                                progress.set(100);
                                                status_msg.set("Installation complete!".to_string());
                                                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                                                phase.set(SetupPhase::Complete);
                                            }
                                            Err(e) => {
                                                error_msg.set(e);
                                                phase.set(SetupPhase::Error);
                                            }
                                        }
                                    });
                                },
                                "Install Now"
                            }
                            button {
                                class: "btn-cancel",
                                onclick: move |_| { dioxus::desktop::window().close(); },
                                "Cancel"
                            }
                        }
                    }
                },
                SetupPhase::Installing => rsx! {
                    div { class: "progress-wrap",
                        div { class: "progress-bar-bg",
                            div { class: "progress-bar-fill", style: "width: {progress()}%;" }
                        }
                        div { class: "status-text", "{status_msg()}" }
                    }
                },
                SetupPhase::Complete => rsx! {
                    div { style: "padding: 10px 0; text-align: center;",
                        div { style: "color: #10b981; font-size: 40px; margin-bottom: 12px;", "✓" }
                        h2 { style: "font-size: 20px; font-weight: 700; color: #f9fafb; margin-bottom: 8px;", "Setup Completed Successfully" }
                        p { style: "font-size: 13px; color: #9ca3af; margin-bottom: 24px;", "DLSS 5 Studio has been successfully installed and configured." }
                        div { class: "actions-row no-drag", style: "justify-content: center;",
                            button {
                                class: "btn-install",
                                onclick: {
                                    let target_folder = install_path();
                                    move |_| {
                                        let exe = PathBuf::from(&target_folder).join("dlss-studio.exe");
                                        if exe.exists() {
                                            let _ = std::process::Command::new(exe).spawn();
                                        }
                                        dioxus::desktop::window().close();
                                    }
                                },
                                "Launch DLSS 5 Studio"
                            }
                            button {
                                class: "btn-cancel",
                                onclick: move |_| { dioxus::desktop::window().close(); },
                                "Finish"
                            }
                        }
                    }
                },
                SetupPhase::Error => rsx! {
                    div { style: "padding: 20px 10px; text-align: center;",
                        div { style: "color: #ef4444; font-size: 32px; margin-bottom: 12px;", "⚠" }
                        h2 { style: "font-size: 19px; font-weight: 700; color: #f3f4f6; margin-bottom: 8px;", "Installation Failed" }
                        p { style: "font-size: 13px; color: #ef4444; margin-bottom: 24px; word-break: break-word;", "{error_msg()}" }
                        div { class: "actions-row no-drag", style: "justify-content: center;",
                            button {
                                class: "btn-browse",
                                onclick: move |_| { phase.set(SetupPhase::Config); },
                                "Back to Options"
                            }
                            button {
                                class: "btn-cancel",
                                onclick: move |_| { dioxus::desktop::window().close(); },
                                "Close"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Creates a Windows shell shortcut (.lnk) using Win32 COM APIs natively
fn create_shortcut(target_exe: &Path, shortcut_path: &Path, description: &str) -> Result<(), String> {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, IPersistFile};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{Interface, HSTRING};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("CoCreateInstance ShellLink failed: {:?}", e))?;

        let target_str = target_exe.to_string_lossy().to_string();
        link.SetPath(&HSTRING::from(target_str))
            .map_err(|e| format!("SetPath failed: {:?}", e))?;

        if let Some(parent) = target_exe.parent() {
            let working_dir = parent.to_string_lossy().to_string();
            link.SetWorkingDirectory(&HSTRING::from(working_dir))
                .map_err(|e| format!("SetWorkingDirectory failed: {:?}", e))?;
        }

        link.SetDescription(&HSTRING::from(description))
            .map_err(|e| format!("SetDescription failed: {:?}", e))?;

        let persist: IPersistFile = link.cast()
            .map_err(|e| format!("Cast to IPersistFile failed: {:?}", e))?;

        let shortcut_str = shortcut_path.to_string_lossy().to_string();
        persist.Save(&HSTRING::from(shortcut_str), true)
            .map_err(|e| format!("Save shortcut failed: {:?}", e))?;

        CoUninitialize();
    }
    Ok(())
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Registers the application in Windows Installed Apps / Add or Remove Programs registry
fn register_uninstall_entry(install_dir: &Path, installed_exe: &Path) -> Result<(), String> {
    use windows::Win32::System::Registry::{
        RegCreateKeyExW, RegSetValueExW, RegCloseKey, HKEY_CURRENT_USER, KEY_WRITE, REG_SZ, REG_OPTION_NON_VOLATILE,
    };

    let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\DLSS 5 Studio");
    let mut hkey = windows::Win32::System::Registry::HKEY::default();

    unsafe {
        let res = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(subkey.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if res.is_err() {
            return Err(format!("RegCreateKeyExW failed: {:?}", res));
        }

        let write_val = |name: &str, val: &str| {
            let name_w = to_wide(name);
            let val_w = to_wide(val);
            let bytes = std::slice::from_raw_parts(val_w.as_ptr() as *const u8, val_w.len() * 2);
            let _ = RegSetValueExW(
                hkey,
                windows::core::PCWSTR(name_w.as_ptr()),
                0,
                REG_SZ,
                Some(bytes),
            );
        };

        write_val("DisplayName", "DLSS 5 Studio");
        write_val("DisplayVersion", env!("CARGO_PKG_VERSION"));
        write_val("Publisher", "Bookamp");
        write_val("DisplayIcon", &format!("{},0", installed_exe.display()));
        write_val("InstallLocation", &install_dir.display().to_string());
        
        let uninst_exe = install_dir.join("uninstall.exe");
        write_val("UninstallString", &format!("\"{}\" --uninstall", uninst_exe.display()));
        write_val("QuietUninstallString", &format!("\"{}\" --uninstall --silent", uninst_exe.display()));

        let _ = RegCloseKey(hkey);
    }
    Ok(())
}

/// Removes the application registration from Windows Installed Apps registry
fn unregister_uninstall_entry() {
    use windows::Win32::System::Registry::{RegDeleteKeyW, HKEY_CURRENT_USER};
    let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\DLSS 5 Studio");
    unsafe {
        let _ = RegDeleteKeyW(HKEY_CURRENT_USER, windows::core::PCWSTR(subkey.as_ptr()));
    }
}

/// Executes the pure native installation
fn run_installation_pipeline(
    target_folder: String,
    storage_folder: String,
    startup: bool,
    background: bool,
    desktop_shortcut: bool,
) -> Result<(), String> {
    let dest = PathBuf::from(&target_folder);
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("Could not create directory {}: {}", dest.display(), e))?;

    // Determine payload bytes:
    let payload_data: Vec<u8> = if !PAYLOAD.is_empty() {
        PAYLOAD.to_vec()
    } else {
        let candidate = Path::new("target/release/dlss-studio.exe");
        let debug_candidate = Path::new("target/debug/dlss-studio.exe");
        if candidate.exists() {
            std::fs::read(candidate).unwrap_or_default()
        } else if debug_candidate.exists() {
            std::fs::read(debug_candidate).unwrap_or_default()
        } else {
            Vec::new()
        }
    };

    if payload_data.is_empty() {
        return Err("Installation payload not found. Please compile dlss-studio first.".to_string());
    }

    let installed_exe = dest.join("dlss-studio.exe");
    std::fs::write(&installed_exe, &payload_data)
        .map_err(|e| format!("Failed to write application binary: {}", e))?;

    // Write storage configuration into installation folder
    let storage_dest = PathBuf::from(&storage_folder);
    let _ = std::fs::create_dir_all(&storage_dest);
    let storage_cfg = serde_json::json!({
        "data_dir": storage_folder
    });
    let _ = std::fs::write(
        dest.join("storage.json"),
        serde_json::to_string_pretty(&storage_cfg).unwrap_or_default(),
    );

    // Copy setup.exe as uninstall.exe in target folder
    if let Ok(curr) = std::env::current_exe() {
        let _ = std::fs::copy(&curr, dest.join("uninstall.exe"));
    }

    // Create Start Menu shortcut
    if let Ok(appdata) = std::env::var("APPDATA") {
        let start_menu = PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs");
        let _ = std::fs::create_dir_all(&start_menu);
        let _ = create_shortcut(
            &installed_exe,
            &start_menu.join("DLSS 5 Studio.lnk"),
            "DLSS 5 Studio - Native DLSS & Frame Generation Manager",
        );
    }

    // Create Desktop shortcut
    if desktop_shortcut {
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let desktop = PathBuf::from(userprofile).join("Desktop");
            let _ = create_shortcut(
                &installed_exe,
                &desktop.join("DLSS 5 Studio.lnk"),
                "DLSS 5 Studio - Native DLSS & Frame Generation Manager",
            );
        }
    }

    // Register in Windows Installed Apps
    let _ = register_uninstall_entry(&dest, &installed_exe);

    // Apply startup & background preferences
    if startup {
        let _ = std::process::Command::new(&installed_exe)
            .creation_flags(CREATE_NO_WINDOW)
            .arg("--enable-startup-only")
            .status();
    }
    if !background {
        let _ = std::process::Command::new(&installed_exe)
            .creation_flags(CREATE_NO_WINDOW)
            .arg("--disable-background-only")
            .status();
    }

    Ok(())
}

/// Silently or interactively uninstalls DLSS 5 Studio
fn perform_native_uninstall(silent: bool) {
    // 1. Terminate any running instances
    let _ = std::process::Command::new("taskkill")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/F", "/IM", "dlss-studio.exe"])
        .status();

    // 2. Remove Shortcuts
    if let Ok(appdata) = std::env::var("APPDATA") {
        let sm = PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\DLSS 5 Studio.lnk");
        let _ = std::fs::remove_file(sm);
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let dt = PathBuf::from(userprofile).join(r"Desktop\DLSS 5 Studio.lnk");
        let _ = std::fs::remove_file(dt);
    }

    // 3. Remove Windows Uninstall Registry Key
    unregister_uninstall_entry();

    // 4. Remove Windows Startup Run Key
    let _ = crate_startup_uninstall_cleanup();

    // 5. Remove Application Binary & Config
    if let Ok(curr) = std::env::current_exe() {
        if let Some(parent) = curr.parent() {
            let exe = parent.join("dlss-studio.exe");
            let _ = std::fs::remove_file(exe);
            let storage_cfg = parent.join("storage.json");
            let _ = std::fs::remove_file(storage_cfg);
        }
    }

    if !silent {
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
        let text = to_wide("DLSS 5 Studio has been successfully uninstalled from your computer.");
        let caption = to_wide("DLSS 5 Studio Uninstall");
        unsafe {
            let _ = MessageBoxW(
                None,
                windows::core::PCWSTR(text.as_ptr()),
                windows::core::PCWSTR(caption.as_ptr()),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}

fn crate_startup_uninstall_cleanup() -> Result<(), ()> {
    use windows::Win32::System::Registry::{RegOpenKeyExW, RegDeleteValueW, HKEY_CURRENT_USER, KEY_WRITE};
    let run_key = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let val_name = to_wide("DLSS5Studio");
    let mut hkey = windows::Win32::System::Registry::HKEY::default();
    unsafe {
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(run_key.as_ptr()),
            0,
            KEY_WRITE,
            &mut hkey,
        ).is_ok() {
            let _ = RegDeleteValueW(hkey, windows::core::PCWSTR(val_name.as_ptr()));
            let _ = windows::Win32::System::Registry::RegCloseKey(hkey);
        }
    }
    Ok(())
}
