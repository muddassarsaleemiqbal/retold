//! Shared configuration command grammar; all writes are application operations.
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::authoring::{self, ChannelOptions, GuideOptions};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn commands(kind: &str) -> Vec<Command> {
    let create = Command::new("create")
        .about("Create a user-owned scaffold or import your configuration")
        .arg(Arg::new("id").required(true));
    let configured = create.args(common_arguments());
    let create = if kind == "guides" {
        configured.args(guide_arguments())
    } else {
        configured.args(channel_arguments())
    };
    vec![
        create,
        document_command(),
        edit_command(),
        Command::new("check")
            .about("Validate schema and identify unfinished configuration")
            .arg(Arg::new("id").required(true)),
        Command::new("schema").about("Print the configuration JSON schema"),
    ]
}

fn common_arguments() -> Vec<Arg> {
    vec![
        Arg::new("name").long("name"),
        Arg::new("from")
            .long("from")
            .value_parser(clap::value_parser!(PathBuf)),
        Arg::new("language")
            .long("language")
            .help("Required for a new scaffold"),
        Arg::new("dry-run")
            .long("dry-run")
            .action(ArgAction::SetTrue),
    ]
}

fn guide_arguments() -> Vec<Arg> {
    let mut arguments = text_arguments(&["purpose", "style", "delivery", "writing", "packaging"]);
    arguments.push(example_argument());
    arguments.push(
        Arg::new("seconds")
            .long("seconds")
            .value_parser(clap::value_parser!(u32)),
    );
    arguments
}

fn channel_arguments() -> Vec<Arg> {
    text_arguments(&[
        "handle",
        "accent",
        "voice-provider",
        "voice-model",
        "voice",
        "font-family",
        "font-style",
    ])
}

fn text_arguments(names: &[&'static str]) -> Vec<Arg> {
    names
        .iter()
        .map(|name| Arg::new(*name).long(*name))
        .collect()
}

pub fn query(config: &Path, kind: &str, args: &ArgMatches) -> Option<Payload> {
    let result = match args.subcommand() {
        Some(("document", arguments)) => {
            authoring::document(config, kind, &required_id(arguments).ok()?)
        }
        Some(("edit", arguments)) => edit_document(config, kind, arguments),
        Some(("schema", _)) => Ok(authoring::schema(kind)),
        Some(("check", arguments)) => authoring::check(config, kind, &text(arguments, "id")?),
        Some(("create", arguments)) if kind == "guides" => create_guide(config, arguments),
        Some(("create", arguments)) => create_channel(config, arguments),
        _ => return None,
    };
    Some(result.unwrap_or_else(Diagnostic::into_payload))
}

fn create_guide(config: &Path, args: &ArgMatches) -> Result<Payload, Diagnostic> {
    let options = GuideOptions {
        id: required_id(args)?,
        name: text(args, "name"),
        from: args.get_one::<PathBuf>("from").cloned(),
        language: text(args, "language"),
        example: text(args, "example"),
        purpose: text(args, "purpose"),
        style: text(args, "style"),
        delivery: text(args, "delivery"),
        writing: text(args, "writing"),
        packaging: text(args, "packaging"),
        seconds: args.get_one::<u32>("seconds").copied(),
        dry_run: args.get_flag("dry-run"),
    };
    authoring::create_guide(config, &options)
}

fn create_channel(config: &Path, args: &ArgMatches) -> Result<Payload, Diagnostic> {
    let options = ChannelOptions {
        id: required_id(args)?,
        name: text(args, "name"),
        from: args.get_one::<PathBuf>("from").cloned(),
        language: text(args, "language"),
        handle: text(args, "handle"),
        accent: text(args, "accent"),
        voice_provider: text(args, "voice-provider"),
        voice_model: text(args, "voice-model"),
        voice: text(args, "voice"),
        font_family: text(args, "font-family"),
        font_style: text(args, "font-style"),
        dry_run: args.get_flag("dry-run"),
    };
    authoring::create_channel(config, &options)
}

fn text(args: &ArgMatches, name: &str) -> Option<String> {
    args.get_one::<String>(name).cloned()
}

fn required_id(args: &ArgMatches) -> Result<String, Diagnostic> {
    text(args, "id")
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Choose an identifier."))
}

fn document_command() -> Command {
    Command::new("document")
        .about("Read structured manifest fields and their review hash")
        .arg(Arg::new("id").required(true))
}

fn edit_command() -> Command {
    Command::new("edit")
        .about("Validate and atomically edit one structured manifest field")
        .args([
            Arg::new("id").required(true),
            Arg::new("field")
                .long("field")
                .required(true)
                .help("Existing field as a JSON pointer, such as /name"),
            Arg::new("value")
                .long("value")
                .required(true)
                .help("New value encoded as JSON"),
            Arg::new("expected-hash")
                .long("expected-hash")
                .required(true)
                .help("Document hash from the reviewed version"),
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue),
        ])
}

fn edit_document(config: &Path, kind: &str, args: &ArgMatches) -> Result<Payload, Diagnostic> {
    let id = required_id(args)?;
    let value = |field| args.get_one::<String>(field).map_or("", String::as_str);
    authoring::edit_document(
        config,
        (kind, &id),
        &authoring::Edit {
            field: value("field"),
            value: value("value"),
            expected_hash: value("expected-hash"),
            dry_run: args.get_flag("dry-run"),
        },
    )
}

fn example_argument() -> Arg {
    let example = Arg::new("example")
        .long("example")
        .value_parser(["story-recap"]);
    example
        .conflicts_with("from")
        .help("Copy a bundled guide example, including its writing sidecars")
}
