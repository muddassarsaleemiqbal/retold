//! Clean Code gate: every crate is checked against `clean-code.toml` at the strictest preset.
//!
//! Findings not yet fixed are frozen per rule in `clean-code-baseline.json`. The gate fails when a
//! rule gains findings and when a rule loses them without the baseline being lowered, so the
//! baseline can only shrink. Regenerate it after a cleanup with
//! `RETOLD_CLEAN_CODE_BASELINE=update cargo test -p retold-cli --test clean_code`.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use clean_code_lints::{Config, Diagnostic, analyze_paths};

const BASELINE_FILE: &str = "clean-code-baseline.json";
const CONFIG_FILE: &str = "clean-code.toml";
const UPDATE_VARIABLE: &str = "RETOLD_CLEAN_CODE_BASELINE";
const EXAMPLES_PER_RULE: usize = 10;

type Counts = BTreeMap<String, usize>;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// First line of the file's `#[cfg(test)] mod` block; in-file tests sit at the end of a file.
fn test_module_start(text: &str) -> Option<usize> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    lines.iter().enumerate().find_map(|(index, line)| {
        let attribute = line.starts_with("#[cfg(test)]") || line.starts_with("#[cfg(all(test,");
        let module = lines
            .iter()
            .skip(index + 1)
            .find(|next| !next.starts_with("#["))
            .is_some_and(|next| next.starts_with("mod "));
        (attribute && module).then_some(index + 1)
    })
}

struct TestRegions {
    starts: std::cell::RefCell<HashMap<PathBuf, Option<usize>>>,
}

impl TestRegions {
    fn contains(&self, diagnostic: &Diagnostic) -> bool {
        let path = &diagnostic.file;
        let in_test_dir = path.components().any(|part| part.as_os_str() == "tests");
        let mut starts = self.starts.borrow_mut();
        let start = starts.entry(path.clone()).or_insert_with(|| {
            fs::read_to_string(path)
                .ok()
                .as_deref()
                .and_then(test_module_start)
        });
        in_test_dir || start.is_some_and(|line| diagnostic.start.line >= line)
    }
}

/// Panicking on a broken assumption is how a test fails, so test code may unwrap.
fn exempt_in_tests(diagnostic: &Diagnostic) -> bool {
    diagnostic.rule == "no-overridden-safeties"
        && (diagnostic.message.contains("`.unwrap()`")
            || diagnostic.message.contains("`.expect()`"))
}

/// Required Serde writer signatures cannot be changed by the adapter implementor.
fn is_required_serde_signature(diagnostic: &Diagnostic) -> bool {
    let required: &[&str] = match diagnostic.rule {
        "command-query-separation" => &["serialize_key", "serialize_value"],
        "max-arguments" => &[
            "serialize_newtype_variant",
            "serialize_tuple_variant",
            "serialize_struct_variant",
        ],
        _ => &[],
    };
    let adapter = diagnostic
        .file
        .ends_with("retold-engine/src/store/serialization.rs");
    let source = fs::read_to_string(&diagnostic.file).unwrap_or_default();
    let signature = source
        .lines()
        .nth(diagnostic.start.line.saturating_sub(1))
        .unwrap_or_default();
    adapter
        && required.iter().any(|method| {
            signature.contains(&format!("fn {method}<"))
                || signature.contains(&format!("fn {method}("))
        })
}

fn findings(root: &Path) -> Vec<Diagnostic> {
    let config = Config::from_file(&root.join(CONFIG_FILE)).unwrap();
    let report = analyze_paths(&[root.join("crates")], &config);
    assert!(
        report.parse_errors.is_empty(),
        "files failed to parse: {:?}",
        report.parse_errors
    );
    let regions = TestRegions {
        starts: std::cell::RefCell::new(HashMap::new()),
    };
    report
        .diagnostics
        .into_iter()
        .filter(|diagnostic| !(exempt_in_tests(diagnostic) && regions.contains(diagnostic)))
        .filter(|diagnostic| !is_required_serde_signature(diagnostic))
        .collect()
}

fn count(diagnostics: &[Diagnostic]) -> Counts {
    let mut counts = Counts::new();
    for diagnostic in diagnostics {
        *counts.entry(diagnostic.rule.to_owned()).or_default() += 1;
    }
    counts
}

fn examples(diagnostics: &[Diagnostic], rule: &str, root: &Path) -> String {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == rule)
        .take(EXAMPLES_PER_RULE)
        .map(|diagnostic| {
            let file = diagnostic
                .file
                .strip_prefix(root)
                .unwrap_or(&diagnostic.file);
            format!(
                "    {}:{}  {}",
                file.display(),
                diagnostic.start.line,
                diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn problems(
    actual: &Counts,
    baseline: &Counts,
    diagnostics: &[Diagnostic],
    root: &Path,
) -> Vec<String> {
    let rules = actual
        .keys()
        .chain(baseline.keys())
        .collect::<std::collections::BTreeSet<_>>();
    rules
        .into_iter()
        .filter_map(|rule| {
            let now = actual.get(rule).copied().unwrap_or_default();
            let allowed = baseline.get(rule).copied().unwrap_or_default();
            match now.cmp(&allowed) {
                std::cmp::Ordering::Greater => Some(format!(
                    "{rule}: {now} findings, baseline allows {allowed}. Fix the new ones:\n{}",
                    examples(diagnostics, rule, root)
                )),
                std::cmp::Ordering::Less => Some(format!(
                    "{rule}: {now} findings, below the baseline of {allowed}. Lower the baseline \
                     ({UPDATE_VARIABLE}=update) so the fix stays fixed."
                )),
                std::cmp::Ordering::Equal => None,
            }
        })
        .collect()
}

fn report_env_findings(diagnostics: &[Diagnostic], root: &Path) {
    if let Ok(target) = std::env::var("RETOLD_PRINT_RULE") {
        for diagnostic in diagnostics {
            if target == "all" || diagnostic.rule == target {
                let file = diagnostic
                    .file
                    .strip_prefix(root)
                    .unwrap_or(&diagnostic.file);
                println!(
                    "RULE_FINDING {}:{} [{}] {}",
                    file.display(),
                    diagnostic.start.line,
                    diagnostic.rule,
                    diagnostic.message
                );
            }
        }
    }
}

#[test]
fn code_meets_clean_code_rules_without_growing_the_baseline() {
    let root = workspace_root();
    let diagnostics = findings(&root);
    report_env_findings(&diagnostics, &root);
    let actual = count(&diagnostics);
    let baseline_path = root.join(BASELINE_FILE);
    if std::env::var(UPDATE_VARIABLE).as_deref() == Ok("update") {
        fs::write(
            &baseline_path,
            serde_json::to_string_pretty(&actual).unwrap() + "\n",
        )
        .unwrap();
        return;
    }
    let baseline: Counts =
        serde_json::from_str(&fs::read_to_string(&baseline_path).unwrap()).unwrap();
    let problems = problems(&actual, &baseline, &diagnostics, &root);
    assert!(
        problems.is_empty(),
        "Clean Code gate:\n\n{}",
        problems.join("\n\n")
    );
}
