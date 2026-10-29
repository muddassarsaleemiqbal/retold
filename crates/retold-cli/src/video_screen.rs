//! Projections of a `video plan` result for the video screen: stages in pipeline order, a
//! composite status, blockers with honest next actions (product spec §4.3–§4.4) and the plan.
//! Pure functions of the plan data; nothing here runs anything.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::human::{amount, text, usd};

/// One stage: its saved evidence, what the plan decided and whether this plan would run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRow {
    pub stage: String,
    pub recorded: u64,
    pub verified: u64,
    pub unresolved: u64,
    pub decision: &'static str,
    pub planned: bool,
}

/// A blocker as the user should read it: what happened, what it affects, what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    pub what: String,
    pub affects: String,
    pub actions: Vec<String>,
}

const DECISIONS: [(&str, &str); 3] = [
    ("reused_from_cache", "saved"),
    ("requires_repair", "needs repair"),
    ("requires_execution_preflight", "not run yet"),
];

/// Stages ordered so each comes after everything it depends on; ties stay alphabetical.
pub fn stages(plan: &Value) -> Vec<StageRow> {
    let rows: BTreeMap<String, (StageRow, BTreeSet<String>)> = elements(&plan["stages"])
        .iter()
        .map(|stage| {
            (
                text(stage, "stage"),
                (row(plan, stage), dependencies(stage)),
            )
        })
        .collect();
    pipeline(rows)
}

fn row(plan: &Value, stage: &Value) -> StageRow {
    let name = text(stage, "stage");
    let decision = text(stage, "decision");
    let label = DECISIONS
        .iter()
        .find(|(code, _)| *code == decision)
        .map_or("unknown", |(_, label)| *label);
    let planned = elements(&plan["authority"]["stages"])
        .iter()
        .any(|planned| planned.as_str() == Some(name.as_str()));
    StageRow {
        stage: name,
        recorded: amount(&stage["recorded_steps"]),
        verified: amount(&stage["envelope_verified"]),
        unresolved: amount(&stage["unresolved_records"]),
        decision: label,
        planned,
    }
}

fn dependencies(stage: &Value) -> BTreeSet<String> {
    elements(&stage["dependencies"])
        .iter()
        .map(|dependency| text(dependency, ""))
        .collect()
}

/// Repeatedly takes every stage whose dependencies are placed; a cycle keeps its leftovers last.
fn next_pipeline_batch(
    pending: &BTreeMap<String, (StageRow, BTreeSet<String>)>,
    placed: &BTreeSet<String>,
) -> Vec<String> {
    let ready: Vec<String> = pending
        .iter()
        .filter(|(_, (_, needs))| {
            needs
                .iter()
                .all(|need| placed.contains(need) || !pending.contains_key(need))
        })
        .map(|(name, _)| name.clone())
        .collect();
    if ready.is_empty() {
        pending.keys().cloned().collect()
    } else {
        ready
    }
}

struct PipelineBuilder {
    pending: BTreeMap<String, (StageRow, BTreeSet<String>)>,
    placed: BTreeSet<String>,
    ordered: Vec<StageRow>,
}

impl PipelineBuilder {
    const fn new(pending: BTreeMap<String, (StageRow, BTreeSet<String>)>) -> Self {
        Self {
            pending,
            placed: BTreeSet::new(),
            ordered: Vec::new(),
        }
    }

    fn step(&mut self, name: &str) {
        let removed = self.pending.remove(name);
        if let Some((row, _)) = removed {
            self.ordered.push(row);
        }
        self.placed.insert(name.to_owned());
    }

    fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn build(mut self) -> Vec<StageRow> {
        while self.has_pending() {
            let next = next_pipeline_batch(&self.pending, &self.placed);
            for name in next {
                self.step(&name);
            }
        }
        self.ordered
    }
}

fn pipeline(pending: BTreeMap<String, (StageRow, BTreeSet<String>)>) -> Vec<StageRow> {
    PipelineBuilder::new(pending).build()
}

/// The video's identity lines.
pub fn identity(plan: &Value) -> Vec<String> {
    let video = &plan["snapshot"]["video"];
    let editions: Vec<String> = elements(&video["editions"])
        .iter()
        .map(|edition| text(edition, ""))
        .collect();
    vec![
        format!("{} · {}", text(video, "ref"), text(video, "title")),
        format!(
            "Channel {} · guide {} · editions {} · ID {}",
            text(video, "channel"),
            text(video, "guide"),
            editions.join(", "),
            text(video, "id")
        ),
    ]
}

