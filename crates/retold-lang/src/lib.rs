//! Language logic with no network or model calls: sentences and speech time, packing units,
//! aligning words to audio, caption groups, font measurement and subtitle files.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod align;
pub mod captions;
pub mod fit;
pub mod profile;
pub mod script;
pub mod subtitles;
pub mod text;
pub mod units;

pub use align::{Alignment, SpokenWord, TimedUnit, TimedWord, align_for_lang, align_with_profile};
pub use captions::Caption;
pub use profile::{
    ArabicProfile, GenericProfile, JapaneseProfile, LanguageProfile, LanguageRegistry,
    SpanishProfile, TurkishProfile, profile_for, profile_for_code, tokenize_subtitles,
};
pub use subtitles::Cue;
