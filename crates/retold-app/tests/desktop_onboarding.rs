//! First-launch preparation previews before creating the account and never reads credentials.
//! Re-runs itself with an isolated HOME so the real installation account is never touched.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]

use retold_app::desktop::onboarding;

const CHILD: &str = "RETOLD_DESKTOP_ONBOARDING_CHILD";

fn exercise() {
    let directory = retold_app::account::canonical_directory().unwrap();
    let timezone = onboarding::local_timezone();
    onboarding::account(&timezone).unwrap();
    assert!(!directory.exists());
    onboarding::initialize(&timezone).unwrap();
    assert!(directory.exists());
    assert!(onboarding::initialize(&timezone).is_err());
    let cost = retold_app::account::Usd::from_micros(1);
    assert!(onboarding::preview_reconcile("missing-attempt", cost).is_err());
}

#[test]
fn the_account_is_previewed_then_created_once() {
    if std::env::var_os(CHILD).is_some() {
        exercise();
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let program = std::env::current_exe().unwrap();
    let mut command = std::process::Command::new(program);
    command.args(["--exact", "the_account_is_previewed_then_created_once"]);
    command.env(CHILD, "1");
    command.env("HOME", home.path());
    command.env_remove("XDG_DATA_HOME");
    assert!(command.status().unwrap().success());
}
