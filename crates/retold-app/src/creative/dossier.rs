//! Story research dossier, character bible validation, and adaptation coverage analysis.

use retold_engine::dossier::{
    Bible, Coverage, Dossier, DossierBeat, Iconic, Importance, Place, Profile, WorkPin,
};
use retold_engine::dossier::{bible_gates, coverage_violations, dossier_gates};
use retold_engine::research::{Source, SourcePart};
use retold_engine::retold_guide::manifest::DossierRules;
use retold_engine::story::{OutlineBeat, Plan, Violation};

use super::types::{
    BibleAuditReport, BibleCharacterItem, DossierAuditReport, ScriptCoverageAuditReport,
};

pub const DEFAULT_PLAN_LIMIT: u32 = 4;
pub const BENCHMARK_MIN_SOURCES: u32 = 2;
pub const BENCHMARK_MIN_BEATS: u32 = 4;
pub const BENCHMARK_MIN_ICONIC: u32 = 2;

struct ImportanceCounts {
    must: usize,
    should: usize,
    could: usize,
}

impl ImportanceCounts {
    const fn must(&self) -> usize {
        self.must
    }

    const fn should(&self) -> usize {
        self.should
    }

    const fn could(&self) -> usize {
        self.could
    }
}

fn count_importance_levels(beats: &[DossierBeat]) -> ImportanceCounts {
    let mut counts = ImportanceCounts {
        must: 0,
        should: 0,
        could: 0,
    };
    for beat in beats {
        match beat.importance {
            Importance::Must => counts.must += 1,
            Importance::Should => counts.should += 1,
            Importance::Could => counts.could += 1,
        }
    }
    counts
}

fn extract_work_meta(work: &WorkPin) -> (String, String, String, String) {
    (
        work.title.clone(),
        work.medium.clone(),
        work.year.clone(),
        work.version_note.clone(),
    )
}

const fn count_dossier_beats(dossier: &Dossier) -> usize {
    dossier.beats.len()
}

const fn count_dossier_iconic(dossier: &Dossier) -> usize {
    dossier.iconic.len()
}

const fn count_dossier_settings(dossier: &Dossier) -> usize {
    dossier.settings.len()
}

fn build_dossier_report(
    dossier: &Dossier,
    counts: &ImportanceCounts,
    gate_errors: Vec<String>,
) -> DossierAuditReport {
    let (work_title, work_medium, work_year, version_note) = extract_work_meta(&dossier.work);
    let passes_gates = gate_errors.is_empty();
    let total_beats = count_dossier_beats(dossier);
    let iconic_count = count_dossier_iconic(dossier);
    let settings_count = count_dossier_settings(dossier);

    DossierAuditReport {
        work_title,
        work_medium,
        work_year,
        version_note,
        logline: dossier.logline.clone(),
        total_beats,
        must_beats: counts.must(),
        should_beats: counts.should(),
        could_beats: counts.could(),
        iconic_count,
        settings_count,
        themes: dossier.themes.clone(),
        conflicts: dossier.conflicts.clone(),
        passes_gates,
        gate_errors,
    }
}

#[must_use]
pub fn audit_dossier(
    dossier: &Dossier,
    source: &Source,
    rules: &DossierRules,
) -> DossierAuditReport {
    let counts = count_importance_levels(&dossier.beats);
    let gate_errors = dossier_gates(dossier, source, rules);
    build_dossier_report(dossier, &counts, gate_errors)
}

fn map_character_item(character: &Profile) -> BibleCharacterItem {
    BibleCharacterItem {
        name: character.name.clone(),
        role: character.role.clone(),
        introduction: character.introduction.clone(),
        want: character.want.clone(),
        flaw: character.flaw.clone(),
        arc: character.arc.clone(),
        look: character.look.clone(),
        emotions: character.emotions.clone(),
        signature_prop: character.signature_object.clone(),
        signature_line: character.signature_line.clone(),
    }
}

#[must_use]
pub fn audit_bible(bible: &Bible, plan_limit: u32) -> BibleAuditReport {
    let characters: Vec<BibleCharacterItem> =
        bible.characters.iter().map(map_character_item).collect();
    let gate_errors = bible_gates(bible, plan_limit);
    let passes_gates = gate_errors.is_empty();

    BibleAuditReport {
        total_characters: characters.len(),
        characters,
        passes_gates,
        gate_errors,
    }
}

#[derive(Debug)]
pub struct CoverageContext<'a> {
    pub coverage: &'a Coverage,
    pub plan: &'a Plan,
    pub dossier: &'a Dossier,
    pub narration: &'a str,
}

