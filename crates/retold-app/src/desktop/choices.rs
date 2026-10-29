//! Installed guide questions are exposed as typed offline creation choices.
use std::path::Path;

use retold_engine::retold_guide::{Catalog, Guide, channel};

use crate::protocol::{Diagnostic, ErrorCode};

#[derive(Debug, Clone)]
pub struct DraftChoices {
    pub guides: Vec<GuideChoice>,
    pub channels: Vec<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GuideChoice {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub languages: Vec<String>,
    pub fields: Vec<DraftField>,
}

#[derive(Debug, Clone)]
pub struct DraftField {
    pub name: String,
    pub prompt: String,
    pub help: Option<String>,
    pub required: bool,
    pub default: String,
    pub choices: Vec<String>,
}

pub fn draft_choices(config: &Path) -> Result<DraftChoices, Diagnostic> {
    let catalog = Catalog::load(Some(&config.join("guides")))
        .map_err(|error| Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()))?;
    let guides = catalog
        .entries()
        .iter()
        .map(|entry| guide_choice(&entry.guide))
        .collect();
    let (channels, broken) = channel::list(&config.join("channels"));
    let mut issues: Vec<_> = catalog
        .broken()
        .iter()
        .map(|broken| broken.error.to_string())
        .collect();
    issues.extend(broken.into_iter().map(|error| format!("{error:?}")));
    Ok(DraftChoices {
        guides,
        channels: channels.iter().map(|channel| channel.id().into()).collect(),
        issues,
    })
}

fn guide_choice(guide: &Guide) -> GuideChoice {
    let manifest = guide.manifest();
    let fields = manifest
        .inputs
        .iter()
        .map(|(name, field)| DraftField {
            name: name.clone(),
            prompt: field.prompt.clone(),
            help: field.help.clone(),
            required: field.required,
            default: field
                .default
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            choices: field.choices.clone(),
        })
        .collect();
    GuideChoice {
        id: manifest.id.to_string(),
        name: manifest.name.clone(),
        summary: manifest.summary.clone(),
        languages: manifest.length.keys().map(ToString::to_string).collect(),
        fields,
    }
}
