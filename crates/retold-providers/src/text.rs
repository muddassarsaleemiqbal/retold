//! The text/vision contract every provider implements, and JSON Schema helpers.

use std::path::PathBuf;

use async_trait::async_trait;
use retold_core::Usd;
use schemars::JsonSchema;
use schemars::r#gen::SchemaSettings;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::error::ProviderError;

/// How a provider is paid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Billing {
    /// Covered by a subscription you already pay for: no extra cost per call.
    Subscription,
    /// Charged per call.
    PerUse,
    /// Runs on this machine.
    Local,
}

/// Output tokens a paid text reply may use, reasoning included; also its quoted ceiling.
pub const MAX_OUTPUT_TOKENS: u32 = 24_000;

/// A request for structured output.
#[derive(Debug, Clone)]
pub struct TextRequest {
    pub system: Option<String>,
    pub prompt: String,
    /// JSON Schema the reply must match (strict form; see [`strict_schema`]).
    pub schema: Value,
    /// Images to show the model (vision).
    pub images: Vec<PathBuf>,
    /// Let the model search and read the web while answering (research only).
    pub web: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// What was actually charged; zero on subscriptions.
    pub cost: Usd,
    /// Whether a pay-per-use response supplied its exact cost.
    pub cost_known: bool,
}

#[derive(Debug, Clone)]
pub struct TextResponse {
    pub value: Value,
    pub usage: Usage,
}

/// A provider that can answer text (and possibly vision) requests with structured JSON.
#[async_trait]
pub trait TextProvider: Send + Sync + std::fmt::Debug {
    /// Id from `providers.toml`.
    fn id(&self) -> &str;
    fn billing(&self) -> Billing;
    fn supports_images(&self) -> bool;
    /// Turns a route selector (`smart`, `default`, a model id) into a model name.
    fn resolve_model(&self, selector: &str) -> String;
    async fn generate(
        &self,
        req: &TextRequest,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<TextResponse, ProviderError>;
}

/// The strict JSON Schema for `T`: everything inlined, every property required, no extra
/// properties. This is the shape OpenAI-style strict structured outputs accept, and it is
/// also valid for Claude.
#[must_use]
pub fn strict_schema<T: JsonSchema>() -> Value {
    let settings = SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
        s.meta_schema = None;
    });
    let schema = settings.into_generator().into_root_schema_for::<T>();
    let mut v = serde_json::to_value(schema).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        obj.remove("title");
        obj.remove("definitions");
    }
    make_strict(&mut v);
    v
}

fn make_strict(v: &mut Value) {
    match v {
        Value::Object(map) => {
            let is_object = map.get("type").is_some_and(|t| {
                t == "object"
                    || t.as_array()
                        .is_some_and(|a| a.iter().any(|x| x == "object"))
            });
            if is_object {
                if let Some(Value::Object(props)) = map.get("properties") {
                    let keys: Vec<Value> = props.keys().cloned().map(Value::String).collect();
                    map.insert("required".into(), Value::Array(keys));
                }
                map.insert("additionalProperties".into(), Value::Bool(false));
            }
            for child in map.values_mut() {
                make_strict(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(make_strict),
        _ => {}
    }
}

/// Pulls a JSON value out of model text that may be wrapped in a code fence.
pub(crate) fn parse_json_text(text: &str) -> Result<Value, ProviderError> {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(t)
        .trim();
    serde_json::from_str(t).map_err(|e| {
        let snippet: String = t.chars().take(200).collect();
        ProviderError::Malformed(format!("reply is not JSON ({e}): {snippet}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(JsonSchema, serde::Deserialize)]
    #[expect(dead_code, reason = "fields only shape the generated schema")]
    struct Inner {
        a: u32,
        b: Option<String>,
    }

    #[derive(JsonSchema, serde::Deserialize)]
    #[expect(dead_code, reason = "fields only shape the generated schema")]
    struct Outer {
        items: Vec<Inner>,
        note: Option<String>,
    }

    #[test]
    fn strict_schema_requires_everything_and_inlines() {
        let s = strict_schema::<Outer>();
        let text = s.to_string();
        assert!(!text.contains("$ref"), "{text}");
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["required"], serde_json::json!(["items", "note"]));
        assert_eq!(
            s["properties"]["items"]["items"]["required"],
            serde_json::json!(["a", "b"])
        );
    }

    #[test]
    fn parses_fenced_json() {
        assert_eq!(parse_json_text("```json\n{\"a\":1}\n```").unwrap()["a"], 1);
        assert!(parse_json_text("sure! here").is_err());
    }
}
