//! Offline structured text through llama.cpp's one-shot completion binary.
//! Only local GGUF paths are accepted; no model downloads or server are used.

use std::path::PathBuf;

use async_trait::async_trait;
use indexmap::IndexMap;
use tokio_util::sync::CancellationToken;

use crate::ProviderError;
use crate::process::{self, Limits};
use crate::text::{Billing, TextProvider, TextRequest, TextResponse, Usage, parse_json_text};

#[derive(Debug)]
pub struct LocalText {
    id: String,
    binary: PathBuf,
    models: IndexMap<String, PathBuf>,
    limits: Limits,
    max_tokens: u32,
    context_size: u32,
}

impl LocalText {
    pub fn new(
        id: &str,
        binary: PathBuf,
        models: &IndexMap<String, PathBuf>,
        limits: Limits,
        max_tokens: u32,
        context_size: u32,
    ) -> Result<Self, String> {
        if models.is_empty() || max_tokens == 0 || context_size <= max_tokens {
            return Err("local_text needs models and 0 < max_tokens < context_size".into());
        }
        // Remote settings must not influence a locally billed provider. Even an empty
        // RPC variable is rejected by llama.cpp builds without RPC support.
        for name in [
            "LLAMA_ARG_HF_REPO",
            "LLAMA_ARG_HF_FILE",
            "LLAMA_ARG_MODEL_URL",
            "LLAMA_ARG_RPC",
        ] {
            if std::env::var_os(name).is_some() {
                return Err(format!(
                    "unset {name} before local_text inference; remote model/RPC settings are forbidden"
                ));
            }
        }
        let models = models.iter().map(|(name, path)| {
            if !path.is_absolute() || !path.is_file() {
                return Err(format!("local model {} must be an existing absolute GGUF path; install it before running", path.display()));
            }
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            if path.extension().is_none_or(|ext| ext != "gguf") {
                return Err(format!("{} is not a GGUF model", path.display()));
            }
            Ok((name.clone(), path))
        }).collect::<Result<_, String>>()?;
        Ok(Self {
            id: id.into(),
            binary,
            models,
            limits,
            max_tokens,
            context_size,
        })
    }
}

#[async_trait]
impl TextProvider for LocalText {
    fn id(&self) -> &str {
        &self.id
    }
    fn billing(&self) -> Billing {
        Billing::Local
    }
    fn supports_images(&self) -> bool {
        bool::default()
    }
    fn resolve_model(&self, selector: &str) -> String {
        self.models
            .get(selector)
            .map_or_else(|| selector.into(), |p| p.to_string_lossy().into_owned())
    }

