import { useEffect, useState } from 'react';
import { api, clock, message, on, type MeetingRow, type Progress, type Tick, type Unfinished } from '../api';
import Modal from '../components/Modal';

// Placeholder wording until leadership (and legal, if needed) set PUSH's consent policy (PRD §10, §17).
const CONSENT_TEXT =
  'Before you record, tell everyone in the meeting that you are recording and transcribing it, and stop if anyone objects.';

export default function Meeting({
  tick,
  onOpen,
  callRequest,
}: {
  tick: Tick | null;
  onOpen: (id: number) => void;
  callRequest?: { app: string | null; n: number } | null;
}) {
  const [recording, setRecording] = useState(false);
  const [title, setTitle] = useState('');
  const [consentOpen, setConsentOpen] = useState(false);
  const [error, setError] = useState('');
  const [progress, setProgress] = useState<Progress | null>(null);
  const [queued, setQueued] = useState(0);
  const [doneId, setDoneId] = useState<number | null>(null);
  const [unfinished, setUnfinished] = useState<Unfinished[]>([]);
  const [recent, setRecent] = useState<MeetingRow[]>([]);

  const refresh = async () => {
    setRecording(await api.meetingRecording());
    setUnfinished(await api.unfinished());
    setRecent((await api.libraryList()).slice(0, 4));
    const p = await api.processingStatus();
    setQueued(p.queued);
    if (!p.current) setProgress(null);
  };

  useEffect(() => {
    refresh();
    const offs = [
      on<Progress>('processing', setProgress),
      on<number>('processing-done', (id) => {
        setProgress(null);
        setDoneId(id);
        refresh();
      }),
      on<string>('processing-error', (e) => {
        setProgress(null);
        setError(`Processing failed: ${e}. The recording is kept below so you can try again.`);
        refresh();
      }),
    ];
    return () => offs.forEach((off) => off());
  }, []);

  useEffect(() => {
    if (tick) setRecording(tick.recording);
  }, [tick]);

  // From the "On a call?" prompt: go straight to the consent dialog.
  useEffect(() => {
    if (callRequest && !recording) {
      setConsentOpen(true);
    }
  }, [callRequest?.n]);

  const stop = async () => {
    setError('');
    try {
      await api.meetingStop();
      setRecording(false);
      setTitle('');
      refresh();
    } catch (e) {
      setError(message(e));
    }
  };

  return (
    <div className="stack">
      {recording && tick ? (
        <div className="card stack" style={{ alignItems: 'center', padding: 28 }}>
          <div className="row">
            <span className="dot pulse" />
            <span className="muted">Recording{tick.title ? `: ${tick.title}` : ''}</span>
          </div>
          <div className="timer">{clock(tick.elapsed_secs)}</div>
          <div style={{ width: 320 }} className="stack">
            <Level label="Me (microphone)" value={tick.mic_level} />
            {!tick.mic_only && <Level label="Others (computer audio)" value={tick.system_level} />}
          </div>
          <button className="primary" onClick={stop}>Stop and transcribe</button>
          <span className="faint">Audio is saved as you go, so a crash won't lose the meeting.</span>
        </div>
      ) : (
        <div className="card stack">
          <h2>Record a meeting</h2>
          <p className="muted small">
            Works with Teams, Zoom, Meet or any other app: PushTalks records your microphone as “Me” and your computer's
            sound as the other people. Transcription starts when you stop.
          </p>
          <div className="row">
            <input
              type="text"
              placeholder="Title (optional): PushTalks uses the meeting or calendar name"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              style={{ flex: 1 }}
            />
            <button className="primary" onClick={() => setConsentOpen(true)}>Start recording</button>
          </div>
        </div>
      )}

      {error && <div className="banner error">{error}</div>}

      {progress && (
        <div className="card stack">
          <div className="row">
            <b>Transcribing {progress.title}</b>
            <span className="spacer" />
            <span className="muted small">
              {Math.round(progress.fraction * 100)}%
              {progress.eta_secs != null && progress.fraction < 1 ? `, about ${clock(progress.eta_secs)} left` : ''}
            </span>
          </div>
          <div className="progress"><div style={{ width: `${progress.fraction * 100}%` }} /></div>
          <span className="faint">
            You can keep working. Close this window and PushTalks carries on in the menu bar.
            {queued > 0 ? ` ${queued} more waiting.` : ''}
          </span>
        </div>
      )}

      {doneId != null && !progress && (
        <div className="card row">
          <span>✓ Transcript ready.</span>
          <span className="spacer" />
          <button onClick={() => onOpen(doneId)}>Open in PushTalks</button>
          <button onClick={() => api.meetingReveal(doneId)}>Show file</button>
        </div>
      )}

      {unfinished.length > 0 && (
        <div className="card stack">
          <h3>Recordings waiting to be transcribed</h3>
          <p className="faint">These were stopped before transcription finished, for example if PushTalks quit.</p>
          {unfinished.map((u) => (
            <div key={u.dir} className="row">
              <span>{u.title}</span>
              <span className="faint">{new Date(u.started_at).toLocaleString()}</span>
              <span className="spacer" />
              <button onClick={async () => { await api.recordingProcess(u.dir); refresh(); }}>Transcribe</button>
              <button
                className="danger"
                onClick={async () => {
                  if (confirm(`Delete the audio for “${u.title}”? This can't be undone.`)) {
                    await api.recordingDiscard(u.dir);
                    refresh();
                  }
                }}
              >
                Delete
              </button>
            </div>
          ))}
        </div>
      )}

      {recent.length > 0 && (
        <div className="stack" style={{ gap: 8 }}>
          <h3 className="muted" style={{ margin: '6px 0 0' }}>Recent meetings</h3>
          <div className="list">
            {recent.map((m) => (
              <div key={m.id} className="list-item" onClick={() => onOpen(m.id)}>
                <span className="dot" style={{ width: 8, height: 8 }} />
                <span style={{ flex: 1 }}>{m.title}</span>
                <span className="faint">{new Date(m.started_at).toLocaleDateString([], { month: 'short', day: 'numeric' })}</span>
                <span className="faint" style={{ width: 48, textAlign: 'right' }}>{Math.max(1, Math.round(m.duration_secs / 60))} min</span>
              </div>
            ))}
          </div>
        </div>
      )}

      <p className="faint">
        Minutes: once a meeting is transcribed, ask Claude “Write minutes for my latest PushTalks meeting” (set it up
        in Settings › Use with Claude). PushTalks itself never sends transcripts anywhere.
      </p>

      {consentOpen && (
        <Consent
          onCancel={() => setConsentOpen(false)}
          onStart={async (micOnly) => {
            setError('');
            try {
              await api.meetingStart(title || null, true, micOnly);
              setConsentOpen(false);
              setRecording(true);
            } catch (e) {
              setConsentOpen(false);
              setError(message(e));
            }
          }}
        />
      )}
    </div>
  );
}

