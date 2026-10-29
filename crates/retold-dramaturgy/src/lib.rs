//! Script quality gates: emotional delta between beats ($\Delta E \ge 1.0$) and oral cadence.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod beat;
pub mod cadence;

pub use beat::{
    BeatError, DramaticBeat, DramaturgyMetrics, KnowledgeShift, storybit_benchmark,
    validate_beat_gates,
};
pub use cadence::{
    BANNED_CLICHES, CadenceCategory, CadenceEngine, CadenceReport, CadenceViolation, SENSORY_VERBS,
};
