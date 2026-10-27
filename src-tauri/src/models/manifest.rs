//! Pinned model sources. Every file PushTalks loads is listed here with its SHA-256;
//! anything that doesn't match is refused (PRD §7.3).

use std::path::{Path, PathBuf};

/// One file the app loads at runtime, with the hash it must match.
#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    /// File name inside the model's install folder.
    pub name: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

/// Where a model comes from and what it installs.
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub license: &'static str,
    pub url: &'static str,
    /// SHA-256 of the download itself (archive or single file).
    pub download_sha256: &'static str,
    pub download_size: u64,
    /// For archives: the folder inside the archive that holds `files`.
    pub archive_root: Option<&'static str>,
    pub files: &'static [ModelFile],
}

impl ModelSpec {
    /// The folder this model is installed into.
    pub fn install_dir(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.id)
    }

    pub fn file_path(&self, models_dir: &Path, name: &str) -> PathBuf {
        self.install_dir(models_dir).join(name)
    }

    pub fn installed_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// NVIDIA Parakeet TDT 0.6B v3 (multilingual), int8 ONNX export packaged by k2-fsa/sherpa-onnx.
/// Optional since 2026-10-21: installed only on request (`fetch-models --model v3`).
/// Release asset `asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2` (2025-08-16).
pub const PARAKEET_V3_INT8: ModelSpec = ModelSpec {
    id: "parakeet-tdt-0.6b-v3-int8",
    label: "NVIDIA Parakeet TDT 0.6B v3 (int8)",
    license: "CC BY 4.0 (NVIDIA); ONNX export by k2-fsa/sherpa-onnx",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
    download_sha256: "5793d0fd397c5778d2cf2126994d58e9d56b1be7c04d13c7a15bb1b4eafb16bf",
    download_size: 487_170_055,
    archive_root: Some("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8"),
    files: PARAKEET_FILES,
};

/// Silero VAD, as published in sherpa-onnx `asr-models/silero_vad.onnx` (2025-07-11).
pub const SILERO_VAD: ModelSpec = ModelSpec {
    id: "silero-vad",
    label: "Silero VAD",
    license: "MIT",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx",
    download_sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
    download_size: 643_854,
    archive_root: None,
    files: &[ModelFile {
        name: "silero_vad.onnx",
        sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
        size: 643_854,
    }],
};

// Per-file hashes of the extracted Parakeet files, pinned from the verified archive above.
const PARAKEET_FILES: &[ModelFile] = &[
    ModelFile {
        name: "encoder.int8.onnx",
        sha256: "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247",
        size: 652_184_281,
    },
    ModelFile {
        name: "decoder.int8.onnx",
        sha256: "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e",
        size: 11_845_275,
    },
    ModelFile {
        name: "joiner.int8.onnx",
        sha256: "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3",
        size: 6_355_277,
    },
    ModelFile {
        name: "tokens.txt",
        sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
        size: 93_939,
    },
];

/// Every model the app needs, in install order.
/// v2 became the default on 2026-10-21 after beating v3 on real PUSH audio (PRD §18).
/// The two diarization models (about 46 MB) are installed with the rest so speaker labels work
/// offline; meetings still process without them, with "Others" as the only remote speaker.
pub const ALL_MODELS: &[ModelSpec] = &[
    SILERO_VAD,
    PARAKEET_V2_INT8,
    PYANNOTE_SEGMENTATION,
    TITANET_SMALL,
];

/// pyannote segmentation 3.0 (MIT, CNRS), ONNX export by k2-fsa/sherpa-onnx. Finds speaker turns
/// for diarization (PRD §6.3). Release asset `speaker-segmentation-models/
/// sherpa-onnx-pyannote-segmentation-3-0.tar.bz2`; hashes computed 2026-10-22.
pub const PYANNOTE_SEGMENTATION: ModelSpec = ModelSpec {
    id: "pyannote-segmentation-3-0",
    label: "pyannote segmentation 3.0",
    license: "MIT (CNRS); ONNX export by k2-fsa/sherpa-onnx",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2",
    download_sha256: "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488",
    download_size: 6_958_444,
    archive_root: Some("sherpa-onnx-pyannote-segmentation-3-0"),
    files: &[ModelFile {
        name: "model.onnx",
        sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
        size: 5_992_913,
    }],
};

