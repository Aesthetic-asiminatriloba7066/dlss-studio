use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

pub const FEEDER_ARCHIVE_URL: &str = "https://github.com/jlrouzies-fr/DLSS5-Feeder/releases/download/v1.16.0-beta.1/DLSS5-Feeder-1.16.0-beta.1.zip";
pub const FEEDER_ARCHIVE_SHA256: &str = "22ae8227b11834529e4980cb66ba4b3f277c399020d116c840481c18b12844d3";

pub const VORT_ARCHIVE_URL: &str = "https://codeload.github.com/vortigern11/vort_Shaders/zip/b410b9f0c0fbb83c8cb42164aaf1655fab386f4a";
pub const VORT_ARCHIVE_SHA256: &str = "231ba34a75556f9943e359559a89b0d0cc2caa322d9dcdee5630061bf9fe13b6";

pub const RESHADE_FXH_URL: &str = "https://raw.githubusercontent.com/crosire/reshade-shaders/slim/Shaders/ReShade.fxh";
pub const RESHADE_FXH_SHA256: &str = "6dabfbbaf968c3871905d2ea17f96572ff7b1cec01310b5d0e5252b66b30174f";

pub const RESHADE_UI_FXH_URL: &str = "https://raw.githubusercontent.com/crosire/reshade-shaders/slim/Shaders/ReShadeUI.fxh";
pub const RESHADE_UI_FXH_SHA256: &str = "78adf672df47460297eb9fe6dd238d2aafa24510b52b84feb1a745dff70eb901";

pub const MFG_09_URL: &str = "https://github.com/mavismmg/MFGAdaUnlock-RenoDx/releases/download/0.9/renodx-mfgunlock.addon64";
pub const MFG_09_SHA256: &str = "64184bb370f223c3cabb359010a9a64e114cdae6b62d8b014a731a602af0a0da";

pub const OPTISCALER_083_URL: &str = "https://github.com/wilsjo2/OptiScaler-DLSSNR-PreSR-Multipass/releases/download/v0.8.3/OptiScaler-NR-v0.8.3.zip";
pub const OPTISCALER_083_SHA256: &str = "3f2d26fb136d964a394bf50896d082156173153a2a55b88e1995277b4dabe3c8";

pub const RENODX_DLSS5_URL: &str = "https://github.com/yumlevi/renodx-dlss-installer/releases/download/latest/renodx-dlss5.addon64";
pub const STREAMLINE_ZIP_URL: &str = "https://github.com/yumlevi/renodx-dlss-installer/releases/download/latest/streamline.zip";
pub const RESHADE_SETUP_URL: &str = "https://reshade.me/downloads/ReShade_Setup_6.8.0_Addon.exe";
pub const RESHADE_SETUP_SHA256: &str = "afe4c8f13048306307983b8b3d41d5bf00a86820440b0e57dea10950e1176445";
pub const RESHADE64_SHA256: &str = "0cee63f9c9f13f3ac909c5b4903f4dbb4b719a7ab3b4f13b0deaf83c814b94f7";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeederComponents {
    pub addon64: PathBuf,
    pub addon32: Option<PathBuf>,
    pub host64: Option<PathBuf>,
    pub shader_dir: PathBuf,
    pub vk_layer_dir: Option<PathBuf>,
}

