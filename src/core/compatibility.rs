use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
}
