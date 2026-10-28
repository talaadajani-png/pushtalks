import { useEffect, useState } from 'react';
import { api, holdKeyName, isMac, mb, message, on, type AppInfo, type Connector, type ModelInfo, type Permissions, type Voice, type Retention, type Rule, type Settings } from '../api';

export default function SettingsView({ settings, onChange }: { settings: Settings; onChange: (s: Settings) => void }) {
  const [mics, setMics] = useState<string[]>([]);
  const [error, setError] = useState('');
  useEffect(() => {
    api.mics().then(setMics);
  }, []);

  const save = async (patch: Partial<Settings>) => {
    setError('');
    try {
      onChange(await api.settingsSave({ ...settings, ...patch }));
    } catch (e) {
      setError(message(e));
    }
  };

  const retentionValue = settings.retention.mode === 'keep_days' ? `days:${settings.retention.days}` : settings.retention.mode;
  const setRetention = (v: string) => {
    const r: Retention = v.startsWith('days:') ? { mode: 'keep_days', days: Number(v.slice(5)) } : ({ mode: v } as Retention);
    save({ retention: r });
  };

  return (
    <div className="stack">
      {error && <div className="banner error">{error}</div>}
      <section className="card">
        <h2>Dictation</h2>
        <div className="settings-grid">
          <label>Start dictation by</label>
          <select value={settings.dictation_mode} onChange={(e) => save({ dictation_mode: e.target.value as Settings['dictation_mode'] })}>
            <option value="hold">Holding {holdKeyName}</option>
            <option value="toggle">Pressing a shortcut to start and stop</option>
          </select>
          {settings.dictation_mode === 'toggle' && (
            <>
              <label>Shortcut</label>
              <input
                type="text"
                defaultValue={settings.toggle_shortcut}
                onBlur={(e) => e.target.value !== settings.toggle_shortcut && save({ toggle_shortcut: e.target.value })}
                placeholder="CommandOrControl+Shift+Space"
              />
            </>
          )}
          <label>Longest dictation</label>
          <select value={settings.max_dictation_secs} onChange={(e) => save({ max_dictation_secs: Number(e.target.value) })}>
            {[30, 60, 120, 180, 300].map((s) => (
              <option key={s} value={s}>{s < 60 ? `${s} seconds` : `${s / 60} minute${s > 60 ? 's' : ''}`}</option>
            ))}
          </select>
          <label>Clean up</label>
          <label className="row" style={{ color: 'inherit' }}>
            <input type="checkbox" checked={settings.remove_fillers} onChange={(e) => save({ remove_fillers: e.target.checked })} />
            Remove “um” and “uh”
          </label>
        </div>
      </section>

      <section className="card">
        <h2>Microphone</h2>
        <select value={settings.mic ?? ''} onChange={(e) => save({ mic: e.target.value || null })} style={{ width: '100%' }}>
          <option value="">System default</option>
          {mics.map((m) => <option key={m} value={m}>{m}</option>)}
        </select>
      </section>

      <section className="card">
        <h2>Meetings</h2>
        <div className="settings-grid">
          <label>Save transcripts to</label>
          <div className="row">
            <span className="small" style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{settings.output_dir}</span>
            <span className="spacer" />
            <button
              onClick={async () => {
                const dir = await api.chooseFolder();
                if (dir) save({ output_dir: dir });
              }}
            >
              Change…
            </button>
          </div>
          <label>Meeting audio</label>
          <select value={retentionValue} onChange={(e) => setRetention(e.target.value)}>
            <option value="delete">Delete after transcription</option>
            <option value="days:7">Keep for 7 days</option>
            <option value="days:30">Keep for 30 days</option>
            <option value="forever">Keep forever</option>
          </select>
        </div>
        <p className="faint" style={{ marginTop: 8 }}>
          Deleting audio after transcription is the default. PUSH's retention policy is waiting for leadership approval.
        </p>
      </section>

      <SpeakerNames settings={settings} onToggle={(remember_voices) => save({ remember_voices })} />
      <PermissionsSection />
      <Claude />
      <Vocabulary />
      <Models settings={settings} onModel={(asr_model) => save({ asr_model })} />

      <section className="card stack">
        <h2>Calls</h2>
        <label className="row">
          <input type="checkbox" checked={settings.suggest_recording} onChange={(e) => save({ suggest_recording: e.target.checked })} />
          Offer to record when a call starts
        </label>
        <p className="faint">
          When Teams, Zoom, Meet or another app starts using your microphone, PushTalks asks if you want to record. It
          never records without your consent confirmation.
        </p>
      </section>

      <section className="card">
        <h2>General</h2>
        <label className="row">
          <input type="checkbox" checked={settings.launch_at_login} onChange={(e) => save({ launch_at_login: e.target.checked })} />
          Open PushTalks when I log in
        </label>
      </section>

      <About />
    </div>
  );
}

