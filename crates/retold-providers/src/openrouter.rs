//! OpenRouter: pay-per-use text, vision, images, speech and transcription over HTTPS.
//! This module covers structured text/vision and the key's credit; images and audio join
//! with their stages.

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use retold_core::Usd;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::error::ProviderError;
use crate::text::{Billing, TextProvider, TextRequest, TextResponse, Usage, parse_json_text};

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

#[derive(Debug)]
pub struct OpenRouter {
    pub id: String,
    pub base_url: String,
    key: SecretString,
    http: reqwest::Client,
    /// Recent prompts, to find a shared prefix worth caching (see [`cache_split`]).
    recent: std::sync::Mutex<std::collections::VecDeque<String>>,
}

/// What the key can still spend.
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub used: Usd,
    /// `None` when the key has no limit.
    pub remaining: Option<Usd>,
}

impl OpenRouter {
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        key: SecretString,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("retold/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ProviderError::Failed(format!("HTTP client: {e}")))?;
        Ok(Self {
            id: id.into(),
            base_url: base_url.into(),
            key,
            http,
            recent: std::sync::Mutex::default(),
        })
    }

    pub(crate) fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(self.key.expose_secret())
            .header("X-Title", "Retold")
    }

    /// Spend so far and what is left on this key.
    pub async fn key_limits(&self) -> Result<KeyInfo, ProviderError> {
        #[derive(Deserialize)]
        struct Reply {
            data: Data,
        }
        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            usage: Option<f64>,
            #[serde(default)]
            limit_remaining: Option<f64>,
        }
        let resp = self
            .request(reqwest::Method::GET, "/key")
            .send()
            .await
            .map_err(|e| net(&e))?;
        let resp = check(resp).await?;
        let r: Reply = resp
            .json()
            .await
            .map_err(|e| ProviderError::Malformed(format!("/key: {e}")))?;
        Ok(KeyInfo {
            used: Usd::from_dollars(r.data.usage.unwrap_or(0.0)).unwrap_or_default(),
            remaining: r.data.limit_remaining.and_then(Usd::from_dollars),
        })
    }
}

pub(crate) fn net(e: &reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError::Timeout(format!("OpenRouter: {e}"))
    } else {
        ProviderError::Unavailable(format!("OpenRouter unreachable: {e}"))
    }
}

/// Turns an HTTP error status into the right [`ProviderError`].
pub(crate) async fn check(mut resp: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs);
    let has_generation = resp.headers().contains_key("x-generation-id");
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = resp.chunk().await {
        let remaining = 4096usize.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if bytes.len() == 4096 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    let validation = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned));
    if status.as_u16() == 400
        && !has_generation
        && validation
            .as_ref()
            .is_some_and(|m| m.starts_with("Gemini TTS only supports response_format="))
    {
        return Err(ProviderError::Unbilled(validation.unwrap_or_default()));
    }
    let body: String = text.chars().take(400).collect();
    let message = format!("OpenRouter HTTP {}: {body}", status.as_u16());
    Err(match status.as_u16() {
        429 | 503 | 529 => ProviderError::RateLimited {
            message,
            retry_after,
        },
        401..=403 => ProviderError::Unavailable(message),
        408 | 504 => ProviderError::Timeout(message),
        _ => ProviderError::Failed(message),
    })
}

pub(crate) fn image_part(path: &std::path::Path) -> Result<Value, ProviderError> {
    let bytes = std::fs::read(path)
        .map_err(|e| ProviderError::Failed(format!("reading {}: {e}", path.display())))?;
    // The bytes decide the type: a `.png` name may hold JPEG data.
    let mime = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
        Ok(image::ImageFormat::WebP) => "image/webp",
        _ => "image/png",
    };
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(json!({"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{data}")}}))
}

#[async_trait]
impl TextProvider for OpenRouter {
    fn id(&self) -> &str {
        &self.id
    }

    fn billing(&self) -> Billing {
        Billing::PerUse
    }

    fn supports_images(&self) -> bool {
        true
    }

    fn resolve_model(&self, selector: &str) -> String {
        selector.to_owned()
    }

