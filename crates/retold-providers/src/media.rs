//! Image, speech and transcript capabilities. Paid requests are never retried here:
//! ambiguous failures must be reconciled by the run ledger before another call.

use std::io::Cursor;
use std::path::PathBuf;

use async_trait::async_trait;
use base64::Engine as _;
use retold_core::{LanguageTag, Usd};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::openrouter::{OpenRouter, check, image_part, net};
use crate::{Billing, ProviderError};
use retold_core::num;

/// Tokens reserved per attached image when quoting paid vision requests. Vision models
/// bill an image by its pixels, not its file size: about 1,600 tokens at most for Claude and
/// about 1,100 for OpenAI at high detail, so this allowance stays conservative.
pub const IMAGE_TOKEN_ALLOWANCE: usize = 3_000;

pub const MAX_MEDIA_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRequest {
    pub prompt: String,
    pub references: Vec<PathBuf>,
    pub aspect: String,
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechRequest {
    pub text: String,
    pub voice: String,
    pub instructions: Option<String>,
    pub language: LanguageTag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub word: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub duration: f64,
    pub words: Vec<Word>,
}

impl Transcript {
    pub fn validate(&self) -> Result<(), ProviderError> {
        let mut previous = 0.0;
        if !self.duration.is_finite() || self.duration <= 0.0 || self.words.is_empty() {
            return Err(ProviderError::Malformed(
                "transcription needs positive duration and word timestamps".into(),
            ));
        }
        for w in &self.words {
            if w.word.trim().is_empty()
                || !w.start.is_finite()
                || !w.end.is_finite()
                || w.start < previous
                || w.end <= w.start
                || w.end > self.duration + 0.05
            {
                return Err(ProviderError::Malformed(
                    "transcription has invalid or unordered word times".into(),
                ));
            }
            previous = w.start;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUsage {
    pub generation: Option<String>,
    /// Missing billing is deliberately distinct from zero.
    pub cost: Option<Usd>,
}

#[derive(Debug, Clone)]
pub struct Media<T> {
    pub value: T,
    pub usage: MediaUsage,
}

/// A conservative maximum based on live provider pricing. `provider_tag` pins image
/// routing to the endpoint whose price was quoted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub ceiling: Usd,
    pub provider_tag: Option<String>,
}

#[async_trait]
pub trait MediaProvider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &str;
    fn billing(&self) -> Billing;
    async fn speaker_embedding(
        &self,
        _: &std::path::Path,
        _: &CancellationToken,
    ) -> Result<Vec<f64>, ProviderError> {
        Err(ProviderError::Unsupported(
            "speaker embedding unavailable".into(),
        ))
    }

    async fn text_quote(
        &self,
        _: &str,
        _: &crate::TextRequest,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Err(ProviderError::Unsupported(
            "this provider cannot quote paid text".into(),
        ))
    }
    async fn recorded_cost(
        &self,
        _: &str,
        _: &CancellationToken,
    ) -> Result<Option<Usd>, ProviderError> {
        Ok(None)
    }
    async fn image_quote(
        &self,
        model: &str,
        req: &ImageRequest,
        cancel: &CancellationToken,
    ) -> Result<Quote, ProviderError>;
    async fn speech_quote(
        &self,
        model: &str,
        req: &SpeechRequest,
        cancel: &CancellationToken,
    ) -> Result<Quote, ProviderError>;
    async fn transcript_quote(
        &self,
        model: &str,
        duration: f64,
        cancel: &CancellationToken,
    ) -> Result<Quote, ProviderError>;
    async fn image(
        &self,
        model: &str,
        req: &ImageRequest,
        quote: &Quote,
        cancel: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError>;
    async fn speech(
        &self,
        model: &str,
        req: &SpeechRequest,
        cancel: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError>;
    async fn transcribe(
        &self,
        model: &str,
        bytes: &[u8],
        format: &str,
        lang: &LanguageTag,
        cancel: &CancellationToken,
    ) -> Result<Media<Transcript>, ProviderError>;
}

impl OpenRouter {
    pub(crate) async fn media_json(
        &self,
        path: &str,
        body: Option<&Value>,
        cancel: &CancellationToken,
    ) -> Result<(Value, Option<String>), ProviderError> {
        let (bytes, generation) = self.media_bytes(path, body, cancel).await?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| ProviderError::Malformed(e.to_string()))?;
        if let Some(error) = value.get("error") {
            return Err(ProviderError::Failed(error.to_string()));
        }
        Ok((value, generation))
    }

    async fn media_bytes(
        &self,
        path: &str,
        body: Option<&Value>,
        cancel: &CancellationToken,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        tokio::select! {
            () = cancel.cancelled() => Err(ProviderError::Cancelled),
            result = async {
                let method = if body.is_some() { reqwest::Method::POST } else { reqwest::Method::GET };
                let request = self.request(method, path);
                let request = if let Some(body) = body { request.json(body) } else { request };
                let mut response = check(request.send().await.map_err(|e| net(&e))?).await?;
                let generation = response.headers().get("x-generation-id").and_then(|v| v.to_str().ok()).map(str::to_owned);
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|e| net(&e))? {
                    if bytes.len() + chunk.len() > MAX_MEDIA_BYTES { return Err(ProviderError::Malformed("media response exceeds 64 MB".into())); }
                    bytes.extend_from_slice(&chunk);
                }
                Ok((bytes, generation))
            } => result,
        }
    }

    async fn model_price(
        &self,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, ProviderError> {
        let (value, _) = self
            .media_json("/models?output_modalities=all", None, cancel)
            .await?;
        let mut model_value = value["data"]
            .as_array()
            .and_then(|items| items.iter().find(|m| m["id"] == model))
            .cloned()
            .ok_or_else(|| {
                ProviderError::Unavailable(format!("model `{model}` is not in the live catalog"))
            })?;
        let (endpoints, _) = self
            .media_json(&format!("/models/{model}/endpoints"), None, cancel)
            .await?;
        for field in ["prompt", "completion"] {
            let maximum = endpoints["data"]["endpoints"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| number(&e["pricing"][field]))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .fold(number(&model_value["pricing"][field])?, f64::max);
            model_value["pricing"][field] = json!(maximum);
        }
        let endpoints = endpoints["data"]["endpoints"].as_array();
        let limits = endpoints.and_then(|items| {
            items
                .iter()
                .map(|e| e["max_completion_tokens"].as_u64().filter(|n| *n > 0))
                .collect::<Option<Vec<_>>>()
        });
        model_value["bounded_completion_tokens"] =
            json!(limits.and_then(|items| items.into_iter().max()));
        Ok(model_value)
    }

    pub async fn generation_cost(
        &self,
        id: &str,
        cancel: &CancellationToken,
    ) -> Result<Usd, ProviderError> {
        // Generation ids contain only URL-safe letters/digits and separators.
        if !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(ProviderError::Malformed("invalid generation id".into()));
        }
        for attempt in 0..6 {
            if let Ok(Ok((value, _))) = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                self.media_json(&format!("/generation?id={id}"), None, cancel),
            )
            .await
                && let Some(cost) = value["data"]["total_cost"]
                    .as_f64()
                    .and_then(Usd::from_dollars)
                    .filter(|c| c.micros() >= 0)
            {
                return Ok(cost);
            }
            if attempt < 5 {
                tokio::select! {()=cancel.cancelled()=>return Err(ProviderError::Cancelled),()=tokio::time::sleep(std::time::Duration::from_millis(400))=>{}}
            }
        }
        Err(ProviderError::Malformed(
            "generation cost is not available yet".into(),
        ))
    }
}

#[async_trait]
impl MediaProvider for OpenRouter {
    fn id(&self) -> &str {
        &self.id
    }
    fn billing(&self) -> Billing {
        Billing::PerUse
    }

    async fn recorded_cost(
        &self,
        generation: &str,
        cancel: &CancellationToken,
    ) -> Result<Option<Usd>, ProviderError> {
        Ok(self.generation_cost(generation, cancel).await.ok())
    }
    async fn text_quote(
        &self,
        model: &str,
        req: &crate::TextRequest,
        cancel: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        let price = self.model_price(model, cancel).await?;
        let tokens = quote_input_tokens(req)?;
        Ok(Quote {
            ceiling: ceiling(
                number(&price["pricing"]["prompt"])? * f64::from(tokens)
                    + number(&price["pricing"]["completion"])?
                        * f64::from(crate::text::MAX_OUTPUT_TOKENS),
            )?,
            provider_tag: None,
        })
    }

    async fn image_quote(
        &self,
        model: &str,
        req: &ImageRequest,
        cancel: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        let model = model.split_once('@').map_or(model, |(id, _)| id);
        let (value, _) = self
            .media_json(&format!("/images/models/{model}/endpoints"), None, cancel)
            .await?;
        fixed_image_quote(&value, req)
    }

    async fn speech_quote(
        &self,
        model: &str,
        req: &SpeechRequest,
        cancel: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        let price = self.model_price(model, cancel).await?;
        if price["architecture"]["output_modalities"]
            .as_array()
            .is_none_or(|v| !v.contains(&json!("speech")))
        {
            return Err(ProviderError::Unsupported(
                "model does not output speech".into(),
            ));
        }
        if let Some(voices) = price["supported_voices"].as_array()
            && !voices.contains(&json!(req.voice))
        {
            return Err(ProviderError::Unsupported(format!(
                "voice `{}` is not supported by {model}",
                req.voice
            )));
        }
        let output_rate = number(&price["pricing"]["completion"])?;
        let input_rate = number(&price["pricing"]["prompt"])?;
        if output_rate.abs() <= f64::EPSILON {
            let count = u32::try_from(req.text.chars().count())
                .map_err(|_| ProviderError::Malformed("speech text too long".into()))?;
            return Ok(Quote {
                ceiling: ceiling(input_rate * f64::from(count))?,
                provider_tag: None,
            });
        }
        if !model.starts_with("google/gemini-") {
            return Err(ProviderError::Unsupported(
                "speech output has no verified token ceiling".into(),
            ));
        }
        let cap = price["bounded_completion_tokens"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                ProviderError::Unsupported(
                    "every speech endpoint must declare a hard output-token limit".into(),
                )
            })?;
        let text_len = req.text.len();
        let inst_len = req.instructions.as_ref().map_or(0, String::len);
        let token_overhead = 2048;
        let input = u32::try_from(text_len + inst_len + token_overhead)
            .map_err(|_| ProviderError::Malformed("speech input too long".into()))?;
        Ok(Quote {
            ceiling: ceiling(input_rate * f64::from(input) + output_rate * f64::from(cap))?,
            provider_tag: None,
        })
    }

    async fn transcript_quote(
        &self,
        model: &str,
        duration: f64,
        cancel: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        if !model.contains("whisper") {
            return Err(ProviderError::Unsupported(
                "only duration-priced Whisper transcription is supported for bounded billing"
                    .into(),
            ));
        }
        let price = self.model_price(model, cancel).await?;
        Ok(Quote {
            ceiling: ceiling(number(&price["pricing"]["prompt"])? * duration.ceil())?,
            provider_tag: None,
        })
    }

    async fn image(
        &self,
        model: &str,
        req: &ImageRequest,
        quote: &Quote,
        cancel: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        let model = model.split_once('@').map_or(model, |(id, _)| id);
        let references = req
            .references
            .iter()
            .map(|p| image_part(p))
            .collect::<Result<Vec<_>, _>>()?;
        let mut body = json!({"model":model,"prompt":req.prompt,"input_references":references,"aspect_ratio":req.aspect,"resolution":req.resolution,"n":1,"output_format":"png"});
        if let Some(tag) = &quote.provider_tag {
            body["provider"] = json!({"only":[tag],"allow_fallbacks":false});
        }
        let (value, generation) = self.media_json("/images", Some(&body), cancel).await?;
        let data = value["data"][0]["b64_json"]
            .as_str()
            .ok_or_else(|| ProviderError::Malformed("image API returned no base64 image".into()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| ProviderError::Malformed(e.to_string()))?;
        let dimensions = image_dimensions(&bytes)?;
        validate_aspect(dimensions, &req.aspect)?;
        Ok(Media {
            value: bytes,
            usage: MediaUsage {
                generation,
                cost: cost(&value),
            },
        })
    }

    async fn speech(
        &self,
        model: &str,
        req: &SpeechRequest,
        cancel: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        let pcm = model.starts_with("google/gemini-");
        let mut body = json!({"model":model,"input":req.text,"voice":req.voice,"response_format":if pcm{"pcm"}else{"mp3"}});
        if let Some(instructions) = &req.instructions {
            body["instructions"] = json!(instructions);
        }
        let (bytes, generation) = self
            .media_bytes("/audio/speech", Some(&body), cancel)
            .await?;
        let bytes = if pcm && !bytes.starts_with(b"RIFF") {
            pcm_wav(&bytes, 24_000)?
        } else {
            bytes
        };
        audio_duration(&bytes)?;
        let cost = if let Some(id) = &generation {
            self.generation_cost(id, cancel).await.ok()
        } else {
            None
        };
        Ok(Media {
            value: bytes,
            usage: MediaUsage { generation, cost },
        })
    }

    async fn transcribe(
        &self,
        model: &str,
        bytes: &[u8],
        format: &str,
        lang: &LanguageTag,
        cancel: &CancellationToken,
    ) -> Result<Media<Transcript>, ProviderError> {
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        let body = json!({"model":model,"input_audio":{"data":data,"format":format},"language":lang.language(),"response_format":"verbose_json","timestamp_granularities":["word"]});
        let (value, generation) = self
            .media_json("/audio/transcriptions", Some(&body), cancel)
            .await?;
        let transcript: Transcript = serde_json::from_value(value.clone())
            .map_err(|e| ProviderError::Malformed(e.to_string()))?;
        let exact = if let Some(cost) = cost(&value) {
            Some(cost)
        } else if let Some(id) = &generation {
            self.generation_cost(id, cancel).await.ok()
        } else {
            None
        };
        Ok(Media {
            value: transcript,
            usage: MediaUsage {
                generation,
                cost: exact,
            },
        })
    }
}

pub fn image_dimensions(bytes: &[u8]) -> Result<[u32; 2], ProviderError> {
    if bytes.len() > MAX_MEDIA_BYTES {
        return Err(ProviderError::Malformed("image exceeds size cap".into()));
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| ProviderError::Malformed(format!("invalid image: {e}")))?;
    Ok([image.width(), image.height()])
}

fn validate_aspect(dimensions: [u32; 2], aspect: &str) -> Result<(), ProviderError> {
    let expected = aspect
        .split_once(':')
        .and_then(|(w, h)| Some(w.parse::<f64>().ok()? / h.parse::<f64>().ok()?))
        .filter(|r| r.is_finite() && *r > 0.0)
        .ok_or_else(|| ProviderError::Malformed("invalid image aspect".into()))?;
    let actual = f64::from(dimensions[0]) / f64::from(dimensions[1]);
    if (actual / expected - 1.0).abs() > 0.03 {
        return Err(ProviderError::Malformed(
            "image output does not match requested aspect".into(),
        ));
    }
    Ok(())
}

pub fn audio_duration(bytes: &[u8]) -> Result<f64, ProviderError> {
    use symphonia::core::{
        formats::FormatOptions,
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
        probe::Hint,
    };
    if bytes.len() > MAX_MEDIA_BYTES {
        return Err(ProviderError::Malformed("audio exceeds size cap".into()));
    }
    let stream = MediaSourceStream::new(
        Box::new(Cursor::new(bytes.to_vec())),
        MediaSourceStreamOptions::default(),
    );
    let mut probe = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| ProviderError::Malformed(format!("invalid audio: {e}")))?;
    let track = probe
        .format
        .default_track()
        .ok_or_else(|| ProviderError::Malformed("audio has no track".into()))?;
    let id = track.id;
    let time_base = track
        .codec_params
        .time_base
        .ok_or_else(|| ProviderError::Malformed("audio has no time base".into()))?;
    let mut end = 0;
    while let Ok(packet) = probe.format.next_packet() {
        if packet.track_id() == id {
            end = end.max(packet.ts.saturating_add(packet.dur));
        }
    }
    let time = time_base.calc_time(end);
    let duration = num::u64_f64(time.seconds) + time.frac;
    if duration <= 0.0 || !duration.is_finite() {
        return Err(ProviderError::Malformed(
            "audio duration must be positive".into(),
        ));
    }
    Ok(duration)
}

