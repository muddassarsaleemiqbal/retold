//! Every CLI command as an action the app can show, fill in and run. Built from the same clap
//! tree the command line parses, so a new command appears here with no extra code.
use clap::{Arg, ArgAction, Command};

use super::section::Section;

/// Flags that select output or location for the whole process; the app sets those itself.
const GLOBAL_ARGS: [&str; 8] = [
    "json",
    "plain",
    "quiet",
    "no-color",
    "library",
    "workspace",
    "help",
    "version",
];

/// Commands that are not useful as actions: clap's own `help` and placeholders.
const HIDDEN_COMMANDS: [&str; 2] = ["help", "publish"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// Reads saved state only.
    Read,
    /// Changes files or settings, but cannot spend money.
    Write,
    /// May send paid provider requests within an explicit allowance.
    Spend,
    /// Removes or replaces something.
    Destructive,
}

impl Risk {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "read only",
            Self::Write => "changes files",
            Self::Spend => "may spend",
            Self::Destructive => "removes data",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// An on/off switch.
    Flag,
    /// Free text; numbers and paths are text the command validates.
    Text,
    /// One of a fixed set of values.
    Choice(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub help: String,
    /// `--name` for options and flags; `None` for positional arguments.
    pub long: Option<String>,
    pub kind: FieldKind,
    pub required: bool,
    /// Several values, entered separated by `;`.
    pub multiple: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// Words after `retold`, such as `["video", "continue"]`.
    pub path: Vec<String>,
    /// The last word, such as `Continue`.
    pub title: String,
    /// What it acts on, such as `Video` or `Config profile`.
    pub group: String,
    pub about: String,
    pub notes: String,
    pub section: Section,
    pub risk: Risk,
    pub fields: Vec<Field>,
}

impl Action {
    #[must_use]
    pub fn command(&self) -> String {
        self.path.join(" ")
    }

