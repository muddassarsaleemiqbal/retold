//! Line-oriented rendering with no prompts, cursor control or provider text.

use retold_app::protocol::{Envelope, Payload};

#[derive(Debug, Clone, Copy)]
pub enum Format {
    Plain,
    Json,
}

pub fn render(envelope: &Envelope, format: Format) -> Result<String, serde_json::Error> {
    let text = match format {
        Format::Json => serde_json::to_string(envelope)?,
        Format::Plain => plain(&envelope.body)?,
    };
    Ok(format!("{text}\n"))
}

pub fn plain(body: &Payload) -> Result<String, serde_json::Error> {
    match body {
        Payload::Result { contents, .. } => {
            if let Some(text) = crate::human::render(contents) {
                return Ok(text);
            }
            let text = contents.get("text").or_else(|| contents.get("message"));
            let text = text.and_then(serde_json::Value::as_str);
            text.map_or_else(
                || serde_json::to_string_pretty(contents),
                |text| Ok(text.into()),
            )
        }
        Payload::Error { diagnostic, .. } => Ok(plain_error(diagnostic)),
        Payload::Event(detail) => serde_json::to_string(detail),
    }
}

fn plain_error(diagnostic: &retold_app::protocol::Diagnostic) -> String {
    let results = diagnostic
        .context
        .get("results")
        .map(|_| format!("{}\n", crate::human::test_results(&diagnostic.context)));
    let owners = diagnostic.context.get("owners").map(|owners| {
        let rows = crate::human::owner_rows(owners);
        format!("{}\n", rows.join("\n"))
    });
    format!(
        "{}{}{}: {}\nFix: {}",
        results.unwrap_or_default(),
        owners.unwrap_or_default(),
        diagnostic.code.as_str(),
        diagnostic.message,
        diagnostic.code.explanation()
    )
}
