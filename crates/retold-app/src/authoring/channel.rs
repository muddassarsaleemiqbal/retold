//! User-owned channel scaffolds and bounded, symlink-free asset imports.
use std::path::{Path, PathBuf};

use retold_engine::LanguageTag;
use retold_engine::retold_guide::channel::{Channel, ChannelFile, FontSpec, VoiceSpec};

use super::{Publication, Target, failed, import_directory, invalid};
use crate::account::{self, commands};
use crate::protocol::{Diagnostic, Payload};

const TEMPLATE: &str = include_str!("../../../../docs/examples/channel/channel.toml");
const UNCONFIGURED: &str = "CONFIGURE";
const MAX_IMPORT_ENTRIES: usize = 4096;
const MAX_IMPORT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_IMPORT_DEPTH: usize = 12;

#[derive(Debug, Clone, Default)]
pub struct ChannelOptions {
    pub id: String,
    pub name: Option<String>,
    pub from: Option<PathBuf>,
    pub handle: Option<String>,
    pub accent: Option<String>,
    pub language: Option<String>,
    pub voice_provider: Option<String>,
    pub voice_model: Option<String>,
    pub voice: Option<String>,
    pub font_family: Option<String>,
    pub font_style: Option<String>,
    pub dry_run: bool,
}

impl ChannelOptions {
    fn prepare(&self, target: &Target<'_>) -> Result<(ChannelFile, Option<Import>), Diagnostic> {
        let (mut file, import) = if let Some(source) = &self.from {
            self.check_import_options()?;
            let directory = import_directory(source, "channel.toml")?;
            let channel = Channel::load(&directory)
                .map_err(|_| invalid("The imported channel or its asset paths are invalid."))?;
            (channel.file().clone(), Some(Import::inspect(&directory)?))
        } else {
            (self.scaffold()?, Option::default())
        };
        file.id = target.id.clone();
        if let Some(name) = &self.name {
            file.name.clone_from(name);
        }
        Channel::validate_manifest(&file).map_err(|_| {
            invalid(
                "The channel failed validation. Check its handle, fonts, colours and voice fields.",
            )
        })?;
        Ok((file, import))
    }

    fn scaffold(&self) -> Result<ChannelFile, Diagnostic> {
        let mut file: ChannelFile = toml::from_str(TEMPLATE)
            .map_err(|_| failed("The neutral channel scaffold is invalid."))?;
        let language = self.language()?;
        file.name = self.name.clone().unwrap_or_else(|| self.id.clone());
        file.handle = self
            .handle
            .clone()
            .unwrap_or_else(|| format!("@{}", self.id));
        if let Some(accent) = &self.accent {
            file.colors.insert(
                "accent".into(),
                accent
                    .parse()
                    .map_err(|_| invalid("Accent must be a six-digit colour such as #3B82F6."))?,
            );
        }
        file.voices = [(language.clone(), self.voice_spec())].into();
        file.fonts = [(language.script(), self.font_spec())].into();
        Ok(file)
    }

    fn language(&self) -> Result<LanguageTag, Diagnostic> {
        self.language
            .as_deref()
            .ok_or_else(|| invalid("Choose --language when creating a channel scaffold."))?
            .parse()
            .map_err(|_| invalid("Use a valid narration language tag such as en, hi or ur."))
    }

    fn font_spec(&self) -> FontSpec {
        FontSpec {
            family: self
                .font_family
                .clone()
                .unwrap_or_else(|| UNCONFIGURED.into()),
            style: self
                .font_style
                .clone()
                .unwrap_or_else(|| UNCONFIGURED.into()),
        }
    }

    fn voice_spec(&self) -> VoiceSpec {
        VoiceSpec {
            provider: self
                .voice_provider
                .clone()
                .unwrap_or_else(|| UNCONFIGURED.into()),
            model: self
                .voice_model
                .clone()
                .unwrap_or_else(|| UNCONFIGURED.into()),
            voice: self.voice.clone().unwrap_or_else(|| UNCONFIGURED.into()),
            instructions: None,
            sample: None,
            sample_language: None,
            fallbacks: Vec::new(),
        }
    }

    fn check_import_options(&self) -> Result<(), Diagnostic> {
        let scaffold = [
            &self.handle,
            &self.accent,
            &self.language,
            &self.voice_provider,
            &self.voice_model,
            &self.voice,
            &self.font_family,
            &self.font_style,
        ];
        let configured = scaffold.iter().any(|value| value.is_some());
        if configured {
            Err(invalid(
                "--from imports a complete channel; do not combine it with scaffold options.",
            ))
        } else {
            Ok(())
        }
    }
}

