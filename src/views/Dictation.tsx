import { useEffect, useState } from 'react';
import { api, holdKeyName, message, on, type Correction, type Dictation, type Permissions, type Settings } from '../api';
import CorrectionPrompt from '../components/CorrectionPrompt';

export default function DictationView({ settings }: { settings: Settings }) {
  const [items, setItems] = useState<Dictation[]>([]);
  const [perms, setPerms] = useState<Permissions | null>(null);
  const [editing, setEditing] = useState<number | null>(null);
  const [draft, setDraft] = useState('');
  const [pending, setPending] = useState<Correction | null>(null);
  const [note, setNote] = useState('');

  useEffect(() => {
    const load = () => api.dictations().then(setItems);
    load();
    api.permissions().then(setPerms);
    return on('dictations-changed', load);
  }, []);

  const save = async (d: Dictation) => {
    setEditing(null);
    if (draft.trim() === d.text.trim()) return;
    await api.dictationUpdate(d.id, draft.trim());
    setItems(await api.dictations());
    setPending(await api.vocabCorrection(d.text, draft));
  };

  const how =
    settings.dictation_mode === 'hold'
      ? `Hold ${holdKeyName}, speak, then let go.`
      : `Press ${settings.toggle_shortcut} to start, and again to stop.`;

  return (
    <div className="stack">
      <div className="card stack">
        <h2>Dictate anywhere</h2>
        <p>{how} The text is pasted where you're typing, and your clipboard is put back afterwards.</p>
        {settings.dictation_mode === 'hold' && perms && !perms.hold_key_active && (
          <div className="banner small">
            PushTalks can't see the {holdKeyName} key yet.{' '}
            <button className="link" onClick={() => api.permissionRequest('input_monitoring')}>Allow Input Monitoring</button>
            , then{' '}
            <button className="link" onClick={async () => setPerms({ ...perms, hold_key_active: await api.holdKeyRetry() })}>
              check again
            </button>
            .
          </div>
        )}
        {perms && !perms.accessibility && (
          <div className="banner small">
            PushTalks needs Accessibility access to paste.{' '}
            <button className="link" onClick={() => api.permissionRequest('accessibility')}>Allow</button>
          </div>
        )}
      </div>

      <div className="card stack">
        <div className="row">
          <h3>Recent dictations</h3>
          <span className="spacer" />
          {items.length > 0 && (
            <button
              className="small"
              onClick={async () => {
                if (confirm('Clear your recent dictations?')) {
                  await api.dictationsClear();
                  setItems([]);
                }
              }}
            >
              Clear
            </button>
          )}
        </div>
        <p className="faint">
          The last 20, stored only on this computer. Fix a misheard word here and PushTalks can remember the spelling.
          Edits made in other apps can't be learned.
        </p>
        {items.map((d) => (
          <div key={d.id} className="segment">
            <span className="ts">{new Date(d.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
            {editing === d.id ? (
              <textarea
                autoFocus
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onBlur={() => save(d)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    (e.target as HTMLTextAreaElement).blur();
                  }
                }}
              />
            ) : (
              <span className="text" onClick={() => { setEditing(d.id); setDraft(d.text); }}>{d.text}</span>
            )}
          </div>
        ))}
        {items.length === 0 && <p className="muted">Nothing yet.</p>}
      </div>
      {note && <div className="toast" onClick={() => setNote('')}>{note}</div>}
      {pending && (
        <CorrectionPrompt
          correction={pending}
          onAlways={async () => {
            try {
              const r = await api.vocabAdd(pending.heard, pending.spelling);
              setNote(r.mode === 'confirm' ? `Saved. PushTalks will ask before changing “${r.heard}” in meetings.` : `Saved: “${r.heard}” → “${r.spelling}”.`);
            } catch (e) {
              setNote(message(e));
            }
            setPending(null);
            setTimeout(() => setNote(''), 4000);
          }}
          onOnce={() => setPending(null)}
          onNever={async () => {
            await api.vocabNeverAsk(pending.heard);
            setPending(null);
          }}
        />
      )}
    </div>
  );
}
