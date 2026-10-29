//! Semantic script commands use the same versioned document boundary as the native editor.
use std::path::Path;

use clap::{Arg, ArgMatches, Command};
use retold_app::editing::{self, Action, SectionEdit};
use retold_app::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

pub fn command() -> Command {
    let root = Command::new("script").about("Read and safely edit the actual saved master script");
    root.subcommand_required(true)
        .subcommands(script_operations())
}

fn script_operations() -> Vec<Command> {
    let edit = versioned(
        "edit",
        "Save one script section; invalidates downstream work",
    );
    vec![
        operation("show", "Inspect saved script and its current edit version"),
        operation("history", "Inspect immutable script revision history").arg(value("cursor")),
        edit.arg(value("section").required(true))
            .arg(value("text").required(true)),
        versioned("approve", "Approve the exact current script content"),
        versioned("undo", "Undo one script revision; clears approval"),
        versioned(
            "redo",
            "Restore one undone script revision; clears approval",
        ),
    ]
}

fn operation(name: &'static str, description: &'static str) -> Command {
    Command::new(name).about(description).arg(
        Arg::new("reference")
            .required(true)
            .help("Saved video ID or explicit reference"),
    )
}

fn versioned(name: &'static str, description: &'static str) -> Command {
    operation(name, description).arg(value("expected-version").required(true))
}

fn value(name: &'static str) -> Arg {
    let help = match name {
        "expected-version" => {
            "Exact document version returned by video script show; rejects concurrent changes"
        }
        "section" => "Existing master-script section ID to edit",
        "text" => "Complete replacement text for the selected section, preserving Unicode",
        _ => "JSON history cursor returned by the previous page",
    };
    Arg::new(name).long(name).help(help)
}

pub fn query(root: &Path, args: &ArgMatches) -> Payload {
    script_result(root, args).unwrap_or_else(Diagnostic::into_payload)
}

fn script_result(root: &Path, args: &ArgMatches) -> Result<Payload, Diagnostic> {
    let (operation, args) = args.subcommand().ok_or_else(invalid)?;
    let reference = argument(args, "reference");
    if operation == "history" {
        let cursor = args
            .get_one::<String>("cursor")
            .map(|source| {
                serde_json::from_str::<editing::HistoryCursor>(source).map_err(|_| invalid())
            })
            .transpose()?;
        return Ok(response(serde_json::json!(editing::history(
            root,
            reference,
            cursor.as_ref()
        )?)));
    }
    let action = if operation == "show" {
        Action::Read
    } else if operation == "edit" {
        edit_action(args)
    } else {
        version_change(operation, args)?
    };
    Ok(response(serde_json::json!(editing::script(
        root, reference, action
    )?)))
}

fn edit_action(args: &ArgMatches) -> Action<'_> {
    Action::Save(SectionEdit {
        expected_version: argument(args, "expected-version"),
        section: argument(args, "section"),
        text: argument(args, "text"),
    })
}

fn version_change<'a>(operation: &str, args: &'a ArgMatches) -> Result<Action<'a>, Diagnostic> {
    let expected = argument(args, "expected-version");
    let changes = [
        ("approve", Action::Approve(expected)),
        ("undo", Action::Undo(expected)),
        ("redo", Action::Redo(expected)),
    ];
    changes
        .into_iter()
        .find_map(|(name, action)| (name == operation).then_some(action))
        .ok_or_else(invalid)
}

fn argument<'a>(args: &'a ArgMatches, name: &str) -> &'a str {
    args.get_one::<String>(name).map_or("", String::as_str)
}

const fn response(contents: serde_json::Value) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents,
    }
}

fn invalid() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::InputRequired,
        "Supply a valid script operation, reference and expected version; history cursors use JSON.",
    )
}
