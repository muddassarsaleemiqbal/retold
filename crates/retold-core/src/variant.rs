//! Language variants: one narration language plus its caption tracks.
//!
//! The command-line spec is `<voice>[:<captions>]`, where captions are comma-separated and a
//! track ending in `.srt` or `.vtt` is a sidecar file instead of burned-in text:
//!
//! | Spec | Meaning |
//! | --- | --- |
//! | `en` | English voice, English captions burned in |
//! | `ur:hi` | Urdu voice, Hindi (Devanagari) captions burned in |
//! | `hi:hi,en.srt` | Hindi voice, Hindi burned in, English `.srt` |
//! | `hi:en.srt` | Hindi voice, no burned captions, English `.srt` |

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::lang::{LanguageError, LanguageTag};

/// Subtitle file format for sidecar tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubtitleFormat {
    Srt,
    Vtt,
}

impl SubtitleFormat {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
        }
    }
}

/// How a caption track is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptionMode {
    /// Drawn into the video.
    Burned,
    /// Written as a subtitle file next to the video.
    Sidecar(SubtitleFormat),
}

/// One caption track of a variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaptionTrack {
    pub lang: LanguageTag,
    pub mode: CaptionMode,
}

/// How a caption track relates to the narration; decides how captions are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionKind {
    /// Same language and script: captions are the spoken words.
    Same,
    /// Same speech, different script (Urdu voice, Hindi captions): word-by-word
    /// transliteration, so word timings carry over.
    Transliteration,
    /// A different language: phrase-level translation shown for each phrase's time span.
    Translation,
}

/// One language edition of a video.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Variant {
    pub narration: LanguageTag,
    pub captions: Vec<CaptionTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VariantError {
    #[error(transparent)]
    Language(#[from] LanguageError),
    #[error("`{0}` has an empty caption entry")]
    EmptyCaption(String),
    #[error("`{0}` burns in more than one caption track; keep one and make the others `.srt`")]
    TwoBurned(String),
    #[error("`{0}` lists the same caption track twice")]
    Duplicate(String),
}

impl Variant {
    /// The burned-in track, if any.
    #[must_use]
    pub fn burned(&self) -> Option<&CaptionTrack> {
        self.captions.iter().find(|t| t.mode == CaptionMode::Burned)
    }

    /// How `track` relates to this variant's narration.
    #[must_use]
    pub fn caption_kind(&self, track: &CaptionTrack) -> CaptionKind {
        if !self.narration.same_speech(&track.lang) {
            CaptionKind::Translation
        } else if self.narration.script() == track.lang.script()
            && self.narration.language() == track.lang.language()
        {
            CaptionKind::Same
        } else {
            CaptionKind::Transliteration
        }
    }

    /// Plain-language description, e.g. `Urdu voice · Hindi captions · English .srt`.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts = vec![format!("{} voice", self.narration.display_name())];
        let mut burned = false;
        for track in &self.captions {
            match track.mode {
                CaptionMode::Burned => {
                    burned = true;
                    parts.push(format!("{} captions", track.lang.display_name()));
                }
                CaptionMode::Sidecar(format) => parts.push(format!(
                    "{} .{}",
                    track.lang.display_name(),
                    format.extension()
                )),
            }
        }
        if !burned {
            parts.insert(1, "no burned captions".into());
        }
        parts.join(" · ")
    }
}

fn parse_variant(spec: &str) -> Result<Variant, VariantError> {
    let spec = spec.trim();
    let (voice, captions) = match spec.split_once(':') {
        Some((v, c)) => (v, Some(c)),
        None => (spec, None),
    };
    let narration: LanguageTag = voice.parse()?;
    let captions = match captions {
        None => vec![CaptionTrack {
            lang: narration.clone(),
            mode: CaptionMode::Burned,
        }],
        Some(list) => list
            .split(',')
            .map(|item| parse_track(item.trim(), spec))
            .collect::<Result<Vec<_>, _>>()?,
    };
    if captions
        .iter()
        .filter(|t| t.mode == CaptionMode::Burned)
        .count()
        > 1
    {
        return Err(VariantError::TwoBurned(spec.to_owned()));
    }
    for (i, t) in captions.iter().enumerate() {
        if captions[..i].contains(t) {
            return Err(VariantError::Duplicate(spec.to_owned()));
        }
    }
    Ok(Variant {
        narration,
        captions,
    })
}

