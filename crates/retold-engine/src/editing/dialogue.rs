//! Manual edition lines: reviewed per-language overrides of transcreated narration or dialogue.
//! Generated `units.json` stays original; overrides apply only after approval of their hash.
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{EDITING_FORMAT_VERSION, EditError, MAX_DOCUMENT_BYTES, MAX_POINTER_BYTES, files};
use crate::{RunLayout, Stage, fsx, languages::LocalUnit, store};

/// One spoken line stays a short unit of narration or dialogue.
pub const MAX_LINE_BYTES: usize = 4096;
const MAX_LANGUAGE_BYTES: usize = 35;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Overrides {
    schema: u32,
    lines: BTreeMap<usize, String>,
    approved_hash: Option<String>,
    approved_at: Option<String>,
}

/// One edition line with its generated original and the text production will speak.
#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub unit: usize,
    pub section: String,
    pub generated: String,
    pub text: String,
    pub edited: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DialogueSnapshot {
    pub language: String,
    pub lines: Vec<Line>,
    pub version: String,
    pub content_hash: String,
    pub approved: bool,
    pub approved_at: Option<String>,
    pub edited: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LineEdit<'a> {
    pub expected_version: &'a str,
    pub language: &'a str,
    pub unit: usize,
    pub text: &'a str,
}

/// Exact stages a line change rebuilds; narration in other languages is untouched.
#[derive(Debug, Clone, Serialize)]
pub struct LineImpact {
    pub changed_units: Vec<usize>,
    pub affected_stages: Vec<String>,
    pub restores_generated: bool,
}

fn valid_language(language: &str) -> bool {
    let shaped = !language.is_empty() && language.len() <= MAX_LANGUAGE_BYTES;
    shaped
        && language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn require_language(language: &str) -> Result<(), EditError> {
    if !valid_language(language) {
        return Err(EditError::Invalid(
            "Choose an existing edition language.".into(),
        ));
    }
    Ok(())
}

fn overrides_path(layout: &RunLayout, language: &str) -> PathBuf {
    files::directory(layout)
        .join("dialogue")
        .join(format!("{language}.json"))
}

fn read_overrides(layout: &RunLayout, language: &str) -> Result<Option<Vec<u8>>, EditError> {
    files::check_existing_directory(&layout.meta())?;
    files::check_existing_directory(&files::directory(layout))?;
    files::check_existing_directory(&files::directory(layout).join("dialogue"))?;
    Ok(fsx::read_bounded(
        &overrides_path(layout, language),
        MAX_POINTER_BYTES,
    )?)
}

/// The raw override bytes (for versioning) and their decoded content, empty when never edited.
fn load(layout: &RunLayout, language: &str) -> Result<(Option<Vec<u8>>, Overrides), EditError> {
    let bytes = read_overrides(layout, language)?;
    let overrides = match &bytes {
        Some(bytes) => decode_overrides(bytes)?,
        None => Overrides {
            schema: EDITING_FORMAT_VERSION,
            ..Overrides::default()
        },
    };
    Ok((bytes, overrides))
}

fn decode_overrides(bytes: &[u8]) -> Result<Overrides, EditError> {
    let overrides: Overrides = files::decode(bytes)?;
    if overrides.schema != EDITING_FORMAT_VERSION {
        return Err(EditError::Integrity(
            "Unsupported edition line schema.".into(),
        ));
    }
    Ok(overrides)
}

fn generated(layout: &RunLayout, language: &str) -> Result<Option<Vec<LocalUnit>>, EditError> {
    let path = layout
        .stage(Stage::Script)
        .join(language)
        .join("units.json");
    fsx::read_bounded(&path, MAX_DOCUMENT_BYTES)?
        .map(|bytes| {
            files::decode::<crate::languages::LocalScript>(&bytes).map(|script| script.units)
        })
        .transpose()
}

/// Languages with transcreated lines, in stable order.
pub fn languages(layout: &RunLayout) -> Result<Vec<String>, EditError> {
    let root = layout.stage(Stage::Script);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(Vec::new());
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_language(name))
        .filter(|name| root.join(name).join("units.json").is_file())
        .collect();
    found.sort();
    Ok(found)
}

