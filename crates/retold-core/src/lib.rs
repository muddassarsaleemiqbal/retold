//! Domain types shared by every Retold crate.
//!
//! This crate does no I/O. Everything here is plain data with invariants checked at
//! construction ("parse, don't validate"), so the rest of the code can trust it.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod lang;
pub mod money;
pub mod num;
pub mod slug;
pub mod tier;
pub mod time;
pub mod variant;

pub use lang::{Direction, LanguageTag, WritingScript};
pub use money::Usd;
pub use slug::Slug;
pub use tier::{Capability, Tier};
pub use time::{Fps, Frames, Seconds, TICKS_PER_SECOND, Ticks};
pub use variant::{CaptionKind, CaptionMode, CaptionTrack, SubtitleFormat, Variant};
