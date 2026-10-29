//! Compact human projections of application results. User labels cannot inject terminal controls.
use serde_json::Value;

pub fn render(contents: &Value) -> Option<String> {
    render_operations(contents).or_else(|| render_media(contents))
}

fn render_operations(contents: &Value) -> Option<String> {
    config_migrated(contents)
        .or_else(|| account_reconciled(contents))
        .or_else(|| video_reconciled(contents))
        .or_else(|| video_costs(contents))
        .or_else(|| account_attempts(contents))
        .or_else(|| account_processes(contents))
        .or_else(|| keyed(contents, "spend").map(spend))
        .or_else(|| keyed(contents, "limit_changed").map(limit_changed))
        .or_else(|| keyed(contents, "reconcile_preview").map(reconcile_preview))
        .or_else(|| {
            keyed(contents, "home").map(|home| crate::home::plain(home, crate::home::now()))
        })
        .or_else(|| keyed(contents, "backup_created").map(backup_created))
        .or_else(|| keyed(contents, "backup_preview").map(backup_preview))
        .or_else(|| keyed(contents, "backup_verified").map(backup_verified))
        .or_else(|| keyed(contents, "bundle_created").map(bundle_created))
        .or_else(|| keyed(contents, "bundle_preview").map(bundle_preview))
        .or_else(|| provider_health(contents))
        .or_else(|| keyed(contents, "test_plan").map(test_plan))
        .or_else(|| keyed(contents, "tested").map(test_results))
        .or_else(|| configured_providers(contents))
        .or_else(|| provider_routes(contents))
        .or_else(|| provider_inventory(contents))
}

fn render_media(contents: &Value) -> Option<String> {
    list_view(contents)
        .or_else(|| keyed(contents, "snapshot").map(|_| plan(contents)))
        .or_else(|| keyed(contents, "continued").map(continuation))
        .or_else(|| keyed(contents, "exported").map(exported_summary))
        .or_else(|| keyed(contents, "video").map(|_| snapshot(contents)))
        .or_else(|| video_paths(contents))
        .or_else(|| video_verify(contents))
        .or_else(|| video_clean(contents))
        .or_else(|| video_package(contents))
        .or_else(|| asset_detail(contents))
        .or_else(|| asset_verify(contents))
        .or_else(|| asset_usages(contents))
        .or_else(|| storage_usage(contents))
        .or_else(|| keyed(contents, "audit").map(creative_audit))
        .or_else(|| keyed(contents, "enhanced_prompt").map(enhanced_prompt))
        .or_else(|| keyed(contents, "cost_optimization").map(cost_optimization))
        .or_else(|| keyed(contents, "beat_report").map(beat_report_card))
        .or_else(|| keyed(contents, "dossier_report").map(dossier_report_card))
        .or_else(|| keyed(contents, "bible_report").map(bible_report_card))
        .or_else(|| keyed(contents, "coverage_report").map(coverage_report_card))
        .or_else(|| keyed(contents, "storyboard_report").map(storyboard_report_card))
        .or_else(|| keyed(contents, "soundscape_report").map(soundscape_report_card))
}

fn list_view(contents: &Value) -> Option<String> {
    let items = contents.get("items").and_then(Value::as_array)?;
    Some(list(contents, items))
}

fn keyed<'a>(contents: &'a Value, key: &str) -> Option<&'a Value> {
    contents.as_object()?.get(key)
}

