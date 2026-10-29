//! Burned-in caption groups: a few words at a time, the spoken word highlighted.

use retold_core::WritingScript;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::align::TimedWord;
use crate::text::ends_phrase;

/// Grouping rules from the channel.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub max_words: usize,
    /// Visible characters (graphemes) per line.
    pub max_chars: usize,
    pub uppercase: bool,
    /// How long the last group stays after its last word, seconds.
    pub tail: f64,
}

/// Whether caption words are uppercased when the script has letter case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    AsWritten,
    Upper,
}

impl Rules {
    #[must_use]
    pub const fn case(self) -> Case {
        if self.uppercase {
            Case::Upper
        } else {
            Case::AsWritten
        }
    }
}

/// One caption on screen: its words, each with the moment it becomes the highlighted one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub words: Vec<CaptionWord>,
    /// When the caption leaves the screen.
    pub end: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionWord {
    /// As displayed (quotes stripped, uppercased where the script has case).
    pub text: String,
    pub start: f64,
}

fn is_cjk_str(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            crate::script::script_of(c),
            Some(WritingScript::Jpan | WritingScript::Hans | WritingScript::Hant)
        )
    })
}

impl Caption {
    #[must_use]
    pub fn start(&self) -> f64 {
        self.words.first().map_or(self.end, |w| w.start)
    }

    fn is_cjk(&self) -> bool {
        self.words.iter().any(|w| is_cjk_str(&w.text))
    }

    /// The line as shown.
    #[must_use]
    pub fn line(&self) -> String {
        let texts: Vec<&str> = self.words.iter().map(|w| w.text.as_str()).collect();
        if self.is_cjk() {
            texts.concat()
        } else {
            texts.join(" ")
        }
    }

    /// Character range of word `i` in [`Caption::line`], in chars (for per-character styles).
    #[must_use]
    pub fn char_range(&self, i: usize) -> (usize, usize) {
        let sep = usize::from(!self.is_cjk());
        let start: usize = self.words[..i]
            .iter()
            .map(|word_entry| {
                let word_count = word_entry.text.chars().count();
                word_count + sep
            })
            .sum();
        let target_len = self.words[i].text.chars().count();
        (start, start + target_len)
    }
}

/// How a word is shown.
#[must_use]
pub fn display(word: &str, script: WritingScript, case: Case) -> String {
    let w = word.trim_matches(['\"', '\'', '“', '”', '‘', '’']);
    if case == Case::Upper && script.has_case() {
        w.to_uppercase()
    } else {
        w.to_owned()
    }
}

fn width(words: &[&str]) -> usize {
    let is_cjk = words.iter().any(|w| is_cjk_str(w));
    let char_count: usize = words.iter().map(|w| w.graphemes(true).count()).sum();
    if is_cjk {
        char_count
    } else {
        char_count + words.len().saturating_sub(1)
    }
}

/// Groups timed words into captions. A caption ends at a phrase break, at `max_words`, or
/// before it would pass `max_chars`. Each caption stays until the next one starts.
#[must_use]
pub fn group(words: &[TimedWord], script: WritingScript, rules: Rules) -> Vec<Caption> {
    let case = rules.case();
    let shown: Vec<String> = words
        .iter()
        .map(|w| display(&w.text, script, case))
        .collect();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let mut probe: Vec<&str> = cur.iter().map(|&j| shown[j].as_str()).collect();
        probe.push(&shown[i]);
        if !cur.is_empty() && width(&probe) > rules.max_chars {
            groups.push(std::mem::take(&mut cur));
        }
        cur.push(i);
        if cur.len() >= rules.max_words || ends_phrase(&w.text) {
            groups.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        groups.push(cur);
    }
    groups
        .iter()
        .enumerate()
        .map(|(gi, idx)| {
            let last = words[*idx.last().unwrap_or(&0)].end;
            let end = groups
                .get(gi + 1)
                .and_then(|next| next.first())
                .map_or(last + rules.tail, |&first| words[first].start.max(last));
            Caption {
                words: idx
                    .iter()
                    .map(|&i| CaptionWord {
                        text: shown[i].clone(),
                        start: words[i].start,
                    })
                    .collect(),
                end,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, end: f64) -> TimedWord {
        TimedWord {
            text: text.into(),
            start,
            end,
        }
    }

    const RULES: Rules = Rules {
        max_words: 3,
        max_chars: 18,
        uppercase: true,
        tail: 0.25,
    };

    #[test]
    fn groups_respect_word_char_and_punctuation_limits_and_hold_until_next() {
        let words = vec![
            w("\"Then,\"", 0.0, 0.3),
            w("she", 0.3, 0.5),
            w("ran", 0.5, 0.8),
            w("out.", 0.8, 1.1),
            w("Extraordinarily", 1.2, 1.8),
            w("bright", 1.8, 2.2),
        ];
        let c = group(&words, WritingScript::Latn, RULES);
        assert_eq!(c[0].line(), "THEN,");
        assert!((c[0].end - 0.3).abs() < 1e-6);
        assert_eq!(c[1].line(), "SHE RAN OUT.");
        assert_eq!(c[1].char_range(1), (4, 7));
        assert!((c[1].end - 1.2).abs() < 1e-6);
        assert_eq!(c[2].line(), "EXTRAORDINARILY");
        assert_eq!(c[3].line(), "BRIGHT");
        assert!((c[3].end - 2.45).abs() < 1e-6);
    }

    #[test]
    fn non_latin_scripts_keep_their_characters() {
        let words = vec![w("اسرار", 0.0, 0.5), w("رات", 0.5, 1.0)];
        let c = group(&words, WritingScript::Arab, RULES);
        assert_eq!(c[0].line(), "اسرار رات");
    }
}
