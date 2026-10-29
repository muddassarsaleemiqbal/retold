//! Typed descriptions of every operation's options, derived from the command definitions, so a
//! graphical client can present real controls and build exactly the arguments the CLI accepts.
use std::path::PathBuf;

use clap::{Arg, ArgAction, Command};

/// Options every command inherits; a graphical client sets these itself.
const GLOBAL: [&str; 6] = ["json", "plain", "quiet", "no-color", "library", "workspace"];

/// The control a field needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// On/off switch (`--flag`).
    Toggle,
    /// One of a fixed set of values.
    Choice(Vec<String>),
    /// A file or folder on disk.
    Path,
    /// A whole number.
    Number,
    /// Free text.
    Text,
}

/// One option of an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub id: String,
    /// Readable label, such as "Dry run".
    pub label: String,
    pub help: String,
    pub kind: FieldKind,
    pub required: bool,
    /// Positional arguments have no `--name`.
    pub positional: bool,
    /// Accepts several values.
    pub repeated: bool,
    pub default: Option<String>,
    long: Option<String>,
}

/// One operation and its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    /// Space-separated path, such as `account limit`.
    pub path: String,
    pub summary: String,
    pub fields: Vec<Field>,
}

impl Operation {
    /// Whether the operation offers a preview mode.
    #[must_use]
    pub fn has_dry_run(&self) -> bool {
        self.fields.iter().any(|field| field.id == "dry-run")
    }
}

/// A value entered for one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    On,
    Text(String),
    Many(Vec<String>),
}

/// Words shown in capitals in labels.
const ACRONYMS: [&str; 9] = [
    "usd", "id", "url", "json", "sfx", "api", "tts", "ffmpeg", "mp4",
];

fn capitalized(word: &str) -> String {
    let mut characters = word.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

fn word(index: usize, word: &str) -> String {
    if ACRONYMS.contains(&word) {
        word.to_uppercase()
    } else if index == 0 {
        capitalized(word)
    } else {
        word.to_owned()
    }
}

/// Labels that read better than the option id they come from.
const PLAIN_LABELS: [(&str, &str); 4] = [
    ("reconcile", "Settle pending charges"),
    ("request", "Charge to correct"),
    ("window", "Resets every"),
    ("resource", "Other limits"),
];

/// A readable label such as "Dry run" or "USD" from an option id.
#[must_use]
pub fn label(id: &str) -> String {
    plain_label(id).unwrap_or_else(|| words(id))
}

fn plain_label(id: &str) -> Option<String> {
    PLAIN_LABELS
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, plain)| (*plain).to_owned())
}

fn words(id: &str) -> String {
    id.split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .enumerate()
        .map(|(index, part)| word(index, part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_toggle(arg: &Arg) -> bool {
    matches!(
        arg.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    )
}

fn kind(arg: &Arg) -> FieldKind {
    if is_toggle(arg) {
        return FieldKind::Toggle;
    }
    let choices: Vec<String> = arg
        .get_possible_values()
        .iter()
        .map(|value| value.get_name().to_owned())
        .collect();
    if !choices.is_empty() {
        return FieldKind::Choice(choices);
    }
    let parser = arg.get_value_parser().type_id();
    if parser == std::any::TypeId::of::<PathBuf>() {
        return FieldKind::Path;
    }
    let numeric = [
        std::any::TypeId::of::<u64>(),
        std::any::TypeId::of::<u32>(),
        std::any::TypeId::of::<usize>(),
        std::any::TypeId::of::<i64>(),
    ];
    let is_number = numeric.iter().any(|number| parser == *number);
    if is_number {
        return FieldKind::Number;
    }
    FieldKind::Text
}

fn field(arg: &Arg) -> Field {
    let id = arg.get_id().as_str().to_owned();
    let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
    let repeated = matches!(arg.get_action(), ArgAction::Append)
        || arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1);
    let default = arg
        .get_default_values()
        .first()
        .map(|value| value.to_string_lossy().into_owned());
    Field {
        label: label(&id),
        help,
        kind: kind(arg),
        required: arg.is_required_set(),
        positional: arg.is_positional(),
        repeated,
        default,
        long: arg.get_long().map(str::to_owned),
        id,
    }
}

fn visible(arg: &Arg) -> bool {
    let id = arg.get_id().as_str();
    !arg.is_hide_set() && !GLOBAL.contains(&id) && id != "help" && id != "version"
}

fn describe(command: &Command, path: String) -> Operation {
    Operation {
        summary: command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default(),
        fields: command
            .get_arguments()
            .filter(|arg| visible(arg))
            .map(field)
            .collect(),
        path,
    }
}

fn collect(command: &Command, prefix: &str) -> Vec<Operation> {
    command
        .get_subcommands()
        .flat_map(|sub| {
            let path = if prefix.is_empty() {
                sub.get_name().to_owned()
            } else {
                format!("{prefix} {}", sub.get_name())
            };
            let leaf = sub.get_subcommands().next().is_none();
            if leaf {
                vec![describe(sub, path)]
            } else {
                collect(sub, &path)
            }
        })
        .collect()
}

/// Every operation with its fields, in command order.
#[must_use]
pub fn operations() -> Vec<Operation> {
    collect(&super::command_tree(), "")
}

fn named(field: &Field, text: &str) -> Vec<String> {
    let flag = field.long.as_ref().map(|long| format!("--{long}"));
    flag.into_iter()
        .chain(std::iter::once(text.to_owned()))
        .collect()
}

/// The arguments one entered value contributes.
fn value_arguments(field: &Field, value: &Value) -> Vec<String> {
    match value {
        Value::On if field.kind == FieldKind::Toggle => {
            vec![format!("--{}", field.long.as_deref().unwrap_or(&field.id))]
        }
        Value::On => Vec::new(),
        Value::Text(text) if text.trim().is_empty() => Vec::new(),
        Value::Text(text) => named(field, text),
        Value::Many(values) => values
            .iter()
            .filter(|text| !text.trim().is_empty())
            .flat_map(|text| named(field, text))
            .collect(),
    }
}

/// Build the exact argument list for `operation` from entered values, positionals first.
///
/// # Errors
/// Names the first required field that has no value.
pub fn arguments(operation: &Operation, values: &[(String, Value)]) -> Result<Vec<String>, String> {
    let lookup = |id: &str| {
        values
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, value)| value)
    };
    let missing = operation.fields.iter().find(|field| {
        field.required
            && lookup(&field.id)
                .is_none_or(|value| matches!(value, Value::Text(text) if text.trim().is_empty()))
    });
    if let Some(field) = missing {
        return Err(format!("{} is required.", field.label));
    }
    let mut arguments: Vec<String> = operation.path.split(' ').map(str::to_owned).collect();
    let (positional, named): (Vec<&Field>, Vec<&Field>) =
        operation.fields.iter().partition(|field| field.positional);
    for field in positional.into_iter().chain(named) {
        if let Some(value) = lookup(&field.id) {
            arguments.extend(value_arguments(field, value));
        }
    }
    Ok(arguments)
}
