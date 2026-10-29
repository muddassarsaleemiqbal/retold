//! Shared descriptions for command arguments used by both CLI help and generated TUI forms.
use clap::{Arg, Command};

pub fn complete(command: Command) -> Command {
    let mut command = command.mut_args(describe);
    for child in command.get_subcommands_mut() {
        *child = complete(std::mem::take(child));
    }
    command
}

fn describe(arg: Arg) -> Arg {
    let described = arg.get_help().is_some();
    if described {
        return arg;
    }
    let help = FIELDS.iter().find(|(id, _)| *id == arg.get_id().as_str());
    match help {
        Some((_, help)) => arg.help(*help),
        None => arg,
    }
}

const FIELDS: &[(&str, &str)] = &[
    ("reference", "Saved video ID, slug, or run directory"),
    (
        "root",
        "Configuration root; omit to use the selected workspace",
    ),
    (
        "dry-run",
        "Preview and validate without saving changes or running work",
    ),
    ("id", "Identifier of the item to inspect or change"),
    ("name", "Name of the item"),
    (
        "yes",
        "Accept the reviewed plan; grants no additional spend or export permission",
    ),
    (
        "spend",
        "Maximum new paid spend in USD for this action; 0 permits no paid requests",
    ),
    ("scope", "Scope to inspect or change"),
    (
        "plan-hash",
        "Hash of the reviewed plan; execution stops if the plan has changed",
    ),
    ("limit", "Maximum number of records to return"),
    ("kind", "Kind of resource or content"),
    ("title", "Human-readable title"),
    (
        "output",
        "Destination path for the generated file or directory",
    ),
    (
        "max-requests",
        "Maximum new provider requests allowed for this action",
    ),
    ("lang", "Language or edition identifier, such as en or hi"),
    ("archive", "Include archived items"),
    ("year", "Release year, such as 2024"),
    (
        "until",
        "Last pipeline stage to run; continuing does not grant export permission",
    ),
    ("status", "Filter by recorded status"),
    ("source-file", "Path to a UTF-8 source document"),
    (
        "reconcile",
        "Settle pending charges using the recorded provider costs",
    ),
    ("quality", "Production quality profile"),
    ("license", "License covering use of this asset"),
    (
        "idempotency-key",
        "Stable key for this submission; reuse it when retrying the same work",
    ),
    ("from", "Starting point or source item"),
    ("file", "Path to the input file"),
    ("edition", "Language edition to inspect or export"),
    ("cost", "Confirmed cost in USD for this attempt"),
    (
        "background",
        "Submit work to the studio and return without waiting for completion",
    ),
    ("attribution", "Credit text required by the asset license"),
    ("aspect", "Output aspect ratio"),
    ("after", "Continue pagination after this cursor"),
    ("writing", "Writing preference for this channel"),
    ("window", "When the limit starts counting again"),
    ("voice-provider", "Configured provider used for speech"),
    ("voice-model", "Speech model selected for this voice"),
    ("voice", "Voice identifier or reference"),
    ("view", "Saved library view to apply"),
    ("video", "Select one saved video"),
    ("type", "Resource type"),
    (
        "timezone",
        "IANA time zone for accounting periods, such as Asia/Karachi",
    ),
    ("tier", "Quality tier to inspect or use"),
    ("text", "Text to inspect or process"),
    ("take", "Recorded take to select"),
    ("tag", "Tag to apply or filter by"),
    ("style", "Visual or creative style preference"),
    ("state", "State to report or reconcile"),
    ("stale", "Include stale artifacts"),
    ("staging", "Use the staging destination"),
    ("spoken-only", "Include spoken text only"),
    ("sort", "Sort order for the results"),
    ("seconds", "Duration in seconds"),
    ("search", "Text to match in the library"),
    ("requests", "Provider request limit"),
    (
        "request",
        "The provider request to correct (from the cost list)",
    ),
    ("release", "Release a recorded hold"),
    ("purpose", "Purpose of this artifact or resource"),
    (
        "project-only",
        "Create the editable project without rendering a video",
    ),
    ("profile-dir", "Directory for this provider login profile"),
    ("preset", "Named export preset"),
    ("pending", "Include pending work"),
    ("path", "Filesystem path to inspect or use"),
    ("packaging", "Packaging preference"),
    (
        "order",
        "Ordered provider selectors, such as claude:smart,codex:smart",
    ),
    ("look", "Visual treatment for this channel"),
    (
        "key-env",
        "Environment variable containing the API key; enter its name, not the secret",
    ),
    ("into", "Destination directory for the restored workspace"),
    ("handle", "Public channel handle"),
    ("format", "Output format"),
    ("font-style", "Font style, such as Regular or Bold"),
    ("font-family", "Installed font family"),
    ("failed", "Include failed work"),
    ("delivery", "Delivery preference"),
    ("cursor", "Pagination cursor returned by the previous page"),
    ("code", "Diagnostic code, such as RG-INPUT-REQUIRED"),
    ("characters", "Character or cast selection"),
    ("channel", "Installed channel profile"),
    (
        "attest-prior-stopped",
        "Confirm the prior installation is stopped before transferring execution",
    ),
    ("attempt", "Recorded provider attempt identifier"),
    ("all", "Select all eligible items"),
    ("action", "Action to perform"),
    ("accent", "Accent preference for speech"),
    ("json", "Emit machine-readable JSON"),
    ("plain", "Use noninteractive text output"),
    ("quiet", "Suppress progress; keep the terminal result"),
    ("no-color", "Disable ANSI colors"),
    ("library", "Library root to use for this command"),
    ("workspace", "Workspace root to use for this command"),
];
