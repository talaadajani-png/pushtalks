//! A synchronous Tauri command runs on the main thread, where any blocking call (a dialog, audio
//! device setup) freezes the window. Every command must be `async` or `#[tauri::command(async)]`.

#[test]
fn no_command_runs_on_the_main_thread() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app/commands.rs"))
        .expect("read commands.rs");
    let lines: Vec<&str> = src.lines().collect();
    let bad: Vec<String> = lines
        .windows(2)
        .filter(|w| w[0].trim() == "#[tauri::command]" && w[1].trim_start().starts_with("fn "))
        .map(|w| w[1].trim().to_string())
        .collect();
    assert!(
        bad.is_empty(),
        "commands that would run on the main thread: {bad:?}"
    );
}
