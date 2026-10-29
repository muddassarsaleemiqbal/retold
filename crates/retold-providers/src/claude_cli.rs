//! Claude Code CLI on a subscription login.
//!
//! `claude -p --output-format json --json-schema <schema> --model <m> --tools ""
//!  --no-session-persistence --setting-sources "" --system-prompt <system>`
//! with the prompt on stdin. Tools are off and no project settings are loaded, so the model
//! can only answer. The reply envelope carries `structured_output` when a schema is given.
//!
//! Requests with images use `--input-format stream-json --output-format stream-json
//! --verbose`: one user message carrying base64 image blocks and the prompt, and the
//! envelope is the final `result` event. Images are downscaled to Claude's 1,568-pixel
//! working size and sent as JPEG, well under its 5 MB per-image limit.

use std::path::PathBuf;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::error::{ProviderError, classify_cli_failure};
use crate::process::{self, Limits};
use crate::text::{Billing, TextProvider, TextRequest, TextResponse, Usage, parse_json_text};

#[derive(Debug)]
pub struct ClaudeCli {
    pub id: String,
    pub binary: PathBuf,
    /// Selector → model (`fast = "haiku"`).
    pub tiers: IndexMap<String, String>,
    pub limits: Limits,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    structured_output: Option<serde_json::Value>,
    #[serde(default)]
    usage: Option<EnvelopeUsage>,
    #[serde(default)]
    api_error_status: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "fields mirror Claude's JSON usage object"
)]
struct EnvelopeUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

/// The only tools a research request may use: search and read web pages.
const WEB_TOOLS: &str = "WebSearch,WebFetch";

impl ClaudeCli {}

fn claude_cli_args(req: &TextRequest, model: &str) -> Vec<String> {
    let streaming = !req.images.is_empty();
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        if streaming { "stream-json" } else { "json" },
        "--tools",
        if req.web { WEB_TOOLS } else { "" },
        "--no-session-persistence",
        "--setting-sources",
        "",
        "--strict-mcp-config",
    ]
    .map(str::to_owned)
    .into();
    a.extend([
        "--json-schema".into(),
        req.schema.to_string(),
        "--model".into(),
        model.to_owned(),
    ]);
    if req.web {
        a.extend(["--allowedTools".into(), WEB_TOOLS.into()]);
    }
    if let Some(system) = &req.system {
        a.extend(["--system-prompt".into(), system.clone()]);
    }
    if streaming {
        a.extend([
            "--input-format".into(),
            "stream-json".into(),
            "--verbose".into(),
        ]);
    }
    a
}

/// Longest edge Claude works at; larger images are scaled down by the API anyway.
const CLAUDE_IMAGE_EDGE: u32 = 1568;

/// One stream-json user message: the images, then the prompt.
fn image_message(prompt: &str, images: &[PathBuf]) -> Result<String, ProviderError> {
    use base64::Engine as _;
    let image_count = images.len();
    let mut content = Vec::with_capacity(image_count + 1);
    for path in images {
        // Decode by content, not extension: some providers return JPEG bytes that are saved
        // under a `.png` name.
        let img = image::ImageReader::open(path)
            .and_then(image::ImageReader::with_guessed_format)
            .map_err(|e| ProviderError::Failed(format!("reading {}: {e}", path.display())))?
            .decode()
            .map_err(|e| ProviderError::Failed(format!("reading {}: {e}", path.display())))?;
        let img = if img.width().max(img.height()) > CLAUDE_IMAGE_EDGE {
            img.resize(
                CLAUDE_IMAGE_EDGE,
                CLAUDE_IMAGE_EDGE,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            img
        };
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode_image(&img.to_rgb8())
            .map_err(|e| ProviderError::Failed(format!("encoding {}: {e}", path.display())))?;
        content.push(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/jpeg",
                "data": base64::engine::general_purpose::STANDARD.encode(jpeg),
            }
        }));
    }
    content.push(serde_json::json!({"type": "text", "text": prompt}));
    let message = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": content},
    });
    Ok(format!("{message}\n"))
}

/// The final `result` event of a stream-json run.
fn result_event(stdout: &str) -> Option<&str> {
    stdout.lines().rev().map(str::trim).find(|l| {
        l.starts_with('{')
            && serde_json::from_str::<serde_json::Value>(l)
                .is_ok_and(|v| v.get("type").and_then(|t| t.as_str()) == Some("result"))
    })
}

/// Reads Claude's JSON envelope.
pub(crate) fn parse_envelope(
    id: &str,
    stdout: &str,
    stderr: &str,
) -> Result<TextResponse, ProviderError> {
    let env: Envelope = if let Ok(e) = serde_json::from_str(stdout.trim()) {
        e
    } else {
        let text = if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        };
        return Err(classify_cli_failure(id, text));
    };
    if env.is_error || env.subtype.as_deref().is_some_and(|s| s != "success") {
        let mut msg = env.result.clone().unwrap_or_default();
        if let Some(status) = env.api_error_status.filter(|s| !s.is_null()) {
            msg = format!("{msg} (HTTP {status})");
        }
        if msg.trim().is_empty() {
            msg = format!(
                "ended with {}",
                env.subtype.as_deref().unwrap_or("an error")
            );
        }
        return Err(classify_cli_failure(id, &msg));
    }
    let value = match env.structured_output {
        Some(v) => v,
        None => parse_json_text(env.result.as_deref().unwrap_or_default())?,
    };
    let usage = env.usage.map_or_else(Usage::default, |u| Usage {
        input_tokens: u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens,
        output_tokens: u.output_tokens,
        // `total_cost_usd` is the list price; a subscription is not charged per call.
        cost: retold_core::Usd::ZERO,
        cost_known: true,
    });
    Ok(TextResponse { value, usage })
}

