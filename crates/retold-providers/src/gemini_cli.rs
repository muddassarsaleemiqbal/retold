//! Optional Gemini text adapter. Empty tool and MCP allowlists keep calls text-only.
use crate::process::{self, Limits};
use crate::{Billing, ProviderError, TextProvider, TextRequest, TextResponse, Usage};
use async_trait::async_trait;
use indexmap::IndexMap;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
#[derive(Debug)]
pub struct GeminiCli {
    pub id: String,
    pub binary: PathBuf,
    pub tiers: IndexMap<String, String>,
    pub limits: Limits,
}
#[async_trait]
impl TextProvider for GeminiCli {
    fn id(&self) -> &str {
        &self.id
    }
    fn billing(&self) -> Billing {
        Billing::Subscription
    }
    fn supports_images(&self) -> bool {
        bool::default()
    }
    fn resolve_model(&self, selector: &str) -> String {
        self.tiers
            .get(selector)
            .cloned()
            .unwrap_or_else(|| selector.into())
    }
    async fn generate(
        &self,
        req: &TextRequest,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<TextResponse, ProviderError> {
        if !req.images.is_empty() {
            return Err(ProviderError::Unsupported(
                "Gemini adapter is text-only".into(),
            ));
        }
        let temp = tempfile::tempdir().map_err(|e| ProviderError::Failed(e.to_string()))?;
        let config = temp.path().join(".gemini");
        std::fs::create_dir(&config).map_err(|e| ProviderError::Failed(e.to_string()))?;
        std::fs::write(config.join("settings.json"),if req.web {r#"{"tools":{"core":["google_web_search","web_fetch"]},"mcp":{"allowed":[]},"useWriteTodos":false,"general":{"sessionRetention":{"enabled":false}}}"#} else {r#"{"tools":{"core":[]},"mcp":{"allowed":[]},"useWriteTodos":false,"general":{"sessionRetention":{"enabled":false}}}"#}).map_err(|e|ProviderError::Failed(e.to_string()))?;
        let mut args = vec![
            "--output-format".into(),
            "json".into(),
            "--sandbox".into(),
            "--prompt".into(),
            "Return only the requested JSON.".into(),
        ];
        if model != "default" {
            args.extend(["--model".into(), model.into()]);
        }
        let prompt = format!(
            "{}\n{}\nOutput JSON conforming exactly to this schema: {}",
            req.system.as_deref().unwrap_or_default(),
            req.prompt,
            req.schema
        );
        let output = process::run(
            &self.binary,
            &args,
            &prompt,
            temp.path(),
            &self.limits,
            cancel,
        )
        .await?;
        parse(&output.stdout)
    }
}
fn parse(stdout: &str) -> Result<TextResponse, ProviderError> {
    let envelope: serde_json::Value =
        serde_json::from_str(stdout).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    if let Some(error) = envelope.get("error") {
        return Err(crate::error::classify_cli_failure(
            "gemini",
            &error.to_string(),
        ));
    }
    let text = envelope["response"]
        .as_str()
        .ok_or_else(|| ProviderError::Malformed("Gemini response missing".into()))?;
    Ok(TextResponse {
        value: crate::text::parse_json_text(text)?,
        usage: Usage::default(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_json_and_refuses_missing_response() {
        assert_eq!(
            parse(r#"{"response":"{\"ok\":true}"}"#).unwrap().value["ok"],
            true
        );
        assert!(parse("{}").is_err());
    }
}
