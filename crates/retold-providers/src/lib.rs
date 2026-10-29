//! Providers: subscription CLIs (Claude Code, Codex, Gemini) and OpenRouter behind one typed
//! contract, multi-login agent pools, sliding usage ledger, plus the router that picks between them.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod claude_cli;
pub mod codex_cli;
pub mod codex_image;
pub mod config;
pub mod detect;
pub mod error;
pub mod gemini_cli;
pub mod local_text;
pub mod local_whisper;
pub mod media;
pub mod openrouter;
pub mod peaks;
pub mod process;
pub mod quota;
pub mod router;
pub mod suggest;
pub mod text;

pub use config::{ProviderConfig, ProvidersConfig, RouteSpec};
pub use error::ProviderError;
pub use router::{Answer, Policy, RouteError, Router, RouterEvent};
pub use text::{Billing, TextProvider, TextRequest, TextResponse, Usage};

pub mod health;