fn apply(units: &[LocalUnit], overrides: &Overrides) -> Vec<LocalUnit> {
    units
        .iter()
        .map(|unit| LocalUnit {
            id: unit.id,
            section: unit.section.clone(),
            text: overrides
                .lines
                .get(&unit.id)
                .cloned()
                .unwrap_or_else(|| unit.text.clone()),
        })
        .collect()
}

fn lines(units: &[LocalUnit], overrides: &Overrides) -> Vec<Line> {
    units
        .iter()
        .zip(apply(units, overrides))
        .map(|(unit, line)| Line {
            unit: unit.id,
            section: unit.section.clone(),
            generated: unit.text.clone(),
            text: line.text,
            edited: overrides.lines.contains_key(&unit.id),
        })
        .collect()
}

pub fn snapshot(layout: &RunLayout, language: &str) -> Result<Option<DialogueSnapshot>, EditError> {
    require_language(language)?;
    let (bytes, overrides) = load(layout, language)?;
    let Some(units) = generated(layout, language)? else {
        return Ok(Option::default());
    };
    let content_hash = store::hash(&apply(&units, &overrides))?;
    let edited = !overrides.lines.is_empty();
    let approved = !edited || overrides.approved_hash.as_deref() == Some(content_hash.as_str());
    Ok(Some(DialogueSnapshot {
        language: language.into(),
        lines: lines(&units, &overrides),
        version: store::hash(&(&bytes, &units))?,
        approved,
        approved_at: overrides.approved_at,
        content_hash,
        edited,
    }))
}

fn current(
    layout: &RunLayout,
    language: &str,
    expected: &str,
) -> Result<DialogueSnapshot, EditError> {
    let current = snapshot(layout, language)?
        .ok_or_else(|| EditError::Invalid("This edition has no saved lines yet.".into()))?;
    if current.version != expected {
        return Err(EditError::SceneChanged);
    }
    Ok(current)
}

pub fn validate_line(edit: &LineEdit<'_>) -> Result<(), EditError> {
    require_language(edit.language)?;
    let unusable = edit.text.trim().is_empty() || edit.text.len() > MAX_LINE_BYTES;
    if unusable {
        return Err(EditError::Invalid(
            "Use a nonempty line within the 4 KiB limit.".into(),
        ));
    }
    Ok(())
}

fn saved_overrides(current: &DialogueSnapshot) -> BTreeMap<usize, String> {
    current
        .lines
        .iter()
        .filter(|line| line.edited)
        .map(|line| (line.unit, line.text.clone()))
        .collect()
}

fn proposed(
    current: &DialogueSnapshot,
    edit: LineEdit<'_>,
    generated: &str,
) -> BTreeMap<usize, String> {
    let mut lines = saved_overrides(current);
    let restores = generated == edit.text;
    if restores {
        lines.remove(&edit.unit);
    } else {
        lines.insert(edit.unit, edit.text.into());
    }
    lines
}

/// The proposed override map, and whether the edit restores the generated original.
fn edited(
    current: &DialogueSnapshot,
    edit: LineEdit<'_>,
) -> Result<(BTreeMap<usize, String>, bool), EditError> {
    validate_line(&edit)?;
    let line = current
        .lines
        .iter()
        .find(|line| line.unit == edit.unit)
        .ok_or_else(|| EditError::Invalid("Choose an existing line.".into()))?;
    let restores = line.generated == edit.text;
    Ok((proposed(current, edit, &line.generated), restores))
}

