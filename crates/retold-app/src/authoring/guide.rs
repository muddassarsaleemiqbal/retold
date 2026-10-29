//! Neutral guide scaffolds and validated imports; neither path installs production defaults.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use retold_engine::retold_guide::{
    Guide, Origin,
    manifest::{GuideManifest, LengthRule},
};

use super::{Publication, Target, failed, import_directory, invalid};
use crate::account::{self, commands};
use crate::protocol::{Diagnostic, Payload};

const TEMPLATE: &str = include_str!("../../../../docs/examples/guide/guide.toml");
const DEFAULT_SECONDS: u32 = 60;
const MIN_SECONDS: u32 = 10;
const MAX_SECONDS: u32 = 3600;
const DURATION_MARGIN: f64 = 5.0;

#[derive(Debug, Clone, Default)]
pub struct GuideOptions {
    pub id: String,
    pub name: Option<String>,
    pub from: Option<PathBuf>,
    pub example: Option<String>,
    pub language: Option<String>,
    pub purpose: Option<String>,
    pub style: Option<String>,
    pub delivery: Option<String>,
    pub writing: Option<String>,
    pub packaging: Option<String>,
    pub seconds: Option<u32>,
    pub dry_run: bool,
}

impl GuideOptions {
    fn prepare(&self, target: &Target<'_>) -> Result<Guide, Diagnostic> {
        let mut files = self.source_files()?;
        let source = files
            .get("guide.toml")
            .ok_or_else(|| invalid("Import has no guide.toml."))?;
        let mut manifest: GuideManifest =
            toml::from_str(source).map_err(|_| invalid("Guide manifest is invalid."))?;
        manifest.id = target.id.clone();
        if let Some(name) = &self.name {
            manifest.name.clone_from(name);
        }
        let scaffold = self.from.is_none() && self.example.is_none();
        if scaffold {
            manifest = self.configure(manifest)?;
        }
        let encoded = toml::to_string_pretty(&manifest)
            .map_err(|_| failed("Could not encode guide configuration."))?;
        files.insert("guide.toml".into(), encoded);
        Guide::from_files(Origin::Inline, files)
            .map_err(|_| invalid("The guide failed schema or template validation."))
    }

    fn source_files(&self) -> Result<BTreeMap<String, String>, Diagnostic> {
        self.check_sources()?;
        if let Some(example) = &self.example {
            self.check_import_options()?;
            return super::examples::guide(example);
        }
        if let Some(source) = &self.from {
            self.check_import_options()?;
            let directory = import_directory(source, "guide.toml")?;
            return Guide::from_dir(&directory)
                .map(|guide| guide.files().clone())
                .map_err(|_| invalid("Could not load a valid guide from the import directory."));
        }
        let voice = self.writing.clone().unwrap_or_else(|| {
            "CONFIGURE: describe audience, writing style and factual restrictions.".into()
        });
        Ok([
            ("guide.toml".into(), TEMPLATE.into()),
            ("voice.md".into(), voice),
        ]
        .into())
    }

    fn check_sources(&self) -> Result<(), Diagnostic> {
        let conflicting = self.from.is_some() && self.example.is_some();
        if conflicting {
            return Err(invalid("Choose either --from or --example, not both."));
        }
        Ok(())
    }

    fn check_import_options(&self) -> Result<(), Diagnostic> {
        let scaffold = [
            &self.language,
            &self.purpose,
            &self.style,
            &self.delivery,
            &self.writing,
            &self.packaging,
        ];
        let configured = scaffold.iter().any(|value| value.is_some()) || self.seconds.is_some();
        if configured {
            Err(invalid(
                "--from and --example supply complete guides; omit scaffold options.",
            ))
        } else {
            Ok(())
        }
    }

    fn configure(&self, mut manifest: GuideManifest) -> Result<GuideManifest, Diagnostic> {
        let language = self
            .language
            .as_deref()
            .ok_or_else(|| invalid("Choose --language when creating a guide scaffold."))?
            .parse()
            .map_err(|_| invalid("Use a valid source language tag such as en, hi or ur."))?;
        let duration = duration_range(self.seconds.unwrap_or(DEFAULT_SECONDS))?;
        manifest = configure_duration(manifest, language, duration);
        let delivery = self
            .delivery
            .clone()
            .unwrap_or_else(|| "CONFIGURE: define narration delivery.".into());
        manifest.narration.direction = [(manifest.format.source_language.clone(), delivery)].into();
        Ok(self.configure_style(manifest))
    }

    fn configure_style(&self, mut manifest: GuideManifest) -> GuideManifest {
        let unnamed = self.name.is_none();
        if unnamed {
            manifest.name.clone_from(&self.id);
        }
        if let Some(purpose) = &self.purpose {
            manifest.summary.clone_from(purpose);
        }
        if let Some(style) = &self.style {
            manifest.visuals.style.clone_from(style);
        }
        if let Some(packaging) = &self.packaging {
            manifest.packaging.rules.clone_from(packaging);
        }
        manifest
    }
}

fn duration_range(seconds: u32) -> Result<[f64; 2], Diagnostic> {
    let valid_seconds = (MIN_SECONDS..=MAX_SECONDS).contains(&seconds);
    if !valid_seconds {
        return Err(invalid("Duration must be between 10 and 3600 seconds."));
    }
    Ok([
        f64::from(seconds) - DURATION_MARGIN,
        f64::from(seconds) + DURATION_MARGIN,
    ])
}

pub fn create_guide(config: &Path, options: &GuideOptions) -> Result<Payload, Diagnostic> {
    let target = Target::new(config, "guides", &options.id)?;
    let guide = options.prepare(&target)?;
    if options.dry_run {
        return Ok(target.report(Publication::Preview));
    }
    let _owner = account::activity("create guide").map_err(|error| commands::diagnostic(&error))?;
    let staging = target.stage()?;
    guide
        .write_to(staging.path())
        .map_err(|_| failed("Could not save the guide files."))?;
    sync_guide(&guide, staging.path())?;
    target.publish(staging.path())
}

fn configure_duration(
    mut manifest: GuideManifest,
    language: retold_engine::LanguageTag,
    duration: [f64; 2],
) -> GuideManifest {
    manifest.format.source_language = language.clone();
    manifest.format.target_seconds = duration;
    manifest.length = [(
        language,
        LengthRule {
            words: None,
            seconds: Some(duration),
        },
    )]
    .into();
    manifest
}

fn sync_guide(guide: &Guide, directory: &Path) -> Result<(), Diagnostic> {
    for filename in guide.files().keys() {
        std::fs::File::open(directory.join(filename))
            .and_then(|file| file.sync_all())
            .map_err(|_| failed("Could not flush guide files before publication."))?;
    }
    Ok(())
}
