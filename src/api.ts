// Typed wrappers around the Rust commands in src-tauri/src/app/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Retention = { mode: 'delete' } | { mode: 'keep_days'; days: number } | { mode: 'forever' };
export type RuleMode = 'auto' | 'confirm';

export interface Settings {
  onboarded: boolean;
  dictation_mode: 'hold' | 'toggle';
  hold_key: string;
  toggle_shortcut: string;
  max_dictation_secs: number;
  remove_fillers: boolean;
  mic: string | null;
  output_dir: string;
  retention: Retention;
  asr_model: string;
  unload_after_mins: number;
  launch_at_login: boolean;
  suggest_recording: boolean;
  remember_voices: boolean;
}

export interface Word { w: string; s: number; e: number }
export interface Segment {
  start: number;
  end: number;
  speaker: string;
  text: string;
  words: Word[];
  original_text?: string;
}
export interface MeetingRow {
  id: number;
  title: string;
  started_at: string;
  duration_secs: number;
  md_path: string;
  audio_dir: string | null;
}
export interface SearchHit { meeting: MeetingRow; snippet: string }
export interface Suggestion { segment: number; rule_id: number; heard: string; spelling: string }
export interface MeetingOut {
  row: MeetingRow;
  segments: Segment[];
  asr_model: string;
  speakers: string[];
  notes: string | null;
  suggestions: Suggestion[];
}
export interface Rule { id: number; heard: string; spelling: string; mode: RuleMode }
export interface Correction { heard: string; spelling: string; common: boolean }
export interface Dictation { id: number; created_at: string; text: string }
export interface ModelInfo {
  id: string;
  label: string;
  license: string;
  size: number;
  download_size: number;
  installed: boolean;
  required: boolean;
}
export interface Permissions {
  accessibility: boolean;
  input_monitoring: boolean;
  screen_recording: boolean;
  calendar: boolean;
  hold_key_active: boolean;
}
export interface Unfinished { dir: string; title: string; started_at: string; status: string }
export interface Tick {
  recording: boolean;
  elapsed_secs: number;
  mic_level: number;
  system_level: number;
  title: string;
  mic_only: boolean;
}
export interface Progress { id: string; title: string; fraction: number; eta_secs: number | null }
export interface Connector { path: string; installed: boolean; claude_code: string; claude_desktop: string }
export interface MicResult { device: string; peak: number; speech: boolean; blocked: boolean }
export interface Voice { id: number; name: string; samples: number }
export interface AppInfo { version: string; os: string; notices: string; models_dir: string }

export const api = {
  appInfo: () => invoke<AppInfo>('app_info'),
  settingsGet: () => invoke<Settings>('settings_get'),
  settingsSave: (settings: Settings) => invoke<Settings>('settings_save', { settings }),
  chooseFolder: () => invoke<string | null>('choose_folder'),
  permissions: () => invoke<Permissions>('permissions_status'),
  permissionRequest: (kind: string) => invoke<void>('permission_request', { kind }),
  permissionOpenSettings: (kind: string) => invoke<void>('permission_open_settings', { kind }),
  holdKeyRetry: () => invoke<boolean>('hold_key_retry'),
  mics: () => invoke<string[]>('mics_list'),
  micCheck: () => invoke<MicResult>('mic_check'),
  models: () => invoke<ModelInfo[]>('models_status'),
  modelsDownload: () => invoke<void>('models_download'),
  meetingStart: (title: string | null, consentConfirmed: boolean, micOnly: boolean) =>
    invoke<void>('meeting_start', { title, consentConfirmed, micOnly }),
  meetingStop: () => invoke<void>('meeting_stop'),
  meetingRecording: () => invoke<boolean>('meeting_recording'),
  unfinished: () => invoke<Unfinished[]>('recordings_unfinished'),
  recordingProcess: (dir: string) => invoke<void>('recording_process', { dir }),
  recordingDiscard: (dir: string) => invoke<void>('recording_discard', { dir }),
  processingStatus: () => invoke<{ current: string | null; queued: number }>('processing_status'),
  libraryList: () => invoke<MeetingRow[]>('library_list'),
  librarySearch: (query: string) => invoke<SearchHit[]>('library_search', { query }),
  meetingGet: (id: number) => invoke<MeetingOut>('meeting_get', { id }),
  meetingUpdate: (id: number, title: string, segments: { text: string; speaker: string }[]) =>
    invoke<MeetingOut>('meeting_update', { id, title, segments }),
  meetingRenameSpeaker: (id: number, from: string, to: string) =>
    invoke<{ meeting: MeetingOut; learned_voice: boolean }>('meeting_rename_speaker', { id, from, to }),
  meetingApplyRule: (id: number, ruleId: number, segment: number | null) =>
    invoke<{ meeting: MeetingOut; changed: number }>('meeting_apply_rule', { id, ruleId, segment }),
  meetingRestore: (id: number, segments: Segment[]) => invoke<MeetingOut>('meeting_restore', { id, segments }),
  meetingSetNotes: (id: number, notes: string) => invoke<MeetingOut>('meeting_set_notes', { id, notes }),
  appRestart: () => invoke<void>('app_restart'),
  callPromptState: () => invoke<string | null>('call_prompt_state'),
  callPromptRecord: () => invoke<void>('call_prompt_record'),
  callPromptDismiss: () => invoke<void>('call_prompt_dismiss'),
  voices: () => invoke<Voice[]>('voices_list'),
  voiceDelete: (id: number) => invoke<void>('voice_delete', { id }),
  voicesForgetAll: () => invoke<void>('voices_forget_all'),
  claudeConnector: () => invoke<Connector>('claude_connector'),
  meetingDelete: (id: number, deleteFiles: boolean) => invoke<void>('meeting_delete', { id, deleteFiles }),
  meetingReveal: (id: number) => invoke<void>('meeting_reveal', { id }),
  meetingOpen: (id: number) => invoke<void>('meeting_open', { id }),
  vocabList: () => invoke<Rule[]>('vocab_list'),
  vocabAdd: (heard: string, spelling: string, mode?: RuleMode) =>
    invoke<Rule>('vocab_add', { heard, spelling, mode: mode ?? null }),
  vocabDelete: (id: number) => invoke<void>('vocab_delete', { id }),
  vocabNeverAsk: (heard: string) => invoke<void>('vocab_never_ask', { heard }),
  vocabCorrection: (old: string, next: string) => invoke<Correction | null>('vocab_correction', { old, new: next }),
  dictations: () => invoke<Dictation[]>('dictations_list'),
  dictationUpdate: (id: number, text: string) => invoke<void>('dictation_update', { id, text }),
  dictationsClear: () => invoke<void>('dictations_clear'),
};

export function on<T>(event: string, handler: (payload: T) => void): () => void {
  let unlisten: UnlistenFn | null = null;
  let cancelled = false;
  listen<T>(event, (e) => handler(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

export const isMac = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac');
export const holdKeyName = isMac ? 'Right Option (⌥)' : 'Right Alt';

export function clock(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const pad = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(s % 60)}` : `${pad(m)}:${pad(s % 60)}`;
}

export function mb(bytes: number): string {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

export function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}
