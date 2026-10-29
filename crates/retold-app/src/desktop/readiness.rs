//! First-launch readiness: what a workspace still needs before a reviewed run can execute.
//! Every check is offline and read-only; nothing is installed, repaired or contacted.
use std::path::Path;

use crate::protocol::Diagnostic;

/// One prerequisite and whether it is satisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub ready: bool,
    pub detail: String,
}

fn ready(label: &str, detail: impl Into<String>) -> Item {
    Item {
        label: label.into(),
        ready: true,
        detail: detail.into(),
    }
}

fn missing(label: &str, detail: impl Into<String>) -> Item {
    Item {
        label: label.into(),
        ready: false,
        detail: detail.into(),
    }
}

fn tools_and_resources(library: &Path) -> Result<Vec<Item>, Diagnostic> {
    let report = crate::doctor::generate_doctor_report(library)?;
    Ok(report
        .checks
        .into_iter()
        .filter(|check| !check.name.contains("directory"))
        .map(|check| Item {
            ready: check.status == "ok",
            label: check.name,
            detail: check.message,
        })
        .collect())
}

fn providers(config: &Path) -> Item {
    let routes = super::providers::inspect(config)
        .map(|snapshot| snapshot.routing.values().any(|route| !route.is_empty()));
    match routes {
        Ok(true) => ready(
            "Provider routes",
            "At least one task has a configured provider.",
        ),
        Ok(false) => missing(
            "Provider routes",
            "Retold needs an AI to write your script. Use an AI app you already have, or add an API key.",
        ),
        Err(error) => missing("Provider routes", error.message),
    }
}

/// Ready only when the account actually opens, the same test a run plan applies.
fn account() -> Item {
    let opened = crate::account::canonical_directory()
        .and_then(|directory| crate::account::Account::open(&directory));
    match opened {
        Ok(_) => ready(
            "Installation account",
            "Spending limits and records are available.",
        ),
        Err(_) => missing(
            "Installation account",
            "Set a spending limit. Every paid request stops at this cap.",
        ),
    }
}

/// The full checklist: tools, guides and channels, provider routes and the account.
pub fn checklist(library: &Path, config: &Path) -> Result<Vec<Item>, Diagnostic> {
    let mut items = tools_and_resources(library)?;
    items.push(providers(config));
    items.push(account());
    Ok(items)
}