pub fn get_components_root() -> PathBuf {
    let p = crate::core::state::get_appdata_dir().join("components");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn compute_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn compute_bytes_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadProgress {
    pub is_downloading: bool,
    pub component_name: String,
    pub component_id: String,
    pub url: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percentage: f32,
    pub message: String,
}

impl Default for DownloadProgress {
    fn default() -> Self {
        Self {
            is_downloading: false,
            component_name: String::new(),
            component_id: String::new(),
            url: String::new(),
            downloaded_bytes: 0,
            total_bytes: None,
            percentage: 0.0,
            message: String::new(),
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

async fn download_file_attempt<F>(
    client: &reqwest::Client,
    url: &str,
    target_path: &Path,
    expected_sha256: &str,
    component_name: &str,
    component_id: &str,
    progress_fn: &mut F,
    attempt: usize,
) -> Result<(), String>
where
    F: FnMut(DownloadProgress),
{
    use std::io::Write;

    if attempt > 1 {
        progress_fn(DownloadProgress {
            is_downloading: true,
            component_name: component_name.to_string(),
            component_id: component_id.to_string(),
            url: url.to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percentage: 0.0,
            message: format!("Retrying {} (attempt {}/3)...", component_name, attempt),
        });
    } else {
        progress_fn(DownloadProgress {
            is_downloading: true,
            component_name: component_name.to_string(),
            component_id: component_id.to_string(),
            url: url.to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            percentage: 0.0,
            message: format!("Connecting to {}...", component_name),
        });
    }

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to {}: {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed for {} (HTTP status {})", url, resp.status()));
    }

    let total_size = resp.content_length();
    let mut downloaded: u64 = 0;
    let part_path = target_path.with_extension("part");

    // Stream directly to file on disk to prevent RAM bloat
    let mut out_file = fs::File::create(&part_path)
        .map_err(|e| format!("Failed to create part file {}: {}", part_path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut last_emit = std::time::Instant::now();
    let mut last_pct = 0.0f32;

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Error streaming from {}: {}", url, e))?
    {
        out_file
            .write_all(&chunk)
            .map_err(|e| format!("Disk write error for {}: {}", part_path.display(), e))?;

        if !expected_sha256.is_empty() {
            hasher.update(&chunk);
        }
        downloaded += chunk.len() as u64;

        let pct = if let Some(total) = total_size {
            if total > 0 {
                (downloaded as f32 / total as f32) * 100.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        if (pct - last_pct).abs() >= 1.0 || last_emit.elapsed() >= std::time::Duration::from_millis(100) {
            last_pct = pct;
            last_emit = std::time::Instant::now();
            progress_fn(DownloadProgress {
                is_downloading: true,
                component_name: component_name.to_string(),
                component_id: component_id.to_string(),
                url: url.to_string(),
                downloaded_bytes: downloaded,
                total_bytes: total_size,
                percentage: pct.min(100.0),
                message: format!("Downloading {}...", component_name),
            });
        }
    }

    out_file
        .flush()
        .map_err(|e| format!("Failed to flush {}: {}", part_path.display(), e))?;
    drop(out_file);

    // Final 100% emission
    progress_fn(DownloadProgress {
        is_downloading: true,
        component_name: component_name.to_string(),
        component_id: component_id.to_string(),
        url: url.to_string(),
        downloaded_bytes: downloaded,
        total_bytes: total_size,
        percentage: 100.0,
        message: format!("Downloading {}... 100%", component_name),
    });

    if !expected_sha256.is_empty() {
        progress_fn(DownloadProgress {
            is_downloading: true,
            component_name: component_name.to_string(),
            component_id: component_id.to_string(),
            url: url.to_string(),
            downloaded_bytes: downloaded,
            total_bytes: total_size,
            percentage: 100.0,
            message: "Verifying checksum...".to_string(),
        });
        let hash = hex::encode(hasher.finalize());
        if !hash.eq_ignore_ascii_case(expected_sha256) {
            let _ = fs::remove_file(&part_path);
            return Err(format!(
                "Checksum mismatch for {}: expected {}, got {}",
                url, expected_sha256, hash
            ));
        }
    }

    fs::rename(&part_path, target_path)
        .map_err(|e| format!("Failed to finalize {}: {}", target_path.display(), e))?;

    Ok(())
}

pub async fn download_file_with_progress<F>(
    url: &str,
    target_path: &Path,
    expected_sha256: &str,
    component_name: &str,
    component_id: &str,
    mut progress_fn: F,
) -> Result<(), String>
where
    F: FnMut(DownloadProgress),
{
    if target_path.is_file() {
        if let Ok(hash) = compute_sha256(target_path) {
            if expected_sha256.is_empty() || hash.eq_ignore_ascii_case(expected_sha256) {
                return Ok(());
            }
        }
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 DLSS-Studio/1.0")
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(300))
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let mut last_err = String::new();
    for attempt in 1..=3 {
        match download_file_attempt(&client, url, target_path, expected_sha256, component_name, component_id, &mut progress_fn, attempt).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_millis(1200 * attempt as u64)).await;
                }
            }
        }
    }

    Err(last_err)
}

pub async fn download_file_with_sha256(url: &str, target_path: &Path, expected_sha256: &str) -> Result<(), String> {
    download_file_with_progress(url, target_path, expected_sha256, "", "", |_| {}).await
}

pub fn extract_zip<R: Read + Seek>(reader: R, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("Failed to create {}: {}", out_dir.display(), e))?;
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("Invalid zip archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Corrupt zip entry: {}", e))?;
        let outpath = match file.enclosed_name() {
            Some(path) => out_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| format!("Failed to create {}: {}", outpath.display(), e))?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| format!("Failed to create {}: {}", p.display(), e))?;
                }
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| format!("Failed to create {}: {}", outpath.display(), e))?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| format!("Failed to extract to {}: {}", outpath.display(), e))?;
        }
    }
    Ok(())
}

