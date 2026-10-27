//! Keeps the speech model loaded while it's in use and unloads it after an idle period
//! (PRD §7.2), so dictation is instant without holding ~1 GB of RAM all day.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;

use crate::asr::VadSettings;
use crate::models::{self, ModelSpec};
use crate::pipeline::{self, Loaded};

pub struct AsrService {
    models_dir: PathBuf,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    loaded: Option<(Arc<Loaded>, &'static ModelSpec)>,
    last_used: Option<Instant>,
}

impl AsrService {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            state: Mutex::new(State::default()),
        }
    }

    /// The loaded model, loading (and verifying checksums) first if needed. Slow on first call:
    /// call it from a background thread only.
    pub fn get(&self, model: &str) -> Result<Arc<Loaded>> {
        let spec =
            models::asr_model(model).ok_or_else(|| anyhow!("unknown speech model {model}"))?;
        let mut st = self.state.lock();
        st.last_used = Some(Instant::now());
        if let Some((l, s)) = &st.loaded {
            if s.id == spec.id {
                return Ok(l.clone());
            }
        }
        st.loaded = None;
        let loaded = Arc::new(pipeline::load_model(
            &self.models_dir,
            spec,
            pipeline::default_threads(),
            VadSettings::default(),
            false,
        )?);
        st.loaded = Some((loaded.clone(), spec));
        Ok(loaded)
    }

    pub fn is_loaded(&self) -> bool {
        self.state.lock().loaded.is_some()
    }

    /// Frees the model if it hasn't been used for `idle`. Work in progress keeps its own `Arc`.
    pub fn unload_if_idle(&self, idle: Duration) {
        let mut st = self.state.lock();
        if st.loaded.is_some() && st.last_used.is_some_and(|t| t.elapsed() >= idle) {
            st.loaded = None;
        }
    }

    pub fn unload(&self) {
        self.state.lock().loaded = None;
    }
}
