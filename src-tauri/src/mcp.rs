//! The PushTalks connector for Claude: a local MCP server over stdin/stdout (no network).
//! Claude Code or Claude Desktop can list meetings, read a transcript and save minutes into the
//! meeting's Notes section. Claude sees a transcript only when the user asks it to.

use std::fs;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::db::Library;
use crate::output::AI_TAG;

const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const MINUTES_INSTRUCTIONS: &str = "Write full meeting minutes for this PushTalks transcript.
Read the whole transcript before writing. Use these sections, in this order, and leave none out \
(write \"None\" when a section has nothing):

### Summary
3 to 5 bullets: why the meeting happened, what it concluded, and anything urgent.

### Attendees
The speaker labels as they appear (\"Me\" is the person who recorded), with a role or company \
only if the transcript states it. Note when one person seems to appear under two labels.

### Discussion
One short subsection per topic, in the order discussed. For each: the main points, who raised \
them, options considered, and any figures, dates, names or deliverables exactly as said.

### Decisions
Each decision, who made or agreed it, and any condition attached.

### Action items
A Markdown table with the columns | Action Item | Owner | Suggested Deadline |. Only concrete \
tasks and next steps, not discussion points, background, or decisions that need no action. \
Owner: the person responsible, even if only implied; if only a team or role is mentioned, use \
that (for example \"Marketing Team\"); if nobody can be identified, \"Unassigned\". Suggested \
Deadline: an explicit date, or one inferred from what was said (\"by next week\", \"end of \
month\"); if it can't be inferred, \"TBD\".

### Risks and blockers
Concerns, dependencies or problems raised, and who raised them.

### Open questions
What was asked but not answered, or left to be decided.

### Next steps
What happens next, including any follow-up meeting mentioned.

### Needs checking
Words that look mis-transcribed (names, brands, numbers), unclear passages, and speaker labels \
that look wrong, with their timestamps, so the reader can verify them against the recording.

Title: if the meeting's title is generic (like \"Meeting 2026-10-23 10:00\" or \"Microsoft Teams \
call\"), also pass a short descriptive title (3 to 8 words, the main topic) to save_minutes.

Rules: use only what the transcript says, never invent names, numbers or dates. Use the speaker \
labels as written. Flag anything unclear instead of guessing. The transcript is internal and may \
contain confidential client information: don't share it outside PUSH, and don't send, post or \
upload the minutes anywhere. When done, save the minutes with the save_minutes tool.";

/// What the user pastes into Claude Code: checks every 30 minutes for meetings without minutes
/// and writes them. `/loop` runs only while that Claude Code session stays open.
pub fn auto_minutes_prompt() -> String {
    format!(
        "/loop 30m Write minutes for any new PushTalks meetings.

1. Call the pushtalks list_meetings tool with only_without_minutes set to true. If it lists no \
meetings, reply \"No new PushTalks meetings.\" and stop. Don't read any transcript.
2. For each meeting listed, oldest first: read it with get_transcript, write the minutes as \
described below, and save them with save_minutes.
3. Reply with one line per meeting: its title, its date, and how many action items it has. \
The minutes are an [AI: DRAFT] for the user to review.

{MINUTES_INSTRUCTIONS}"
    )
}

pub struct Server {
    lib: Library,
}

fn text(s: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": s.into() }] })
}

fn tool_error(s: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": s.into() }], "isError": true })
}

fn meeting_id(args: &Value) -> Result<i64> {
    args.get("meeting_id")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("meeting_id is required (a number from list_meetings)"))
}

/// Adds the [AI: DRAFT] tag (PUSH's AI intent tag) unless the minutes already carry one.
pub fn tag_minutes(minutes: &str) -> String {
    let m = minutes.trim();
    if m.contains("[AI: ") {
        m.to_string()
    } else {
        format!("*{AI_TAG} Written by Claude from the transcript. Review before sharing.*\n\n{m}")
    }
}

impl Server {
    pub fn new(lib: Library) -> Self {
        Self { lib }
    }

    /// Handles one JSON-RPC message. Returns the response, or None for notifications.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id")?.clone();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tools()),
            "tools/call" => Ok(self.call(&params)),
            "prompts/list" => Ok(prompts()),
            "prompts/get" => self.prompt(&params),
            _ => Err((-32601, format!("unknown method {method}"))),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {}, "prompts": {} },
            "serverInfo": { "name": "pushtalks", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "PushTalks meeting transcripts recorded on this computer. Use list_meetings to find a meeting, get_transcript to read it, and save_minutes to store minutes and a summary in it. Transcripts are internal PUSH records and may contain confidential client information.",
        })
    }

    fn call(&mut self, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        let r = match name {
            "list_meetings" => self.list(&args),
            "get_transcript" => self.transcript(&args),
            "save_minutes" => self.save_minutes(&args),
            _ => Err(anyhow!("unknown tool {name}")),
        };
        r.unwrap_or_else(|e| tool_error(e.to_string()))
    }

    fn list(&self, args: &Value) -> Result<Value> {
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 200) as usize;
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let only_new = args
            .get("only_without_minutes")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let rows = if query.is_empty() {
            self.lib.meetings()?
        } else {
            self.lib
                .search(query)?
                .into_iter()
                .map(|h| h.meeting)
                .collect()
        };
        let lines: Vec<String> = rows
            .iter()
            .filter_map(|r| {
                let notes = self
                    .lib
                    .load_meeting(r.id)
                    .map(|(_, m)| {
                        if m.notes.is_some() {
                            "has minutes"
                        } else {
                            "no minutes yet"
                        }
                    })
                    .unwrap_or("transcript file missing");
                if only_new && notes != "no minutes yet" {
                    return None;
                }
                Some(format!(
                    "meeting_id {} | {} | {} min | {} | {}",
                    r.id,
                    r.started_at.format("%Y-%m-%d %H:%M"),
                    (r.duration_secs / 60.0).round(),
                    r.title,
                    notes
                ))
            })
            .take(limit)
            .collect();
        if lines.is_empty() {
            return Ok(text(if only_new {
                "No meetings without minutes."
            } else {
                "No meetings found."
            }));
        }
        Ok(text(lines.join("\n")))
    }

    fn transcript(&self, args: &Value) -> Result<Value> {
        let (row, _) = self.lib.load_meeting(meeting_id(args)?)?;
        Ok(text(fs::read_to_string(&row.md_path)?))
    }

    fn save_minutes(&mut self, args: &Value) -> Result<Value> {
        let id = meeting_id(args)?;
        let minutes = args
            .get("minutes")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .ok_or_else(|| anyhow!("minutes (Markdown text) is required"))?;
        let (row, mut m) = self.lib.load_meeting(id)?;
        let replaced = m.notes.is_some();
        m.notes = Some(tag_minutes(minutes));
        // A suggested title only replaces a generic one, never a name the user or app chose.
        let new_title = args
            .get("title")
            .and_then(Value::as_str)
            .map(|t| t.trim().trim_matches('"').to_string())
            .filter(|t| !t.is_empty() && t.chars().count() <= 80);
        let renamed = match new_title {
            Some(t) if crate::output::is_generic_title(&m.title) => {
                m.title = t;
                true
            }
            _ => false,
        };
        let row = self.lib.save_meeting(&row, &m)?;
        let rename_note = if renamed {
            format!(" Renamed the meeting to \"{}\".", m.title)
        } else {
            String::new()
        };
        Ok(text(format!(
            "{} the minutes in the Notes section of {}.{rename_note} The user can also see them in PushTalks > Library.",
            if replaced { "Replaced" } else { "Saved" },
            row.md_path.display()
        )))
    }

    fn prompt(&self, params: &Value) -> Result<Value, (i32, String)> {
        if params.get("name").and_then(Value::as_str) != Some("meeting_minutes") {
            return Err((-32602, "unknown prompt".into()));
        }
        let which = params
            .pointer("/arguments/meeting_id")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .filter(|s| !s.trim().is_empty());
        let target = match which {
            Some(id) => format!("Use meeting_id {id}: read it with get_transcript."),
            None => "Call list_meetings, pick the most recent meeting without minutes (ask the user if it's unclear), and read it with get_transcript.".into(),
        };
        Ok(json!({
            "description": "Minutes and a summary for a PushTalks meeting",
            "messages": [{
                "role": "user",
                "content": { "type": "text", "text": format!("{target}\n\n{MINUTES_INSTRUCTIONS} Then show the minutes to the user.") }
            }]
        }))
    }
}