pub fn preview(layout: &RunLayout, edit: LineEdit<'_>) -> Result<LineImpact, EditError> {
    let current = current(layout, edit.language, edit.expected_version)?;
    let (_, restores) = edited(&current, edit)?;
    let changed = current
        .lines
        .iter()
        .any(|line| line.unit == edit.unit && line.text != edit.text);
    let affected_stages = if changed {
        crate::graph::Graph::stages()
            .affected("voice")
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    Ok(LineImpact {
        changed_units: if changed { vec![edit.unit] } else { Vec::new() },
        affected_stages,
        restores_generated: restores,
    })
}

fn write(layout: &RunLayout, language: &str, overrides: &Overrides) -> Result<(), EditError> {
    let bytes = files::encode(overrides, MAX_POINTER_BYTES)?;
    files::prepare_directory(layout, "dialogue")?;
    fsx::write_atomic(&overrides_path(layout, language), &bytes)?;
    Ok(())
}

pub fn save_line(layout: &RunLayout, edit: LineEdit<'_>) -> Result<DialogueSnapshot, EditError> {
    validate_line(&edit)?;
    let mut store = store::Store::open(layout)?;
    let current = current(layout, edit.language, edit.expected_version)?;
    let (lines, _) = edited(&current, edit)?;
    let unchanged = current
        .lines
        .iter()
        .any(|line| line.unit == edit.unit && line.text == edit.text);
    if unchanged {
        return Ok(current);
    }
    let overrides = Overrides {
        schema: EDITING_FORMAT_VERSION,
        lines,
        approved_hash: None,
        approved_at: None,
    };
    store.bump_revision("voice")?;
    write(layout, edit.language, &overrides)?;
    current_after(layout, edit.language)
}

pub fn approve(
    layout: &RunLayout,
    language: &str,
    expected: &str,
) -> Result<DialogueSnapshot, EditError> {
    let _store = store::Store::open(layout)?;
    let current = current(layout, language, expected)?;
    if current.approved {
        return Ok(current);
    }
    let (_, mut overrides) = load(layout, language)?;
    overrides.approved_hash = Some(current.content_hash);
    overrides.approved_at = Some(jiff::Timestamp::now().to_string());
    write(layout, language, &overrides)?;
    current_after(layout, language)
}

fn current_after(layout: &RunLayout, language: &str) -> Result<DialogueSnapshot, EditError> {
    snapshot(layout, language)?
        .ok_or_else(|| EditError::Integrity("Edition lines disappeared while saving.".into()))
}

/// Edited editions must be approved at their current hash before any provider call.
pub fn ready(layout: &RunLayout) -> Result<(), EditError> {
    for language in languages(layout)? {
        let unapproved = snapshot(layout, &language)?.is_some_and(|lines| !lines.approved);
        if unapproved {
            return Err(EditError::Unapproved);
        }
    }
    Ok(())
}

/// Hash of every edition override file, or none when no line was ever edited.
pub(super) fn digest(layout: &RunLayout) -> Result<Option<String>, EditError> {
    let mut found = BTreeMap::new();
    for language in languages(layout)? {
        if let Some(bytes) = read_overrides(layout, &language)? {
            found.insert(language, bytes);
        }
    }
    (!found.is_empty())
        .then(|| store::hash(&found).map_err(EditError::from))
        .transpose()
}

/// Applied after transcreation; an unapproved override fails before any speech request.
pub(crate) fn effective(
    layout: &RunLayout,
    language: &str,
    units: Vec<LocalUnit>,
) -> Result<Vec<LocalUnit>, EditError> {
    let (_, overrides) = load(layout, language)?;
    let untouched = overrides.lines.is_empty();
    if untouched {
        return Ok(units);
    }
    let applied = apply(&units, &overrides);
    let applied_hash = store::hash(&applied)?;
    let approved = overrides.approved_hash.as_deref() == Some(applied_hash.as_str());
    if !approved {
        return Err(EditError::Unapproved);
    }
    Ok(applied)
}
