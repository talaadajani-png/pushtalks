pub mod download;
pub mod manifest;

pub use manifest::{
    asr_model, models_dir, ModelSpec, ALL_MODELS, PARAKEET_V2_INT8, PARAKEET_V3_INT8,
    PYANNOTE_SEGMENTATION, SILERO_VAD, TITANET_SMALL,
};
