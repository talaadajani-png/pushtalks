# PushTalks: project rules for Claude Code

## What this is
Local-first dictation and meeting transcription desktop app for PUSH Media staff (macOS + Windows).
Read `PRD.md` before making architectural changes. Keep `PRD.md` (and the shared PRD doc) updated as
features and decisions change; record measured results next to the targets they validate.

The previous Electron prototype is archived on the `electron-v1` branch. Borrow ideas from it, not code
that conflicts with this stack.

## Hard rules
- No network calls anywhere except `src-tauri/src/models/download.rs`.
- No telemetry, analytics, or crash reporting services.
- Never commit audio files or real meeting transcripts. `tests/fixtures/` holds only public-domain or
  self-recorded clips.
- Every model file must have a pinned URL and SHA-256 in `src-tauri/src/models/manifest.rs`; refuse to
  load on mismatch.
- Never send more than 30 seconds of audio to the ASR encoder in one call.
- All inference runs off the UI thread.
- Meeting recording never starts without the user's consent confirmation.
- Never log transcript text, audio, or secrets.
- Approvals (pilot go-ahead, consent policy, retention defaults, any cloud AI use) belong to Tala's
  supervisor and senior leadership. Don't build features that assume an approval that hasn't been given.

## Stack (decided in PRD.md; don't swap without asking)
- Tauri 2, Rust backend, React + TypeScript frontend.
- sherpa-onnx (official `sherpa-onnx` crate) for ASR (Parakeet TDT 0.6B v2 int8, English-only; v3 available as an optional model), VAD (Silero), diarization.
- cpal for mic; ScreenCaptureKit (macOS) and WASAPI loopback (Windows) for system audio.
- SQLite with FTS5 for the library.

## Conventions
- Rust: clippy clean, rustfmt, no `unwrap()` outside tests.
- Frontend: functional components, no browser storage for app data (use Tauri commands).
- UI colors: background #111111, text #FFFFFF, accent #8A66FF. Font: Work Sans.
- Transcript output format is defined in PRD.md §6.4. Do not change it without updating the PRD.
- Verify crate and model details against their sources rather than assuming from memory.
- When a decision isn't covered here or in PRD.md, ask Tala instead of guessing.

## Commands (run from `src-tauri/`)
- Fetch + verify models: `cargo run --release --bin pushtalks-cli -- fetch-models`
- Transcribe a WAV: `cargo run --release --bin pushtalks-cli -- transcribe <file.wav>`
- Benchmark: `cargo run --release --bin benchmark -- <file.wav> [reference.txt]`
- Test: `cargo test`; frontend: `npm test` (from the repo root)
- Run the app: `npm run tauri dev` (from the repo root)
- Lint: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
