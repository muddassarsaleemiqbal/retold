//! A canon: a franchise, film series, TV series or book series researched once, part by
//! part, then sliced spoiler-safely for each part's video.
//!
//! A canon is built by a run whose part input is `canon`. It discovers the parts from the
//! franchise sources, researches each part with the normal research (sources, extraction,
//! dossier, character bible; every answer cached under a per-part step scope, so an
//! interrupted or extended build repeats nothing), and merges the characters into one cast
//! with a status and look per part. Everything is saved beside the shared cast, so every
//! part video in the franchise folder finds it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use retold_guide::inputs::Inputs;
use retold_guide::manifest::{CanonOrder, CanonRules, DossierRules, InputValue};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dossier::{Bible, Dossier};
use crate::pipeline::{Pipeline, PipelineError};
use crate::research::{self, Source};
use crate::run::RunMeta;
use crate::store::{Store, hash};
use crate::{EngineError, Stage, fsx};

/// The part input value that builds the canon instead of a video.
pub const BUILD: &str = "canon";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
// No doc comments on variants: they turn the schema into `oneOf`, which strict structured output rejects.
// main: A numbered film, a season, or a book in the main series.
// spinoff: A spin-off film or series set in the same world.
// special: A short, special or other side story.
pub enum PartKind {
    Main,
    Spinoff,
    Special,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// `P01`, `P02`… in release order.
    pub id: String,
    pub title: String,
    pub kind: PartKind,
    /// Release year; 0 when unknown.
    pub year: u32,
    /// Season number for a TV series; 0 otherwise.
    pub season: u32,
    /// 1 for the first part released.
    pub release_order: u32,
    /// Position in the story's own chronology (1 = earliest events).
    pub story_order: u32,
    /// Two or three sentences: what happens in this part.
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartList {
    parts: Vec<Part>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
// No doc comments on variants: they turn the schema into `oneOf`, which strict structured output rejects.
// present: Appears and is alive.
// absent: Alive but not in this part.
// presumed_dead: Believed dead by the other characters (or the audience) during this part.
// dead: Dead.
// returned: Back after being presumed dead or absent.
pub enum Status {
    Present,
    Absent,
    PresumedDead,
    Dead,
    Returned,
}

/// A character's state in one part.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartState {
    pub part: String,
    pub status: Status,
    /// Their role in this part, one line (empty when absent).
    pub role: String,
    /// What changes in their look for this part (clothes, hair, age, injury); empty when
    /// it matches their base look.
    pub look: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanonCharacter {
    pub name: String,
    /// Other names the parts use for them (surname, nickname, title).
    pub aliases: Vec<String>,
    /// Base design: age, build, face, hair, skin, usual clothes; never an actor's likeness.
    pub look: String,
    /// Part id where they first appear.
    pub first_part: String,
    /// One entry per researched part, in release order.
    pub timeline: Vec<PartState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CastMerge {
    characters: Vec<CanonCharacter>,
}

/// The saved canon. Part research lives in `parts/<id>.json` beside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Canon {
    /// Content hash of everything below; part videos record it.
    pub version: String,
    pub work: String,
    pub kind: String,
    /// Researched parts, in release order.
    pub parts: Vec<Part>,
    /// Parts found but not researched (spin-offs and specials, unless asked for).
    pub optional: Vec<Part>,
    pub characters: Vec<CanonCharacter>,
}

/// One part's research, as a part video uses it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartResearch {
    pub part: Part,
    pub source: Source,
    pub dossier: Dossier,
    pub bible: Bible,
}

/// Where a franchise's canon is saved: beside its shared cast.
#[must_use]
pub fn dir(layout: &crate::RunLayout) -> PathBuf {
    layout.cast_dir().join("canon")
}

/// The saved canon, if one has been built for this franchise folder.
pub fn load(layout: &crate::RunLayout) -> Result<Option<Canon>, EngineError> {
    read(&dir(layout).join("canon.json"))
}

pub fn load_part(layout: &crate::RunLayout, id: &str) -> Result<Option<PartResearch>, EngineError> {
    read(&dir(layout).join("parts").join(format!("{id}.json")))
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, EngineError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| EngineError::Config {
                path: path.into(),
                message: e.to_string(),
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EngineError::io(path, e)),
    }
}

