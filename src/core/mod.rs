pub mod pe;
pub mod gpu;
pub mod scan;
pub mod journal;
pub mod optiscaler;
pub mod mfg_unlock;
pub mod state;
pub mod install_guards;
pub mod compatibility;
pub mod emulators;
pub mod steamart;
pub mod install_routes;

pub mod i18n;
pub mod overlay_bridge;
pub mod tray;
pub mod logger;
pub mod single_instance;
pub mod downloader;
pub mod vulkan_layer;
pub mod overlay_preview_window;
 
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_version_matches_package() {
        assert_eq!(APP_VERSION, "1.0.6");
        assert!(!APP_VERSION.is_empty());
    }
}
