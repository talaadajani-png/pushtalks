//! Audio in: WAV loading and resampling, plus live capture of the mic and system audio.

pub mod capture;
pub mod mic;
pub mod resample;
#[cfg(target_os = "macos")]
pub mod system_mac;
#[cfg(target_os = "windows")]
pub mod system_win;

/// Starts capturing everything the computer plays (the other people on a call).
#[cfg(target_os = "macos")]
pub use system_mac::start as start_system;
#[cfg(target_os = "windows")]
pub use system_win::start as start_system;