impl FromStr for Variant {
    type Err = VariantError;

    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        parse_variant(spec)
    }
}

fn parse_track(item: &str, spec: &str) -> Result<CaptionTrack, VariantError> {
    if item.is_empty() {
        return Err(VariantError::EmptyCaption(spec.to_owned()));
    }
    let (lang, mode) = if let Some(l) = item.strip_suffix(".srt") {
        (l, CaptionMode::Sidecar(SubtitleFormat::Srt))
    } else if let Some(l) = item.strip_suffix(".vtt") {
        (l, CaptionMode::Sidecar(SubtitleFormat::Vtt))
    } else {
        (item, CaptionMode::Burned)
    };
    Ok(CaptionTrack {
        lang: lang.parse()?,
        mode,
    })
}

impl fmt::Display for Variant {
    /// The canonical spec, so it can be pasted back into `--lang`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.narration)?;
        for (i, t) in self.captions.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}", t.lang)?;
            if let CaptionMode::Sidecar(fmt) = t.mode {
                write!(f, ".{}", fmt.extension())?;
            }
        }
        Ok(())
    }
}

impl Serialize for Variant {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Variant {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_editions_in_plain_language() {
        let d = |s: &str| s.parse::<Variant>().unwrap().describe();
        assert_eq!(d("en"), "English voice · English captions");
        assert_eq!(d("ur:hi"), "Urdu voice · Hindi captions");
        assert_eq!(
            d("hi:en.srt"),
            "Hindi voice · no burned captions · English .srt"
        );
    }

    fn v(s: &str) -> Variant {
        s.parse().unwrap()
    }

    #[test]
    fn bare_language_burns_its_own_captions() {
        assert_eq!(v("en").to_string(), "en:en");
    }

    #[test]
    fn parses_mixed_tracks() {
        let var = v("hi:hi,en.srt");
        assert_eq!(var.captions.len(), 2);
        assert_eq!(var.burned().map(|t| t.lang.to_string()), Some("hi".into()));
        assert_eq!(var.to_string(), "hi:hi,en.srt");
    }

    #[test]
    fn sidecar_only_is_allowed() {
        assert!(v("hi:en.srt").burned().is_none());
    }

    #[test]
    fn caption_kinds() {
        let ur_hi = v("ur:hi");
        assert_eq!(
            ur_hi.caption_kind(&ur_hi.captions[0]),
            CaptionKind::Transliteration
        );
        let roman = v("ur:ur-Latn");
        assert_eq!(
            roman.caption_kind(&roman.captions[0]),
            CaptionKind::Transliteration
        );
        let hi_en = v("hi:en");
        assert_eq!(
            hi_en.caption_kind(&hi_en.captions[0]),
            CaptionKind::Translation
        );
        let en = v("en");
        assert_eq!(en.caption_kind(&en.captions[0]), CaptionKind::Same);
    }

    #[test]
    fn rejects_bad_specs() {
        assert!(matches!(
            "hi:hi,en".parse::<Variant>(),
            Err(VariantError::TwoBurned(_))
        ));
        assert!(matches!(
            "hi:en.srt,en.srt".parse::<Variant>(),
            Err(VariantError::Duplicate(_))
        ));
        assert!(matches!(
            "hi:".parse::<Variant>(),
            Err(VariantError::EmptyCaption(_))
        ));
        assert!("zz:en".parse::<Variant>().is_err());
    }
}
