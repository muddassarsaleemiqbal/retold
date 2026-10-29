//! Explicit opt-in examples are bundled so installed binaries never need the source checkout.
use crate::protocol::Diagnostic;
use std::collections::BTreeMap;

const STORY_RECAP: [(&str, &str); 6] = [
    (
        "guide.toml",
        include_str!("../../../../docs/examples/story-recap/guide.toml"),
    ),
    (
        "voice.md",
        include_str!("../../../../docs/examples/story-recap/voice.md"),
    ),
    (
        "voice.hi.md",
        include_str!("../../../../docs/examples/story-recap/voice.hi.md"),
    ),
    (
        "voice.ur.md",
        include_str!("../../../../docs/examples/story-recap/voice.ur.md"),
    ),
    (
        "examples/the-shining.md",
        include_str!("../../../../docs/examples/story-recap/examples/the-shining.md"),
    ),
    (
        "examples/the-notebook.md",
        include_str!("../../../../docs/examples/story-recap/examples/the-notebook.md"),
    ),
];

pub(super) fn guide(name: &str) -> Result<BTreeMap<String, String>, Diagnostic> {
    if name != "story-recap" {
        return Err(super::invalid("Choose the bundled story-recap example."));
    }
    Ok(STORY_RECAP
        .into_iter()
        .map(|(path, text)| (path.into(), text.into()))
        .collect())
}
