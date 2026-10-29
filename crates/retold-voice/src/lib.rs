//! Real local speech, validated dialogue and acoustic quality checks.
#![deny(clippy::print_stdout, clippy::print_stderr)]
pub mod audio;
pub mod dialogue;
pub mod local;
pub use dialogue::{DialogueScript, DialogueTurn, EmotionalTag, SpeakerRole};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Infer(#[from] retold_infer::Error),
    #[error(transparent)]
    Provider(#[from] retold_providers::ProviderError),
    #[error("{0}")]
    Invalid(String),
}
