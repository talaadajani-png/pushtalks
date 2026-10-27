import { Fragment, useEffect, useState } from 'react';
import {
  api,
  clock,
  message,
  on,
  type Correction,
  type MeetingOut,
  type MeetingRow,
  type SearchHit,
  type Segment,
} from '../api';
import CorrectionPrompt from '../components/CorrectionPrompt';
import Modal from '../components/Modal';

export default function Library({ openId, onOpenChange }: { openId: number | null; onOpenChange: (id: number | null) => void }) {
  if (openId != null) return <Transcript id={openId} onBack={() => onOpenChange(null)} />;
  return <MeetingList onOpen={onOpenChange} />;
}

function MeetingList({ onOpen }: { onOpen: (id: number) => void }) {
  const [rows, setRows] = useState<MeetingRow[]>([]);
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    const load = () => api.libraryList().then(setRows).catch((e) => setError(message(e)));
    load();
    return on('library-changed', load);
  }, []);

  useEffect(() => {
    if (!query.trim()) {
      setHits(null);
      return;
    }
    const t = setTimeout(() => api.librarySearch(query).then(setHits).catch((e) => setError(message(e))), 150);
    return () => clearTimeout(t);
  }, [query]);

  const list = hits ?? rows.map((meeting) => ({ meeting, snippet: '' }));
  return (
    <div className="stack">
      <input type="search" placeholder="Search titles and transcripts" value={query} onChange={(e) => setQuery(e.target.value)} />
      {error && <div className="error small">{error}</div>}
      <div className="list">
        {list.map(({ meeting: m, snippet }) => (
          <div key={m.id} className="list-item" onClick={() => onOpen(m.id)}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div>{m.title}</div>
              {snippet && <div className="snippet"><Snippet text={snippet} /></div>}
            </div>
            <span className="faint">{new Date(m.started_at).toLocaleDateString()}</span>
            <span className="faint" style={{ width: 56, textAlign: 'right' }}>{Math.round(m.duration_secs / 60)} min</span>
          </div>
        ))}
        {list.length === 0 && (
          <p className="muted">{hits ? 'No matches.' : 'No meetings yet. Record one from the Meeting tab.'}</p>
        )}
      </div>
    </div>
  );
}

// FTS5 marks matches with [ and ]; render them highlighted without using HTML.
function Snippet({ text }: { text: string }) {
  const parts = text.split(/(\[[^\]]*\])/g);
  return (
    <>
      {parts.map((p, i) =>
        p.startsWith('[') && p.endsWith(']') ? <mark key={i}>{p.slice(1, -1)}</mark> : <Fragment key={i}>{p}</Fragment>,
      )}
    </>
  );
}

interface Pending {
  correction: Correction;
}

interface Undo {
  label: string;
  run: () => Promise<void>;
  /// Just a note: no Undo button.
  info?: boolean;
}

