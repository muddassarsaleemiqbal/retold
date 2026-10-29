//! Pages open on top of a section and close with Esc; modals float over everything.
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use retold_app::protocol::Payload;

use super::catalog::Risk;
use super::form::Form;
use super::input::Input;

#[derive(Debug, Clone)]
pub enum Page {
    Form(Form),
    /// The result of a read: cached and refreshed like any screen.
    Reading(Reading),
    /// The result of a change the person confirmed.
    Outcome(Outcome),
    Video(VideoPage),
}

impl Page {
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::Form(form) => form.action.label(),
            Self::Reading(reading) => reading.title.clone(),
            Self::Outcome(outcome) => outcome.form.action.label(),
            Self::Video(video) => video.reference.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reading {
    pub title: String,
    pub words: Vec<String>,
    /// The form it came from, for `e` to change the options.
    pub form: Option<Form>,
    pub scroll: u16,
    pub stream: Option<super::stream::Stream>,
}

impl Reading {
    pub fn provider(id: &str) -> Self {
        Self {
            title: format!("Provider status · {id}"),
            words: vec![
                "providers".into(),
                "login".into(),
                "--status".into(),
                id.into(),
            ],
            form: None,
            scroll: 0,
            stream: None,
        }
    }

    pub fn document_subject(&self) -> Option<(&str, &str)> {
        match self.words.as_slice() {
            [kind, command, id]
                if ["guides", "channels"].contains(&kind.as_str()) && command == "document" =>
            {
                Some((kind, id))
            }
            _ => None,
        }
    }
    pub fn series_id(&self) -> Option<&str> {
        match self.words.as_slice() {
            [group, command, id] if group == "series" && command == "show" => Some(id),
            _ => None,
        }
    }

    pub fn provider_id(&self) -> Option<&str> {
        match self.words.as_slice() {
            [group, command, flag, id]
                if group == "providers" && command == "login" && flag == "--status" =>
            {
                Some(id)
            }
            _ => None,
        }
    }

    /// The job being followed, for `queue attach` and the pause, resume and stop controls.
    #[must_use]
    pub fn job_id(&self) -> String {
        self.form
            .as_ref()
            .and_then(|form| form.value("id"))
            .unwrap_or_default()
    }

    /// The last event sequence kept, or zero when this read is not a live stream.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.stream.as_ref().map_or(0, |stream| stream.sequence)
    }

    /// Keeps new events and asks for only the ones after them next time.
    pub fn follow_stream(&mut self, contents: &serde_json::Value) {
        let before = self.sequence();
        if let Some(stream) = &mut self.stream {
            stream.accept(contents);
        }
        let after = self.sequence();
        if after != before {
            let mut next = super::store::words("queue attach");
            next.extend([self.job_id(), "--after-sequence".into(), after.to_string()]);
            self.words = next;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub id: u64,
    pub form: Form,
    pub words: Vec<String>,
    /// `None` while running.
    pub payload: Option<Payload>,
    pub started: u64,
    pub scroll: u16,
}

/// One tab of the video workspace and the read command behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoTab {
    pub title: &'static str,
    /// Run with the video reference appended.
    pub command: &'static str,
}

/// The Plan tab comes first and gets the richest view.
pub const PLAN_TAB: usize = 0;

pub const VIDEO_TABS: [VideoTab; 8] = [
    VideoTab {
        title: "Plan",
        command: "video plan",
    },
    VideoTab {
        title: "Evidence",
        command: "video show",
    },
    VideoTab {
        title: "Review",
        command: "video review",
    },
    VideoTab {
        title: "Costs",
        command: "video costs",
    },
    VideoTab {
        title: "History",
        command: "video history",
    },
    VideoTab {
        title: "Script & Shots",
        command: "video show",
    },
    VideoTab {
        title: "Files",
        command: "video paths",
    },
    VideoTab {
        title: "Deliver",
        command: "video paths --kind delivery",
    },
];

impl VideoPage {
    #[must_use]
    pub fn new(reference: &str) -> Self {
        Self {
            reference: reference.to_owned(),
            tab: PLAN_TAB,
            scroll: 0,
            selected: 0,
            filter: super::review_filter::Filter::default(),
            comparison: None,
            language: None,
        }
    }

    /// Next (positive) or previous tab, wrapping around.
    pub const fn step_tab(&mut self, step: isize) {
        let count = VIDEO_TABS.len().cast_signed();
        let current = self.tab.cast_signed();
        self.tab = current
            .saturating_add(step)
            .rem_euclid(count)
            .cast_unsigned();
        self.scroll = 0;
    }

    #[must_use]
    pub fn current(&self) -> VideoTab {
        VIDEO_TABS
            .get(self.tab)
            .copied()
            .unwrap_or(VIDEO_TABS[PLAN_TAB])
    }