fn filter_violations_by_gate(violations: &[Violation], gate_name: &str) -> Vec<String> {
    violations
        .iter()
        .filter(|violation| violation.gate == gate_name)
        .map(|violation| violation.message.clone())
        .collect()
}

fn filter_iconic_violations(violations: &[Violation]) -> Vec<String> {
    violations
        .iter()
        .filter(|violation| {
            let is_setup = violation.gate == "iconic_setup";
            let is_payoff = violation.gate == "iconic_payoff";
            is_setup || is_payoff
        })
        .map(|violation| violation.message.clone())
        .collect()
}

fn count_outline_beats(plan: &Plan) -> usize {
    plan.outline
        .iter()
        .filter(|outline_beat| outline_beat.include)
        .count()
}

const fn count_iconic_targets(plan: &Plan) -> usize {
    plan.iconic.len()
}

const fn compute_coverage_difference(total: usize, missing: usize) -> usize {
    total.saturating_sub(missing)
}

const fn check_coverage_gates(violations: &[Violation], coverage: &Coverage) -> bool {
    violations.is_empty() && coverage.unintroduced.is_empty() && coverage.foreign.is_empty()
}

struct CoverageMetrics {
    outline_beats: usize,
    missing_beats: Vec<String>,
    missing_iconic: Vec<String>,
    covered_beats: usize,
    covered_iconic: usize,
    passes_gates: bool,
}

fn compute_coverage_metrics(
    plan: &Plan,
    coverage: &Coverage,
    violations: &[Violation],
) -> CoverageMetrics {
    let outline_beats = count_outline_beats(plan);
    let missing_beats = filter_violations_by_gate(violations, "coverage");
    let missing_iconic = filter_iconic_violations(violations);
    let covered_beats = compute_coverage_difference(outline_beats, missing_beats.len());
    let total_iconic = count_iconic_targets(plan);
    let covered_iconic = compute_coverage_difference(total_iconic, missing_iconic.len());
    let passes_gates = check_coverage_gates(violations, coverage);

    CoverageMetrics {
        outline_beats,
        missing_beats,
        missing_iconic,
        covered_beats,
        covered_iconic,
        passes_gates,
    }
}

fn assemble_coverage_report(
    metrics: CoverageMetrics,
    coverage: &Coverage,
) -> ScriptCoverageAuditReport {
    ScriptCoverageAuditReport {
        total_outline_beats: metrics.outline_beats,
        covered_beats: metrics.covered_beats,
        missing_beats: metrics.missing_beats,
        covered_iconic: metrics.covered_iconic,
        missing_iconic: metrics.missing_iconic,
        unintroduced_characters: coverage.unintroduced.clone(),
        foreign_elements: coverage.foreign.clone(),
        passes_gates: metrics.passes_gates,
    }
}

#[must_use]
pub fn audit_script_coverage(context: &CoverageContext<'_>) -> ScriptCoverageAuditReport {
    let violations = coverage_violations(
        context.coverage,
        context.plan,
        context.dossier,
        context.narration,
    );
    let metrics = compute_coverage_metrics(context.plan, context.coverage, &violations);
    assemble_coverage_report(metrics, context.coverage)
}

fn benchmark_source_parts() -> Vec<SourcePart> {
    vec![
        SourcePart {
            id: "S1".into(),
            kind: "wikipedia".into(),
            title: "The Count of Monte Cristo (novel)".into(),
            url: Some("https://en.wikipedia.org/wiki/The_Count_of_Monte_Cristo".into()),
            text: "Edmond Dantes is falsely accused of treason by jealous rivals...".into(),
        },
        SourcePart {
            id: "S2".into(),
            kind: "gutenberg".into(),
            title: "The Count of Monte Cristo full text".into(),
            url: Some("https://www.gutenberg.org/ebooks/1184".into()),
            text:
                "On the 24th of February, 1815, the look-out at Notre-Dame de la Garde signaled..."
                    .into(),
        },
    ]
}

fn benchmark_work_pin() -> WorkPin {
    WorkPin {
        title: "The Count of Monte Cristo".into(),
        medium: "novel".into(),
        year: "1844".into(),
        language: "fr".into(),
        creator: "Alexandre Dumas".into(),
        version_note: "Canonical 1844 unabridged French serialized novel; strictly excludes 2002 film alterations.".into(),
    }
}

fn benchmark_early_beats() -> Vec<DossierBeat> {
    vec![
        DossierBeat {
            id: "B01".into(),
            text: "Edmond Dantes is wrongfully arrested at his betrothal feast.".into(),
            characters: vec!["Edmond Dantes".into(), "Fernand Mondego".into()],
            importance: Importance::Must,
            sources: vec!["S1".into(), "S2".into()],
        },
        DossierBeat {
            id: "B02".into(),
            text: "Dantes is cast into the subterranean dungeon of Chateau d'If.".into(),
            characters: vec!["Edmond Dantes".into()],
            importance: Importance::Must,
            sources: vec!["S1".into()],
        },
    ]
}

