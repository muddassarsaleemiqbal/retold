//! Universal Pluggable Language Architecture: profiles, Bunsetsu tokenization,
//! and script-specific normalization for CJK, Arabic, Spanish, Turkish, and generic languages.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use retold_core::{LanguageTag, WritingScript};
use unicode_normalization::UnicodeNormalization;

/// A pluggable language profile defining tokenization, normalization, and caption rendering rules.
pub trait LanguageProfile: Send + Sync {
    /// Primary BCP-47 language subtag (e.g. "ja", "ar", "es", "tr", "en").
    fn language_code(&self) -> &str;

    /// The associated writing script.
    fn script(&self) -> WritingScript;

    /// Tokenizes text into discrete units for subtitles and audio-text alignment.
    /// Handles CJK non-whitespace Bunsetsu segmentation and whitespace scripts alike.
    fn tokenize_subtitles(&self, text: &str) -> Vec<String>;

    /// Normalizes/folds a token for speech-to-text transcript alignment.
    fn fold_token(&self, token: &str) -> String;

    /// Joins words into a single line for display (e.g. " " for Latin/Arabic, "" for CJK).
    fn join_tokens(&self, tokens: &[&str]) -> String;

    /// Estimates the word/token count for timing calculations.
    fn count_words(&self, text: &str) -> usize;

    /// Whether this language is written without spaces between words (CJK).
    fn is_cjk(&self) -> bool {
        matches!(
            self.script(),
            WritingScript::Jpan | WritingScript::Hans | WritingScript::Hant
        )
    }
}

// ---------------------------------------------------------------------------
// Japanese Profile (CJK Bunsetsu / Non-Whitespace Tokenization)
// ---------------------------------------------------------------------------

/// Japanese language profile with Bunsetsu particle-aware chunking and CJK layout.
#[derive(Debug, Default, Clone, Copy)]
pub struct JapaneseProfile;

impl JapaneseProfile {
    const PARTICLES: &'static [&'static str] = &[
        "でした",
        "ました",
        "ません",
        "ている",
        "ていた",
        "である",
        "から",
        "まで",
        "より",
        "ので",
        "のに",
        "ても",
        "でも",
        "です",
        "ます",
        "は",
        "が",
        "を",
        "に",
        "で",
        "と",
        "へ",
        "も",
        "の",
        "ね",
        "よ",
        "か",
        "だ",
    ];

    const PUNCT: &'static [char] = &[
        '。', '、', '！', '？', '・', '…', '\n', ' ', '\u{3000}', '「', '」', '『', '』',
    ];

    const fn is_kanji(c: char) -> bool {
        matches!(c as u32, 0x4E00..=0x9FFF | 0x3005)
    }

    const fn is_katakana(c: char) -> bool {
        matches!(c as u32, 0x30A0..=0x30FF)
    }

    fn should_split_particle(current: &str, chars: &[char], i: usize) -> bool {
        let next_is_boundary = chars.get(i + 1).is_none_or(|&next| {
            Self::is_kanji(next)
                || Self::is_katakana(next)
                || Self::PUNCT.contains(&next)
                || current.chars().count() >= 4
        });
        next_is_boundary
            && Self::PARTICLES
                .iter()
                .any(|p| current.ends_with(p) && current.chars().count() > p.chars().count())
    }
}

