# Third-party notices

PushTalks uses the following third-party components. This file ships with the app and is shown
in Settings > About (PRD §11).

## Models (downloaded on first launch, not bundled)

| Component | License | Attribution |
|---|---|---|
| NVIDIA Parakeet TDT 0.6B v2 (default) | CC BY 4.0 | Model by NVIDIA (nvidia/parakeet-tdt-0.6b-v2). int8 ONNX export by the k2-fsa/sherpa-onnx project. |
| NVIDIA Parakeet TDT 0.6B v3 (optional) | CC BY 4.0 | Model by NVIDIA (nvidia/parakeet-tdt-0.6b-v3). int8 ONNX export by the k2-fsa/sherpa-onnx project. |
| Silero VAD | MIT | Silero Team (snakers4/silero-vad), ONNX file as published by k2-fsa/sherpa-onnx. |
| pyannote segmentation 3.0 | MIT | Copyright (c) 2022 CNRS (pyannote/segmentation-3.0). ONNX export by k2-fsa/sherpa-onnx. Used for speaker diarization. |
| NVIDIA TitaNet small | CC BY 4.0 | Speaker embedding model by NVIDIA (nvidia/speakerverification_en_titanet_small, NeMo). ONNX export by k2-fsa/sherpa-onnx. Used for speaker diarization. |

## Libraries linked into the app

| Component | License |
|---|---|
| sherpa-onnx (k2-fsa) | Apache 2.0 |
| ONNX Runtime (Microsoft) | MIT |
| kaldi-native-fbank, kaldi-decoder (k2-fsa) | Apache 2.0 |
| OpenFst | Apache 2.0 |
| KISS FFT | BSD 3-Clause |
| SentencePiece (Google) | Apache 2.0 |
| piper-phonemize (partly linked via the sherpa-onnx static build) | MIT |
| Tauri 2 and plugins (global-shortcut, autostart, dialog, opener, single-instance) | MIT or Apache 2.0 |
| cpal | Apache 2.0 |
| enigo, rusqlite | MIT |
| SQLite (bundled by libsqlite3-sys) | Public domain |
| Rust crates: anyhow, clap, hound, serde, serde_json, sha2, hex, bzip2, tar, ureq, dirs, arboard, chrono, uuid, crossbeam-channel, parking_lot, screencapturekit, apple-cf, core-graphics, core-foundation, libc, windows / windows-sys | MIT or Apache 2.0 |

## Frontend

| Component | License |
|---|---|
| React, React DOM | MIT |
| @tauri-apps/api | MIT or Apache 2.0 |
| Work Sans font (via @fontsource/work-sans, bundled offline) | SIL Open Font License 1.1 |

The sherpa-onnx static libraries also contain espeak-ng (GPL v3) for text-to-speech. PushTalks does
not use text-to-speech, and espeak-ng code is not linked into the PushTalks binaries (checked
2026-10-20 with `nm`). Re-check this whenever sherpa-onnx is upgraded.

Full license texts will be collected here before the pilot build.