function Vocabulary() {
  const [rules, setRules] = useState<Rule[]>([]);
  const [filter, setFilter] = useState('');
  const [heard, setHeard] = useState('');
  const [spelling, setSpelling] = useState('');
  const [error, setError] = useState('');
  const load = () => api.vocabList().then(setRules);
  useEffect(() => {
    load();
  }, []);
  const add = async () => {
    setError('');
    try {
      await api.vocabAdd(heard, spelling);
      setHeard('');
      setSpelling('');
      load();
    } catch (e) {
      setError(message(e));
    }
  };
  const f = filter.toLowerCase();
  const shown = rules.filter((r) => !f || r.heard.toLowerCase().includes(f) || r.spelling.toLowerCase().includes(f));
  return (
    <section className="card stack">
      <h2>Vocabulary</h2>
      <p className="faint">
        Spellings PushTalks has learned. They apply to every new dictation and meeting on this computer. They may
        contain client and people's names, so they stay on this computer.
      </p>
      <div className="row">
        <input type="text" placeholder="Heard as (e.g. tick tock)" value={heard} onChange={(e) => setHeard(e.target.value)} style={{ flex: 1 }} />
        <span className="muted">→</span>
        <input type="text" placeholder="Write as (e.g. TikTok)" value={spelling} onChange={(e) => setSpelling(e.target.value)} style={{ flex: 1 }} />
        <button onClick={add} disabled={!heard.trim() || !spelling.trim()}>Add</button>
      </div>
      {error && <div className="error small">{error}</div>}
      {rules.length > 6 && <input type="search" placeholder="Search vocabulary" value={filter} onChange={(e) => setFilter(e.target.value)} />}
      <div>
        {shown.map((r) => (
          <div key={r.id} className="row" style={{ padding: '4px 0' }}>
            <span className="muted">{r.heard}</span>
            <span className="faint">→</span>
            <b>{r.spelling}</b>
            {r.mode === 'confirm' && <span className="tag" title="A common word: PushTalks asks before changing it">asks first</span>}
            <span className="spacer" />
            <button className="link danger" onClick={async () => { await api.vocabDelete(r.id); load(); }}>Delete</button>
          </div>
        ))}
        {rules.length === 0 && <p className="muted small">Nothing learned yet.</p>}
      </div>
    </section>
  );
}

