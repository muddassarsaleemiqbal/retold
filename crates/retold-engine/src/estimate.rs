//! Cost visibility: a preflight count of the image calls a storyboard will make, and the
//! recorded spend of a run grouped by stage.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use retold_core::Usd;
use serde::{Deserialize, Serialize};

use crate::assets::Storyboard;
use crate::ledger::Charge;
use crate::store::Receipt;
use retold_core::num;

/// Image calls a storyboard is expected to make, before any of them is paid for.
#[derive(Debug, Clone, Serialize)]
pub struct ImageEstimate {
    /// (what, how many), e.g. ("frames", 24).
    pub lines: Vec<(String, u64)>,
    pub images: u64,
    /// Expected regenerations at `redraw_rate`, rounded up.
    pub redraws: u64,
    pub price_per_image: Usd,
    pub total: Usd,
}

/// Counts sheets, location and prop references, frames and the cover. Shared references
/// already drawn for the series cost nothing again; pass them in `cached`.
#[must_use]
pub fn images(
    board: &Storyboard,
    cached: &BTreeSet<String>,
    price_per_image: Usd,
    redraw_rate: f64,
) -> ImageEstimate {
    let shots = &board.direction.shots;
    let fresh = |names: Vec<&String>| -> u64 {
        u64::try_from(
            names
                .into_iter()
                .filter(|name| !cached.contains(*name))
                .count(),
        )
        .unwrap_or(0)
    };
    let sheets = fresh(board.plan.characters.iter().collect());
    let places = fresh(
        board
            .plan
            .settings
            .iter()
            .map(|place| &place.name)
            .filter(|name| shots.iter().any(|shot| &shot.setting == *name))
            .collect(),
    );
    let props = fresh(
        board
            .plan
            .props
            .iter()
            .filter(|prop| shots.iter().any(|shot| shot.props.contains(prop)))
            .collect(),
    );
    let frames = u64::try_from(unique_frames(board)).unwrap_or(0);
    let lines = vec![
        ("character sheets".to_owned(), sheets),
        ("location references".to_owned(), places),
        ("prop references".to_owned(), props),
        ("frames".to_owned(), frames),
        ("cover".to_owned(), 1),
    ];
    let images: u64 = lines.iter().map(|(_, count)| count).sum();
    let redraws =
        num::trunc_u64((num::u64_f64(sheets + frames) * redraw_rate.clamp(0.0, 1.0)).ceil());
    let calls = i64::try_from(images + redraws).unwrap_or(i64::MAX);
    ImageEstimate {
        lines,
        images,
        redraws,
        price_per_image,
        total: Usd::from_micros(price_per_image.micros().saturating_mul(calls)),
    }
}

/// Names of the storyboard's characters, places and props already drawn for this series
/// (their shared reference is saved under `cast`), which cost nothing again.
#[must_use]
pub fn already_drawn(cast: &Path, board: &Storyboard) -> BTreeSet<String> {
    let mut cached = BTreeSet::new();
    for (folder, names) in [
        (cast.to_path_buf(), board.plan.characters.clone()),
        (
            cast.join("places"),
            board
                .plan
                .settings
                .iter()
                .map(|place| place.name.clone())
                .collect(),
        ),
        (cast.join("props"), board.plan.props.clone()),
    ] {
        for name in names {
            if let Some(found) = drawn_in(&folder, &name) {
                cached.insert(found);
            }
        }
    }
    cached
}

fn drawn_in(folder: &Path, name: &str) -> Option<String> {
    let slug = retold_core::Slug::from_title(name).ok()?;
    let prefix = format!("{slug}-");
    let exists = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            let path = entry.path();
            path.extension().is_some_and(|ext| ext == "json")
                && path
                    .file_name()
                    .is_some_and(|file| file.to_string_lossy().starts_with(&prefix))
        });
    exists.then(|| name.to_owned())
}