    /// Actions with nothing to fill in run as soon as they are chosen.
    #[must_use]
    pub fn runs_immediately(&self) -> bool {
        self.risk == Risk::Read && self.fields.iter().all(|field| !field.required)
    }
}

/// All runnable commands, in command-tree order.
#[must_use]
pub fn actions() -> Vec<Action> {
    collect(&crate::command(), &[])
}

/// The command's own action, when it runs by itself, followed by its children's.
fn collect(command: &Command, path: &[String]) -> Vec<Action> {
    let children = visible_children(command);
    let runs_alone = children.is_empty() || !command.is_subcommand_required_set();
    let named = !path.is_empty();
    let own = (named && runs_alone).then(|| action(command, path));
    let nested = children
        .into_iter()
        .flat_map(|child| collect(child, &extended(path, child.get_name())));
    own.into_iter().chain(nested).collect()
}

fn visible_children(command: &Command) -> Vec<&Command> {
    command
        .get_subcommands()
        .filter(|child| !HIDDEN_COMMANDS.contains(&child.get_name()))
        .collect()
}

fn extended(path: &[String], name: &str) -> Vec<String> {
    [path, &[name.to_owned()]].concat()
}

fn text_of(styled: Option<&clap::builder::StyledStr>) -> String {
    styled.map(ToString::to_string).unwrap_or_default()
}

fn action(command: &Command, path: &[String]) -> Action {
    Action {
        path: path.to_vec(),
        title: title(path),
        group: group(path),
        about: text_of(command.get_about()),
        notes: text_of(command.get_after_help()),
        section: section(path),
        risk: risk(path),
        fields: command.get_arguments().filter_map(field).collect(),
    }
}

fn field(arg: &Arg) -> Option<Field> {
    let id = arg.get_id().as_str();
    let hidden = GLOBAL_ARGS.contains(&id) || arg.is_hide_set();
    (!hidden).then(|| Field {
        id: id.to_owned(),
        label: humanize(id),
        help: text_of(arg.get_help()),
        long: arg.get_long().map(str::to_owned),
        kind: kind(arg),
        required: arg.is_required_set(),
        multiple: multiple(arg),
        default: default_value(arg),
    })
}

fn kind(arg: &Arg) -> FieldKind {
    let choices: Vec<String> = arg
        .get_possible_values()
        .iter()
        .map(|value| value.get_name().to_owned())
        .collect();
    let takes_value = arg.get_action().takes_values();
    match (takes_value, choices.is_empty()) {
        (false, _) => FieldKind::Flag,
        (true, true) => FieldKind::Text,
        (true, false) => FieldKind::Choice(choices),
    }
}

fn multiple(arg: &Arg) -> bool {
    let appends = matches!(arg.get_action(), ArgAction::Append);
    let several = arg
        .get_num_args()
        .is_some_and(|range| range.max_values() > 1);
    appends || several
}

fn default_value(arg: &Arg) -> Option<String> {
    let takes_value = arg.get_action().takes_values();
    arg.get_default_values()
        .first()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .filter(|_| takes_value)
}

/// `max-requests` → `Max requests`.
#[must_use]
pub fn humanize(id: &str) -> String {
    let spaced = id.replace(['-', '_'], " ");
    let mut characters = spaced.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// `["video", "edit", "shot"]` → `Shot`; one-word commands such as `spend` use their name.
fn title(path: &[String]) -> String {
    path.last().map_or_else(String::new, |word| humanize(word))
}

/// `["video", "edit", "shot"]` → `Video edit`; one-word commands belong to `Retold`.
fn group(path: &[String]) -> String {
    path.split_last()
        .map(|(_, parents)| parents.join(" "))
        .filter(|parents| !parents.is_empty())
        .map_or_else(|| "Retold".to_owned(), |parents| humanize(&parents))
}

impl Action {
    /// `Video › Continue`, for lists that mix groups.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{} › {}", self.group, self.title)
    }
}

fn section(path: &[String]) -> Section {
    let first = path.first().map_or("", String::as_str);
    let second = path.get(1).map_or("", String::as_str);
    match (first, second) {
        ("new", _) | ("series" | "video", "new") | ("video", "copy") => Section::Create,
        ("video", _) => Section::Videos,
        ("series", _) => Section::Series,
        ("studio" | "queue", _) => Section::Studio,
        ("assets" | "library", _) => Section::Library,
        ("providers", _) => Section::Providers,
        ("account" | "spend", _) => Section::Money,
        ("guides" | "channels" | "export-presets" | "views", _) | ("config", "profile") => {
            Section::Configure
        }
        ("creative", _) => Section::Tools,
        ("home", _) => Section::Home,
        _ => Section::System,
    }
}

const SPENDING: [&str; 4] = [
    "video continue",
    "video export",
    "series run",
    "providers test",
];

const REMOVING: [&str; 10] = [
    "delete",
    "remove",
    "restore",
    "activate",
    "clean",
    "stop",
    "unlock",
    "set-limits",
    "reconcile",
    "migrate",
];

const READING: [&str; 28] = [
    "attach",
    "list",
    "show",
    "plan",
    "costs",
    "review",
    "history",
    "paths",
    "verify",
    "scan",
    "health",
    "status",
    "schema",
    "document",
    "usage",
    "home",
    "explain",
    "check",
    "attempts",
    "processes",
    "licence",
    "uses",
    "validate",
    "completions",
    "man",
    "verify-backup",
    "spend",
    "diagnostics",
];

/// How careful to be: paid work first, then reads, removals, and everything else writes.
fn risk(path: &[String]) -> Risk {
    let listed = |words: &[&str], word: &str| words.contains(&word);
    let joined = path.join(" ");
    let verb = path.last().map_or("", String::as_str);
    let creative = path.first().is_some_and(|first| first == "creative");
    if listed(&SPENDING, &joined) {
        Risk::Spend
    } else if creative || listed(&READING, verb) {
        Risk::Read
    } else if listed(&REMOVING, verb) {
        Risk::Destructive
    } else {
        Risk::Write
    }
}
