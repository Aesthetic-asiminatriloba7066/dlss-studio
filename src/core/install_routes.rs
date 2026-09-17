#![allow(dead_code)]

use crate::core::scan::GameEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallRoute {
    Native,
    OptiScaler,
    Feeder,
}

impl InstallRoute {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallRoute::Native => "native",
            InstallRoute::OptiScaler => "optiscaler",
            InstallRoute::Feeder => "feeder",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptiReason {
    Unsupported,
    NeedsDlss,
}

impl OptiReason {
    pub fn message(&self) -> &'static str {
        match self {
            OptiReason::Unsupported => "OptiScaler DLSS-NR is offered only for 64-bit DX11/DX12/Vulkan games, not DX8/DX9, OpenGL or emulators.",
            OptiReason::NeedsDlss => "OptiScaler needs the game's original DLSS pipeline. No original DLSS DLL was found; copied/injected DLLs alone do not qualify.",
        }
    }
}

/// Checks why OptiScaler is not eligible for a given game.
/// Mirrors `optiReason(target, api)` from original src/shared/install-routes.js:
/// 1. target.bitness !== 64 || target.emulator -> 'optiUnsupported'
/// 2. !['dxgi', 'vulkan'].includes(api) || target.apiLabel === 'DirectX 10' -> 'optiUnsupported'
/// 3. !target.hasNativeDlss -> 'optiNeedsDlss'
pub fn check_opti_reason(game: &GameEntry) -> Option<OptiReason> {
    if game.bitness != 64 {
        return Some(OptiReason::Unsupported);
    }

    let api_lower = game.api.to_lowercase();
    let is_dxgi_or_vulkan = api_lower.contains("12") 
        || api_lower.contains("11") 
        || api_lower.contains("dxgi") 
        || api_lower.contains("vulkan");

    let is_unsupported_api = api_lower.contains("10")
        || api_lower.contains("directx 8")
        || api_lower.contains("d3d8")
        || api_lower.contains("directx 9")
        || api_lower.contains("d3d9")
        || api_lower.contains("opengl");

    if !is_dxgi_or_vulkan || is_unsupported_api {
        return Some(OptiReason::Unsupported);
    }

    if game.dlss_version.is_none() {
        return Some(OptiReason::NeedsDlss);
    }

    None
}

/// Returns the supported installation routes for a given game.
/// Mirrors `routesFor(target, api)` from original src/shared/install-routes.js:
pub fn routes_for(game: &GameEntry) -> Vec<InstallRoute> {
    if game.bitness != 32 && game.bitness != 64 {
        return Vec::new();
    }

    let api_lower = game.api.to_lowercase();

    if api_lower.contains("10") && !api_lower.contains("11") && !api_lower.contains("12") {
        return Vec::new(); // DX10 unsupported
    }

    if api_lower.contains("d3d8") || api_lower.contains("directx 8") {
        return if game.bitness == 32 { vec![InstallRoute::Feeder] } else { Vec::new() };
    }

    if api_lower.contains("d3d9") || api_lower.contains("directx 9") || api_lower.contains("opengl") || api_lower.contains("vulkan") {
        let opti_res = check_opti_reason(game);
        if opti_res.is_none() {
            return vec![InstallRoute::Feeder, InstallRoute::OptiScaler];
        } else {
            return vec![InstallRoute::Feeder];
        }
    }

    // DXGI / DirectX 11 / DirectX 12
    let mut routes = if is_native_dlss_supported(game) {
        vec![InstallRoute::Native, InstallRoute::Feeder]
    } else {
        vec![InstallRoute::Feeder]
    };

    if check_opti_reason(game).is_none() {
        routes.push(InstallRoute::OptiScaler);
    }

    routes
}

/// Computes the recommended default route for a game.
/// Mirrors `recommendedRoute(scan, target)` from original src/shared/install-routes.js:
pub fn recommended_route(game: &GameEntry) -> InstallRoute {
    let routes = routes_for(game);
    let has_native_dlss = game.dlss_version.is_some();
    let wanted = if has_native_dlss && routes.contains(&InstallRoute::Native) {
        InstallRoute::Native
    } else {
        InstallRoute::Feeder
    };

    if routes.contains(&wanted) {
        wanted
    } else if let Some(&first) = routes.first() {
        first
    } else {
        InstallRoute::Feeder
    }
}

/// Determines if Native DLSS (RenoDX D3D12 NGX hook) is strictly supported by the game engine.
/// Native DLSS relies exclusively on D3D12 NGX EvaluateFeature; non-DX12 engines (Vulkan, DX11) cannot use it.
pub fn is_native_dlss_supported(game: &GameEntry) -> bool {
    let api_lower = game.api.to_lowercase();
    let is_dx12 = api_lower.contains("12");
    game.bitness == 64 && is_dx12 && game.dlss_version.is_some()
}

