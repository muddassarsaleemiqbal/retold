//! Codex CLI on a subscription login.
//!
//! `codex exec --json --output-schema <file> --sandbox read-only --ephemeral
//!  --skip-git-repo-check -C <empty tmp> [-m <model>] [-i <image>…] -`
//! with the prompt on stdin. The read-only sandbox means the agent cannot change anything.
//! Output is JSON lines: `thread.started`, `item.completed` (`agent_message`), `turn.completed`.

use std::path::PathBuf;

use async_trait::async_trait;
use indexmap::IndexMap;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::error::{ProviderError, classify_cli_failure};
use crate::process::{self, Limits};
use crate::text::{Billing, TextProvider, TextRequest, TextResponse, Usage, parse_json_text};

/// Selector meaning "Codex's configured default model" (no `-m`).
pub const DEFAULT_MODEL: &str = "default";

#[derive(Debug)]
pub struct CodexCli {
    pub id: String,
    pub binary: PathBuf,
    pub tiers: IndexMap<String, String>,
    pub limits: Limits,
}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    item: Option<Item>,
    #[serde(default)]
    usage: Option<EventUsage>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Item {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

/// What a Codex run printed, reduced to what Retold needs.
#[derive(Debug, Default)]
pub(crate) struct Run {
    pub thread_id: Option<String>,
    pub last_message: Option<String>,
    pub usage: Usage,
    pub failure: Option<String>,
}

pub(crate) fn parse_events(stdout: &str) -> Run {
    let mut run = Run::default();
    for line in stdout.lines().filter(|l| l.trim_start().starts_with('{')) {
        let Ok(ev) = serde_json::from_str::<Event>(line) else {
            continue;
        };
        match ev.kind.as_str() {
            "thread.started" => run.thread_id = ev.thread_id,
            "item.completed" => {
                if let Some(item) = ev.item.filter(|i| i.kind == "agent_message") {
                    run.last_message = item.text;
                }
            }
            "turn.completed" => {
                if let Some(u) = ev.usage {
                    run.usage.input_tokens += u.input_tokens;
                    run.usage.output_tokens += u.output_tokens;
                }
            }
            _ => match ev.kind.as_str() {
                "turn.failed" | "error" => {
                    let detail = ev
                        .message
                        .or_else(|| {
                            ev.error.map(|e| {
                                e.get("message")
                                    .and_then(|m| m.as_str())
                                    .map_or_else(|| e.to_string(), str::to_owned)
                            })
                        })
                        .unwrap_or_else(|| ev.kind.clone());
                    run.failure = Some(detail);
                }
                _ => {}
            },
        }
    }
    run
}

/// Parameters for one `codex exec` self.
struct Invocation<'a> {
    model: &'a str,
    schema_file: &'a std::path::Path,
    cwd: &'a std::path::Path,
    images: &'a [PathBuf],
    web: bool,
}

impl Invocation<'_> {
    fn to_args(&self) -> Vec<String> {
        let mut arguments: Vec<String> = if self.web {
            vec!["--search".into()]
        } else {
            vec![]
        };
        arguments.extend::<[String; 1]>(["exec".into()]);
        arguments.extend::<Vec<String>>(vec![
            "--json".into(),
            "--output-schema".into(),
            self.schema_file.display().to_string(),
            "--sandbox".into(),
            "read-only".into(),
            "--ephemeral".into(),
            "--skip-git-repo-check".into(),
            "-C".into(),
            self.cwd.display().to_string(),
        ]);
        if self.model != DEFAULT_MODEL {
            arguments.extend(["-m".into(), self.model.to_owned()]);
        }
        for image in self.images {
            arguments.extend(["-i".into(), image.display().to_string()]);
        }
        arguments.push("-".into());
        arguments
    }
}

#[async_trait]
impl TextProvider for CodexCli {
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
        let TextRequest {
            system,
            prompt,
            schema,
            images,
            web,
        } = req;
        let tmp =
            tempfile::tempdir().map_err(|e| ProviderError::Failed(format!("temp dir: {e}")))?;
        let work = tmp.path().join("work");
        std::fs::create_dir(&work).map_err(|e| ProviderError::Failed(format!("temp dir: {e}")))?;
        let schema_file = tmp.path().join("schema.json");
        std::fs::write(&schema_file, schema.to_string())
            .map_err(|e| ProviderError::Failed(format!("writing schema: {e}")))?;
        let prompt_text = match system {
            Some(s) => format!("{s}\n\n---\n\n{prompt}"),
            None => prompt.clone(),
        };
        let args = Invocation {
            model,
            schema_file: &schema_file,
            cwd: &work,
            images,
            web: *web,
        }
        .to_args();
        let out = process::run(
            &self.binary,
            &args,
            &prompt_text,
            &work,
            &self.limits,
            cancel,
        )
        .await?;
        let run = parse_events(&out.stdout);
        if let Some(failure) = run.failure {
            return Err(classify_cli_failure(&self.id, &failure));
        }
        let Some(text) = run.last_message else {
            let detail = if out.stderr.trim().is_empty() {
                "no reply"
            } else {
                out.stderr.as_str()
            };
            return Err(classify_cli_failure(&self.id, detail));
        };
        Ok(TextResponse {
            value: parse_json_text(&text)?,
            usage: run.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"{"type":"thread.started","thread_id":"01a1"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"{\"ok\":true,\"n\":7}"}}
{"type":"turn.completed","usage":{"input_tokens":24619,"cached_input_tokens":0,"output_tokens":19}}
"#;

    #[test]
    fn reads_the_last_agent_message() {
        let run = parse_events(OK);
        assert_eq!(run.thread_id.as_deref(), Some("01a1"));
        assert_eq!(
            parse_json_text(run.last_message.as_deref().unwrap()).unwrap()["n"],
            7
        );
        assert_eq!(run.usage.input_tokens, 24619);
        assert!(run.failure.is_none());
    }

    #[test]
    fn reads_failures() {
        let run = parse_events(
            r#"{"type":"turn.failed","error":{"message":"You've hit your usage limit"}}"#,
        );
        assert!(matches!(
            classify_cli_failure("codex", &run.failure.unwrap()),
            ProviderError::RateLimited { .. }
        ));
    }

    #[test]
    fn default_model_adds_no_flag() {
        let args = Invocation {
            model: DEFAULT_MODEL,
            schema_file: "s.json".as_ref(),
            cwd: "/tmp/w".as_ref(),
            images: &[],
            web: false,
        }
        .to_args();
        assert!(!args.contains(&"-m".to_owned()));
        assert_eq!(args.last().map(String::as_str), Some("-"));
    }
}