function Models({ settings, onModel }: { settings: Settings; onModel: (m: string) => void }) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState('');
  const [error, setError] = useState('');
  const load = () => api.models().then(setModels);
  useEffect(() => {
    load();
    const off1 = on<{ id: string; received: number; total: number }>('model-progress', (p) =>
      setProgress(`${p.id}: ${Math.round((p.received / p.total) * 100)}%`),
    );
    const off2 = on<string | null>('models-done', (e) => {
      setBusy(false);
      setProgress('');
      setError(e ?? '');
      load();
    });
    return () => {
      off1();
      off2();
    };
  }, []);
  const missing = models.some((m) => m.required && !m.installed);
  return (
    <section className="card stack">
      <h2>Speech model</h2>
      <div className="settings-grid">
        <label>Model</label>
        <select value={settings.asr_model} onChange={(e) => onModel(e.target.value)}>
          <option value="v2">Parakeet v2, English (recommended)</option>
          <option value="v3">Parakeet v3, 25 European languages</option>
        </select>
      </div>
      {models.map((m) => (
        <div key={m.id} className="row small">
          <span>{m.label}</span>
          <span className="faint">{mb(m.size)}</span>
          <span className="spacer" />
          <span className={m.installed ? 'tag ok' : 'tag'}>{m.installed ? 'Installed, checksum OK' : m.required ? 'Missing' : 'Not installed'}</span>
        </div>
      ))}
      <div className="row">
        <button onClick={load}>Verify checksums</button>
        {missing && (
          <button className="primary" disabled={busy} onClick={async () => { setBusy(true); await api.modelsDownload(); }}>
            {busy ? progress || 'Downloading…' : 'Download missing'}
          </button>
        )}
      </div>
      {error && <div className="error small">{error}</div>}
    </section>
  );
}

function About() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [show, setShow] = useState(false);
  useEffect(() => {
    api.appInfo().then(setInfo);
  }, []);
  if (!info) return null;
  return (
    <section className="card stack">
      <h2>About</h2>
      <p className="small">
        PushTalks {info.version}. Internal PUSH Media tool, draft for review. Speech recognition: NVIDIA Parakeet TDT
        0.6B (CC BY 4.0), run locally with sherpa-onnx. No audio or text leaves this computer.
      </p>
      <button className="link small" onClick={() => setShow(!show)}>{show ? 'Hide' : 'Show'} third-party notices</button>
      {show && <pre className="notices">{info.notices}</pre>}
    </section>
  );
}