/// A string field, or the value itself when `field` is empty.
pub fn text(contents: &Value, field: &str) -> String {
    let value = if field.is_empty() {
        Some(contents)
    } else {
        contents.get(field)
    };
    value
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn list_item_field(item: &Value, primary: &str, fallback: &str) -> String {
    let primary_val = text(item, primary);
    if primary_val == "unknown" {
        text(item, fallback)
    } else {
        primary_val
    }
}

fn list_item_summary(item: &Value) -> String {
    let key = list_item_field(item, "ref", "id");
    let title = list_item_field(item, "title", "work");
    let name = list_item_field(item, "name", "title");
    let label = if name == "unknown" { title } else { name };
    format!("{key} · {label}")
}

fn list(contents: &Value, items: &[Value]) -> String {
    let mut rows: Vec<_> = items.iter().map(list_item_summary).collect();
    if rows.is_empty() {
        rows.push("No saved items match this selection.".into());
    }
    rows.extend(page_footer(contents));
    rows.join("\n")
}

fn page_footer(contents: &Value) -> Vec<String> {
    let warning = (contents.get("complete") == Some(&Value::Bool(false))).then(|| {
        "Some manifests could not be inspected; see JSON issues before selecting a video.".into()
    });
    let next = contents
        .get("next_cursor")
        .and_then(Value::as_str)
        .map(|cursor| format!("Next page: repeat this command with --cursor {cursor}"));
    [warning, next].into_iter().flatten().collect()
}

fn snapshot(contents: &Value) -> String {
    let video = &contents["video"];
    let recorded = &contents["recorded"];
    let steps = recorded
        .get("steps")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    format!(
        "{} · {}\nID: {}\nRecorded cache entries: {steps}\nReadiness: requires execution preflight.",
        text(video, "ref"),
        text(video, "title"),
        text(video, "id")
    )
}

/// `$0.40 → $0.25` for one balance field of the account's lifetime money.
fn money_change(preview: &Value, field: &str) -> String {
    let before = amount(&preview["before"]["usd_micros"][field]);
    let after = amount(&preview["after"]["usd_micros"][field]);
    format!("{} → {}", usd(before), usd(after))
}

fn reconcile_preview(preview: &Value) -> String {
    format!(
        "Reconcile {} at {}\nCharged: {} · Held: {}\nDry run; nothing was written.",
        text(preview, "attempt_id"),
        text(preview, "cost_formatted"),
        money_change(preview, "charged"),
        money_change(preview, "held"),
    )
}

const MICROS_PER_USD: u64 = 1_000_000;
const CENT_MICROS: u64 = 10_000;

/// Whole cents, rounded up so a ceiling or charge is never understated.
pub fn usd(micros: u64) -> String {
    let cents = micros.div_ceil(CENT_MICROS);
    let cents_per_usd = MICROS_PER_USD / CENT_MICROS;
    format!("${}.{:02}", cents / cents_per_usd, cents % cents_per_usd)
}

/// Whole cents, rounded down so remaining headroom is never overstated.
pub fn usd_down(micros: u64) -> String {
    usd(micros - micros % CENT_MICROS)
}

pub fn amount(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

fn authority(contents: &Value) -> Vec<String> {
    let authority = &contents["authority"];
    let stages: Vec<String> = authority["stages"]
        .as_array()
        .map(|stages| stages.iter().map(|stage| text(stage, "")).collect())
        .unwrap_or_default();
    let routes = authority["routes"]
        .as_object()
        .map_or(0, serde_json::Map::len);
    let all_cached = contents["all_cached"].as_bool().unwrap_or(false);
    let summary = if all_cached {
        "0 steps · $0".to_string()
    } else {
        format!(
            "spend up to {} · up to {} paid requests · {routes} paid routes",
            usd(amount(&authority["usd_micros"])),
            amount(&authority["requests"])
        )
    };
    vec![
        format!(
            "Plan {}: {} · {summary}",
            text(contents, "plan_hash"),
            action(authority)
        ),
        format!("Stages: {}", stages.join(", ")),
    ]
}

/// What the plan may do: continue to a stop, or export named editions and formats.
fn action(authority: &Value) -> String {
    let scope = &authority["export"];
    if scope.is_null() {
        return format!("continue to {}", text(authority, "until"));
    }
    let names = |field: &str| -> String {
        scope[field]
            .as_array()
            .map(|names| names.iter().map(|name| text(name, "")).collect::<Vec<_>>())
            .unwrap_or_default()
            .join(", ")
    };
    let project_only = scope["project_only"] == Value::Bool(true);
    let target = if project_only {
        "editor project only"
    } else {
        "render"
    };
    format!(
        "export {} in {} ({target})",
        names("editions"),
        names("formats")
    )
}

fn next_step(contents: &Value) -> Vec<String> {
    let blockers = contents["blockers"].as_array().cloned().unwrap_or_default();
    if blockers.is_empty() {
        let command: Vec<String> = contents["next"]
            .as_array()
            .map(|words| words.iter().map(|word| text(word, "")).collect())
            .unwrap_or_default();
        return vec![format!("Next: {}", command.join(" "))];
    }
    let reasons: Vec<String> = blockers
        .iter()
        .map(|blocker| format!("{}: {}", text(blocker, "code"), text(blocker, "message")))
        .collect();
    vec![format!("Blocked: {}", reasons.join("; "))]
}

fn exported_summary(exported: &Value) -> String {
    let usd_micros = &exported["usage"]["usd_micros"];
    format!(
        "Exported as {} · job {}\nPaid: {} charged, {} held. Only the planned editions and formats were opened.",
        text(exported, "until"),
        text(exported, "job_id"),
        usd(amount(&usd_micros["charged"])),
        usd(amount(&usd_micros["held"]))
    )
}

fn continuation(continued: &Value) -> String {
    let usd_micros = &continued["usage"]["usd_micros"];
    format!(
        "Continued to {} · job {}\nPaid: {} charged, {} held. No project or export was created.",
        text(continued, "until"),
        text(continued, "job_id"),
        usd(amount(&usd_micros["charged"])),
        usd(amount(&usd_micros["held"]))
    )
}

fn plan(contents: &Value) -> String {
    let mut rows = title(&contents["snapshot"]["video"]);
    rows.extend(authority(contents));
    rows.extend(next_step(contents));
    rows.extend(evidence(contents));
    rows.join("\n")
}

fn title(video: &Value) -> Vec<String> {
    vec![
        format!("{} · {}", text(video, "ref"), text(video, "title")),
        format!("ID: {}", text(video, "id")),
    ]
}

/// Saved evidence for the stages this plan may run.
fn evidence(contents: &Value) -> Vec<String> {
    let planned = contents["authority"]["stages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let stages = contents["stages"].as_array().cloned().unwrap_or_default();
    let mut rows: Vec<String> = stages
        .iter()
        .filter(|stage| planned.contains(&stage["stage"]))
        .map(|stage| {
            format!(
                "{}: {} recorded, {} envelopes verified, {} unresolved",
                text(stage, "stage"),
                stage["recorded_steps"],
                stage["envelope_verified"],
                stage["unresolved_records"]
            )
        })
        .collect();
    rows.push("Estimates: unknown; the ceilings above bound what may be spent.".into());
    rows
}

fn provider_inventory(contents: &Value) -> Option<String> {
    contents.get("inventory").map(inventory_summary)
}

fn inventory_summary(inventory: &Value) -> String {
    let tools = array_field(inventory, "tools");
    let profiles = array_field(inventory, "profiles");
    scan_summary(tools, profiles.len())
}

fn array_field<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn elements(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn tool_summary(tool: &Value) -> String {
    let missing = tool["path"].is_null();
    let status = if missing {
        "not found on PATH"
    } else {
        "found on PATH"
    };
    format!("{}: {status}", text(tool, "name"))
}

fn scan_notices() -> [String; 2] {
    [
        "Versions and logins: not checked. No requests sent.".into(),
        "JSON includes paths and credential-variable presence; .env, Keychain and model files were not inspected.".into(),
    ]
}

fn scan_summary(tools: &[Value], profile_count: usize) -> String {
    let mut rows = vec!["Offline provider scan".to_owned()];
    rows.extend(tools.iter().map(tool_summary));
    rows.push(format!("Profile folders found: {profile_count}"));
    rows.extend(scan_notices());
    rows.join("\n")
}

fn provider_routes(contents: &Value) -> Option<String> {
    let routing = contents.get("routing")?.as_object()?;
    let rows: Vec<_> = routing
        .iter()
        .map(|(tier, chain)| route_row(tier, chain))
        .collect();
    Some(if rows.is_empty() {
        "No provider routes are configured.".into()
    } else {
        rows.join("\n")
    })
}

fn route_row(tier: &str, chain: &Value) -> String {
    let tier = text(&Value::String(tier.to_owned()), "");
    let hops: Vec<_> = chain
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| text(value, ""))
        .collect();
    format!("{tier}: {}", hops.join(" -> "))
}

fn configured_providers(contents: &Value) -> Option<String> {
    let providers = contents.get("providers")?.as_array()?;
    Some(provider_list_text(providers))
}

fn provider_list_text(providers: &[Value]) -> String {
    let mut rows = provider_rows(providers);
    rows.push("Adapter capabilities only. Authentication, models, quota and runtime health were not checked.".into());
    rows.join("\n")
}

fn provider_rows(providers: &[Value]) -> Vec<String> {
    let mut rows: Vec<_> = providers.iter().map(configured_provider).collect();
    if rows.is_empty() {
        rows.push("No providers configured. Use providers add to import a definition.".into());
    }
    rows
}

fn configured_provider(provider: &Value) -> String {
    let routes = provider["routes"]
        .as_array()
        .map_or_else(Vec::new, |routes| {
            routes
                .iter()
                .map(|route| format!("{} #{}", text(route, "tier"), amount(&route["position"])))
                .collect()
        });
    let tiers = provider["supported_tiers"]
        .as_array()
        .map_or_else(Vec::new, |tiers| {
            tiers.iter().map(|tier| text(tier, "")).collect()
        });
    format!(
        "{} · {} · max parallel {} · health not checked\n  Adapter tiers: {}\n  Routes: {}",
        text(provider, "id"),
        text(provider, "kind"),
        amount(&provider["max_parallel"]),
        tiers.join(", "),
        if routes.is_empty() {
            "unassigned".into()
        } else {
            routes.join(", ")
        }
    )
}

fn provider_health(contents: &Value) -> Option<String> {
    let health = contents.get("health")?.as_array()?;
    let rows: Vec<_> = health.iter().map(format_health_row).collect();
    Some(format!(
        "{}\nRecorded cooldowns only; elapsed does not mean healthy or logged in.",
        rows.join("\n")
    ))
}

fn format_health_row(entry: &Value) -> String {
    let id = text(entry, "id");
    let status = text(entry, "status");
    match status.as_str() {
        "logged_out" => format!("{id} · logged out · authentication required"),
        "failing" => {
            let count = entry["observation"]["consecutive_failures"]
                .as_u64()
                .unwrap_or(1);
            format!("{id} · failing ({count} in a row)")
        }
        "ok" => format!("{id} · ok"),
        _ => {
            let retry = entry["observation"]["retry_at"]
                .as_u64()
                .map_or_else(|| "unknown".into(), |time| time.to_string());
            format!("{id} · {status} · retry at {retry} (Unix seconds)")
        }
    }
}

fn account_attempts(contents: &Value) -> Option<String> {
    let page = contents.get("account_attempts")?;
    let attempts = page["attempts"].as_array()?;
    Some(attempt_page(page, attempts))
}

fn account_processes(contents: &Value) -> Option<String> {
    let processes = contents.get("processes")?.as_array()?;
    Some(processes_page(contents, processes))
}

fn processes_page(page: &Value, processes: &[Value]) -> String {
    let mut rows = supervision_rows(processes);
    rows.extend(processes_next_page(page));
    rows.push(
        "Inspection only. Evidence compares saved identity with the live process; nothing was signalled or changed."
            .into(),
    );
    rows.join("\n")
}

fn supervision_rows(processes: &[Value]) -> Vec<String> {
    if processes.is_empty() {
        return vec!["No supervised processes are recorded.".into()];
    }
    processes.iter().map(supervision_row).collect()
}

fn processes_next_page(page: &Value) -> Option<String> {
    let after = page.get("next_after").and_then(Value::as_str)?;
    let after = text(&Value::String(after.into()), "");
    Some(format!(
        "Next page: repeat with --after {after} and the same limit."
    ))
}

fn supervision_row(process: &Value) -> String {
    let worker = process["worker"]["pid"].as_u64().map_or_else(
        || "no worker recorded".into(),
        |pid| format!("worker {pid}"),
    );
    format!(
        "{} · {} · pid {} · {worker} · evidence {}",
        text(process, "id"),
        text(process, "status"),
        amount(&process["owner"]["pid"]),
        text(process, "worker_observation"),
    )
}

fn attempt_page(page: &Value, attempts: &[Value]) -> String {
    let mut rows = attempt_rows(attempts);
    rows.extend(attempt_next_page(page));
    rows.push("Inspection only. Reserved/dispatched or unknown observations require explicit recovery; no charges changed.".into());
    rows.join("\n")
}

fn attempt_rows(attempts: &[Value]) -> Vec<String> {
    let mut rows: Vec<_> = attempts
        .iter()
        .map(|attempt| {
            format!(
                "{} · {} · job {} · video {}",
                text(attempt, "id"),
                text(attempt, "state"),
                text(attempt, "job_id"),
                text(attempt, "video_id")
            )
        })
        .collect();
    if rows.is_empty() {
        rows.push("No recorded attempts match this selection.".into());
    }
    rows
}

fn attempt_next_page(page: &Value) -> Vec<String> {
    let mut rows = Vec::new();
    let after = page["next_after"].as_str();
    if let Some(after) = after {
        let after = text(&Value::String(after.into()), "");
        rows.push(format!(
            "Next page: repeat with --after {after} and the same filters."
        ));
    }
    rows
}

fn account_reconciled(contents: &Value) -> Option<String> {
    let attempt_id = contents.get("attempt_id")?.as_str()?;
    let cost = text(contents, "cost_formatted");
    Some(format!(
        "Reconciled account attempt `{attempt_id}` at {cost}.\nNo dispatch authority granted."
    ))
}

fn video_reconciled(contents: &Value) -> Option<String> {
    let video = contents.get("video")?;
    let request = contents.get("request")?.as_str()?;
    let cost = text(contents, "cost_formatted");
    Some(format!(
        "Reconciled {} request `{request}` at {cost}.\nNo dispatch authority granted.",
        text(video, "ref")
    ))
}

fn video_costs_title(video: &Value, spent: &str, budget: &str) -> Vec<String> {
    vec![
        format!("{} · {}", text(video, "ref"), text(video, "title")),
        format!("Spend: {spent} of {budget} budget"),
    ]
}

fn video_costs(contents: &Value) -> Option<String> {
    let video = contents.get("video")?;
    let budget = text(contents, "budget_formatted");
    let spent = text(contents, "total_spent_formatted");
    let has_budget = budget != "unknown";
    let has_spent = spent != "unknown";
    if !has_budget || !has_spent {
        return None;
    }
    Some(format_video_costs(video, contents))
}

fn stage_section(contents: &Value) -> Vec<String> {
    let stage_map = contents.get("stages").and_then(Value::as_object);
    stage_map.map_or_else(Vec::new, stage_cost_rows)
}

fn unresolved_section(contents: &Value, video: &Value) -> Vec<String> {
    let unresolved_list = contents.get("unresolved").and_then(Value::as_array);
    let video_ref = text(video, "ref");
    unresolved_list.map_or_else(Vec::new, |items| unresolved_summary(items, &video_ref))
}

fn format_video_costs(video: &Value, contents: &Value) -> String {
    let budget = text(contents, "budget_formatted");
    let spent = text(contents, "total_spent_formatted");
    let mut rows = video_costs_title(video, &spent, &budget);
    rows.extend(stage_section(contents));
    rows.extend(unresolved_section(contents, video));
    rows.push("Inspection only. No dispatch authority granted.".into());
    rows.join("\n")
}

fn stage_cost_rows(stages: &serde_json::Map<String, Value>) -> Vec<String> {
    if stages.is_empty() {
        return Vec::new();
    }
    let mut rows = vec!["Stages:".into()];
    for (stage, stage_data) in stages {
        let cost = text(stage_data, "cost_formatted");
        let calls = amount(&stage_data["calls"]);
        rows.push(format!("  {stage:<12} {cost:>8}  ({calls} calls)"));
    }
    rows
}

fn unresolved_summary(unresolved: &[Value], video_ref: &str) -> Vec<String> {
    if unresolved.is_empty() {
        return vec!["All paid requests are settled.".into()];
    }
    let mut rows = vec!["Unresolved charges:".into()];
    for item in unresolved {
        let req = text(item, "request");
        let held = text(item, "held_formatted");
        rows.push(format!("  {req}: held at {held}"));
    }
    rows.push(format!(
        "Settle with: retold video costs {video_ref} --request <id> --cost <usd>"
    ));
    rows
}

fn config_migrated(contents: &Value) -> Option<String> {
    let to = contents.get("to_version")?.as_u64()?;
    let migrated = contents.get("migrated")?.as_bool()?;
    if !migrated {
        return Some(format!("Configuration is already at schema version {to}."));
    }
    Some(render_applied_migration(contents, to))
}

fn render_applied_migration(contents: &Value, to: u64) -> String {
    let from = contents["from_version"].as_u64().unwrap_or(0);
    let dry_run = contents["dry_run"].as_bool().unwrap_or(false);
    let mode = if dry_run { " (dry run)" } else { "" };
    let backup_msg = contents["backup"]
        .as_str()
        .map_or(String::new(), |backup_path| {
            format!(" Backup saved to {backup_path}.")
        });
    format!("Configuration migrated from schema version {from} to {to}{mode}.{backup_msg}")
}

/// Probe decisions, the allowance and how to confirm; nothing has been sent yet.
fn test_plan(plan: &Value) -> String {
    let authority = &plan["authority"];
    let mut rows = vec![format!(
        "Test plan {} · spend up to {} · up to {} paid requests",
        text(plan, "plan_hash"),
        usd(amount(&authority["usd_micros"])),
        amount(&authority["requests"])
    )];
    rows.extend(array_field(authority, "probes").iter().map(probe_row));
    rows.extend(next_step(plan));
    rows.push("Nothing is sent until you confirm with --plan-hash or --yes.".into());
    rows.join("\n")
}

const SKIP_REASONS: &[(&str, &str)] = &[
    ("no_text_capability", "only text is tested"),
    ("unrouted", "no text route uses it"),
    ("needs_spend", "per-use; needs a positive --spend"),
];

fn probe_row(probe: &Value) -> String {
    let provider = text(probe, "provider");
    let billing = text(probe, "billing").replace('_', " ");
    if text(probe, "decision") == "test" {
        return format!(
            "{provider} · {billing} · test {} ({})",
            text(probe, "tier"),
            text(probe, "selector")
        );
    }
    let reason = text(probe, "reason");
    let explained = SKIP_REASONS
        .iter()
        .find(|(code, _)| *code == reason)
        .map_or(reason.as_str(), |(_, explained)| *explained);
    format!("{provider} · {billing} · skipped: {explained}")
}

/// Per-provider results of a confirmed test, from a result or an error's context.
pub fn test_results(tested: &Value) -> String {
    let job = tested["job_id"].as_str().map_or_else(
        || "no job admitted".into(),
        |_| format!("job {}", text(tested, "job_id")),
    );
    let mut rows = vec![format!("Provider test · {job}")];
    rows.extend(array_field(tested, "results").iter().map(result_row));
    let usd_micros = &tested["usage"]["usd_micros"];
    rows.push(format!(
        "Paid: {} charged, {} held.",
        usd(amount(&usd_micros["charged"])),
        usd(amount(&usd_micros["held"]))
    ));
    rows.join("\n")
}

fn result_row(result: &Value) -> String {
    let mut parts = vec![
        text(result, "provider"),
        text(result, "status").replace('_', " "),
    ];
    let optional =
        ["detail", "attempt_id"].map(|field| result[field].as_str().map(|_| text(result, field)));
    parts.extend(optional.into_iter().flatten());
    parts.join(" · ")
}

/// Charged and held money per row, the account ceiling and anything still unresolved.
fn spend(report: &Value) -> String {
    let mut rows = vec![spend_title(report)];
    rows.extend(spend_rows(report));
    rows.extend(spend_notes(report));
    rows.join("\n")
}

fn spend_notes(report: &Value) -> Vec<String> {
    let total = format!("Total · {}", spend_totals(&report["total"]));
    let notes = [
        account_ceiling(report),
        carried_holds(&report["carried"]),
        unresolved_attempts(report),
    ];
    let mut rows = vec![total];
    rows.extend(notes.into_iter().flatten());
    rows.push("Report only; nothing was changed.".into());
    rows
}

fn spend_rows(report: &Value) -> Vec<String> {
    let entries = array_field(report, "rows");
    if entries.is_empty() {
        return vec!["No recorded attempts in this period.".into()];
    }
    entries.iter().map(|row| spend_row(report, row)).collect()
}

fn spend_title(report: &Value) -> String {
    let period = match text(report, "window").as_str() {
        "day" => format!("today {}", text(report, "period")),
        "month" => format!("this month {}", text(report, "period")),
        _ => "all time".into(),
    };
    format!(
        "Spend · {period} ({}) · by {}",
        text(report, "timezone"),
        text(report, "by")
    )
}

fn spend_row(report: &Value, row: &Value) -> String {
    let ungrouped = row["key"].is_null();
    let key = if ungrouped {
        format!("no {}", text(report, "by"))
    } else {
        text(row, "key")
    };
    let mut parts = vec![key, spend_totals(row)];
    parts.extend(headroom(&row["ceiling"]).map(|headroom| format!("limit {headroom}")));
    parts.join(" · ")
}

fn spend_totals(totals: &Value) -> String {
    let resources = &totals["resources"];
    format!(
        "{} charged · {} held · {} · {}",
        usd(amount(&resources["usd_micros"]["charged"])),
        usd(amount(&resources["usd_micros"]["held"])),
        counted(amount(&resources["requests"]["charged"]), "request"),
        counted(amount(&totals["attempts"]), "attempt")
    )
}

fn counted(count: u64, noun: &str) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{plural}")
}

/// The USD limit and what is left of it, when the ceiling bounds money.
fn headroom(ceiling: &Value) -> Option<String> {
    let money = ceiling.get("usd_micros")?;
    Some(format!(
        "{} · {} left",
        usd_down(amount(&money["limit"])),
        usd_down(amount(&money["available"]))
    ))
}

fn account_ceiling(report: &Value) -> Option<String> {
    let window = text(report, "window");
    let ceiling = headroom(&report["ceiling"]);
    if let Some(ceiling) = ceiling {
        return Some(format!("Account {window} limit {ceiling}"));
    }
    (window != "lifetime")
        .then(|| format!("No account {window} money limit is set; paid work stays blocked."))
}

fn carried_holds(carried: &Value) -> Option<String> {
    let attempts = amount(&carried["attempts"]);
    (attempts > 0).then(|| {
        format!(
            "Earlier unresolved: {} holding {}; they count against this period until reconciled.",
            counted(attempts, "attempt"),
            usd(amount(&carried["resources"]["usd_micros"]["held"]))
        )
    })
}

fn unresolved_attempts(report: &Value) -> Option<String> {
    let unresolved = amount(&report["total"]["unresolved"]);
    (unresolved > 0).then(|| {
        format!(
            "Unresolved: {} still holding quotes. Inspect with `retold account attempts`; \
             settle with `retold account reconcile`.",
            counted(unresolved, "attempt")
        )
    })
}

/// One limit's old and new amounts, what is used against it now and whether work is blocked.
fn limit_changed(changed: &Value) -> String {
    let name = limit_name(changed);
    let mut rows = vec![match (&changed["before"], &changed["after"]) {
        (Value::Null, after) => format!("{name}: set to {}", resources(after)),
        (before, Value::Null) => format!("{name}: removed (was {})", resources(before)),
        (before, after) => format!("{name}: {} → {}", resources(before), resources(after)),
    }];
    rows.extend(limit_usage(&changed["headroom"]));
    rows.extend(limit_warning(changed));
    let dry_run = changed["dry_run"].as_bool().unwrap_or(false);
    rows.push(if dry_run {
        "Dry run; nothing was written.".into()
    } else {
        "Recorded charges and holds are unchanged.".into()
    });
    rows.join("\n")
}

fn limit_name(changed: &Value) -> String {
    let scope = &changed["scope"];
    let kind = text(scope, "kind");
    let account_wide = scope["id"].is_null();
    let owner = if account_wide {
        kind
    } else {
        format!("{kind} {}", text(scope, "id"))
    };
    let window = text(changed, "window");
    let owner = owner[..1].to_uppercase() + &owner[1..];
    format!("{owner} {window} limit")
}

/// Amounts per unit: money in dollars, everything else as a counted unit.
fn resources(amounts: &Value) -> String {
    let Some(amounts) = amounts.as_object() else {
        return "nothing".into();
    };
    let parts: Vec<String> = money_first(amounts)
        .map(|(unit, value)| resource(unit, amount(value)))
        .collect();
    parts.join(" · ")
}

/// Money, then requests, then the other units in name order.
fn money_first(units: &serde_json::Map<String, Value>) -> impl Iterator<Item = (&String, &Value)> {
    let rank = |unit: &str| match unit {
        "usd_micros" => 0,
        "requests" => 1,
        _ => 2,
    };
    let mut ordered: Vec<_> = units.iter().collect();
    ordered.sort_by_key(|(unit, _)| rank(unit));
    ordered.into_iter()
}

fn resource(unit: &str, value: u64) -> String {
    match unit {
        "usd_micros" => usd(value),
        "requests" => counted(value, "request"),
        _ => format!(
            "{value} {}",
            text(&Value::String(unit.replace('_', " ")), "")
        ),
    }
}

fn limit_usage(headroom: &Value) -> Option<String> {
    let units = headroom.as_object()?;
    let used: Vec<String> = money_first(units)
        .map(|(unit, room)| resource(unit, amount(&room["used"])))
        .collect();
    let left: Vec<String> = money_first(units)
        .map(|(unit, room)| left(unit, amount(&room["available"])))
        .collect();
    Some(format!(
        "Used now: {} · left: {}",
        used.join(" · "),
        left.join(" · ")
    ))
}

fn left(unit: &str, available: u64) -> String {
    if unit == "usd_micros" {
        usd_down(available)
    } else {
        resource(unit, available)
    }
}

fn limit_warning(changed: &Value) -> Option<String> {
    let removed = changed["after"].is_null();
    if removed {
        return Some("Paid work that requires this limit is now blocked.".into());
    }
    let exhausted = array_field(changed, "exhausted");
    (!exhausted.is_empty()).then(|| {
        let units: Vec<String> = exhausted
            .iter()
            .map(|unit| match text(unit, "").as_str() {
                "usd_micros" => "money".into(),
                other => other.replace('_', " "),
            })
            .collect();
        let periodic = text(changed, "window") != "lifetime";
        let ends = if periodic { " or its period ends" } else { "" };
        format!(
            "Nothing left of: {}. New paid work in this scope is blocked until the limit \
             rises{ends}.",
            units.join(", ")
        )
    })
}

const SHORT_ID: usize = 12;

fn account_id(record: &Value) -> String {
    text(record, "account_id").chars().take(SHORT_ID).collect()
}

fn backup_contents(contents: &Value) -> String {
    let money = &contents["usd_micros"];
    format!(
        "{} · {} · {} · {} unresolved · {} · {} charged, {} held",
        counted(amount(&contents["workspaces"]), "workspace"),
        counted(amount(&contents["jobs"]), "job"),
        counted(amount(&contents["attempts"]), "attempt"),
        amount(&contents["unresolved"]),
        counted(amount(&contents["limits"]), "limit"),
        usd(amount(&money["charged"])),
        usd(amount(&money["held"]))
    )
}

fn omitted(list: &Value) -> String {
    let items: Vec<String> = elements(list).iter().map(|item| text(item, "")).collect();
    format!("Not included: {}.", items.join("; "))
}

fn backup_created(published: &Value) -> String {
    let manifest = &published["manifest"];
    let output = text(published, "output");
    [
        format!("Backed up account {} to {output}", account_id(manifest)),
        backup_contents(&manifest["contents"]),
        omitted(&manifest["omitted"]),
        format!("Check it any time with: retold account verify-backup {output}"),
    ]
    .join("\n")
}

fn backup_preview(preview: &Value) -> String {
    let mut rows = vec![
        format!(
            "Would back up account {} to {}",
            account_id(preview),
            text(preview, "output")
        ),
        backup_contents(&preview["contents"]),
        omitted(&preview["omitted"]),
    ];
    rows.extend(owner_rows(&preview["busy"]));
    rows.push("Dry run; nothing was written.".into());
    rows.join("\n")
}

/// Running jobs and changes that hold the account, oldest first.
pub fn owner_rows(owners: &Value) -> Vec<String> {
    let owners = elements(owners);
    if owners.is_empty() {
        return Vec::new();
    }
    let mut rows = vec!["Holding the account now (a backup would be refused):".into()];
    rows.extend(owners.iter().map(|owner| {
        format!(
            "  {} · process {} · since {}",
            text(owner, "purpose"),
            amount(&owner["pid"]),
            text(owner, "since")
        )
    }));
    rows
}

fn backup_verified(verified: &Value) -> String {
    let manifest = &verified["manifest"];
    let same = match verified["same_account"].as_bool() {
        Some(true) => "yes",
        Some(false) => "no, it came from another account",
        None => "unknown, no account here",
    };
    [
        format!(
            "Backup verified: account {} taken {}",
            account_id(manifest),
            text(manifest, "created_at")
        ),
        backup_contents(&manifest["contents"]),
        format!("From this installation's account: {same}"),
        "Verification only; nothing was restored or changed.".into(),
    ]
    .join("\n")
}

fn path_row(key: &str, path_val: &Value) -> String {
    format!("  {key}: {}", text(path_val, ""))
}

fn video_paths(contents: &Value) -> Option<String> {
    let paths = contents.get("paths")?.as_object()?;
    let reference = text(contents, "reference");
    let mut rows = vec![format!("Paths for {reference}:")];
    rows.extend(
        paths
            .iter()
            .map(|(path_key, path_value)| path_row(path_key, path_value)),
    );
    Some(rows.join("\n"))
}

fn video_verify(contents: &Value) -> Option<String> {
    let status = text(contents, "status");
    let has_scope = contents.get("scope").is_some();
    let is_verified_video = status == "verified" && has_scope;
    if !is_verified_video {
        return None;
    }
    let reference = text(contents, "reference");
    let scope = text(contents, "scope");
    let count = amount(&contents["files_checked"]);
    let bytes = amount(&contents["total_bytes"]);
    Some(format!(
        "Verified {reference} (scope: {scope}): {count} files checked ({bytes} bytes). Integrity OK."
    ))
}

fn video_clean(contents: &Value) -> Option<String> {
    let status = text(contents, "status");
    let reference = text(contents, "reference");
    let scope = text(contents, "scope");
    if status == "preview" {
        let count = amount(&contents["files_to_remove"]);
        let bytes = amount(&contents["bytes_to_free"]);
        return Some(format!(
            "Clean preview for {reference} (scope: {scope}): {count} files ({bytes} bytes) would be removed. Pass --yes to confirm."
        ));
    }
    if status == "cleaned" {
        let count = amount(&contents["removed_files"]);
        let bytes = amount(&contents["freed_bytes"]);
        return Some(format!(
            "Cleaned intermediate files for {reference} (scope: {scope}): {count} files removed ({bytes} bytes freed). Kept deliverables and accounting."
        ));
    }
    None
}

fn video_package(contents: &Value) -> Option<String> {
    let status = text(contents, "status");
    let reference = text(contents, "reference");
    let edition = text(contents, "edition");
    let aspect = text(contents, "aspect");
    let label = text(contents, "label");
    let output = text(contents, "output");
    let release_id = text(contents, "release_id");
    let count = amount(&contents["file_count"]);
    let bytes = amount(&contents["total_bytes"]);

    if status == "preview" {
        return Some(format!(
            "Package preview for {reference} ({edition}, {aspect}) -> {output}\n  Release: {release_id}\n  Label: {label}\n  Files: {count} ({bytes} bytes)"
        ));
    }
    if status == "packaged" {
        return Some(format!(
            "Packaged {reference} ({edition}, {aspect}) -> {output}\n  Release: {release_id}\n  Label: {label}\n  Files: {count} ({bytes} bytes)\n  Verified package manifest: package.json\n  Checksums: CHECKSUMS.sha256"
        ));
    }
    None
}

fn asset_detail(contents: &Value) -> Option<String> {
    let asset = contents.get("asset")?;
    let id = text(asset, "id");
    let name = text(asset, "name");
    let kind = text(asset, "kind");
    let reference = text(asset, "reference");
    let bytes = amount(&asset["bytes"]);
    let sha256 = text(asset, "sha256");
    let blake3 = text(asset, "blake3");
    let path = text(asset, "path");
    let is_immutable = asset["is_immutable"].as_bool().unwrap_or(false);
    let imm_label = if is_immutable { "immutable" } else { "mutable" };
    Some(format!(
        "Asset {id} · {name}
  Kind: {kind} ({imm_label})
  Reference: {reference}
  Path: {path}
  Size: {bytes} bytes
  SHA-256: {sha256}
  BLAKE3:  {blake3}"
    ))
}

fn asset_verify(contents: &Value) -> Option<String> {
    let is_verified = contents
        .get("verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !is_verified {
        return None;
    }
    let id = text(contents, "id");
    let name = text(contents, "name");
    let reference = text(contents, "reference");
    let sha256 = text(contents, "sha256");
    Some(format!(
        "Verified integrity for asset {id} ({name})
  Reference: {reference}
  Status: verified OK
  SHA-256: {sha256}"
    ))
}

fn format_usage_entry(entry: &Value) -> String {
    let target_ref = text(entry, "reference");
    let context = text(entry, "context");
    let stage = text(entry, "dependent_stage");
    format!("  - {target_ref} [{stage}]: {context}")
}

fn asset_usage_rows(usages: &[Value]) -> Vec<String> {
    let is_empty = usages.is_empty();
    if is_empty {
        vec!["  No dependents or references found.".to_string()]
    } else {
        usages.iter().map(format_usage_entry).collect()
    }
}

fn asset_usages(contents: &Value) -> Option<String> {
    let usages = contents.get("usages")?.as_array()?;
    let id = text(contents, "id");
    let name = text(contents, "name");
    let header = format!("Usages for asset {id} ({name}):");
    let mut rows = vec![header];
    rows.extend(asset_usage_rows(usages));
    Some(rows.join(
        "
",
    ))
}

fn format_storage_group(group: &Value) -> String {
    let name = text(group, "name");
    let files = amount(&group["files"]);
    let bytes = amount(&group["bytes"]);
    let pct = group["percentage"].as_f64().unwrap_or(0.0);
    format!("  {name:<24} {files:>4} files  {bytes:>10} bytes  ({pct:>5.1}%)")
}

fn storage_usage(contents: &Value) -> Option<String> {
    let total_bytes = amount(&contents["total_bytes"]);
    let total_files = amount(&contents["total_files"]);
    let reclaimable = amount(&contents["reclaimable_bytes"]);
    let grouping = text(contents, "grouping");
    let groups = contents.get("groups")?.as_array()?;

    let header = format!(
        "Storage usage ({grouping}): {total_files} files · {total_bytes} bytes · {reclaimable} bytes reclaimable"
    );
    let mut rows = vec![header];
    rows.extend(groups.iter().map(format_storage_group));
    Some(rows.join(
        "
",
    ))
}

fn format_content_classes(bundle: &Value) -> String {
    elements(&bundle["content_classes"])
        .iter()
        .filter_map(|class_val| class_val.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn bundle_created(bundle: &Value) -> String {
    let output = text(bundle, "output_path");
    let file_count = amount(&bundle["file_count"]);
    let total_bytes = amount(&bundle["total_bytes"]);
    let classes = format_content_classes(bundle);
    [
        format!("Support bundle created at {output}"),
        format!("Files: {file_count} ({total_bytes} bytes)"),
        format!("Content classes: {classes}"),
        "Redacted secret patterns, credential paths, and prompt text by default.".to_string(),
    ]
    .join(
        "
",
    )
}

fn bundle_preview(preview: &Value) -> String {
    let output = text(preview, "output_path");
    let file_count = amount(&preview["file_count"]);
    let redactions = amount(&preview["redactions_count"]);
    let classes = format_content_classes(preview);
    [
        format!("Support bundle preview for {output}"),
        format!("Files to include: {file_count}"),
        format!("Content classes: {classes}"),
        format!("Redactions detected: {redactions}"),
        "Dry run; no files were written to disk.".to_string(),
    ]
    .join(
        "
",
    )
}

fn format_string_list(items: Option<&[Value]>, prefix: &str) -> Vec<String> {
    let Some(list) = items else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry_val| entry_val.as_str())
        .map(|text| format!("{prefix}{text}"))
        .collect()
}

fn format_audit_header(audit: &Value) -> Vec<String> {
    let score = amount(&audit["script_score"]);
    let words = amount(&audit["total_words"]);
    let sentences = amount(&audit["sentence_count"]);
    let sensory = amount(&audit["sensory_verb_count"]);
    let density = audit["sensory_verb_density_pct"].as_f64().unwrap_or(0.0);
    vec![
        format!("Script Quality Audit: {score}/100"),
        format!(
            "Metrics: {words} words, {sentences} sentences, {sensory} sensory verbs ({density:.1}% density)"
        ),
    ]
}

fn format_audit_sections(audit: &Value) -> Vec<String> {
    let cliches = audit["banned_cliches_found"].as_array().map(Vec::as_slice);
    let recommendations = audit["recommendations"].as_array().map(Vec::as_slice);
    let mut sections = format_string_list(cliches, "  - Banned cliche: ");
    let rec_lines = format_string_list(recommendations, "  - ");
    if !rec_lines.is_empty() {
        sections.push("Recommendations:".to_string());
        sections.extend(rec_lines);
    }
    sections
}

fn creative_audit(audit: &Value) -> String {
    let mut lines = format_audit_header(audit);
    lines.extend(format_audit_sections(audit));
    lines.join("\n")
}

fn format_prompt_header(prompt: &Value) -> Vec<String> {
    let model = text(prompt, "target_model");
    let positive = text(prompt, "positive_prompt");
    let negative = text(prompt, "negative_prompt");
    let tokens = amount(&prompt["estimated_tokens"]);
    vec![
        format!("Cinematic Prompt [{model}] (~{tokens} tokens):"),
        format!("Positive:\n  {positive}"),
        format!("Negative:\n  {negative}"),
    ]
}

fn format_prompt_body(prompt: &Value) -> Vec<String> {
    let enhancements = prompt["cinematic_enhancements"]
        .as_array()
        .map(Vec::as_slice);
    let enhancement_lines = format_string_list(enhancements, "  - ");
    if enhancement_lines.is_empty() {
        Vec::new()
    } else {
        let mut body = vec!["Enhancements:".to_string()];
        body.extend(enhancement_lines);
        body
    }
}

fn enhanced_prompt(prompt: &Value) -> String {
    let mut lines = format_prompt_header(prompt);
    lines.extend(format_prompt_body(prompt));
    lines.join("\n")
}

fn cost_optimization(summary: &Value) -> String {
    let shots = amount(&summary["total_shots"]);
    let fresh = amount(&summary["fresh_generations"]);
    let reused = amount(&summary["reused_frames"]);
    let parallax = amount(&summary["parallax_upgrades"]);
    let original = text(summary, "original_cost_usd");
    let optimized = text(summary, "optimized_cost_usd");
    let savings = text(summary, "savings_usd");
    let pct = summary["savings_percentage"].as_f64().unwrap_or(0.0);

    [
        format!("Storyboard Cost Optimization ({shots} shots):"),
        format!("  Fresh generations: {fresh} (hero & keyframes)"),
        format!("  Reused frames:     {reused} ($0 cost)"),
        format!("  Parallax 2.5D:     {parallax} (87.5% savings)"),
        format!("Total spend: {original} -> {optimized} (Saved {savings}, {pct:.1}% reduction)"),
    ]
    .join("\n")
}

fn format_beat_row(beat: &Value) -> String {
    let idx = beat["index"].as_u64().unwrap_or(0) + 1;
    let section = text(beat, "section");
    let de = beat["delta_e"].as_f64().unwrap_or(0.0);
    let sparkline = text(beat, "sparkline");
    let is_climax = beat["is_climax"].as_bool().unwrap_or(false);
    let climax_flag = if is_climax { " (CLIMAX)" } else { "" };
    format!("  [{idx}] {section:<14} {de:4.2} {sparkline}{climax_flag}")
}

fn format_beat_evaluations(evaluations: Option<&[Value]>) -> Vec<String> {
    let Some(evals) = evaluations else {
        return Vec::new();
    };
    evals.iter().map(format_beat_row).collect()
}

fn format_beat_summary(report: &Value) -> Vec<String> {
    let total = amount(&report["total_beats"]);
    let avg = report["average_delta_e"].as_f64().unwrap_or(0.0);
    let peak = report["peak_delta_e"].as_f64().unwrap_or(0.0);
    let peak_idx = report["peak_index"].as_u64().unwrap_or(0) + 1;
    let has_inversion = report["has_inversion"].as_bool().unwrap_or(false);
    let inversion = if has_inversion { "Yes" } else { "No" };
    let gate_passed = report["passes_gates"].as_bool().unwrap_or(false);
    let status = if gate_passed {
        "PASSED"
    } else {
        "FAILED GATES"
    };

    vec![
        "Summary:".to_string(),
        format!("  Total Beats:       {total}"),
        format!("  Average ΔE:        {avg:.2}"),
        format!("  Peak Tension:      {peak:.2} (Beat {peak_idx})"),
        format!("  Reality Inversion: {inversion}"),
        format!("  Gate Status:       {status}"),
    ]
}

fn beat_report_card(report: &Value) -> String {
    let mut lines = vec!["Dramatic Tension Arc (ΔE per Beat):".to_string()];
    let evals = report["beat_evaluations"].as_array().map(Vec::as_slice);
    lines.extend(format_beat_evaluations(evals));
    lines.push(String::new());
    lines.extend(format_beat_summary(report));
    lines.join(
        "
",
    )
}

fn format_dossier_header(report: &Value) -> Vec<String> {
    let title = text(report, "work_title");
    let medium = text(report, "work_medium");
    let year = text(report, "work_year");
    let version = text(report, "version_note");
    let logline = text(report, "logline");
    vec![
        format!("Research Dossier: {title} ({year} {medium})"),
        format!("  Version Note: {version}"),
        format!("  Logline:      {logline}"),
    ]
}

fn format_dossier_counts(report: &Value) -> Vec<String> {
    let total = amount(&report["total_beats"]);
    let must = amount(&report["must_beats"]);
    let should = amount(&report["should_beats"]);
    let could = amount(&report["could_beats"]);
    let iconic = amount(&report["iconic_count"]);
    let settings = amount(&report["settings_count"]);
    let gate_passed = report["passes_gates"].as_bool().unwrap_or(false);
    let status = if gate_passed {
        "PASSED"
    } else {
        "FAILED GATES"
    };

    vec![
        "Structure Breakdown:".to_string(),
        format!("  Chronological Beats: {total} (Must: {must}, Should: {should}, Could: {could})"),
        format!("  Iconic Elements:     {iconic}"),
        format!("  Named Settings:      {settings}"),
        format!("  Gates Status:        {status}"),
    ]
}

fn dossier_report_card(report: &Value) -> String {
    let mut lines = format_dossier_header(report);
    lines.push(String::new());
    lines.extend(format_dossier_counts(report));
    lines.join(
        "
",
    )
}

fn format_character_block(character: &Value) -> Vec<String> {
    let name = text(character, "name");
    let role = text(character, "role");
    let intro = text(character, "introduction");
    let want = text(character, "want");
    let arc = text(character, "arc");
    let look = text(character, "look");
    let prop = text(character, "signature_prop");

    vec![
        format!("  [{role}] {name}"),
        format!("    Introduction: {intro}"),
        format!("    Motivation:   {want}"),
        format!("    Dramatic Arc: {arc}"),
        format!("    Visual Look:  {look}"),
        format!("    Signature:    {prop}"),
    ]
}

fn bible_report_card(report: &Value) -> String {
    let total = amount(&report["total_characters"]);
    let is_passed = report["passes_gates"].as_bool();
    let gate_passed = is_passed.unwrap_or(false);
    let status = if gate_passed {
        "PASSED"
    } else {
        "FAILED GATES"
    };

    let mut lines = vec![format!(
        "Character Bible ({total} characters, Status: {status}):"
    )];

    let character_list = report["characters"].as_array();
    if let Some(characters) = character_list {
        for char_val in characters {
            lines.extend(format_character_block(char_val));
        }
    }

    lines.join(
        "
",
    )
}

fn format_coverage_warnings(report: &Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let missing_list = report["missing_beats"].as_array();
    if let Some(missing) = missing_list {
        for miss in missing {
            if let Some(miss_str) = miss.as_str() {
                warnings.push(format!("  Warning: Missing beat {miss_str}"));
            }
        }
    }
    warnings
}

fn coverage_report_card(report: &Value) -> String {
    let covered_b = amount(&report["covered_beats"]);
    let total_b = amount(&report["total_outline_beats"]);
    let covered_i = amount(&report["covered_iconic"]);
    let is_passed = report["passes_gates"].as_bool();
    let gate_passed = is_passed.unwrap_or(false);
    let status = if gate_passed {
        "PASSED"
    } else {
        "FAILED GATES"
    };

    let mut lines = vec![
        "Script Adaptation Coverage:".to_string(),
        format!("  Beats Realized:   {covered_b}/{total_b}"),
        format!("  Iconic Setups:    {covered_i}"),
        format!("  Gate Status:      {status}"),
    ];
    lines.extend(format_coverage_warnings(report));
    lines.join("\n")
}

fn format_storyboard_header(report: &Value) -> Vec<String> {
    let total = amount(&report["total_shots"]);
    let dynamic = amount(&report["dynamic_motion_shots"]);
    let static_cnt = amount(&report["static_shots"]);
    let coverage_pct = report["parallax_coverage_pct"].as_f64().unwrap_or(0.0);
    let passes = report["passes_gates"].as_bool().unwrap_or(false);
    let status = if passes { "PASSED" } else { "FAILED GATES" };

    vec![
        format!("Storyboard Choreography ({total} shots, Status: {status}):"),
        format!("  Dynamic Motion:   {dynamic} shots ({coverage_pct:.1}% parallax coverage)"),
        format!("  Static Lock-off:  {static_cnt} shots"),
    ]
}

fn format_storyboard_shot_line(shot: &Value) -> String {
    let unit = amount(&shot["unit"]);
    let location = text(shot, "location");
    let motion = text(shot, "motion_type");
    let camera = text(shot, "camera_package");
    format!("  [Unit {unit}] {location} | Camera: {camera} | Motion: {motion}")
}

fn format_storyboard_shot_lines(shots: &[Value]) -> Vec<String> {
    shots.iter().map(format_storyboard_shot_line).collect()
}

fn extract_storyboard_shot_lines(report: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    let shot_list = report["shots"].as_array();
    if let Some(shots) = shot_list {
        lines.push("  Shot Setups:".to_string());
        lines.extend(format_storyboard_shot_lines(shots));
    }
    lines
}

fn storyboard_report_card(report: &Value) -> String {
    let mut lines = format_storyboard_header(report);
    lines.extend(extract_storyboard_shot_lines(report));
    lines.join("\n")
}

fn format_soundscape_header(report: &Value) -> Vec<String> {
    let total = amount(&report["total_units"]);
    let stings = amount(&report["stings_count"]);
    let silences = amount(&report["silence_moments"]);
    let mood = text(report, "dominant_mood");
    let passes = report["passes_gates"].as_bool().unwrap_or(false);
    let status = if passes { "PASSED" } else { "FAILED GATES" };

    vec![
        format!("Audio Soundscape & Score ({total} units, Status: {status}):"),
        format!("  Dominant Mood:    {mood}"),
        format!("  Score Accents:    {stings} stings, {silences} silence moments"),
    ]
}

fn format_sound_unit_line(unit: &Value) -> String {
    let unit_idx = amount(&unit["unit"]);
    let mood = text(unit, "mood");
    let pace = text(unit, "pace");
    let tone = text(unit, "tone");
    let sting_flag = unit["sting"].as_bool().unwrap_or(false);
    let sting_label = if sting_flag { " [STING]" } else { "" };
    format!("  [Unit {unit_idx}] Mood: {mood}{sting_label} | Narration: {pace} pace, {tone} tone")
}

fn format_soundscape_unit_lines(units: &[Value]) -> Vec<String> {
    units.iter().map(format_sound_unit_line).collect()
}

fn extract_soundscape_unit_lines(report: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    let unit_list = report["units"].as_array();
    if let Some(units) = unit_list {
        lines.push("  Unit Soundscape:".to_string());
        lines.extend(format_soundscape_unit_lines(units));
    }
    lines
}

fn soundscape_report_card(report: &Value) -> String {
    let mut lines = format_soundscape_header(report);
    lines.extend(extract_soundscape_unit_lines(report));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn processes_rows_show_saved_identity_and_the_next_cursor() {
        let contents = json!({
            "processes": [
                {
                    "id": "a",
                    "status": "active",
                    "owner": { "pid": 41 },
                    "worker": { "pid": 42 },
                    "worker_observation": "matching"
                },
                {
                    "id": "b",
                    "status": "legacy_unconfirmed",
                    "owner": { "pid": 7 },
                    "worker": null,
                    "worker_observation": null
                }
            ],
            "next_after": "b"
        });
        let page = account_processes(&contents).unwrap();
        assert!(page.contains("a · active · pid 41 · worker 42 · evidence matching"));
        assert!(page.contains("b · legacy_unconfirmed · pid 7 · no worker recorded"));
        assert!(page.contains("--after b"));
    }
}
