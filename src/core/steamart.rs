#![allow(dead_code)]

use std::fs;
use std::path::Path;
use regex::Regex;
use serde::Deserialize;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use crate::core::state::get_appdata_dir;

#[derive(Debug, Clone, Deserialize)]
struct StoreSearchResult {
    items: Option<Vec<StoreItem>>,
}

#[derive(Debug, Clone, Deserialize)]
struct StoreItem {
    id: u64,
    name: Option<String>,
    #[serde(rename = "type")]
    item_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GameArt {
    pub appid: Option<u64>,
    pub cover_path: Option<String>,
    pub hero_path: Option<String>,
}

/// Normalizes and cleans game folder names by removing scene tags, repacks, and edition markers.
pub fn clean_name(name: &str) -> String {
    let re_brackets = Regex::new(r"\[[^\]]*\]|\([^)]*\)").unwrap();
    let re_tags = Regex::new(r"(?i)\b(repack|fitgirl|dodi|elamigos|codex|rune|empress|plaza|skidrow|multi\d*)\b").unwrap();
    let re_punct = Regex::new(r"[_\-—–:.]+").unwrap();
    let re_spaces = Regex::new(r"\s+").unwrap();

    let s = re_brackets.replace_all(name, " ");
    let s = re_tags.replace_all(&s, " ");
    let s = re_punct.replace_all(&s, " ");
    let s = re_spaces.replace_all(&s, " ");
    s.trim().to_string()
}

fn norm_title(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Evaluates candidate Steam search rows and picks the highest scoring match.
fn pick_best(items: &[StoreItem], query: &str) -> Option<StoreItem> {
    let q = norm_title(query);
    let q_words: std::collections::HashSet<&str> = q.split_whitespace().collect();
    let mut best: Option<StoreItem> = None;
    let mut best_score: i32 = -1;

    for item in items {
        if let Some(ref t) = item.item_type {
            if t != "app" {
                continue;
            }
        }
        let n = norm_title(item.name.as_deref().unwrap_or_default());
        let words: Vec<&str> = n.split_whitespace().collect();
        let shared = words.iter().filter(|w| q_words.contains(*w)).count() as i32;

        let mut score: i32 = 0;
        if n == q {
            score += 120;
        } else if n.starts_with(&q) || q.starts_with(&n) {
            score += 70;
        }
        if !q_words.is_empty() {
            score += (shared * 40) / (q_words.len() as i32);
        }
        if words.len() > q_words.len() {
            score -= (words.len() - q_words.len()) as i32 * 6;
        }

        if score > best_score {
            best_score = score;
            best = Some(item.clone());
        }
    }

    if best_score > 10 {
        best
    } else {
        None
    }
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub fn url_decode(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(val);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Helper to read file and convert to base64 data URI (legacy fallback)
pub fn file_to_data_uri(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 500 {
        return None;
    }
    let encoded = BASE64_STANDARD.encode(&bytes);
    Some(format!("data:image/jpeg;base64,{}", encoded))
}

pub fn bytes_to_data_uri(bytes: &[u8]) -> String {
    let encoded = BASE64_STANDARD.encode(bytes);
    format!("data:image/jpeg;base64,{}", encoded)
}

/// Helper to convert a local file to a lightweight HTTP URI served by Wry's custom desktop protocol
pub fn file_to_art_uri(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let art_dir = get_appdata_dir().join("art");
    let _ = fs::create_dir_all(&art_dir);

    // If already in art cache, reference filename directly
    if let Ok(rel) = path.strip_prefix(&art_dir) {
        let s = rel.to_string_lossy().replace('\\', "/");
        return Some(format!("http://dlss-art.localhost/art/{}", s));
    }

    // Otherwise, copy to art cache under safe key
    let key = key_for_dir(path);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
    let cached = art_dir.join(format!("{}.{}", key, ext));
    if !cached.exists() {
        let _ = fs::copy(path, &cached);
    }
    Some(format!("http://dlss-art.localhost/art/{}.{}", key, ext))
}

/// Normalizes any poster URI (legacy dlss-art://, raw local file paths, or current scheme)
/// into a Wry-compatible http://dlss-art.localhost/art/... URL.
pub fn normalize_art_uri(uri: &str) -> String {
    if uri.starts_with("data:image/") {
        return uri.to_string();
    }
    if uri.starts_with("http://dlss-art.localhost/") {
        return uri.to_string();
    }
    if let Some(rest) = uri.strip_prefix("dlss-art://art/") {
        return format!("http://dlss-art.localhost/art/{}", rest.trim_start_matches('/'));
    }
    if let Some(rest) = uri.strip_prefix("dlss-art://") {
        let clean = rest
            .trim_start_matches("localhost/")
            .trim_start_matches("local/")
            .trim_start_matches("art/");
        return format!("http://dlss-art.localhost/art/{}", clean.trim_start_matches('/'));
    }
    if let Some(rest) = uri.strip_prefix("http://dlss-art.local/art/") {
        return format!("http://dlss-art.localhost/art/{}", rest.trim_start_matches('/'));
    }
    // If it's a raw filesystem path, convert it via file_to_art_uri
    let p = Path::new(uri);
    if p.is_file() {
        if let Some(art_url) = file_to_art_uri(p) {
            return art_url;
        }
    }
    uri.to_string()
}

/// Handles requests to the custom dlss-art protocol, reading cached posters directly from disk.
/// Accepts URLs from Wry's internal rewrite (dlss-art://localhost/art/...), direct HTTP (http://dlss-art.localhost/art/...),
/// or legacy (dlss-art://art/...).
pub fn handle_art_request(uri: &str) -> Option<(String, Vec<u8>)> {
    let art_dir = get_appdata_dir().join("art");

    let target_file = if let Some(idx) = uri.find("file=") {
        let raw = &uri[idx + 5..];
        let raw_file = raw.split('&').next().unwrap_or(raw);
        let decoded = url_decode(raw_file);
        std::path::PathBuf::from(decoded)
    } else {
        let clean = uri.split('?').next().unwrap_or(uri);
        let filename = if let Some(idx) = clean.find("/art/") {
            &clean[idx + 5..]
        } else if let Some(stripped) = clean.strip_prefix("art/") {
            stripped
        } else {
            let without_proto = clean
                .trim_start_matches("dlss-art://")
                .trim_start_matches("dlss-art:")
                .trim_start_matches("http://dlss-art.localhost/")
                .trim_start_matches("http://dlss-art.local/");
            without_proto.trim_start_matches("localhost/").trim_start_matches('/')
        };
        art_dir.join(filename.trim_start_matches('/'))
    };

    if target_file.is_file() {
        if let Ok(bytes) = fs::read(&target_file) {
            let mime = if target_file.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("png")).unwrap_or(false) {
                "image/png"
            } else {
                "image/jpeg"
            };
            return Some((mime.to_string(), bytes));
        }
    }
    None
}

/// Searches Steam's public store endpoint for a matching game title and returns its AppID.
pub async fn search_steam_appid(name: &str) -> Option<(u64, String)> {
    let query = clean_name(name);
    if query.is_empty() {
        return None;
    }

    let url = format!(
        "https://store.steampowered.com/api/storesearch/?term={}&cc=us&l=en",
        url_encode(&query)
    );

    let client = reqwest::Client::builder()
        .user_agent("DLSS5-Swapper-Native/1.0")
        .build()
        .ok()?;

    let res = client.get(&url).send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }

    let data = res.json::<StoreSearchResult>().await.ok()?;
    let items = data.items.unwrap_or_default();
    let best = pick_best(&items, &query)?;
    Some((best.id, best.name.unwrap_or_else(|| query.to_string())))
}

/// Generates a filesystem-safe cache key for a game directory.
pub fn key_for_dir(dir: &Path) -> String {
    let s = dir.to_string_lossy().to_lowercase().replace('/', "\\");
    let mut hasher = sha2::Sha256::default();
    use sha2::Digest;
    hasher.update(s.as_bytes());
    hex::encode(&hasher.finalize()[..8])
}

/// Resolves or downloads the cover and hero background images for a game.
/// Caches files locally in %APPDATA%\dlss5-swapper-rust\art\<key>-cover.jpg and <key>-hero.jpg.
/// Returns lightweight http://dlss-art.localhost/art/<key>-cover.jpg URIs so WebView2 streams directly from disk with zero base64 bloat.
pub async fn resolve_game_art(name: &str, dir: &Path, known_appid: Option<u64>) -> GameArt {
    let art_dir = get_appdata_dir().join("art");
    let _ = fs::create_dir_all(&art_dir);
    let key = key_for_dir(dir);

    let cover_file = art_dir.join(format!("{}-cover.jpg", key));
    let hero_file = art_dir.join(format!("{}-hero.jpg", key));

    let hero_cached = if hero_file.exists() && hero_file.metadata().map(|m| m.len() > 2000).unwrap_or(false) {
        Some(format!("http://dlss-art.localhost/art/{}-hero.jpg", key))
    } else {
        None
    };

    let cover_cached = if cover_file.exists() && cover_file.metadata().map(|m| m.len() > 2000).unwrap_or(false) {
        Some(format!("http://dlss-art.localhost/art/{}-cover.jpg", key))
    } else {
        None
    };

    if cover_cached.is_some() || hero_cached.is_some() {
        return GameArt {
            appid: known_appid,
            cover_path: cover_cached.or_else(|| hero_cached.clone()),
            hero_path: hero_cached,
        };
    }

    let appid = if let Some(id) = known_appid {
        id
    } else if let Some((id, _)) = search_steam_appid(name).await {
        id
    } else {
        return GameArt::default();
    };

    let client = reqwest::Client::builder()
        .user_agent("DLSS5-Swapper-Native/1.0")
        .build()
        .unwrap_or_default();

    // 1. Download Cover Poster (library_600x900.jpg)
    let cover_url = format!("https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{}/library_600x900.jpg", appid);
    let mut downloaded_cover = None;
    if let Ok(resp) = client.get(&cover_url).send().await {
        if resp.status().is_success() {
            if let Ok(bytes) = resp.bytes().await {
                if bytes.len() > 2000 {
                    let _ = fs::write(&cover_file, &bytes);
                    downloaded_cover = Some(format!("http://dlss-art.localhost/art/{}-cover.jpg", key));
                }
            }
        }
    }

    // 2. Download Hero Banner (library_hero.jpg or fallback header.jpg)
    let hero_url = format!("https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{}/library_hero.jpg", appid);
    let mut downloaded_hero = None;
    if let Ok(resp) = client.get(&hero_url).send().await {
        if resp.status().is_success() {
            if let Ok(bytes) = resp.bytes().await {
                if bytes.len() > 2000 {
                    let _ = fs::write(&hero_file, &bytes);
                    downloaded_hero = Some(format!("http://dlss-art.localhost/art/{}-hero.jpg", key));
                }
            }
        }
    }

    if downloaded_hero.is_none() {
        let header_url = format!("https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{}/header.jpg", appid);
        if let Ok(resp) = client.get(&header_url).send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    if bytes.len() > 2000 {
                        let _ = fs::write(&hero_file, &bytes);
                        downloaded_hero = Some(format!("http://dlss-art.localhost/art/{}-hero.jpg", key));
                    }
                }
            }
        }
    }
    let final_cover = downloaded_cover.or_else(|| downloaded_hero.clone());
    GameArt {
        appid: Some(appid),
        cover_path: final_cover,
        hero_path: downloaded_hero,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_name_tags_and_brackets() {
        assert_eq!(clean_name("Cyberpunk 2077 [v2.12] (DODI Repack)"), "Cyberpunk 2077");
        assert_eq!(clean_name("The.Witcher.3.Wild.Hunt-FitGirl"), "The Witcher 3 Wild Hunt");
        assert_eq!(clean_name("Baldur's Gate 3 (ElAmigos)"), "Baldur's Gate 3");
        assert_eq!(clean_name("Hogwarts Legacy [CODEX]"), "Hogwarts Legacy");
        assert_eq!(clean_name("Starfield [FitGirl Repack]"), "Starfield");
    }

    #[test]
    fn test_norm_title_and_scoring() {
        assert_eq!(norm_title("Cyberpunk 2077: Phantom Liberty"), "cyberpunk 2077 phantom liberty");
        let items = vec![
            StoreItem { id: 1091500, name: Some("Cyberpunk 2077".to_string()), item_type: Some("app".to_string()) },
            StoreItem { id: 2138330, name: Some("Cyberpunk 2077: Phantom Liberty".to_string()), item_type: Some("app".to_string()) },
            StoreItem { id: 9999999, name: Some("Cyberpunk Bonus Pack".to_string()), item_type: None },
        ];
        let best = pick_best(&items, "Cyberpunk 2077");
        assert!(best.is_some());
        assert_eq!(best.unwrap().id, 1091500);

        let best_dlc = pick_best(&items, "Phantom Liberty");
        assert!(best_dlc.is_some());
        assert_eq!(best_dlc.unwrap().id, 2138330);
    }

    #[test]
    fn test_key_for_dir_hashing() {
        let p1 = Path::new("C:\\Games\\Cyberpunk 2077");
        let p2 = Path::new("C:/Games/Cyberpunk 2077");
        assert_eq!(key_for_dir(p1), key_for_dir(p2));
        assert_ne!(key_for_dir(p1), key_for_dir(Path::new("D:\\Games\\Witcher 3")));
    }

    #[test]
    fn test_data_uri_conversion() {
        let bytes = vec![0x89u8; 1024]; // 1 KB buffer > 500 bytes
        let uri = bytes_to_data_uri(&bytes);
        assert!(uri.starts_with("data:image/jpeg;base64,"));
        assert!(uri.len() > 25);

        let temp = std::env::temp_dir().join(format!("test_art_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let file_p = temp.join("cover.jpg");
        fs::write(&file_p, &bytes).unwrap();
        assert_eq!(file_to_data_uri(&file_p), Some(uri));
        assert_eq!(file_to_data_uri(&temp.join("missing.jpg")), None);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_file_to_art_uri_and_url_decode() {
        assert_eq!(url_decode("Hello%20World%2BTest"), "Hello World+Test");
        let temp = std::env::temp_dir().join(format!("test_art_uri_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let file_p = temp.join("test_cover.jpg");
        fs::write(&file_p, b"synthetic-jpeg-data").unwrap();

        let art_uri = file_to_art_uri(&file_p);
        assert!(art_uri.is_some());
        let uri_str = art_uri.unwrap();
        assert!(uri_str.starts_with("http://dlss-art.localhost/art/"));

        // Direct HTTP URI
        let handled_direct = handle_art_request(&uri_str);
        assert!(handled_direct.is_some());
        let (mime, data) = handled_direct.unwrap();
        assert_eq!(mime, "image/jpeg");
        assert_eq!(data, b"synthetic-jpeg-data");

        // Wry internal rewritten URI (dlss-art://localhost/art/...)
        let wry_uri = uri_str.replace("http://dlss-art.localhost/art/", "dlss-art://localhost/art/");
        let handled_wry = handle_art_request(&wry_uri);
        assert!(handled_wry.is_some());
        assert_eq!(handled_wry.unwrap().1, b"synthetic-jpeg-data");

        // Legacy stored URI (dlss-art://art/...)
        let legacy_uri = uri_str.replace("http://dlss-art.localhost/art/", "dlss-art://art/");
        let handled_legacy = handle_art_request(&legacy_uri);
        assert!(handled_legacy.is_some());
        assert_eq!(handled_legacy.unwrap().1, b"synthetic-jpeg-data");

        // normalize_art_uri tests
        assert_eq!(
            normalize_art_uri("dlss-art://art/my-cover.jpg"),
            "http://dlss-art.localhost/art/my-cover.jpg"
        );
        assert_eq!(
            normalize_art_uri("dlss-art://localhost/art/my-cover.jpg"),
            "http://dlss-art.localhost/art/my-cover.jpg"
        );
        assert_eq!(
            normalize_art_uri("http://dlss-art.localhost/art/my-cover.jpg"),
            "http://dlss-art.localhost/art/my-cover.jpg"
        );

        let _ = fs::remove_dir_all(&temp);
    }
}
