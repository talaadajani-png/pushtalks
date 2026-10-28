# PushTalks PRD

**[AI: DRAFT]** Draft for human review. Owner: Tala Aldajani, AI & Strategy Associate, PUSH Media.
Version 0.1 | October 2026 | Status: Pending approval

> All model sizes, speeds, and accuracy figures in this document come from public model cards and community benchmarks. Treat them as targets to validate in Phase 0, not commitments.

---

## 1. Summary

PushTalks is a free, local-first desktop app for PUSH Media staff with two modes:

1. **Dictation mode.** Hold a hotkey, speak, release. Clean text is typed into whatever app has focus (Slack, Outlook, Notion, a browser field). Similar in feel to Wispr Flow.
2. **Meeting mode.** Record a meeting (mic plus system audio), then transcribe it on the device after the meeting ends and output a transcript in one unified PUSH doc format.

All speech recognition runs on the user's own computer. No audio leaves the device. The app uses open-weight models, so there are no per-seat or per-minute costs.

## 2. Problem

- Meeting notes across PUSH are inconsistent in format, depth, and where they live.
- Paid dictation and note-taker tools cost money per seat and usually send audio to third-party clouds, which creates client-data governance risk.
- Staff already use a Claude transcript-summary skill, but getting a clean transcript into it is a manual step.

## 3. Goals

