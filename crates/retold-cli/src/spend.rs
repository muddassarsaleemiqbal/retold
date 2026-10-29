//! Spend reports read the installation account only; they never reconcile or grant authority.
use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::account::report::{self, Grouping, Query};
use retold_app::account::types::Window;
use retold_app::protocol::Payload;

pub fn command() -> Command {
    let window = |name: &'static str, help: &'static str| {
        Arg::new(name)
            .long(name)
            .help(help)
            .action(ArgAction::SetTrue)
    };
    Command::new("spend")
        .about("Report recorded spend and holds without changing the account")
        .args([
            window(
                "today",
                "Only attempts reserved today, in the account timezone",
            )
            .conflicts_with("month"),
            window(
                "month",
                "Only attempts reserved this month, in the account timezone",
            ),
            grouping(),
        ])
        .after_help(
            "Charges belong to the day their request was reserved. Unresolved holds count until \
             `account reconcile` settles them, including holds from earlier days.",
        )
}

fn grouping() -> Arg {
    let by = Arg::new("by").long("by");
    let by = by.help("Group rows by provider account, video, series or reservation day");
    by.value_parser(["provider", "video", "series", "day"])
        .default_value("provider")
}

pub fn query(arguments: &ArgMatches) -> Payload {
    let window = if arguments.get_flag("today") {
        Window::Day
    } else if arguments.get_flag("month") {
        Window::Month
    } else {
        Window::Lifetime
    };
    let by = match arguments.get_one::<String>("by").map(String::as_str) {
        Some("video") => Grouping::Video,
        Some("series") => Grouping::Series,
        Some("day") => Grouping::Day,
        _ => Grouping::Provider,
    };
    report::respond(Query { window, by })
}
