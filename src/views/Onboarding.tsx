import { useEffect, useState } from 'react';
import { api, holdKeyName, isMac, mb, message, on, type MicResult, type ModelInfo, type Permissions, type Settings } from '../api';

type Step = 'welcome' | 'mic' | 'accessibility' | 'input' | 'screen' | 'model' | 'try';
const STEPS: Step[] = isMac
  ? ['welcome', 'mic', 'accessibility', 'input', 'screen', 'model', 'try']
  : ['welcome', 'mic', 'model', 'try'];

// First-launch flow (PRD §9).
export default function Onboarding({ onDone }: { onDone: () => void }) {
  const [i, setI] = useState(0);
  const step = STEPS[i];
  const next = () => setI((n) => Math.min(n + 1, STEPS.length - 1));
  return (
    <div className="content">
      <div className="onboarding">
        <div className="steps">
          {STEPS.map((s, n) => (
            <div key={s} className={n <= i ? 'done' : ''} />
          ))}
        </div>
        {step === 'welcome' && <Welcome onNext={next} />}
        {step === 'mic' && <Mic onNext={next} />}
        {step === 'accessibility' && (
          <Permission
            kind="accessibility"
            title="Allow PushTalks to paste for you"
            body="Dictation pastes text into the app you're typing in. macOS calls this Accessibility access."
            onNext={next}
          />
        )}
        {step === 'input' && (
          <Permission
            kind="input_monitoring"
            title={`Let PushTalks notice the ${holdKeyName} key`}
            body={`You'll hold ${holdKeyName} to dictate. macOS calls this Input Monitoring. PushTalks only watches that one key and never records what you type.`}
            onNext={next}
          />
        )}
        {step === 'screen' && (
          <Permission
            kind="screen_recording"
            title="Allow meeting audio"
            body="To transcribe the other people on a call, PushTalks records your computer's sound. macOS files this under Screen & System Audio Recording. PushTalks never records your screen. You can skip this if you only want dictation."
            onNext={next}
            optional
          />
        )}
        {step === 'model' && <Model onNext={next} />}
        {step === 'try' && <Try onDone={onDone} />}
      </div>
    </div>
  );
}

function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="stack">
      <h1>Welcome to PushTalks</h1>
      <p className="muted">
        Dictate anywhere and get transcripts of your meetings. Speech recognition runs on this computer. Your audio and
        transcripts never leave it.
      </p>
      <div className="card stack">
        <div><b>Dictation.</b> Hold {holdKeyName}, speak, let go. The text appears where you're typing.</div>
        <div><b>Meetings.</b> Record any call (Teams, Zoom, Meet). You get a Markdown transcript with speakers and timestamps.</div>
        <div className="faint">
          Transcripts are internal records and may contain confidential client information. They're marked [AI: DRAFT]
          for you to review.
        </div>
      </div>
      <div className="row">
        <span className="spacer" />
        <button className="primary" onClick={onNext}>Get started</button>
      </div>
    </div>
  );
}

function Mic({ onNext }: { onNext: () => void }) {
  const [result, setResult] = useState<MicResult | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [level, setLevel] = useState(0);
  const [mics, setMics] = useState<string[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    api.mics().then(setMics);
    api.settingsGet().then(setSettings);
    return on<number>('mic-level', setLevel);
  }, []);
  const test = async () => {
    setBusy(true);
    setError('');
    setResult(null);
    try {
      setResult(await api.micCheck());
    } catch (e) {
      setError(message(e));
    }
    setBusy(false);
    setLevel(0);
  };
  const chooseMic = async (mic: string) => {
    if (!settings) return;
    setSettings(await api.settingsSave({ ...settings, mic: mic || null }));
    setResult(null);
  };
  const heard = result?.speech === true;
  return (
    <div className="stack">
      <h1>Microphone</h1>
      <p className="muted">
        Click Test, then talk normally for a few seconds. Your computer may ask to allow microphone access first.
      </p>
      <div className="card stack">
        {mics.length > 1 && settings && (
          <select value={settings.mic ?? ''} onChange={(e) => chooseMic(e.target.value)}>
            <option value="">System default microphone</option>
            {mics.map((m) => <option key={m} value={m}>{m}</option>)}
          </select>
        )}
        <div className="row">
          <button onClick={test} disabled={busy}>{busy ? 'Listening… keep talking' : 'Test microphone'}</button>
          {result && (
            <span className={heard ? 'tag ok' : 'tag'}>
              {heard ? `Heard you on ${result.device}` : result.blocked ? 'Microphone blocked' : `Too quiet on ${result.device}`}
            </span>
          )}
        </div>
        {busy && <div className="meter"><div style={{ width: `${Math.min(100, Math.sqrt(level) * 220)}%` }} /></div>}
        {result?.blocked && (
          <div className="small muted">
            macOS is giving PushTalks silence, which means microphone access is off. Turn PushTalks on in{' '}
            <button className="link" onClick={() => api.permissionOpenSettings('microphone')}>Microphone settings</button>,
            then <button className="link" onClick={() => api.appRestart()}>restart PushTalks</button> and test again.
          </div>
        )}
        {result && !heard && !result.blocked && (
          <div className="small muted">
            PushTalks only heard background noise. Speak closer to the mic, or pick a different microphone above, and test
            again.
          </div>
        )}
        {error && <div className="error small">{error}</div>}
      </div>
      <div className="row">
        <span className="spacer" />
        {!heard && <button onClick={onNext}>Continue anyway</button>}
        <button className="primary" onClick={onNext} disabled={!heard}>Continue</button>
      </div>
    </div>
  );
}

