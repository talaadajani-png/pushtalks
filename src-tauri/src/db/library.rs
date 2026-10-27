//! Local SQLite store (PRD §6.5, §6.7): the meeting index with FTS5 search, recent dictations,
//! and the learned vocabulary. Transcript files on disk stay the source of truth for content.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::output::{self, Meeting};
use crate::vocab::{rules, Rule, RuleMode};
use crate::voices::{self, Voice};

/// How many recent dictations to keep for after-the-fact correction.
const DICTATION_HISTORY: i64 = 20;

pub struct Library {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeetingRow {
    pub id: i64,
    pub title: String,
    pub started_at: DateTime<Local>,
    pub duration_secs: f32,
    pub md_path: PathBuf,
    /// Present while the raw audio is kept (retention setting).
    pub audio_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchHit {
    pub meeting: MeetingRow,
    /// A short excerpt around the match, with the match wrapped in `[` `]`.
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Dictation {
    pub id: i64,
    pub created_at: DateTime<Local>,
    pub text: String,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meetings (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    started_at TEXT NOT NULL,
    duration_secs REAL NOT NULL,
    md_path TEXT NOT NULL UNIQUE,
    audio_dir TEXT,
    audio_expires_at TEXT
);
CREATE VIRTUAL TABLE IF NOT EXISTS meetings_fts USING fts5(title, body, tokenize = 'unicode61');
CREATE TABLE IF NOT EXISTS dictations (
    id INTEGER PRIMARY KEY,
    created_at TEXT NOT NULL,
    text TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS vocab (
    id INTEGER PRIMARY KEY,
    heard TEXT NOT NULL UNIQUE COLLATE NOCASE,
    spelling TEXT NOT NULL,
    mode TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS vocab_never_ask (
    heard TEXT PRIMARY KEY COLLATE NOCASE
);
CREATE TABLE IF NOT EXISTS voices (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    embedding BLOB NOT NULL,
    samples INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS meeting_voiceprints (
    meeting_id INTEGER NOT NULL,
    label TEXT NOT NULL,
    embedding BLOB NOT NULL,
    PRIMARY KEY (meeting_id, label)
);
";

fn parse_time(s: String) -> DateTime<Local> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Local))
        .unwrap_or_else(|_| Local::now())
}

fn meeting_row(r: &rusqlite::Row) -> rusqlite::Result<MeetingRow> {
    Ok(MeetingRow {
        id: r.get(0)?,
        title: r.get(1)?,
        started_at: parse_time(r.get(2)?),
        duration_secs: r.get(3)?,
        md_path: PathBuf::from(r.get::<_, String>(4)?),
        audio_dir: r.get::<_, Option<String>>(5)?.map(PathBuf::from),
    })
}

const MEETING_COLS: &str = "m.id, m.title, m.started_at, m.duration_secs, m.md_path, m.audio_dir";

impl Library {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).context("opening the library database")?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    // ---- Meetings ----

    /// Adds or updates a meeting and its search text. Returns its id.
    pub fn upsert_meeting(
        &self,
        id: Option<i64>,
        title: &str,
        started_at: &DateTime<Local>,
        duration_secs: f32,
        md_path: &Path,
        body: &str,
    ) -> Result<i64> {
        let path = md_path.to_string_lossy();
        let id = match id {
            Some(id) => {
                self.conn.execute(
                    "UPDATE meetings SET title = ?2, started_at = ?3, duration_secs = ?4, md_path = ?5 WHERE id = ?1",
                    params![id, title, started_at.to_rfc3339(), duration_secs, path],
                )?;
                id
            }
            None => {
                self.conn.execute(
                    "INSERT INTO meetings (title, started_at, duration_secs, md_path) VALUES (?1, ?2, ?3, ?4)",
                    params![title, started_at.to_rfc3339(), duration_secs, path],
                )?;
                self.conn.last_insert_rowid()
            }
        };
        self.conn
            .execute("DELETE FROM meetings_fts WHERE rowid = ?1", [id])?;
        self.conn.execute(
            "INSERT INTO meetings_fts (rowid, title, body) VALUES (?1, ?2, ?3)",
            params![id, title, body],
        )?;
        Ok(id)
    }

    pub fn set_audio(
        &self,
        id: i64,
        dir: Option<&Path>,
        expires: Option<DateTime<Local>>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE meetings SET audio_dir = ?2, audio_expires_at = ?3 WHERE id = ?1",
            params![
                id,
                dir.map(|d| d.to_string_lossy().into_owned()),
                expires.map(|e| e.to_rfc3339())
            ],
        )?;
        Ok(())
    }

    /// Meetings whose kept audio has passed its retention date.
    pub fn expired_audio(&self, now: &DateTime<Local>) -> Result<Vec<(i64, PathBuf)>> {
        let mut st = self.conn.prepare(
            "SELECT id, audio_dir FROM meetings WHERE audio_dir IS NOT NULL AND audio_expires_at IS NOT NULL AND audio_expires_at <= ?1",
        )?;
        let rows = st
            .query_map([now.to_rfc3339()], |r| {
                Ok((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?)))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn meetings(&self) -> Result<Vec<MeetingRow>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {MEETING_COLS} FROM meetings m ORDER BY m.started_at DESC"
        ))?;
        let rows = st
            .query_map([], meeting_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn meeting(&self, id: i64) -> Result<Option<MeetingRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {MEETING_COLS} FROM meetings m WHERE m.id = ?1"),
                [id],
                meeting_row,
            )
            .optional()?)
    }

