
#[derive(Debug, Clone, PartialEq)]
pub struct EmulatorProfile {
    pub key: &'static str,
    pub name: &'static str,
    pub system: &'static str,
    pub exes: &'static [&'static str],
    pub apis: &'static [&'static str],
    pub hint: &'static str,
}

pub const PROFILES: &[EmulatorProfile] = &[
    EmulatorProfile {
        key: "duckstation",
        name: "DuckStation",
        system: "PlayStation 1",
        exes: &["duckstation-qt-x64.exe", "duckstation-qt-x64-releaseltcg.exe", "duckstation-nogui-x64.exe", "duckstation.exe"],
        apis: &["dxgi", "vulkan", "opengl"],
        hint: "Settings > Graphics > Renderer: Direct3D 11/12",
    },
    EmulatorProfile {
        key: "pcsx2",
        name: "PCSX2",
        system: "PlayStation 2",
        exes: &["pcsx2-qt.exe", "pcsx2x64.exe", "pcsx2x64-avx2.exe", "pcsx2.exe"],
        apis: &["dxgi", "vulkan", "opengl"],
        hint: "Settings > Graphics > Renderer: Direct3D 11/12",
    },
    EmulatorProfile {
        key: "dolphin",
        name: "Dolphin",
        system: "GameCube / Wii",
        exes: &["dolphin.exe", "dolphinqt.exe"],
        apis: &["dxgi", "vulkan", "opengl"],
        hint: "Graphics > Backend: Direct3D 11/12",
    },
    EmulatorProfile {
        key: "ppsspp",
        name: "PPSSPP",
        system: "PSP",
        exes: &["ppssppwindows64.exe", "ppssppwindows.exe"],
        apis: &["dxgi", "vulkan", "opengl"],
        hint: "Settings > Graphics > Backend: Direct3D 11",
    },
    EmulatorProfile {
        key: "xenia",
        name: "Xenia",
        system: "Xbox 360",
        exes: &["xenia.exe", "xenia_canary.exe"],
        apis: &["dxgi", "vulkan"],
        hint: "Use the Direct3D 12 backend",
    },
    EmulatorProfile {
        key: "cemu",
        name: "Cemu",
        system: "Wii U",
        exes: &["cemu.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Options > General settings > Graphics: Vulkan",
    },
    EmulatorProfile {
        key: "rpcs3",
        name: "RPCS3",
        system: "PlayStation 3",
        exes: &["rpcs3.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Configuration > GPU > Renderer: Vulkan",
    },
    EmulatorProfile {
        key: "ryujinx",
        name: "Ryujinx",
        system: "Nintendo Switch",
        exes: &["ryujinx.exe", "ryujinx.ava.exe", "ryujinx.headless.sdl2.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Settings > Graphics > Backend: Vulkan",
    },
    EmulatorProfile {
        key: "yuzu",
        name: "yuzu / suyu / Eden / Citron",
        system: "Nintendo Switch",
        exes: &["yuzu.exe", "suyu.exe", "eden.exe", "citron.exe", "sudachi.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Graphics API: Vulkan",
    },
    EmulatorProfile {
        key: "shadps4",
        name: "shadPS4",
        system: "PlayStation 4",
        exes: &["shadps4.exe"],
        apis: &["vulkan"],
        hint: "Vulkan renderer",
    },
    EmulatorProfile {
        key: "azahar",
        name: "Azahar / Citra / Lime3DS",
        system: "Nintendo 3DS",
        exes: &["azahar.exe", "citra.exe", "citra-qt.exe", "lime3ds.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Graphics API: Vulkan",
    },
    EmulatorProfile {
        key: "melonds",
        name: "melonDS",
        system: "Nintendo DS",
        exes: &["melonds.exe"],
        apis: &["opengl"],
        hint: "OpenGL renderer",
    },
    EmulatorProfile {
        key: "flycast",
        name: "Flycast",
        system: "Dreamcast",
        exes: &["flycast.exe"],
        apis: &["dxgi", "vulkan", "opengl"],
        hint: "Video > Renderer: DirectX 11",
    },
    EmulatorProfile {
        key: "xemu",
        name: "xemu",
        system: "Xbox",
        exes: &["xemu.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Renderer: Vulkan",
    },
    EmulatorProfile {
        key: "vita3k",
        name: "Vita3K",
        system: "PlayStation Vita",
        exes: &["vita3k.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Backend Renderer: Vulkan",
    },
    EmulatorProfile {
        key: "retroarch",
        name: "RetroArch",
        system: "Multi-system",
        exes: &["retroarch.exe"],
        apis: &["dxgi", "vulkan", "opengl"],
        hint: "Video driver: d3d11 or d3d12",
    },
    EmulatorProfile {
        key: "mgba",
        name: "mGBA",
        system: "Game Boy Advance",
        exes: &["mgba.exe"],
        apis: &["opengl"],
        hint: "OpenGL renderer",
    },
    EmulatorProfile {
        key: "snes9x",
        name: "Snes9x",
        system: "SNES",
        exes: &["snes9x-x64.exe", "snes9x.exe"],
        apis: &["dxgi"],
        hint: "Output method: Direct3D",
    },
    EmulatorProfile {
        key: "play",
        name: "Play!",
        system: "PlayStation 2",
        exes: &["play.exe"],
        apis: &["vulkan", "opengl"],
        hint: "Renderer: Vulkan",
    },
];

pub fn profile_for_exe(exe_name: &str) -> Option<&'static EmulatorProfile> {
    let lower = exe_name.to_lowercase();
    for p in PROFILES {
        for exe in p.exes {
            if exe.eq_ignore_ascii_case(&lower) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_for_exe_resolution() {
        assert_eq!(profile_for_exe("duckstation-qt-x64.exe").map(|p| p.name), Some("DuckStation"));
        assert_eq!(profile_for_exe("PCSX2.EXE").map(|p| p.name), Some("PCSX2"));
        assert_eq!(profile_for_exe("dolphin.exe").map(|p| p.name), Some("Dolphin"));
        assert_eq!(profile_for_exe("cemu.exe").map(|p| p.name), Some("Cemu"));
        assert_eq!(profile_for_exe("xenia.exe").map(|p| p.name), Some("Xenia"));
        assert_eq!(profile_for_exe("rpcs3.exe").map(|p| p.name), Some("RPCS3"));
        assert!(profile_for_exe("yuzu.exe").map(|p| p.name.to_lowercase()).unwrap().contains("yuzu"));
        assert_eq!(profile_for_exe("ryujinx.exe").map(|p| p.name), Some("Ryujinx"));
        assert_eq!(profile_for_exe("random_game.exe"), None);
    }
}