/// Determines if Frame Generation (native DLSS-G, Streamline, or injected MFG) is supported for a game.
pub fn is_frame_generation_supported(game: &GameEntry) -> bool {
    let api_lower = game.api.to_lowercase();
    let is_vulkan = api_lower.contains("vulkan");
    let is_dx12 = api_lower.contains("12") || api_lower.contains("d3d12");
    let is_dx11 = api_lower.contains("11") || api_lower == "d3d11";

    // Native DLSS-G games (DX12) always support FG
    if game.has_frame_generation {
        return true;
    }
    // Already installed MFG unlock
    if game.mfg_unlock_installed {
        return true;
    }
    // Injected FG is strictly blocked on DX11
    if is_dx11 {
        return false;
    }
    // Injected FG requires 64-bit Vulkan (or DX12 with Streamline)
    game.can_inject_fg && (is_vulkan || is_dx12)
}

/// Returns a human-readable status label and an active boolean for the Frame Generation spec in the UI.
pub fn frame_generation_status(game: &GameEntry) -> (&'static str, bool) {
    if game.has_frame_generation {
        ("Supported (DLSS-G)", true)
    } else if game.mfg_unlock_installed {
        ("Installed (4x MFG)", true)
    } else if game.can_inject_fg {
        let api_lower = game.api.to_lowercase();
        if api_lower.contains("vulkan") {
            ("Supported (Vulkan Streamline)", true)
        } else {
            ("Unsupported (Requires Native DLSS-G)", false)
        }
    } else {
        ("Unsupported (Requires Native DLSS-G)", false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_test_game(api: &str, bitness: u32, dlss: Option<&str>) -> GameEntry {
        GameEntry {
            name: "Test Game".to_string(),
            dir: PathBuf::from("C:\\Games\\Test"),
            exe_path: PathBuf::from("C:\\Games\\Test\\game.exe"),
            exe_rel: "game.exe".to_string(),
            bitness,
            api: api.to_string(),
            dlss_version: dlss.map(|s| s.to_string()),
            optiscaler_installed: false,
            optiscaler_presr: false,
            optiscaler_passes: 1,
            has_frame_generation: false,
            can_inject_fg: false,
            mfg_unlock_installed: false,
            has_backup: false,
            launcher: "Steam".to_string(),
            poster: None,
            reshade_installed: false,
            reshade_version: None,
            reshade_addon_support: false,
            addon_installed: false,
            installed_route: None,
            files: Vec::new(),
            available_exes: Vec::new(),
            is_laa: false,
            nr_style: 0,
        }
    }

    #[test]
    fn test_32bit_game_rejects_optiscaler() {
        let g = make_test_game("DirectX 12", 32, Some("3.7.0"));
        assert_eq!(check_opti_reason(&g), Some(OptiReason::Unsupported));
        let routes = routes_for(&g);
        assert!(!routes.contains(&InstallRoute::OptiScaler));
        assert_eq!(routes, vec![InstallRoute::Feeder]);
    }

    #[test]
    fn test_game_without_dlss_rejects_optiscaler() {
        let g = make_test_game("DirectX 12", 64, None);
        assert_eq!(check_opti_reason(&g), Some(OptiReason::NeedsDlss));
        let routes = routes_for(&g);
        assert!(!routes.contains(&InstallRoute::OptiScaler));
        assert_eq!(routes, vec![InstallRoute::Feeder]);
        assert_eq!(recommended_route(&g), InstallRoute::Feeder);
    }

    #[test]
    fn test_dx9_game_rejects_optiscaler() {
        let g = make_test_game("DirectX 9", 64, None);
        assert_eq!(check_opti_reason(&g), Some(OptiReason::Unsupported));
        let routes = routes_for(&g);
        assert_eq!(routes, vec![InstallRoute::Feeder]);
    }

    #[test]
    fn test_dx12_game_with_dlss_allows_all() {
        let g = make_test_game("DirectX 12", 64, Some("3.7.0"));
        assert_eq!(check_opti_reason(&g), None);
        let routes = routes_for(&g);
        assert!(routes.contains(&InstallRoute::Native));
        assert!(routes.contains(&InstallRoute::Feeder));
        assert!(routes.contains(&InstallRoute::OptiScaler));
        assert_eq!(recommended_route(&g), InstallRoute::Native);
        assert!(is_native_dlss_supported(&g));
    }

    #[test]
    fn test_vulkan_and_dx11_games_reject_native_dlss() {
        let g_vk = make_test_game("Vulkan", 64, Some("2.4.2"));
        assert!(!is_native_dlss_supported(&g_vk), "Vulkan games cannot use D3D12 Native DLSS");

        let g_dx11 = make_test_game("DirectX 11", 64, Some("2.4.2"));
        assert!(!is_native_dlss_supported(&g_dx11), "DX11 games cannot use D3D12 Native DLSS");

        let g_nodlss = make_test_game("DirectX 12", 64, None);
        assert!(!is_native_dlss_supported(&g_nodlss), "Games without DLSS cannot use Native DLSS");
    }

    #[test]
    fn test_install_route_and_opti_reason_methods() {
        assert_eq!(InstallRoute::Native.as_str(), "native");
        assert_eq!(InstallRoute::OptiScaler.as_str(), "optiscaler");
        assert_eq!(InstallRoute::Feeder.as_str(), "feeder");

        assert!(OptiReason::Unsupported.message().contains("64-bit"));
        assert!(OptiReason::NeedsDlss.message().contains("original DLSS"));
    }

    #[test]
    fn test_dx8_dx10_opengl_route_rules() {
        let g_dx10 = make_test_game("DirectX 10", 64, Some("2.4.2"));
        assert_eq!(routes_for(&g_dx10), Vec::<InstallRoute>::new());

        let g_dx8_32 = make_test_game("DirectX 8", 32, None);
        assert_eq!(routes_for(&g_dx8_32), vec![InstallRoute::Feeder]);

        let g_dx8_64 = make_test_game("DirectX 8", 64, None);
        assert_eq!(routes_for(&g_dx8_64), Vec::<InstallRoute>::new());

        let g_opengl_dlss = make_test_game("OpenGL", 64, Some("2.4.2"));
        // OpenGL check_opti_reason is Unsupported, so routes_for returns only Feeder
        assert_eq!(routes_for(&g_opengl_dlss), vec![InstallRoute::Feeder]);

        let g_unknown_bitness = make_test_game("DirectX 12", 16, Some("3.7.0"));
        assert_eq!(routes_for(&g_unknown_bitness), Vec::<InstallRoute>::new());
    }

    #[test]
    fn test_vulkan_dlss2_game_supports_frame_generation_injection() {
        let mut g = make_test_game("Vulkan", 64, Some("2.4.2"));
        g.can_inject_fg = true;
        g.has_frame_generation = false;

        assert!(is_frame_generation_supported(&g), "BG3 Vulkan must support frame generation injection");
        let (label, on) = frame_generation_status(&g);
        assert_eq!(label, "Supported (Vulkan Streamline)");
        assert!(on);
    }

    #[test]
    fn test_dx11_dlss2_game_rejects_frame_generation_injection() {
        let mut g = make_test_game("DirectX 11", 64, Some("2.4.2"));
        g.can_inject_fg = false;
        g.has_frame_generation = false;

        assert!(!is_frame_generation_supported(&g), "BG3 DX11 must reject frame generation injection");
        let (label, on) = frame_generation_status(&g);
        assert_eq!(label, "Unsupported (Requires Native DLSS-G)");
        assert!(!on);
    }

    #[test]
    fn test_native_dlssg_game_reports_native_status() {
        let mut g = make_test_game("DirectX 12", 64, Some("3.7.0"));
        g.has_frame_generation = true;
        g.can_inject_fg = false;

        assert!(is_frame_generation_supported(&g), "Native DLSS-G game must support frame generation");
        let (label, on) = frame_generation_status(&g);
        assert_eq!(label, "Supported (DLSS-G)");
        assert!(on);
    }

    #[test]
    fn test_raster_game_without_dlss_rejects_frame_generation() {
        let mut g = make_test_game("DirectX 11", 64, None);
        g.has_frame_generation = false;
        g.can_inject_fg = false;
        g.mfg_unlock_installed = false;

        assert!(!is_frame_generation_supported(&g), "Pure raster game without DLSS must NOT support frame generation");
        let (label, on) = frame_generation_status(&g);
        assert_eq!(label, "Unsupported (Requires Native DLSS-G)");
        assert!(!on);
    }

    #[test]
    fn test_switching_between_vulkan_and_dx11_updates_frame_generation_support() {
        // Initial state: Vulkan game with DLSS 2 (can_inject_fg = true)
        let mut g = make_test_game("Vulkan", 64, Some("2.4.2"));
        g.can_inject_fg = true;
        g.has_frame_generation = false;

        assert!(is_frame_generation_supported(&g), "Vulkan game must support frame generation");

        // User switches dropdown to DX11 executable (bg3_dx11.exe)
        g.api = "DirectX 11".to_string();
        // Even if can_inject_fg was momentarily stale or set, is_frame_generation_supported must strictly reject DX11
        assert!(!is_frame_generation_supported(&g), "DirectX 11 executable must strictly reject frame generation");

        // User switches dropdown back to Vulkan (bg3.exe)
        g.api = "Vulkan".to_string();
        assert!(is_frame_generation_supported(&g), "Vulkan executable must support frame generation again");
    }
}

