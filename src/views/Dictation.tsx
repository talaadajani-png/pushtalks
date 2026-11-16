import { useEffect, useState } from 'react';
import { api, holdKeyName, message, on, type Correction, type Dictation, type Permissions, type Settings } from '../api';
import CorrectionPrompt from '../components/CorrectionPrompt';

export default function DictationView({ settings, onChange }: { settings: Settings; onChange: (s: Settings) => void }) {
  const [items, setItems] = useState<Dictation[]>([]);
  const [perms, setPerms] = useState<Permissions | null>(null);
  const [editing, setEditing] = useState<number | null>(null);
  const [draft, setDraft] = useState('');
  const [pending, setPending] = useState<Correction | null>(null);
  const [note, setNote] = useState('');
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);

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
          <h3>Dictation notes</h3>
          {items.length > 0 && <span className="faint">{items.length === 1 ? '1 note' : `${items.length} notes`}</span>}
          <span className="spacer" />
          {items.length > 0 && (
            <button
              className="small"
              onClick={async () => {
                if (confirm('Delete all your dictation notes? This can’t be undone.')) {
                  await api.dictationsClear();
                  setItems([]);
                }
              }}
            >
              Clear all
            </button>
          )}
        </div>
        <div className="row small">
          <span className="faint">
            Everything you dictate is kept here, only on this computer. Fix a misheard word and PushTalks can remember
            the spelling.
          </span>
          <span className="spacer" />
          <select
            aria-label="How many notes to keep"
            value={settings.keep_dictations}
            onChange={async (e) => {
              const keep = Number(e.target.value);
              if (keep !== 0 && keep < items.length && !confirm(`Keep only the newest ${keep}? Older notes will be deleted.`)) return;
              onChange(await api.settingsSave({ ...settings, keep_dictations: keep }));
            }}
          >
            <option value={20}>Keep the last 20</option>
            <option value={200}>Keep the last 200</option>
            <option value={0}>Keep all</option>
          </select>
        </div>
        {items.length > 0 && (
          <input type="search" placeholder="Search notes" value={query} onChange={(e) => setQuery(e.target.value)} />
        )}
        {groupByDay(items, query).map(([day, notes]) => (
          <section key={day} className="stack note-day">
            <h4 className="overline">{dayLabel(day)}</h4>
            {notes.map((d) => {
              const expanded = open === d.id;
              return (
                <div key={d.id} className={`note-card${expanded ? ' open' : ''}`}>
                  <button className="note-head" aria-expanded={expanded} onClick={() => setOpen(expanded ? null : d.id)}>
                    {!expanded && <span className="note-preview">{d.text}</span>}
                    <span className="note-meta faint">
                      <span>{new Date(d.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
                      <span>{words(d.text)}</span>
                      {d.duration_secs != null && <span>{duration(d.duration_secs)}</span>}
                    </span>
                    <span className={`chev${expanded ? ' up' : ''}`}>⌄</span>
                  </button>
                  {expanded && (
                    <div className="stack note-body">
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
                        <p className="note-text">{d.text}</p>
                      )}
                      <div className="row">
                        <button
                          className="small"
                          onClick={async () => {
                            await navigator.clipboard.writeText(d.text);
                            setCopied(d.id);
                            setTimeout(() => setCopied(null), 1500);
                          }}
                        >
                          {copied === d.id ? 'Copied' : 'Copy'}
                        </button>
                        <button className="small" onClick={() => { setEditing(d.id); setDraft(d.text); }}>
                          Fix a word
                        </button>
                        <span className="spacer" />
                        <button
                          className="small danger"
                          onClick={async () => {
                            if (confirm('Delete this note?')) {
                              await api.dictationDelete(d.id);
                              setOpen(null);
                              setItems(await api.dictations());
                            }
                          }}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </section>
        ))}
        {items.length === 0 && <p className="muted">No notes yet. {how}</p>}
        {items.length > 0 && groupByDay(items, query).length === 0 && (
          <p className="muted">No notes match “{query.trim()}”.</p>
        )}
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

/** Notes matching `query`, grouped by local day (newest first, as the list comes). */
export function groupByDay(items: Dictation[], query: string): [string, Dictation[]][] {
  const q = query.trim().toLowerCase();
  const days = new Map<string, Dictation[]>();
  for (const d of items) {
    if (q && !d.text.toLowerCase().includes(q)) continue;
    const day = new Date(d.created_at).toDateString();
    days.set(day, [...(days.get(day) ?? []), d]);
  }
  return [...days.entries()];
}

function dayLabel(day: string): string {
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (day === today.toDateString()) return 'Today';
  if (day === yesterday.toDateString()) return 'Yesterday';
  return new Date(day).toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' });
}

function words(text: string): string {
  const n = text.trim().split(/\s+/).filter(Boolean).length;
  return n === 1 ? '1 word' : `${n} words`;
}

/** 4 → "4s", 95 → "1m 35s" */
function duration(secs: number): string {
  const s = Math.round(secs);
  return s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
}
