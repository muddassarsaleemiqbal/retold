//! Deterministic, zero-cost non-AI optimization algorithms for narration quality,
//! image validation, and broadcast audio mastering.
//!
//! Provides pure-Rust evaluation suites:
//! - [`linguistics`]: Syllable counting, Flesch-Kincaid grade level, Dale-Chall formula, and cadence analysis.
//! - [`vision`]: CIEDE2000 color difference, Sobel/Laplacian blur detection, and luminance clipping analysis.
//! - [`retrieval`]: min-hash near-duplicate paragraph removal and BM25 passage retrieval.
//! - [`dsp`]: ITU-R BS.1770-4 / EBU R128 loudness measurement, true peak estimation, RMS envelopes, and zero-crossing silence trimming.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod dsp;
pub mod linguistics;
pub mod retrieval;
pub mod vision;

pub(crate) mod dale_chall_words;
