//! Optional local whisper.cpp transcription. Uses an explicit model file and no network.
use crate::media::{Media, MediaProvider, MediaUsage, Quote, Transcript, Word};
use crate::process::{self, Limits};
use crate::{Billing, ProviderError};
use async_trait::async_trait;
use retold_core::{LanguageTag, Usd};
use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};
use std::fmt;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
#[derive(Debug)]
pub struct LocalWhisper {
    pub id: String,
    pub binary: PathBuf,
    pub model: PathBuf,
    pub ffmpeg: PathBuf,
    pub limits: Limits,
}
#[async_trait]
impl MediaProvider for LocalWhisper {
    fn id(&self) -> &str {
        &self.id
    }
    fn billing(&self) -> Billing {
        Billing::Subscription
    }
    async fn image_quote(
        &self,
        _: &str,
        _: &crate::media::ImageRequest,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Err(ProviderError::Unsupported(
            "local Whisper is transcription-only".into(),
        ))
    }
    async fn speech_quote(
        &self,
        _: &str,
        _: &crate::media::SpeechRequest,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Err(ProviderError::Unsupported(
            "local Whisper is transcription-only".into(),
        ))
    }
    async fn image(
        &self,
        _: &str,
        _: &crate::media::ImageRequest,
        _: &Quote,
        _: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        Err(ProviderError::Unsupported(
            "local Whisper is transcription-only".into(),
        ))
    }
    async fn speech(
        &self,
        _: &str,
        _: &crate::media::SpeechRequest,
        _: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        Err(ProviderError::Unsupported(
            "local Whisper is transcription-only".into(),
        ))
    }
    async fn transcript_quote(
        &self,
        _: &str,
        _: f64,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        if !self.model.is_file() {
            return Err(ProviderError::Unavailable(format!(
                "local Whisper model missing: {}",
                self.model.display()
            )));
        }
        Ok(Quote {
            ceiling: Usd::ZERO,
            provider_tag: None,
        })
    }
    async fn transcribe(
        &self,
        _: &str,
        audio: &[u8],
        _: &str,
        lang: &LanguageTag,
        cancel: &CancellationToken,
    ) -> Result<Media<Transcript>, ProviderError> {
        let temp = tempfile::tempdir().map_err(|e| ProviderError::Failed(e.to_string()))?;
        let input = temp.path().join("input.audio");
        let wav = temp.path().join("input.wav");
        let output = temp.path().join("words");
        std::fs::write(&input, audio).map_err(|e| ProviderError::Failed(e.to_string()))?;
        let convert = process::run(
            &self.ffmpeg,
            &[
                "-nostdin".into(),
                "-v".into(),
                "error".into(),
                "-i".into(),
                input.display().to_string(),
                "-ar".into(),
                "16000".into(),
                "-ac".into(),
                "1".into(),
                "-c:a".into(),
                "pcm_s16le".into(),
                wav.display().to_string(),
            ],
            "",
            temp.path(),
            &self.limits,
            cancel,
        )
        .await?;
        if convert.status != Some(0) {
            return Err(ProviderError::Failed(format!(
                "local audio conversion failed: {}",
                convert.stderr
            )));
        }
        let result = process::run(
            &self.binary,
            &[
                "-m".into(),
                self.model.display().to_string(),
                "-f".into(),
                wav.display().to_string(),
                "-l".into(),
                lang.language().into(),
                "-ojf".into(),
                "-ml".into(),
                "1".into(),
                "-sow".into(),
                "-of".into(),
                output.display().to_string(),
            ],
            "",
            temp.path(),
            &self.limits,
            cancel,
        )
        .await?;
        if result.status != Some(0) {
            return Err(ProviderError::Failed(format!(
                "local Whisper failed: {}",
                result.stderr
            )));
        }
        let bytes = std::fs::read(output.with_extension("json"))
            .map_err(|e| ProviderError::Failed(e.to_string()))?;
        let duration = crate::media::audio_duration(audio)?;
        let transcript = parse(&bytes, duration)?;
        Ok(Media {
            value: transcript,
            usage: MediaUsage {
                generation: None,
                cost: Some(Usd::ZERO),
            },
        })
    }
}
#[derive(Deserialize)]
struct Document {
    transcription: Vec<Segment>,
}
struct Segment {
    text: String,
    offsets: Offsets,
}
#[derive(Deserialize)]
struct Offsets {
    from: f64,
    to: f64,
}

impl<'de> Deserialize<'de> for Segment {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SegmentVisitor;
        impl<'de> Visitor<'de> for SegmentVisitor {
            type Value = Segment;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("Whisper word segment")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Segment, M::Error> {
                parse_whisper_segment(map)
            }
        }
        deserializer.deserialize_map(SegmentVisitor)
    }
}