    async fn generate(
        &self,
        req: &TextRequest,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<TextResponse, ProviderError> {
        let TextRequest {
            prompt,
            system,
            images,
            schema,
            web,
        } = req;
        if *web {
            return Err(ProviderError::Unsupported(
                "web research runs on subscription CLIs only".into(),
            ));
        }
        let mut messages = Vec::new();
        if let Some(system_prompt) = system {
            messages.push(json!({"role": "system", "content": system_prompt}));
        }
        let split = if model.starts_with("anthropic/") {
            let mut recent = self
                .recent
                .lock()
                .map_err(|_| ProviderError::Failed("prompt history lock poisoned".into()))?;
            let split = cache_split(prompt, recent.iter().map(String::as_str));
            recent.push_front(prompt.clone());
            recent.truncate(RECENT_PROMPTS);
            split
        } else {
            None
        };
        let mut content = match split {
            Some(at) => vec![
                json!({"type": "text", "text": &prompt[..at], "cache_control": {"type": "ephemeral"}}),
                json!({"type": "text", "text": &prompt[at..]}),
            ],
            None => vec![json!({"type": "text", "text": prompt})],
        };
        for img in images {
            content.push(image_part(img)?);
        }
        messages.push(json!({"role": "user", "content": content}));
        let body = json!({
            "model": model,
            "messages": messages,
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "output", "strict": true, "schema": schema},
            },
            "usage": {"include": true},
            "max_tokens": crate::text::MAX_OUTPUT_TOKENS,
            "reasoning": {"max_tokens": MAX_REASONING_TOKENS},
        });
        let send = self
            .request(reqwest::Method::POST, "/chat/completions")
            .json(&body)
            .send();
        let resp = tokio::select! {
            r = send => r.map_err(|e| net(&e))?,
            () = cancel.cancelled() => return Err(ProviderError::Cancelled),
        };
        let resp = check(resp).await?;
        let v: Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Malformed(format!("chat reply: {e}")))?;
        parse_chat(&v)
    }
}

/// Cap extended-thinking budget so structured JSON output keeps >=21k of the 24k output budget.
const MAX_REASONING_TOKENS: u32 = 2_048;
const RECENT_PROMPTS: usize = 32;
/// Anthropic caches only prefixes of at least 1024 tokens; about four characters each.
const MIN_CACHED_CHARS: usize = 4_500;

/// Where to end the cached block: the longest prefix this prompt shares with a recent one,
/// cut back to a line end, when it is long enough to cache. Repair loops, section drafts
/// and fact-check passes repeat a large stable context first, so later calls read it from
/// the cache at a fraction of the input price.
fn cache_split<'a>(prompt: &str, recent: impl Iterator<Item = &'a str>) -> Option<usize> {
    let shared = recent
        .map(|other| {
            prompt
                .bytes()
                .zip(other.bytes())
                .take_while(|(a, b)| a == b)
                .count()
        })
        .max()?;
    // A newline byte is always a character boundary.
    let at = prompt.as_bytes()[..shared]
        .iter()
        .rposition(|b| *b == b'\n')?
        + 1;
    (at >= MIN_CACHED_CHARS && at < prompt.len()).then_some(at)
}

pub(crate) fn parse_chat(v: &Value) -> Result<TextResponse, ProviderError> {
    if let Some(err) = v.get("error") {
        return Err(ProviderError::Failed(format!("OpenRouter: {err}")));
    }
    let choice = &v["choices"][0];
    if let Some(refusal) = choice["message"]["refusal"]
        .as_str()
        .filter(|r| !r.is_empty())
    {
        return Err(ProviderError::Refused(refusal.to_owned()));
    }
    let text = choice["message"]["content"].as_str().ok_or_else(|| {
        let message = choice["message"].to_string();
        ProviderError::Malformed(format!(
            "reply has no message content (finish_reason: {}, native: {}, message: {})",
            choice["finish_reason"],
            choice["native_finish_reason"],
            message.chars().take(400).collect::<String>()
        ))
    })?;
    let u = &v["usage"];
    let usage = Usage {
        input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
        cost_known: u["cost"].as_f64().is_some(),
        cost: u["cost"]
            .as_f64()
            .and_then(Usd::from_dollars)
            .unwrap_or_default(),
    };
    Ok(TextResponse {
        value: parse_json_text(text)?,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_content_and_exact_cost() {
        let v = json!({"choices":[{"message":{"content":"{\"a\":1}"}}],"usage":{"prompt_tokens":10,"completion_tokens":2,"cost":0.00042}});
        let r = parse_chat(&v).unwrap();
        assert_eq!(r.value["a"], 1);
        assert_eq!(r.usage.cost, Usd::from_micros(420));
    }

    #[test]
    fn reports_refusals() {
        let v = json!({"choices":[{"message":{"content":null,"refusal":"no"}}]});
        assert!(matches!(parse_chat(&v), Err(ProviderError::Refused(_))));
    }
}

#[cfg(test)]
mod cache_tests {
    use super::cache_split;

    #[test]
    fn caches_the_shared_stable_prefix_up_to_a_line_end() {
        let context = "guide and dossier line\n".repeat(300);
        let first = format!("{context}Write section intro.");
        let second = format!("{context}Write section middle.");
        assert_eq!(cache_split(&second, std::iter::empty()), None);
        assert_eq!(
            cache_split(&second, [first.as_str()].into_iter()),
            Some(context.len())
        );
        let short = "short shared\nthen different";
        assert_eq!(cache_split(short, ["short shared\nelse"].into_iter()), None);
    }
}