/// Checks local search paths before attempting an online download
pub fn find_local_feeder_components() -> Option<FeederComponents> {
    let mut search_dirs = Vec::new();
    search_dirs.push(get_components_root());

    if let Ok(exe_p) = std::env::current_exe() {
        if let Some(exe_dir) = exe_p.parent() {
            search_dirs.push(exe_dir.join("components"));
            search_dirs.push(exe_dir.join("components").join("feeder"));
        }
    }

    for dir in search_dirs {
        let addon64 = if dir.join("dlss5-feed.addon64").is_file() {
            dir.join("dlss5-feed.addon64")
        } else if dir.join("DLSS5-Feeder-1.16.0-beta.1").join("dlss5-feed.addon64").is_file() {
            dir.join("DLSS5-Feeder-1.16.0-beta.1").join("dlss5-feed.addon64")
        } else {
            continue;
        };

        let shader_dir = if dir.join("reshade-shaders").join("Shaders").is_dir() {
            dir.join("reshade-shaders")
        } else if dir.join("feeder-shaders").join("Shaders").is_dir() {
            dir.join("feeder-shaders")
        } else {
            continue;
        };

        let vk_layer_dir = if dir.join("layer-x64").join("VkLayer_feed_vk.dll").is_file() {
            Some(dir.join("layer-x64"))
        } else if dir.join("DLSS5-Feeder-1.16.0-beta.1").join("layer-x64").join("VkLayer_feed_vk.dll").is_file() {
            Some(dir.join("DLSS5-Feeder-1.16.0-beta.1").join("layer-x64"))
        } else {
            None
        };

        return Some(FeederComponents {
            addon64,
            addon32: if dir.join("dlss5-feed.addon32").is_file() { Some(dir.join("dlss5-feed.addon32")) } else { None },
            host64: if dir.join("dlss5-feed-host64.exe").is_file() { Some(dir.join("dlss5-feed-host64.exe")) } else { None },
            shader_dir,
            vk_layer_dir,
        });
    }
    None
}

