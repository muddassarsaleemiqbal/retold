//! Home: a getting-started checklist until the installation can make a video, then what to
//! continue, and quick ways to start. Every row is derived from saved state on each frame.
use serde_json::Value;

use super::section::Section;
use super::store::{Store, Want};
use crate::human::text;

/// What choosing a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Open the action at this command path.
    Action(&'static str),
    Section(Section),
    Video(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub group: &'static str,
    pub label: String,
    pub detail: String,
    /// `Some(done)` for checklist steps.
    pub done: Option<bool>,
    pub target: Target,
}

const HOME_READ: &str = "home";
const WORKSPACE_READ: &str = "workspace show";
const PROVIDERS_READ: &str = "providers list";
const ROUTES_READ: &str = "providers route";
const ACCOUNT_READ: &str = "account show";
const GUIDES_READ: &str = "guides list";
const CHANNELS_READ: &str = "channels list";
const VIDEOS_READ: &str = "video list";

#[must_use]
pub fn wants() -> Vec<Want> {
    let once = [
        WORKSPACE_READ,
        PROVIDERS_READ,
        ROUTES_READ,
        ACCOUNT_READ,
        GUIDES_READ,
        CHANNELS_READ,
        VIDEOS_READ,
    ];
    let mut wants = vec![Want::live(HOME_READ)];
    wants.extend(once.into_iter().map(Want::once));
    wants
}

/// How a checklist step knows it is done.
#[derive(Clone, Copy)]
enum Done {
    /// The active workspace is registered.
    Registered,
    /// The read answered at all, as `account show` does once the account exists.
    Answers(&'static str),
    /// The read lists something under this key.
    Lists(&'static str, &'static str),
}

struct Step {
    label: &'static str,
    detail: &'static str,
    path: &'static str,
    done: Done,
}

const STEPS: [Step; 7] = [
    Step {
        label: "Create a workspace",
        detail: "Where videos, music and settings live",
        path: "workspace init",
        done: Done::Registered,
    },
    Step {
        label: "Add a provider",
        detail: "Claude, Codex, Gemini, OpenRouter or a local model",
        path: "providers add",
        done: Done::Lists(PROVIDERS_READ, "providers"),
    },
    Step {
        label: "Route tasks to providers",
        detail: "Which provider writes, draws, speaks and listens",
        path: "providers route",
        done: Done::Lists(ROUTES_READ, "routing"),
    },
    Step {
        label: "Set up spending",
        detail: "Create the account and its limits; starts at $0",
        path: "account init",
        done: Done::Answers(ACCOUNT_READ),
    },
    Step {
        label: "Create a guide",
        detail: "The kind of video you make and how it is written",
        path: "guides create",
        done: Done::Lists(GUIDES_READ, "items"),
    },
    Step {
        label: "Create a channel",
        detail: "Your brand: name, voice, colours and fonts",
        path: "channels create",
        done: Done::Lists(CHANNELS_READ, "items"),
    },
    Step {
        label: "Create your first video",
        detail: "Pick a guide and a story; nothing is spent yet",
        path: "new",
        done: Done::Lists(VIDEOS_READ, "items"),
    },
];

fn contents<'a>(store: &'a Store, command: &str) -> Option<&'a Value> {
    store.contents(&super::store::words(command))
}

fn is_done(store: &Store, done: Done) -> bool {
    match done {
        Done::Registered => contents(store, WORKSPACE_READ)
            .is_some_and(|found| found["workspace"]["registered"].as_bool() == Some(true)),
        Done::Answers(command) => contents(store, command).is_some(),
        Done::Lists(command, key) => {
            contents(store, command).is_some_and(|found| non_empty(&found[key]))
        }
    }
}

fn non_empty(value: &Value) -> bool {
    value.as_array().is_some_and(|list| !list.is_empty())
        || value.as_object().is_some_and(|map| !map.is_empty())
}

/// The group the getting-started steps belong to.
pub const CHECKLIST: &str = "Get started";

/// A group heading takes a blank line and the heading line.
const HEADING_LINES: usize = 2;

/// Whether row `index` is the first of its group, and so gets a heading.
#[must_use]
pub fn starts_group(rows: &[Row], index: usize) -> bool {
    let previous = index.checked_sub(1).and_then(|before| rows.get(before));
    let current = rows.get(index);
    match (previous, current) {
        (Some(before), Some(row)) => before.group != row.group,
        _ => true,
    }
}

