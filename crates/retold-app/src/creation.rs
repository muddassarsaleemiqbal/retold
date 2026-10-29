//! Offline creation of a genuine engine run. Saving a draft never admits generation or export.
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use retold_engine::retold_guide::{Catalog, Channel, Guide, Inputs, inputs};
use retold_engine::{NewRun, Paths, Quality, RunLayout, Slug, Variant, run};

use crate::account::{self, commands};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use crate::video::Usd;

const DEFAULT_BUDGET: &str = "1.00";
const MAX_SOURCE_BYTES: u64 = 2_000_000;

mod source;
pub use source::{SourcePreview, preview_source, save_draft_from_source};

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub guide: Option<String>,
    pub work: Option<String>,
    pub channel: Option<String>,
    pub inputs: BTreeMap<String, String>,
    pub languages: Vec<String>,
    pub quality: Option<String>,
    pub budget: Option<String>,
    pub source_file: Option<PathBuf>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CreatedDraft {
    pub video_id: String,
    pub reference: String,
    pub title: String,
    pub directory: PathBuf,
    pub guide: String,
    pub channel: String,
    pub editions: Vec<String>,
    pub budget_usd_micros: Usd,
    pub stage: &'static str,
}

struct Prepared {
    paths: Paths,
    guide: Guide,
    channel: Channel,
    inputs: Inputs,
    variants: Vec<Variant>,
    quality: Quality,
    budget: Usd,
}

impl Options {
    fn prepare(&self, root: &Path) -> Result<Prepared, Diagnostic> {
        let paths = retold_engine::paths::workspace::for_library(root).map_err(engine_error)?;
        let guide = load_guide(&paths, self.guide.as_deref())?;
        let channel = load_channel(&paths, self.channel.as_deref())?;
        let inputs = resolve_inputs(&guide, self)?;
        let variants = resolve_variants(&guide, &channel, &self.languages)?;
        let quality = self
            .quality
            .as_deref()
            .unwrap_or("standard")
            .parse()
            .map_err(input_error)?;
        let budget = self
            .budget
            .as_deref()
            .unwrap_or(DEFAULT_BUDGET)
            .parse()
            .map_err(|_| input_error("Budget must be a nonnegative USD amount."))?;
        validate_source(self.source_file.as_deref())?;
        Ok(Prepared {
            paths,
            guide,
            channel,
            inputs,
            variants,
            quality,
            budget,
        })
    }
}

impl Prepared {
    fn request<'a>(&'a self, source_file: Option<&'a Path>) -> NewRun<'a> {
        NewRun {
            guide: &self.guide,
            channel: &self.channel,
            inputs: self.inputs.clone(),
            variants: self.variants.clone(),
            quality: self.quality,
            budget: self.budget,
            skip_reviews: false,
            source_file,
        }
    }
}

pub fn create(root: &Path, options: &Options) -> Result<Payload, Diagnostic> {
    let prepared = options.prepare(root)?;
    let layout = available_layout(&prepared)?;
    if options.dry_run {
        return Ok(preview(&prepared, &layout));
    }
    Ok(created_payload(&save_prepared(&prepared, options)?))
}

fn available_layout(prepared: &Prepared) -> Result<RunLayout, Diagnostic> {
    let layout = RunLayout::resolve(
        &prepared.paths.library,
        &prepared.channel,
        &prepared.guide,
        &prepared.inputs,
    )
    .map_err(engine_error)?;
    let exists = std::fs::symlink_metadata(layout.video_dir()).is_ok();
    if exists {
        return Err(input_error(
            "This video already exists. Use video list and continue its saved run.",
        ));
    }
    Ok(layout)
}

/// Save a draft using the same validation and account barrier as the CLI, without dispatch.
pub fn save_draft(root: &Path, options: &Options) -> Result<CreatedDraft, Diagnostic> {
    if options.dry_run {
        return Err(input_error("Use draft preview for dry-run requests."));
    }
    save_prepared(&options.prepare(root)?, options)
}

fn save_prepared(prepared: &Prepared, options: &Options) -> Result<CreatedDraft, Diagnostic> {
    available_layout(prepared)?;
    let _owner = account::activity("create video").map_err(|error| commands::diagnostic(&error))?;
    let (layout, meta) = run::create(
        &prepared.paths.library,
        prepared.request(options.source_file.as_deref()),
    )
    .map_err(engine_error)?;
    Ok(CreatedDraft {
        video_id: meta.id,
        reference: format!("{}/{}", meta.channel, layout.display_path()),
        title: meta.title,
        directory: layout.video_dir().to_path_buf(),
        guide: meta.guide.id,
        channel: meta.channel,
        editions: meta.variants.iter().map(ToString::to_string).collect(),
        budget_usd_micros: meta.budget,
        stage: "created",
    })
}

fn created_payload(draft: &CreatedDraft) -> Payload {
    let reference = &draft.reference;
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": format!("Created '{}'. Next: retold video plan {reference}", draft.title),
            "created": draft,
            "next_command": ["retold", "video", "plan", reference],
        }),
    }
}