fn benchmark_late_beats() -> Vec<DossierBeat> {
    vec![
        DossierBeat {
            id: "B03".into(),
            text: "Abbe Faria reveals the buried Spada treasure on the isle of Monte Cristo."
                .into(),
            characters: vec!["Edmond Dantes".into(), "Abbe Faria".into()],
            importance: Importance::Must,
            sources: vec!["S1".into(), "S2".into()],
        },
        DossierBeat {
            id: "B04".into(),
            text: "Dantes sews himself into Faria's burial shroud and plunges into the sea.".into(),
            characters: vec!["Edmond Dantes".into()],
            importance: Importance::Must,
            sources: vec!["S1".into()],
        },
    ]
}

fn benchmark_dossier_beats() -> Vec<DossierBeat> {
    let mut beats = benchmark_early_beats();
    beats.extend(benchmark_late_beats());
    beats
}

fn benchmark_dossier_iconic() -> Vec<Iconic> {
    vec![
        Iconic {
            id: "I01".into(),
            kind: retold_engine::dossier::FactKind::Object,
            text: "Coarse burial sack and knife used to slice open the shroud under water.".into(),
            quote: String::new(),
            setup_beat: "B03".into(),
            payoff_beat: "B04".into(),
            sources: vec!["S1".into()],
        },
        Iconic {
            id: "I02".into(),
            kind: retold_engine::dossier::FactKind::Symbol,
            text: "The Spada family testament written in invisible ink revealed by flame.".into(),
            quote: String::new(),
            setup_beat: "B02".into(),
            payoff_beat: "B03".into(),
            sources: vec!["S2".into()],
        },
    ]
}

fn benchmark_dossier_settings() -> Vec<Place> {
    vec![
        Place {
            name: "Chateau d'If".into(),
            description: "Grim fortress dungeon carved from limestone off the coast of Marseille."
                .into(),
        },
        Place {
            name: "Isle of Monte Cristo".into(),
            description: "Barren granite island harboring hidden sea caves and ancient treasure."
                .into(),
        },
    ]
}

#[must_use]
pub fn benchmark_dossier() -> (Dossier, Source, DossierRules) {
    let dossier = Dossier {
        work: benchmark_work_pin(),
        logline: "A betrayed sailor escapes a fortress prison to exact measured vengeance.".into(),
        beats: benchmark_dossier_beats(),
        iconic: benchmark_dossier_iconic(),
        settings: benchmark_dossier_settings(),
        themes: vec!["justice vs retribution".into(), "patience and hope".into()],
        conflicts: vec!["No duel reconciliation in 1844 canon unlike later cinema".into()],
        pronunciations: Vec::new(),
    };

    let source = Source::from_parts("The Count of Monte Cristo", benchmark_source_parts());
    let rules = DossierRules {
        min_sources: BENCHMARK_MIN_SOURCES,
        min_beats: BENCHMARK_MIN_BEATS,
        min_iconic: BENCHMARK_MIN_ICONIC,
    };

    (dossier, source, rules)
}

fn benchmark_edmond_profile() -> Profile {
    Profile {
        name: "Edmond Dantes".into(),
        role: "protagonist".into(),
        introduction:
            "A candid nineteen-year-old first mate transformed into an omniscient avenger.".into(),
        want: "Escape imprisonment and uncover the conspirators who ruined his life.".into(),
        flaw: "Hubris in appointing himself God's emissary of cosmic retribution.".into(),
        arc: "From naive innocence through cold vengeance to redemptive humility.".into(),
        turning_points: vec![
            "Betrayal at wedding".into(),
            "Faria's death".into(),
            "Arrival in Paris".into(),
        ],
        signature_line: "Wait and hope.".into(),
        signature_object: "The emerald pillbox containing hasheesh.".into(),
        relationships: vec![
            "Fernand Mondego (rival)".into(),
            "Abbe Faria (mentor)".into(),
        ],
        look: "Tall, slender, pale olive complexion, dark piercing eyes, somber dark attire."
            .into(),
        emotions: vec![
            "calm".into(),
            "steely".into(),
            "anguished".into(),
            "triumphant".into(),
        ],
    }
}

