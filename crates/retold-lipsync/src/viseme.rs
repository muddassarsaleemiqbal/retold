//! Phonemes to mouth shapes, timed to the frame, plus seeded natural blinks.
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

/// The eight mouth shapes of a mouth sheet, in sheet order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Viseme {
    Rest,
    Ai,
    E,
    O,
    U,
    Mbp,
    Fv,
    L,
}
impl Viseme {
    pub const ALL: [Self; 8] = [
        Self::Rest,
        Self::Ai,
        Self::E,
        Self::O,
        Self::U,
        Self::Mbp,
        Self::Fv,
        Self::L,
    ];
    /// How the sheet prompt describes this shape.
    #[must_use]
    pub fn description(self) -> &'static str {
        let descriptions = [
            (Self::Rest, "relaxed and closed, at rest"),
            (Self::Ai, "wide open, as in 'ah' (A/I)"),
            (
                Self::E,
                "slightly open and spread, teeth showing, as in 'ee' and most consonants (E)",
            ),
            (Self::O, "open and rounded, as in 'oh' (O)"),
            (Self::U, "small and pursed forward, as in 'oo' (U)"),
            (Self::Mbp, "lips pressed together, as in 'm', 'b', 'p'"),
            (
                Self::Fv,
                "upper front teeth pressed directly onto the lower lip, lower lip tucked under the teeth, nearly closed with NO black gap under the teeth and NO smile, as in 'f', 'v'",
            ),
            (
                Self::L,
                "open with the tongue tip touching behind the upper teeth, as in 'l'",
            ),
        ];
        descriptions
            .iter()
            .find(|(viseme_item, ..)| *viseme_item == self)
            .map_or("", |(_, desc_str)| *desc_str)
    }
}

/// Eye state for blinks; `Open` is the original artwork and needs no sprite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Eyes {
    Open,
    Half,
    Closed,
}

/// Mouth shapes for one espeak-ng IPA transcription, each with a relative duration.
#[must_use]
pub fn from_ipa(ipa: &str) -> Vec<(Viseme, f64)> {
    let mut out: Vec<(Viseme, f64)> = Vec::new();
    for c in ipa.chars() {
        if c == 'ː' {
            if let Some(last) = out.last_mut() {
                last.1 *= 1.5;
            }
            continue;
        }
        if matches!(c, 'ˈ' | 'ˌ' | 'ʰ' | 'ʱ' | 'ʲ' | 'ʷ' | 'ˑ') {
            continue;
        }
        if let Some(shape) = ipa_vowel_shape(c).or_else(|| ipa_consonant_shape(c)) {
            out.push(shape);
        } else if c.is_alphabetic() {
            out.push((Viseme::E, 1.0));
        }
    }
    out
}

/// Language-specific visible articulations on top of the shared IPA table.
/// Japanese /ɸ/ rounds the lips, /ɾ/ uses the tongue tip, and /ɯ/ is unrounded.
#[must_use]
pub fn from_ipa_for_language(ipa: &str, language: &str) -> Vec<(Viseme, f64)> {
    let base = language.split('-').next().unwrap_or(language);
    match base {
        "ja" => from_ipa(&ipa.replace('ɸ', "u").replace('ɾ', "l").replace('ɯ', "ɨ")),
        "hi" | "ur" => from_ipa(
            &ipa.replace('ã', "a")
                .replace('ẽ', "e")
                .replace('ĩ', "i")
                .replace('õ', "o")
                .replace('ũ', "u"),
        ),
        _ => from_ipa(ipa),
    }
}