fn preview(prepared: &Prepared, layout: &RunLayout) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": format!("Would create a video in {}. No files changed or requests sent.", layout.video_dir().display()),
            "dry_run": true, "directory": layout.video_dir(), "guide": prepared.guide.id(),
        "channel": prepared.channel.id(), "editions": prepared.variants, "budget_usd_micros": prepared.budget,
        "routes": role_routes(prepared),
        }),
    }
}

/// Each guide role with its tier and the provider chain that would serve it; unrouted roles
/// show an empty chain so the draft can be fixed before planning.
fn role_routes(prepared: &Prepared) -> serde_json::Value {
    let routing = retold_engine::provider_routes::show(&prepared.paths.config).unwrap_or_default();
    let manifest = prepared.guide.manifest();
    let roles = manifest.roles.all();
    roles
        .into_iter()
        .map(|(role, tier)| {
            let chain = routing.get(tier.as_str()).cloned().unwrap_or_default();
            serde_json::json!({"role": role, "tier": tier.as_str(), "chain": chain})
        })
        .collect()
}

fn load_guide(paths: &Paths, requested: Option<&str>) -> Result<Guide, Diagnostic> {
    let catalog = Catalog::load(Some(&paths.guides()))
        .map_err(|_| input_error("The guide library could not be read. Inspect guides list."))?;
    let selected = requested
        .or_else(|| match catalog.entries() {
            [only] => Some(only.guide.id()),
            _ => None,
        })
        .ok_or_else(|| {
            input_error("Choose a guide: retold guides list, then retold new <guide>.")
        })?;
    catalog.get(selected).cloned().ok_or_else(|| {
        input_error(format!(
            "Guide '{selected}' is not installed. Run guides list or guides create."
        ))
    })
}

fn load_channel(paths: &Paths, requested: Option<&str>) -> Result<Channel, Diagnostic> {
    let preferred = paths.settings().map_err(engine_error)?.default_channel;
    if let Some(id) = requested.or(preferred.as_deref()) {
        let slug: Slug = id
            .parse()
            .map_err(|_| input_error("Channel names must be lowercase slugs."))?;
        return Channel::load(&paths.channels().join(slug.as_str())).map_err(|_| {
            input_error(format!(
                "Channel '{id}' is missing or invalid. Run channels list or channels create."
            ))
        });
    }
    let (channels, _) = retold_engine::retold_guide::channel::list(&paths.channels());
    let mut available = channels.into_iter();
    match (available.next(), available.next()) {
        (Some(only), None) => Ok(only),
        _ => Err(input_error(
            "Choose --channel explicitly. Run channels list or channels create.",
        )),
    }
}

fn resolve_inputs(guide: &Guide, options: &Options) -> Result<Inputs, Diagnostic> {
    let mut raw = options.inputs.clone();
    if let Some(work) = &options.work {
        let name = ["title", "topic"]
            .into_iter()
            .find(|name| guide.manifest().inputs.contains_key(*name))
            .ok_or_else(|| {
                input_error(
                    "This guide needs named inputs. Use --input NAME=VALUE; see guides show.",
                )
            })?;
        if let Some(previous) = raw.insert(name.into(), work.clone())
            && previous != *work
        {
            return Err(input_error("The work title conflicts with a named input."));
        }
    }
    let raw = raw.into_iter().collect();
    inputs::resolve(guide.manifest(), &raw).map_err(|problems| {
        input_error(
            problems
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })
}

fn resolve_variants(
    guide: &Guide,
    channel: &Channel,
    requested: &[String],
) -> Result<Vec<Variant>, Diagnostic> {
    let default = [guide.manifest().format.source_language.to_string()];
    let languages = if requested.is_empty() {
        default.as_slice()
    } else {
        requested
    };
    let variants = languages
        .iter()
        .map(|language| resolve_variant(guide, channel, language))
        .collect::<Result<Vec<_>, _>>()?;
    ensure_unique_variants(&variants)?;
    Ok(variants)
}

fn resolve_variant(
    guide: &Guide,
    channel: &Channel,
    language: &str,
) -> Result<Variant, Diagnostic> {
    let variant: Variant = language
        .parse()
        .map_err(|_| input_error("Invalid --lang edition. Use en or hi:hi,en.srt."))?;
    channel.voice(&variant.narration).map_err(input_error)?;
    let supported = guide.manifest().length.contains_key(&variant.narration);
    if !supported {
        return Err(input_error(format!(
            "The guide has no length rule for {}.",
            variant.narration
        )));
    }
    Ok(variant)
}

fn ensure_unique_variants(variants: &[Variant]) -> Result<(), Diagnostic> {
    let mut seen = HashSet::new();
    let unique = variants
        .iter()
        .all(|variant| seen.insert(variant.to_string()));
    if unique {
        Ok(())
    } else {
        Err(input_error(
            "Each language edition may be listed only once.",
        ))
    }
}

fn validate_source(source: Option<&Path>) -> Result<(), Diagnostic> {
    if let Some(path) = source {
        self::source::read_source(path)?;
    }
    Ok(())
}

fn input_error(message: impl AsRef<str>) -> Diagnostic {
    Diagnostic::new(ErrorCode::InputRequired, message.as_ref())
}

fn engine_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string())
}