function Level({ label, value }: { label: string; value: number }) {
  // RMS is small for speech; scale so normal talking fills most of the bar.
  const pct = Math.min(100, Math.sqrt(value) * 220);
  return (
    <div>
      <div className="faint">{label}</div>
      <div className="meter"><div style={{ width: `${pct}%` }} /></div>
    </div>
  );
}

function Consent({ onCancel, onStart }: { onCancel: () => void; onStart: (micOnly: boolean) => void }) {
  const [agreed, setAgreed] = useState(false);
  const [micOnly, setMicOnly] = useState(false);
  const [busy, setBusy] = useState(false);
  return (
    <Modal onClose={onCancel}>
      <h2>Before you record</h2>
      <p>{CONSENT_TEXT}</p>
      <p className="faint">Placeholder wording. PUSH's recording consent policy is waiting for leadership approval.</p>
      <label className="row">
        <input type="checkbox" checked={agreed} onChange={(e) => setAgreed(e.target.checked)} />
        Everyone in this meeting knows it's being recorded.
      </label>
      <label className="row small muted">
        <input type="checkbox" checked={micOnly} onChange={(e) => setMicOnly(e.target.checked)} />
        In-room meeting: record the microphone only
      </label>
      <div className="row">
        <span className="spacer" />
        <button onClick={onCancel}>Cancel</button>
        <button
          className="primary"
          disabled={!agreed || busy}
          onClick={() => {
            setBusy(true);
            onStart(micOnly);
          }}
        >
          Start recording
        </button>
      </div>
    </Modal>
  );
}
