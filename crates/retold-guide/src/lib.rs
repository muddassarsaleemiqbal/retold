//! Guides (video types) and channels (brands): loading, validation, templates, schemas.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod catalog;
pub mod channel;
mod error;
mod file_set;
pub mod guide;
pub mod inputs;
pub mod manifest;
mod reader;
pub mod template;

pub use catalog::Catalog;
pub use channel::Channel;
pub use error::LoadError;
pub use guide::{Guide, Origin};
pub use inputs::Inputs;
pub use manifest::GuideManifest;

/// JSON Schema for `guide.toml`, for editor autocomplete (`#:schema` in Taplo/Even Better TOML).
#[must_use]
pub fn guide_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(manifest::GuideManifest)).unwrap_or_default()
}

/// JSON Schema for `channel.toml`.
#[must_use]
pub fn channel_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(channel::ChannelFile)).unwrap_or_default()
}