fn parse(bytes: &[u8], duration: f64) -> Result<Transcript, ProviderError> {
    // Token metadata can contain incomplete Unicode BPE fragments. Deserialize only
    // complete word segments; their text and offsets remain strictly validated.
    // Some zero-duration BPE fragments are raw truncated UTF-8. Mark them so
    // the segment visitor can discard only those untimed fragments; replacement
    // characters in actual timed words are rejected.
    let normalized = String::from_utf8_lossy(bytes);
    let document: Document =
        serde_json::from_str(&normalized).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let mut words = Vec::new();
    let mut texts = Vec::new();
    let count = document.transcription.len();
    for (index, segment) in document.transcription.into_iter().enumerate() {
        if !segment.offsets.from.is_finite()
            || !segment.offsets.to.is_finite()
            || segment.offsets.from < 0.0
            || segment.offsets.to < segment.offsets.from
        {
            return Err(ProviderError::Malformed(
                "invalid local word offsets".into(),
            ));
        }
        let text = segment.text.trim();
        if text.is_empty() || text.starts_with('[') {
            continue;
        }
        if text.split_whitespace().count() != 1 {
            return Err(ProviderError::Malformed(
                "local Whisper did not return word-level segments; update whisper-cli".into(),
            ));
        }
        texts.push(text.to_owned());
        let start = segment.offsets.from / 1000.0;
        let mut end = segment.offsets.to / 1000.0;
        // whisper.cpp timestamps the final word against its padded decode
        // window. Bound that final interval to the actual audio, retaining its
        // measured start; other out-of-range intervals remain errors.
        if index + 1 == count && start < duration && end > duration {
            end = duration;
        }
        if end > start {
            words.push(Word {
                word: text.into(),
                start,
                end,
            });
        }
    }
    let text = texts.join(" ");
    let transcript = Transcript {
        text,
        duration,
        words,
    };
    transcript.validate()?;
    Ok(transcript)
}
fn parse_whisper_segment<'de, M: MapAccess<'de>>(mut map: M) -> Result<Segment, M::Error> {
    let mut offsets: Option<Offsets> = None;
    let mut text = None;
    while let Some(key) = map.next_key::<String>()? {
        match key.as_str() {
            "offsets" => {
                if offsets.is_some() {
                    return Err(de::Error::duplicate_field("offsets"));
                }
                offsets = Some(map.next_value()?);
            }
            "text" => {
                if text.is_some() {
                    return Err(de::Error::duplicate_field("text"));
                }
                text = Some(map.next_value::<Box<serde_json::value::RawValue>>()?);
            }
            _ => {
                map.next_value::<de::IgnoredAny>()?;
            }
        }
    }
    Ok(Segment {
        text: {
            let raw = text.ok_or_else(|| de::Error::missing_field("text"))?;
            match serde_json::from_str::<String>(raw.get()) {
                Ok(value) if !value.contains('\u{fffd}') => value,
                Ok(_)
                    if offsets
                        .as_ref()
                        .is_some_and(|o| o.from >= 0.0 && o.to <= o.from) =>
                {
                    String::new()
                }
                Ok(_) => {
                    return Err(de::Error::custom("invalid Unicode in timed word"));
                }
                Err(_)
                    if offsets
                        .as_ref()
                        .is_some_and(|o| o.from >= 0.0 && o.to <= o.from) =>
                {
                    String::new()
                }
                Err(error) => return Err(de::Error::custom(error)),
            }
        },
        offsets: offsets.ok_or_else(|| de::Error::missing_field("offsets"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_token_fragments_preserve_complete_word_segments() {
        let raw=br#"{"transcription":[{"text":"hello","offsets":{"from":0,"to":300},"tokens":[{"text":"\ud800"}]}]}"#;
        assert_eq!(parse(raw, 1.0).unwrap().text, "hello");
    }
    #[test]
    fn zero_duration_invalid_unicode_fragment_is_not_a_timed_word() {
        let mut raw = br#"{"transcription":[{"offsets":{"from":0,"to":0},"text":""#.to_vec();
        raw.extend([0xe0, 0xa4]);
        raw.extend(br#""},{"offsets":{"from":0,"to":300},"text":"hello"}]}"#);
        assert_eq!(parse(&raw, 1.0).unwrap().text, "hello");
        let mut timed = br#"{"transcription":[{"offsets":{"from":0,"to":300},"text":""#.to_vec();
        timed.extend([0xe0, 0xa4]);
        timed.extend(br#""}]}"#);
        assert!(
            parse(&timed, 1.0).is_err(),
            "timed words must have valid Unicode"
        );
    }
    #[test]
    fn validates_word_level_offsets() {
        let v = serde_json::json!({"transcription":[{"text":"hello","offsets":{"from":0,"to":300}},{"text":"world","offsets":{"from":300,"to":600}}]});
        let t = parse(&serde_json::to_vec(&v).unwrap(), 1.0).unwrap();
        assert_eq!(t.text, "hello world");
        assert_eq!(t.words[1].start, 0.3);
        assert!(parse(b"{}", 1.0).is_err());
    }
    #[test]
    fn valid_zero_duration_words_remain_in_accuracy_text_and_final_window_is_bounded() {
        let v = serde_json::json!({"transcription":[
            {"text":"hello", "offsets":{"from":0,"to":300}},
            {"text":"there", "offsets":{"from":300,"to":300}},
            {"text":"world", "offsets":{"from":300,"to":1000}}
        ]});
        let t = parse(&serde_json::to_vec(&v).unwrap(), 0.8).unwrap();
        assert_eq!(t.text, "hello there world");
        assert_eq!(t.words.len(), 2);
        assert_eq!(t.words[1].end, 0.8);
        assert!(parse(&serde_json::to_vec(&v).unwrap(), 0.2).is_err());
    }
}
