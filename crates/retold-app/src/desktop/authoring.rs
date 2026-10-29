//! Native guide and channel authoring: preview-then-create, and hash-bound single-field edits.
use std::path::Path;

use crate::authoring::{self, ChannelOptions, Edit, GuideOptions};
use crate::protocol::{Diagnostic, ErrorCode, Payload};

/// Which creative resource a form targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Guide,
    Channel,
}

impl Resource {
    const fn kind(self) -> &'static str {
        match self {
            Self::Guide => "guides",
            Self::Channel => "channels",
        }
    }
}

/// A loaded document: its current fields, missing required fields and content hash.
#[derive(Debug, Clone)]
pub struct Document {
    pub resource: Resource,
    pub id: String,
    pub value: serde_json::Value,
    pub missing: Vec<String>,
    pub hash: String,
}

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        Payload::Error { diagnostic, .. } => Err(diagnostic),
        Payload::Event(_) => Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Unexpected authoring response.",
        )),
    }
}

/// Create a guide or channel in one language; `dry_run` previews without writing.
pub fn create(
    config: &Path,
    (resource, id, language): (Resource, &str, &str),
    dry_run: Preview,
) -> Result<serde_json::Value, Diagnostic> {
    let dry_run = dry_run == Preview::Only;
    let payload = match resource {
        Resource::Guide => authoring::create_guide(
            config,
            &GuideOptions {
                id: id.into(),
                language: Some(language.into()),
                dry_run,
                ..GuideOptions::default()
            },
        )?,
        Resource::Channel => authoring::create_channel(
            config,
            &ChannelOptions {
                id: id.into(),
                language: Some(language.into()),
                dry_run,
                ..ChannelOptions::default()
            },
        )?,
    };
    contents(payload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preview {
    Only,
    Write,
}

fn member(document: &serde_json::Value, name: &str) -> serde_json::Value {
    document.get(name).cloned().unwrap_or_default()
}

pub fn open(config: &Path, resource: Resource, id: &str) -> Result<Document, Diagnostic> {
    let document = member(
        &contents(authoring::document(config, resource.kind(), id)?)?,
        "document",
    );
    Ok(Document {
        resource,
        id: id.into(),
        hash: member(&document, "hash")
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        missing: serde_json::from_value(member(&document, "missing")).unwrap_or_default(),
        value: member(&document, "value"),
    })
}

/// Set one text field (a JSON pointer such as `/name`) of the loaded document to plain text;
/// refused if the file changed since it was opened.
pub fn edit(
    config: &Path,
    document: &Document,
    (field, value, preview): (&str, &str, Preview),
) -> Result<serde_json::Value, Diagnostic> {
    let encoded = serde_json::Value::String(value.into()).to_string();
    let edit = Edit {
        field,
        value: &encoded,
        expected_hash: &document.hash,
        dry_run: preview == Preview::Only,
    };
    contents(authoring::edit_document(
        config,
        (document.resource.kind(), &document.id),
        &edit,
    )?)
}

/// The bundled guide example that can be copied into user-owned resources.
pub const EXAMPLE_GUIDE: &str = "story-recap";

/// Copy the bundled example guide; `Preview::Only` shows what would be written.
pub fn copy_example(config: &Path, preview: Preview) -> Result<serde_json::Value, Diagnostic> {
    contents(authoring::create_guide(
        config,
        &GuideOptions {
            id: EXAMPLE_GUIDE.into(),
            example: Some(EXAMPLE_GUIDE.into()),
            dry_run: preview == Preview::Only,
            ..GuideOptions::default()
        },
    )?)
}
