//! A form for any action: one entry per argument, turned into the exact command-line words the
//! CLI would parse. The footer shows those words, so the form teaches the command.
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::catalog::{Action, Field, FieldKind};
use super::input::Input;
mod steps;

/// F7 goes back a step in stepped forms.
const PREVIOUS_STEP_KEY: u8 = 7;

/// Ctrl-S: continue to the next step, or run the form on the last one, from any field.
fn continues(key: KeyEvent) -> bool {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    control && key.code == KeyCode::Char('s')
}

/// Separates several values typed into one field.
pub const LIST_SEPARATOR: char = ';';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Flag(bool),
    Text(Input),
    /// Index into the field's choices; empty keeps the command's default.
    Choice(Option<usize>),
}

impl Entry {
    fn blank(field: &Field) -> Self {
        match field.kind {
            FieldKind::Flag => Self::Flag(bool::default()),
            FieldKind::Text => Self::Text(Input::default()),
            FieldKind::Choice(_) => Self::Choice(Option::default()),
        }
    }

    fn filled(field: &Field, value: &str) -> Self {
        match &field.kind {
            FieldKind::Flag => Self::Flag(value == "true"),
            FieldKind::Text => Self::Text(Input::with(value)),
            FieldKind::Choice(choices) => {
                Self::Choice(choices.iter().position(|choice| choice == value))
            }
        }
    }

    #[must_use]
    pub fn is_blank(&self) -> bool {
        match self {
            Self::Flag(on) => !on,
            Self::Text(input) => input.text().trim().is_empty(),
            Self::Choice(selected) => selected.is_none(),
        }
    }

    /// Space switches a flag; ← → step through choices; text fields take typing.
    fn apply(&mut self, key: KeyEvent, choices: usize) {
        match self {
            Self::Flag(on) => *on ^= switches(key),
            Self::Text(input) => input.edit(key),
            Self::Choice(selected) => *selected = cycled(*selected, choices, key),
        }
    }
}

const fn switches(key: KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::Char(' ' | 'x') | KeyCode::Left | KeyCode::Right
    )
}