- Accurate English transcription that runs fully offline on standard PUSH laptops. (French was dropped as a v1 goal on 2026-10-21 because PUSH meetings are almost all in English. The default model, Parakeet v2, is English-only; the optional v3 model handles French if it's needed later.)
- One consistent meeting transcript format across the agency.
- Total install footprint under 1 GB including models.
- Zero recurring software cost.
- Transcript output that feeds directly into PUSH's existing transcript-summary workflow.

## 4. Non-goals (v1)

- Real-time live captions during meetings.
- Cloud sync, shared team workspaces, or a server backend.
- Automatic upload of transcripts anywhere.
- Mobile apps.
- Bot that joins meetings as a participant.
- Languages other than English. The optional Parakeet v3 model covers 25 European languages if that changes (see Phase 4 for other fallbacks).

## 5. Users and use cases

| User | Use case |
|---|---|
| Account and strategy staff | Record client calls and internal syncs, get a clean transcript for recaps and action items |
| Media planners and buyers | Dictate long Slack messages, emails, and notes quickly |
| Leadership | Consistent meeting records without adopting a paid tool |
| Canadian office | English meetings. Occasional French isn't validated in v1 |

### User stories

- As a user, I hold a hotkey, speak a sentence, and see the text appear in my active window within about 1 to 2 seconds of releasing the key.
- As a user, I click Record before a Teams call and click Stop after. A few minutes later I have a transcript with timestamps and "Me" vs "Others" speaker labels.
- As a user, I rename speakers ("Speaker 2" becomes "Jordan") and the transcript updates everywhere.
- As a user, I export the transcript as Markdown and drop it into Claude with the transcript-summary skill.
- As a user, I know audio is deleted after transcription unless I chose to keep it.
- As an admin, I can confirm the app makes no network calls after the one-time model download.

## 6. Feature requirements

### 6.1 Dictation mode (P0)

- Global push-to-talk hotkey, user-configurable. Default: hold Right Option (macOS) / Right Alt (Windows).
- Optional toggle mode (press once to start, again to stop).
- On release: transcribe, apply light cleanup, insert text into the focused app.
- Text insertion via clipboard plus simulated paste, then restore the user's previous clipboard contents.
- Small floating indicator (recording, processing, done) that never steals focus.
- Cleanup rules (local, rule-based in v1): trim filler words ("um", "uh"), fix spacing around punctuation, capitalize sentence starts. Model already outputs punctuation and capitalization.
- Learned vocabulary (P1, see §6.7): correct a word once in PushTalks and it's spelled right from then on. Examples: "push media" to "PUSH Media", people's and clients' names, platform abbreviations, internal acronyms.
- Max dictation length: 2 minutes per press (configurable).

### 6.2 Meeting mode (P0)

- Start and Stop buttons in the main window and the tray/menu bar menu.
- Captures two separate tracks:
  - **Mic track** = the local user, labeled "Me".
  - **System audio track** = everyone else on the call.
- Works with any meeting app (Teams desktop, Zoom, Google Meet in a browser) because it captures at the OS level.
- Recording written to disk in chunks while recording so a crash does not lose the meeting.
- Consent prompt before every recording (see Section 10).
- Processing starts automatically on Stop. Progress bar with estimated time remaining.
- User can keep working while processing runs in the background at reduced priority.

### 6.3 Speaker labels (P0 basic, P1 full)

- **P0:** Track-based labels. Mic audio is "Me", system audio is "Others". This is free, fast, and accurate for most remote calls.
- **P1:** On-device speaker diarization on the system audio track to split "Others" into Speaker 1, Speaker 2, and so on.
- User can rename speakers after processing.
- In-room meetings (mic only): diarization also runs on the mic track. If it hears one voice, the label stays "Me"; with more, they're split into speakers (built 2026-10-22).
- **Voice memory (built 2026-10-22, off by default):** when the user renames "Speaker 2" to a person's name, PushTalks saves that speaker's voiceprint (a TitaNet embedding, numbers not audio) in the local library. Later meetings name that voice automatically; unknown voices stay "Speaker N".
  - Every person can be forgotten individually, or all at once (Settings > Speaker names).
  - Matching uses cosine similarity ≥ 0.6. On synthetic text-to-speech voices, the same voice scored 0.96 and different voices ≤ 0.27; this needs verifying on real meetings.
  - **Governance:** voiceprints are biometric data (for example under Illinois BIPA and Quebec Law 25). Turning this on needs supervisor approval and, likely, legal review and notice to the people recorded (§17).

### 6.4 Unified transcript format (P0)

Every meeting produces one Markdown file and one JSON sidecar with identical content.

**Markdown template:**

```markdown
---
title: "<user-entered or auto: Meeting YYYY-MM-DD HH:MM>"
date: 2026-10-20
start_time: "14:00"
duration_minutes: 42
speakers: ["Me", "Speaker 1", "Speaker 2"]
languages_detected: ["en"]
asr_model: "parakeet-tdt-0.6b-v2-int8"
app_version: "0.1.0"
ai_tag: "[AI: DRAFT]"
confidentiality: "Internal. May contain confidential client information."
---

# <title>

**Date:** 2026-10-20 | **Duration:** 42 min | **Speakers:** Me, Speaker 1, Speaker 2

## Notes
<!-- Empty until you ask Claude for minutes through the PushTalks connector (§6.8). -->

## Transcript

**[00:00:04] Me:** Thanks everyone for joining...

**[00:00:19] Speaker 1:** Happy to be here. Quick update on...
```

**JSON sidecar** (`<same-name>.json`): metadata block, an optional `notes` string (the minutes in Markdown, present once written, §6.8), plus an array of segments:
`{ "start": 4.12, "end": 18.40, "speaker": "Me", "text": "...", "words": [{ "w": "Thanks", "s": 4.12, "e": 4.40 }] }`

When a learned spelling (§6.7) changed a segment, the segment also carries `"original_text"`, the model's raw output. That keeps corrections auditable and reversible. The Markdown file shows only the corrected text.

Rules:
- Merge consecutive segments from the same speaker into one paragraph, with a new timestamp every 60 seconds or on speaker change.
- File name: `YYYY-MM-DD_HHMM_<slugified-title>.md`.
- Saved to a user-chosen folder (default: `~/Documents/PushTalks/`).
- The transcript structure must be clean input for the existing PUSH transcript-summary Claude skill.

### 6.5 Library (P1)

- List of past meetings with title, date, duration.
- Search across transcript text (local full-text index, SQLite FTS5).
- Open, rename, re-export, delete.

### 6.6 Settings (P0)

- Hotkey configuration.
- Microphone selection.
- Output folder.
- Audio retention: Delete after transcription (default) / Keep for N days / Keep forever.
- Model status: installed version, size on disk, re-download, verify checksum.
- Vocabulary (P1): view, search, edit and delete learned spellings; add new ones; import and export (P2).
- Launch at login (off by default).

### 6.7 Learned vocabulary (P1, pulled forward to Phase 2)

**Goal: correct a word once, and PushTalks remembers it.** When a user fixes a misheard or misspelled word inside PushTalks, the app offers to remember the fix and applies it to every future transcript and dictation on that computer. This targets the name-and-term errors found in Phase 0 (about 1 in 6 errors on real meetings, §18).

**Where corrections come from:**
- **Meeting transcript editor:** the user edits a word or phrase in a transcript. This is the main source.
- **Dictation history:** a short list of recent dictations the user can correct after the fact.
- **Speaker renames** don't feed the vocabulary. They stay per meeting (§6.3).
- **Settings > Vocabulary:** add, edit or delete entries by hand, for example before a meeting with a new client.
- **Limitation:** PushTalks can't see edits made after text is pasted into another app (Slack, Outlook, Notion). Those corrections have to be made in PushTalks to be learned.

**How remembering works:**
1. After an edit, PushTalks shows a small prompt, for example: *Always write "Stefan" as "Stephan"?*, with **Always**, **Just this once** and **Never ask for this word**.
2. **Always** saves the rule: what the model heard and the correct spelling. It's applied right away to the rest of the current transcript, with an undo.
3. Rules apply automatically to every new dictation and meeting, after transcription and before cleanup, insertion and export.

**Matching rules, to avoid wrong replacements:**
- Whole words or whole phrases only, never inside other words.
- Case-insensitive matching. The saved capitalisation is used, for example "PUSH Media", "TikTok", "M&M".
- Multi-word phrases are supported, in both directions: "tick tock" to "TikTok", and "Choice Hotels" kept together.
- **Common English words need extra care.** If the misheard form is a common word (for example "humble" for a client called "Humber"), PushTalks doesn't replace it everywhere by default. It asks for confirmation the first few times, or saves the rule as a hint only (below).
- Rules never touch the "Me" / "Speaker N" labels or the YAML header.

**Hints to the speech model (to verify):** sherpa-onnx offers hotword biasing for some model types. If it supports Parakeet's transducer, saved spellings are also passed as hotwords so the model hears them right in the first place. This must be verified in Phase 2; if it isn't supported, the post-transcription rules above still apply.

**Data and privacy:**
- The vocabulary is stored locally in SQLite. It's treated as confidential because it will contain client and people's names.
- It's never sent anywhere, never logged, and never committed to the repo.
- Users can view, edit and delete every entry in Settings > Vocabulary.
- **Export and import** to a file (P2) lets a team start from a shared list. Sharing a list that contains client names follows PUSH's client-data rules, and a team-wide default list needs leadership approval (§17).

**Acceptance criteria:**
- A word corrected with **Always** is spelled correctly in 100% of later occurrences of the same misheard form, across both dictation and meetings.
- No rule changes a word inside another word, and common-word rules never apply without confirmation.
- Every correction is reversible: undo in the transcript, and delete in Settings.
- On the Phase 0 human-checked clip, adding the clip's names and terms as rules reduces WER. Measure and record the gain in §18.

### 6.8 Minutes with Claude (built 2026-10-22, draft)

**Goal:** Claude writes minutes and a summary for each meeting and hands them back to the user, without PushTalks itself calling any AI service.

- PushTalks ships a local connector, `pushtalks-mcp`, an MCP server over stdin/stdout with no network code. The user adds it to Claude Code or Claude Desktop once; Settings > Use with Claude shows the command and config.
- **Tools:**
  - `list_meetings` (with optional search).
  - `get_transcript` (the Markdown file).
  - `save_minutes` (writes the minutes into the meeting's Notes section).
- **Prompt:** `meeting_minutes`, with these sections: Summary, Decisions, Action items, Open questions, Next steps. It follows the transcript-summary skill's format when that skill is available.
- **Flow:** the user asks Claude, for example "Write minutes for my latest PushTalks meeting." Claude reads that one transcript, writes the minutes and saves them.
- **Where the minutes show up:**
  - The Notes section of the `.md` file.
  - The `notes` field of the JSON sidecar.
  - PushTalks > Library, where the user can copy or edit them.
  - Library search.
- **Tagging:** saved minutes are tagged *[AI: DRAFT]* unless they already carry an AI intent tag.
- **Governance:** this is option A from Phase 3: the user chooses to send a transcript to Claude. Nothing runs automatically. It still sends transcript text to Claude, so using it on client meetings needs supervisor confirmation (§17).

## 7. Technical architecture

### 7.1 Stack

| Layer | Choice | Why |
|---|---|---|
| App shell | **Tauri 2** (Rust backend, TypeScript + React frontend) | App shell is roughly 10 MB vs 100+ MB for Electron. Native performance, small updates. |
| Speech recognition | **NVIDIA Parakeet TDT 0.6B v2, int8 ONNX** (English-only, default since 2026-10-21). v3 int8 (25 European languages) stays available as an optional model | v2 beat v3 on real PUSH audio in Phase 0 (§18): 17.0% vs 21.4% WER on a human-checked room clip, 3.5% vs 4.7% on a Zoom call, same speed and size. Word timestamps and punctuation. CC BY 4.0. |
| Inference runtime | **sherpa-onnx** (official `sherpa-onnx` Rust crate, 1.13.8) | Runs Parakeet, Silero VAD, and diarization models in one cross-platform runtime on CPU. Parakeet v2 and v3 int8 verified in Phase 0. |
| Voice activity detection | **Silero VAD** (ONNX, about 2 MB) | Skips silence and splits audio into segments under 30 seconds. |
| Diarization (P1) | sherpa-onnx speaker diarization (pyannote segmentation + speaker embedding model) | On-device, no Python dependency. Verify model licenses before shipping. |
| Audio capture | `cpal` crate for mic; **ScreenCaptureKit** (macOS 13+) and **WASAPI loopback** (Windows) for system audio | OS-level capture works with every meeting app. |
| Text insertion | Clipboard + simulated paste via `enigo` crate | Works in nearly every app. |
| Hotkeys | `tauri-plugin-global-shortcut` | Cross-platform global shortcuts. |
| Local data | SQLite (`rusqlite`) with FTS5 | Library index and search, plus the learned vocabulary table (§6.7). |
| Audio encoding | WAV during capture, optional Opus for retained audio | Opus cuts kept-audio size by roughly 10x. |

### 7.2 Pipeline

```
Dictation:
  Hotkey down -> mic capture (16 kHz mono) -> hotkey up
  -> Parakeet (single pass, audio under 30 s)
  -> cleanup rules + custom vocab -> clipboard paste -> restore clipboard

Meeting:
  Record -> mic.wav + system.wav (chunked to disk)
  -> Stop -> resample both to 16 kHz mono
  -> Silero VAD per track -> segments under 30 s
  -> Parakeet per segment (word timestamps)
  -> [P1] diarization on system track
  -> merge tracks by timestamp, label speakers
  -> write .md + .json, index in SQLite
  -> delete or compress audio per retention setting
```

Important implementation notes:
- **Segment length.** The Parakeet ONNX encoder handles roughly 35 seconds per pass. Never send longer audio. VAD segmentation with a 30 second hard cap solves this.
- **Model loading.** Load the model once and keep it warm while the app runs so dictation feels instant. Unload after 15 minutes idle to free RAM (configurable).
- **Threading.** All inference runs on a background thread pool, never on the UI thread. Meeting processing runs at below-normal OS priority.
- **Overlap handling.** When mic and system tracks both have speech at the same moment (crosstalk or echo), prefer the mic track for "Me" and de-duplicate near-identical text within a 2 second window.

### 7.3 Model management

- Models are **not** bundled in the installer. Downloaded on first launch from a pinned source.
- Every model file has a pinned revision and a SHA-256 checksum hardcoded in the app. Refuse to load on mismatch.
- Stored in the OS app data folder (`~/Library/Application Support/PushTalks/models/` on macOS, `%APPDATA%\PushTalks\models\` on Windows).
- One shared model serves both dictation and meeting mode.
- Optional later: host the model files on an internal PUSH location so the first download never touches the public internet.

### 7.4 Storage budget

| Component | Approx. size |
|---|---|
| App shell | 10 to 20 MB |
| Parakeet v2 int8 (encoder + decoder + joiner) | 661 MB (measured) |
| Silero VAD | about 2 MB |
| Diarization models (P1) | about 30 to 50 MB |
| **Total installed** | **under 800 MB target** |
| Retained audio (if kept, Opus) | about 30 MB per hour |
| Transcripts | under 1 MB per hour |

### 7.5 Suggested repo structure

```
pushtalks/
  CLAUDE.md                  # project rules for Claude Code (see Appendix A)
  PRD.md                     # this document
  README.md
  src-tauri/
    src/
      main.rs
      audio/
        mic.rs               # cpal mic capture
        system_mac.rs        # ScreenCaptureKit loopback
        system_win.rs        # WASAPI loopback
        resample.rs
      asr/
        engine.rs            # sherpa-onnx Parakeet wrapper
        vad.rs
        diarize.rs           # P1
      dictation/
        hotkey.rs
        insert.rs            # clipboard paste + restore
        cleanup.rs
      meeting/
        recorder.rs
        pipeline.rs
        merge.rs
      output/
        markdown.rs
        json.rs
      models/
        manifest.rs          # pinned URLs + SHA-256
        download.rs
      db/
        library.rs           # SQLite + FTS5
      vocab/
        rules.rs             # learned spellings: storage, matching, apply (§6.7)
        hotwords.rs          # optional model biasing, if supported
      settings.rs
    Cargo.toml
    tauri.conf.json
  src/                       # React frontend
    components/
    views/
      Dictation.tsx
      Meeting.tsx
      Library.tsx
      Settings.tsx
      Onboarding.tsx
    styles/
  tests/
    fixtures/                # short test clips, no client audio
    golden/                  # expected transcripts for regression tests
  scripts/
    benchmark.rs             # WER + speed benchmark harness
```

## 8. Non-functional requirements

| Area | Requirement (target, validate in Phase 0) |
|---|---|
| Dictation latency | Text inserted within 1.5 s of key release for a 15 s utterance on an Apple M1 or a recent Intel i5 / Ryzen 5 laptop |
| Meeting processing | 60 min meeting processed in under 10 min on the same hardware |
| Accuracy | Word error rate under 10% on PUSH test recordings (English) |
| Memory | Under 2 GB RAM during processing; under 300 MB idle with model unloaded |
| Disk | Under 800 MB installed |
| Battery | No measurable drain when idle |
| Offline | Fully functional with no network after model download |
| Platforms | macOS 13+ (Apple Silicon and Intel), Windows 10 22H2+ and Windows 11 |
| Crash safety | A crash during recording loses at most 10 seconds of audio |

## 9. UI and design

- Follows PUSH brand: black `#111111` background, white text, purple `#8A66FF` as the primary accent. Work Sans for UI text.
- Main window: three tabs (Meeting, Library, Settings). Dictation lives in the tray/menu bar.
- Recording state is always visible: a purple pulsing dot in the menu bar plus a timer.
- Onboarding flow (first launch):
  1. Welcome and what stays local.
  2. Grant microphone permission.
  3. Grant accessibility permission (needed for text insertion, macOS).
  4. Grant screen/system audio recording permission (needed for meeting mode, macOS).
  5. Download model with progress and size shown.
  6. Try dictation with a sample sentence.

## 10. Privacy, security, and governance

- **No network calls** after the model download. Add a test that fails the build if any new network client is introduced outside `models/download.rs`.
- No telemetry or analytics in v1.
- Audio deleted after transcription by default.
- Transcripts stored only where the user chooses. Header marks them as internal and potentially containing confidential client information.
- **Recording consent.** Before each meeting recording, the app shows a reminder that all participants must be informed. The user must confirm. Exact wording and policy to be set with leadership and, if needed, legal.
- Transcripts are internal records. Anything shared outside PUSH follows existing client-data rules.
- **Approval required before rollout:** new internal AI tool approval, recording consent policy, and data retention defaults go to Tala's supervisor and senior leadership. This PRD does not make those decisions.

## 11. Licensing and attribution

| Component | License | Obligation |
|---|---|---|
| Parakeet TDT 0.6B v2 (default) and v3 (optional) | CC BY 4.0 | Attribute NVIDIA (and ONNX export author) in About screen and README |
| sherpa-onnx | Apache 2.0 | Include license notice |
| Silero VAD | MIT | Include license notice |
| Diarization models: pyannote segmentation 3.0 + NVIDIA TitaNet small | MIT (CNRS) and CC BY 4.0 (NVIDIA), checked 2026-10-22 | Attribute CNRS and NVIDIA in THIRD_PARTY_NOTICES.md (done) |
| Tauri | MIT / Apache 2.0 | Include license notice |
| Whisper (Phase 4 fallback) | MIT | Include license notice |

Ship a `THIRD_PARTY_NOTICES.md` and show it in Settings > About.

## 12. Distribution

- Build installers with Tauri: `.dmg` (macOS) and `.msi` (Windows).
- Unsigned apps trigger OS security warnings. Options to decide with IT:
  - Apple Developer account for signing and notarization (annual fee, roughly US$99) plus a Windows code-signing certificate.
  - Or deploy through PUSH's device management, which can approve the app centrally.
- Auto-update via Tauri updater pointing at an internal location (P2).

## 13. Phased plan

### Phase 0: Validation spike (1 week)
- Build a CLI in Rust that transcribes a WAV file with Parakeet int8 via sherpa-onnx (v3 first; v2 chosen as the default after benchmarking, §18).
- Benchmark on 5 to 10 real internal recordings (English, with permission from participants): accuracy, speed, RAM, on both a Mac and a Windows laptop.
- Compare int8 vs fp16 accuracy. Confirm int8 quality is acceptable.
- **Exit criteria:** NFR targets in Section 8 met or revised with data.

### Phase 1: Dictation MVP (2 weeks)
- Tauri shell, tray icon, onboarding, model download with checksums.
- Push-to-talk, transcription, cleanup, clipboard insertion.
- Settings: hotkey, mic, model status.
- **Exit criteria:** 5 internal testers use dictation daily for one week.

### Phase 2: Meeting mode (3 weeks)
- Dual-track recording (mic + system audio) on both OSes.
- VAD segmentation, batch transcription, track-based speaker labels.
- Unified Markdown + JSON output.
- Consent prompt, retention setting, crash-safe chunked recording.
- Transcript editor with learned vocabulary (§6.7): **Always / Just this once** prompt, applying rules to new transcripts, and Settings > Vocabulary. Pulled forward from Phase 3 because names and terms are a measured accuracy gap (§18).
- **Exit criteria:** 10 real internal meetings processed with acceptable transcripts. A word corrected once is spelled right in every later meeting.

### Phase 3: Quality and polish (2 to 3 weeks)
- On-device diarization for multiple remote speakers.
- Speaker renaming, Library with search.
- Learned vocabulary for dictation history, hotword biasing if supported, and vocabulary import/export (§6.7).
- **Notes section:** decide approach (see Open Decisions). Options:
  - A. No in-app summary. User sends the transcript to Claude with the transcript-summary skill. Zero added storage. **Built (draft, 2026-10-22) as a local Claude connector, §6.8.** Using it on client meetings still needs supervisor confirmation.
  - B. Small local LLM (3B class, 4-bit) via llama.cpp. Adds roughly 2 GB storage and noticeable processing time.
  - C. Approved Claude Enterprise API from inside the app. Best quality, but transcript text leaves the device and needs governance approval.
- Signed installers, internal pilot with 15 to 20 users.

### Phase 4: Optional extensions
- Whisper Large v3 Turbo (via whisper.cpp) fallback for languages Parakeet does not cover.
- macOS CoreML build of Parakeet for Neural Engine speed.
- Chrome extension for browser-only meetings (Google Meet, Teams web) using the same model via ONNX Runtime Web. Note: an extension cannot capture desktop meeting apps or type into other programs.

## 14. Testing

- **Unit tests:** cleanup rules, segment merging, timestamp formatting, Markdown/JSON writers, checksum verification.
- **Learned vocabulary tests:** whole-word and phrase matching, capitalisation, no replacements inside words, common-word confirmation, undo, and the `original_text` field.
- **Golden-file regression tests:** short public-domain or self-recorded clips in `tests/fixtures/` with expected transcripts. Fail if WER regresses more than 1 point.
- **Benchmark harness:** `scripts/benchmark` reports WER, real-time factor, and peak RAM per model build.
- **Manual QA matrix:** macOS Apple Silicon, macOS Intel, Windows 10, Windows 11 across Teams desktop, Zoom, Google Meet in Chrome, Slack, Outlook, Notion, and a browser text field.
- **Never commit client meeting audio or transcripts to the repo.** Add these paths to `.gitignore` and a pre-commit check.

## 15. Success metrics (pilot)

- Weekly active users among pilot group.
- Meetings processed per week.
- Median processing time per meeting hour.
- User-reported transcript accuracy (1 to 5 rating after each meeting).
- Number of manual edits per transcript (proxy for accuracy).
- Zero data incidents.

## 16. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Accuracy drops with accents, crosstalk, or poor mics | Phase 0 benchmarks on real recordings; custom vocabulary; allow model swap |
| macOS permission friction (accessibility, screen recording) | Clear onboarding with screenshots; deep links to the right Settings pane |
| System audio capture breaks with OS updates | Isolate per-OS capture code; test on OS betas |
| Older or low-RAM laptops too slow | Minimum spec documented; dictation-only mode for low-spec machines |
| Recording without participant consent | Mandatory consent prompt; policy set by leadership |
| Model license changes or download source disappears | Pinned revisions, checksums, mirror files internally |
| Scope creep into a full meeting platform | Non-goals in Section 4 are binding for v1 |

## 17. Open decisions

| Decision | Owner | Needed by |
|---|---|---|
| Approval to build and pilot PushTalks | Supervisor / senior leadership | Before Phase 1 |
| Recording consent wording and policy | Senior leadership, legal if needed | Before Phase 2 |
| Default audio retention period | Senior leadership | Before Phase 2 |
| Notes/summary approach (A, B, or C above) | Tala + senior leadership | Before Phase 3 |
| Signing and distribution route | Tala + IT | Before pilot |
| Whether teams may share a vocabulary list (it will contain client names) | Supervisor / senior leadership | Before vocabulary export ships (P2) |
| Internal model hosting location | Tala + IT | Before pilot |
| Whether voice memory (stored voiceprints, biometric data) may be used, and the notice or consent it needs | Supervisor / senior leadership, legal | Before anyone turns it on |

---

## Appendix A: Starter CLAUDE.md for the repo

The repo's `CLAUDE.md` is based on this appendix, with PUSH governance rules added.

## Appendix B: First prompts to give Claude Code

1. "Read PRD.md and CLAUDE.md. Scaffold the Tauri 2 project with the repo structure in Section 7.5. No features yet."
2. "Implement Phase 0: a Rust CLI binary that loads Parakeet v3 int8 via sherpa-onnx and transcribes a 16 kHz mono WAV, using Silero VAD to split into segments under 30 seconds. Print segments with timestamps."
3. "Add the benchmark binary: given a WAV and a reference .txt, report WER, real-time factor, and peak RAM."
4. "Implement models/manifest.rs and models/download.rs with pinned URLs, SHA-256 checks, resumable downloads, and progress events to the frontend."
5. "Implement Phase 1 dictation: global push-to-talk hotkey, mic capture, transcription, cleanup rules, clipboard paste with restore."

---

## 18. Phase 0 results (2026-10-20) [AI: DRAFT]

**Status: speed, memory and disk pass; accuracy does not yet meet the target.** On a human-checked transcript of a real internal meeting, Parakeet's WER is 21.4%, or 17.5% ignoring fillers and stutters, against the < 10% target. See "Human-checked accuracy" below.

**Setup:** Apple M4, 16 GB, macOS 26.6. Parakeet TDT 0.6B v3 int8 and Silero VAD run through the official `sherpa-onnx` Rust crate 1.13.8, with 8 threads. All model files are pinned by SHA-256 and checked before loading.

| NFR (§8) | Target | Measured | Result |
|---|---|---|---|
| Dictation latency | ≤ 1.5 s for a 15 s utterance | 0.97 s to transcribe a 13.8 s utterance with the model warm. Paste time not included (Phase 1) | Pass (transcription only) |
| Meeting processing | 60 min in < 10 min | 10.2 min of audio in 41.5 s (RTF 0.068, about 15× real time). Projects to about 4 min per hour | Pass (projected) |
| Accuracy (English) | WER < 10% on PUSH recordings | 0 to 12.5% per clip on 4 synthetic English clips | Not yet testable |
| Accuracy (French) | Out of scope for v1 (2026-10-21) | 0 to 26.7% per clip on 3 synthetic French voices, kept for reference | n/a |
| Memory | < 2 GB while processing | Peak 1.1 GB (10-minute run); 0.8 GB for short clips | Pass |
| Disk | < 800 MB installed | Models 671 MB + CLI 29 MB. Tauri shell not built yet | On track |
| Offline | No network after model download | Only `models/download.rs` uses the network; a test fails the build otherwise. Not yet run with Wi-Fi off | Pass (by code check) |
| Encoder cap | ≤ 30 s per pass | Longest segment 20.3 s (synthetic), 25.0 s (real meeting) | Pass |

**First real recording (2026-10-21):** one internal Zoom meeting. It ran 36.3 min, with 3 speakers and about 5,000 words, mostly English, recorded on Zoom's audio track.

| Measure | Result |
|---|---|
| Agreement with Zoom's own transcript | 95.3% of words match (4.7% differ). Zoom's transcript is also machine-generated, so this is not a true WER |
| Processing time | 187 to 209 s for 36.3 min (RTF 0.086 to 0.096, 10 to 12× real time), about 5 to 6 min per hour (target < 10 min) |
| Peak RAM | 1.36 to 1.58 GB (target < 2 GB) |
| Segments | 130, longest 25.0 s |

The meeting's `.mp4` and `.m4a` files gave identical transcripts, so they count as one recording. Audio and transcript copies made for the test were deleted afterwards. Nothing was committed.

**Four more real meetings (2026-10-21):** 115 min of internal meetings, two of them client video-feedback sessions. Each was recorded with Pocket and compared against Pocket's own transcript, which is also machine-generated. Pocket's MP3 export was the audio given to PushTalks.

| Meeting | Length | Words differing from Pocket | Words found (PushTalks / Pocket) | Processing | Peak RAM |
|---|---|---|---|---|---|
| Process review | 58.2 min | 28.2% | 83% | 413 s | 1.40 GB |
| Internal regroup | 40.7 min | 20.5% | 94% | 244 s | 1.31 GB |
| Client feedback A | 9.5 min | 14.3% | 106% | 54 s | 1.29 GB |
| Client feedback B | 6.9 min | 19.9% | 99% | 60 s | 1.19 GB |
| **All** | **115 min** | | | **12.9 min (6.7 min per meeting hour)** | **≤ 1.40 GB** |

**What the gaps mean:**
- **Speed and memory pass on every meeting**, including the 58-minute one (target < 10 min per hour, < 2 GB).
- **The short meetings found the same amount of speech** as Pocket (99 to 106% of its words). Their differences are wording, and either transcript could be the wrong one.
- **The 58-minute meeting is missing about 17% of Pocket's words.** The gaps cluster in particular stretches and are ordinary words, not filler.
- **Segmentation is not the cause.** On the worst 6-minute stretch, every setting gave 580 to 610 words: VAD thresholds from 0.2 to 0.5, segment caps from 5 to 25 s, and no VAD at all (fixed 8 s and 15 s windows).
- **A second engine heard even less.** whisper base.en, run as an independent cross-check, gave 358 words on that stretch and flagged 21 non-speech moments. 98% of Parakeet's words there appear in Pocket's transcript, so what it does output is real English.
- **Likely cause (unconfirmed):** Pocket may transcribe from higher-quality audio than its MP3 export, or use a model that recovers more overlapping speech. Pocket's processing isn't documented here, so this needs checking. Phase 2 records mic and system audio as separate tracks (§6.2), which should help with calls.

The benchmark now takes `--vad-threshold`, `--min-silence`, `--merge-max`, `--max-speech` and `--fixed-windows` flags for this kind of check. The defaults stay as validated. All temporary audio and transcript copies were deleted, and the recordings folder in the repo is git-ignored.

**Human-checked accuracy (2026-10-21):** a 7.2-minute clip from the 58-minute process review, transcribed by hand. It's a far-field room recording with several speakers and fast back-and-forth.

| Engine | WER, all words | WER, fillers and stutters ignored | Notes |
|---|---|---|---|
| **Parakeet v3 int8 (greedy)** | **21.4%** | **17.5%** | 1,102 words vs 1,092 in the reference, so nothing missing. 72 substitutions, 76 deletions, 86 insertions. 30 s to process (RTF 0.069), 1.33 GB peak |
| Parakeet v3 int8 (beam search) | 21.5% | n/a | No gain, 20% slower. Greedy stays the default |
| whisper base.en (prototype engine, for comparison) | 22.8% | 20.9% | |

**Where the errors are:**
- **Not the clip edges.** Excluding the start and end of the clip, WER is still 20.1%.
- **Mostly real misrecognitions in conversational speech.** They cluster in the opening and in the closing stretch of quick multi-speaker back-and-forth.
- **About 1 in 6 errors involve names or terms:** people, client names and platform abbreviations.
- **About 1 in 4 are filler or repeated short words**, which matter little for notes.

**Result:** the < 10% target is not met on this audio. The PRD's §16 risk ("accuracy drops with accents, crosstalk, or poor mics") applies.

**Parakeet v2 vs v3 (2026-10-21).** v2 is NVIDIA's English-only Parakeet TDT 0.6B v2: int8, CC BY 4.0, free, pinned by SHA-256 like v3. It's installed with `pushtalks-cli fetch-models --model v2` and selected with `--model v2`.

| Test | v3 int8 (previous default) | v2 int8 (English-only, **now the default**) |
|---|---|---|
| Human-checked room clip (7.2 min), WER | 21.4% | **17.0%** |
| Same, fillers and stutters ignored | 17.5% | **15.0%** |
| 36-min Zoom call, words differing from Zoom's transcript | 4.7% | **3.5%** |
| Processing (room clip / Zoom call) | 29.8 s / 148 s | 29.2 s / 146 s |
| Peak RAM (room clip / Zoom call) | 1.38 GB / 1.81 GB | 1.56 GB / 1.37 GB |

**Decision (2026-10-21, Tala): Parakeet v2 int8 is the default.** v3 stays available with `--model v3`.

**Summary:** v2 is more accurate on both tests at the same speed: about a fifth fewer errors on the room clip and a quarter fewer differences on the call. Disk size is about the same (661 MB vs 670 MB). v2 is English-only, which fits now that French is out of scope.

**Still above target:** the room clip remains above the < 10% target on either model. v3's peak RAM on the long call (1.81 GB) is close to the 2 GB limit; Phase 2 streaming (below) is still required.

**Options to close the gap** (to decide; see Open decisions):
1. ~~Switch the default to Parakeet v2~~ **Done (2026-10-21).** An fp16 build of v2 exists (1.1 GB) but has no published checksum; it's optional and untested.
2. **Learned vocabulary** (§6.7, added 2026-10-21): correct a name once and it's remembered. Hotword biasing is to be verified.
3. **Better audio:** per-track capture for calls (§6.2), and a headset or a mic closer to the speakers in rooms.
4. **Revise the target** for far-field multi-speaker rooms (for example, < 10% for calls and dictation, a separate target for rooms), with leadership's agreement.

**Memory grows with meeting length.** The Phase 0 CLI loads the whole file and gives the VAD a buffer as long as the audio. A 60 to 90 minute meeting could pass 2 GB. Phase 2 must stream the VAD and ASR in chunks.

**What was fixed during Phase 0:**
- The first VAD settings cut at short pauses and clipped words at segment edges. Mixed-language WER was 15.3%.
- Padding segments by 0.25 s and merging speech separated by under 1 s (up to 25 s per segment) brought it to 8.6%.
- Number formatting is not counted as an error. Parakeet writes amounts and percentages as figures (for example "$1,000" and "10%"), which is the style PUSH would want.

**Still to do before the Phase 0 exit:**
- [x] Five real internal meetings benchmarked, about 151 min in total (see above).
- [x] One human-checked transcript: 7.2 min, WER 21.4% (17.5% without fillers). Target not met.
- [ ] **Decide how to close the accuracy gap** (options 1 to 4 above), and re-test on more human-checked clips, including a call recorded as separate tracks and a dictation sample.
- [ ] **Windows laptop** and an Intel Mac benchmark run.
- [x] **Model choice:** Parakeet v2 int8 is the default (2026-10-21). fp16 was not adopted: v2 int8 already met the speed, memory and disk targets, and the v2 fp16 file has no published checksum to pin.
- [ ] Dictation latency including paste (Phase 1).

**Other notes:**
- **Model size:** the installed Parakeet files are 670 MB (encoder 652 MB), within the §7.4 estimate.
- **Licensing:** the sherpa-onnx static build bundles text-to-speech libraries. GPL-licensed espeak-ng is not linked into PushTalks; MIT-licensed piper-phonemize partly is. Both are listed in `THIRD_PARTY_NOTICES.md`.

## 19. App build status (2026-10-22) [AI: DRAFT]

The full desktop app (Phases 1 to 3 scope) is built as a **draft for review**. Nothing here assumes approvals from §17: there's no pilot rollout, no cloud AI (notes use option A), the consent wording is a marked placeholder, and audio is deleted after transcription by default.

**Built:**
- **Shell:** Tauri 2 app in the menu bar / tray (Open, Start/Stop meeting with a live timer, Quit). Closing the window keeps it running. Single instance. Launch at login (off by default).
- **Onboarding (§9):** welcome, mic test, Accessibility, Input Monitoring and Screen Recording on macOS, model download with progress and SHA-256 checks, try dictation.
- **Dictation (§6.1):**
  - Hold Right Option (macOS) / Right Alt (Windows), or a toggle shortcut.
  - Pressing another key while holding cancels, so Option/Alt shortcuts still work.
  - Max length setting, filler cleanup, learned spellings, clipboard paste with restore (text and images).
  - Floating indicator that isn't focusable.
  - The last 20 dictations are kept locally for correction.
- **Meetings (§6.2, §6.3):**
  - Consent dialog before every recording; the start command refuses without it.
  - Mic + system audio (ScreenCaptureKit / WASAPI loopback) to 16 kHz WAV per track, flushed every 2 s.
  - Unfinished recordings can be transcribed or discarded after a crash.
  - In-room option: mic only.
  - Processing queue in the background with progress and ETA, at background thread priority.
  - Echo de-duplication, on-device diarization ("Speaker 1, 2…"), speaker rename.
- **Output (§6.4):** Markdown + JSON exactly as specified, including `original_text`.
- **Library (§6.5):** list, FTS5 search with highlighted snippets, open, rename, open file, show in folder, delete.
- **Learned vocabulary (§6.7):**
  - Transcript and dictation-history editors detect the corrected word and offer Always / Just this once / Never ask.
  - Always applies to the rest of the transcript, with undo.
  - Single common English words become "ask first" rules, shown as per-line suggestions.
  - Settings > Vocabulary to view, add and delete.
- **Settings (§6.6):** dictation mode, mic, output folder, retention (delete / 7 / 30 days / forever, with automatic purge), speech model (v2/v3) and checksum status, launch at login, About with third-party notices.

**Measured (synthetic test only):**
- A two-track, 7-minute meeting made with macOS text-to-speech voices processed in 7.8 s without diarization and 24.2 s with it, on an Apple M4.
- Diarization split the two remote voices correctly. Real-meeting accuracy of diarization is **not yet measured**.

**Not verified yet (needs a person at the keyboard or a Windows machine):**
- Real dictation end to end: hold key, paste into Slack/Outlook/Notion, clipboard restore, and that the indicator never steals focus.
- A real Teams/Zoom/Meet call recorded through ScreenCaptureKit.
- Everything on Windows: the code compiles only on macOS so far. The Windows hotkey, loopback and paste code is written but untested.
- macOS permission flows on a fresh machine. The dev build is unsigned, so each rebuild may need permissions granted again.

**Windows build (2026-10-27):**
- GitHub Actions (private repo) builds PushTalks on Windows and produces an `.exe` installer (per-user, no admin rights) and an `.msi` installer.
- The first real Windows compile passed: all Rust tests (including the no-network guard), clippy and the frontend tests.
- **Not yet run on a Windows PC.** The hold-to-talk key, system audio (WASAPI loopback), paste, call detection and permissions are untested.

**Processing speed fix (2026-10-27, v0.1.1):**
- **Problem:** a tester reported that a 45-minute meeting took hours to process on Windows.
- **Cause:** processing ran at lowered priority. On macOS that is "background" priority, which can move work onto the slow efficiency cores. The same 29-minute recording took 268 s at normal priority, and was still unfinished after 600 s at background priority. Nothing stopped Windows efficiency mode, macOS App Nap, or the computer sleeping while the window was hidden.
- **Fix:** while a meeting processes, PushTalks:
  - runs at normal priority;
  - opts out of Windows efficiency mode (EcoQoS) and macOS App Nap;
  - keeps the computer from idle sleep.
- **Threads:** one inference thread per physical core, up to 8. Intel/AMD hyper-threads are no longer counted twice.
- **Measured (Mac, 10 cores):** 29.4 min of meeting with two tracks and diarization took 268 s (RTF 0.15). Diarization was 101 s of that.
- **Windows baseline (old build, 0.1.0):** about 12 hours for a 45-minute meeting on a 2022 Lenovo V14 G3: i7-1255U with 2 performance cores and 8 efficiency cores, 16 GB RAM, Windows 11 Pro. That is RTF about 16, roughly 100 times slower than the Mac.
- **Not yet measured on Windows with v0.1.1.** Each processed meeting now appends one line of stage durations (no content) to `processing-times.log` in the app data folder (`%APPDATA%\PushTalks` on Windows). That line is the number to record here.
- **Version:** builds now carry their own version number (0.1.1), so Windows installs them as updates.

**Speaker count fix (2026-10-28):**
- **Problem:** a tester's call with about 8 people came out with 57 speakers.
- **Cause:** the clustering step splits one person into many fragments. On a kept 29-minute call it found 109 clusters, of which 3 spoke for 30 s or more. Raising the clustering threshold alone wasn't enough: 0.9 still gave 31.
- **Fix:** clusters with 20 s or more of speech count as main speakers. Each smaller fragment:
  - joins the main speaker whose voiceprint it matches best (cosine 0.35 or more);
  - otherwise keeps its own label if it has 8 s or more of speech (someone who spoke briefly);
  - otherwise joins the nearest main speaker in time.
- **Measured:** the same call now gives 6 speakers. The true number of people on that call is not yet confirmed. Folding adds under 1 s to processing.
- **Not verified:** the thresholds were tuned on one call, so check them against more meetings with known attendee counts.

**Automatic minutes prompt (2026-10-28):**
- Settings > Use with Claude now offers a copyable prompt for Claude Code. It starts with `/loop 30m`, so every 30 minutes Claude checks for meetings without minutes and writes them.
- The check is cheap: `list_meetings` takes `only_without_minutes`, and no transcript is read when nothing is new.
- **Fuller minutes format** (also used by the one-off `meeting_minutes` prompt): Summary, Attendees, Discussion by topic, Decisions, Action items table, Risks and blockers, Open questions, Next steps, Needs checking.
- **Limits:** it runs only while that Claude Code session stays open. Sending transcripts to Claude is cloud AI use and needs the supervisor's approval for client meetings (§17).

**Automatic meeting titles (2026-10-23):**
- **At recording start (and again at Stop, if still unnamed):** PushTalks takes the title from the meeting window or, failing that, the Mac calendar event happening now. A typed title always wins.
  - Teams meeting windows are used; Teams navigation windows ("Chat | …", "Calendar | …") are ignored.
  - Google Meet tab names are used; bare room codes are ignored.
  - Calendar access is optional and asked for in Settings > Permissions.
  - Everything stays local.
- **Claude renames generic meetings:** when Claude writes minutes, it may pass a short descriptive title. It replaces only a generic title ("Meeting 2026-10-23 10:00", "Microsoft Teams call"), never one the user or the meeting app gave.
- **Action items in the minutes** use a table: | Action Item | Owner | Suggested Deadline |. Owner falls back to a role or team, then "Unassigned"; the deadline is explicit or inferred, otherwise "TBD".
- **Verified:** the title rules are unit-tested, and Teams' chat window was correctly ignored live. A live Teams meeting window and the calendar lookup are not yet verified.

**Minutes with Claude (§6.8), added 2026-10-22:**
- Tested end to end with Claude Code (2.1.299) as the client, on a synthetic two-voice meeting. Claude listed the meeting, read it, and saved minutes with a summary, decisions, an owned action item and open questions. It flagged the "10%" figure and the inferred due date for checking.
- Not yet tested with Claude Desktop or on a real meeting.

**Call detection and fixes (2026-10-22, later):**
- **"On a call?" prompt:** when another app has used the mic for about 6 seconds, PushTalks shows a small prompt naming the likely meeting app.
  - "Record…" only opens the consent dialog; nothing records without confirmation (§10).
  - "Not now" silences it until that call ends.
  - On by default; Settings > Calls turns it off.
  - macOS uses the CoreAudio "device running somewhere" flag; Windows uses the microphone privacy consent store (untested).
  - Verified on macOS that the flag reads correctly while idle. A live Teams call hasn't been tested yet.
- **Bug fixed:** macOS didn't recognize the Accessibility grant, because the bundle wasn't signed with its app ID. It's now ad-hoc signed as `agency.push.pushtalks`. Ad-hoc grants may reset with each new build until the app is signed with a PUSH Apple Developer ID (§12).
- **Bug fixed:** the signed app got silent audio from the mic. The hardened runtime needs the `com.apple.security.device.audio-input` entitlement, which is now added (`src-tauri/Entitlements.plist`).
  - The mic test now listens for up to 8 s with a live meter.
  - It tells "blocked by macOS" (all-zero samples) apart from "too quiet".
  - It lets the user pick another mic.
  - Capture verified from the terminal on EarPods: real audio, not blocked.
- **Bug fixed:** permission screens kept showing "not allowed" after the user enabled them, because macOS caches the answer inside a running app. The app now asks a short-lived helper copy of itself.
- **Bug fixed:** the window froze when the folder picker opened, because commands ran on the main thread. Every command now runs on a background thread, enforced by a test. PushTalks also now shows its window when opened, and stays hidden only when started at login.
- **Regular Mac app (2026-10-22):** PushTalks now has a Dock icon and shows in Cmd-Tab and Launchpad. Closing the window keeps it running; Cmd-Q or the menu bar quits it. Verified that the dictation capsule appearing doesn't take focus from the front app.
- **Bug fixed:** startup hashed about 700 MB of model files on the main thread before showing the window. Startup now only checks the files are present at their pinned sizes. The full SHA-256 check still runs before any model is loaded (§7.3).
- **Known limitation:** with ad-hoc signing, macOS treats every new build as a different app, so testers must re-allow permissions after each update. A PUSH Apple Developer ID certificate (§12) fixes this and is an IT/leadership decision.
- **Workaround on the build Mac:** `npm run bundle:mac` re-signs the app with a local self-signed certificate (`scripts/sign-local.sh`, using rcodesign and `~/.pushtalks-signing/`). macOS then ties permissions to the app ID and that certificate, so they survive rebuilds. It's not for distribution.
- **Onboarding no longer blocks.** Permission steps offer "Continue anyway" and a "Restart PushTalks" button.

**Decisions made while building (review these):**
- A **Dictation tab** was added next to Meeting, Library and Settings (§9 said three tabs). It holds the hold-key status and the recent-dictations list that §6.7 needs.
- **Diarization models:** pyannote segmentation 3.0 + NVIDIA TitaNet small. They're installed with the other models: about 46 MB more, total about 710 MB, still under the 800 MB budget.
- **Retained audio stays WAV,** about 230 MB per hour for both tracks. Opus compression (§7.1) is not done.
- **Hotword biasing (§6.7) is not done.** It's still unverified whether sherpa-onnx supports it for Parakeet's transducer. Vocabulary export/import (P2) is also not done.
- **Processing starts on Stop, not during the meeting.** It reads audio in 5-minute windows, so memory stays flat. The system track is loaded whole only for diarization, about 230 MB per hour.
- The app stores its preferences in `preferences.json`, because the archived Electron prototype uses `settings.json` in the same folder.