/// Frames that need their own generation; shots that reuse an earlier frame are free.
#[must_use]
pub fn unique_frames(board: &Storyboard) -> usize {
    board
        .direction
        .shots
        .iter()
        .filter(|shot| shot.reuse.is_none())
        .count()
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct StageCost {
    pub calls: u64,
    pub cost: Usd,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Recorded calls grouped by the stage their step belongs to.
#[must_use]
pub fn by_stage(receipts: &BTreeMap<String, Receipt>) -> BTreeMap<&'static str, StageCost> {
    let mut out: BTreeMap<&'static str, StageCost> = BTreeMap::new();
    for (step, receipt) in receipts {
        let stage = out.entry(stage_of(step)).or_default();
        stage.calls += 1;
        stage.cost += receipt.cost;
        stage.input_tokens += receipt.input_tokens;
        stage.output_tokens += receipt.output_tokens;
    }
    out
}

#[derive(Deserialize)]
struct CachedEnvelope {
    step: String,
    record: CachedKey,
    value: CachedMedia,
}

#[derive(Deserialize)]
struct CachedKey {
    key: String,
}

#[derive(Deserialize)]
struct CachedMedia {
    provider: Option<String>,
    model: Option<String>,
    request_id: Option<String>,
    quote: Option<CachedQuote>,
    usage: Option<CachedUsage>,
}

#[derive(Deserialize)]
struct CachedQuote {
    ceiling: Option<Usd>,
}

#[derive(Default, Deserialize)]
struct CachedUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    cost: Option<Usd>,
    generation: Option<String>,
}

/// Enriches `saved` receipts with media cache envelopes and reconciled ledger amounts.
#[must_use]
pub fn merged_receipts(
    meta_dir: &Path,
    saved: &BTreeMap<String, Receipt>,
    ledger: &BTreeMap<String, Charge>,
) -> BTreeMap<String, Receipt> {
    let mut out = saved.clone();
    for env in read_cached_media(meta_dir) {
        if let Some((receipt_key, receipt)) = media_receipt(env, ledger) {
            out.insert(receipt_key, receipt);
        }
    }
    out
}

fn media_receipt(
    env: CachedEnvelope,
    ledger: &BTreeMap<String, Charge>,
) -> Option<(String, Receipt)> {
    let provider = env.value.provider?;
    let model = env.value.model?;
    let ledger_cost = env
        .value
        .request_id
        .as_deref()
        .and_then(|id| ledger.get(id))
        .map(Charge::amount);
    let usage = env.value.usage.unwrap_or_default();
    let ceiling = env.value.quote.and_then(|quote| quote.ceiling);
    let cost = ledger_cost
        .or(usage.cost)
        .or(ceiling)
        .filter(|amount| *amount > Usd::ZERO)?;
    let receipt_key = format!("{}-{}", env.step, env.record.key);
    let receipt = Receipt {
        provider,
        model,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cost,
    };
    Some((receipt_key, receipt))
}

/// Human-readable step, model and provider generation ID for each ledger request ID.
#[must_use]
pub fn request_notes(meta_dir: &Path) -> BTreeMap<String, String> {
    let mut notes = BTreeMap::new();
    for env in read_cached_media(meta_dir) {
        if let Some((request_id, label)) = media_note(env) {
            notes.insert(request_id, label);
        }
    }
    for (request_id, label) in billing_error_notes(meta_dir) {
        notes.entry(request_id).or_insert(label);
    }
    notes
}

fn media_note(env: CachedEnvelope) -> Option<(String, String)> {
    let request_id = env.value.request_id?;
    let model = env.value.model.unwrap_or_default();
    let generation = env
        .value
        .usage
        .and_then(|usage| usage.generation)
        .unwrap_or_default();
    let label = match (model.is_empty(), generation.is_empty()) {
        (false, false) => format!("{} \u{b7} {model} \u{b7} {generation}", env.step),
        (false, true) => format!("{} \u{b7} {model}", env.step),
        (true, _) => env.step,
    };
    Some((request_id, label))
}

