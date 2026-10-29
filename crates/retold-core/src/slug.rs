//! Slugs: the lowercase ASCII names used for folders, files and ids.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MAX_LEN: usize = 80;

/// Lowercase ASCII letters, digits and single dashes, e.g. `nosferatu-1922`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SlugError {
    #[error(
        "`{0}` is not a valid name: use lowercase letters, digits and dashes (like `test-studio`)"
    )]
    Invalid(String),
    #[error("`{0}` has no letters or digits to build a name from")]
    Empty(String),
}

impl Slug {
    /// Builds a slug from any text, transliterating non-Latin scripts
    /// ("Nosferatu (1922)" → `nosferatu-1922`, "दंगल" → `dangal`).
    pub fn from_title(text: &str) -> Result<Self, SlugError> {
        let ascii = deunicode::deunicode(text).to_ascii_lowercase();
        let mut out = String::with_capacity(ascii.len());
        for c in ascii.chars() {
            if c.is_ascii_alphanumeric() {
                out.push(c);
            } else if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        }
        let mut out = out.trim_end_matches('-').to_owned();
        if out.len() > MAX_LEN {
            out.truncate(MAX_LEN);
            out = out.trim_end_matches('-').to_owned();
        }
        if out.is_empty() {
            return Err(SlugError::Empty(text.to_owned()));
        }
        Ok(Self(out))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_LEN
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
}

impl FromStr for Slug {
    type Err = SlugError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if is_valid(s) {
            Ok(Self(s.to_owned()))
        } else {
            Err(SlugError::Invalid(s.to_owned()))
        }
    }
}

impl TryFrom<String> for Slug {
    type Error = SlugError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Slug> for String {
    fn from(s: Slug) -> Self {
        s.0
    }
}

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn titles_become_slugs() {
        let s = |t: &str| Slug::from_title(t).unwrap().to_string();
        assert_eq!(s("Nosferatu (1922)"), "nosferatu-1922");
        assert_eq!(s("  You — Season 1 "), "you-season-1");
        assert_eq!(s("Amélie"), "amelie");
        assert_ne!(s("दंगल"), "");
    }

    #[test]
    fn empty_titles_are_rejected() {
        assert!(Slug::from_title("!!!").is_err());
    }

    #[test]
    fn parse_is_strict() {
        assert!("test-studio".parse::<Slug>().is_ok());
        for bad in ["", "The", "a--b", "-a", "a-", "a b", "ä"] {
            assert!(bad.parse::<Slug>().is_err(), "{bad}");
        }
    }

    proptest! {
        #[test]
        fn from_title_always_valid(t in "\\PC{0,200}") {
            if let Ok(s) = Slug::from_title(&t) {
                prop_assert!(is_valid(s.as_str()), "{}", s);
            }
        }
    }
}