    pub fn delete_meeting(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM meetings WHERE id = ?1", [id])?;
        self.conn.execute(
            "DELETE FROM meeting_voiceprints WHERE meeting_id = ?1",
            [id],
        )?;
        self.conn
            .execute("DELETE FROM meetings_fts WHERE rowid = ?1", [id])?;
        Ok(())
    }

    /// Full-text search over titles and transcripts, best matches first.
    pub fn search(&self, query: &str) -> Result<Vec<SearchHit>> {
        let q = fts_query(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut st = self.conn.prepare(&format!(
            "SELECT {MEETING_COLS}, snippet(meetings_fts, 1, '[', ']', '…', 12)
             FROM meetings_fts f JOIN meetings m ON m.id = f.rowid
             WHERE meetings_fts MATCH ?1 ORDER BY rank LIMIT 50"
        ))?;
        let rows = st
            .query_map([q], |r| {
                Ok(SearchHit {
                    meeting: meeting_row(r)?,
                    snippet: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Reads a meeting's transcript files back into a [`Meeting`].
    pub fn load_meeting(&self, id: i64) -> Result<(MeetingRow, Meeting)> {
        let row = self
            .meeting(id)?
            .ok_or_else(|| anyhow::anyhow!("That meeting is no longer in the library."))?;
        let side =
            output::read_sidecar(&row.md_path.with_extension("json")).with_context(|| {
                "Couldn't read the transcript file. It may have been moved or deleted.".to_string()
            })?;
        let m = Meeting {
            title: row.title.clone(),
            started_at: row.started_at,
            duration_secs: row.duration_secs,
            asr_model: side.asr_model,
            segments: side.segments,
            notes: side.notes,
        };
        Ok((row, m))
    }

    /// Rewrites a meeting's Markdown and JSON files (renaming them if the title changed) and
    /// re-indexes it for search.
    pub fn save_meeting(&self, row: &MeetingRow, m: &Meeting) -> Result<MeetingRow> {
        let dir = row
            .md_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow::anyhow!("The transcript has no folder."))?;
        let md = m.write(&dir, Some(&row.md_path))?;
        if md != row.md_path {
            let _ = std::fs::remove_file(&row.md_path);
            let _ = std::fs::remove_file(row.md_path.with_extension("json"));
        }
        self.upsert_meeting(
            Some(row.id),
            &m.title,
            &m.started_at,
            m.duration_secs,
            &md,
            &m.search_body(),
        )?;
        self.meeting(row.id)?
            .ok_or_else(|| anyhow::anyhow!("The meeting vanished while saving."))
    }

    // ---- Voice memory (biometric: local only, opt-in) ----

    pub fn voices(&self) -> Result<Vec<Voice>> {
        let mut st = self.conn.prepare(
            "SELECT id, name, embedding, samples FROM voices ORDER BY name COLLATE NOCASE",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(Voice {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    embedding: voices::from_bytes(&r.get::<_, Vec<u8>>(2)?),
                    samples: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Learns (or refines) a person's voice from one meeting's voiceprint.
    pub fn learn_voice(&self, name: &str, embedding: &[f32]) -> Result<()> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "a name is needed");
        let existing = self
            .voices()?
            .into_iter()
            .find(|v| v.name.eq_ignore_ascii_case(name));
        let (merged, samples) = match &existing {
            Some(v) => (
                voices::merge(&v.embedding, v.samples, embedding),
                v.samples + 1,
            ),
            None => (voices::merge(&[], 0, embedding), 1),
        };
        self.conn.execute(
            "INSERT INTO voices (name, embedding, samples, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET embedding = excluded.embedding, samples = excluded.samples,
             updated_at = excluded.updated_at",
            params![name, voices::to_bytes(&merged), samples, Local::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn delete_voice(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM voices WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Forgets every remembered voice and every stored meeting voiceprint.
    pub fn forget_all_voices(&self) -> Result<()> {
        self.conn
            .execute_batch("DELETE FROM voices; DELETE FROM meeting_voiceprints;")?;
        Ok(())
    }

    pub fn set_meeting_voiceprints(
        &self,
        meeting_id: i64,
        prints: &[(String, Vec<f32>)],
    ) -> Result<()> {
        for (label, e) in prints {
            self.conn.execute(
                "INSERT OR REPLACE INTO meeting_voiceprints (meeting_id, label, embedding) VALUES (?1, ?2, ?3)",
                params![meeting_id, label, voices::to_bytes(e)],
            )?;
        }
        Ok(())
    }

    /// When a speaker is renamed: moves their voiceprint to the new label and returns it.
    pub fn rename_meeting_voiceprint(
        &self,
        meeting_id: i64,
        from: &str,
        to: &str,
    ) -> Result<Option<Vec<f32>>> {
        let e: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT embedding FROM meeting_voiceprints WHERE meeting_id = ?1 AND label = ?2",
                params![meeting_id, from],
                |r| r.get(0),
            )
            .optional()?;
        if e.is_some() {
            self.conn.execute(
                "UPDATE OR REPLACE meeting_voiceprints SET label = ?3 WHERE meeting_id = ?1 AND label = ?2",
                params![meeting_id, from, to],
            )?;
        }
        Ok(e.map(|b| voices::from_bytes(&b)))
    }

    // ---- Dictation history ----

    pub fn add_dictation(&self, text: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO dictations (created_at, text) VALUES (?1, ?2)",
            params![Local::now().to_rfc3339(), text],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "DELETE FROM dictations WHERE id NOT IN (SELECT id FROM dictations ORDER BY id DESC LIMIT ?1)",
            [DICTATION_HISTORY],
        )?;
        Ok(id)
    }

    pub fn dictations(&self) -> Result<Vec<Dictation>> {
        let mut st = self
            .conn
            .prepare("SELECT id, created_at, text FROM dictations ORDER BY id DESC")?;
        let rows = st
            .query_map([], |r| {
                Ok(Dictation {
                    id: r.get(0)?,
                    created_at: parse_time(r.get(1)?),
                    text: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn update_dictation(&self, id: i64, text: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE dictations SET text = ?2 WHERE id = ?1",
            params![id, text],
        )?;
        Ok(())
    }

    pub fn clear_dictations(&self) -> Result<()> {
        self.conn.execute("DELETE FROM dictations", [])?;
        Ok(())
    }

    // ---- Vocabulary ----

    pub fn rules(&self) -> Result<Vec<Rule>> {
        let mut st = self.conn.prepare(
            "SELECT id, heard, spelling, mode FROM vocab ORDER BY spelling COLLATE NOCASE",
        )?;
        let rows = st
            .query_map([], |r| {
                let mode: String = r.get(3)?;
                Ok(Rule {
                    id: r.get(0)?,
                    heard: r.get(1)?,
                    spelling: r.get(2)?,
                    mode: if mode == "confirm" {
                        RuleMode::Confirm
                    } else {
                        RuleMode::Auto
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Saves a learned spelling, replacing any rule for the same misheard form.
    /// `mode` defaults to the common-word check (§6.7).
    pub fn save_rule(&self, heard: &str, spelling: &str, mode: Option<RuleMode>) -> Result<Rule> {
        let heard = heard.split_whitespace().collect::<Vec<_>>().join(" ");
        let spelling = spelling.split_whitespace().collect::<Vec<_>>().join(" ");
        anyhow::ensure!(
            !heard.is_empty() && !spelling.is_empty(),
            "both spellings are needed"
        );
        let mode = mode.unwrap_or_else(|| rules::mode_for(&heard));
        let mode_s = match mode {
            RuleMode::Auto => "auto",
            RuleMode::Confirm => "confirm",
        };
        self.conn.execute(
            "INSERT INTO vocab (heard, spelling, mode, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(heard) DO UPDATE SET spelling = excluded.spelling, mode = excluded.mode",
            params![heard, spelling, mode_s, Local::now().to_rfc3339()],
        )?;
        let id = self
            .conn
            .query_row("SELECT id FROM vocab WHERE heard = ?1", [&heard], |r| {
                r.get(0)
            })?;
        Ok(Rule {
            id,
            heard,
            spelling,
            mode,
        })
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM vocab WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn never_ask(&self, heard: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO vocab_never_ask (heard) VALUES (?1)",
            [heard.trim()],
        )?;
        Ok(())
    }

    pub fn is_never_ask(&self, heard: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM vocab_never_ask WHERE heard = ?1",
                [heard.trim()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

/// Turns free text into a safe FTS5 query: each word quoted, the last one as a prefix.
fn fts_query(input: &str) -> String {
    let words: Vec<String> = input
        .split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .collect();
    let n = words.len();
    words
        .into_iter()
        .enumerate()
        .map(|(i, w)| {
            if i + 1 == n {
                format!("\"{w}\"*")
            } else {
                format!("\"{w}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_searches_and_deletes_meetings() {
        let lib = Library::in_memory().unwrap();
        let now = Local::now();
        let id = lib
            .upsert_meeting(
                None,
                "Q4 planning",
                &now,
                600.0,
                Path::new("/x/a.md"),
                "talk about the TikTok budget",
            )
            .unwrap();
        lib.upsert_meeting(
            None,
            "Standup",
            &now,
            60.0,
            Path::new("/x/b.md"),
            "nothing much",
        )
        .unwrap();
        let hits = lib.search("tikt").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].meeting.id, id);
        assert!(hits[0].snippet.contains("[TikTok]"));
        assert_eq!(lib.search("\"q4 plan").unwrap().len(), 1);
        lib.upsert_meeting(
            Some(id),
            "Renamed",
            &now,
            600.0,
            Path::new("/x/c.md"),
            "new body",
        )
        .unwrap();
        assert!(lib.search("tiktok").unwrap().is_empty());
        assert_eq!(lib.meeting(id).unwrap().unwrap().title, "Renamed");
        lib.delete_meeting(id).unwrap();
        assert_eq!(lib.meetings().unwrap().len(), 1);
    }

    #[test]
    fn saves_and_loads_meeting_files_with_notes() {
        let lib = Library::in_memory().unwrap();
        let dir = std::env::temp_dir().join(format!("pt-lib-{}", std::process::id()));
        let mut m = crate::output::sample();
        let md = m.write(&dir, None).unwrap();
        let id = lib
            .upsert_meeting(
                None,
                &m.title,
                &m.started_at,
                m.duration_secs,
                &md,
                &m.search_body(),
            )
            .unwrap();
        let (row, loaded) = lib.load_meeting(id).unwrap();
        assert_eq!(loaded.segments, m.segments);
        m.notes = Some("Decision: move budget to social".into());
        m.title = "Renamed sync".into();
        let row = lib.save_meeting(&row, &m).unwrap();
        assert!(row.md_path.to_string_lossy().contains("renamed-sync"));
        assert!(!md.exists());
        assert_eq!(lib.load_meeting(id).unwrap().1.notes, m.notes);
        assert_eq!(lib.search("budget social").unwrap().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn learns_refines_and_forgets_voices() {
        let lib = Library::in_memory().unwrap();
        lib.set_meeting_voiceprints(4, &[("Speaker 2".into(), vec![1.0, 0.0])])
            .unwrap();
        let e = lib
            .rename_meeting_voiceprint(4, "Speaker 2", "Jordan")
            .unwrap()
            .unwrap();
        lib.learn_voice("Jordan", &e).unwrap();
        lib.learn_voice("jordan", &[0.0, 1.0]).unwrap();
        let v = lib.voices().unwrap();
        assert_eq!((v.len(), v[0].samples), (1, 2));
        assert!((v[0].embedding[0] - v[0].embedding[1]).abs() < 1e-6);
        assert!(lib
            .rename_meeting_voiceprint(4, "Speaker 2", "X")
            .unwrap()
            .is_none());
        lib.forget_all_voices().unwrap();
        assert!(lib.voices().unwrap().is_empty());
    }

    #[test]
    fn audio_retention_expiry() {
        let lib = Library::in_memory().unwrap();
        let now = Local::now();
        let id = lib
            .upsert_meeting(None, "m", &now, 1.0, Path::new("/x/a.md"), "")
            .unwrap();
        lib.set_audio(
            id,
            Some(Path::new("/x/audio")),
            Some(now - chrono::Duration::days(1)),
        )
        .unwrap();
        assert_eq!(
            lib.expired_audio(&now).unwrap(),
            vec![(id, PathBuf::from("/x/audio"))]
        );
        lib.set_audio(id, None, None).unwrap();
        assert!(lib.expired_audio(&now).unwrap().is_empty());
    }

    #[test]
    fn vocabulary_rules_and_never_ask() {
        let lib = Library::in_memory().unwrap();
        let r = lib.save_rule("  stefan ", "Stephan", None).unwrap();
        assert_eq!(r.mode, RuleMode::Auto);
        let r2 = lib.save_rule("STEFAN", "Steffan", None).unwrap();
        assert_eq!(r2.id, r.id);
        assert_eq!(
            lib.save_rule("humble", "Humber", None).unwrap().mode,
            RuleMode::Confirm
        );
        assert_eq!(lib.rules().unwrap().len(), 2);
        lib.delete_rule(r.id).unwrap();
        assert_eq!(lib.rules().unwrap().len(), 1);
        lib.never_ask("Pocket").unwrap();
        assert!(lib.is_never_ask("pocket").unwrap());
        assert!(lib.save_rule("", "x", None).is_err());
    }

    #[test]
    fn keeps_a_short_dictation_history() {
        let lib = Library::in_memory().unwrap();
        for i in 0..25 {
            lib.add_dictation(&format!("d{i}")).unwrap();
        }
        let d = lib.dictations().unwrap();
        assert_eq!(d.len(), 20);
        assert_eq!(d[0].text, "d24");
        lib.update_dictation(d[0].id, "fixed").unwrap();
        assert_eq!(lib.dictations().unwrap()[0].text, "fixed");
    }
}
