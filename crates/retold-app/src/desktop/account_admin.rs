//! Native account limit changes and provider tests, both reviewed before they take effect.
use std::path::Path;

use crate::account::limits::{self, Input};
use crate::account::types::Window;
use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::provider_tests::{self, TestOptions};

/// One limit edit: a scope such as `installation`, a window and the new USD and request caps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitEdit {
    pub scope: String,
    pub window: Window,
    pub usd: Option<String>,
    pub requests: Option<u64>,
    pub remove: bool,
}

/// A previewed limit change: what it changes, headroom and exhausted units, as JSON.
#[derive(Debug, Clone)]
pub struct LimitReview {
    pub edit: LimitEdit,
    pub preview: serde_json::Value,
}

/// A reviewed provider-test plan; only its hash authorizes the probes.
#[derive(Debug, Clone)]
pub struct TestReview {
    pub provider: Option<String>,
    pub plan: serde_json::Value,
    pub plan_hash: String,
}

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        Payload::Error { diagnostic, .. } => Err(diagnostic),
        Payload::Event(_) => Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Unexpected account response.",
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Preview,
    Apply,
}

fn limit(edit: &LimitEdit, mode: Mode) -> Result<serde_json::Value, Diagnostic> {
    let input = Input {
        scope: &edit.scope,
        window: edit.window,
        usd: edit.usd.as_deref(),
        requests: edit.requests,
        resources: &[],
        remove: edit.remove,
        dry_run: mode == Mode::Preview,
    };
    let mut value = contents(limits::respond(&input))?;
    let changed = value
        .get_mut("limit_changed")
        .and_then(serde_json::Value::as_object_mut);
    if let Some(changed) = changed {
        changed.remove("dry_run");
    }
    Ok(value)
}

/// Preview a limit change without writing.
pub fn review_limit(edit: LimitEdit) -> Result<LimitReview, Diagnostic> {
    let preview = limit(&edit, Mode::Preview)?;
    Ok(LimitReview { edit, preview })
}

/// Apply only if a fresh preview still matches the reviewed one exactly.
pub fn apply_limit(review: &LimitReview) -> Result<serde_json::Value, Diagnostic> {
    let unchanged = limit(&review.edit, Mode::Preview)? == review.preview;
    if !unchanged {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "Account usage or limits changed. Review the limit again.",
        ));
    }
    limit(&review.edit, Mode::Apply)
}

const fn test_options<'a>(root: &'a Path, provider: Option<&'a str>) -> TestOptions<'a> {
    TestOptions {
        root,
        provider,
        spend: None,
        plan_hash: None,
        yes: false,
    }
}

/// Plan free provider probes without contacting any provider.
pub fn review_test(settings: &Path, provider: Option<&str>) -> Result<TestReview, Diagnostic> {
    let plan = contents(provider_tests::respond(&test_options(settings, provider)))?;
    let plan_hash = plan
        .get("plan_hash")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            Diagnostic::new(
                ErrorCode::DataInvalid,
                "The provider test plan has no hash.",
            )
        })?
        .to_owned();
    Ok(TestReview {
        provider: provider.map(str::to_owned),
        plan,
        plan_hash,
    })
}

/// Run exactly the reviewed probes; a changed plan is refused before any request.
pub fn run_test(settings: &Path, review: &TestReview) -> Result<serde_json::Value, Diagnostic> {
    let mut options = test_options(settings, review.provider.as_deref());
    options.plan_hash = Some(&review.plan_hash);
    contents(provider_tests::respond(&options))
}