#[async_trait]
impl TextProvider for ClaudeCli {
    fn id(&self) -> &str {
        &self.id
    }

    fn billing(&self) -> Billing {
        Billing::Subscription
    }

    fn supports_images(&self) -> bool {
        true
    }

    fn resolve_model(&self, selector: &str) -> String {
        self.tiers
            .get(selector)
            .cloned()
            .unwrap_or_else(|| selector.to_owned())
    }

    async fn generate(
        &self,
        req: &TextRequest,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<TextResponse, ProviderError> {
        let TextRequest { prompt, images, .. } = req;
        let has_images = !images.is_empty();
        let stdin = if has_images {
            let (prompt, images) = (prompt.clone(), images.clone());
            tokio::task::spawn_blocking(move || image_message(&prompt, &images))
                .await
                .map_err(|e| ProviderError::Failed(format!("preparing images: {e}")))??
        } else {
            prompt.clone()
        };
        let tmp =
            tempfile::tempdir().map_err(|e| ProviderError::Failed(format!("temp dir: {e}")))?;
        let out = process::run(
            &self.binary,
            &claude_cli_args(req, model),
            &stdin,
            tmp.path(),
            &self.limits,
            cancel,
        )
        .await?;
        let envelope = if has_images {
            result_event(&out.stdout).unwrap_or_default()
        } else {
            out.stdout.as_str()
        };
        parse_envelope(&self.id, envelope, &out.stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"ok\":true,\"n\":7}","structured_output":{"ok":true,"n":7},"total_cost_usd":0.0009825,"usage":{"input_tokens":2,"cache_creation_input_tokens":4098,"cache_read_input_tokens":0,"output_tokens":73}}"#;

    #[test]
    fn reads_structured_output() {
        let r = parse_envelope("claude-code", OK, "").unwrap();
        assert_eq!(r.value["n"], 7);
        assert_eq!(r.usage.input_tokens, 4100);
        assert_eq!(r.usage.cost, retold_core::Usd::ZERO);
    }

    #[test]
    fn recognises_limits_and_login() {
        let limited = r#"{"type":"result","subtype":"success","is_error":true,"result":"Claude AI usage limit reached|1760000000"}"#;
        assert!(matches!(
            parse_envelope("c", limited, ""),
            Err(ProviderError::RateLimited { .. })
        ));
        assert!(matches!(
            parse_envelope("c", "", "Invalid API key · Please run /login"),
            Err(ProviderError::Unavailable(_))
        ));
    }

    #[test]
    fn falls_back_to_result_text() {
        let env = r#"{"subtype":"success","is_error":false,"result":"```json\n{\"a\":1}\n```"}"#;
        assert_eq!(parse_envelope("c", env, "").unwrap().value["a"], 1);
    }
    #[test]
    fn image_requests_stream_one_message_and_read_the_result_event() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("big.png");
        image::RgbImage::from_pixel(2000, 3000, image::Rgb([200, 120, 40]))
            .save(&png)
            .unwrap();
        let line = image_message("Check this frame.", std::slice::from_ref(&png)).unwrap();
        let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        let content = &v["message"]["content"];
        assert_eq!(content[0]["source"]["media_type"], "image/jpeg");
        assert_eq!(content[1]["text"], "Check this frame.");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content[0]["source"]["data"].as_str().unwrap())
            .unwrap();
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!(back.height(), CLAUDE_IMAGE_EDGE);

        let req = TextRequest {
            system: None,
            prompt: "p".into(),
            schema: serde_json::json!({}),
            images: vec![png],
            web: false,
        };
        let args = claude_cli_args(&req, "haiku");
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--input-format" && w[1] == "stream-json")
        );
        assert!(args.contains(&"--verbose".to_owned()));

        let stream = format!("{{\"type\":\"system\"}}\n{OK}\n");
        let r = parse_envelope("c", result_event(&stream).unwrap(), "").unwrap();
        assert_eq!(r.value["n"], 7);
    }
    #[test]
    fn jpeg_bytes_under_a_png_name_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unit-3.png");
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&image::RgbImage::from_pixel(
                40,
                70,
                image::Rgb([10, 20, 30]),
            ))
            .unwrap();
        std::fs::write(&path, jpeg).unwrap();
        assert!(image_message("p", std::slice::from_ref(&path)).is_ok());
        let part = crate::openrouter::image_part(&path).unwrap();
        assert!(
            part["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
    }
}