function Permission({
  kind,
  title,
  body,
  onNext,
  optional,
}: {
  kind: keyof Permissions;
  title: string;
  body: string;
  onNext: () => void;
  optional?: boolean;
}) {
  const [perms, setPerms] = useState<Permissions | null>(null);
  useEffect(() => {
    const poll = async () => {
      const p = await api.permissions();
      if (kind === 'input_monitoring' && p.input_monitoring && !p.hold_key_active) {
        p.hold_key_active = await api.holdKeyRetry();
      }
      setPerms(p);
    };
    poll();
    const t = setInterval(poll, 1500);
    return () => clearInterval(t);
  }, [kind]);
  const granted = perms?.[kind] === true;
  return (
    <div className="stack">
      <h1>{title}</h1>
      <p className="muted">{body}</p>
      <div className="card row">
        {granted ? (
          <span className="tag ok">Allowed</span>
        ) : (
          <>
            <button onClick={() => api.permissionRequest(kind)}>Allow</button>
            <button className="link" onClick={() => api.permissionOpenSettings(kind)}>Open System Settings</button>
          </>
        )}
      </div>
      {!granted && (
        <p className="faint">
          Already allowed it? macOS sometimes only tells PushTalks after a restart.{' '}
          <button className="link" onClick={() => api.appRestart()}>Restart PushTalks</button>
        </p>
      )}
      <div className="row">
        <span className="spacer" />
        {!granted && <button onClick={onNext}>{optional ? 'Skip for now' : 'Continue anyway'}</button>}
        <button className="primary" onClick={onNext} disabled={!granted}>Continue</button>
      </div>
    </div>
  );
}

function Model({ onNext }: { onNext: () => void }) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, number>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const refresh = () => api.models().then(setModels);
  useEffect(() => {
    refresh();
    const off1 = on<{ id: string; received: number; total: number }>('model-progress', (p) =>
      setProgress((old) => ({ ...old, [p.id]: p.received / p.total })),
    );
    const off2 = on<string | null>('models-done', (e) => {
      setBusy(false);
      setError(e ?? '');
      refresh();
    });
    return () => {
      off1();
      off2();
    };
  }, []);
  const required = models.filter((m) => m.required);
  const missing = required.filter((m) => !m.installed);
  const total = missing.reduce((n, m) => n + m.download_size, 0);
  const done = missing.reduce((n, m) => n + (progress[m.id] ?? 0) * m.download_size, 0);
  const ready = required.length > 0 && missing.length === 0;
  return (
    <div className="stack">
      <h1>Download the speech model</h1>
      <p className="muted">
        PushTalks downloads NVIDIA's Parakeet model once ({mb(required.reduce((n, m) => n + m.size, 0))} on disk). Every
        file is checked against a pinned SHA-256 fingerprint. After this, PushTalks works offline.
      </p>
      <div className="card stack">
        {required.map((m) => (
          <div key={m.id} className="row">
            <span>{m.label}</span>
            <span className="spacer" />
            <span className={m.installed ? 'tag ok' : 'tag'}>
              {m.installed ? 'Installed' : progress[m.id] ? `${Math.round(progress[m.id] * 100)}%` : mb(m.download_size)}
            </span>
          </div>
        ))}
        {busy && (
          <div className="progress"><div style={{ width: `${total ? (done / total) * 100 : 0}%` }} /></div>
        )}
        {error && <div className="error small">{error}. Check your connection and try again; it resumes where it stopped.</div>}
      </div>
      <div className="row">
        <span className="spacer" />
        {!ready && (
          <button
            className="primary"
            disabled={busy || models.length === 0}
            onClick={async () => {
              setBusy(true);
              setError('');
              await api.modelsDownload();
            }}
          >
            {busy ? 'Downloading…' : `Download ${mb(total)}`}
          </button>
        )}
        {ready && <button className="primary" onClick={onNext}>Continue</button>}
      </div>
    </div>
  );
}

function Try({ onDone }: { onDone: () => void }) {
  const [text, setText] = useState('');
  return (
    <div className="stack">
      <h1>Try it</h1>
      <p className="muted">
        Click in the box, then hold <b>{holdKeyName}</b> and read this sentence out loud. Let go when you're done.
      </p>
      <div className="card">
        <i>“PUSH Media helps brands grow with performance media, creative and analytics.”</i>
      </div>
      <textarea rows={4} value={text} onChange={(e) => setText(e.target.value)} placeholder="Your words appear here" autoFocus />
      <p className="faint">The first dictation loads the model and takes a few seconds longer.</p>
      <div className="row">
        <span className="spacer" />
        <button className={text ? 'primary' : ''} onClick={onDone}>{text ? 'Finish' : 'Skip and finish'}</button>
      </div>
    </div>
  );
}