function Transcript({ id, onBack }: { id: number; onBack: () => void }) {
  const [m, setM] = useState<MeetingOut | null>(null);
  const [title, setTitle] = useState('');
  const [editing, setEditing] = useState<number | null>(null);
  const [draft, setDraft] = useState('');
  const [pending, setPending] = useState<Pending | null>(null);
  const [undo, setUndo] = useState<Undo | null>(null);
  const [error, setError] = useState('');
  const [renaming, setRenaming] = useState<{ from: string; to: string } | null>(null);

  const show = (out: MeetingOut) => {
    setM(out);
    setTitle(out.row.title);
  };

  useEffect(() => {
    const load = () => api.meetingGet(id).then(show).catch((e) => setError(message(e)));
    load();
    // Claude may have saved minutes while PushTalks was in the background.
    window.addEventListener('focus', load);
    return () => window.removeEventListener('focus', load);
  }, [id]);

  useEffect(() => {
    if (!undo) return;
    const t = setTimeout(() => setUndo(null), 10000);
    return () => clearTimeout(t);
  }, [undo]);

  if (!m) return error ? <div className="stack"><button onClick={onBack}>← Library</button><div className="error">{error}</div></div> : null;

  const edits = (segments: Segment[]) => segments.map((s) => ({ text: s.text, speaker: s.speaker }));

  const saveSegment = async (index: number) => {
    const old = m.segments[index].text;
    setEditing(null);
    if (draft.trim() === old.trim()) return;
    const segs = m.segments.map((s, i) => (i === index ? { ...s, text: draft } : s));
    try {
      show(await api.meetingUpdate(id, title, edits(segs)));
      const c = await api.vocabCorrection(old, draft);
      if (c) setPending({ correction: c });
    } catch (e) {
      setError(message(e));
    }
  };

  const always = async () => {
    if (!pending) return;
    const c = pending.correction;
    setPending(null);
    try {
      const snapshot = m.segments;
      const rule = await api.vocabAdd(c.heard, c.spelling);
      if (rule.mode === 'confirm') {
        show(await api.meetingGet(id));
        setUndo({ label: `Saved. PushTalks will ask before changing “${c.heard}”.`, run: async () => api.vocabDelete(rule.id) });
        return;
      }
      const r = await api.meetingApplyRule(id, rule.id, null);
      show(r.meeting);
      setUndo({
        label: r.changed > 0 ? `Saved, and fixed ${r.changed} more ${r.changed === 1 ? 'place' : 'places'} here.` : 'Saved for future transcripts.',
        run: async () => {
          await api.vocabDelete(rule.id);
          if (r.changed > 0) show(await api.meetingRestore(id, snapshot));
        },
      });
    } catch (e) {
      setError(message(e));
    }
  };

  const renameSpeaker = async (from: string, to: string) => {
    setRenaming(null);
    if (!to.trim() || to.trim() === from) return;
    try {
      const r = await api.meetingRenameSpeaker(id, from, to.trim());
      show(r.meeting);
      if (r.learned_voice) {
        setUndo({ label: `PushTalks will recognise ${to.trim()}'s voice in future meetings.`, run: async () => {}, info: true });
      }
    } catch (e) {
      setError(message(e));
    }
  };

  const saveTitle = async () => {
    if (title.trim() === m.row.title || !title.trim()) return;
    try {
      show(await api.meetingUpdate(id, title, edits(m.segments)));
    } catch (e) {
      setError(message(e));
    }
  };

  const del = async () => {
    if (!confirm(`Delete “${m.row.title}” and its transcript files? This can't be undone.`)) return;
    await api.meetingDelete(id, true);
    onBack();
  };

  // Paragraphs like the Markdown file: a new header on speaker change or every 60 s.
  let paraStart = -Infinity;
  let paraSpeaker = '';

  return (
    <div className="stack">
      <div className="row">
        <button onClick={onBack}>← Library</button>
        <span className="spacer" />
        <button onClick={() => api.meetingOpen(id)}>Open file</button>
        <button onClick={() => api.meetingReveal(id)}>Show in folder</button>
        <button className="danger" onClick={del}>Delete</button>
      </div>
      <input
        type="text"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onBlur={saveTitle}
        onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
        aria-label="Meeting title"
        style={{ fontSize: 20, fontWeight: 600, background: 'none', border: 'none', padding: 0 }}
      />
      <div className="row wrap small muted">
        <span>{new Date(m.row.started_at).toLocaleString()}</span>
        <span>·</span>
        <span>{Math.round(m.row.duration_secs / 60)} min</span>
        <span>·</span>
        <span>Speakers:</span>
        {m.speakers.map((s) => (
          <button key={s} className="tag" onClick={() => setRenaming({ from: s, to: s.startsWith('Speaker ') ? '' : s })} title="Rename">{s} ✎</button>
        ))}
        <span className="tag">[AI: DRAFT]</span>
      </div>
      <Notes id={id} notes={m.notes} onSaved={show} />
      <p className="faint">Click any line to correct it. Fix a name once and PushTalks can remember the spelling.</p>
      {error && <div className="error small">{error}</div>}
      <div className="card">
        {m.segments.map((s, i) => {
          const header = s.speaker !== paraSpeaker || s.start - paraStart >= 60;
          if (header) {
            paraStart = s.start;
            paraSpeaker = s.speaker;
          }
          const suggestions = m.suggestions.filter((x) => x.segment === i);
          return (
            <div key={i} className="segment">
              <span className="ts">{header ? clock(s.start) : ''}</span>
              <div>
                {header && <span className="speaker">{s.speaker}</span>}
                {editing === i ? (
                  <textarea
                    aria-label="Edit line"
                    autoFocus
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onBlur={() => saveSegment(i)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        (e.target as HTMLTextAreaElement).blur();
                      }
                      if (e.key === 'Escape') setEditing(null);
                    }}
                  />
                ) : (
                  <span
                    className="text"
                    onClick={() => {
                      setEditing(i);
                      setDraft(s.text);
                    }}
                  >
                    {s.text}
                  </span>
                )}
                {s.original_text && editing !== i && <span className="edited" title={`Model heard: ${s.original_text}`}>edited</span>}
                {suggestions.map((x) => (
                  <div key={x.rule_id} className="suggest">
                    Change “{x.heard}” to “{x.spelling}” here?{' '}
                    <button
                      className="link"
                      onClick={async () => show((await api.meetingApplyRule(id, x.rule_id, i)).meeting)}
                    >
                      Change
                    </button>
                  </div>
                ))}
              </div>
            </div>
          );
        })}
        {m.segments.length === 0 && <p className="muted">No speech was found in this recording.</p>}
      </div>

      {renaming && (
        <Modal onClose={() => setRenaming(null)}>
          <h2>Who is “{renaming.from}”?</h2>
          <input
            type="text"
            autoFocus
            aria-label="Speaker name"
            placeholder="Name, e.g. Jordan"
            value={renaming.to}
            onChange={(e) => setRenaming({ ...renaming, to: e.target.value })}
            onKeyDown={(e) => e.key === 'Enter' && renameSpeaker(renaming.from, renaming.to)}
          />
          <div className="row">
            <span className="spacer" />
            <button onClick={() => setRenaming(null)}>Cancel</button>
            <button className="primary" onClick={() => renameSpeaker(renaming.from, renaming.to)}>Rename</button>
          </div>
        </Modal>
      )}
      {pending && (
        <CorrectionPrompt
          correction={pending.correction}
          onAlways={always}
          onOnce={() => setPending(null)}
          onNever={async () => {
            await api.vocabNeverAsk(pending.correction.heard);
            setPending(null);
          }}
        />
      )}
      {!pending && undo && (
        <div className="toast row">
          <span>{undo.label}</span>
          {!undo.info && (
            <button
              onClick={async () => {
                const u = undo;
                setUndo(null);
                await u.run();
              }}
            >
              Undo
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// Minutes and summary, usually written by Claude through the PushTalks connector.
function Notes({ id, notes, onSaved }: { id: number; notes: string | null; onSaved: (m: MeetingOut) => void }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState('');
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState('');

  if (editing) {
    return (
      <div className="card stack">
        <h3>Minutes</h3>
        <textarea aria-label="Minutes" rows={12} value={draft} onChange={(e) => setDraft(e.target.value)} />
        {error && <div className="error small">{error}</div>}
        <div className="row">
          <span className="spacer" />
          <button onClick={() => setEditing(false)}>Cancel</button>
          <button
            className="primary"
            onClick={async () => {
              try {
                onSaved(await api.meetingSetNotes(id, draft));
                setEditing(false);
              } catch (e) {
                setError(message(e));
              }
            }}
          >
            Save
          </button>
        </div>
      </div>
    );
  }

  if (!notes) {
    return (
      <div className="card small muted">
        <b style={{ color: 'var(--text)' }}>No minutes yet.</b> In Claude, ask: “Write minutes for my latest PushTalks
        meeting.” Claude saves them here (set it up in Settings › Use with Claude).{' '}
        <button className="link" onClick={() => { setDraft(''); setEditing(true); }}>Write them yourself</button>
      </div>
    );
  }

  return (
    <div className="card stack">
      <div className="row">
        <h3>Minutes</h3>
        <span className="spacer" />
        <button
          onClick={async () => {
            await navigator.clipboard.writeText(notes);
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          }}
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
        <button onClick={() => { setDraft(notes); setEditing(true); }}>Edit</button>
      </div>
      <div className="notes">{notes}</div>
    </div>
  );
}