/// IPA for one word from espeak-ng; `language` is an espeak voice such as `en`, `hi`, `ur`.
pub fn ipa(espeak: &Path, language: &str, word: &str) -> Result<String, Error> {
    if !language
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(Error::Invalid("invalid espeak language".into()));
    }
    let output = std::process::Command::new(espeak)
        .args(["-q", "--ipa", "--sep=|", "-v", language, "--"])
        .arg(word)
        .output()?;
    if !output.status.success() {
        return Err(Error::Invalid(format!(
            "espeak-ng failed for {language}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A spoken word with its IPA, in seconds relative to the shot start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpokenWord {
    pub start: f64,
    pub end: f64,
    pub ipa: String,
}

/// An acoustically aligned phone, in seconds on the narration clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phoneme {
    pub ipa: String,
    pub start: f64,
    pub end: f64,
    pub confidence: f64,
}

/// Sample measured phoneme boundaries at frame centres, keeping pauses at rest.
#[must_use]
pub fn timed_plan(
    phones: &[Phoneme],
    speech: (f64, f64),
    start: f64,
    fps: u32,
    frames: u32,
    language: &str,
) -> Vec<Option<Viseme>> {
    let mut out: Vec<_> = (0..frames)
        .map(|frame| {
            let time = start + (f64::from(frame) + 0.5) / f64::from(fps.max(1));
            if time < speech.0 || time >= speech.1 {
                return None;
            }
            Some(
                phones
                    .iter()
                    .find(|p| time >= p.start && time < p.end)
                    .and_then(|p| from_ipa_for_language(&p.ipa, language).first().map(|s| s.0))
                    .unwrap_or(Viseme::Rest),
            )
        })
        .collect();
    hold(&mut out, 2);
    out
}

/// Mouth shape per frame. `None` is not speaking (the original artwork shows);
/// gaps between words inside `speech` are `Rest`. Runs shorter than `min_hold`
/// frames merge into their neighbour so the mouth never flickers.
#[must_use]
pub fn plan(
    words: &[SpokenWord],
    speech: (f64, f64),
    fps: u32,
    frames: u32,
    min_hold: u32,
) -> Vec<Option<Viseme>> {
    let fps = f64::from(fps);
    let mut segments: Vec<(f64, f64, Viseme)> = Vec::new();
    for word in words {
        let shapes = from_ipa(&word.ipa);
        let total: f64 = shapes.iter().map(|s| s.1).sum();
        if total <= 0.0 || word.end <= word.start {
            continue;
        }
        let mut t = word.start;
        for (shape, weight) in shapes {
            let length = (word.end - word.start) * weight / total;
            segments.push((t, t + length, shape));
            t += length;
        }
    }
    let mut out: Vec<Option<Viseme>> = (0..frames)
        .map(|i| {
            let t = (f64::from(i) + 0.5) / fps;
            if t < speech.0 || t >= speech.1 {
                return None;
            }
            Some(
                segments
                    .iter()
                    .find(|(a, b, _)| t >= *a && t < *b)
                    .map_or(Viseme::Rest, |s| s.2),
            )
        })
        .collect();
    hold(&mut out, min_hold as usize);
    out
}

fn hold(frames: &mut [Option<Viseme>], min: usize) {
    if min < 2 {
        return;
    }
    let mut i = 0;
    while i < frames.len() {
        let mut j = i;
        while j < frames.len() && frames[j] == frames[i] {
            j += 1;
        }
        if frames[i].is_some() && j - i < min {
            let fill = if i > 0 && frames[i - 1].is_some() {
                frames[i - 1]
            } else if j < frames.len() && frames[j].is_some() {
                frames[j]
            } else {
                None
            };
            frames[i..j].fill(fill);
        }
        i = j;
    }
}

/// Seeded blinks every 2–6 s: half, closed, closed, half (scaled to `fps`).
#[must_use]
pub fn blinks(frames: u32, fps: u32, seed: u64) -> Vec<Eyes> {
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let fps = fps.max(1);
    let unit = (fps / 24).max(1);
    let shape = [Eyes::Half, Eyes::Closed, Eyes::Closed, Eyes::Half];
    let mut out = vec![Eyes::Open; frames as usize];
    // The first blink comes 1–4 s in so short shots still blink sometimes.
    let mut at = u64::from(fps) + next() % (3 * u64::from(fps));
    while at < u64::from(frames) {
        for (k, eyes) in shape.iter().enumerate() {
            for u in 0..unit {
                let index = at + (k as u64) * u64::from(unit) + u64::from(u);
                if let Some(slot) = usize::try_from(index).ok().and_then(|i| out.get_mut(i)) {
                    *slot = *eyes;
                }
            }
        }
        at += 2 * u64::from(fps) + next() % (4 * u64::from(fps));
    }
    out
}

fn ipa_vowel_shape(c: char) -> Option<(Viseme, f64)> {
    const VOWELS: &[(Viseme, &[char])] = &[
        (Viseme::Ai, &['a', 'ɑ', 'ɐ', 'ʌ', 'æ', 'ə', 'ɜ', 'ɚ']),
        (Viseme::E, &['i', 'ɪ', 'e', 'ɛ', 'ɨ', 'y', 'ʏ', 'ø']),
        (Viseme::O, &['o', 'ɔ', 'ɒ', 'œ', 'ɵ']),
        (Viseme::U, &['u', 'ʊ', 'ɯ', 'w']),
    ];
    for (viseme_val, chars) in VOWELS {
        if chars.contains(&c) {
            return Some((*viseme_val, 2.0));
        }
    }
    None
}

fn ipa_consonant_shape(c: char) -> Option<(Viseme, f64)> {
    const CONSONANTS: &[(Viseme, &[char])] = &[
        (Viseme::Mbp, &['m', 'b', 'p', 'ɓ', 'ɱ']),
        (Viseme::Fv, &['f', 'v', 'ʋ']),
        (Viseme::L, &['l', 'ɭ', 'ɫ', 'ʎ']),
    ];
    for (viseme_val, chars) in CONSONANTS {
        if chars.contains(&c) {
            return Some((*viseme_val, 1.0));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipa_maps_to_mouth_shapes() {
        let shapes: Vec<_> = from_ipa("həlˈəʊ").into_iter().map(|s| s.0).collect();
        assert_eq!(
            shapes,
            [Viseme::E, Viseme::Ai, Viseme::L, Viseme::Ai, Viseme::U]
        );
        let urdu: Vec<_> = from_ipa("mˈẽ").into_iter().map(|s| s.0).collect();
        assert_eq!(urdu, [Viseme::Mbp, Viseme::E]);
        let long = from_ipa("aː");
        assert!((long[0].1 - 3.0).abs() < 1e-9);
    }

    #[test]
    fn plan_times_shapes_and_holds_two_frames() {
        let words = [SpokenWord {
            start: 0.0,
            end: 0.5,
            ipa: "maː".into(),
        }];
        let frames = plan(&words, (0.0, 1.0), 24, 30, 2);
        assert_eq!(frames.len(), 30);
        assert_eq!(frames[0], Some(Viseme::Mbp));
        assert_eq!(frames[8], Some(Viseme::Ai));
        assert_eq!(frames[20], Some(Viseme::Rest));
        assert_eq!(frames[25], None);
        for run in frames.chunk_by(|a, b| a == b) {
            assert!(run.len() >= 2 || run[0].is_none());
        }
    }

    #[test]
    fn acoustic_boundaries_and_silence_round_within_one_frame_at_nonzero_shot_start() {
        let phones = [
            Phoneme {
                ipa: "m".into(),
                start: 10.21,
                end: 10.42,
                confidence: 0.9,
            },
            Phoneme {
                ipa: "a".into(),
                start: 10.42,
                end: 10.79,
                confidence: 0.9,
            },
        ];
        for fps in [24, 25, 30, 60] {
            let frames = timed_plan(&phones, (10.21, 10.79), 10.0, fps, fps, "en");
            let first = frames.iter().position(Option::is_some).unwrap();
            let open = frames.iter().position(|v| *v == Some(Viseme::Ai)).unwrap();
            let end = frames.iter().rposition(Option::is_some).unwrap() + 1;
            for (frame, time) in [(first, 0.21), (open, 0.42), (end, 0.79)] {
                let actual = f64::from(u32::try_from(frame).unwrap()) / f64::from(fps);
                assert!((actual - time).abs() <= 1.0 / f64::from(fps));
            }
            assert!(
                frames
                    .chunk_by(|a, b| a == b)
                    .all(|run| run[0].is_none() || run.len() >= 2)
            );
        }
    }

    #[test]
    fn language_tables_map_indic_nasal_vowels_and_japanese_articulation() {
        for language in ["hi", "ur"] {
            let shapes: Vec<_> = from_ipa_for_language("mãĩõũ", language)
                .into_iter()
                .map(|v| v.0)
                .collect();
            assert_eq!(
                shapes,
                [Viseme::Mbp, Viseme::Ai, Viseme::E, Viseme::O, Viseme::U]
            );
        }
        let shapes: Vec<_> = from_ipa_for_language("ɸɯɾa", "ja-JP")
            .into_iter()
            .map(|v| v.0)
            .collect();
        assert_eq!(shapes, [Viseme::U, Viseme::E, Viseme::L, Viseme::Ai]);
    }

    #[test]
    fn blinks_are_seeded_and_spaced() {
        let a = blinks(24 * 20, 24, 7);
        assert_eq!(a, blinks(24 * 20, 24, 7));
        assert_ne!(a, blinks(24 * 20, 24, 8));
        let starts = a
            .windows(2)
            .filter(|w| w[0] == Eyes::Open && w[1] == Eyes::Half)
            .count();
        assert!((3..=10).contains(&starts), "{starts}");
    }
}