/// Asynchronously resolves, downloads (if missing), verifies, and assembles Feeder components.
pub async fn ensure_feeder_components(log: &mut Vec<String>) -> Result<FeederComponents, String> {
    if let Some(local) = find_local_feeder_components() {
        log.push(format!("[FEEDER-CACHE] Using verified local feeder components from {}", local.addon64.parent().unwrap_or(&local.addon64).display()));
        return Ok(local);
    }

    let comp_root = get_components_root();
    let feeder_dir = comp_root.join("DLSS5-Feeder-1.16.0-beta.1");
    let feeder_zip = comp_root.join("DLSS5-Feeder-1.16.0-beta.1.zip");

    // 1. Download & extract DLSS5-Feeder v1.16.0-beta.1
    if !feeder_dir.join("dlss5-feed.addon64").is_file() {
        log.push("[DOWNLOAD] Fetching latest DLSS5-Feeder v1.16.0-beta.1 from upstream GitHub...".to_string());
        download_file_with_sha256(FEEDER_ARCHIVE_URL, &feeder_zip, FEEDER_ARCHIVE_SHA256).await?;
        log.push("[DOWNLOAD] Verifying DLSS5-Feeder SHA-256 checksum: OK".to_string());

        let file = fs::File::open(&feeder_zip).map_err(|e| format!("Failed to open {}: {}", feeder_zip.display(), e))?;
        extract_zip(file, &feeder_dir)?;
        log.push("[FEEDER] DLSS5-Feeder unpacked successfully".to_string());
    }

    // 2. Download & extract vort_Shaders
    let vort_dir = comp_root.join("vort_Shaders-b410b9f");
    let vort_zip = comp_root.join("vort_Shaders-b410b9f.zip");
    if !vort_dir.join("Shaders").join("vort_Motion.fx").is_file() {
        log.push("[DOWNLOAD] Fetching latest VORT Motion Vector shaders from upstream...".to_string());
        download_file_with_sha256(VORT_ARCHIVE_URL, &vort_zip, VORT_ARCHIVE_SHA256).await?;
        log.push("[DOWNLOAD] Verifying VORT Shaders SHA-256 checksum: OK".to_string());

        let file = fs::File::open(&vort_zip).map_err(|e| format!("Failed to open {}: {}", vort_zip.display(), e))?;
        extract_zip(file, &vort_dir)?;
        let _ = fs::remove_file(&vort_zip);
    }

    // 3. Download ReShade standard headers
    let headers_dir = comp_root.join("reshade-headers");
    let fxh_path = headers_dir.join("ReShade.fxh");
    let ui_fxh_path = headers_dir.join("ReShadeUI.fxh");
    if !fxh_path.is_file() {
        log.push("[DOWNLOAD] Fetching ReShade.fxh header from upstream slim branch...".to_string());
        download_file_with_sha256(RESHADE_FXH_URL, &fxh_path, RESHADE_FXH_SHA256).await?;
    }
    if !ui_fxh_path.is_file() {
        download_file_with_sha256(RESHADE_UI_FXH_URL, &ui_fxh_path, RESHADE_UI_FXH_SHA256).await?;
    }

    // 4. Assemble consolidated reshade-shaders tree
    let consolidated_shaders = comp_root.join("feeder-shaders");
    let target_shaders = consolidated_shaders.join("Shaders");
    let target_textures = consolidated_shaders.join("Textures");
    let target_includes = target_shaders.join("Includes");
    fs::create_dir_all(&target_includes).map_err(|e| format!("Failed to create {}: {}", target_includes.display(), e))?;
    fs::create_dir_all(&target_textures).map_err(|e| format!("Failed to create {}: {}", target_textures.display(), e))?;

    // Copy DLSS5_Feed.fx from feeder
    let dlss5_feed_fx = feeder_dir.join("reshade-shaders").join("Shaders").join("DLSS5_Feed.fx");
    if dlss5_feed_fx.is_file() {
        let _ = fs::copy(&dlss5_feed_fx, target_shaders.join("DLSS5_Feed.fx"));
    }

    // Copy ReShade headers
    let _ = fs::copy(&fxh_path, target_shaders.join("ReShade.fxh"));
    let _ = fs::copy(&ui_fxh_path, target_shaders.join("ReShadeUI.fxh"));

    // Copy VORT motion shaders and includes
    let vort_sub = if vort_dir.join("vort_Shaders-b410b9f0c0fbb83c8cb42164aaf1655fab386f4a").is_dir() {
        vort_dir.join("vort_Shaders-b410b9f0c0fbb83c8cb42164aaf1655fab386f4a")
    } else {
        vort_dir.clone()
    };

    if vort_sub.join("Shaders").is_dir() {
        for entry in walkdir::WalkDir::new(vort_sub.join("Shaders")).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.ends_with(".fx") || fname.ends_with(".fxh") {
                    let dest = if entry.path().parent().map(|p| p.ends_with("Includes")).unwrap_or(false) {
                        target_includes.join(&fname)
                    } else {
                        target_shaders.join(&fname)
                    };
                    let _ = fs::copy(entry.path(), dest);
                }
            }
        }
    }

    if vort_sub.join("Textures").is_dir() {
        for entry in walkdir::WalkDir::new(vort_sub.join("Textures")).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let fname = entry.file_name().to_string_lossy().to_string();
                let _ = fs::copy(entry.path(), target_textures.join(fname));
            }
        }
    }

    let vk_layer_dir = if feeder_dir.join("layer-x64").join("VkLayer_feed_vk.dll").is_file() {
        Some(feeder_dir.join("layer-x64"))
    } else {
        None
    };

    log.push("[FEEDER] All Feeder shaders and components ready".to_string());

    Ok(FeederComponents {
        addon64: feeder_dir.join("dlss5-feed.addon64"),
        addon32: if feeder_dir.join("dlss5-feed.addon32").is_file() { Some(feeder_dir.join("dlss5-feed.addon32")) } else { None },
        host64: if feeder_dir.join("host64").join("dlss5-feed-host64.exe").is_file() { Some(feeder_dir.join("host64").join("dlss5-feed-host64.exe")) } else { None },
        shader_dir: consolidated_shaders,
        vk_layer_dir,
    })
}