fn text(inputs: &Inputs, name: Option<&str>) -> String {
    name.and_then(|n| inputs.get(n))
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Whether this run builds the canon rather than a video.
#[must_use]
pub fn is_build(rules: &CanonRules, inputs: &Inputs) -> bool {
    text(inputs, Some(&rules.part_input))
        .trim()
        .eq_ignore_ascii_case(BUILD)
}

/// Finds the part a video asks for: a canon id (`P03`), a release number (`3`), a season
/// (`Season 2`), or a title (case-insensitive).
#[must_use]
pub fn find_part<'a>(canon: &'a Canon, wanted: &str) -> Option<&'a Part> {
    let w = wanted.trim().to_lowercase();
    let number: Option<u32> = w
        .trim_start_matches("season")
        .trim_start_matches("part")
        .trim()
        .parse()
        .ok();
    canon.parts.iter().chain(&canon.optional).find(|p| {
        p.id.eq_ignore_ascii_case(&w)
            || p.title.to_lowercase() == w
            || (w.starts_with("season") && number.is_some_and(|n| p.season == n))
            || (!w.starts_with("season")
                && p.kind == PartKind::Main
                && number.is_some_and(|n| p.release_order == n))
    })
}

/// Why a discovered part list cannot be used; empty when it can.
#[must_use]
pub fn part_gates(parts: &[Part]) -> Vec<String> {
    let mut errors = Vec::new();
    if !parts.iter().any(|part| part.kind == PartKind::Main) {
        errors.push("list at least one main part".into());
    }
    let invalid_id = parts.iter().any(|part| !crate::store::is_part_id(&part.id));
    if invalid_id {
        errors.push("part ids must be P01, P02…".into());
    }
    let ids: BTreeSet<_> = parts.iter().map(|p| p.id.as_str()).collect();
    if ids.len() != parts.len() {
        errors.push("part ids must be unique".into());
    }
    let mut releases: Vec<_> = parts.iter().map(|p| p.release_order).collect();
    releases.sort_unstable();
    if releases != (1..=u32::try_from(parts.len()).unwrap_or(u32::MAX)).collect::<Vec<_>>() {
        errors.push("release_order must number the parts 1, 2, 3… without gaps".into());
    }
    if parts
        .iter()
        .any(|p| p.title.trim().is_empty() || p.summary.trim().is_empty() || p.story_order == 0)
    {
        errors.push("every part needs a title, a summary and a story_order".into());
    }
    errors
}

/// Why a merged cast cannot be used; empty when it can.
#[must_use]
pub fn cast_gates(
    cast: &[CanonCharacter],
    parts: &[Part],
    bibles: &[(String, Bible)],
) -> Vec<String> {
    let mut errors = Vec::new();
    let ids: BTreeSet<_> = parts.iter().map(|p| p.id.as_str()).collect();
    let known = |name: &str| {
        cast.iter().any(|c| {
            c.name.eq_ignore_ascii_case(name)
                || c.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
        })
    };
    for (part, bible) in bibles {
        for c in &bible.characters {
            if !known(&c.name) {
                errors.push(format!("{} (from {part}) is missing from the cast", c.name));
            }
        }
    }
    for c in cast {
        if !ids.contains(c.first_part.as_str()) {
            errors.push(format!(
                "{}: first_part must be a researched part id",
                c.name
            ));
        }
        let listed: Vec<_> = c.timeline.iter().map(|s| s.part.as_str()).collect();
        if listed.len() != ids.len() || !listed.iter().all(|p| ids.contains(p)) {
            errors.push(format!(
                "{}: give exactly one timeline entry per researched part",
                c.name
            ));
        }
    }
    errors
}

