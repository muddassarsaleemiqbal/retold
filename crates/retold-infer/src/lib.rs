//! Local, pinned model assets and cancellable inference. No provider requests at preflight.
pub mod detect;
pub mod models;
pub mod onnx;
pub mod upscale;
pub mod worker;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Process(#[from] retold_providers::ProviderError),
    #[error("ONNX inference: {0}")]
    Onnx(#[from] ort::Error),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Invalid(String),
}
