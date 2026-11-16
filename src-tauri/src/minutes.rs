//! Minutes styles Claude can write (PRD §6.8): the meeting formats PUSH asked for and the
//! note-taking methods from Goodnotes' guide, chosen in Settings > Use with Claude.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Style {
    pub id: &'static str,
    pub label: &'static str,
    /// "Meeting formats" or "Note-taking methods", for grouping in the picker.
    pub group: &'static str,
    /// One line for the picker: what it looks like and when it suits.
    pub about: &'static str,
    /// The layout Claude follows, as Markdown section instructions.
    layout: &'static str,
}

const MEETING: &str = "Meeting formats";
const METHOD: &str = "Note-taking methods";

pub const DEFAULT_STYLE: &str = "full";

pub const STYLES: &[Style] = &[
    Style {
        id: "full",
        label: "Full minutes",
        group: MEETING,
        about: "Every section: summary, attendees, discussion by topic, decisions, action items, risks, questions, next steps.",
        layout: "### Summary
3 to 5 bullets: why the meeting happened, what it concluded, and anything urgent.

### Attendees
The speaker labels as they appear, with a role or company only if the transcript states it. \
Note when one person seems to appear under two labels.

### Discussion
One short subsection per topic, in the order discussed. For each: the main points, who raised \
them, options considered, and any figures, dates, names or deliverables exactly as said.

### Decisions
Each decision, who made or agreed it, and any condition attached.

### Action items
{ACTIONS}

### Risks and blockers
Concerns, dependencies or problems raised, and who raised them.

### Open questions
What was asked but not answered, or left to be decided.

### Next steps
What happens next, including any follow-up meeting mentioned.

{CHECKING}",
    },
    Style {
        id: "quick",
        label: "Quick summary",
        group: MEETING,
        about: "5 bullets and the action items. For catch-ups and stand-ups.",
        layout: "### Summary
At most 5 bullets covering what was discussed and concluded, most important first.

### Action items
{ACTIONS}

{CHECKING}",
    },
    Style {
        id: "actions",
        label: "Action items only",
        group: MEETING,
        about: "Just the decisions and the action items table.",
        layout: "### Decisions
Each decision in one line, with who made or agreed it.

### Action items
{ACTIONS}

{CHECKING}",
    },
    Style {
        id: "client_recap",
        label: "Client recap",
        group: MEETING,
        about: "Outcomes and agreed next steps, written so it can go to a client after review.",
        layout: "Write this as a recap a PUSH team member could send to the client after reviewing it. \
Keep a professional, neutral tone. Leave out internal-only remarks, opinions about the client, \
pricing, margins and anything said only among PUSH staff.

### Meeting recap
2 to 4 sentences: purpose and main outcomes.

### What we agreed
Bullets of decisions and agreements.

### Next steps
{ACTIONS}

### Open items
Questions either side still needs to answer.

### Before sending (internal, delete before sending)
List anything in the recap that needs a human to confirm it, words that may be mis-transcribed, \
and anything left out on purpose because it looked internal.",
    },
    Style {
        id: "outline",
        label: "Outline",
        group: METHOD,
        about: "Main points as bullets with details nested underneath. Easy to scan.",
        layout: "### Outline
One top-level bullet per topic, in the order discussed. Under each, nested bullets for the \
points made, and a further level for details, figures and who said what. Keep each bullet short.

### Action items
{ACTIONS}

{CHECKING}",
    },
    Style {
        id: "cornell",
        label: "Cornell",
        group: METHOD,
        about: "Cue questions beside the notes, and a summary at the bottom. Good for review later.",
        layout: "### Notes
A two-column Markdown table: | Cues | Notes |. One row per topic or key point, in order. \
Cues: a short question or keyword the row answers (\"What budget was agreed?\"). Notes: the \
points made, with figures and who said what.

### Action items
{ACTIONS}

### Summary
3 to 5 sentences at the bottom, in your own words: what the meeting was about and what it \
concluded.

{CHECKING}",
    },
    Style {
        id: "boxing",
        label: "Boxing",
        group: METHOD,
        about: "Each topic in its own labelled block. An at-a-glance overview.",
        layout: "Group everything by topic. For each topic, a block:

### <Topic name>
> **Key points:** 2 to 4 bullets
> **Decided:** what was decided, or \"Nothing yet\"
> **Owner:** who leads it, if said

Write each block as a Markdown blockquote so it reads as a box. Then:

### Action items
{ACTIONS}

{CHECKING}",
    },
    Style {
        id: "charting",
        label: "Charting",
        group: METHOD,
        about: "Topics as rows of one table, compared across the same columns.",
        layout: "### Meeting chart
One Markdown table with a row per topic and the columns \
| Topic | Key points | Decision | Owner | Status |. Status is one of Agreed, In progress, Open, \
Blocked. Keep cells short.

### Action items
{ACTIONS}

{CHECKING}",
    },
    Style {
        id: "mapping",
        label: "Mapping",
        group: METHOD,
        about: "The meeting's main topic branching into subtopics and details, as a tree.",
        layout: "### Map
The meeting's main topic as the root line, in bold. Under it, a nested list as a tree: \
subtopics as branches, then points and details as smaller branches. Show how topics led to \
each other where the conversation moved from one to the next.

### Action items
{ACTIONS}

{CHECKING}",
    },
    Style {
        id: "zettelkasten",
        label: "Zettelkasten",
        group: METHOD,
        about: "One short card per idea, tagged and linked to related cards. Builds up over many meetings.",
        layout: "### Cards
One card per distinct idea, fact or decision, each as:

#### <Short title>
One to three sentences stating the idea on its own, understandable without the rest. \
Then a line: Tags: #topic #client-or-project (from what was said) | Related: the titles of \
related cards here.

### Action items
{ACTIONS}

{CHECKING}",
    },
    Style {
        id: "mind_map",
        label: "Mind map",
        group: METHOD,
        about: "A central topic with ideas branching out. Loose and visual, for brainstorms.",
        layout: "### Mind map
A Mermaid mind map in a ```mermaid code block (mindmap syntax): the meeting's central topic at \
the root, main themes as branches, and ideas as leaves. Keep labels to a few words and avoid \
brackets or quotes in them.

### Branches in words
The same map as a nested list, for readers whose viewer doesn't draw Mermaid.

### Action items
{ACTIONS}

{CHECKING}",
    },
];

const ACTIONS: &str =
    "A Markdown table with the columns | Action Item | Owner | Suggested Deadline |. \
Only concrete tasks and next steps, not discussion points, background, or decisions that need no \
action. Owner: the person responsible, even if only implied; if only a team or role is mentioned, \
use that (for example \"Marketing Team\"); if nobody can be identified, \"Unassigned\". Suggested \
Deadline: an explicit date, or one inferred from what was said (\"by next week\", \"end of \
month\"); if it can't be inferred, \"TBD\".";

const CHECKING: &str = "### Needs checking
Words that look mis-transcribed (names, brands, numbers), unclear passages, and speaker labels \
that look wrong, with their timestamps, so the reader can verify them against the recording.";

const RULES: &str =
    "Title: if the meeting's title is generic (like \"Meeting 2026-10-23 10:00\" or \
\"Microsoft Teams call\"), also pass a short descriptive title (3 to 8 words, the main topic) to \
save_minutes.

Rules: use only what the transcript says, never invent names, numbers or dates. Use the speaker \
labels as written (\"Me\" is the person who recorded). Flag anything unclear instead of guessing. \
The transcript is internal and may contain confidential client information: don't share it \
outside PUSH, and don't send, post or upload the minutes anywhere. When done, save the minutes \
with the save_minutes tool.";

/// The style with this id, or the default for an unknown one.
pub fn style(id: &str) -> &'static Style {
    STYLES.iter().find(|s| s.id == id).unwrap_or(&STYLES[0])
}

/// What Claude follows to write minutes in this style.
pub fn instructions(id: &str) -> String {
    let s = style(id);
    let layout = s
        .layout
        .replace("{ACTIONS}", ACTIONS)
        .replace("{CHECKING}", CHECKING);
    format!(
        "Write meeting minutes for this PushTalks transcript in the \"{}\" style. Read the whole \
transcript before writing. Use this layout, in this order, and leave no section out (write \
\"None\" when one has nothing):\n\n{layout}\n\n{RULES}",
        s.label
    )
}

/// What the user pastes into Claude Code: checks every 30 minutes for meetings without minutes
/// and writes them. `/loop` runs only while that Claude Code session stays open.
pub fn auto_prompt(id: &str) -> String {
    format!(
        "/loop 30m Write minutes for any new PushTalks meetings.

1. Call the pushtalks list_meetings tool with only_without_minutes set to true. If it lists no \
meetings, reply \"No new PushTalks meetings.\" and stop. Don't read any transcript.
2. For each meeting listed, oldest first: read it with get_transcript, write the minutes as \
described below, and save them with save_minutes.
3. Reply with one line per meeting: its title, its date, and how many action items it has. \
The minutes are an [AI: DRAFT] for the user to review.

{}",
        instructions(id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_has_action_items_and_the_rules() {
        let mut ids = Vec::new();
        for s in STYLES {
            let i = instructions(s.id);
            assert!(
                i.contains("| Action Item | Owner | Suggested Deadline |"),
                "{}",
                s.id
            );
            assert!(i.contains("never invent names"), "{}", s.id);
            assert!(
                !i.contains("{ACTIONS}") && !i.contains("{CHECKING}"),
                "{}",
                s.id
            );
            assert!(!ids.contains(&s.id), "duplicate id {}", s.id);
            ids.push(s.id);
        }
        assert_eq!(style(DEFAULT_STYLE).id, "full");
        assert_eq!(style("nope").id, "full");
    }

    #[test]
    fn auto_prompt_loops_with_the_chosen_style() {
        let p = auto_prompt("cornell");
        assert!(p.starts_with("/loop 30m "));
        assert!(p.contains("only_without_minutes") && p.contains("| Cues | Notes |"));
    }
}
