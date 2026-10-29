//! Channels: brand settings (colours, voices, fonts, caption look, music library).
//!
//! A channel is a folder holding `channel.toml` plus its assets (style reference, music,
//! sound effects). Users create and own their channel configurations.

#[cfg(any(test, feature = "test-fixtures"))]
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[cfg(any(test, feature = "test-fixtures"))]
use include_dir::{Dir, include_dir};
use indexmap::IndexMap;
use retold_core::{LanguageTag, Slug, WritingScript};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::LoadError;
use crate::manifest::SCHEMA_VERSION;

pub const CHANNEL_FILE: &str = "channel.toml";

#[cfg(any(test, feature = "test-fixtures"))]
static TEMPLATES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/channels");

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelFile {
    pub schema: u32,
    pub id: Slug,
    pub name: String,
    pub handle: String,
    /// Named brand colours; other settings refer to them by name.
    pub colors: IndexMap<String, HexColor>,
    /// Narration voice per language.
    #[serde(default)]
    pub voices: IndexMap<LanguageTag, VoiceSpec>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub character_voices: IndexMap<String, VoiceSpec>,
    /// Optional channel-relative room-tone WAV for dialogue pauses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_tone: Option<String>,
    /// Spelling-to-speech fixes per language (`Daenerys = "Dah-NAIR-iss"`).
    #[serde(default)]
    pub pronunciations: IndexMap<LanguageTag, IndexMap<String, String>>,
    /// Preferred native spellings in transcreation and captions.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub name_spellings: IndexMap<LanguageTag, IndexMap<String, String>>,
    /// Font per writing system.
    pub fonts: IndexMap<WritingScript, FontSpec>,
    pub captions: Captions,
    pub title: TitleStyle,
    pub look: Look,
    pub library: Library,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VoiceSpec {
    /// Provider id from `providers.toml` (`openrouter`).
    pub provider: String,
    pub model: String,
    pub voice: String,
    /// Delivery instructions, for models that accept them (`OpenAI` `gpt-4o-mini-tts`).
    #[serde(default)]
    pub instructions: Option<String>,
    /// 10–30 second narrator/character clone sample, relative to this channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
    /// Language actually spoken in the clone reference (defaults to the guide source).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_language: Option<LanguageTag>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<VoiceFallback>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VoiceFallback {
    pub provider: String,
    pub model: String,
    pub voice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSpec {
    pub family: String,
    pub style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Captions {
    pub size: u32,
    pub fill: ColorRef,
    pub stroke: Stroke,
    pub highlight: ColorRef,
    pub max_words: u32,
    pub max_chars: u32,
    /// Vertical position, as a fraction of frame height.
    pub y: f64,
    /// Space kept clear at each side, as a fraction of frame width.
    pub side_margin: f64,
    /// Only applied to scripts that have case.
    pub uppercase: bool,
    /// Colour for character, place and prop names in captions; defaults to the `accent`
    /// brand colour when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emphasis: Option<ColorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub color: ColorRef,
    pub width: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TitleStyle {
    /// Opt into traditional top-to-bottom CJK title composition.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub vertical: bool,
    pub size: u32,
    pub y: f64,
    pub side_margin: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Look {
    pub vignette: Vignette,
    pub grain: Grain,
    pub grade: Grade,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vignette {
    pub amount: f64,
    pub midpoint: f64,
    pub feather: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Grain {
    pub amount: f64,
    pub size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Grade {
    pub contrast: f64,
    pub saturation: f64,
    pub highlights: f64,
}

/// Paths relative to the channel folder.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub style_reference: String,
    pub music: String,
    pub sfx: String,
}

/// `#RRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HexColor(pub [u8; 3]);

impl FromStr for HexColor {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || format!("`{s}` is not a colour like \"#FF5A1F\"");
        let hex = s.strip_prefix('#').ok_or_else(bad)?;
        if hex.len() != 6 || !hex.is_ascii() {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        Ok(Self([byte(0)?, byte(2)?, byte(4)?]))
    }
}

impl fmt::Display for HexColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02X}{g:02X}{b:02X}")
    }
}

impl Serialize for HexColor {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for HexColor {
    fn schema_name() -> String {
        "HexColor".to_owned()
    }
    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut s = schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            ..Default::default()
        };
        s.string().pattern = Some("^#[0-9A-Fa-f]{6}$".to_owned());
        s.into()
    }
}

/// A colour by name (from `[colors]`) or as `#RRGGBB`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ColorRef(pub String);

/// A loaded, validated channel.
#[derive(Debug, Clone)]
pub struct Channel {
    file: ChannelFile,
    dir: PathBuf,
}

impl Channel {
    /// Validates configuration in memory without creating a folder or reading assets.
    pub fn validate_manifest(file: &ChannelFile) -> Result<(), LoadError> {
        let found = problems(file);
        if found.is_empty() {
            Ok(())
        } else {
            Err(LoadError::invalid(CHANNEL_FILE, found))
        }
    }

    /// Loads `dir/channel.toml` and checks it.
    pub fn load(dir: &Path) -> Result<Self, LoadError> {
        load_channel_from(dir)
    }

    #[must_use]
    pub const fn file(&self) -> &ChannelFile {
        &self.file
    }

    #[must_use]
    pub fn id(&self) -> &str {
        self.file.id.as_str()
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Resolves a colour reference (already checked at load time).
    #[must_use]
    pub fn color(&self, r: &ColorRef) -> HexColor {
        resolve(&self.file, r).unwrap_or(HexColor([0, 0, 0]))
    }

    /// Keep a saved channel's rules while resolving shared assets in its original folder.
    #[must_use]
    pub fn with_asset_dir(mut self, directory: PathBuf) -> Self {
        self.dir = directory;
        self
    }

    #[must_use]
    pub fn library_path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    /// The voice for `lang`, with a hint when the channel has none.
    pub fn voice(&self, lang: &LanguageTag) -> Result<&VoiceSpec, String> {
        self.file.voices.get(lang).ok_or_else(|| {
            format!(
                "channel `{}` has no narration voice for {}; add one under [voices] in {}",
                self.id(),
                lang.display_name(),
                self.dir.join(CHANNEL_FILE).display()
            )
        })
    }

    /// Template context: `channel.name`, `channel.colors.accent`…
    #[must_use]
    pub fn template_context(&self) -> serde_json::Value {
        let colors: serde_json::Map<String, serde_json::Value> = self
            .file
            .colors
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string().into()))
            .collect();
        serde_json::json!({
            "id": self.file.id,
            "name": self.file.name,
            "handle": self.file.handle,
            "colors": colors,
        })
    }
}

fn resolve(file: &ChannelFile, r: &ColorRef) -> Option<HexColor> {
    if r.0.starts_with('#') {
        r.0.parse().ok()
    } else {
        file.colors.get(&r.0).copied()
    }
}

fn parse(shown: &str, src: &str) -> Result<ChannelFile, LoadError> {
    reject_newer_schema(shown, src)?;
    let file: ChannelFile = toml::from_str(src).map_err(|e| LoadError::syntax(shown, src, &e))?;
    let problems = problems(&file);
    if problems.is_empty() {
        Ok(file)
    } else {
        Err(LoadError::invalid(shown, problems))
    }
}

fn reject_newer_schema(shown: &str, src: &str) -> Result<(), LoadError> {
    let found = src
        .parse::<toml::Table>()
        .ok()
        .and_then(|raw| raw.get("schema").and_then(toml::Value::as_integer));
    let Some(found) = found else { return Ok(()) };
    let found = u32::try_from(found).unwrap_or(u32::MAX);
    if found > SCHEMA_VERSION {
        return Err(LoadError::NewerSchema {
            file: shown.to_owned(),
            found,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(())
}

/// Every problem with a parsed channel, so one check shows the full list.
fn problems(file: &ChannelFile) -> Vec<String> {
    let mut found = Vec::new();
    if file.schema != SCHEMA_VERSION {
        found.push(format!("schema must be {SCHEMA_VERSION}"));
    }
    if file.name.trim().is_empty() {
        found.push("name must not be empty".into());
    }
    if !file.handle.starts_with('@') {
        found.push(format!("handle: `{}` should start with @", file.handle));
    }
    found.extend(library_problems(file));
    found.extend(voice_problems(file));
    found.extend(look_problems(file));
    found.extend(layout_problems(file));
    found.extend(colour_problems(file));
    found
}

/// Whether `path` names a file inside the channel folder: nonempty, relative, no `..`.
fn inside_channel(path: &str) -> bool {
    !path.trim().is_empty()
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn library_problems(file: &ChannelFile) -> Vec<String> {
    let library = [
        ("library.style_reference", &file.library.style_reference),
        ("library.music", &file.library.music),
        ("library.sfx", &file.library.sfx),
    ];
    let mut found: Vec<String> = library
        .into_iter()
        .filter(|(_, path)| !crate::guide::relative_path(path))
        .map(|(at, _)| format!("{at}: use a nonempty relative path without parent traversal"))
        .collect();
    if file
        .room_tone
        .as_deref()
        .is_some_and(|path| !inside_channel(path))
    {
        found.push("room_tone must be a channel-relative path without traversal".into());
    }
    found
}

fn voice_problems(file: &ChannelFile) -> Vec<String> {
    let blank = |fields: [&String; 3]| fields.iter().any(|field| field.trim().is_empty());
    let language_voices = file
        .voices
        .iter()
        .map(|(language, voice)| (language.to_string(), voice));
    let character_voices = file
        .character_voices
        .iter()
        .map(|(name, voice)| (name.clone(), voice));
    let mut found = Vec::new();
    for (owner, voice) in language_voices.chain(character_voices) {
        if blank([&voice.provider, &voice.model, &voice.voice]) {
            found.push(format!(
                "voices.{owner}: provider, model and voice must not be empty"
            ));
        }
        if voice
            .sample
            .as_deref()
            .is_some_and(|path| !inside_channel(path))
        {
            found.push("voice sample must be a channel-relative path without traversal".into());
        }
        if voice
            .fallbacks
            .iter()
            .any(|fallback| blank([&fallback.provider, &fallback.model, &fallback.voice]))
        {
            found.push("voice fallback fields must be nonempty".into());
        }
    }
    found
}

fn look_problems(file: &ChannelFile) -> Vec<String> {
    let look = &file.look;
    let values = [
        ("look.vignette.amount", look.vignette.amount),
        ("look.vignette.midpoint", look.vignette.midpoint),
        ("look.vignette.feather", look.vignette.feather),
        ("look.grain.amount", look.grain.amount),
        ("look.grain.size", look.grain.size),
        ("look.grade.contrast", look.grade.contrast),
        ("look.grade.saturation", look.grade.saturation),
        ("look.grade.highlights", look.grade.highlights),
    ];
    let mut found: Vec<String> = values
        .into_iter()
        .filter(|(_, value)| !value.is_finite())
        .map(|(at, _)| format!("{at}: must be finite"))
        .collect();
    for (script, font) in &file.fonts {
        if font.family.trim().is_empty() || font.style.trim().is_empty() {
            found.push(format!(
                "fonts.{script:?}: family and style must not be empty"
            ));
        }
    }
    found
}

/// Largest side margin, as a fraction of frame width.
const MAX_SIDE_MARGIN: f64 = 0.5;

fn layout_problems(file: &ChannelFile) -> Vec<String> {
    let mut found = Vec::new();
    let margins = [
        ("captions.side_margin", file.captions.side_margin),
        ("title.side_margin", file.title.side_margin),
    ];
    for (at, margin) in margins {
        if !margin.is_finite() || !(0.0..MAX_SIDE_MARGIN).contains(&margin) {
            found.push(format!(
                "{at}: must be a finite fraction from 0 to below 0.5"
            ));
        }
    }
    for (at, height) in [("captions.y", file.captions.y), ("title.y", file.title.y)] {
        if !(0.0..=1.0).contains(&height) {
            found.push(format!(
                "{at}: {height} must be a fraction of the frame height (0 to 1)"
            ));
        }
    }
    if file.captions.size == 0 || file.title.size == 0 {
        found.push("caption/title sizes must be positive".into());
    }
    if file.captions.max_words == 0 || file.captions.max_chars == 0 {
        found.push("captions: max_words and max_chars must be at least 1".to_owned());
    }
    found
}

fn colour_problems(file: &ChannelFile) -> Vec<String> {
    let references = [
        ("captions.fill", &file.captions.fill),
        ("captions.stroke.color", &file.captions.stroke.color),
        ("captions.highlight", &file.captions.highlight),
    ];
    let names = || {
        file.colors
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    references
        .into_iter()
        .filter(|(_, colour)| resolve(file, colour).is_none())
        .map(|(at, colour)| {
            format!(
                "{at}: `{}` is neither \"#RRGGBB\" nor a colour name ({})",
                colour.0,
                names()
            )
        })
        .collect()
}

/// Built-in channel templates, as `(id, files)` where files maps relative path to bytes.
#[must_use]
#[cfg(any(test, feature = "test-fixtures"))]
pub fn templates() -> BTreeMap<String, &'static Dir<'static>> {
    TEMPLATES
        .dirs()
        .filter_map(|d| {
            let id = d.path().file_name()?.to_string_lossy().into_owned();
            Some((id, d))
        })
        .collect()
}

/// Copies a built-in template into `dest` (which must not exist yet) and loads it.
#[cfg(any(test, feature = "test-fixtures"))]
pub fn init_from_template(id: &str, dest: &Path) -> Result<Channel, LoadError> {
    let templates = templates();
    let dir = templates.get(id).ok_or_else(|| LoadError::Missing {
        origin: format!(
            "built-in channel templates (have: {})",
            templates.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
        file: "that template",
    })?;
    write_dir(dir, dir.path(), dest)?;
    Channel::load(dest)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn write_dir(dir: &Dir<'_>, root: &Path, dest: &Path) -> Result<(), LoadError> {
    for f in dir.files() {
        let rel = f.path().strip_prefix(root).unwrap_or_else(|_| f.path());
        let target = dest.join(rel);
        let io = |source| LoadError::Io {
            path: target.display().to_string(),
            source,
        };
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&target, f.contents()).map_err(io)?;
    }
    for sub in dir.dirs() {
        write_dir(sub, root, dest)?;
    }
    Ok(())
}

/// The channels in a folder, plus any that failed to load.
pub fn list(dir: &Path) -> (Vec<Channel>, Vec<(PathBuf, LoadError)>) {
    let mut ok: Vec<Channel> = Vec::new();
    let mut broken = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return (ok, broken);
    };
    let mut folders: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    folders.sort();
    for path in folders {
        match Channel::load(&path) {
            Ok(c) => {
                let id = c.id().to_owned();
                if let Some(pos) = ok.iter().position(|existing| existing.id() == id) {
                    let prev = ok.remove(pos);
                    broken.push((
                        prev.dir().to_path_buf(),
                        LoadError::invalid("channel.id", vec![format!("duplicate channel id `{id}`")]),
                    ));
                    broken.push((
                        path,
                        LoadError::invalid("channel.id", vec![format!("duplicate channel id `{id}`")]),
                    ));
                } else if broken.iter().any(|(_, err)| {
                    matches!(err, LoadError::Invalid { problems, .. } if problems.iter().any(|msg| msg.contains(&format!("duplicate channel id `{id}`"))))
                }) {
                    broken.push((
                        path,
                        LoadError::invalid("channel.id", vec![format!("duplicate channel id `{id}`")]),
                    ));
                } else {
                    ok.push(c);
                }
            }
            Err(e) => broken.push((path, e)),
        }
    }
    broken.sort_by(|(a, _), (b, _)| a.cmp(b));
    (ok, broken)
}

fn load_channel_from(dir: &Path) -> Result<Channel, LoadError> {
    let path = dir.join(CHANNEL_FILE);
    let shown = path.display().to_string();
    let src = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            LoadError::Missing {
                origin: dir.display().to_string(),
                file: CHANNEL_FILE,
            }
        } else {
            LoadError::Io {
                path: shown.clone(),
                source,
            }
        }
    })?;
    let file = parse(&shown, &src)?;
    for rel in [
        &file.library.style_reference,
        &file.library.music,
        &file.library.sfx,
    ]
    .into_iter()
    .chain(
        file.voices
            .values()
            .chain(file.character_voices.values())
            .filter_map(|v| v.sample.as_ref())
            .chain(file.room_tone.iter()),
    ) {
        let path = dir.join(rel);
        if path.is_symlink() {
            return Err(LoadError::invalid(
                &shown,
                vec![format!("asset path `{rel}` must not be a symlink")],
            ));
        }
        if path.exists() {
            let canonical = path.canonicalize().map_err(|source| LoadError::Io {
                path: path.display().to_string(),
                source,
            })?;
            let root = dir.canonicalize().map_err(|source| LoadError::Io {
                path: dir.display().to_string(),
                source,
            })?;
            if !canonical.starts_with(root) {
                return Err(LoadError::invalid(
                    &shown,
                    vec![format!("asset path `{rel}` escapes the channel folder")],
                ));
            }
        }
    }
    Ok(Channel {
        file,
        dir: dir.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_installs_and_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("test-studio");
        let ch = init_from_template("test-studio", &dest).unwrap();
        assert_eq!(ch.file().name, "Test Studio");
        assert_eq!(
            ch.color(&ch.file().captions.highlight).to_string(),
            "#FF5A1F"
        );
        assert!(dest.join("style-ref.png").exists());
        assert!(ch.voice(&"en".parse().unwrap()).is_ok());
        assert!(
            ch.voice(&"fr".parse().unwrap())
                .unwrap_err()
                .contains("French")
        );
        let (all, broken) = list(tmp.path());
        assert_eq!((all.len(), broken.len()), (1, 0));
    }

    #[test]
    fn bad_colour_reference_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("c");
        init_from_template("test-studio", &dest).unwrap();
        let path = dest.join(CHANNEL_FILE);
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("fill = \"cream\"", "fill = \"creme\"");
        std::fs::write(&path, text).unwrap();
        let err = Channel::load(&dest).unwrap_err();
        assert!(matches!(err, LoadError::Invalid { .. }), "{err:?}");
    }

    #[test]
    fn duplicate_channel_ids_are_both_marked_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let dest1 = tmp.path().join("c1");
        let dest2 = tmp.path().join("c2");
        init_from_template("test-studio", &dest1).unwrap();
        init_from_template("test-studio", &dest2).unwrap();

        let (ok, broken) = list(tmp.path());
        assert!(ok.is_empty(), "duplicate channels must not be usable");
        assert_eq!(
            broken.len(),
            2,
            "both duplicate channels must be marked broken"
        );
    }

    #[test]
    fn hidden_staging_directories_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let staged = tmp.path().join(".retold-create-channel-123");
        init_from_template("test-studio", &staged).unwrap();

        let (ok, broken) = list(tmp.path());
        assert!(ok.is_empty());
        assert!(broken.is_empty());
    }

    #[test]
    fn symlink_asset_paths_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("c");
        init_from_template("test-studio", &dest).unwrap();
        let style_ref = dest.join("style-ref.png");
        std::fs::remove_file(&style_ref).unwrap();
        let external = tmp.path().join("outside.png");
        std::fs::write(&external, b"png").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&external, &style_ref).unwrap();

        #[cfg(unix)]
        {
            let err = Channel::load(&dest).unwrap_err();
            assert!(matches!(err, LoadError::Invalid { .. }), "{err:?}");
        }
    }
}
