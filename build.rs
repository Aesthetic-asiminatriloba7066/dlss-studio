fn main() {
    #[cfg(windows)]
    {
        // Ensure the Windows 10 SDK rc.exe path is present in PATH for winres
        if let Ok(path) = std::env::var("PATH") {
            let win_sdk_bin = r"D:\Windows Kits\10\bin\10.0.22621.0\x64";
            if std::path::Path::new(win_sdk_bin).exists() && !path.contains(win_sdk_bin) {
                std::env::set_var("PATH", format!("{};{}", win_sdk_bin, path));
            }
        }

        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "DLSS 5 Studio - Ultra-low latency DLSS 5 & OptiScaler Manager");
        res.set("ProductName", "DLSS 5 Studio");
        res.set("OriginalFilename", "dlss-studio.exe");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=Failed to compile Windows resource: {}", e);
        }
    }

    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| ".".to_string());
    let payload_dest = std::path::Path::new(&out_dir).join("installer_payload.bin");
    let exe_path = std::path::Path::new("target/release/dlss-studio.exe");
    let debug_exe_path = std::path::Path::new("target/debug/dlss-studio.exe");
    if exe_path.exists() {
        let _ = std::fs::copy(exe_path, &payload_dest);
    } else if debug_exe_path.exists() {
        let _ = std::fs::copy(debug_exe_path, &payload_dest);
    } else if !payload_dest.exists() {
        let _ = std::fs::write(&payload_dest, &[]);
    }
    println!("cargo:rerun-if-changed=target/release/dlss-studio.exe");
    println!("cargo:rerun-if-changed=target/debug/dlss-studio.exe");
}
