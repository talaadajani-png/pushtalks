fn main() {
    #[cfg(target_os = "macos")]
    swift_link_paths();
    tauri_build::build()
}

/// screencapturekit links Swift's compatibility libraries but only searches the full-Xcode
/// toolchain path. Add the active developer dir's paths so Command Line Tools-only Macs link too.
#[cfg(target_os = "macos")]
fn swift_link_paths() {
    let dev = std::process::Command::new("xcode-select")
        .arg("-p")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "/Library/Developer/CommandLineTools".into());
    for sub in [
        "usr/lib/swift/macosx",
        "Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx",
    ] {
        let p = std::path::Path::new(&dev).join(sub);
        if p.exists() {
            println!("cargo:rustc-link-search=native={}", p.display());
        }
    }
    // The Swift runtime itself ships with macOS.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
