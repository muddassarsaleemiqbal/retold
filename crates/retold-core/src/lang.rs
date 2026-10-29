//! Languages and writing systems.
//!
//! A [`LanguageTag`] is a small, validated subset of BCP-47: `language[-Script][-REGION]`,
//! for example `en`, `hi`, `ur-Latn` (Roman Urdu) or `pt-BR`. Every tag resolves to a
//! [`WritingScript`], which decides fonts, text direction, casing and line breaking.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A writing system (ISO 15924 code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum WritingScript {
    Latn,
    Deva,
    Arab,
    Beng,
    Guru,
    Cyrl,
    Hans,
    Hant,
    Jpan,
    Kore,
}

/// Direction text runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

impl WritingScript {
    pub const ALL: [Self; 10] = [
        Self::Latn,
        Self::Deva,
        Self::Arab,
        Self::Beng,
        Self::Guru,
        Self::Cyrl,
        Self::Hans,
        Self::Hant,
        Self::Jpan,
        Self::Kore,
    ];

    const METADATA: [(Self, &'static str, &'static str); 10] = [
        (Self::Latn, "Latn", "Latin"),
        (Self::Deva, "Deva", "Devanagari"),
        (Self::Arab, "Arab", "Arabic"),
        (Self::Beng, "Beng", "Bengali"),
        (Self::Guru, "Guru", "Gurmukhi"),
        (Self::Cyrl, "Cyrl", "Cyrillic"),
        (Self::Hans, "Hans", "Simplified Chinese"),
        (Self::Hant, "Hant", "Traditional Chinese"),
        (Self::Jpan, "Jpan", "Japanese"),
        (Self::Kore, "Kore", "Korean"),
    ];

    /// The four-letter ISO 15924 code.
    #[must_use]
    pub fn code(self) -> &'static str {
        Self::METADATA
            .iter()
            .find(|(script_entry, ..)| *script_entry == self)
            .map_or("", |(_, script_code, _)| *script_code)
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        Self::METADATA
            .iter()
            .find(|(script_entry, ..)| *script_entry == self)
            .map_or("", |(.., script_name)| *script_name)
    }

    #[must_use]
    pub const fn direction(self) -> Direction {
        match self {
            Self::Arab => Direction::Rtl,
            _ => Direction::Ltr,
        }
    }

    /// Whether the script has upper and lower case (so `uppercase` captions apply).
    #[must_use]
    pub const fn has_case(self) -> bool {
        matches!(self, Self::Latn | Self::Cyrl)
    }

    fn parse(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|s| s.code().eq_ignore_ascii_case(code))
    }
}

impl fmt::Display for WritingScript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for WritingScript {
    type Err = LanguageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| LanguageError::UnknownScript(s.to_owned()))
    }
}

impl Serialize for WritingScript {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for WritingScript {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for WritingScript {
    fn schema_name() -> String {
        "WritingScript".to_owned()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(Self::ALL.iter().map(|s| s.code().into()).collect()),
            ..Default::default()
        }
        .into()
    }
}

/// Languages Retold knows by name, with their usual script.
const KNOWN: &[(&str, &str, WritingScript)] = &[
    ("en", "English", WritingScript::Latn),
    ("hi", "Hindi", WritingScript::Deva),
    ("ur", "Urdu", WritingScript::Arab),
    ("ar", "Arabic", WritingScript::Arab),
    ("fa", "Persian", WritingScript::Arab),
    ("bn", "Bengali", WritingScript::Beng),
    ("pa", "Punjabi", WritingScript::Guru),
    ("mr", "Marathi", WritingScript::Deva),
    ("ne", "Nepali", WritingScript::Deva),
    ("es", "Spanish", WritingScript::Latn),
    ("fr", "French", WritingScript::Latn),
    ("de", "German", WritingScript::Latn),
    ("it", "Italian", WritingScript::Latn),
    ("pt", "Portuguese", WritingScript::Latn),
    ("tr", "Turkish", WritingScript::Latn),
    ("id", "Indonesian", WritingScript::Latn),
    ("ms", "Malay", WritingScript::Latn),
    ("sw", "Swahili", WritingScript::Latn),
    ("ru", "Russian", WritingScript::Cyrl),
    ("uk", "Ukrainian", WritingScript::Cyrl),
    ("zh", "Chinese", WritingScript::Hans),
    ("ja", "Japanese", WritingScript::Jpan),
    ("ko", "Korean", WritingScript::Kore),
];

/// A language such as `en`, `hi` or `ur-Latn`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LanguageTag {
    language: String,
    script: WritingScript,
    explicit_script: bool,
    region: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LanguageError {
    #[error("`{0}` is not a language tag (expected something like `en`, `hi` or `ur-Latn`)")]
    Malformed(String),
    #[error(
        "unknown script `{0}` (supported: Latn, Deva, Arab, Beng, Guru, Cyrl, Hans, Hant, Jpan, Kore)"
    )]
    UnknownScript(String),
    #[error("Retold does not know which script `{0}` is written in; add one, e.g. `{0}-Latn`")]
    NeedsScript(String),
}

