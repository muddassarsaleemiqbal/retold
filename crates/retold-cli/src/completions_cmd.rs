//! Shell completions generation for bash, elvish, fish, powershell, and zsh.

use clap::{Arg, ArgMatches, Command};
use clap_complete::Shell;
use retold_app::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

const SHELL_ENTRIES: [(&str, Shell); 5] = [
    ("bash", Shell::Bash),
    ("elvish", Shell::Elvish),
    ("fish", Shell::Fish),
    ("powershell", Shell::PowerShell),
    ("zsh", Shell::Zsh),
];

pub fn command() -> Command {
    Command::new("completions")
        .about("Generate shell completion scripts to stdout")
        .arg(
            Arg::new("shell")
                .required(true)
                .value_parser(["bash", "elvish", "fish", "powershell", "zsh"])
                .help("Target shell name"),
        )
}

#[must_use]
pub fn query(args: &ArgMatches, base_cmd: Command) -> Payload {
    let Some(shell_name) = args.get_one::<String>("shell") else {
        return Diagnostic::new(ErrorCode::InputRequired, "Shell name is required.").into_payload();
    };
    let Some(shell) = parse_shell(shell_name) else {
        return Diagnostic::new(ErrorCode::InputRequired, "Unsupported shell.").into_payload();
    };
    let script = generate_script(shell, base_cmd);
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "shell": shell_name,
            "text": script,
        }),
    }
}

fn parse_shell(name: &str) -> Option<Shell> {
    for (entry_name, shell) in SHELL_ENTRIES {
        if entry_name == name {
            return Some(shell);
        }
    }
    None
}

fn generate_script(shell: Shell, mut cmd: Command) -> String {
    let mut buffer = Vec::new();
    clap_complete::generate(shell, &mut cmd, "retold", &mut buffer);
    String::from_utf8_lossy(&buffer).into_owned()
}