pub fn pcm_wav(bytes: &[u8], rate: u32) -> Result<Vec<u8>, ProviderError> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) || bytes.len() > MAX_MEDIA_BYTES {
        return Err(ProviderError::Malformed("invalid 16-bit PCM audio".into()));
    }
    let data_len = bytes.len();
    let length =
        u32::try_from(data_len).map_err(|_| ProviderError::Malformed("PCM too large".into()))?;
    let mut wav = Vec::with_capacity(data_len + 44);
    wav.extend(b"RIFF");
    wav.extend((length + 36).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(length.to_le_bytes());
    wav.extend(bytes);
    Ok(wav)
}

fn cost(value: &Value) -> Option<Usd> {
    value["usage"]["cost"]
        .as_f64()
        .and_then(Usd::from_dollars)
        .filter(|c| c.micros() >= 0)
}
fn number(value: &Value) -> Result<f64, ProviderError> {
    let n = value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite() && *n >= 0.0);
    n.ok_or_else(|| ProviderError::Malformed("pricing is missing or invalid".into()))
}
fn ceiling(dollars: f64) -> Result<Usd, ProviderError> {
    Usd::from_dollars((dollars * 1_000_000.0).ceil() / 1_000_000.0)
        .filter(|c| c.micros() >= 0)
        .ok_or_else(|| ProviderError::Malformed("invalid quote".into()))
}