fn billing_error_notes(meta_dir: &Path) -> BTreeMap<String, String> {
    let path = meta_dir.join("billing-errors.json");
    let parsed: BTreeMap<String, serde_json::Value> = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    parsed
        .into_iter()
        .map(|(request_id, entry)| (request_id, describe_billing_error(&entry)))
        .collect()
}

fn describe_billing_error(entry: &serde_json::Value) -> String {
    let body = entry
        .get("body")
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    let model = body
        .as_ref()
        .and_then(|json| json.get("model"))
        .and_then(serde_json::Value::as_str);
    let generation = body
        .as_ref()
        .and_then(|json| json.get("id"))
        .and_then(serde_json::Value::as_str);
    match (model, generation) {
        (Some(model_name), Some(gen_id)) => format!("{model_name} \u{b7} {gen_id}"),
        (Some(model_name), None) => model_name.to_owned(),
        _ => "unsettled provider call".to_owned(),
    }
}

fn read_cached_media(meta_dir: &Path) -> Vec<CachedEnvelope> {
    std::fs::read_dir(meta_dir.join("cache"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let bytes = std::fs::read(&path).ok()?;
            serde_json::from_slice::<CachedEnvelope>(&bytes).ok()
        })
        .collect()
}

const STAGE_PATTERNS: &[(&[&str], &str)] = &[
    (
        &["research", "web", "extract", "dossier", "bible", "canon"],
        "research",
    ),
    (
        &[
            "plan", "draft", "write", "stitch", "judge", "fact", "fix", "hooks", "rank",
            "coverage", "outline", "section", "checks", "hook",
        ],
        "script",
    ),
    (&["segment", "direct"], "storyboard"),
    (&["design", "sheet", "style"], "characters"),
    (
        &["place", "prop", "image", "panels", "qa", "cover"],
        "images",
    ),
    (
        &["voice", "voices", "cast", "hindi", "speech", "repair"],
        "voice",
    ),
    (&["transcript"], "captions"),
    (&["mouth", "mouths"], "lipsync"),
    (
        &[
            "package",
            "metadata",
            "transcreate",
            "translate",
            "transliterate",
            "local",
            "deliver",
        ],
        "packaging",
    ),
];

/// The pipeline stage a cached step name belongs to.
#[must_use]
pub fn stage_of(step: &str) -> &'static str {
    let unscoped = crate::store::strip_part_scope(step);
    let head = unscoped.split('-').next().unwrap_or(unscoped);
    for (keywords, stage_name) in STAGE_PATTERNS {
        if keywords.contains(&head) {
            return stage_name;
        }
    }
    "other"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_group_by_stage() {
        let make_receipt = |cost| Receipt {
            provider: "p".into(),
            model: "m".into(),
            input_tokens: 10,
            output_tokens: 5,
            cost: Usd::from_micros(cost),
        };
        let receipts = BTreeMap::from([
            ("dossier".to_owned(), make_receipt(100)),
            ("draft-intro".to_owned(), make_receipt(50)),
            ("image-3-0".to_owned(), make_receipt(18_000)),
            ("qa-3-0".to_owned(), make_receipt(1_000)),
        ]);
        let stages = by_stage(&receipts);
        assert_eq!(stages["images"].calls, 2);
        assert_eq!(stages["images"].cost, Usd::from_micros(19_000));
        assert_eq!(stages["research"].input_tokens, 10);
        assert_eq!(stage_of("sheet-check-vera-0"), "characters");
        assert_eq!(stage_of("transliterate-ur-hi-abc"), "packaging");
        assert_eq!(stage_of("transcript-en-abc"), "captions");
        assert_eq!(stage_of("canon-parts-abc"), "research");
        assert_eq!(stage_of("p01-dossier-abc"), "research");
    }
}