/// Inputs for researching one part with the guide's normal sources.
fn part_inputs(inputs: &Inputs, rules: &CanonRules, work: &str, part: &Part) -> Inputs {
    let mut out = inputs.clone();
    let series = part.season > 0;
    let set = |out: &mut Inputs, k: &str, v: Option<InputValue>| {
        out.0.insert(k.into(), v);
    };
    set(
        &mut out,
        "title",
        Some(InputValue::Text(if series {
            work.to_owned()
        } else {
            part.title.clone()
        })),
    );
    set(
        &mut out,
        &rules.work_input,
        Some(InputValue::Text(work.to_owned())),
    );
    set(
        &mut out,
        &rules.part_input,
        Some(InputValue::Text(part.title.clone())),
    );
    set(
        &mut out,
        "year",
        (part.year > 0).then(|| InputValue::Int(i64::from(part.year))),
    );
    set(
        &mut out,
        "season",
        series.then(|| InputValue::Text(format!("Season {}", part.season))),
    );
    out
}

impl Pipeline<'_> {
    /// Builds (or extends) the canon for this run's work and saves it beside the cast.
    pub(crate) async fn build_canon(
        &self,
        store: &mut Store,
        rules: &CanonRules,
        dossier_rules: &DossierRules,
    ) -> Result<Canon, PipelineError> {
        let work = text(&self.meta.inputs, Some(&rules.work_input));
        if work.trim().is_empty() {
            return Err(PipelineError::Gates(format!(
                "set `{}` to the franchise or series name",
                rules.work_input
            )));
        }
        let kind = text(&self.meta.inputs, rules.kind_input.as_deref());
        let with_spinoffs = text(&self.meta.inputs, rules.spinoffs_input.as_deref()) == "true";
        let mut list = self.discover_parts(store, &work, &kind).await?;
        list.parts.sort_by_key(|p| p.release_order);
        let (parts, optional): (Vec<_>, Vec<_>) = list
            .parts
            .into_iter()
            .partition(|p| with_spinoffs || p.kind == PartKind::Main);
        let folder = dir(self.layout).join("parts");
        let mut bibles = Vec::new();
        for part in &parts {
            self.check_cancel()?;
            (self.progress)(
                &format!("canon: researching {} ({})", part.title, part.id),
                false,
            );
            let saved = folder.join(format!("{}.json", part.id));
            let research = self
                .research_part(store, rules, dossier_rules, &work, part)
                .await?;
            fsx::write_json_atomic(&saved, &research)?;
            bibles.push((part.id.clone(), research.bible));
        }
        let characters = self.merge_cast(store, &work, &parts, &bibles).await?;
        let mut canon = Canon {
            version: String::new(),
            work,
            kind,
            parts,
            optional,
            characters,
        };
        canon.version = hash(&(
            &canon.parts,
            &canon.optional,
            &canon.characters,
            &bibles.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        ))?;
        fsx::write_json_atomic(&dir(self.layout).join("canon.json"), &canon)?;
        self.write(Stage::Research, "canon.json", &canon)?;
        Ok(canon)
    }

    /// Every part of the work, from the guide's sources about the work as a whole.
    async fn discover_parts(
        &self,
        store: &mut Store,
        work: &str,
        kind: &str,
    ) -> Result<PartList, PipelineError> {
        let m = self.guide.manifest();
        let work = work.to_owned();
        // Franchise-level sources: the guide's normal sources, about the work as a whole.
        let mut franchise = self.meta.inputs.clone();
        franchise
            .0
            .insert("title".into(), Some(InputValue::Text(work.clone())));
        let key = hash(
            &serde_json::json!({"version": 1, "inputs": franchise, "guide": self.guide.content_hash()}),
        )?;
        let sources: Source = if let Some(s) = store.cached("canon-sources", &key)? {
            s
        } else {
            store.begin("canon-sources")?;
            let s = research::gather(self.guide, &franchise, None, self.cancel).await?;
            store.commit("canon-sources", &key, &s, None)?;
            s
        };
        let prompt = format!(
            "Work: {work} (kind: {kind}). From these sources, list every part of this work: each numbered film, season or book of the main series, and every spin-off, special or short set in the same world. id: P01, P02… in release order. kind: main, spinoff or special. year: release year (0 if unknown). season: the season number for a TV series (0 otherwise). release_order: 1 for the first released, counting every listed part. story_order: position in the story's own chronology (equal to release_order unless events happen out of order). summary: two or three sentences on what happens. Only parts the sources name; nothing announced but unreleased unless the sources describe its plot.\nSOURCES:\n{}",
            research::truncate(&sources.text, 60_000)
        );
        let mut list: PartList = self
            .ask(store, "canon-parts", m.roles.plan, prompt.clone())
            .await?;
        let problems = part_gates(&list.parts);
        if !problems.is_empty() {
            list = self
                .ask(
                    store,
                    "canon-parts-fix",
                    m.roles.plan,
                    format!(
                        "{prompt}\nYour previous list was rejected: {}. Answer again.",
                        problems.join("; ")
                    ),
                )
                .await?;
            let problems = part_gates(&list.parts);
            if !problems.is_empty() {
                return Err(PipelineError::Gates(format!(
                    "canon parts: {}",
                    problems.join("; ")
                )));
            }
        }
        Ok(list)
    }

    /// One part researched exactly as a video of it would be, under its own step scope.
    async fn research_part(
        &self,
        store: &mut Store,
        rules: &CanonRules,
        dossier_rules: &DossierRules,
        work: &str,
        part: &Part,
    ) -> Result<PartResearch, PipelineError> {
        let mut meta: RunMeta = self.meta.clone();
        meta.title = part.title.clone();
        meta.inputs = part_inputs(&self.meta.inputs, rules, work, part);
        let scope = format!("{}{}-", self.scope, part.id.to_lowercase());
        let scoped = Pipeline {
            meta: &meta,
            scope: &scope,
            ..*self
        };
        let key = hash(
            &serde_json::json!({"version": 1, "inputs": meta.inputs, "guide": self.guide.content_hash()}),
        )?;
        let step = format!("{scope}research");
        let mut source: Source = if let Some(s) = store.cached(&step, &key)? {
            s
        } else {
            store.begin(&step)?;
            let s = research::gather(self.guide, &meta.inputs, None, self.cancel).await?;
            store.commit(&step, &key, &s, None)?;
            s
        };
        let (dossier, bible) = scoped.research(store, &mut source, dossier_rules).await?;
        Ok(PartResearch {
            part: part.clone(),
            source,
            dossier,
            bible,
        })
    }

    async fn merge_cast(
        &self,
        store: &mut Store,
        work: &str,
        parts: &[Part],
        bibles: &[(String, Bible)],
    ) -> Result<Vec<CanonCharacter>, PipelineError> {
        let m = self.guide.manifest();
        let prompt = format!(
            "Work: {work}. Merge these per-part character bibles into one cast for the whole work. One entry per real person, even when parts spell or name them differently (list other names in aliases). look: one base design that stays recognisable across every part (age at first appearance, build, face, hair, skin, usual clothes), an original illustration, never an actor's likeness. first_part: the part id where they first appear. timeline: exactly one entry per part id in {:?}: status present, absent, presumed_dead, dead or returned (returned only when they come back after being presumed dead or absent); role in that part (empty when absent); look: only what changes for that part (clothes, hair, age, injury), empty when unchanged. Use only these bibles.\nPARTS: {}\nBIBLES: {}",
            parts.iter().map(|p| &p.id).collect::<Vec<_>>(),
            serde_json::to_string(parts).map_err(|e| EngineError::Setup(e.to_string()))?,
            serde_json::to_string(bibles).map_err(|e| EngineError::Setup(e.to_string()))?,
        );
        let mut merged: CastMerge = self
            .ask(store, "canon-cast", m.roles.plan, prompt.clone())
            .await?;
        let problems = cast_gates(&merged.characters, parts, bibles);
        if !problems.is_empty() {
            merged = self
                .ask(
                    store,
                    "canon-cast-fix",
                    m.roles.plan,
                    format!(
                        "{prompt}\nYour previous cast was rejected: {}. Answer again.",
                        problems.join("; ")
                    ),
                )
                .await?;
            let problems = cast_gates(&merged.characters, parts, bibles);
            if !problems.is_empty() {
                return Err(PipelineError::Gates(format!(
                    "canon cast: {}",
                    problems.join("; ")
                )));
            }
        }
        Ok(merged.characters)
    }
}