pub fn fixed_image_quote(value: &Value, req: &ImageRequest) -> Result<Quote, ProviderError> {
    let mut quotes = Vec::new();
    for endpoint in value["endpoints"].as_array().into_iter().flatten() {
        let Some(tag) = endpoint["provider_tag"].as_str() else {
            continue;
        };
        let Some(lines) = endpoint["pricing"].as_array() else {
            continue;
        };
        let resolution = &endpoint["supported_parameters"]["resolution"]["values"];
        if resolution.as_array().is_some_and(|v| {
            !v.iter().any(|x| {
                x.as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&req.resolution))
            })
        }) {
            continue;
        }
        let mut total = 0.0;
        let mut output = false;
        let mut bounded = true;
        for line in lines {
            if line["variant"]
                .as_str()
                .is_some_and(|s| !s.eq_ignore_ascii_case(&req.resolution))
            {
                continue;
            }
            let units = match (line["billable"].as_str(), line["unit"].as_str()) {
                (Some("output_image"), Some("image")) => {
                    output = true;
                    1.0
                }
                (Some("input_image" | "input_reference"), Some("image")) => {
                    f64::from(u32::try_from(req.references.len()).unwrap_or(u32::MAX))
                }
                _ => {
                    bounded = false;
                    break;
                }
            };
            total += number(&line["cost_usd"])? * units;
        }
        if bounded && output {
            quotes.push(Quote {
                ceiling: ceiling(total)?,
                provider_tag: Some(tag.into()),
            });
        }
    }
    quotes.into_iter().min_by_key(|q| q.ceiling).ok_or_else(|| {
        ProviderError::Unsupported(
            "no fixed-price image endpoint can safely quote this request".into(),
        )
    })
}

