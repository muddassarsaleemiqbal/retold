//! Setup resumes from persisted application state; skipped steps remain available.
use super::{dashboard, store::Store};

pub const STEPS: [(&str, &str); 9] = [
    ("Create a workspace", "workspace init"),
    ("Select the workspace", "workspace switch"),
    ("Add a provider", "providers add"),
    ("Route tasks to providers", "providers route"),
    ("Check provider login", "providers login"),
    ("Set up spending", "account init"),
    ("Create a guide", "guides create"),
    ("Create a channel", "channels create"),
    ("Create your first video", "new"),
];

pub fn completed(store: &Store, title: &str) -> bool {
    dashboard::rows(store)
        .iter()
        .any(|row| row.label == title && row.done == Some(true))
}

pub fn first_pending(store: &Store) -> usize {
    STEPS
        .iter()
        .position(|(title, _)| !completed(store, title))
        .unwrap_or_default()
}