/// What one part's video may know: earlier parts (by the guide's spoiler order), each
/// character's state going into this part, and names that only appear later.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slice {
    pub canon_version: String,
    pub work: String,
    pub part: Part,
    /// Main-series position and count, for "Part 3 of 9".
    pub number: u32,
    pub of: u32,
    pub earlier: Vec<Part>,
    /// (name, state in the latest earlier part where they appeared), for characters met before.
    pub returning: Vec<(String, PartState)>,
    /// Character names (and aliases) first seen in later parts and not in this one, and
    /// later part titles: a script naming any of them spoils what comes next.
    pub forbidden: Vec<String>,
}

/// The spoiler-safe view of `canon` for `part`.
#[must_use]
pub fn slice(canon: &Canon, part: &Part, bible: &Bible, order: CanonOrder) -> Slice {
    let pos = |p: &Part| match order {
        CanonOrder::Release => p.release_order,
        CanonOrder::Story => p.story_order,
    };
    let here = pos(part);
    let earlier: Vec<Part> = canon
        .parts
        .iter()
        .filter(|p| pos(p) < here)
        .cloned()
        .collect();
    let earlier_ids: BTreeSet<_> = earlier.iter().map(|p| p.id.as_str()).collect();
    let later_ids: BTreeSet<_> = canon
        .parts
        .iter()
        .chain(&canon.optional)
        .filter(|p| pos(p) > here)
        .map(|p| p.id.as_str())
        .collect();
    let in_part = |c: &CanonCharacter| {
        bible.characters.iter().any(|b| {
            b.name.eq_ignore_ascii_case(&c.name)
                || c.aliases.iter().any(|a| a.eq_ignore_ascii_case(&b.name))
        })
    };
    let mut returning = Vec::new();
    let mut forbidden = Vec::new();
    for c in &canon.characters {
        let seen: Vec<_> = c
            .timeline
            .iter()
            .filter(|s| earlier_ids.contains(s.part.as_str()) && s.status != Status::Absent)
            .collect();
        if let Some(last) = seen.last() {
            returning.push((c.name.clone(), (*last).clone()));
        } else if later_ids.contains(c.first_part.as_str()) && !in_part(c) {
            forbidden.push(c.name.clone());
            forbidden.extend(c.aliases.iter().filter(|a| a.chars().count() >= 4).cloned());
        }
    }
    forbidden.extend(
        canon
            .parts
            .iter()
            .chain(&canon.optional)
            .filter(|p| later_ids.contains(p.id.as_str()))
            .map(|p| p.title.clone()),
    );
    // A later title that is just the work's name (e.g. a reboot) is not a spoiler.
    forbidden
        .retain(|f| !f.eq_ignore_ascii_case(&canon.work) && !f.eq_ignore_ascii_case(&part.title));
    let mains: Vec<_> = canon
        .parts
        .iter()
        .filter(|p| p.kind == PartKind::Main)
        .collect();
    Slice {
        canon_version: canon.version.clone(),
        work: canon.work.clone(),
        part: part.clone(),
        number: mains
            .iter()
            .position(|p| p.id == part.id)
            .map_or(0, |i| u32::try_from(i + 1).unwrap_or(0)),
        of: u32::try_from(mains.len()).unwrap_or(0),
        earlier,
        returning,
        forbidden,
    }
}