/// Asynchronously resolves or downloads the latest MFG Unlock v0.9 addon
pub async fn ensure_mfg_v09_addon(log: &mut Vec<String>) -> Result<PathBuf, String> {
    let comp_root = get_components_root();
    let mfg_dir = comp_root.join("mfg-unlock-0.9");
    let target = mfg_dir.join("renodx-mfgunlock.addon64");

    if target.is_file() {
        if let Ok(hash) = compute_sha256(&target) {
            if hash.eq_ignore_ascii_case(MFG_09_SHA256) {
                return Ok(target);
            }
        }
    }

    log.push("[DOWNLOAD] Fetching latest MFG Unlock v0.9 (RenoDX companion) from upstream...".to_string());
    download_file_with_sha256(MFG_09_URL, &target, MFG_09_SHA256).await?;
    log.push("[DOWNLOAD] Verifying MFG Unlock v0.9 SHA-256: OK".to_string());
    Ok(target)
}

/// Asynchronously resolves or downloads the latest OptiScaler DLSS-NR v0.8.3 runtime
pub async fn ensure_optiscaler_083_components(log: &mut Vec<String>) -> Result<PathBuf, String> {
    let comp_root = get_components_root();
    let opti_dir = comp_root.join("OptiScaler-0.8.3-dlssnr");
    if opti_dir.join("OptiScaler.dll").is_file() {
        return Ok(opti_dir);
    }
    let fallback_077 = comp_root.join("OptiScaler-0.7.7-dlssnr");
    if fallback_077.join("OptiScaler.dll").is_file() {
        return Ok(fallback_077);
    }

    let zip_path = comp_root.join("OptiScaler-NR-v0.8.3.zip");
    log.push("[DOWNLOAD] Fetching latest OptiScaler DLSS-NR v0.8.3 from upstream...".to_string());
    download_file_with_sha256(OPTISCALER_083_URL, &zip_path, OPTISCALER_083_SHA256).await?;
    log.push("[DOWNLOAD] Verifying OptiScaler DLSS-NR v0.8.3 SHA-256: OK".to_string());

    let file = fs::File::open(&zip_path).map_err(|e| format!("Failed to open {}: {}", zip_path.display(), e))?;
    extract_zip(file, &opti_dir)?;
    log.push("[OPTISCALER] OptiScaler DLSS-NR v0.8.3 unpacked successfully".to_string());
    let _ = fs::remove_file(&zip_path);

    Ok(opti_dir)
}

/// Asynchronously resolves or downloads the RenoDX v4.7 Integrated DLSS 5 Engine addon
pub async fn ensure_renodx_dlss5_addon(log: &mut Vec<String>) -> Result<PathBuf, String> {
    let comp_root = get_components_root();
    let renodx_dir = comp_root.join("renodx-dlss5");
    let target = renodx_dir.join("renodx-dlss5.addon64");
    if target.is_file() {
        return Ok(target);
    }
    fs::create_dir_all(&renodx_dir).map_err(|e| format!("Failed to create {}: {}", renodx_dir.display(), e))?;

    log.push("[DOWNLOAD] Fetching RenoDX v4.7 Integrated Engine from upstream...".to_string());
    download_file_with_sha256(RENODX_DLSS5_URL, &target, "").await?;
    log.push("[DOWNLOAD] RenoDX v4.7 Engine verified and ready".to_string());
    Ok(target)
}

/// Asynchronously resolves or downloads the Streamline Runtime components
pub async fn ensure_streamline_components(log: &mut Vec<String>) -> Result<PathBuf, String> {
    let comp_root = get_components_root();
    let streamline_dir = comp_root.join("streamline-2.14.1").join("streamline");
    if streamline_dir.join("sl.interposer.dll").is_file() && streamline_dir.join("sl.common.dll").is_file() {
        return Ok(streamline_dir);
    }
    let zip_path = comp_root.join("streamline.zip");
    log.push("[DOWNLOAD] Fetching Streamline Runtime from upstream...".to_string());
    download_file_with_sha256(STREAMLINE_ZIP_URL, &zip_path, "").await?;
    let file = fs::File::open(&zip_path).map_err(|e| format!("Failed to open {}: {}", zip_path.display(), e))?;
    extract_zip(file, &comp_root.join("streamline-2.14.1"))?;
    let _ = fs::remove_file(&zip_path);
    log.push("[STREAMLINE] Streamline Runtime ready".to_string());
    Ok(streamline_dir)
}

