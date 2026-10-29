//! Matching script words to the times they are spoken.
//!
//! The narration is read in one continuous take; a transcriber returns spoken words with
//! times. Words are compared on a folded form (lowercase, no punctuation, no nukta, diacritics
//! or harakat, with Bunsetsu and non-whitespace support), matched with a diff, and any word
//! the transcriber heard differently gets a time interpolated between its matched neighbours.

use std::sync::Arc;

use retold_core::LanguageTag;
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::profile::{LanguageProfile, global_registry};
use retold_core::num;

/// A word the transcriber heard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpokenWord {
    pub word: String,
    pub start: f64,
    pub end: f64,
}

/// A script word with its time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// One unit's words with times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedUnit {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub words: Vec<TimedWord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alignment {
    /// Length of the narration audio, seconds.
    pub duration: f64,
    pub units: Vec<TimedUnit>,
    /// Share of script words matched directly (the rest are interpolated).
    pub matched: f64,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AlignError {
    #[error(
        "the transcript shares no words with the script; the voice may have read the wrong text"
    )]
    NoMatch,
    #[error("the script is empty")]
    Empty,
}

/// The comparison form of a word using generic folding rules.
#[must_use]
pub fn fold(word: &str) -> String {
    crate::profile::GenericProfile::default().fold_token(word)
}

/// Detects the most appropriate language profile from unit texts if none was explicitly given.
#[must_use]
pub fn detect_profile_from_units(units: &[(String, String)]) -> Arc<dyn LanguageProfile> {
    for (_, text) in units {
        for c in text.chars() {
            let u = c as u32;
            if matches!(u, 0x3040..=0x30FF) {
                return global_registry().profile_for_code("ja");
            }
            if matches!(u, 0x0600..=0x06FF | 0x0750..=0x077F) {
                return global_registry().profile_for_code("ar");
            }
            if matches!(c, '¿' | '¡') {
                return global_registry().profile_for_code("es");
            }
            if matches!(c, 'ı' | 'İ' | 'ğ' | 'Ğ' | 'ş' | 'Ş') {
                return global_registry().profile_for_code("tr");
            }
        }
    }
    global_registry().profile_for_code("en")
}

/// Aligns each unit's text (`(id, text)`, in order) to the spoken words, auto-detecting language.
pub fn align(
    units: &[(String, String)],
    spoken: &[SpokenWord],
    duration: f64,
) -> Result<Alignment, AlignError> {
    let profile = detect_profile_from_units(units);
    align_with_profile(units, spoken, duration, profile.as_ref())
}

/// Aligns unit text to spoken words using an explicit language tag.
pub fn align_for_lang(
    units: &[(String, String)],
    spoken: &[SpokenWord],
    duration: f64,
    lang: &LanguageTag,
) -> Result<Alignment, AlignError> {
    let profile = crate::profile::profile_for(lang);
    align_with_profile(units, spoken, duration, profile.as_ref())
}

/// Aligns each unit's text to spoken words using a specific language profile.
pub fn align_with_profile(
    units: &[(String, String)],
    spoken: &[SpokenWord],
    duration: f64,
    profile: &dyn LanguageProfile,
) -> Result<Alignment, AlignError> {
    let script: Vec<(usize, String)> = units
        .iter()
        .enumerate()
        .flat_map(|(i, (_, t))| {
            profile
                .tokenize_subtitles(t)
                .into_iter()
                .map(move |w| (i, w))
        })
        .collect();
    if script.is_empty() {
        return Err(AlignError::Empty);
    }

    let a: Vec<String> = script.iter().map(|(_, w)| profile.fold_token(w)).collect();
    let b: Vec<String> = spoken.iter().map(|w| profile.fold_token(&w.word)).collect();

    let mut times: Vec<Option<(f64, f64)>> = vec![None; a.len()];
    diff_match_times(&a, &b, spoken, &mut times);

    // Fallback: For CJK scripts, if no exact token matched (e.g. slight compound boundary differences in Japanese),
    // perform substring/prefix overlap matching.
    if profile.is_cjk() && times.iter().all(Option::is_none) {
        fallback_overlap_match(&a, &b, spoken, &mut times);
    }

    let known_count = times.iter().filter(|t| t.is_some()).count();
    if known_count == 0 {
        return Err(AlignError::NoMatch);
    }

    let matched = num::count_f64(known_count) / num::count_f64(a.len());
    let filled = interpolate_gaps(&times, a.len(), duration);
    let out = build_timed_units(units, &script, &filled);

    Ok(Alignment {
        duration,
        units: out,
        matched: round(matched),
    })
}