/// How each finished part video opened, so later parts open differently.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Openings {
    /// (part id, the opening section's text).
    pub parts: Vec<(String, String)>,
}

fn openings_path(layout: &crate::RunLayout) -> PathBuf {
    dir(layout).join("openings.json")
}

/// Records this part's opening (replacing an earlier one for the same part).
pub fn record_opening(
    layout: &crate::RunLayout,
    part: &str,
    text: &str,
) -> Result<(), EngineError> {
    let mut saved: Openings = read(&openings_path(layout))?.unwrap_or_default();
    saved.parts.retain(|(p, _)| p != part);
    saved.parts.push((part.to_owned(), text.trim().to_owned()));
    fsx::write_json_atomic(&openings_path(layout), &saved)
}

/// Openings of earlier parts' videos, for the writer to avoid. Later parts are left out so a
/// finished script is not invalidated (and rewritten) whenever a sequel is scripted.
pub fn other_openings(
    layout: &crate::RunLayout,
    slice: &Slice,
) -> Result<Vec<String>, EngineError> {
    Ok(read::<Openings>(&openings_path(layout))?
        .unwrap_or_default()
        .parts
        .into_iter()
        .filter(|(p, _)| slice.earlier.iter().any(|e| &e.id == p))
        .map(|(_, t)| t)
        .collect())
}

