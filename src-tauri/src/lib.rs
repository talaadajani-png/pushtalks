//! PushTalks core: models, audio, VAD and ASR, dictation, meetings, the library and vocabulary.
//! The Tauri app (`app`), the `pushtalks-cli` and the `benchmark` binaries all call these modules.

pub mod app;
pub mod asr;
pub mod audio;
pub mod calls;
pub mod db;
pub mod dictation;
pub mod mcp;
pub mod meeting;
pub mod metrics;
pub mod models;
pub mod output;
pub mod pipeline;
pub mod settings;
pub mod titles;
pub mod vocab;
pub mod voices;
