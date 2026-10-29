//! Capability tiers: what a pipeline role needs from a provider. Guides name a tier for each
//! role, and the router maps each tier to a chain of providers.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[non_exhaustive]
pub enum Tier {
    #[serde(rename = "text.fast")]
    TextFast,
    #[serde(rename = "text.smart")]
    TextSmart,
    #[serde(rename = "text.best")]
    TextBest,
    /// Text inference on this machine; cloud fallbacks are forbidden.
    #[serde(rename = "text.local")]
    TextLocal,
    #[serde(rename = "vision.fast")]
    VisionFast,
    #[serde(rename = "vision.smart")]
    VisionSmart,
    #[serde(rename = "image")]
    Image,
    /// Images with no reference pictures (frames without characters).
    #[serde(rename = "image.no_refs")]
    ImageNoRefs,
    #[serde(rename = "tts")]
    Tts,
    #[serde(rename = "transcribe")]
    Transcribe,
}

/// The kind of work a tier asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Text,
    Vision,
    Image,
    Tts,
    Transcribe,
}

impl Tier {
    /// The tier for repair attempt `pass` after a failed automatic check: the first repair
    /// keeps the guide's tier (the named problems usually suffice), later ones move up one
    /// step, so a guide can start cheap without risking a run on a weak model.
    #[must_use]
    pub const fn for_repair(self, pass: u32) -> Self {
        if pass == 0 {
            return self;
        }
        match self {
            Self::TextFast => Self::TextSmart,
            Self::TextSmart => Self::TextBest,
            Self::VisionFast => Self::VisionSmart,
            other => other,
        }
    }

    pub const ALL: [Self; 10] = [
        Self::TextFast,
        Self::TextSmart,
        Self::TextBest,
        Self::TextLocal,
        Self::VisionFast,
        Self::VisionSmart,
        Self::Image,
        Self::ImageNoRefs,
        Self::Tts,
        Self::Transcribe,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        let meta = [
            (Self::TextFast, "text.fast"),
            (Self::TextSmart, "text.smart"),
            (Self::TextBest, "text.best"),
            (Self::TextLocal, "text.local"),
            (Self::VisionFast, "vision.fast"),
            (Self::VisionSmart, "vision.smart"),
            (Self::Image, "image"),
            (Self::ImageNoRefs, "image.no_refs"),
            (Self::Tts, "tts"),
            (Self::Transcribe, "transcribe"),
        ];
        meta.iter()
            .find(|(tier_entry, ..)| *tier_entry == self)
            .map_or("", |(_, name_str)| *name_str)
    }

    #[must_use]
    pub fn capability(self) -> Capability {
        let caps = [
            (Self::VisionFast, Capability::Vision),
            (Self::VisionSmart, Capability::Vision),
            (Self::Image, Capability::Image),
            (Self::ImageNoRefs, Capability::Image),
            (Self::Tts, Capability::Tts),
            (Self::Transcribe, Capability::Transcribe),
        ];
        caps.iter()
            .find(|(tier_entry, ..)| *tier_entry == self)
            .map_or(Capability::Text, |(_, cap)| *cap)
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "unknown tier `{0}` (expected one of: text.fast, text.smart, text.best, text.local, vision.fast, vision.smart, image, image.no_refs, tts, transcribe)"
)]
pub struct TierParseError(String);

impl FromStr for Tier {
    type Err = TierParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|t| t.as_str() == s)
            .ok_or_else(|| TierParseError(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_forms_agree_with_serde() {
        for t in Tier::ALL {
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(json, format!("\"{t}\""));
            assert_eq!(t.as_str().parse::<Tier>().unwrap(), t);
        }
    }
}

#[cfg(test)]
mod repair_tests {
    use super::Tier;

    #[test]
    fn repairs_escalate_one_step_after_the_first() {
        assert_eq!(Tier::TextFast.for_repair(0), Tier::TextFast);
        assert_eq!(Tier::TextFast.for_repair(1), Tier::TextSmart);
        assert_eq!(Tier::TextSmart.for_repair(2), Tier::TextBest);
        assert_eq!(Tier::TextLocal.for_repair(3), Tier::TextLocal);
    }
}
