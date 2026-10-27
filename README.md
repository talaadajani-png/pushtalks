# PushTalks

Local-first dictation and meeting transcription for PUSH Media staff, on macOS and Windows. Speech
recognition runs on your own computer with NVIDIA Parakeet. No audio leaves the device.

See [PRD.md](PRD.md) for what we're building and why. [CLAUDE.md](CLAUDE.md) has the project rules.

> **Status: full app built as a draft for review** (PRD §19). Supervisor approval for a pilot,
> consent wording and retention defaults is still pending (PRD §17). The earlier Electron prototype
> is archived on the `electron-v1` branch.

## Run the app (macOS)

You need Rust (`rustup`), Node 20+, and the Xcode Command Line Tools.

```bash
npm install
npm run tauri dev        # development build with hot reload
npm run tauri build      # release .app and .dmg in src-tauri/target/release/bundle/
```

On the build Mac, `npm run bundle:mac` builds the app and, if `~/.pushtalks-signing/signing.pem`
and `rcodesign` exist, re-signs it so macOS permissions survive rebuilds (see `scripts/sign-local.sh`).

The release build is unsigned, so macOS asks you to confirm the first time you open it
(right-click > Open). Onboarding asks for the microphone, Accessibility (to paste), Input
Monitoring (the hold-to-talk key) and Screen Recording (meeting audio), then downloads the models.

## Minutes with Claude

PushTalks includes `pushtalks-mcp`, a local connector so Claude can write minutes for your meetings
(PRD §6.8). Settings > Use with Claude shows the exact setup. For a release build it's:

```bash
claude mcp add --scope user pushtalks -- "/Applications/PushTalks.app/Contents/MacOS/pushtalks-mcp"
```

Then ask Claude: "Write minutes for my latest PushTalks meeting." Claude saves them into the
meeting's Notes section, tagged [AI: DRAFT]. This sends the chosen transcript to Claude, so check with
your supervisor before using it on client meetings.

## Command-line tools

You need Rust (`rustup`) and the Xcode Command Line Tools.

```bash
cd src-tauri
cargo build --release
./target/release/pushtalks-cli fetch-models      # one-time ~490 MB download, SHA-256 checked
./target/release/pushtalks-cli transcribe path/to/recording.wav
```

The `fetch-models` step:
- Installs Parakeet v2 int8 (661 MB, English) and Silero VAD into `~/Library/Application Support/PushTalks/models/`, or `%APPDATA%\PushTalks\models\` on Windows.
- Is the only time PushTalks uses the network.
- Resumes if it's interrupted.

To also install the multilingual v3 model for comparison, run `fetch-models --model v3`, then pass `--model v3` to `benchmark`.

`transcribe` accepts any WAV file: any sample rate, mono or stereo. Add `--json` to get segments
with per-word timestamps.

## Benchmark

```bash
./target/release/benchmark recording.wav reference.txt
```

This reports:
- Segment count and length (always under the 30 s encoder limit).
- Model load time.
- Processing time and real-time factor.
- Peak RAM.
- Word error rate against `reference.txt`.

Keep real recordings and reference transcripts outside the repo. `.gitignore` blocks audio files,
and nothing with client content may be committed.

## Development

```bash
cd src-tauri
cargo test                                        # unit tests + the no-network guard
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cd .. && npm test                                 # frontend component tests
```

Layout:

| Path | What's there |
|---|---|
| `src-tauri/src/models/` | Pinned model manifest (`manifest.rs`) and the only networked module (`download.rs`) |
| `src-tauri/src/asr/` | Parakeet engine and Silero VAD segmentation |
| `src-tauri/src/audio/` | WAV loading and resampling to 16 kHz mono |
| `src-tauri/src/metrics.rs` | WER and peak-memory helpers |
| `src-tauri/src/bin/pushtalks-cli.rs` | CLI: fetch/verify models, transcribe a WAV, process a recording folder (`meeting`) |
| `src-tauri/src/app/` | Tauri app: setup, tray, commands, dictation and meeting controllers |
| `src-tauri/src/dictation/` | Hold-key listener, cleanup rules, clipboard paste |
| `src-tauri/src/meeting/` | Recorder, windowed processing, track merge |
| `src-tauri/src/mcp.rs`, `src-tauri/src/bin/pushtalks-mcp.rs` | The Claude connector (MCP over stdio) |
| `src-tauri/src/db/`, `src-tauri/src/vocab/` | SQLite library + FTS5, learned spellings |
| `src-tauri/src/output/` | Markdown and JSON transcript writers (PRD §6.4) |
| `src/` | React frontend (views, components, styles) |
| `scripts/benchmark.rs` | Benchmark harness |
| `tests/fixtures/`, `tests/golden/` | Public-domain or self-recorded clips only |

Third-party licenses and attribution, including NVIDIA's for Parakeet (CC BY 4.0), are in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
