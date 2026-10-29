//! Subtitle files (`.srt`, `.vtt`) for uploads that take separate caption tracks.

use std::fmt::Write as _;

use retold_core::WritingScript;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::align::TimedWord;
use crate::text::ends_phrase;
use retold_core::num;

/// One subtitle on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

fn is_cjk_words(words: &[&str]) -> bool {
    words.iter().any(|w| {
        w.chars().any(|c| {
            matches!(
                crate::script::script_of(c),
                Some(WritingScript::Jpan | WritingScript::Hans | WritingScript::Hant)
            )
        })
    })
}

fn join_words(words: &[&str]) -> String {
    if is_cjk_words(words) {
        words.concat()
    } else {
        words.join(" ")
    }
}

/// Readable subtitle cues from timed words: up to two lines of `line_chars`, breaking at
/// phrase ends where possible, and never longer than `max_seconds`.
#[must_use]
pub fn cues_from_words(words: &[TimedWord], line_chars: usize, max_seconds: f64) -> Vec<Cue> {
    let mut cues = Vec::new();
    let mut cur: Vec<&TimedWord> = Vec::new();
    let flush = |cur: &mut Vec<&TimedWord>, cues: &mut Vec<Cue>| {
        if let (Some(f), Some(l)) = (cur.first(), cur.last()) {
            let text = wrap(
                &cur.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
                line_chars,
            );
            cues.push(Cue {
                start: f.start,
                end: l.end,
                text,
            });
        }
        cur.clear();
    };
    for w in words {
        let cur_chars: usize = cur
            .iter()
            .map(|x| {
                let count = x.text.graphemes(true).count();
                count + 1
            })
            .sum::<usize>();
        let word_chars = w.text.graphemes(true).count();
        let chars = cur_chars + word_chars;
        let too_long = cur.first().is_some_and(|f| w.end - f.start > max_seconds);
        if !cur.is_empty() && (chars > line_chars * 2 || too_long) {
            flush(&mut cur, &mut cues);
        }
        cur.push(w);
        let chars_now: usize = cur
            .iter()
            .map(|x| {
                let count = x.text.graphemes(true).count();
                count + 1
            })
            .sum();
        if ends_phrase(&w.text) && chars_now > line_chars / 2 {
            flush(&mut cur, &mut cues);
        }
    }
    flush(&mut cur, &mut cues);
    // Keep cues from overlapping, and give short ones a moment to be read.
    for i in 0..cues.len() {
        let next = cues.get(i + 1).map(|c| c.start);
        let c = &mut cues[i];
        c.end = c.end.max(c.start + 0.8);
        if let Some(n) = next {
            c.end = c.end.min(n);
        }
    }
    cues
}

/// Breaks a cue into at most two balanced lines.
fn wrap(words: &[&str], line_chars: usize) -> String {
    let joined = join_words(words);
    if joined.graphemes(true).count() <= line_chars || words.len() < 2 {
        return joined;
    }
    let total = joined.graphemes(true).count();
    let mut best = (usize::MAX, 1);
    let mut left = 0;
    let cjk = is_cjk_words(words);
    for i in 1..words.len() {
        let prev_count = words[i - 1].graphemes(true).count();
        let sep_count = usize::from(i > 1 && !cjk);
        left += prev_count + sep_count;
        let diff = left.abs_diff(total.saturating_sub(left));
        if diff < best.0 {
            best = (diff, i);
        }
    }
    format!(
        "{}\n{}",
        join_words(&words[..best.1]),
        join_words(&words[best.1..])
    )
}

fn stamp(seconds: f64, sep: char) -> String {
    let ms = num::round_u64(seconds.max(0.0) * 1000.0);
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

/// Formats cues as `SubRip` (`.srt`).
#[must_use]
pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}\n{} --> {}\n{}\n",
            i + 1,
            stamp(c.start, ','),
            stamp(c.end, ','),
            c.text
        );
    }
    out
}

/// Formats cues as `WebVTT` (`.vtt`).
#[must_use]
pub fn to_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (i, c) in cues.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}\n{} --> {}\n{}\n",
            i + 1,
            stamp(c.start, '.'),
            stamp(c.end, '.'),
            c.text
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timed(words: &[(&str, f64, f64)]) -> Vec<TimedWord> {
        words
            .iter()
            .map(|(w, s, e)| TimedWord {
                text: (*w).to_owned(),
                start: *s,
                end: *e,
            })
            .collect()
    }

    #[test]
    fn cues_cover_all_words_without_overlap() {
        let w = timed(&[
            ("A", 0.0, 0.2),
            ("man", 0.2, 0.5),
            ("walks", 0.5, 0.8),
            ("into", 0.8, 1.1),
            ("Wisborg,", 1.1, 1.5),
            ("shadows", 1.5, 2.0),
            ("stretch", 2.0, 2.5),
            ("long.", 2.5, 3.0),
        ]);
        let cues = cues_from_words(&w, 20, 2.0);
        assert_ne!(cues.len(), 0);
        for pair in cues.windows(2) {
            assert!(pair[0].end <= pair[1].start);
        }
    }

    #[test]
    fn formats_srt_and_vtt() {
        let cues = vec![
            Cue {
                start: 0.0,
                end: 1.5,
                text: "Line one".to_owned(),
            },
            Cue {
                start: 1.6,
                end: 3.2,
                text: "Line two".to_owned(),
            },
        ];
        let srt = to_srt(&cues);
        let vtt = to_vtt(&cues);
        assert!(srt.contains("00:00:00,000 --> 00:00:01,500"));
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:00:00.000 --> 00:00:01.500"));
    }
}
