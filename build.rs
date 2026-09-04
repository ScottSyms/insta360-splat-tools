fn main() {
    println!("cargo:rerun-if-changed=tools/vision_person.swift");
    // Auto-compile Swift Vision helper on macOS
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "macos" {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let swift_src = format!("{}/tools/vision_person.swift", manifest_dir);
        if !std::path::Path::new(&swift_src).exists() {
            return;
        }
        let dest = format!("{}/../../../vision-person", out_dir);
        let _ = std::process::Command::new("swiftc")
            .arg(&swift_src)
            .arg("-o")
            .arg(&dest)
            .status();
        // Also ensure target/debug and release have it for runtime discovery
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let target_bin = format!("target/{}/vision-person", profile);
        let _ = std::process::Command::new("swiftc")
            .arg(&swift_src)
            .arg("-o")
            .arg(&target_bin)
            .status();
    }
}