fn benchmark_fernand_profile() -> Profile {
    Profile {
        name: "Fernand Mondego".into(),
        role: "antagonist".into(),
        introduction: "A Catalan fisherman consumed by jealous obsession who betrays his comrade."
            .into(),
        want: "Possess Mercedes at all costs and elevate himself into French nobility.".into(),
        flaw: "Cowardly treachery masked behind decorated military rank.".into(),
        arc: "Ascends to Count de Morcerf through treason before total public disgrace.".into(),
        turning_points: vec![
            "Mailing the denunciation letter".into(),
            "Ali Pasha betrayal".into(),
        ],
        signature_line: "Mercedes must be mine.".into(),
        signature_object: "The forged denunciation letter.".into(),
        relationships: vec![
            "Edmond Dantes (victim)".into(),
            "Mercedes (coveted wife)".into(),
        ],
        look: "Broad-shouldered, sun-baked Catalan features, arrogant military uniform.".into(),
        emotions: vec!["arrogant".into(), "panicked".into(), "desperate".into()],
    }
}

#[must_use]
pub fn benchmark_bible() -> Bible {
    Bible {
        characters: vec![benchmark_edmond_profile(), benchmark_fernand_profile()],
    }
}

fn benchmark_outline_beats() -> Vec<OutlineBeat> {
    vec![
        OutlineBeat {
            id: "B01".into(),
            section: "hook".into(),
            include: true,
            reason: String::new(),
        },
        OutlineBeat {
            id: "B02".into(),
            section: "rising_action".into(),
            include: true,
            reason: String::new(),
        },
        OutlineBeat {
            id: "B03".into(),
            section: "rising_action".into(),
            include: true,
            reason: String::new(),
        },
        OutlineBeat {
            id: "B04".into(),
            section: "climax".into(),
            include: true,
            reason: String::new(),
        },
    ]
}

#[must_use]
pub fn benchmark_plan_outline() -> Plan {
    Plan {
        storyline: "The betrayal and escape of Edmond Dantes.".into(),
        characters: vec!["Edmond Dantes".into(), "Fernand Mondego".into()],
        tone: "epic drama".into(),
        open_loop: "Will Dantes survive the subterranean sea plunge?".into(),
        emotional_peak: "The shroud knife slash in the dark waters.".into(),
        omitted: Vec::new(),
        beats: Vec::new(),
        outline: benchmark_outline_beats(),
        iconic: vec!["I01".into(), "I02".into()],
        cast: Vec::new(),
        settings: Vec::new(),
        props: Vec::new(),
        iconic_images: Vec::new(),
    }
}

pub const BENCHMARK_NARRATION: &str = "Edmond Dantes is wrongfully arrested at his betrothal feast. Dantes is cast into the subterranean dungeon of Chateau d'If. Abbe Faria reveals the buried Spada treasure on the isle of Monte Cristo. Dantes sews himself into Faria's burial shroud and plunges into the sea with the coarse burial sack.";

fn benchmark_coverage_beats() -> Vec<retold_engine::dossier::BeatCoverage> {
    vec![
        retold_engine::dossier::BeatCoverage {
            id: "B01".into(),
            line: "Edmond Dantes is wrongfully arrested at his betrothal feast.".into(),
        },
        retold_engine::dossier::BeatCoverage {
            id: "B02".into(),
            line: "Dantes is cast into the subterranean dungeon of Chateau d'If.".into(),
        },
        retold_engine::dossier::BeatCoverage {
            id: "B03".into(),
            line: "Abbe Faria reveals the buried Spada treasure on the isle of Monte Cristo.".into(),
        },
        retold_engine::dossier::BeatCoverage {
            id: "B04".into(),
            line: "Dantes sews himself into Faria's burial shroud and plunges into the sea with the coarse burial sack.".into(),
        },
    ]
}

fn benchmark_coverage_iconic() -> Vec<retold_engine::dossier::IconicCoverage> {
    vec![
        retold_engine::dossier::IconicCoverage {
            id: "I01".into(),
            setup_line: "Abbe Faria reveals the buried Spada treasure on the isle of Monte Cristo.".into(),
            payoff_line: "Dantes sews himself into Faria's burial shroud and plunges into the sea with the coarse burial sack.".into(),
        },
        retold_engine::dossier::IconicCoverage {
            id: "I02".into(),
            setup_line: "Dantes is cast into the subterranean dungeon of Chateau d'If.".into(),
            payoff_line: "Abbe Faria reveals the buried Spada treasure on the isle of Monte Cristo.".into(),
        },
    ]
}

#[must_use]
pub fn benchmark_coverage() -> (Coverage, Plan, Dossier, &'static str) {
    let (dossier, _source, _rules) = benchmark_dossier();
    let plan = benchmark_plan_outline();
    let coverage = Coverage {
        beats: benchmark_coverage_beats(),
        iconic: benchmark_coverage_iconic(),
        unintroduced: Vec::new(),
        foreign: Vec::new(),
    };
    (coverage, plan, dossier, BENCHMARK_NARRATION)
}