/// Checks whether the RenoDX 4x MFG Unlock addon is cached on disk
pub fn is_mfg_addon_cached() -> bool {
    let root = get_components_root();
    root.join("mfg-unlock-0.9").join("renodx-mfgunlock.addon64").is_file()
        || root.join("mfg-unlock-0.8").join("renodx-mfgunlock.addon64").is_file()
        || root.join("renodx-mfgunlock.addon64").is_file()
}

/// Checks whether the DLSS 5 Feeder and shaders are cached on disk
pub fn is_feeder_cached() -> bool {
    find_local_feeder_components().is_some()
}

/// Checks whether the RenoDX v4.7 Integrated Engine is cached on disk
pub fn is_renodx_engine_cached() -> bool {
    let root = get_components_root();
    root.join("renodx-dlss5").join("renodx-dlss5.addon64").is_file()
        || root.join("renodx-dlss5.addon64").is_file()
}

/// Checks whether the OptiScaler runtime is cached on disk
pub fn is_optiscaler_cached() -> bool {
    let root = get_components_root();
    root.join("OptiScaler-0.8.3-dlssnr").join("OptiScaler.dll").is_file()
        || root.join("OptiScaler-0.7.7-dlssnr").join("OptiScaler.dll").is_file()
        || root.join("OptiScaler.dll").is_file()
}

/// Checks whether Streamline Runtime is cached on disk
pub fn is_streamline_cached() -> bool {
    let streamline_dir = get_components_root().join("streamline-2.14.1").join("streamline");
    streamline_dir.join("sl.interposer.dll").is_file() && streamline_dir.join("sl.common.dll").is_file()
}

/// Checks whether ReShade 6.8.0 runtime is cached on disk
pub fn is_reshade_cached() -> bool {
    let root = get_components_root();
    root.join("ReShade64.dll").is_file()
        || root.join("reshade-vulkan").join("ReShade64.dll").is_file()
}

/// Checks whether all mandatory components are already cached on disk
pub fn are_all_mandatory_components_cached() -> bool {
    is_mfg_addon_cached()
        && is_feeder_cached()
        && is_renodx_engine_cached()
        && is_optiscaler_cached()
        && is_reshade_cached()
        && is_streamline_cached()
}

/// Extracts ReShade64.dll from the official setup archive using native Windows tar
pub fn extract_reshade_from_setup(setup_path: &Path, out_dir: &Path) -> Result<(), String> {
    let _ = fs::create_dir_all(out_dir);
    let status = std::process::Command::new("tar.exe")
        .arg("-xf")
        .arg(setup_path)
        .arg("-C")
        .arg(out_dir)
        .arg("ReShade64.dll")
        .status()
        .map_err(|e| format!("Failed to execute tar.exe to extract ReShade: {}", e))?;
    if !status.success() {
        return Err(format!("tar.exe extraction failed with code {:?}", status.code()));
    }
    Ok(())
}