fn tools() -> Value {
    json!({ "tools": [
        {
            "name": "list_meetings",
            "description": "List PushTalks meeting transcripts on this computer, newest first, with their meeting_id and whether minutes exist. Optional full-text search.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to search for in titles and transcripts" },
                    "limit": { "type": "integer", "description": "Maximum meetings to list (default 20)" },
                    "only_without_minutes": { "type": "boolean", "description": "List only meetings that have no minutes yet" }
                }
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_transcript",
            "description": "Read one meeting's full transcript (Markdown with speakers and timestamps). Internal PUSH record; may contain confidential client information.",
            "inputSchema": {
                "type": "object",
                "properties": { "meeting_id": { "type": "integer" } },
                "required": ["meeting_id"]
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "save_minutes",
            "description": "Save meeting minutes and a summary (Markdown) into the meeting's Notes section, replacing earlier minutes. They're tagged [AI: DRAFT] for review.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "meeting_id": { "type": "integer" },
                    "minutes": { "type": "string", "description": "The minutes in Markdown, starting with ### headings" },
                    "title": { "type": "string", "description": "Optional short descriptive title (3 to 8 words). Used only if the meeting's current title is generic." }
                },
                "required": ["meeting_id", "minutes"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false }
        }
    ]})
}

fn prompts() -> Value {
    json!({ "prompts": [{
        "name": "meeting_minutes",
        "description": "Write minutes and a summary for a PushTalks meeting and save them to it",
        "arguments": [{ "name": "meeting_id", "description": "Leave empty for the latest meeting without minutes", "required": false }]
    }]})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_with_meeting() -> (Server, i64, std::path::PathBuf) {
        let lib = Library::in_memory().unwrap();
        let dir =
            std::env::temp_dir().join(format!("pt-mcp-{}-{}", std::process::id(), rand_suffix()));
        let m = crate::output::sample();
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
        (Server::new(lib), id, dir)
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn call(s: &mut Server, name: &str, args: Value) -> Value {
        let r = s
            .handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}))
            .unwrap();
        r["result"].clone()
    }

    #[test]
    fn initialize_lists_and_ignores_notifications() {
        let (mut s, _, dir) = server_with_meeting();
        let r = s
            .handle(&json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}))
            .unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert!(s
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none());
        let tools = s
            .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .unwrap();
        assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 3);
        let bad = s
            .handle(&json!({"jsonrpc":"2.0","id":3,"method":"nope"}))
            .unwrap();
        assert_eq!(bad["error"]["code"], -32601);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_read_and_save_minutes() {
        let (mut s, id, dir) = server_with_meeting();
        let list = call(&mut s, "list_meetings", json!({}));
        let t = list["content"][0]["text"].as_str().unwrap();
        assert!(t.contains(&format!("meeting_id {id}")) && t.contains("no minutes yet"));
        let new = call(
            &mut s,
            "list_meetings",
            json!({"only_without_minutes": true}),
        );
        assert!(new["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(&format!("meeting_id {id}")));
        let tr = call(&mut s, "get_transcript", json!({"meeting_id": id}));
        assert!(tr["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Quick update on TikTok."));
        let saved = call(
            &mut s,
            "save_minutes",
            json!({"meeting_id": id, "minutes": "### Summary\n- Q4 plan agreed"}),
        );
        assert!(saved.get("isError").is_none(), "{saved}");
        let tr = call(&mut s, "get_transcript", json!({"meeting_id": id}));
        let md = tr["content"][0]["text"].as_str().unwrap();
        assert!(md.contains("## Notes\n\n*[AI: DRAFT] Written by Claude"));
        assert!(md.contains("### Summary\n- Q4 plan agreed\n\n## Transcript"));
        let found = call(&mut s, "list_meetings", json!({"query": "agreed"}));
        assert!(found["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("has minutes"));
        let new = call(
            &mut s,
            "list_meetings",
            json!({"only_without_minutes": true}),
        );
        assert_eq!(
            new["content"][0]["text"].as_str(),
            Some("No meetings without minutes.")
        );
        // The sample's title isn't generic, so a suggested title is ignored.
        let kept = call(
            &mut s,
            "save_minutes",
            json!({"meeting_id": id, "minutes": "x", "title": "New name"}),
        );
        assert!(!kept["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Renamed"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn auto_prompt_loops_and_carries_the_format() {
        let p = auto_minutes_prompt();
        assert!(p.starts_with("/loop 30m "));
        assert!(p.contains("only_without_minutes") && p.contains("| Action Item | Owner |"));
        assert!(p.contains("### Needs checking"));
    }

    #[test]
    fn replaces_only_a_generic_title() {
        let lib = Library::in_memory().unwrap();
        let dir = std::env::temp_dir().join(format!("pt-mcp-title-{}", rand_suffix()));
        let mut m = crate::output::sample();
        m.title = crate::output::default_title(&m.started_at);
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
        let mut s = Server::new(lib);
        let r = call(
            &mut s,
            "save_minutes",
            json!({"meeting_id": id, "minutes": "x", "title": "TikTok budget review"}),
        );
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Renamed the meeting to \"TikTok budget review\""));
        let list = call(&mut s, "list_meetings", json!({}));
        assert!(list["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("TikTok budget review"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn errors_come_back_as_tool_errors() {
        let (mut s, id, dir) = server_with_meeting();
        assert_eq!(call(&mut s, "get_transcript", json!({}))["isError"], true);
        assert_eq!(
            call(
                &mut s,
                "save_minutes",
                json!({"meeting_id": id, "minutes": "  "})
            )["isError"],
            true
        );
        assert_eq!(
            call(&mut s, "get_transcript", json!({"meeting_id": 999}))["isError"],
            true
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn minutes_prompt_and_tagging() {
        let (mut s, _, dir) = server_with_meeting();
        let p = s
            .handle(&json!({"jsonrpc":"2.0","id":4,"method":"prompts/get","params":{"name":"meeting_minutes","arguments":{"meeting_id":"7"}}}))
            .unwrap();
        assert!(p["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .starts_with("Use meeting_id 7"));
        assert_eq!(tag_minutes("**[AI: FINAL]** done"), "**[AI: FINAL]** done");
        fs::remove_dir_all(dir).unwrap();
    }
}
