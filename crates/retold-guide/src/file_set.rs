//! Every guide origin uses the same bounded, portable set of text filenames.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::LoadError;

pub const MAX_ENTRIES: usize = 4096;
pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_DEPTH: usize = 12;

pub fn validate(files: &BTreeMap<String, String>) -> Result<(), LoadError> {
    validate_count(files.len())?;
    files
        .iter()
        .try_fold(
            Inventory {
                names: BTreeSet::new(),
                remaining: MAX_BYTES,
            },
            |inventory, (name, text)| inventory.file(name, text),
        )
        .map(|_| ())
}

struct Inventory {
    names: BTreeSet<String>,
    remaining: u64,
}

impl Inventory {
    fn file(mut self, name: &str, text: &str) -> Result<Self, LoadError> {
        validate_name(name)?;
        let unique = self.names.insert(name.to_lowercase());
        if !unique {
            return Err(invalid(
                "Guide filenames must not collide when case is ignored.",
            ));
        }
        self.remaining = account_bytes(account_bytes(self.remaining, name)?, text)?;
        Ok(self)
    }
}

fn validate_name(name: &str) -> Result<(), LoadError> {
    let path = Path::new(name);
    let portable = crate::guide::relative_path(name);
    let supported = crate::guide::is_text(path);
    let bounded = path.components().count() <= MAX_DEPTH;
    if portable && supported && bounded {
        for component in name.split('/') {
            validate_component(component)?;
        }
        Ok(())
    } else {
        Err(invalid(
            "Use bounded, portable relative text filenames without traversal or control characters.",
        ))
    }
}

fn validate_count(count: usize) -> Result<(), LoadError> {
    if count > MAX_ENTRIES {
        Err(invalid("Guide has too many text files."))
    } else {
        Ok(())
    }
}

fn validate_component(name: &str) -> Result<(), LoadError> {
    let boundary = has_unsafe_boundary(name);
    let punctuation = name.contains(['<', '>', '"', '|', '?', '*']);
    let device = is_device_name(name);
    if boundary || punctuation || device {
        Err(invalid(
            "Guide filename components must be portable and visible on all supported platforms.",
        ))
    } else {
        Ok(())
    }
}

fn has_unsafe_boundary(name: &str) -> bool {
    name.is_empty() || name.starts_with('.') || name.ends_with(['.', ' '])
}

fn is_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let port = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"));
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || matches!(
            port,
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        )
}

fn account_bytes(remaining: u64, text: &str) -> Result<u64, LoadError> {
    let size = u64::try_from(text.len()).unwrap_or(u64::MAX);
    remaining
        .checked_sub(size)
        .ok_or_else(|| invalid("Guide exceeds the 16 MB text limit."))
}

fn invalid(message: &str) -> LoadError {
    LoadError::invalid("guide files", vec![message.into()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Guide, Origin, catalog::bundled};

    fn story_files() -> BTreeMap<String, String> {
        bundled()
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .files()
            .clone()
    }

    #[test]
    fn all_origins_reject_escaping_or_nonportable_snapshot_names() {
        for name in [
            "/outside.txt",
            "../outside.txt",
            "a/../../outside.txt",
            "a\\outside.txt",
            "C:outside.txt",
            "bad\n.txt",
            ".hidden.txt",
            "a/.hidden/b.txt",
            "CON.txt",
            "LPT1.txt",
            "a./b.txt",
            "a /b.txt",
            "a//b.txt",
            "bad?.txt",
        ] {
            let mut files = story_files();
            files.insert(name.into(), "untrusted".into());
            assert!(
                Guide::from_files(Origin::Inline, files).is_err(),
                "{name:?}"
            );
        }
    }

    #[test]
    fn case_collisions_and_excessive_depth_are_rejected() {
        let mut files = story_files();
        files.insert("VOICE.md".into(), "different file on some platforms".into());
        assert!(Guide::from_files(Origin::Inline, files).is_err());
        let mut files = story_files();
        files.insert(
            format!("{}extra.txt", "nested/".repeat(MAX_DEPTH)),
            "too deep".into(),
        );
        assert!(Guide::from_files(Origin::Inline, files).is_err());
    }

    #[test]
    fn inline_guides_obey_the_same_byte_limit_as_disk_guides() {
        let mut files = story_files();
        files.insert(
            "extra.txt".into(),
            "x".repeat(usize::try_from(MAX_BYTES).unwrap()),
        );
        assert!(Guide::from_files(Origin::Inline, files).is_err());
    }
}