impl LanguageProfile for JapaneseProfile {
    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "GenericProfile returns its owned code"
    )]
    fn language_code(&self) -> &str {
        "ja"
    }

    fn script(&self) -> WritingScript {
        WritingScript::Jpan
    }

    fn tokenize_subtitles(&self, text: &str) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut current = String::new();
        let n = chars.len();
        let mut i = 0;

        while i < n {
            let c = chars[i];
            current.push(c);

            if Self::PUNCT.contains(&c) {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    tokens.push(trimmed);
                }
                current.clear();
                i += 1;
                continue;
            }

            if Self::should_split_particle(&current, &chars, i) {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    tokens.push(trimmed);
                }
                current.clear();
                i += 1;
                continue;
            }

            if current.chars().count() >= 7 && i + 1 < n {
                let next = chars[i + 1];
                if Self::is_kanji(next) || Self::is_katakana(next) {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        tokens.push(trimmed);
                    }
                    current.clear();
                }
            }

            i += 1;
        }

        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            tokens.push(trimmed);
        }
        tokens
    }

    fn fold_token(&self, token: &str) -> String {
        token
            .nfd()
            .filter(|c| !Self::PUNCT.contains(c) && !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .nfc()
            .collect()
    }

    fn join_tokens(&self, tokens: &[&str]) -> String {
        tokens.concat()
    }

    fn count_words(&self, text: &str) -> usize {
        self.tokenize_subtitles(text).len().max(1)
    }
}

// ---------------------------------------------------------------------------
// Arabic Profile (Right-to-Left, Tashkeel & Alef Normalization)
// ---------------------------------------------------------------------------

/// Arabic language profile with Tashkeel folding and Alef normalization.
#[derive(Debug, Default, Clone, Copy)]
pub struct ArabicProfile;

impl ArabicProfile {
    const PUNCT: &'static [char] = &[
        '،', '؛', '؟', '.', '!', ':', '\"', '(', ')', '[', ']', '—', '-',
    ];

    const fn is_harakat(c: char) -> bool {
        matches!(c, '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{0640}')
    }

    const fn normalize_arabic_char(c: char) -> char {
        match c {
            '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0671}' => '\u{0627}', // Alef forms -> bare Alef
            '\u{0629}' => '\u{0647}',                                        // Ta Marbuta -> Ha
            '\u{0649}' => '\u{064A}',                                        // Alef Maksura -> Ya
            other => other,
        }
    }
}

impl LanguageProfile for ArabicProfile {
    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "GenericProfile returns its owned code"
    )]
    fn language_code(&self) -> &str {
        "ar"
    }

    fn script(&self) -> WritingScript {
        WritingScript::Arab
    }

    fn tokenize_subtitles(&self, text: &str) -> Vec<String> {
        text.split_whitespace().map(ToString::to_string).collect()
    }

    fn fold_token(&self, token: &str) -> String {
        token
            .nfd()
            .filter(|c| !Self::is_harakat(*c) && !Self::PUNCT.contains(c) && !c.is_whitespace())
            .map(Self::normalize_arabic_char)
            .collect::<String>()
            .nfc()
            .collect()
    }

    fn join_tokens(&self, tokens: &[&str]) -> String {
        tokens.join(" ")
    }

    fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

// ---------------------------------------------------------------------------
// Spanish Profile (Inverted Punctuation & Accent Folding)
// ---------------------------------------------------------------------------

/// Spanish language profile handling inverted punctuation and diacritical accents.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpanishProfile;

impl SpanishProfile {
    const PUNCT: &'static [char] = &[
        '¿', '¡', '?', '!', '.', ',', ';', ':', '«', '»', '\"', '(', ')',
    ];
}

impl LanguageProfile for SpanishProfile {
    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "GenericProfile returns its owned code"
    )]
    fn language_code(&self) -> &str {
        "es"
    }

    fn script(&self) -> WritingScript {
        WritingScript::Latn
    }

    fn tokenize_subtitles(&self, text: &str) -> Vec<String> {
        text.split_whitespace().map(ToString::to_string).collect()
    }

    fn fold_token(&self, token: &str) -> String {
        token
            .nfd()
            .filter(|c| !matches!(c, '\u{0300}'..='\u{036F}'))
            .filter(|c| !Self::PUNCT.contains(c) && !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .nfc()
            .collect()
    }

    fn join_tokens(&self, tokens: &[&str]) -> String {
        tokens.join(" ")
    }

    fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

// ---------------------------------------------------------------------------
// Turkish Profile (Dotted/Dotless I Normalization)
// ---------------------------------------------------------------------------

/// Turkish language profile handling dotted 'İ'/'i' and dotless 'I'/'ı'.
#[derive(Debug, Default, Clone, Copy)]
pub struct TurkishProfile;

impl TurkishProfile {
    const PUNCT: &'static [char] = &[
        '.', ',', '!', '?', ';', ':', '\'', '\"', '(', ')', '[', ']', '—', '-',
    ];

    const fn fold_turkish_char(c: char) -> char {
        match c {
            '\u{0130}' => 'i',        // Dotted I -> i
            '\u{0049}' => '\u{0131}', // Dotless I -> ı
            other => other.to_ascii_lowercase(),
        }
    }
}

