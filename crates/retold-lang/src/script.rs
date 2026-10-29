//! Which writing system a piece of text is in, so a transliteration can be checked in code
//! instead of trusted.

use retold_core::WritingScript;

/// The script a letter belongs to, by Unicode block. Marks, digits and punctuation have none.
const SCRIPT_RANGES: &[(std::ops::RangeInclusive<u32>, WritingScript)] = &[
    (0x0041..=0x024F, WritingScript::Latn),
    (0x1E00..=0x1EFF, WritingScript::Latn),
    (0x0900..=0x097F, WritingScript::Deva),
    (0xA8E0..=0xA8FF, WritingScript::Deva),
    (0x0600..=0x06FF, WritingScript::Arab),
    (0x0750..=0x077F, WritingScript::Arab),
    (0x08A0..=0x08FF, WritingScript::Arab),
    (0xFB50..=0xFDFF, WritingScript::Arab),
    (0xFE70..=0xFEFF, WritingScript::Arab),
    (0x0980..=0x09FF, WritingScript::Beng),
    (0x0A00..=0x0A7F, WritingScript::Guru),
    (0x0400..=0x04FF, WritingScript::Cyrl),
    (0x3040..=0x30FF, WritingScript::Jpan),
    (0xAC00..=0xD7AF, WritingScript::Kore),
    (0x1100..=0x11FF, WritingScript::Kore),
    (0x4E00..=0x9FFF, WritingScript::Hans),
];

#[must_use]
pub fn script_of(c: char) -> Option<WritingScript> {
    if !c.is_alphabetic() {
        return None;
    }
    let code_point = u32::from(c);
    SCRIPT_RANGES
        .iter()
        .find(|(range, ..)| range.contains(&code_point))
        .map(|(_, script_val)| *script_val)
}

/// Whether every letter in `word` is written in `script`. Words without letters (numbers,
/// punctuation) pass. Chinese and Japanese share Han characters, so either accepts them.
#[must_use]
pub fn is_written_in(word: &str, script: WritingScript) -> bool {
    word.chars().filter_map(script_of).all(|s| {
        s == script
            || (matches!(script, WritingScript::Jpan | WritingScript::Hant)
                && s == WritingScript::Hans)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_scripts_per_letter() {
        assert!(is_written_in("एक", WritingScript::Deva));
        assert!(is_written_in("पिशाच।", WritingScript::Deva));
        assert!(
            !is_written_in("एک", WritingScript::Deva),
            "mixed Devanagari and Urdu"
        );
        assert!(!is_written_in("ویمپائر", WritingScript::Deva));
        assert!(is_written_in("ویمپائر", WritingScript::Arab));
        assert!(is_written_in("Hutter's,", WritingScript::Latn));
        assert!(is_written_in("1922", WritingScript::Deva));
    }
}