    /// The read behind the current tab, for this video.
    #[must_use]
    pub fn words(&self) -> Vec<String> {
        let mut words = super::store::words(self.current().command);
        words.push(self.reference.clone());
        words
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPage {
    pub reference: String,
    pub filter: super::review_filter::Filter,
    pub comparison: Option<String>,
    pub language: Option<String>,
    pub tab: usize,
    pub scroll: u16,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub enum Modal {
    Palette(Palette),
    Confirm(Form),
    Batch(Batch),
    BulkTag(Input),
    Preview(super::preview::Preview),
    Document(super::document::Editor),
    Plan(super::production::Review),
    Browser(super::files::Browser),
    Values(Palette),
    Setup(usize),
    Route(super::routes::Editor),
    Destroy { form: Form, typed: Input },
    Menu(Menu),
    Help,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Value {
        field: String,
        value: String,
    },
    Section(super::section::Section),
    /// Index into the catalog, with values to fill in.
    Action {
        index: usize,
        fill: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub detail: String,
    pub risk: Option<Risk>,
    pub choice: Choice,
}

#[derive(Debug, Clone)]
pub struct Palette {
    pub query: Input,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

impl Palette {
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            query: Input::default(),
            entries,
            selected: 0,
        }
    }

    /// Arrows move through matches; anything else edits the query.
    pub fn press(&mut self, key: KeyEvent) {
        let count = self.matches().len();
        let moving = matches!(
            key.code,
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
        );
        if moving {
            self.selected = super::landing::moved(self.selected, count, key.code);
        } else {
            self.query.edit(key);
            self.selected = 0;
        }
    }

    #[must_use]
    pub fn choice(&self) -> Option<Choice> {
        let matches = self.matches();
        matches.get(self.selected).map(|entry| entry.choice.clone())
    }

    #[must_use]
    pub fn matches(&self) -> Vec<&Entry> {
        let query = self.query.text().to_lowercase();
        let mut scored: Vec<(usize, &Entry)> = self
            .entries
            .iter()
            .filter_map(|entry| score(&query, entry).map(|value| (value, entry)))
            .collect();
        scored.sort_by_key(|(value, _)| *value);
        scored.into_iter().map(|(_, entry)| entry).collect()
    }
}

/// Score bands: lower is better. Whole words in the label beat words in the description, which
/// beat letters in order within the label. Descriptions never match letter by letter, so short
/// queries stay precise.
const LABEL_WORDS: usize = 0;
const DETAIL_WORDS: usize = 1_000;
const LABEL_LETTERS: usize = 2_000;
/// Letter matches may skip at most this many characters per typed letter.
const LETTER_SLACK: usize = 2;

fn score(query: &str, entry: &Entry) -> Option<usize> {
    let words = query_words(query);
    if words.is_empty() {
        return Some(LABEL_WORDS);
    }
    let label = lowered(&entry.label);
    let both = lowered(&format!("{} {}", entry.label, entry.detail));
    let bands = [
        (LABEL_WORDS, all_words(&label, &words)),
        (DETAIL_WORDS, all_words(&both, &words)),
        (LABEL_LETTERS, letters(&label, &squeezed(query))),
    ];
    bands
        .into_iter()
        .find_map(|(band, found)| found.map(|position| band + position))
}

fn query_words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_owned).collect()
}

fn lowered(text: &str) -> String {
    text.to_lowercase()
}

fn squeezed(query: &str) -> String {
    query.replace(' ', "")
}

/// Where the first word starts, when every word appears.
fn all_words(haystack: &str, words: &[String]) -> Option<usize> {
    let positions: Option<Vec<usize>> = words
        .iter()
        .map(|word| haystack.find(word.as_str()))
        .collect();
    positions.and_then(|found| found.into_iter().min())
}

/// Total gap between letters matched in order, or `None` when the gaps are too loose.
fn letters(haystack: &str, needle: &str) -> Option<usize> {
    let matched = needle
        .chars()
        .try_fold((haystack, 0_usize), |(rest, gaps), wanted| {
            let (index, found) = rest.char_indices().find(|(_, letter)| *letter == wanted)?;
            let after = rest.get(index + found.len_utf8()..)?;
            Some((after, gaps + index))
        });
    let limit = needle.chars().count().saturating_mul(LETTER_SLACK);
    matched.map(|(_, gaps)| gaps).filter(|gaps| *gaps <= limit)
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub title: String,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

impl Menu {
    pub fn step(&mut self, code: KeyCode) {
        let count = self.entries.len();
        self.selected = super::landing::moved(self.selected, count, code);
    }

    #[must_use]
    pub fn choice(&self) -> Option<Choice> {
        self.entries
            .get(self.selected)
            .map(|entry| entry.choice.clone())
    }
}

#[derive(Debug, Clone)]
pub struct Batch {
    pub forms: Vec<Form>,
    pub scroll: u16,
}