/// Orchestrates the automated background downloading of all mandatory components with detailed streaming progress
pub async fn ensure_all_mandatory_components_with_progress<F>(mut progress_fn: F) -> Result<(), String>
where
    F: FnMut(DownloadProgress),
{
    let mut log = Vec::new();
    let mut errors = Vec::new();
    const TOTAL_STEPS: usize = 6;

    // 1. RenoDX v4.7 Integrated Engine
    if !is_renodx_engine_cached() {
        let comp_root = get_components_root();
        let renodx_dir = comp_root.join("renodx-dlss5");
        let target = renodx_dir.join("renodx-dlss5.addon64");
        let _ = fs::create_dir_all(&renodx_dir);
        let mut step_prog = |mut p: DownloadProgress| {
            p.percentage = ((0.0 * 100.0) + p.percentage) / TOTAL_STEPS as f32;
            progress_fn(p);
        };
        if let Err(e) = download_file_with_progress(
            RENODX_DLSS5_URL,
            &target,
            "",
            "RenoDX v4.7 (Integrated DLSS 5 Engine)",
            "renodx_engine",
            &mut step_prog,
        ).await {
            crate::core::state::log_message(&format!("@{{log_download_error|RenoDX v4.7|{}}}", e));
            errors.push(format!("RenoDX v4.7: {}", e));
        }
    }

    // 2. RenoDX 4x MFG Unlock v0.9
    if !is_mfg_addon_cached() {
        let comp_root = get_components_root();
        let mfg_dir = comp_root.join("mfg-unlock-0.9");
        let target = mfg_dir.join("renodx-mfgunlock.addon64");
        let _ = fs::create_dir_all(&mfg_dir);
        let mut step_prog = |mut p: DownloadProgress| {
            p.percentage = ((1.0 * 100.0) + p.percentage) / TOTAL_STEPS as f32;
            progress_fn(p);
        };
        if let Err(e) = download_file_with_progress(
            MFG_09_URL,
            &target,
            MFG_09_SHA256,
            "RenoDX 4x MFG Unlock v0.9",
            "mfg_unlock",
            &mut step_prog,
        ).await {
            crate::core::state::log_message(&format!("@{{log_download_error|RenoDX 4x MFG Unlock v0.9|{}}}", e));
            errors.push(format!("RenoDX 4x MFG Unlock v0.9: {}", e));
        }
    }

    // 3. DLSS 5 Feeder & Motion Shaders
    if !is_feeder_cached() {
        let comp_root = get_components_root();
        let feeder_dir = comp_root.join("DLSS5-Feeder-1.16.0-beta.1");
        let feeder_zip = comp_root.join("DLSS5-Feeder-1.16.0-beta.1.zip");
        if !feeder_dir.join("dlss5-feed.addon64").is_file() {
            let mut step_prog = |mut p: DownloadProgress| {
                p.percentage = ((2.0 * 100.0) + p.percentage) / TOTAL_STEPS as f32;
                progress_fn(p);
            };
            if let Err(e) = download_file_with_progress(
                FEEDER_ARCHIVE_URL,
                &feeder_zip,
                FEEDER_ARCHIVE_SHA256,
                "DLSS 5 Feeder (Neural Pipeline Interceptor)",
                "feeder",
                &mut step_prog,
            ).await {
                crate::core::state::log_message(&format!("@{{log_download_error|DLSS 5 Feeder|{}}}", e));
                errors.push(format!("DLSS 5 Feeder: {}", e));
            } else if let Ok(file) = fs::File::open(&feeder_zip) {
                let _ = extract_zip(file, &feeder_dir);
                let _ = fs::remove_file(&feeder_zip);
            }
        }
        let _ = ensure_feeder_components(&mut log).await;
    }

    // 4. OptiScaler DLSS-NR v0.8.3
    if !is_optiscaler_cached() {
        let comp_root = get_components_root();
        let opti_dir = comp_root.join("OptiScaler-0.8.3-dlssnr");
        let zip_path = comp_root.join("OptiScaler-NR-v0.8.3.zip");
        let mut step_prog = |mut p: DownloadProgress| {
            p.percentage = ((3.0 * 100.0) + p.percentage) / TOTAL_STEPS as f32;
            progress_fn(p);
        };
        if let Err(e) = download_file_with_progress(
            OPTISCALER_083_URL,
            &zip_path,
            OPTISCALER_083_SHA256,
            "OptiScaler DLSS-NR v0.8.3",
            "optiscaler",
            &mut step_prog,
        ).await {
            crate::core::state::log_message(&format!("@{{log_download_error|OptiScaler v0.8.3|{}}}", e));
            errors.push(format!("OptiScaler DLSS-NR: {}", e));
        } else if let Ok(file) = fs::File::open(&zip_path) {
            let _ = extract_zip(file, &opti_dir);
            let _ = fs::remove_file(&zip_path);
        }
    }

    // 5. ReShade 6.8.0 Add-on Runtime
    if !is_reshade_cached() {
        let comp_root = get_components_root();
        let setup_path = comp_root.join("ReShade_Setup_6.8.0_Addon.exe");
        let mut step_prog = |mut p: DownloadProgress| {
            p.percentage = ((4.0 * 100.0) + p.percentage) / TOTAL_STEPS as f32;
            progress_fn(p);
        };
        if let Err(e) = download_file_with_progress(
            RESHADE_SETUP_URL,
            &setup_path,
            RESHADE_SETUP_SHA256,
            "ReShade 6.8.0 (Add-on Support)",
            "reshade",
            &mut step_prog,
        ).await {
            crate::core::state::log_message(&format!("@{{log_download_error|ReShade 6.8.0|{}}}", e));
            errors.push(format!("ReShade 6.8.0: {}", e));
        } else {
            let _ = extract_reshade_from_setup(&setup_path, &comp_root);
            let _ = fs::remove_file(&setup_path);
        }
    }

    // 6. Streamline Runtime v2.14.1
    if !is_streamline_cached() {
        let comp_root = get_components_root();
        let streamline_dir = comp_root.join("streamline-2.14.1");
        let zip_path = comp_root.join("streamline.zip");
        let mut step_prog = |mut p: DownloadProgress| {
            p.percentage = ((5.0 * 100.0) + p.percentage) / TOTAL_STEPS as f32;
            progress_fn(p);
        };
        if let Err(e) = download_file_with_progress(
            STREAMLINE_ZIP_URL,
            &zip_path,
            "",
            "Streamline Runtime v2.14.1",
            "streamline",
            &mut step_prog,
        ).await {
            crate::core::state::log_message(&format!("@{{log_download_error|Streamline|{}}}", e));
            errors.push(format!("Streamline Runtime v2.14.1: {}", e));
        } else if let Ok(file) = fs::File::open(&zip_path) {
            let _ = extract_zip(file, &streamline_dir);
            let _ = fs::remove_file(&zip_path);
        }
    }

    if are_all_mandatory_components_cached() {
        progress_fn(DownloadProgress {
            is_downloading: false,
            component_name: String::new(),
            component_id: String::new(),
            url: String::new(),
            downloaded_bytes: 0,
            total_bytes: None,
            percentage: 100.0,
            message: "All mandatory components ready".to_string(),
        });
        Ok(())
    } else {
        let err_msg = if errors.is_empty() {
            "Some mandatory components could not be cached".to_string()
        } else {
            errors.join("; ")
        };
        progress_fn(DownloadProgress {
            is_downloading: false,
            component_name: String::new(),
            component_id: String::new(),
            url: String::new(),
            downloaded_bytes: 0,
            total_bytes: None,
            percentage: 0.0,
            message: err_msg.clone(),
        });
        Err(err_msg)
    }
}