/// Run state and plan state as separate facts, with the one most needing attention first.
fn active_tasks_fact(recorded: &Value) -> Option<String> {
    let active = elements(&recorded["active_tasks"]).len();
    (active > 0).then(|| {
        format!(
            "{active} tasks were active when last saved; another process may still own this run"
        )
    })
}

fn recorded_failure_fact(recorded: &Value) -> Option<String> {
    let failed = recorded["recorded_failure"].as_bool();
    (failed == Some(true)).then(|| "the last run recorded a failure".into())
}

fn recorded_facts(recorded: &Value) -> Vec<String> {
    let mut facts = Vec::new();
    facts.extend(active_tasks_fact(recorded));
    facts.extend(recorded_failure_fact(recorded));
    facts
}

pub fn status(plan: &Value) -> String {
    let mut facts = recorded_facts(&plan["snapshot"]["recorded"]);
    facts.push(plan_state(plan));
    facts.join(" · ")
}

fn plan_state(plan: &Value) -> String {
    let blocked = elements(&plan["blockers"]).len();
    let to_run = stages(plan)
        .iter()
        .filter(|row| row.planned && row.decision != "saved")
        .count();
    let all_cached = plan["all_cached"].as_bool();
    if blocked > 0 {
        format!("needs attention: {blocked} blocker(s) before continuing")
    } else if all_cached == Some(true) {
        "everything planned is saved: 0 steps · $0".into()
    } else {
        format!("ready to continue: {to_run} planned stage(s) without saved results")
    }
}

/// Every blocker with its consequence and at least one honest action.
pub fn blockers(plan: &Value) -> Vec<Blocker> {
    let reference = text(&plan["snapshot"]["video"], "ref");
    elements(&plan["blockers"])
        .iter()
        .map(|blocker| explain(blocker, &reference))
        .collect()
}

fn explain(blocker: &Value, reference: &str) -> Blocker {
    let what = text(blocker, "message");
    let replan = format!("retold video plan {reference}");
    match text(blocker, "code").as_str() {
        "RG-ACCOUNT-REQUIRED" => account(blocker, what, &replan),
        "RG-NOT-READY" => Blocker {
            what,
            affects: "Export only; saved work and continuing to ready are unaffected.".into(),
            actions: vec![replan],
        },
        code => Blocker {
            what,
            affects: "This plan cannot start until the named input is fixed.".into(),
            actions: vec![format!("retold explain {code}"), replan],
        },
    }
}

/// Missing limits name the exact `account limit` command for each scope.
fn account(blocker: &Value, what: String, replan: &str) -> Blocker {
    let missing = elements(&blocker["context"]["missing_limits"]);
    let mut actions: Vec<String> = missing.iter().map(limit_command).collect();
    if missing.is_empty() {
        actions.push("retold account init".into());
    }
    actions.push(format!(
        "{replan}   (or plan with --spend 0 for free work only)"
    ));
    Blocker {
        what,
        affects: "Paid stages in this plan. Saved results stay usable.".into(),
        actions,
    }
}

fn limit_command(missing: &Value) -> String {
    let scope = &missing["scope"];
    let kind = text(scope, "kind");
    let has_id = !scope["id"].is_null();
    let scope_target = if has_id {
        format!("{kind}:{}", text(scope, "id"))
    } else {
        kind
    };
    format!(
        "retold account limit {scope_target} {} --usd AMOUNT --requests COUNT",
        text(missing, "window")
    )
}

/// What `continue` would be allowed to do, and the exact command to do it.
pub fn plan_lines(plan: &Value) -> Vec<String> {
    let authority = &plan["authority"];
    let routes = authority["routes"]
        .as_object()
        .map_or(0, serde_json::Map::len);
    let next: Vec<String> = elements(&plan["next"])
        .iter()
        .map(|word| text(word, ""))
        .collect();
    let estimate = plan["estimates"]["usd_micros"]
        .as_u64()
        .map_or_else(|| "unknown; the ceilings bound it".into(), usd);
    vec![
        format!(
            "Up to {} · spend ≤ {} · {} paid requests · {routes} routes · video cap {}",
            text(authority, "until"),
            usd(amount(&authority["usd_micros"])),
            amount(&authority["requests"]),
            usd(amount(&authority["video_cap_usd_micros"]))
        ),
        format!("Estimate: {estimate}"),
        format!("Run: {}", next.join(" ")),
    ]
}

fn elements(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}