/// Steps forward with space or →, back with ←; past either end returns to the default.
fn cycled(selected: Option<usize>, count: usize, key: KeyEvent) -> Option<usize> {
    let forward = matches!(key.code, KeyCode::Char(' ') | KeyCode::Right);
    let backward = matches!(key.code, KeyCode::Left);
    let last = count.checked_sub(1);
    match (forward, backward, selected) {
        (true, _, None) => last.map(|_| 0),
        (true, _, Some(index)) => index.checked_add(1).filter(|next| *next < count),
        (_, true, None) => last,
        (_, true, Some(index)) => index.checked_sub(1),
        _ => selected,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub action: Action,
    pub entries: Vec<Entry>,
    /// A field index, or the Run button after the last field.
    pub focus: usize,
    pub problem: Option<String>,
    pub steps: Vec<steps::Step>,
    pub stage: usize,
    pub edited: std::collections::BTreeSet<String>,
}

impl Form {
    pub fn value(&self, id: &str) -> Option<String> {
        let index = self.action.fields.iter().position(|field| field.id == id)?;
        let field = &self.action.fields[index];
        let entry = &self.entries[index];
        if let Entry::Flag(on) = entry {
            return Some(on.to_string());
        }
        values(field, &self.entries[index])
            .first()
            .cloned()
            .or_else(|| field.default.clone())
    }

    #[must_use]
    pub fn new(action: Action) -> Self {
        let entries = action.fields.iter().map(Entry::blank).collect();
        let mut form = Self {
            action,
            entries,
            focus: 0,
            problem: Option::default(),
            steps: Vec::new(),
            stage: 0,
            edited: std::collections::BTreeSet::new(),
        };
        form.steps = steps::for_form(&form);
        form
    }

    /// Fills a field by id, such as the video reference when opened from a video.
    #[must_use]
    pub fn with(mut self, id: &str, value: &str) -> Self {
        let position = self.action.fields.iter().position(|field| field.id == id);
        let filled = position.and_then(|index| {
            let field = self.action.fields.get(index)?;
            Some((index, Entry::filled(field, value)))
        });
        if let Some((index, entry)) = filled {
            self.entries[index] = entry;
        }
        self.focus = self.missing().unwrap_or_default();
        self
    }

    /// The Run button's position, after the last field.
    #[must_use]
    pub const fn button(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub const fn on_button(&self) -> bool {
        self.focus >= self.button()
    }

    #[must_use]
    pub fn focused_field(&self) -> Option<&Field> {
        self.action.fields.get(self.focus)
    }

    /// The first required field still empty.
    #[must_use]
    pub fn missing(&self) -> Option<usize> {
        self.action
            .fields
            .iter()
            .zip(&self.entries)
            .position(|(field, entry)| field.required && entry.is_blank())
    }

    #[must_use]
    pub fn complete(&self) -> bool {
        self.missing().is_none()
    }

    /// Whether `key` runs the form: Enter on the button with nothing required missing.
    #[must_use]
    pub fn submits(&self, key: KeyEvent) -> bool {
        let on_button = key.code == KeyCode::Enter && self.on_button();
        let pressed = on_button || continues(key);
        pressed && self.complete() && self.last_step()
    }

    /// Moves between fields, edits the focused one, or explains what is missing.
    pub fn press(&mut self, key: KeyEvent) {
        self.problem = Option::default();
        match key.code {
            KeyCode::Down | KeyCode::Tab => self.step(1),
            KeyCode::Up | KeyCode::BackTab => self.step(-1),
            KeyCode::Enter => self.enter(),
            KeyCode::F(PREVIOUS_STEP_KEY) => self.previous_step(),
            _ if continues(key) => self.press_button(),
            _ => self.edit(key),
        }
    }

    /// Ctrl-S from any field does what the button does.
    fn press_button(&mut self) {
        self.focus = self.button();
        self.enter();
    }

    /// Enter moves to the next field; on the button it opens the next step, or on the last
    /// step explains anything still missing.
    fn enter(&mut self) {
        match (self.on_button(), self.last_step()) {
            (false, _) => self.step(1),
            (true, false) => self.next_step(),
            (true, true) => self.flag_missing(),
        }
    }

    fn step(&mut self, step: isize) {
        let fields = self.visible_fields();
        let stops = fields.len().saturating_add(1).cast_signed();
        let focus = self.focus_row().cast_signed();
        let next = focus.saturating_add(step).rem_euclid(stops);
        self.focus = fields
            .get(next.cast_unsigned())
            .copied()
            .unwrap_or_else(|| self.button());
    }

    fn edit(&mut self, key: KeyEvent) {
        self.mark_focused_edited();
        let choices = self.focused_choices();
        let focus = self.focus;
        let entry = self.entries.get_mut(focus);
        if let Some(entry) = entry {
            entry.apply(key, choices);
        }
    }

    pub fn mark_focused_edited(&mut self) {
        let field = self.action.fields.get(self.focus);
        if let Some(field) = field {
            self.edited.insert(field.id.clone());
        }
    }

    pub fn mark_edited(&mut self, id: &str) {
        self.edited.insert(id.to_owned());
    }

    pub fn permits_default(&self, id: &str) -> bool {
        !self.has_input(id) && !self.edited.contains(id)
    }

    fn focused_choices(&self) -> usize {
        match self.focused_field().map(|field| &field.kind) {
            Some(FieldKind::Choice(choices)) => choices.len(),
            _ => 0,
        }
    }

    /// Moves to the first missing required field and says what it needs.
    pub fn flag_missing(&mut self) {
        let Some(index) = self.missing() else {
            return;
        };
        let label = self
            .action
            .fields
            .get(index)
            .map(|field| field.label.clone());
        self.problem = label.map(|label| format!("{label} is required."));
        self.focus = index;
        let holding = self
            .steps
            .iter()
            .position(|step| step.fields.contains(&index));
        if let Some(stage) = holding {
            self.stage = stage;
        }
    }

    /// The words after `retold`: the command path, options and flags, then positionals.
    #[must_use]
    pub fn arguments(&self) -> Vec<String> {
        let pairs = self.action.fields.iter().zip(&self.entries);
        let options: Vec<String> = pairs
            .clone()
            .filter_map(|(field, entry)| {
                field
                    .long
                    .as_ref()
                    .map(|long| option_words(field, long, entry))
            })
            .flatten()
            .collect();
        let positionals: Vec<String> = pairs
            .filter(|(field, _)| field.long.is_none())
            .flat_map(|(field, entry)| values(field, entry))
            .collect();
        let separator = positionals.iter().any(|word| word.starts_with('-'));
        let marker = separator.then(|| "--".to_owned());
        let path = self.action.path.iter().cloned();
        path.chain(options)
            .chain(marker)
            .chain(positionals)
            .collect()
    }

    /// The full command as a person would type it, quoted for a POSIX shell.
    #[must_use]
    pub fn command_line(&self) -> String {
        command_line(&self.arguments())
    }

    /// Use the CLI's conflicts and required groups before asking the user to run.
    #[must_use]
    pub fn validation_problem(&self) -> Option<String> {
        let words = std::iter::once("retold".to_owned()).chain(self.arguments());
        crate::command()
            .try_get_matches_from(words)
            .err()
            .map(|error| validation_message(&error.to_string()))
    }
}

fn validation_message(error: &str) -> String {
    error
        .lines()
        .next()
        .unwrap_or("Check the command inputs.")
        .trim_start_matches("error: ")
        .to_owned()
}

fn values(field: &Field, entry: &Entry) -> Vec<String> {
    match (entry, &field.kind) {
        (Entry::Text(input), _) => split(field, input.text()),
        (Entry::Choice(Some(index)), FieldKind::Choice(choices)) => {
            choices.get(*index).cloned().into_iter().collect()
        }
        _ => Vec::new(),
    }
}

/// One value, or several separated by `;` for fields that repeat.
fn split(field: &Field, text: &str) -> Vec<String> {
    let trimmed = text.trim();
    let parts: Vec<&str> = if field.multiple {
        trimmed.split(LIST_SEPARATOR).map(str::trim).collect()
    } else {
        vec![trimmed]
    };
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn option_words(field: &Field, long: &str, entry: &Entry) -> Vec<String> {
    let flag = format!("--{long}");
    match entry {
        Entry::Flag(true) => vec![flag],
        Entry::Flag(false) => Vec::new(),
        Entry::Text(_) | Entry::Choice(_) => values(field, entry)
            .into_iter()
            .flat_map(|value| [flag.clone(), value])
            .collect(),
    }
}

/// `retold` followed by each word, quoted only when a shell would need it.
#[must_use]
pub fn command_line(words: &[String]) -> String {
    let quoted: Vec<String> = words.iter().map(|word| quote(word)).collect();
    format!("retold {}", quoted.join(" "))
}

const SHELL_SAFE: &str = "-_./:=,@%+~";

fn quote(word: &str) -> String {
    let plain = |letter: char| letter.is_alphanumeric() || SHELL_SAFE.contains(letter);
    let safe = !word.is_empty() && word.chars().all(plain);
    if safe {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

impl Form {
    pub fn review_problem(&self) -> Option<String> {
        let activating = self.action.command() == "storage activate";
        if !activating {
            return self.validation_problem();
        }
        let preview = self.clone().with("plan-hash", "");
        preview.with("dry-run", "true").validation_problem()
    }
}

impl Form {
    pub fn has_input(&self, id: &str) -> bool {
        self.action
            .fields
            .iter()
            .position(|field| field.id == id)
            .is_some_and(|index| !self.entries[index].is_blank())
    }
}

impl Form {
    pub fn list_values(&self, id: &str) -> Vec<String> {
        let Some(index) = self.action.fields.iter().position(|field| field.id == id) else {
            return Vec::new();
        };
        values(&self.action.fields[index], &self.entries[index])
    }
}
