//! The Retold engine: app paths, the run folder layout, run creation and lookup.
//! The task graph, scheduler and stages build on these.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod error;
pub mod fsx;
pub mod layout;
pub mod paths;
pub mod run;

pub use error::EngineError;
pub use layout::{RunLayout, Stage};
pub use paths::{Paths, Settings};
pub use retold_core::{LanguageTag, Slug, Variant};
pub use retold_dramaturgy;
pub use retold_guide;
pub use retold_voice;
pub use run::{FoundRun, NewRun, Quality, RunMeta};

pub mod canon;
pub mod character_approval;
pub mod dossier;
pub mod editing;
pub mod estimate;
pub mod pipeline;
pub mod research;
mod speech;
pub mod store;
pub mod story;

pub mod graph;

pub mod gate;
pub mod ledger;

pub mod media_runtime;

pub mod context;

pub mod assets;

pub mod languages;
pub mod voices;

pub mod assemble;

pub mod waveform;
pub mod workflow;

pub mod review;

pub mod session;

pub mod paid_text;

pub mod events;

mod audio_fit;

mod loudness;

pub mod local_checks;
pub mod local_stages;
pub mod panels;

pub mod mouths;
pub mod parallax;
pub mod phonemes;

pub mod delivery;

pub mod delivery_plates;

pub mod guide_assessment;

pub mod configuration;

pub mod provider_discovery;

pub mod provider_inventory;
pub mod provider_settings;

pub mod provider_routes;

pub mod provider_health;

pub mod provider_test;

pub mod provider_login;

pub mod recovery;

#[cfg(unix)]
#[cfg(unix)]
pub use retold_providers::process::supervision as process_supervision;
#[cfg(windows)]
pub use retold_providers::process::windows_supervision as process_supervision;
