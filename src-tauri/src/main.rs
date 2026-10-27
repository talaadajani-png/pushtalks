// Hides the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // A short-lived helper run of the app: macOS caches permission answers inside a running
    // process, so the app asks a fresh copy of itself (see app::permissions::status_fresh).
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--permissions-out" {
        pushtalks_lib::app::permissions::write_status(std::path::Path::new(&args[2]));
        return;
    }
    pushtalks_lib::app::run();
}