impl LanguageProfile for TurkishProfile {
    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "GenericProfile returns its owned code"
    )]
    fn language_code(&self) -> &str {
        "tr"
    }

    fn script(&self) -> WritingScript {
        WritingScript::Latn
    }

    fn tokenize_subtitles(&self, text: &str) -> Vec<String> {
        text.split_whitespace().map(ToString::to_string).collect()
    }

    fn fold_token(&self, token: &str) -> String {
        token
            .chars()
            .filter(|c| !Self::PUNCT.contains(c) && !c.is_whitespace())
            .map(Self::fold_turkish_char)
            .collect()
    }

    fn join_tokens(&self, tokens: &[&str]) -> String {
        tokens.join(" ")
    }

    fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

// ---------------------------------------------------------------------------
// Generic Profile (Fallback for Latin, Devanagari, Gurmukhi, etc.)
// ---------------------------------------------------------------------------

/// Generic language profile for whitespace-delimited scripts.
#[derive(Debug, Clone)]
pub struct GenericProfile {
    pub code: String,
    pub script: WritingScript,
}

impl Default for GenericProfile {
    fn default() -> Self {
        Self {
            code: "en".to_string(),
            script: WritingScript::Latn,
        }
    }
}

const fn is_folded_mark(c: char) -> bool {
    matches!(
        c,
        '\u{0901}' | '\u{0902}' | '\u{093C}' | '\u{094D}' | '\u{064B}'..='\u{065F}' | '\u{0670}'
    )
}

impl LanguageProfile for GenericProfile {
    fn language_code(&self) -> &str {
        &self.code
    }

    fn script(&self) -> WritingScript {
        self.script
    }

    fn tokenize_subtitles(&self, text: &str) -> Vec<String> {
        text.split_whitespace().map(ToString::to_string).collect()
    }

    fn fold_token(&self, token: &str) -> String {
        token
            .nfd()
            .filter(|c| !is_folded_mark(*c))
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .nfc()
            .collect()
    }

    fn join_tokens(&self, tokens: &[&str]) -> String {
        tokens.join(" ")
    }

    fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

// ---------------------------------------------------------------------------
// Dynamic Language Registry
// ---------------------------------------------------------------------------

/// Dynamic registry of language profiles.
pub struct LanguageRegistry {
    profiles: RwLock<HashMap<String, Arc<dyn LanguageProfile>>>,
}

impl std::fmt::Debug for LanguageRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageRegistry").finish_non_exhaustive()
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        let mut map: HashMap<String, Arc<dyn LanguageProfile>> = HashMap::new();
        map.insert("ja".to_string(), Arc::new(JapaneseProfile));
        map.insert("ar".to_string(), Arc::new(ArabicProfile));
        map.insert("es".to_string(), Arc::new(SpanishProfile));
        map.insert("tr".to_string(), Arc::new(TurkishProfile));
        map.insert(
            "en".to_string(),
            Arc::new(GenericProfile {
                code: "en".into(),
                script: WritingScript::Latn,
            }),
        );
        map.insert(
            "hi".to_string(),
            Arc::new(GenericProfile {
                code: "hi".into(),
                script: WritingScript::Deva,
            }),
        );
        map.insert(
            "ur".to_string(),
            Arc::new(GenericProfile {
                code: "ur".into(),
                script: WritingScript::Arab,
            }),
        );

        Self {
            profiles: RwLock::new(map),
        }
    }
}