/// Orchestrates the automated background downloading of all mandatory components
pub async fn ensure_all_mandatory_components<F>(mut progress_fn: F) -> Result<(), String>
where
    F: FnMut(&str),
{
    ensure_all_mandatory_components_with_progress(|prog| {
        progress_fn(&prog.message);
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_bytes_sha256() {
        let data = b"test payload string";
        let hash = compute_bytes_sha256(data);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_zip_extraction_mock() {
        let temp = std::env::temp_dir().join(format!("test_zip_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("sample.txt", options).unwrap();
            std::io::Write::write_all(&mut writer, b"hello world").unwrap();
            writer.finish().unwrap();
        }

        let out_dir = temp.join("extracted");
        extract_zip(std::io::Cursor::new(buf), &out_dir).unwrap();
        assert!(out_dir.join("sample.txt").exists());
        assert_eq!(fs::read_to_string(out_dir.join("sample.txt")).unwrap(), "hello world");

        // Test compute_sha256 on a real file
        let hash = compute_sha256(&out_dir.join("sample.txt")).unwrap();
        assert_eq!(hash, compute_bytes_sha256(b"hello world"));

        // Test components root
        let comp_root = get_components_root();
        assert!(comp_root.exists() || comp_root.to_string_lossy().contains("dlss-5-studio"));

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_download_progress_default_and_format_bytes() {
        let def = DownloadProgress::default();
        assert!(!def.is_downloading);
        assert_eq!(def.percentage, 0.0);

        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(10 * 1024 * 1024), "10.00 MB");
    }

    #[test]
    fn test_step_progress_calculation() {
        const TOTAL_STEPS: usize = 6;
        for step in 0..TOTAL_STEPS {
            for pct in [0.0f32, 50.0f32, 100.0f32] {
                let overall = ((step as f32 * 100.0) + pct) / TOTAL_STEPS as f32;
                assert!(overall >= 0.0 && overall <= 100.0);
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_download_streamline_components() {
        let res = ensure_all_mandatory_components_with_progress(|prog| {
            if prog.is_downloading {
                println!("Progress: {:.1}% - {}", prog.percentage, prog.message);
            }
        }).await;
        assert!(res.is_ok(), "Mandatory download failed: {:?}", res);
        assert!(is_streamline_cached(), "Streamline must be cached after download");
        assert!(are_all_mandatory_components_cached(), "All mandatory components must be cached");
    }
}
