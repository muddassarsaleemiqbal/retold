//! MiniJinja rendering with strict undefined handling: a misspelled variable is an error,
//! never an empty string in a prompt.

use minijinja::{Environment, UndefinedBehavior};
use serde::Serialize;

/// Variables every guide template may use.
pub const ROOT_VARIABLES: &[&str] = &["inputs", "channel", "guide", "lang", "instruction"];

#[derive(Debug, thiserror::Error)]
#[error("template error in {name}: {source}")]
pub struct TemplateError {
    pub name: String,
    #[source]
    pub source: minijinja::Error,
}

fn env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_trim_blocks(true);
    env.set_keep_trailing_newline(true);
    env
}

/// Renders a one-off template string.
pub fn render(name: &str, source: &str, ctx: impl Serialize) -> Result<String, TemplateError> {
    let wrap = |source| TemplateError {
        name: name.to_owned(),
        source,
    };
    let env = env();
    let tmpl = env.template_from_str(source).map_err(wrap)?;
    tmpl.render(ctx).map_err(wrap)
}

/// Checks that a template compiles and only uses known variables. `inputs` lists the input
/// names the guide declares. Returns human-readable problems.
#[must_use]
pub fn check(name: &str, source: &str, inputs: &[&str]) -> Vec<String> {
    let env = env();
    let tmpl = match env.template_from_str(source) {
        Ok(t) => t,
        Err(e) => return vec![format!("{name}: {e}")],
    };
    let mut problems = Vec::new();
    let mut vars: Vec<String> = tmpl.undeclared_variables(true).into_iter().collect();
    vars.sort();
    for var in vars {
        let mut parts = var.split('.');
        let root = parts.next().unwrap_or_default();
        if !ROOT_VARIABLES.contains(&root) {
            problems.push(format!(
                "{name}: unknown variable `{root}` (available: {})",
                ROOT_VARIABLES.join(", ")
            ));
        } else if root == "inputs"
            && let Some(field) = parts.next()
            && !inputs.contains(&field)
        {
            problems.push(format!(
                "{name}: `inputs.{field}` is not a declared input (declared: {})",
                inputs.join(", ")
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_and_rejects_undefined() {
        let ctx = json!({"inputs": {"title": "Nosferatu", "year": null}});
        let out = render(
            "t",
            "{{ inputs.title }}{% if inputs.year %} {{ inputs.year }}{% endif %}",
            &ctx,
        )
        .unwrap();
        assert_eq!(out, "Nosferatu");
        assert!(render("t", "{{ inputs.titel }}", &ctx).is_err());
    }

    #[test]
    fn check_finds_unknown_variables() {
        let problems = check("q", "{{ inputs.titel }} {{ chanel.name }}", &["title"]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert_eq!(
            check(
                "q",
                "{{ inputs.title }} {{ channel.colors.accent }}",
                &["title"]
            ),
            [] as [std::string::String; 0]
        );
        assert_eq!(check("q", "{% if %}", &[]).len(), 1);
    }
}
