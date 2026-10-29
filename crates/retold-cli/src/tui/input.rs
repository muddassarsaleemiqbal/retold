//! A one-line text field with a cursor, shared by forms, filters and the palette. Positions are
//! counted in grapheme clusters, so accents and joined letters stay together.
use unicode_segmentation::UnicodeSegmentation;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Bound one-line fields so large clipboard payloads cannot stall editing or drawing.
const MAX_INPUT_BYTES: usize = 65_536;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    cursor: usize,
}

/// Control or Alt held: the key is a shortcut, not a letter.
fn chord(key: KeyEvent) -> bool {
    let modifiers = key.modifiers;
    modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

impl Input {
    #[must_use]
    pub fn with(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.graphemes(true).count(),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Inserts a whole paste once. Controls cannot become keys; line breaks become spaces.
    pub fn paste(&mut self, text: &str) {
        let problem = self.paste_problem(text);
        if problem.is_some() {
            return;
        }
        let text: String = text
            .replace("\r\n", "\n")
            .chars()
            .filter_map(pasted_character)
            .collect();
        self.insert_text(&text);
    }

    /// Inspect before editing so the owning page can explain a refused paste.
    pub fn paste_problem(&self, text: &str) -> Option<&'static str> {
        let bytes = text.len().saturating_add(self.text.len());
        (bytes > MAX_INPUT_BYTES).then_some("Paste refused: a text field can hold at most 64 KiB.")
    }

    fn insert_text(&mut self, text: &str) {
        let at = self.byte(self.cursor);
        self.text.insert_str(at, text);
        self.cursor = self.text[..at.saturating_add(text.len())]
            .graphemes(true)
            .count();
    }

    /// Applies an editing key; keys that do not edit are ignored.
    pub fn edit(&mut self, key: KeyEvent) {
        match (chord(key), key.code) {
            (false, KeyCode::Char(letter)) => self.insert(letter),
            (true, KeyCode::Char('u')) => self.delete_to_start(),
            (true, KeyCode::Char('w') | KeyCode::Backspace) => self.delete_word(),
            (true, KeyCode::Char('a')) | (_, KeyCode::Home) => self.cursor = 0,
            (true, KeyCode::Char('e')) | (_, KeyCode::End) => self.cursor = self.length(),
            (_, KeyCode::Backspace) => self.backspace(),
            (_, KeyCode::Delete) => self.delete(),
            (_, KeyCode::Left) => self.cursor = self.cursor.saturating_sub(1),
            (_, KeyCode::Right) => self.step_right(),
            _ => {}
        }
    }

    fn step_right(&mut self) {
        self.cursor = self.cursor.saturating_add(1).min(self.length());
    }

    fn length(&self) -> usize {
        self.text.graphemes(true).count()
    }

    /// The byte offset of grapheme `position`.
    fn byte(&self, position: usize) -> usize {
        let position = self.text.grapheme_indices(true).nth(position);
        position.map_or(self.text.len(), |(at, _)| at)
    }

    fn insert(&mut self, letter: char) {
        let at = self.byte(self.cursor);
        self.text.insert(at, letter);
        self.cursor = self.text[..at.saturating_add(letter.len_utf8())]
            .graphemes(true)
            .count();
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let end = self.byte(self.cursor);
        self.cursor = self.cursor.saturating_sub(1);
        let at = self.byte(self.cursor);
        self.text.replace_range(at..end, "");
    }

    fn delete(&mut self) {
        let inside = self.cursor < self.length();
        if inside {
            let at = self.byte(self.cursor);
            let end = self.byte(self.cursor.saturating_add(1));
            self.text.replace_range(at..end, "");
        }
    }

    fn delete_to_start(&mut self) {
        let at = self.byte(self.cursor);
        self.text.replace_range(..at, "");
        self.cursor = 0;
    }

    /// Removes the word before the cursor and any spaces after it, as shells do.
    fn delete_word(&mut self) {
        let start = word_start(&self.text, self.cursor);
        let (from, to) = (self.byte(start), self.byte(self.cursor));
        self.text.replace_range(from..to, "");
        self.cursor = start;
    }
}

fn pasted_character(letter: char) -> Option<char> {
    if matches!(
        letter,
        '\r' | '\n' | '\t' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    ) {
        Some(' ')
    } else {
        (!letter.is_control()).then_some(letter)
    }
}

/// Where the word ending at `cursor` begins, skipping spaces just before the cursor.
fn word_start(text: &str, cursor: usize) -> usize {
    let before: Vec<&str> = text.graphemes(true).take(cursor).collect();
    let spaces = before
        .iter()
        .rev()
        .take_while(|letter| letter.chars().all(char::is_whitespace))
        .count();
    let word = before
        .iter()
        .rev()
        .skip(spaces)
        .take_while(|letter| !letter.chars().all(char::is_whitespace))
        .count();
    cursor.saturating_sub(spaces).saturating_sub(word)
}