pub fn create_channel(config: &Path, options: &ChannelOptions) -> Result<Payload, Diagnostic> {
    let target = Target::new(config, "channels", &options.id)?;
    let (file, import) = options.prepare(&target)?;
    if options.dry_run {
        return Ok(target.report(Publication::Preview));
    }
    let _owner =
        account::activity("create channel").map_err(|error| commands::diagnostic(&error))?;
    let staging = target.stage()?;
    if let Some(import) = import {
        import.copy_into(staging.path())?;
    }
    write_channel(staging.path(), &file)?;
    Channel::load(staging.path())
        .map_err(|_| invalid("The staged channel or its asset paths failed validation."))?;
    target.publish(staging.path())
}

fn write_channel(directory: &Path, file: &ChannelFile) -> Result<(), Diagnostic> {
    let encoded =
        toml::to_string_pretty(file).map_err(|_| failed("Could not encode the channel."))?;
    retold_engine::fsx::write_atomic(&directory.join("channel.toml"), encoded.as_bytes())
        .map_err(|_| failed("Could not save channel.toml."))?;
    create_audio_directories(directory, file)
}

fn create_audio_directories(directory: &Path, file: &ChannelFile) -> Result<(), Diagnostic> {
    for folder in [&file.library.music, &file.library.sfx] {
        std::fs::create_dir_all(directory.join(folder))
            .map_err(|_| failed("Could not create channel audio folders."))?;
    }
    Ok(())
}

struct Import {
    root: PathBuf,
    files: Vec<(PathBuf, u64)>,
    entries_left: usize,
    bytes_left: u64,
}

impl Import {
    fn inspect(root: &Path) -> Result<Self, Diagnostic> {
        let mut inventory = Self {
            root: root.to_path_buf(),
            files: Vec::new(),
            entries_left: MAX_IMPORT_ENTRIES,
            bytes_left: MAX_IMPORT_BYTES,
        };
        inventory = inventory.walk(Path::new(""))?;
        Ok(inventory)
    }

    fn walk(mut self, relative: &Path) -> Result<Self, Diagnostic> {
        let depth = relative.components().count();
        if depth > MAX_IMPORT_DEPTH {
            return Err(invalid("Channel import nesting exceeds the limit."));
        }
        let entries = std::fs::read_dir(self.root.join(relative))
            .map_err(|_| invalid("Could not inspect imported assets."))?;
        for entry in entries {
            let entry = entry.map_err(|_| invalid("Could not inspect imported assets."))?;
            self.entries_left = self
                .entries_left
                .checked_sub(1)
                .ok_or_else(|| invalid("Channel import has too many entries."))?;
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if hidden {
                continue;
            }
            self = self.inspect_entry(&entry, relative)?;
        }
        Ok(self)
    }

    fn inspect_entry(
        mut self,
        entry: &std::fs::DirEntry,
        relative: &Path,
    ) -> Result<Self, Diagnostic> {
        let kind = entry
            .file_type()
            .map_err(|_| invalid("Could not inspect an imported file."))?;
        let path = relative.join(entry.file_name());
        if kind.is_dir() {
            return self.walk(&path);
        }
        if !kind.is_file() {
            return Err(invalid(
                "Channel imports must contain regular files and directories, without symlinks.",
            ));
        }
        let bytes = imported_size(entry)?;
        self.bytes_left = self
            .bytes_left
            .checked_sub(bytes)
            .ok_or_else(|| invalid("Channel import exceeds the 256 MB limit."))?;
        self.files.push((path, bytes));
        Ok(self)
    }

    fn copy_into(&self, directory: &Path) -> Result<(), Diagnostic> {
        for (relative, size) in &self.files {
            let bytes = retold_engine::fsx::read_bounded(&self.root.join(relative), *size)
                .map_err(|_| {
                    invalid("Imported files changed during creation; inspect the source and retry.")
                })?
                .ok_or_else(|| invalid("An imported file disappeared during creation."))?;
            retold_engine::fsx::write_atomic(&directory.join(relative), &bytes)
                .map_err(|_| failed("Could not save imported channel assets."))?;
        }
        Ok(())
    }
}

fn imported_size(entry: &std::fs::DirEntry) -> Result<u64, Diagnostic> {
    entry
        .metadata()
        .map(|metadata| metadata.len())
        .map_err(|_| invalid("Could not inspect an imported file."))
}

pub(super) fn validate_edit(directory: &Path, encoded: &str) -> Result<(), Diagnostic> {
    let file: ChannelFile =
        toml::from_str(encoded).map_err(|_| invalid("Edited channel schema is invalid."))?;
    Channel::validate_manifest(&file).map_err(|_| invalid("Edited channel failed validation."))?;
    // Reuse the bounded inventory to reject links anywhere in existing assets.
    Import::inspect(directory)?;
    Ok(())
}
