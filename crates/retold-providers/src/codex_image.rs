//! Subscription image generation. The CLI has a read-only working directory; generated
//! images are collected from the new thread's own output folder, never from another run.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;
use retold_core::LanguageTag;
use tokio_util::sync::CancellationToken;

use crate::codex_cli::{DEFAULT_MODEL, parse_events};
use crate::error::classify_cli_failure;
use crate::media::{
    ImageRequest, Media, MediaProvider, MediaUsage, Quote, SpeechRequest, Transcript,
    image_dimensions,
};
use crate::process::{self, Limits};
use crate::{Billing, ProviderError};

#[derive(Debug)]
pub struct CodexImage {
    pub id: String,
    pub binary: PathBuf,
    pub generated_root: PathBuf,
    pub limits: Limits,
}

impl CodexImage {
    /// `config_dir` is the provider's `CODEX_HOME` override; generated images land under it.
    pub fn new(
        id: &str,
        binary: PathBuf,
        config_dir: Option<&std::path::Path>,
        limits: Limits,
    ) -> Result<Self, ProviderError> {
        let home = config_dir
            .map(std::path::Path::to_path_buf)
            .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")))
            .ok_or_else(|| {
                ProviderError::Unavailable("cannot find Codex's generated-image folder".into())
            })?;
        Ok(Self {
            id: id.into(),
            binary,
            generated_root: home.join("generated_images"),
            limits,
        })
    }
}

#[async_trait]
impl MediaProvider for CodexImage {
    fn id(&self) -> &str {
        &self.id
    }
    fn billing(&self) -> Billing {
        Billing::Subscription
    }
    async fn image_quote(
        &self,
        _: &str,
        _: &ImageRequest,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Ok(Quote {
            ceiling: retold_core::Usd::ZERO,
            provider_tag: None,
        })
    }
    async fn speech_quote(
        &self,
        _: &str,
        _: &SpeechRequest,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Err(unsupported())
    }
    async fn transcript_quote(
        &self,
        _: &str,
        _: f64,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Err(unsupported())
    }

    async fn image(
        &self,
        model: &str,
        req: &ImageRequest,
        _: &Quote,
        cancel: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        let temporary = tempfile::tempdir().map_err(|e| ProviderError::Failed(e.to_string()))?;
        let mut args: Vec<String> = [
            "exec",
            "--json",
            "--ephemeral",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
            "--ignore-rules",
            "-C",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        args.push(temporary.path().display().to_string());
        if model != DEFAULT_MODEL && model != "imagegen" {
            args.extend(["-m".into(), model.into()]);
        }
        for reference in &req.references {
            args.extend(["-i".into(), reference.display().to_string()]);
        }
        args.push("-".into());
        let started = SystemTime::now();
        let prompt = format!(
            "Use image generation to produce exactly ONE still PNG image with aspect {} at the largest supported resolution matching that aspect. Generate no video, GIF, animation or audio. The attached references are the exact style and character designs; match their flat cel colors, line weight and texture closely. Do not use shell tools or modify files.\n{}",
            req.aspect, req.prompt
        );
        let output = process::run(
            &self.binary,
            &args,
            &prompt,
            temporary.path(),
            &self.limits,
            cancel,
        )
        .await?;
        let run = parse_events(&output.stdout);
        if output.status != Some(0) || run.failure.is_some() {
            return Err(classify_cli_failure(
                &self.id,
                run.failure.as_deref().unwrap_or(&output.stderr),
            ));
        }
        let thread = run.thread_id.ok_or_else(|| {
            ProviderError::Malformed("image run did not identify its thread".into())
        })?;
        let bytes = collect(&self.generated_root, &thread, started, &req.aspect)?;
        Ok(Media {
            value: bytes,
            usage: MediaUsage {
                generation: Some(thread),
                cost: Some(retold_core::Usd::ZERO),
            },
        })
    }
    async fn speech(
        &self,
        _: &str,
        _: &SpeechRequest,
        _: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        Err(unsupported())
    }
    async fn transcribe(
        &self,
        _: &str,
        _: &[u8],
        _: &str,
        _: &LanguageTag,
        _: &CancellationToken,
    ) -> Result<Media<Transcript>, ProviderError> {
        Err(unsupported())
    }
}

fn unsupported() -> ProviderError {
    ProviderError::Unsupported("Codex subscription image adapter has no audio capability".into())
}

pub fn collect(
    root: &Path,
    thread: &str,
    started: SystemTime,
    aspect: &str,
) -> Result<Vec<u8>, ProviderError> {
    if thread.is_empty()
        || thread.len() > 100
        || !thread
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(ProviderError::Malformed("invalid image thread id".into()));
    }
    let ratio = aspect
        .split_once(':')
        .and_then(|(w, h)| Some(w.parse::<f64>().ok()? / h.parse::<f64>().ok()?))
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| ProviderError::Malformed("invalid image aspect".into()))?;
    let folder = root.join(thread);
    let entries = std::fs::read_dir(&folder).map_err(|_| {
        ProviderError::Failed("Codex completed without generating an image for this thread".into())
    })?;
    let mut images = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_file())
            || entry.path().extension().is_none_or(|e| e != "png")
        {
            continue;
        }
        if !entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|m| m >= started)
        {
            continue;
        }
        let bytes =
            std::fs::read(entry.path()).map_err(|e| ProviderError::Failed(e.to_string()))?;
        if let Ok([w, h]) = image_dimensions(&bytes)
            && (f64::from(w) / f64::from(h) - ratio).abs() / ratio <= 0.03
        {
            images.push((u64::from(w) * u64::from(h), bytes));
        }
    }
    images
        .into_iter()
        .max_by_key(|(pixels, _)| *pixels)
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| {
            ProviderError::Failed("no new, valid PNG matched the requested aspect".into())
        })
}