/// The line row `index` is drawn on once headings are counted.
#[must_use]
pub fn line_of(rows: &[Row], index: usize) -> usize {
    let headings = (0..=index).filter(|row| starts_group(rows, *row)).count();
    index.saturating_add(headings.saturating_mul(HEADING_LINES))
}

/// The row drawn on `line`, if `line` is a row and not a heading.
#[must_use]
pub fn row_at(rows: &[Row], line: usize) -> Option<usize> {
    (0..rows.len()).find(|index| line_of(rows, *index) == line)
}

/// `(done, total)` for the checklist header.
#[must_use]
pub fn progress(store: &Store) -> (usize, usize) {
    let done = STEPS
        .iter()
        .filter(|step| is_done(store, step.done))
        .count();
    (done, STEPS.len())
}

#[must_use]
pub fn rows(store: &Store) -> Vec<Row> {
    let (done, total) = progress(store);
    let unfinished = (done < total).then(|| STEPS.iter().map(|step| checklist_row(store, step)));
    unfinished
        .into_iter()
        .flatten()
        .chain(recent(store))
        .chain(quick())
        .collect()
}

fn checklist_row(store: &Store, step: &Step) -> Row {
    Row {
        group: CHECKLIST,
        label: step.label.to_owned(),
        detail: step.detail.to_owned(),
        done: Some(is_done(store, step.done)),
        target: Target::Action(step.path),
    }
}

const RECENT_VIDEOS: usize = 6;

fn recent(store: &Store) -> Vec<Row> {
    let Some(listed) = contents(store, VIDEOS_READ) else {
        return Vec::new();
    };
    listed["items"]
        .as_array()
        .map(|items| items.iter().take(RECENT_VIDEOS).map(video).collect())
        .unwrap_or_default()
}

fn video(item: &Value) -> Row {
    let reference = text(item, "ref");
    Row {
        group: "Continue",
        label: text(item, "title"),
        detail: reference.clone(),
        done: None,
        target: Target::Video(reference),
    }
}

const QUICK_ACTIONS: [(&str, &str, Target); 5] = [
    (
        "New video",
        "From a guide, a title and the languages you want",
        Target::Action("new"),
    ),
    (
        "New series",
        "Several parts that share research and cast",
        Target::Action("series new"),
    ),
    (
        "Import music",
        "Add a background track to your library",
        Target::Action("library music import"),
    ),
    (
        "Check this installation",
        "Doctor finds problems and how to fix them",
        Target::Action("doctor"),
    ),
    (
        "Spend and limits",
        "What was spent and what may still be spent",
        Target::Section(Section::Money),
    ),
];

fn quick() -> Vec<Row> {
    QUICK_ACTIONS
        .into_iter()
        .map(|(label, detail, target)| Row {
            group: "Start",
            label: label.to_owned(),
            detail: detail.to_owned(),
            done: None,
            target,
        })
        .collect()
}

/// Provider health for the header, such as `codex ✓  claude ✗`.
#[must_use]
pub fn health(store: &Store) -> Vec<(String, &'static str)> {
    let Some(summary) = contents(store, HOME_READ) else {
        return Vec::new();
    };
    summary["home"]["providers"]["data"]["health"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| (text(entry, "id"), status(entry)))
                .collect()
        })
        .unwrap_or_default()
}

fn status(entry: &Value) -> &'static str {
    match entry["status"].as_str() {
        Some("ok") => "ok",
        Some("limited") => "limited",
        Some("logged_out") => "logged out",
        _ => "unknown",
    }
}

/// Today's spend for the header, when the account exists.
#[must_use]
pub fn today(store: &Store) -> Option<String> {
    let summary = contents(store, HOME_READ)?;
    let total = &summary["home"]["today"]["data"]["spend"]["total"];
    let charged = total["resources"]["usd_micros"]["charged"].as_u64()?;
    Some(crate::human::usd(charged))
}

/// The active workspace name for the header.
#[must_use]
pub fn workspace(store: &Store) -> Option<String> {
    let found = contents(store, WORKSPACE_READ)?;
    let name = text(&found["workspace"], "name");
    (name != "unknown").then_some(name)
}