fn diff_match_times(
    a: &[String],
    b: &[String],
    spoken: &[SpokenWord],
    times: &mut [Option<(f64, f64)>],
) {
    for op in capture_diff_slices(Algorithm::Patience, a, b) {
        if let DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = op
        {
            for k in 0..len {
                let s = &spoken[new_index + k];
                times[old_index + k] = Some((s.start, s.end));
            }
        }
    }
}

fn fallback_overlap_match(
    a: &[String],
    b: &[String],
    spoken: &[SpokenWord],
    times: &mut [Option<(f64, f64)>],
) {
    let mut last_b = 0;
    for (i, a_tok) in a.iter().enumerate() {
        if a_tok.is_empty() {
            continue;
        }
        for (j, b_tok) in b.iter().enumerate().skip(last_b) {
            if b_tok.is_empty() {
                continue;
            }
            if a_tok.contains(b_tok) || b_tok.contains(a_tok) {
                let s = &spoken[j];
                times[i] = Some((s.start, s.end));
                last_b = j + 1;
                break;
            }
        }
    }
}

fn interpolate_gaps(times: &[Option<(f64, f64)>], a_len: usize, duration: f64) -> Vec<(f64, f64)> {
    let known: Vec<usize> = times
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.map(|_| i))
        .collect();

    (0..a_len)
        .map(|i| {
            if let Some(t) = times[i] {
                return t;
            }
            let lo = known.iter().rev().find(|&&k| k < i).copied();
            let hi = known.iter().find(|&&k| k > i).copied();
            let t0 = lo.and_then(|k| times[k]).map_or(0.0, |t| t.1);
            let t1 = hi.and_then(|k| times[k]).map_or(duration, |t| t.0);
            let left = lo.map_or(-1.0, num::count_f64);
            let span = hi.map_or_else(|| num::count_f64(a_len), num::count_f64) - left;
            let pos = num::count_f64(i) - left;
            let step = (t1 - t0) / span;
            (t0 + step * (pos - 0.5), t0 + step * (pos + 0.5))
        })
        .collect()
}

fn build_timed_units(
    units: &[(String, String)],
    script: &[(usize, String)],
    filled: &[(f64, f64)],
) -> Vec<TimedUnit> {
    let mut out = Vec::with_capacity(units.len());
    for (ui, (id, _)) in units.iter().enumerate() {
        let words: Vec<TimedWord> = script
            .iter()
            .zip(filled)
            .filter(|((u, _), _)| *u == ui)
            .map(|((_, w), (s, e))| TimedWord {
                text: w.clone(),
                start: round(*s),
                end: round(*e),
            })
            .collect();
        let (start, end) = match (words.first(), words.last()) {
            (Some(f), Some(l)) => (f.start, l.end),
            _ => continue,
        };
        out.push(TimedUnit {
            id: id.clone(),
            start,
            end,
            words,
        });
    }
    out
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spoken(words: &[(&str, f64)]) -> Vec<SpokenWord> {
        words
            .iter()
            .map(|(w, t)| SpokenWord {
                word: (*w).to_owned(),
                start: *t,
                end: t + 0.3,
            })
            .collect()
    }

    fn units(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn exact_transcript_gives_exact_times() {
        let u = units(&[("u01", "A man laughs."), ("u02", "Then he packs.")]);
        let s = spoken(&[
            ("a", 0.0),
            ("man", 0.4),
            ("laughs", 0.8),
            ("then", 1.5),
            ("he", 1.9),
            ("packs", 2.3),
        ]);
        let al = align(&u, &s, 3.0).unwrap();
        assert!((al.matched - 1.0).abs() < 1e-9);
        assert_eq!(al.units[1].start, 1.5);
        assert_eq!(al.units[1].words[2].text, "packs.");
    }

    #[test]
    fn misheard_words_are_interpolated_in_order() {
        let u = units(&[("u01", "Count Orlok rises from Wisborg harbour.")]);
        let s = spoken(&[
            ("count", 0.0),
            ("or", 0.4),
            ("lock", 0.6),
            ("rises", 1.0),
            ("from", 1.4),
            ("wiesburg", 1.8),
            ("harbour", 2.2),
        ]);
        let al = align(&u, &s, 3.0).unwrap();
        let w = &al.units[0].words;
        for pair in w.windows(2) {
            assert!(pair[0].start <= pair[1].start);
            assert!(pair[0].end <= pair[1].end);
        }
    }

    #[test]
    fn nothing_in_common_is_an_error() {
        let u = units(&[("u01", "A red balloon floats.")]);
        let s = spoken(&[("totally", 0.0), ("different", 0.4), ("text", 0.8)]);
        assert!(matches!(align(&u, &s, 1.0), Err(AlignError::NoMatch)));
    }
}