/// Worst-case input tokens for a paid text request: one token per byte of text (real
/// tokenisers average about four bytes per token), plus a fixed allowance per image.
fn quote_input_tokens(req: &crate::TextRequest) -> Result<u32, ProviderError> {
    let prompt_len = req.prompt.len();
    let sys_len = req.system.as_ref().map_or(0, String::len);
    let schema_len = req.schema.to_string().len();
    let base_overhead = 2048;
    let mut tokens = prompt_len + sys_len + schema_len + base_overhead;
    for p in &req.images {
        if !p.is_file() {
            return Err(ProviderError::Failed(format!(
                "{} does not exist",
                p.display()
            )));
        }
        tokens += IMAGE_TOKEN_ALLOWANCE;
    }
    u32::try_from(tokens)
        .map_err(|_| ProviderError::Malformed("text request too large to quote".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vision_quotes_count_images_by_allowance_not_file_size() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("frame.png");
        std::fs::write(&big, vec![0u8; 4_000_000]).unwrap();
        let req = crate::TextRequest {
            system: None,
            prompt: "check".into(),
            schema: json!({}),
            images: vec![big],
            web: false,
        };
        let tokens = quote_input_tokens(&req).unwrap();
        assert!(tokens < 6_000, "{tokens}");
        let missing = crate::TextRequest {
            images: vec![dir.path().join("nope.png")],
            web: false,
            ..req
        };
        assert!(quote_input_tokens(&missing).is_err());
    }
    #[test]
    fn gemini_pcm_has_a_playable_mono_wav_header_and_rejects_truncated_samples() {
        let pcm = vec![0u8; 48_000];
        let wav = pcm_wav(&pcm, 24_000).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 48_000);
        assert_eq!(&wav[44..], pcm);
        assert!(pcm_wav(&[], 24_000).is_err());
        assert!(pcm_wav(&[0], 24_000).is_err());
    }
    #[test]
    fn fixed_quotes_respect_resolution_and_reference_costs() {
        let req = ImageRequest {
            prompt: "scene".into(),
            references: vec!["style.png".into(), "cast.png".into()],
            aspect: "9:16".into(),
            resolution: "2K".into(),
        };
        let value = json!({"endpoints":[{"provider_tag":"seed","supported_parameters":{"resolution":{"values":["1K","2K"]}},"pricing":[{"billable":"output_image","unit":"image","cost_usd":"0.018","variant":"2K"},{"billable":"input_image","unit":"image","cost_usd":"0.001","variant":"2K"}]}]});
        let quote = fixed_image_quote(&value, &req).unwrap();
        assert_eq!(quote.ceiling, Usd::from_micros(20_000));
        assert_eq!(quote.provider_tag.as_deref(), Some("seed"));
        let mut wrong = req;
        wrong.resolution = "4K".into();
        assert!(fixed_image_quote(&value, &wrong).is_err());
    }
    #[test]
    fn rejects_unbounded_price_lines_and_invalid_words() {
        let req = ImageRequest {
            prompt: "scene".into(),
            references: vec![],
            aspect: "9:16".into(),
            resolution: "2K".into(),
        };
        let value = json!({"endpoints":[{"provider_tag":"seed","pricing":[{"billable":"output_image","unit":"token","cost_usd":"0.01"}]}]});
        assert!(fixed_image_quote(&value, &req).is_err());
        let transcript = Transcript {
            text: "bad".into(),
            duration: 1.0,
            words: vec![Word {
                word: "bad".into(),
                start: 0.7,
                end: 0.2,
            }],
        };
        assert!(transcript.validate().is_err());
    }
    #[tokio::test]
    async fn image_selector_suffix_is_not_sent_as_model_id() {
        use tokio::{
            io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut line = String::new();
            socket.read_line(&mut line).await.unwrap();
            assert!(line.starts_with("GET /images/models/test/model/endpoints "));
            while line != "\r\n" {
                line.clear();
                socket.read_line(&mut line).await.unwrap();
            }
            let body=json!({"endpoints":[{"provider_tag":"fixture","pricing":[{"billable":"output_image","unit":"image","cost_usd":"0.018"}]}]}).to_string();
            socket.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            let _ = socket.get_mut().shutdown().await;
            let mut empty = Vec::new();
            let _ = socket.read_to_end(&mut empty).await;
        });
        let client = OpenRouter::new(
            "fixture",
            format!("http://127.0.0.1:{port}"),
            secrecy::SecretString::from("test-key"),
            std::time::Duration::from_secs(2),
        )
        .unwrap();
        let req = ImageRequest {
            prompt: "scene".into(),
            references: vec![],
            aspect: "9:16".into(),
            resolution: "2K".into(),
        };
        let quote = client
            .image_quote("test/model@2K", &req, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(quote.ceiling, Usd::from_micros(18_000));
        server.await.unwrap();
    }
}