/// NVIDIA TitaNet small speaker embeddings (CC BY 4.0), ONNX from sherpa-onnx release
/// `speaker-recongition-models/nemo_en_titanet_small.onnx` (the tag's spelling is upstream's).
pub const TITANET_SMALL: ModelSpec = ModelSpec {
    id: "titanet-small",
    label: "NVIDIA TitaNet small (speaker embeddings)",
    license: "CC BY 4.0 (NVIDIA); ONNX export by k2-fsa/sherpa-onnx",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx",
    download_sha256: "ad4a1802485d8b34c722d2a9d04249662f2ece5d28a7a039063ca22f515a789e",
    download_size: 40_257_283,
    archive_root: None,
    files: &[ModelFile {
        name: "nemo_en_titanet_small.onnx",
        sha256: "ad4a1802485d8b34c722d2a9d04249662f2ece5d28a7a039063ca22f515a789e",
        size: 40_257_283,
    }],
};

/// NVIDIA Parakeet TDT 0.6B v2 (English-only), int8 ONNX export by k2-fsa/sherpa-onnx.
/// Release asset `asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2` (2025-08-16).
/// The default speech model (PRD §7.1).
pub const PARAKEET_V2_INT8: ModelSpec = ModelSpec {
    id: "parakeet-tdt-0.6b-v2-int8",
    label: "NVIDIA Parakeet TDT 0.6B v2, English-only (int8)",
    license: "CC BY 4.0 (NVIDIA); ONNX export by k2-fsa/sherpa-onnx",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2",
    download_sha256: "157c157bc51155e03e37d2466522a3a737dd9c72bb25f36eb18912964161e1ad",
    download_size: 482_468_385,
    archive_root: Some("sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8"),
    files: &[
        ModelFile {
            name: "encoder.int8.onnx",
            sha256: "a32b12d17bbbc309d0686fbbcc2987b5e9b8333a7da83fa6b089f0a2acd651ab",
            size: 652_184_296,
        },
        ModelFile {
            name: "decoder.int8.onnx",
            sha256: "b6bb64963457237b900e496ee9994b59294526439fbcc1fecf705b31a15c6b4e",
            size: 7_257_753,
        },
        ModelFile {
            name: "joiner.int8.onnx",
            sha256: "7946164367946e7f9f29a122407c3252b680dbae9a51343eb2488d057c3c43d2",
            size: 1_739_080,
        },
        ModelFile {
            name: "tokens.txt",
            sha256: "ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d",
            size: 9_384,
        },
    ],
};

/// Speech models PushTalks can run. The first is the default (PRD §7.1).
pub const ASR_MODELS: &[ModelSpec] = &[PARAKEET_V2_INT8, PARAKEET_V3_INT8];

/// Looks up a speech model by short name ("v3", "v2") or full id.
pub fn asr_model(name: &str) -> Option<&'static ModelSpec> {
    ASR_MODELS
        .iter()
        .find(|m| m.id == name || m.id.starts_with(&format!("parakeet-tdt-0.6b-{name}")))
}

/// The OS app data folder from PRD §7.3:
/// `~/Library/Application Support/PushTalks/models/` (macOS), `%APPDATA%\PushTalks\models\` (Windows).
/// `PUSHTALKS_MODELS_DIR` overrides it (tests, IT-managed installs).
pub fn models_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PUSHTALKS_MODELS_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PushTalks")
        .join("models")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_pinned() {
        for m in ALL_MODELS.iter().chain(ASR_MODELS) {
            assert_eq!(m.download_sha256.len(), 64, "{} download hash", m.id);
            assert!(m.url.starts_with("https://"), "{} url", m.id);
            for f in m.files {
                assert_eq!(f.sha256.len(), 64, "{}/{} hash", m.id, f.name);
                assert!(f.sha256.chars().all(|c| c.is_ascii_hexdigit()));
                assert!(f.size > 0, "{}/{} size", m.id, f.name);
            }
        }
    }

    #[test]
    fn finds_speech_models_by_short_name() {
        assert_eq!(
            asr_model("v3").map(|m| m.id),
            Some("parakeet-tdt-0.6b-v3-int8")
        );
        assert_eq!(
            asr_model("v2").map(|m| m.id),
            Some("parakeet-tdt-0.6b-v2-int8")
        );
        assert!(asr_model("v9").is_none());
    }
}