function Claude() {
  const [c, setC] = useState<Connector | null>(null);
  const [copied, setCopied] = useState('');
  useEffect(() => {
    api.claudeConnector().then(setC);
  }, []);
  if (!c) return null;
  const copy = async (label: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(label);
    setTimeout(() => setCopied(''), 1500);
  };
  return (
    <section className="card stack">
      <h2>Use with Claude</h2>
      <p className="small">
        Let Claude write minutes and a summary for your meetings. Claude reads a transcript only when you ask it to, and
        saves the minutes into that meeting's Notes, here and in the Markdown file.
      </p>
      <p className="faint">
        This sends the transcript you pick to Claude. Check with your supervisor before using it on client meetings.
      </p>
      {!c.installed && <div className="banner small">The connector isn't in this build. Use the released PushTalks app.</div>}
      <div className="stack">
        <div className="row">
          <b className="small">Claude Code</b>
          <span className="spacer" />
          <button onClick={() => copy('code', c.claude_code)}>{copied === 'code' ? 'Copied' : 'Copy command'}</button>
        </div>
        <code className="small faint" style={{ userSelect: 'text', wordBreak: 'break-all' }}>{c.claude_code}</code>
        <div className="row">
          <b className="small">Claude Desktop</b>
          <span className="spacer" />
          <button onClick={() => copy('desktop', c.claude_desktop)}>{copied === 'desktop' ? 'Copied' : 'Copy config'}</button>
        </div>
        <span className="faint">
          In Claude Desktop: Settings › Developer › Edit Config, add this to claude_desktop_config.json, then restart Claude.
        </span>
      </div>
      <div className="stack">
        <div className="row">
          <b className="small">Automatic minutes prompt</b>
          <span className="spacer" />
          <button onClick={() => copy('prompt', c.auto_minutes_prompt)}>{copied === 'prompt' ? 'Copied' : 'Copy prompt'}</button>
        </div>
        <span className="faint">
          Paste it into Claude Code after adding the connector. Every 30 minutes Claude checks for meetings without
          minutes and writes full minutes for each: summary, attendees, discussion, decisions, action items, risks, open
          questions, next steps and anything to double-check. It runs while that Claude Code window stays open.
        </span>
        <textarea
          className="small"
          readOnly
          rows={8}
          value={c.auto_minutes_prompt}
          style={{ width: '100%', resize: 'vertical', fontFamily: 'inherit' }}
        />
      </div>
      <p className="small">
        Or ask Claude once: “Write minutes for my latest PushTalks meeting.”
      </p>
    </section>
  );
}

const PERMISSIONS: { kind: keyof Permissions; label: string; why: string }[] = [
  { kind: 'accessibility', label: 'Accessibility', why: 'Paste dictated text' },
  { kind: 'input_monitoring', label: 'Input Monitoring', why: `Notice the ${holdKeyName} key` },
  { kind: 'screen_recording', label: 'Screen & System Audio Recording', why: 'Record the other people on a call' },
  { kind: 'calendar', label: 'Calendar (optional)', why: 'Name meetings after the event happening now' },
];

function PermissionsSection() {
  const [perms, setPerms] = useState<Permissions | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    const load = () => api.permissions().then(setPerms).catch(() => {});
    load();
    const t = setInterval(load, 3000);
    return () => clearInterval(t);
  }, []);
  const restart = async () => {
    setError('');
    try {
      await api.appRestart();
    } catch (e) {
      setError(message(e));
    }
  };
  return (
    <section className="card stack">
      <h2>Permissions</h2>
      {isMac &&
        PERMISSIONS.map((p) => (
          <div key={p.kind} className="row">
            <span>{p.label}</span>
            <span className="faint">{p.why}</span>
            <span className="spacer" />
            {perms?.[p.kind] ? (
              <span className="tag ok">Allowed</span>
            ) : (
              <>
                <span className="tag">Not allowed</span>
                <button onClick={() => api.permissionRequest(p.kind)}>Allow</button>
              </>
            )}
          </div>
        ))}
      <div className="row">
        <span>Microphone</span>
        <span className="faint">Dictation and meetings</span>
        <span className="spacer" />
        <button onClick={() => api.permissionOpenSettings('microphone')}>Open settings</button>
      </div>
      <div className="row">
        <span className="small muted">
          Changed a permission and PushTalks hasn't noticed? Restart it. Your settings and meetings are kept.
        </span>
        <span className="spacer" />
        <button className="primary" onClick={restart}>Restart PushTalks</button>
      </div>
      {error && <div className="error small">{error}</div>}
    </section>
  );
}

function SpeakerNames({ settings, onToggle }: { settings: Settings; onToggle: (on: boolean) => void }) {
  const [voices, setVoices] = useState<Voice[]>([]);
  const load = () => api.voices().then(setVoices);
  useEffect(() => {
    load();
  }, [settings.remember_voices]);
  return (
    <section className="card stack">
      <h2>Speaker names</h2>
      <label className="row">
        <input type="checkbox" checked={settings.remember_voices} onChange={(e) => onToggle(e.target.checked)} />
        Remember voices, so people are named automatically in later meetings
      </label>
      <p className="faint">
        When you rename “Speaker 2” to a name, PushTalks saves a voiceprint (numbers, not audio) on this computer and
        uses that name whenever it hears that voice again. Unknown voices stay “Speaker 1, 2…”. Voiceprints are
        biometric data: get your supervisor's approval, and make sure people know, before turning this on.
      </p>
      {voices.length > 0 && (
        <div>
          {voices.map((v) => (
            <div key={v.id} className="row" style={{ padding: '4px 0' }}>
              <span>{v.name}</span>
              <span className="faint">learned from {v.samples} meeting{v.samples === 1 ? '' : 's'}</span>
              <span className="spacer" />
              <button className="link danger" onClick={async () => { await api.voiceDelete(v.id); load(); }}>Forget</button>
            </div>
          ))}
          <div className="row" style={{ marginTop: 8 }}>
            <span className="spacer" />
            <button
              className="danger"
              onClick={async () => {
                if (confirm('Forget every remembered voice?')) {
                  await api.voicesForgetAll();
                  load();
                }
              }}
            >
              Forget all voices
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
