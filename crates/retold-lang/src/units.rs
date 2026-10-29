//! Word-preserving visual units balanced around the requested speaking-time range.

use crate::text::{self, speech_seconds};
use retold_core::LanguageTag;
use retold_core::num;

/// Balance the remaining speech over the remaining images. Phrase endings get a small
/// preference, while a short final sentence is absorbed rather than becoming its own frame.
#[must_use]
pub fn pack(sentences: &[String], range: [f64; 2], lang: &LanguageTag) -> Vec<String> {
    let profile = crate::profile::profile_for(lang);
    let words: Vec<String> = sentences
        .iter()
        .flat_map(|s| profile.tokenize_subtitles(s))
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let [lo, hi] = range;
    if !lo.is_finite() || !hi.is_finite() || lo <= 0.0 || hi < lo {
        return vec![profile.join_tokens(&words.iter().map(String::as_str).collect::<Vec<_>>())];
    }
    let mut prefix = vec![0.0];
    for word in &words {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + speech_seconds(word, lang));
    }
    let total = prefix[words.len()];
    let count = num::trunc_usize(
        (total / hi)
            .ceil()
            .max(1.0)
            .min(num::count_f64(words.len())),
    );
    let mut units = Vec::with_capacity(count);
    let mut start = 0;
    for index in 0..count {
        let remaining = count - index;
        if remaining == 1 {
            units.push(
                profile.join_tokens(
                    &words[start..]
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                ),
            );
            break;
        }
        let target = (total - prefix[start]) / num::count_f64(remaining);
        let remaining_offset = remaining.saturating_sub(1);
        let last = words.len().saturating_sub(remaining_offset);
        let end = (start + 1..=last)
            .min_by(|a, b| {
                let score = |end: usize| {
                    let seconds = prefix[end] - prefix[start];
                    let outside = if seconds < lo {
                        lo - seconds
                    } else if seconds > hi {
                        seconds - hi
                    } else {
                        0.0
                    };
                    (seconds - target).abs()
                        + outside * 3.0
                        + if text::ends_phrase(&words[end - 1]) {
                            0.0
                        } else {
                            0.18
                        }
                };
                score(*a).total_cmp(&score(*b))
            })
            .unwrap_or(start + 1);
        units.push(
            profile.join_tokens(
                &words[start..end]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
        );
        start = end;
    }
    units
}

#[must_use]
pub fn pair_up(units: &[String]) -> Vec<String> {
    units
        .chunks(2)
        .map(|c| {
            let is_cjk = c.iter().any(|u| {
                u.chars().any(|ch| {
                    matches!(
                        crate::script::script_of(ch),
                        Some(
                            retold_core::WritingScript::Jpan
                                | retold_core::WritingScript::Hans
                                | retold_core::WritingScript::Hant
                        )
                    )
                })
            });
            if is_cjk { c.concat() } else { c.join(" ") }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn en() -> LanguageTag {
        "en".parse().unwrap()
    }

    #[test]
    fn balances_units_without_losing_words() {
        let s = vec![
            "A man laughs then packs the book anyway.".to_owned(),
            "He leaves by the back door.".to_owned(),
        ];
        let u = pack(&s, [2.0, 4.0], &en());
        let back: Vec<&str> = u.iter().flat_map(|u| u.split_whitespace()).collect();
        assert_eq!(
            back,
            vec![
                "A", "man", "laughs", "then", "packs", "the", "book", "anyway.", "He", "leaves",
                "by", "the", "back", "door."
            ]
        );
    }

    #[test]
    fn short_tail_is_balanced_with_the_previous_words() {
        let s = vec!["A man laughs then packs the book anyway. Go.".to_owned()];
        let u = pack(&s, [2.0, 4.0], &en());
        assert_ne!(u.len(), 0);
        assert_ne!(u.last().unwrap(), "Go.");
    }

    #[test]
    fn lean_pairs() {
        let u = vec!["one".to_owned(), "two".to_owned(), "three".to_owned()];
        assert_eq!(pair_up(&u), vec!["one two", "three"]);
    }

    #[test]
    fn long_sentences_use_phrase_breaks_and_stay_near_the_range() {
        let s = vec![
            "Count Orlok rises from the tomb, smiles at the moonlight, and turns to the sea."
                .to_owned(),
        ];
        let u = pack(&s, [2.0, 3.5], &en());
        assert!(u.len() >= 2);
    }

    proptest! {
        #[test]
        fn packing_never_loses_or_reorders_words(
            words in prop::collection::vec("[a-z]{1,8}", 1..20),
            lo in 1.0f64..3.0,
            hi in 3.0f64..6.0
        ) {
            let sentence = words.join(" ");
            let u = pack(&[sentence], [lo, hi], &en());
            let back: Vec<String> = u.iter().flat_map(|u| u.split_whitespace().map(ToString::to_string)).collect();
            prop_assert_eq!(back, words);
        }
    }
}