    async fn generate(
        &self,
        req: &TextRequest,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<TextResponse, ProviderError> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        if !req.images.is_empty() {
            return Err(ProviderError::Unsupported("local_text is text-only".into()));
        }
        if req.web {
            return Err(ProviderError::Unsupported(
                "local_text cannot search the web".into(),
            ));
        }
        if !self.models.values().any(|p| p.to_string_lossy() == model) {
            return Err(ProviderError::Unavailable(format!(
                "unknown local model {model}; configure providers.<id>.models"
            )));
        }
        let dir = tempfile::tempdir().map_err(|e| ProviderError::Failed(e.to_string()))?;
        let schema = dir.path().join("schema.json");
        let prompt = dir.path().join("prompt.txt");
        let system = dir.path().join("system.txt");
        for (path, text) in [
            (&schema, req.schema.to_string()),
            (
                &prompt,
                format!(
                    "{}\n\nReturn only JSON matching this schema:\n{}",
                    req.prompt, req.schema
                ),
            ),
            (
                &system,
                req.system
                    .clone()
                    .unwrap_or_else(|| "Return only the requested JSON.".into()),
            ),
        ] {
            std::fs::write(path, text).map_err(|e| ProviderError::Failed(e.to_string()))?;
        }
        let mut args: Vec<String> = [
            "--offline",
            "--conversation",
            "--single-turn",
            "--jinja",
            "--simple-io",
            "--no-display-prompt",
            "--no-perf",
            "--no-warmup",
            "--no-context-shift",
            "--no-escape",
            "--temp",
            "0",
            "--seed",
            "0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for (flag, value) in [
            ("--model", model.to_owned()),
            ("--file", prompt.to_string_lossy().into_owned()),
            (
                "--system-prompt-file",
                system.to_string_lossy().into_owned(),
            ),
            ("--json-schema-file", schema.to_string_lossy().into_owned()),
            ("--n-predict", self.max_tokens.to_string()),
            ("--ctx-size", self.context_size.to_string()),
        ] {
            args.extend([flag.into(), value]);
        }
        let output =
            process::run(&self.binary, &args, "", dir.path(), &self.limits, cancel).await?;
        if output.status != Some(0) {
            return Err(ProviderError::Failed(format!(
                "local llama.cpp exited {:?}; check the GGUF, context/token limits and llama-completion version: {}",
                output.status,
                output.stderr.chars().take(400).collect::<String>()
            )));
        }
        let text = output.stdout.trim();
        let text = text.strip_suffix("[end of text]").unwrap_or(text).trim();
        Ok(TextResponse {
            value: parse_json_text(text)?,
            usage: Usage::default(),
        })
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    fn fixture(body: &str, timeout: Duration) -> (tempfile::TempDir, LocalText) {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("fixture.gguf");
        std::fs::write(&model, b"fixture, not a real model").unwrap();
        let binary = dir.path().join("completion");
        std::fs::write(&binary, format!("#!/usr/bin/env python3\n{body}\n")).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let provider = LocalText::new(
            "local",
            binary,
            &IndexMap::from([("metadata".into(), model)]),
            Limits {
                timeout,
                ..Limits::default()
            },
            512,
            4096,
        )
        .unwrap();
        (dir, provider)
    }

    fn request() -> TextRequest {
        TextRequest {
            system: Some("Guide rules".into()),
            prompt: "Keep literal \\n and اردو".into(),
            schema: json!({"type":"object","properties":{"title":{"type":"string"}},"required":["title"],"additionalProperties":false}),
            images: vec![],
            web: false,
        }
    }

    #[tokio::test]
    async fn passes_offline_schema_and_prompt_without_shell_or_echo() {
        let (_dir, provider) = fixture(
            r#"import sys,json,pathlib,os
a=sys.argv[1:]
assert '--offline' in a and '--single-turn' in a and '--no-display-prompt' in a
assert '--no-context-shift' in a and '--no-escape' in a
assert json.loads(pathlib.Path(a[a.index('--json-schema-file')+1]).read_text())['type']=='object'
assert pathlib.Path(a[a.index('--file')+1]).read_text().startswith('Keep literal \\n and اردو')
assert pathlib.Path(a[a.index('--system-prompt-file')+1]).read_text()=='Guide rules'
print('{"title":"A local title"} [end of text]')"#,
            Duration::from_secs(5),
        );
        let response = provider
            .generate(
                &request(),
                &provider.resolve_model("metadata"),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.value["title"], "A local title");
        assert_eq!(provider.billing(), Billing::Local);
        assert_eq!(response.usage.cost.micros(), 0);
    }

    #[tokio::test]
    async fn rejects_unknown_models_images_and_bad_replies() {
        let (_dir, provider) = fixture("print('not JSON')", Duration::from_secs(5));
        let cancel = CancellationToken::new();
        assert!(matches!(
            provider.generate(&request(), "missing", &cancel).await,
            Err(ProviderError::Unavailable(_))
        ));
        let mut req = request();
        req.images.push("image.png".into());
        let model = provider.resolve_model("metadata");
        assert!(matches!(
            provider.generate(&req, &model, &cancel).await,
            Err(ProviderError::Unsupported(_))
        ));
        assert!(matches!(
            provider.generate(&request(), &model, &cancel).await,
            Err(ProviderError::Malformed(_))
        ));
        cancel.cancel();
        assert!(matches!(
            provider.generate(&request(), &model, &cancel).await,
            Err(ProviderError::Cancelled)
        ));
    }

    #[tokio::test]
    async fn bounds_inference_and_propagates_worker_errors() {
        let (_dir, provider) = fixture("import time\ntime.sleep(10)", Duration::from_millis(100));
        assert!(matches!(
            provider
                .generate(
                    &request(),
                    &provider.resolve_model("metadata"),
                    &CancellationToken::new()
                )
                .await,
            Err(ProviderError::Timeout(_))
        ));
        let (_dir, provider) = fixture("raise SystemExit(2)", Duration::from_secs(30));
        assert!(matches!(
            provider
                .generate(
                    &request(),
                    &provider.resolve_model("metadata"),
                    &CancellationToken::new()
                )
                .await,
            Err(ProviderError::Failed(_))
        ));
    }
}
