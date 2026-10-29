//! Turning user answers (flags or wizard replies) into typed, checked guide inputs.

use indexmap::IndexMap;
use serde_json::Value;

use crate::manifest::{GuideManifest, InputSpec, InputType, InputValue};

/// Every declared input with its value (`None` = optional and not given), in guide order.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Inputs(pub IndexMap<String, Option<InputValue>>);

impl Inputs {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&InputValue> {
        self.0.get(name).and_then(Option::as_ref)
    }

    /// `inputs` for templates: every declared input, missing ones as `null`.
    #[must_use]
    pub fn template_context(&self) -> Value {
        let map = self
            .0
            .iter()
            .map(|(k, v)| {
                let v = match v {
                    None => Value::Null,
                    Some(InputValue::Bool(b)) => Value::Bool(*b),
                    Some(InputValue::Int(i)) => Value::from(*i),
                    Some(InputValue::Text(s)) => Value::String(s.clone()),
                };
                (k.clone(), v)
            })
            .collect();
        Value::Object(map)
    }
}

/// The command-line flag for an input name (`source_url` → `source-url`).
#[must_use]
pub fn flag_name(input: &str) -> String {
    input.replace('_', "-")
}

impl InputSpec {
    /// Parses one answer, checking type, choices and bounds.
    pub fn parse(&self, raw: &str) -> Result<InputValue, String> {
        let trimmed = raw.trim();
        match self.kind {
            InputType::Text => {
                if trimmed.is_empty() {
                    Err("cannot be empty".into())
                } else {
                    Ok(InputValue::Text(trimmed.to_owned()))
                }
            }
            InputType::Choice => self
                .choices
                .iter()
                .find(|c| c.eq_ignore_ascii_case(trimmed))
                .map(|c| InputValue::Text(c.clone()))
                .ok_or_else(|| format!("`{trimmed}` is not one of: {}", self.choices.join(", "))),
            InputType::Int => {
                let n: i64 = trimmed
                    .parse()
                    .map_err(|_| format!("`{trimmed}` is not a whole number"))?;
                if let Some(min) = self.min.filter(|m| n < *m) {
                    return Err(format!("{n} is below the minimum ({min})"));
                }
                if let Some(max) = self.max.filter(|m| n > *m) {
                    return Err(format!("{n} is above the maximum ({max})"));
                }
                Ok(InputValue::Int(n))
            }
            InputType::Bool => match trimmed.to_ascii_lowercase().as_str() {
                "true" | "yes" | "y" | "1" | "on" => Ok(InputValue::Bool(true)),
                "false" | "no" | "n" | "0" | "off" => Ok(InputValue::Bool(false)),
                _ => Err(format!("`{trimmed}` is not yes or no")),
            },
        }
    }
}

/// A problem with one input, naming the flag that fixes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputProblem {
    pub input: String,
    pub message: String,
}

impl std::fmt::Display for InputProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "--{}: {}", flag_name(&self.input), self.message)
    }
}

/// Resolves raw answers against the guide: parses each, fills defaults, and reports missing
/// required inputs. Collects every problem rather than stopping at the first.
pub fn resolve(
    manifest: &GuideManifest,
    raw: &IndexMap<String, String>,
) -> Result<Inputs, Vec<InputProblem>> {
    let mut out = IndexMap::new();
    let mut problems = Vec::new();
    for name in raw.keys() {
        if !manifest.inputs.contains_key(name) {
            problems.push(InputProblem {
                input: name.clone(),
                message: format!("guide `{}` has no such input", manifest.id),
            });
        }
    }
    for (name, spec) in &manifest.inputs {
        let value = match raw.get(name) {
            Some(r) => match spec.parse(r) {
                Ok(v) => Some(v),
                Err(message) => {
                    problems.push(InputProblem {
                        input: name.clone(),
                        message,
                    });
                    None
                }
            },
            None => spec.default.clone(),
        };
        if value.is_none() && spec.required && !raw.contains_key(name) {
            problems.push(InputProblem {
                input: name.clone(),
                message: format!(
                    "is required ({})",
                    spec.help.as_deref().unwrap_or(&spec.prompt)
                ),
            });
        }
        out.insert(name.clone(), value);
    }
    if problems.is_empty() {
        Ok(Inputs(out))
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::bundled;

    fn story() -> GuideManifest {
        bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap()
            .manifest()
            .clone()
    }

    fn raw(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn fills_defaults_and_parses_types() {
        let inputs = resolve(
            &story(),
            &raw(&[("title", "Nosferatu"), ("kind", "Film"), ("year", "1922")]),
        )
        .unwrap();
        assert_eq!(inputs.get("kind"), Some(&InputValue::Text("film".into())));
        assert_eq!(inputs.get("year"), Some(&InputValue::Int(1922)));
        assert_eq!(
            inputs.get("scope"),
            Some(&InputValue::Text("Full story".into()))
        );
        assert_eq!(inputs.template_context()["year"], 1922);
    }

    #[test]
    fn reports_every_problem_with_its_flag() {
        let problems = resolve(
            &story(),
            &raw(&[("kind", "movie"), ("year", "99"), ("colour", "x")]),
        )
        .unwrap_err();
        let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(problems.len(), 4, "{text:?}");
        assert!(text.iter().any(|t| t.starts_with("--title: is required")));
        assert!(text.iter().any(|t| t.starts_with("--kind:")));
        assert!(text.iter().any(|t| t.contains("below the minimum")));
        assert!(text.iter().any(|t| t.starts_with("--colour:")));
    }

    #[test]
    fn optional_inputs_can_be_absent() {
        let inputs = resolve(&story(), &raw(&[("title", "You")])).unwrap();
        assert_eq!(inputs.get("year"), None);
        assert!(inputs.template_context()["year"].is_null());
    }
}