impl LanguageRegistry {
    /// Create a new language registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a custom language profile.
    pub fn register(&self, profile: Arc<dyn LanguageProfile>) {
        let code = profile.language_code().to_string();
        if let Ok(mut lock) = self.profiles.write() {
            lock.insert(code, profile);
        }
    }

    /// Retrieve the profile for a language tag, falling back to script detection if unknown.
    #[must_use]
    pub fn profile_for(&self, tag: &LanguageTag) -> Arc<dyn LanguageProfile> {
        self.profile_for_code(tag.language())
    }

    /// Retrieve the profile by primary BCP-47 language subtag.
    #[must_use]
    pub fn profile_for_code(&self, code: &str) -> Arc<dyn LanguageProfile> {
        let lower = code.to_ascii_lowercase();
        if let Some(p) = self
            .profiles
            .read()
            .ok()
            .and_then(|l| l.get(&lower).cloned())
        {
            return p;
        }

        // Script-based fallback
        match lower.as_str() {
            "ja" => Arc::new(JapaneseProfile),
            "ar" => Arc::new(ArabicProfile),
            _ => match lower.as_str() {
                "es" => Arc::new(SpanishProfile),
                "tr" => Arc::new(TurkishProfile),
                _ => Arc::new(GenericProfile {
                    code: lower,
                    script: WritingScript::Latn,
                }),
            },
        }
    }
}

/// Global registry instance.
#[must_use]
pub fn global_registry() -> &'static LanguageRegistry {
    static REGISTRY: OnceLock<LanguageRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LanguageRegistry::default)
}

/// Helper function to retrieve the profile for a given language tag.
#[must_use]
pub fn profile_for(tag: &LanguageTag) -> Arc<dyn LanguageProfile> {
    global_registry().profile_for(tag)
}

/// Helper function to retrieve the profile by language code string.
#[must_use]
pub fn profile_for_code(code: &str) -> Arc<dyn LanguageProfile> {
    global_registry().profile_for_code(code)
}

/// Convenience function to tokenize text for subtitles according to language tag.
#[must_use]
pub fn tokenize_subtitles(text: &str, tag: &LanguageTag) -> Vec<String> {
    profile_for(tag).tokenize_subtitles(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn japanese_bunsetsu_tokenization() {
        let profile = JapaneseProfile;
        let text = "昔々、ある所に心優しいおじいさんとおばあさんが住んでいました。";
        let tokens = profile.tokenize_subtitles(text);
        assert_ne!(tokens.len(), 0);
        assert!(tokens.len() >= 4, "Must segment into Bunsetsu phrases");
        let line = profile.join_tokens(&tokens.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(
            line, text,
            "Joined Japanese tokens must equal original text"
        );
    }

    #[test]
    fn arabic_harakat_and_alef_folding() {
        let profile = ArabicProfile;
        let with_tashkeel = "فِي يَوْمٍ مِنَ الأَيَّامِ";
        let folded = profile.fold_token(with_tashkeel);
        assert_eq!(folded, "فييوممنالايام");
    }

    #[test]
    fn spanish_accent_and_inverted_punctuation_folding() {
        let profile = SpanishProfile;
        let text = "¡Hola mundo! ¿Cómo estás?";
        let tokens = profile.tokenize_subtitles(text);
        assert_eq!(tokens.len(), 4);
        assert_eq!(profile.fold_token("¿Cómo"), "como");
        assert_eq!(profile.fold_token("estás?"), "estas");
    }

    #[test]
    fn turkish_dotted_and_dotless_i_handling() {
        let profile = TurkishProfile;
        assert_eq!(profile.fold_token("İSTANBUL"), "istanbul");
        assert_eq!(profile.fold_token("ışık"), "ışık");
    }

    #[test]
    fn registry_resolves_and_caches() {
        let tag = LanguageTag::from_str("ja").unwrap();
        let p = profile_for(&tag);
        assert_eq!(p.language_code(), "ja");
        assert!(p.is_cjk());
    }
}
