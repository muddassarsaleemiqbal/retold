//! The research dossier: cited story beats, character profiles and iconic details gathered
//! from every source, plus the checks that keep a script faithful to it.

use std::collections::{BTreeSet, HashSet};

use retold_guide::manifest::DossierRules;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::research::Source;
use crate::story::{Plan, Violation};

/// Parts longer than this are read in chunks by the fast model before reconciling.
pub const CHUNK_CHARS: usize = 40_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Beat,
    Character,
    Line,
    Object,
    Symbol,
    Setting,
    Foreshadow,
    EasterEgg,
    AdaptationDiff,
    Theme,
    Trivia,
}

/// One fact read from one source chunk.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub kind: FactKind,
    pub text: String,
    pub who: Vec<String>,
    /// Chapter, act or rough position in the story; empty when unknown.
    pub when: String,
    /// Exact words from the source for lines and quotes; otherwise empty.
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Extract {
    pub facts: Vec<Fact>,
}

/// What live web research found; every finding carries the page it came from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WebNotes {
    pub findings: Vec<WebFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WebFinding {
    pub kind: FactKind,
    pub text: String,
    pub quote: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
// No doc comments on the variants: they turn the schema into `oneOf`, which strict
// structured output rejects. must: removing it breaks cause and effect or the ending;
// should: character, iconic or emotional weight; could: texture.
pub enum Importance {
    Must,
    Should,
    Could,
}

/// Exactly which work is being recapped, so facts from other versions stay out.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkPin {
    pub title: String,
    /// novel, film, series, play…
    pub medium: String,
    pub year: String,
    pub language: String,
    pub creator: String,
    /// Which version this is and which versions are excluded.
    pub version_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DossierBeat {
    /// `B01`, `B02`… in story order.
    pub id: String,
    pub text: String,
    pub characters: Vec<String>,
    pub importance: Importance,
    /// Source ids (`S1`) or web URLs.
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Iconic {
    /// `I01`, `I02`…
    pub id: String,
    pub kind: FactKind,
    pub text: String,
    /// Verbatim line when this is a quote; otherwise empty.
    pub quote: String,
    /// Beat where it is introduced, and beat where it pays off (may be the same).
    pub setup_beat: String,
    pub payoff_beat: String,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dossier {
    pub work: WorkPin,
    pub logline: String,
    pub beats: Vec<DossierBeat>,
    pub iconic: Vec<Iconic>,
    pub settings: Vec<Place>,
    pub themes: Vec<String>,
    /// Places where sources disagree, and how each was resolved.
    pub conflicts: Vec<String>,
    /// Names a narrator could mispronounce, with a phonetic respelling.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pronunciations: Vec<Pronunciation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pronunciation {
    /// The name as written in the script.
    pub name: String,
    /// How a text-to-speech voice should say it, spelled plainly (e.g. "WAR-grave").
    pub say: String,
}

/// A character as a writer needs them, and as the illustrator draws them.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    /// protagonist, antagonist, ally, victim, mentor…
    pub role: String,
    /// One line on who they are, used the first time they are named.
    pub introduction: String,
    pub want: String,
    pub flaw: String,
    pub arc: String,
    pub turning_points: Vec<String>,
    pub signature_line: String,
    pub signature_object: String,
    pub relationships: Vec<String>,
    /// Age, build, hair, skin, clothes of the period; never an actor's likeness.
    pub look: String,
    /// Expressions their scenes need, e.g. calm, terrified, enraged.
    pub emotions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bible {
    pub characters: Vec<Profile>,
}

/// Which planned beats and iconic elements a script realises, with the line that does it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub beats: Vec<BeatCoverage>,
    pub iconic: Vec<IconicCoverage>,
    /// Named characters whose first mention does not say who they are.
    pub unintroduced: Vec<String>,
    /// Names or events that belong to a different version of the work.
    pub foreign: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BeatCoverage {
    pub id: String,
    /// The exact script sentence that realises the beat; empty when missing.
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IconicCoverage {
    pub id: String,
    /// Exact script sentences; empty when missing.
    pub setup_line: String,
    pub payoff_line: String,
}

/// Source excerpts as (citation id, text).
pub type Excerpts = Vec<(String, String)>;

/// Drops paragraphs that repeat an earlier one (in this or an earlier source part), so
/// wikis copying each other are not read and extracted twice. Short paragraphs stay.
#[must_use]
pub fn without_repeats(source: &Source) -> Source {
    let paragraphs: Vec<(usize, &str)> = source
        .parts
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.text.split("\n\n").map(move |t| (i, t)))
        .collect();
    let texts: Vec<&str> = paragraphs.iter().map(|(_, t)| *t).collect();
    let repeated = retold_algo::retrieval::near_duplicates(&texts, 0.8);
    let mut out = source.clone();
    for (index, part) in out.parts.iter_mut().enumerate() {
        part.text = paragraphs
            .iter()
            .zip(&repeated)
            .filter(|((i, _), dup)| *i == index && !**dup)
            .map(|((_, t), _)| *t)
            .collect::<Vec<_>>()
            .join("\n\n");
    }
    out
}

/// Splits long source parts for extraction; short parts are read whole by the reconciler.
#[must_use]
pub fn chunks(source: &Source) -> (Excerpts, Excerpts) {
    let mut long = Vec::new();
    let mut short = Vec::new();
    for part in &without_repeats(source).parts {
        let header = format!("[{}] {}: {}", part.id, part.kind, part.title);
        if part.text.chars().count() <= CHUNK_CHARS {
            short.push((part.id.clone(), format!("## {header}\n{}", part.text)));
            continue;
        }
        let mut current = String::new();
        let mut n = 0;
        for paragraph in part.text.split("\n\n") {
            let current_chars = current.chars().count();
            let paragraph_chars = paragraph.chars().count();
            if current_chars + paragraph_chars > CHUNK_CHARS && !current.is_empty() {
                n += 1;
                long.push((
                    format!("{}-{n}", part.id),
                    format!("## {header} (part {n})\n{current}"),
                ));
                current.clear();
            }
            current.push_str(paragraph);
            current.push_str("\n\n");
        }
        if !current.trim().is_empty() {
            n += 1;
            long.push((
                format!("{}-{n}", part.id),
                format!("## {header} (part {n})\n{current}"),
            ));
        }
    }
    (long, short)
}

/// Web findings become a cited source part, so the fact check can quote them.
#[must_use]
pub fn web_text(notes: &WebNotes) -> String {
    notes
        .findings
        .iter()
        .map(|f| {
            let quote = if f.quote.is_empty() {
                String::new()
            } else {
                format!(" \"{}\"", f.quote)
            };
            format!("- ({:?}) {}{quote} <{}>", f.kind, f.text, f.url)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Distinct sources the dossier cites, not counting the user's own file.
fn cited_sources(dossier: &Dossier, source: &Source) -> BTreeSet<String> {
    let file_ids: HashSet<_> = source
        .parts
        .iter()
        .filter(|p| p.kind == "file")
        .map(|p| p.id.as_str())
        .collect();
    let web_ids: HashSet<_> = source
        .parts
        .iter()
        .filter(|p| p.kind == "web")
        .map(|p| p.id.as_str())
        .collect();
    dossier
        .beats
        .iter()
        .flat_map(|b| &b.sources)
        .chain(dossier.iconic.iter().flat_map(|i| &i.sources))
        .map(|s| s.trim().to_owned())
        // A web part is a bundle of pages; its URLs count, its id does not.
        .filter(|s| {
            !s.is_empty() && !file_ids.contains(s.as_str()) && !web_ids.contains(s.as_str())
        })
        .collect()
}

/// Dossier minimums and internal consistency.
#[must_use]
pub fn dossier_gates(dossier: &Dossier, source: &Source, rules: &DossierRules) -> Vec<String> {
    let mut errors = Vec::new();
    let known: HashSet<_> = source.parts.iter().map(|p| p.id.as_str()).collect();
    let valid =
        |s: &str| known.contains(s) || s.starts_with("http://") || s.starts_with("https://");
    let cited = cited_sources(dossier, source);
    if cited.len() < rules.min_sources as usize {
        errors.push(format!(
            "dossier cites {} distinct sources (not counting the user file); needs {}. Cite every source id or URL that supports each beat and detail",
            cited.len(),
            rules.min_sources
        ));
    }
    if dossier.beats.len() < rules.min_beats as usize {
        errors.push(format!(
            "dossier has {} beats; needs at least {}. Break the story into smaller chronological events",
            dossier.beats.len(),
            rules.min_beats
        ));
    }
    let iconic = dossier
        .iconic
        .iter()
        .filter(|i| !i.sources.is_empty())
        .count();
    if iconic < rules.min_iconic as usize {
        errors.push(format!(
            "dossier has {iconic} sourced iconic elements and hidden details; needs {}",
            rules.min_iconic
        ));
    }
    let mut ids = HashSet::new();
    for b in &dossier.beats {
        if !ids.insert(b.id.as_str()) {
            errors.push(format!("beat id {} appears twice", b.id));
        }
        if b.text.trim().is_empty() || b.sources.is_empty() {
            errors.push(format!("beat {} needs text and at least one source", b.id));
        }
        if let Some(bad) = b.sources.iter().find(|s| !valid(s)) {
            errors.push(format!("beat {} cites unknown source `{bad}`", b.id));
        }
    }
    let mut iconic_ids = HashSet::new();
    for i in &dossier.iconic {
        if !iconic_ids.insert(i.id.as_str()) {
            errors.push(format!("iconic id {} appears twice", i.id));
        }
        if !ids.contains(i.setup_beat.as_str()) || !ids.contains(i.payoff_beat.as_str()) {
            errors.push(format!(
                "iconic {} must name existing setup and payoff beat ids",
                i.id
            ));
        }
        if let Some(bad) = i.sources.iter().find(|s| !valid(s)) {
            errors.push(format!("iconic {} cites unknown source `{bad}`", i.id));
        }
    }
    if dossier.work.title.trim().is_empty() || dossier.work.version_note.trim().is_empty() {
        errors.push("dossier must pin the exact work and version".into());
    }
    errors
}

#[must_use]
pub fn bible_gates(bible: &Bible, plan_limit: u32) -> Vec<String> {
    let mut errors = Vec::new();
    let mut names = HashSet::new();
    for c in &bible.characters {
        if !names.insert(c.name.to_lowercase()) {
            errors.push(format!("character {} appears twice", c.name));
        }
        if [&c.introduction, &c.want, &c.arc, &c.look]
            .iter()
            .any(|f| f.trim().is_empty())
            || c.emotions.is_empty()
        {
            errors.push(format!(
                "character {} needs an introduction, want, arc, look and emotions",
                c.name
            ));
        }
    }
    if bible.characters.is_empty() {
        errors.push("the bible needs at least one character".into());
    }
    if bible.characters.len() > (plan_limit as usize) * 2 {
        errors.push(format!(
            "profile only characters who matter (at most {})",
            plan_limit * 2
        ));
    }
    errors
}

/// Outline checks: every must beat is kept, ids exist, every section after the first (which
/// may only tease) has material.
#[must_use]
pub fn outline_gates(
    plan: &Plan,
    dossier: &Dossier,
    sections: &[String],
    min_iconic: usize,
) -> Vec<String> {
    let mut errors = Vec::new();
    let beat_ids: HashSet<_> = dossier.beats.iter().map(|b| b.id.as_str()).collect();
    let mut seen = HashSet::new();
    for o in &plan.outline {
        if !beat_ids.contains(o.id.as_str()) {
            errors.push(format!("outline names unknown beat {}", o.id));
        }
        if !seen.insert(o.id.as_str()) {
            errors.push(format!("outline lists beat {} twice", o.id));
        }
        if o.include && !sections.contains(&o.section) {
            errors.push(format!(
                "beat {} is assigned to unknown section `{}`",
                o.id, o.section
            ));
        }
        if !o.include && o.reason.trim().is_empty() {
            errors.push(format!("dropped beat {} needs a reason", o.id));
        }
    }
    for b in &dossier.beats {
        let kept = plan.outline.iter().any(|o| o.id == b.id && o.include);
        if b.importance == Importance::Must && !kept {
            errors.push(format!("must beat {} ({}) is not included", b.id, b.text));
        }
        if !seen.contains(b.id.as_str()) {
            errors.push(format!(
                "outline must decide on every beat; {} is missing",
                b.id
            ));
        }
    }
    for s in sections.iter().skip(1) {
        if !plan.outline.iter().any(|o| o.include && &o.section == s) {
            errors.push(format!("section `{s}` has no beats"));
        }
    }
    let iconic_ids: HashSet<_> = dossier.iconic.iter().map(|i| i.id.as_str()).collect();
    if let Some(bad) = plan
        .iconic
        .iter()
        .find(|i| !iconic_ids.contains(i.as_str()))
    {
        errors.push(format!("plan chooses unknown iconic element {bad}"));
    }
    if plan.iconic.len() < min_iconic.min(dossier.iconic.len()) {
        errors.push(format!(
            "choose at least {} iconic elements",
            min_iconic.min(dossier.iconic.len())
        ));
    }
    errors
}

fn normal(text: &str) -> String {
    text.to_lowercase()
        .replace(['’', '‘'], "'")
        .replace(['“', '”'], "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turns the coverage report into violations, trusting only lines that really occur in the
/// script, and checking that each setup comes no later than its payoff.
#[must_use]
pub fn coverage_violations(
    coverage: &Coverage,
    plan: &Plan,
    dossier: &Dossier,
    narration: &str,
) -> Vec<Violation> {
    let script = normal(narration);
    let at = |line: &str| {
        let line = normal(line);
        if line.len() < 8 {
            None
        } else {
            script.find(line.trim_end_matches(['.', '!', '?', ',']))
        }
    };
    let mut out = Vec::new();
    for o in plan.outline.iter().filter(|o| o.include) {
        let found = coverage
            .beats
            .iter()
            .find(|b| b.id == o.id)
            .and_then(|b| at(&b.line));
        if found.is_none() {
            let text = dossier
                .beats
                .iter()
                .find(|b| b.id == o.id)
                .map_or("", |b| b.text.as_str());
            out.push(Violation {
                gate: "coverage".into(),
                message: format!(
                    "beat {} is missing from section `{}`: {text}. Add it in story order, concretely, in one or two short lines",
                    o.id, o.section
                ),
            });
        }
    }
    for id in &plan.iconic {
        let Some(item) = dossier.iconic.iter().find(|i| &i.id == id) else {
            continue;
        };
        let report = coverage.iconic.iter().find(|c| &c.id == id);
        let setup = report.and_then(|c| at(&c.setup_line));
        let payoff = report.and_then(|c| at(&c.payoff_line));
        match (setup, payoff) {
            (Some(s), Some(p)) if s <= p => {}
            (Some(_), Some(_)) => out.push(Violation {
                gate: "iconic".into(),
                message: format!(
                    "{}: the payoff comes before its setup: {}",
                    item.id, item.text
                ),
            }),
            _ => out.push(Violation {
                gate: "iconic".into(),
                message: format!(
                    "{} is not both set up and paid off: {}{}",
                    item.id,
                    item.text,
                    if item.quote.is_empty() {
                        String::new()
                    } else {
                        format!(" (line: \"{}\")", item.quote)
                    }
                ),
            }),
        }
    }
    for name in &coverage.unintroduced {
        out.push(Violation {
            gate: "characters".into(),
            message: format!(
                "{name} is named without saying who they are; introduce them at first mention"
            ),
        });
    }
    for item in &coverage.foreign {
        out.push(Violation {
            gate: "version".into(),
            message: format!(
                "`{item}` belongs to another version, not {}; remove or replace it",
                dossier.work.version_note
            ),
        });
    }
    out
}

/// A readable copy for the review page and for people.
#[must_use]
pub fn markdown(dossier: &Dossier, bible: Option<&Bible>) -> String {
    use std::fmt::Write;
    let mut md = format!(
        "# {}\n\n{} · {} · {} · {}\n\n{}\n\n> {}\n\n## Beats\n\n",
        dossier.work.title,
        dossier.work.medium,
        dossier.work.year,
        dossier.work.language,
        dossier.work.creator,
        dossier.work.version_note,
        dossier.logline
    );
    for b in &dossier.beats {
        let _ = writeln!(
            md,
            "- **{}** ({:?}) {} — {}",
            b.id,
            b.importance,
            b.text,
            b.sources.join(", ")
        );
    }
    md.push_str("\n## Iconic elements and hidden details\n\n");
    for i in &dossier.iconic {
        let _ = writeln!(
            md,
            "- **{}** ({:?}) {}{} — set up {}, pays off {} — {}",
            i.id,
            i.kind,
            i.text,
            if i.quote.is_empty() {
                String::new()
            } else {
                format!(" \"{}\"", i.quote)
            },
            i.setup_beat,
            i.payoff_beat,
            i.sources.join(", ")
        );
    }
    if let Some(bible) = bible {
        md.push_str("\n## Characters\n\n");
        for c in &bible.characters {
            let _ = writeln!(
                md,
                "### {} ({})\n\n{}\n\n- Wants: {}\n- Flaw: {}\n- Arc: {}\n- Signature: {} {}\n- Look: {}\n- Emotions: {}\n",
                c.name,
                c.role,
                c.introduction,
                c.want,
                c.flaw,
                c.arc,
                c.signature_line,
                c.signature_object,
                c.look,
                c.emotions.join(", ")
            );
        }
    }
    md.push_str("\n## Themes\n\n");
    for t in &dossier.themes {
        let _ = writeln!(md, "- {t}");
    }
    if !dossier.conflicts.is_empty() {
        md.push_str("\n## Source conflicts\n\n");
        for c in &dossier.conflicts {
            let _ = writeln!(md, "- {c}");
        }
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::SourcePart;
    use crate::story::OutlineBeat;

    fn source() -> Source {
        let part = |id: &str, kind: &str| SourcePart {
            id: id.into(),
            kind: kind.into(),
            title: id.into(),
            url: None,
            text: "text".into(),
        };
        Source::from_parts(
            "Work",
            vec![
                part("S1", "file"),
                part("S2", "wikipedia"),
                part("S3", "web"),
            ],
        )
    }

    fn beat(id: &str, importance: Importance, sources: &[&str]) -> DossierBeat {
        DossierBeat {
            id: id.into(),
            text: format!("event {id}"),
            characters: vec![],
            importance,
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn dossier() -> Dossier {
        Dossier {
            work: WorkPin {
                title: "Work".into(),
                medium: "novel".into(),
                year: "1977".into(),
                language: "English".into(),
                creator: "Author".into(),
                version_note: "the 1977 novel, not the film".into(),
            },
            logline: "A story.".into(),
            beats: vec![
                beat("B01", Importance::Must, &["S2"]),
                beat("B02", Importance::Should, &["https://a.example/x"]),
                beat("B03", Importance::Could, &["S1"]),
            ],
            iconic: vec![Iconic {
                id: "I01".into(),
                kind: FactKind::Object,
                text: "the boiler".into(),
                quote: String::new(),
                setup_beat: "B01".into(),
                payoff_beat: "B02".into(),
                sources: vec!["S2".into()],
            }],
            settings: vec![],
            pronunciations: vec![],
            themes: vec![],
            conflicts: vec![],
        }
    }

    fn plan(outline: Vec<OutlineBeat>) -> Plan {
        Plan {
            settings: vec![],
            props: vec![],
            iconic_images: vec![],
            storyline: "s".into(),
            characters: vec![],
            tone: "t".into(),
            open_loop: String::new(),
            emotional_peak: String::new(),
            omitted: vec![],
            beats: vec![],
            outline,
            iconic: vec!["I01".into()],
            cast: vec![],
        }
    }

    fn kept(id: &str, section: &str) -> OutlineBeat {
        OutlineBeat {
            id: id.into(),
            section: section.into(),
            include: true,
            reason: String::new(),
        }
    }

    #[test]
    fn dossier_counts_web_urls_and_sources_but_not_the_user_file() {
        let rules = DossierRules {
            min_sources: 2,
            min_beats: 3,
            min_iconic: 1,
        };
        assert_eq!(
            dossier_gates(&dossier(), &source(), &rules),
            Vec::<String>::new()
        );
        let strict = DossierRules {
            min_sources: 3,
            ..rules
        };
        let problems = dossier_gates(&dossier(), &source(), &strict);
        assert!(problems[0].contains("2 distinct sources"), "{problems:?}");
        let mut bad = dossier();
        bad.beats[0].sources = vec!["S9".into()];
        bad.iconic[0].payoff_beat = "B99".into();
        let problems = dossier_gates(&bad, &source(), &rules);
        assert!(problems.iter().any(|p| p.contains("unknown source `S9`")));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("I01 must name existing"))
        );
    }

    #[test]
    fn outline_keeps_every_must_beat_and_explains_drops() {
        let sections = vec!["hook".to_owned(), "story".to_owned()];
        let good = plan(vec![
            kept("B01", "story"),
            kept("B02", "story"),
            OutlineBeat {
                id: "B03".into(),
                section: "story".into(),
                include: false,
                reason: "texture".into(),
            },
        ]);
        assert_eq!(
            outline_gates(&good, &dossier(), &sections, 1),
            Vec::<String>::new()
        );
        let bad = plan(vec![
            OutlineBeat {
                id: "B01".into(),
                section: "story".into(),
                include: false,
                reason: String::new(),
            },
            kept("B02", "nowhere"),
        ]);
        let problems = outline_gates(&bad, &dossier(), &sections, 1);
        for needle in [
            "must beat B01",
            "B01 needs a reason",
            "unknown section `nowhere`",
            "B03 is missing",
            "section `story` has no beats",
        ] {
            assert!(
                problems.iter().any(|p| p.contains(needle)),
                "{needle}: {problems:?}"
            );
        }
    }

    #[test]
    fn coverage_trusts_only_lines_in_the_script_and_setup_before_payoff() {
        let plan = plan(vec![kept("B01", "story"), kept("B02", "story")]);
        let narration =
            "Jack finds the old boiler in the basement. Later, the boiler explodes into flames.";
        let report = |b1: &str, setup: &str, payoff: &str| Coverage {
            beats: vec![
                BeatCoverage {
                    id: "B01".into(),
                    line: b1.into(),
                },
                BeatCoverage {
                    id: "B02".into(),
                    line: "the boiler explodes into flames.".into(),
                },
            ],
            iconic: vec![IconicCoverage {
                id: "I01".into(),
                setup_line: setup.into(),
                payoff_line: payoff.into(),
            }],
            unintroduced: vec![],
            foreign: vec![],
        };
        let ok = report(
            "Jack finds the old boiler in the basement.",
            "Jack finds the old boiler in the basement.",
            "Later, the boiler explodes into flames.",
        );
        assert!(coverage_violations(&ok, &plan, &dossier(), narration).is_empty());
        let invented = report(
            "Jack fixes the radio himself.",
            "Later, the boiler explodes into flames.",
            "Jack finds the old boiler in the basement.",
        );
        let v = coverage_violations(&invented, &plan, &dossier(), narration);
        assert!(
            v.iter()
                .any(|v| v.gate == "coverage" && v.message.contains("B01"))
        );
        assert!(
            v.iter()
                .any(|v| v.gate == "iconic" && v.message.contains("before its setup"))
        );
    }

    #[test]
    fn schemas_are_accepted_by_strict_structured_output() {
        for schema in [
            retold_providers::text::strict_schema::<Dossier>(),
            retold_providers::text::strict_schema::<Bible>(),
            retold_providers::text::strict_schema::<Coverage>(),
            retold_providers::text::strict_schema::<WebNotes>(),
            retold_providers::text::strict_schema::<Extract>(),
            retold_providers::text::strict_schema::<Plan>(),
        ] {
            let text = schema.to_string();
            assert!(!text.contains("oneOf") && !text.contains("anyOf"), "{text}");
        }
    }

    #[test]
    fn long_parts_are_chunked_and_short_parts_kept_whole() {
        let mut big = source();
        big.parts[1].text = "word "
            .repeat(CHUNK_CHARS / 5 * 2)
            .replace("word word ", "word word\n\n");
        let (long, short) = chunks(&big);
        assert!(long.len() >= 2 && long.iter().all(|(id, _)| id.starts_with("S2-")));
        assert_eq!(short.len(), 2);
    }
}
