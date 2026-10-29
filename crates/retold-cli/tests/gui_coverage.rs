//! Every CLI operation is mapped to a native desktop surface or an explicit CLI-only reason.
//! A new command fails this test until its desktop coverage is decided.
use std::collections::BTreeMap;

const COVERAGE_TABLE: &str = include_str!("../../../docs/desktop/operation-coverage.tsv");

fn joined(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix} {name}")
    }
}

/// Every runnable leaf command, as space-separated paths.
fn leaves(command: &clap::Command, prefix: &str) -> Vec<String> {
    command
        .get_subcommands()
        .flat_map(|sub| {
            let path = joined(prefix, sub.get_name());
            let leaf = sub.get_subcommands().next().is_none();
            if leaf { vec![path] } else { leaves(sub, &path) }
        })
        .collect()
}

fn decided_surfaces() -> BTreeMap<String, String> {
    COVERAGE_TABLE
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('\t'))
        .map(|(path, surface)| (path.to_owned(), surface.to_owned()))
        .collect()
}

#[test]
fn every_cli_operation_has_a_decided_desktop_surface() {
    let found = leaves(&retold_cli::command_tree(), "");
    if std::env::var_os("RETOLD_PRINT_OPERATIONS").is_some() {
        println!("{}", found.join("\n"));
    }
    let mapped = decided_surfaces();
    let unmapped: Vec<_> = found
        .iter()
        .filter(|path| !mapped.contains_key(*path))
        .collect();
    let stale: Vec<_> = mapped.keys().filter(|path| !found.contains(path)).collect();
    let undecided: Vec<_> = mapped
        .iter()
        .filter(|(_, surface)| {
            !(surface.starts_with("native:") || surface.starts_with("cli-only:"))
        })
        .collect();
    assert!(unmapped.is_empty(), "Unmapped CLI operations: {unmapped:?}");
    assert!(
        stale.is_empty(),
        "Coverage lists removed operations: {stale:?}"
    );
    assert!(
        undecided.is_empty(),
        "Coverage entries need native: or cli-only:: {undecided:?}"
    );
}

fn run(words: &[&str]) -> (bool, String) {
    let arguments: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    let (payload, text) = retold_cli::run_operation(&arguments);
    (
        matches!(payload, retold_app::protocol::Payload::Result { .. }),
        text,
    )
}

#[test]
fn running_an_operation_from_the_desktop_returns_the_cli_outcome_and_text() {
    let (completed, text) = run(&["schema"]);
    let printed = text.len() > 1;
    assert!(completed && printed);
    let (helped, help) = run(&["creative", "audit", "--help"]);
    assert!(helped && help.contains("audit"));
}
