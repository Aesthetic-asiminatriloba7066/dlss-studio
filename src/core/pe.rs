use pelite::pe32::Pe as _;
use pelite::pe64::Pe as _;
use pelite::{FileMap, PeFile, Wrap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeInfo {
    pub bitness: u32,
    pub version: Option<String>,
    pub imports: Vec<String>,
}

pub fn inspect_pe<P: AsRef<Path>>(path: P) -> Option<PeInfo> {
    let map = FileMap::open(path.as_ref()).ok()?;
    let file = PeFile::from_bytes(map.as_ref()).ok()?;

    let (bitness, imports) = match file {
        Wrap::T32(pe32) => {
            let mut list = Vec::new();
            if let Ok(import_dir) = pe32.imports() {
                for desc in import_dir {
                    if let Ok(dll_name) = desc.dll_name() {
                        list.push(dll_name.to_str().unwrap_or("").to_lowercase());
                    }
                }
            }
            (32, list)
        }
        Wrap::T64(pe64) => {
            let mut list = Vec::new();
            if let Ok(import_dir) = pe64.imports() {
                for desc in import_dir {
                    if let Ok(dll_name) = desc.dll_name() {
                        list.push(dll_name.to_str().unwrap_or("").to_lowercase());
                    }
                }
            }
            (64, list)
        }
    };

    let version = get_version_info(&file);

    Some(PeInfo {
        bitness,
        version,
        imports,
    })
}

fn get_version_info(file: &PeFile) -> Option<String> {
    let resources = match file {
        Wrap::T32(pe32) => pe32.resources().ok()?,
        Wrap::T64(pe64) => pe64.resources().ok()?,
    };
    let version_info = resources.version_info().ok()?;

    if let Some(fixed) = version_info.fixed() {
        let fv = &fixed.dwFileVersion;
        if fv.Major > 0 || fv.Minor > 0 || fv.Patch > 0 || fv.Build > 0 {
            return Some(format!("{}.{}.{}.{}", fv.Major, fv.Minor, fv.Patch, fv.Build));
        }
    }

    if let Some(&lang) = version_info.translation().first() {
        if let Some(val) = version_info.value(lang, "FileVersion") {
            return Some(val.to_string());
        }
        if let Some(val) = version_info.value(lang, "ProductVersion") {
            return Some(val.to_string());
        }
    }

    None
}

#[inline(always)]
fn fast_find(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    let first = needle[0];
    let rest = &needle[1..];
    let rest_len = rest.len();
    let max_idx = haystack.len() - needle.len();

    let mut i = 0;
    while i <= max_idx {
        if haystack[i] == first {
            if &haystack[i + 1..i + 1 + rest_len] == rest {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Scans mapped or streamed bytes for Latin1 string markers with zero unnecessary heap allocations.
pub fn find_markers<P: AsRef<Path>>(path: P, markers: &[&str]) -> Vec<String> {
    let p = path.as_ref();
    // Fast path: memory map the file directly so the OS handles paging with zero heap buffers
    if let Ok(map) = FileMap::open(p) {
        let bytes = map.as_ref();
        let scan_limit = bytes.len().min(128 * 1024 * 1024);
        let view = &bytes[..scan_limit];
        let mut found = Vec::new();
        for &m in markers {
            if fast_find(view, m.as_bytes()) {
                found.push(m.to_string());
            }
        }
        return found;
    }

    // Fallback: chunk-based streaming with a compact 256KB buffer (16x smaller than legacy 4MB)
    let mut file = match File::open(p) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let Ok(size) = file.metadata().map(|m| m.len()) else { return Vec::new(); };
    if size == 0 { return Vec::new(); }

    let scan_limit = size.min(128 * 1024 * 1024);
    let chunk_size = 256 * 1024;
    let overlap = 64;
    let mut buf = vec![0u8; chunk_size + overlap];
    let mut carry = 0;
    let mut pos = 0u64;
    let mut found = Vec::new();

    while pos < scan_limit {
        if file.seek(SeekFrom::Start(pos)).is_err() { break; }
        let to_read = (chunk_size as u64).min(scan_limit - pos) as usize;
        let read = match file.read(&mut buf[carry..carry + to_read]) {
            Ok(n) if n > 0 => n,
            _ => break,
        };
        let view = &buf[..carry + read];
        for &m in markers {
            if !found.iter().any(|x: &String| x == m) {
                if fast_find(view, m.as_bytes()) {
                    found.push(m.to_string());
                }
            }
        }
        if found.len() == markers.len() { break; }
        let view_len = view.len();
        carry = overlap.min(view_len);
        buf.copy_within(view_len - carry..view_len, 0);
        pos += read as u64;
    }

    found
}

pub fn is_reshade_dll<P: AsRef<Path>>(path: P) -> (bool, Option<String>, bool) {
    let p = path.as_ref();
    if !p.is_file() {
        return (false, None, false);
    }
    let pe = match inspect_pe(p) {
        Some(info) => info,
        None => return (false, None, false),
    };

    let mut mentions = false;
    let mut has_addon_support = false;

    if let Ok(map) = pelite::FileMap::open(p) {
        if let Ok(file) = pelite::PeFile::from_bytes(map.as_ref()) {
            let res = match file {
                pelite::Wrap::T32(pe32) => pe32.resources().ok(),
                pelite::Wrap::T64(pe64) => pe64.resources().ok(),
            };
            if let Some(resources) = res {
                if let Ok(vi) = resources.version_info() {
                    for &lang in vi.translation() {
                        for key in &["ProductName", "FileDescription", "CompanyName", "OriginalFilename"] {
                            if let Some(val) = vi.value(lang, key) {
                                if val.to_lowercase().contains("reshade") {
                                    mentions = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let markers = find_markers(p, &["Searching for add-ons", "ReShade"]);
    if markers.iter().any(|m| m == "Searching for add-ons") {
        has_addon_support = true;
        mentions = true;
    }
    if markers.iter().any(|m| m == "ReShade") {
        mentions = true;
    }

    (mentions, pe.version, has_addon_support)
}

pub fn is_optiscaler_or_proxy<P: AsRef<Path>>(path: P) -> bool {
    let p = path.as_ref();
    if !p.is_file() {
        return false;
    }

    if let Ok(map) = pelite::FileMap::open(p) {
        if let Ok(file) = pelite::PeFile::from_bytes(map.as_ref()) {
            let res = match file {
                pelite::Wrap::T32(pe32) => pe32.resources().ok(),
                pelite::Wrap::T64(pe64) => pe64.resources().ok(),
            };
            if let Some(resources) = res {
                if let Ok(vi) = resources.version_info() {
                    for &lang in vi.translation() {
                        for key in &["ProductName", "FileDescription", "CompanyName", "OriginalFilename", "InternalName"] {
                            if let Some(val) = vi.value(lang, key) {
                                let lower = val.to_lowercase();
                                if lower.contains("optiscaler") || lower.contains("nitec") || lower.contains("reshade") || lower.contains("rtxmfg") || lower.contains("dashdogy") {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let markers = ["OptiScaler", "nitec", "ReShade", "DLSS-NR", "RTXMFG", "Dashdogy", "Streamline"];
    !find_markers(p, &markers).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_optiscaler_or_proxy_with_marker() {
        let temp_dir = std::env::temp_dir().join(format!("pe_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let dummy_dll = temp_dir.join("dxgi.dll");

        // Plain binary without markers
        std::fs::write(&dummy_dll, vec![0u8; 1024]).unwrap();
        assert!(!is_optiscaler_or_proxy(&dummy_dll));

        // Binary with OptiScaler marker
        let mut with_marker = vec![0u8; 100_000];
        with_marker[50_000..50_010].copy_from_slice(b"OptiScaler");
        std::fs::write(&dummy_dll, with_marker).unwrap();
        assert!(is_optiscaler_or_proxy(&dummy_dll));

        // Binary with Streamline marker
        let mut with_streamline = vec![0u8; 10_000];
        with_streamline[1000..1010].copy_from_slice(b"Streamline");
        std::fs::write(&dummy_dll, with_streamline).unwrap();
        assert!(is_optiscaler_or_proxy(&dummy_dll));

        // Missing file
        assert!(!is_optiscaler_or_proxy(temp_dir.join("missing.dll")));
        assert!(inspect_pe(temp_dir.join("missing.dll")).is_none());

        let (mentions, ver, addon) = is_reshade_dll(&dummy_dll);
        assert!(!mentions);
        assert!(ver.is_none());
        assert!(!addon);

        let reshade_payload = Path::new("payload/reshade-vulkan/ReShade64.dll");
        let reshade_cached = crate::core::downloader::get_components_root().join("reshade-vulkan/ReShade64.dll");
        let target = if reshade_payload.is_file() { Some(reshade_payload) } else if reshade_cached.is_file() { Some(reshade_cached.as_path()) } else { None };
        if let Some(target_p) = target {
            let (is_res, _, has_add) = is_reshade_dll(target_p);
            assert!(is_res);
            assert!(has_add);
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
