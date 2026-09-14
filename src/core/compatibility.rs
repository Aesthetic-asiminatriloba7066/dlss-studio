use std::path::{Path, PathBuf};
use std::fs;

pub fn managed_mod_root(game_dir: &Path, exe_path: Option<&Path>) -> Option<PathBuf> {
    let mut current = exe_path
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| game_dir.to_path_buf());

    for _ in 0..5 {
        let has_mo2 = current.join("ModOrganizer.exe").exists();
        let has_stock = current.join("Stock Game").exists();
        if has_mo2 && has_stock {
            return Some(current);
        }
        if let Some(parent) = current.parent() {
            if parent == current {
                break;
            }
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    None
}

pub fn missing_vc_runtime(bitness: u32, exe_dir: Option<&Path>) -> Vec<String> {
    let mut missing = Vec::new();
    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let sys_dir = if bitness == 32 {
        PathBuf::from(&sys_root).join("SysWOW64")
    } else {
        PathBuf::from(&sys_root).join("System32")
    };

    let files = if bitness == 64 {
        vec!["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll"]
    } else {
        vec!["msvcp140.dll", "vcruntime140.dll"]
    };

    for name in files {
        let in_game = exe_dir.map(|d| d.join(name).exists()).unwrap_or(false);
        let in_sys = sys_dir.join(name).exists();
        if !in_game && !in_sys {
            missing.push(name.to_string());
        }
    }
    missing
}

pub fn is_vulkan_wrapper(file: &Path) -> bool {
    if let Ok(bytes) = fs::read(file) {
        let s = String::from_utf8_lossy(&bytes);
        (s.contains("DXVK") || s.contains("vkd3d")) && !s.contains("ReShade")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_managed_mod_root_detection() {
        let temp = std::env::temp_dir().join(format!("test_mo2_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game_dir = temp.join("Stock Game");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(temp.join("ModOrganizer.exe"), b"dummy").unwrap();

        let detected = managed_mod_root(&game_dir, None);
        assert_eq!(detected, Some(temp.clone()));

        // Negative check without ModOrganizer.exe
        let _ = fs::remove_file(temp.join("ModOrganizer.exe"));
        assert_eq!(managed_mod_root(&game_dir, None), None);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_missing_vc_runtime_detection() {
        let temp = std::env::temp_dir().join(format!("test_vc_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let missing_64 = missing_vc_runtime(64, Some(&temp));
        let missing_32 = missing_vc_runtime(32, Some(&temp));
        assert!(missing_64.len() <= 3);
        assert!(missing_32.len() <= 2);

        for name in &["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll"] {
            fs::write(temp.join(name), b"dummy").unwrap();
        }
        assert_eq!(missing_vc_runtime(64, Some(&temp)), Vec::<String>::new());
        assert_eq!(missing_vc_runtime(32, Some(&temp)), Vec::<String>::new());

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_is_vulkan_wrapper_check() {
        let temp = std::env::temp_dir().join(format!("test_vk_wrap_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let dxvk_file = temp.join("dxvk.dll");
        let vkd3d_file = temp.join("vkd3d.dll");
        let reshade_file = temp.join("reshade.dll");
        let plain_file = temp.join("plain.dll");

        fs::write(&dxvk_file, b"This is a DXVK wrapper library").unwrap();
        fs::write(&vkd3d_file, b"This is a vkd3d wrapper library").unwrap();
        fs::write(&reshade_file, b"DXVK and ReShade together").unwrap();
        fs::write(&plain_file, b"Plain DirectX binary").unwrap();

        assert!(is_vulkan_wrapper(&dxvk_file));
        assert!(is_vulkan_wrapper(&vkd3d_file));
        assert!(!is_vulkan_wrapper(&reshade_file));
        assert!(!is_vulkan_wrapper(&plain_file));
        assert!(!is_vulkan_wrapper(&temp.join("nonexistent.dll")));

        let _ = fs::remove_dir_all(&temp);
    }
}