impl LanguageTag {
    /// The primary language subtag, lowercase (`ur` for `ur-Latn`).
    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    #[must_use]
    pub const fn script(&self) -> WritingScript {
        self.script
    }

    #[must_use]
    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    /// English name, with the script when it is not the usual one ("Urdu (Latin)").
    #[must_use]
    pub fn display_name(&self) -> String {
        let base = KNOWN
            .iter()
            .find(|(code, _, _)| *code == self.language)
            .map_or_else(|| self.language.clone(), |(_, name, _)| (*name).to_owned());
        if self.explicit_script {
            format!("{base} ({})", self.script.name())
        } else {
            base
        }
    }

    /// Whether two tags are the same spoken language, ignoring script and region. Hindi and
    /// Urdu count as one spoken language (Hindustani), so one can caption the other by
    /// transliteration.
    #[must_use]
    pub fn same_speech(&self, other: &Self) -> bool {
        let hindustani = |l: &str| matches!(l, "hi" | "ur");
        self.language == other.language
            || (hindustani(&self.language) && hindustani(&other.language))
    }

    /// The languages Retold offers in menus.
    pub fn suggestions() -> impl Iterator<Item = Self> {
        KNOWN.iter().map(|(code, _, script)| Self {
            language: (*code).to_owned(),
            script: *script,
            explicit_script: false,
            region: None,
        })
    }
}

impl FromStr for LanguageTag {
    type Err = LanguageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let malformed = || LanguageError::Malformed(s.to_owned());
        let mut parts = s.split(['-', '_']);
        let language = parts.next().ok_or_else(malformed)?.to_ascii_lowercase();
        if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_lowercase()) {
            return Err(malformed());
        }
        let mut script = None;
        let mut region = None;
        for part in parts {
            match part.len() {
                4 if script.is_none() && region.is_none() => script = Some(part.parse()?),
                2 if region.is_none() && part.bytes().all(|b| b.is_ascii_alphabetic()) => {
                    region = Some(part.to_ascii_uppercase());
                }
                3 if region.is_none() && part.bytes().all(|b| b.is_ascii_digit()) => {
                    region = Some(part.to_owned());
                }
                _ => return Err(malformed()),
            }
        }
        let explicit_script = script.is_some();
        let script = match script {
            Some(s) => s,
            None => KNOWN
                .iter()
                .find(|(code, _, _)| *code == language)
                .map(|(_, _, s)| *s)
                .ok_or_else(|| LanguageError::NeedsScript(language.clone()))?,
        };
        Ok(Self {
            language,
            script,
            explicit_script,
            region,
        })
    }
}

impl fmt::Display for LanguageTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.language)?;
        if self.explicit_script {
            write!(f, "-{}", self.script)?;
        }
        if let Some(r) = &self.region {
            write!(f, "-{r}")?;
        }
        Ok(())
    }
}

impl Serialize for LanguageTag {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LanguageTag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for LanguageTag {
    fn schema_name() -> String {
        "LanguageTag".to_owned()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let mut s = SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            ..Default::default()
        };
        s.metadata().description =
            Some("BCP-47 language tag: `en`, `hi`, `ur`, `ur-Latn` (Roman Urdu)…".to_owned());
        s.string().pattern = Some(r"^[a-z]{2,3}(-[A-Za-z]{4})?(-([A-Z]{2}|[0-9]{3}))?$".to_owned());
        s.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(s: &str) -> LanguageTag {
        s.parse().unwrap()
    }

    #[test]
    fn known_languages_get_their_usual_script() {
        assert_eq!(tag("en").script(), WritingScript::Latn);
        assert_eq!(tag("hi").script(), WritingScript::Deva);
        assert_eq!(tag("ur").script(), WritingScript::Arab);
        assert_eq!(tag("ur").script().direction(), Direction::Rtl);
    }

    #[test]
    fn explicit_script_wins_and_round_trips() {
        let t = tag("ur-latn");
        assert_eq!(t.script(), WritingScript::Latn);
        assert_eq!(t.to_string(), "ur-Latn");
        assert_eq!(t.display_name(), "Urdu (Latin)");
        assert_eq!(tag("pt_br").to_string(), "pt-BR");
    }

    #[test]
    fn unknown_language_needs_a_script() {
        assert!(matches!(
            "xx".parse::<LanguageTag>(),
            Err(LanguageError::NeedsScript(_))
        ));
        assert_eq!(tag("xx-Latn").script(), WritingScript::Latn);
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "e", "english", "en-Xxxx", "en-US-Latn", "1n"] {
            assert!(bad.parse::<LanguageTag>().is_err(), "{bad}");
        }
    }

    #[test]
    fn hindi_and_urdu_share_speech() {
        assert!(tag("ur").same_speech(&tag("hi")));
        assert!(tag("ur-Latn").same_speech(&tag("ur")));
        assert!(!tag("en").same_speech(&tag("hi")));
    }
}