/// What the script writer is told about the rest of the work.
#[must_use]
pub fn context(slice: &Slice) -> String {
    let earlier: Vec<_> = slice
        .earlier
        .iter()
        .map(|p| format!("{} ({}): {}", p.title, p.year, p.summary))
        .collect();
    let returning: Vec<_> = slice
        .returning
        .iter()
        .map(|(name, s)| {
            format!(
                "{name}: {:?} after {}{}",
                s.status,
                s.part,
                if s.role.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", s.role)
                }
            )
        })
        .collect();
    format!(
        "SERIES: this video tells {} — part {} of {} in {}. EARLIER PARTS (the viewer may have seen them; refer back only briefly): {}. RETURNING CHARACTERS as of the start of this part: {}. NEVER mention these names or titles, which belong to later parts: {}.",
        slice.part.title,
        slice.number,
        slice.of,
        slice.work,
        if earlier.is_empty() {
            "none".into()
        } else {
            earlier.join(" | ")
        },
        if returning.is_empty() {
            "none".into()
        } else {
            returning.join("; ")
        },
        if slice.forbidden.is_empty() {
            "none".into()
        } else {
            slice.forbidden.join(", ")
        },
    )
}

/// Words from `forbidden` that the narration names (whole words, case-insensitive).
#[must_use]
pub fn spoilers(narration: &str, forbidden: &[String]) -> Vec<String> {
    let text = narration.to_lowercase();
    forbidden
        .iter()
        .filter(|f| {
            let f = f.to_lowercase();
            let needle_len = f.len();
            text.match_indices(&f).any(|(i, _)| {
                let before = text[..i]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric());
                let after = text[i + needle_len..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric());
                before && after
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(id: &str, release: u32, story: u32, kind: PartKind) -> Part {
        Part {
            id: id.into(),
            title: format!("Film {id}"),
            kind,
            year: 2000 + release,
            season: 0,
            release_order: release,
            story_order: story,
            summary: "s".into(),
        }
    }

    fn character(name: &str, first: &str, states: &[(&str, Status)]) -> CanonCharacter {
        CanonCharacter {
            name: name.into(),
            aliases: vec![],
            look: "l".into(),
            first_part: first.into(),
            timeline: states
                .iter()
                .map(|(p, s)| PartState {
                    part: (*p).into(),
                    status: *s,
                    role: String::new(),
                    look: String::new(),
                })
                .collect(),
        }
    }

    fn canon() -> Canon {
        Canon {
            version: "v".into(),
            work: "Fast".into(),
            kind: "film".into(),
            // P03 is released third but happens last in the story.
            parts: vec![
                part("P01", 1, 1, PartKind::Main),
                part("P02", 2, 2, PartKind::Main),
                part("P03", 3, 4, PartKind::Main),
                part("P04", 4, 3, PartKind::Main),
            ],
            optional: vec![part("P05", 5, 5, PartKind::Spinoff)],
            characters: vec![
                character(
                    "Dom",
                    "P01",
                    &[
                        ("P01", Status::Present),
                        ("P02", Status::Present),
                        ("P03", Status::Absent),
                        ("P04", Status::Present),
                    ],
                ),
                character(
                    "Han",
                    "P03",
                    &[
                        ("P01", Status::Absent),
                        ("P02", Status::Absent),
                        ("P03", Status::Dead),
                        ("P04", Status::Present),
                    ],
                ),
            ],
        }
    }

    #[test]
    fn parts_are_found_by_id_number_season_or_title() {
        let c = canon();
        assert_eq!(find_part(&c, "p02").unwrap().id, "P02");
        assert_eq!(find_part(&c, "3").unwrap().id, "P03");
        assert_eq!(find_part(&c, "Film P04").unwrap().id, "P04");
        assert_eq!(find_part(&c, "Film P05").unwrap().id, "P05");
        assert!(find_part(&c, "Season 2").is_none());
    }

    #[test]
    fn slices_hide_later_parts_in_release_or_story_order() {
        let c = canon();
        let empty = Bible { characters: vec![] };
        let second = slice(&c, &c.parts[1], &empty, CanonOrder::Release);
        assert_eq!(
            second
                .earlier
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["P01"]
        );
        assert_eq!(
            second
                .returning
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            ["Dom"]
        );
        assert!(
            second.forbidden.contains(&"Han".to_owned())
                && second.forbidden.contains(&"Film P03".to_owned())
        );
        assert_eq!((second.number, second.of), (2, 4));
        // In story order, P04 (released fourth) comes before P03.
        let fourth = slice(&c, &c.parts[3], &empty, CanonOrder::Story);
        assert_eq!(
            fourth
                .earlier
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["P01", "P02"]
        );
        assert!(fourth.forbidden.contains(&"Film P03".to_owned()));
        assert_eq!(
            spoilers("Then HAN arrives, and Dom smiles.", &second.forbidden),
            ["Han"]
        );
        assert_eq!(
            spoilers("Hand over the keys.", &second.forbidden),
            Vec::<String>::new()
        );
    }

    #[test]
    fn part_lists_and_casts_are_checked() {
        let c = canon();
        assert_eq!(part_gates(&c.parts), Vec::<String>::new());
        let mut gap = c.parts.clone();
        gap[1].release_order = 7;
        assert_eq!(part_gates(&gap).len(), 1);
        assert_eq!(
            cast_gates(&c.characters, &c.parts, &[]),
            Vec::<String>::new()
        );
        let bible: Bible = serde_json::from_value(serde_json::json!({"characters":[{"name":"Letty","role":"","introduction":"","want":"","flaw":"","arc":"","turning_points":[],"signature_line":"","signature_object":"","relationships":[],"look":"","emotions":[]}]})).unwrap();
        assert_eq!(
            cast_gates(&c.characters, &c.parts, &[("P01".into(), bible)]).len(),
            1
        );
    }
}

#[cfg(test)]
mod guide_tests {
    use retold_guide::manifest::GuideManifest;

    fn query(inputs: &serde_json::Value) -> String {
        let m: GuideManifest = toml::from_str(include_str!(
            "../../../docs/examples/story-recap/guide.toml"
        ))
        .unwrap();
        let spec = m
            .research
            .sources
            .iter()
            .find(|s| s.kind == retold_guide::manifest::SourceKind::Wikipedia)
            .unwrap();
        retold_guide::template::render(
            "q",
            spec.query.as_deref().unwrap(),
            serde_json::json!({ "inputs": inputs }),
        )
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
    }

    #[test]
    fn story_recap_queries_name_the_right_page() {
        let film = query(
            &serde_json::json!({"kind":"film","work":"Drishyam","part":"Drishyam","year":2015,"author":null}),
        );
        assert_eq!(film, "Drishyam 2015 film");
        let canon = query(
            &serde_json::json!({"kind":"film","work":"Fast & Furious","part":"canon","year":null,"author":null}),
        );
        assert_eq!(canon, "Fast & Furious film series");
        let season = query(
            &serde_json::json!({"kind":"series","work":"You","part":"Season 2","year":null,"author":null}),
        );
        assert_eq!(season, "You TV series");
        let book = query(
            &serde_json::json!({"kind":"book","work":"Frankenstein","part":"Frankenstein","year":null,"author":"Mary Shelley"}),
        );
        assert_eq!(book, "Frankenstein Mary Shelley novel");
    }

    #[test]
    fn canon_schemas_have_no_unions() {
        let part_list_schema =
            serde_json::to_string(&schemars::schema_for!(super::PartList)).unwrap();
        assert!(
            !part_list_schema.contains("oneOf") && !part_list_schema.contains("anyOf"),
            "{part_list_schema}"
        );
        let cast_merge_schema =
            serde_json::to_string(&schemars::schema_for!(super::CastMerge)).unwrap();
        assert!(
            !cast_merge_schema.contains("oneOf") && !cast_merge_schema.contains("anyOf"),
            "{cast_merge_schema}"
        );
    }
}
