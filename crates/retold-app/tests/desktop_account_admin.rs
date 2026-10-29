//! Native limit changes are previewed, then applied only while the preview is unchanged.
//! The test re-runs itself with an isolated HOME so the real account is never touched.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]

use retold_app::account::types::Window;
use retold_app::desktop::account_admin::{self, LimitEdit};

const REQUEST_CAP: u64 = 10;
const CHILD: &str = "RETOLD_DESKTOP_ACCOUNT_CHILD";

fn edit(usd: &str) -> LimitEdit {
    LimitEdit {
        scope: "account".into(),
        window: Window::Day,
        usd: Some(usd.into()),
        requests: Some(REQUEST_CAP),
        remove: false,
    }
}

fn isolated() {
    let home = tempfile::tempdir().unwrap();
    let program = std::env::current_exe().unwrap();
    let mut command = std::process::Command::new(program);
    command.args([
        "--exact",
        "a_previewed_limit_applies_once_and_a_stale_review_is_refused",
    ]);
    command.env(CHILD, "1");
    command.env("HOME", home.path());
    command.env_remove("XDG_DATA_HOME");
    let status = command.status().unwrap();
    assert!(status.success());
}

fn exercise() {
    let directory = retold_app::account::canonical_directory().unwrap();
    retold_app::account::Account::initialize(&directory, "UTC").unwrap();
    let review = account_admin::review_limit(edit("2.00")).unwrap();
    let stale = account_admin::review_limit(edit("3.00")).unwrap();
    account_admin::apply_limit(&review).unwrap();
    assert!(account_admin::apply_limit(&review).is_err());
    let mut forged = stale;
    forged.preview = serde_json::json!({});
    assert!(account_admin::apply_limit(&forged).is_err());
}

#[test]
fn a_previewed_limit_applies_once_and_a_stale_review_is_refused() {
    if std::env::var_os(CHILD).is_some() {
        exercise();
    } else {
        isolated();
    }
}
