//! Route edits preserve unrelated configuration and cannot turn local-only work into cloud work.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use std::path::Path;

use retold_core::Tier;
use retold_engine::provider_routes::{RouteRequest, SuggestRequest, set, show, suggest};
use retold_engine::provider_settings::{AddError, RemoveRequest, remove};

const SOURCE: &str = r#"
[providers.codex]
kind = "codex_cli"
max_parallel = 1
[providers.claude]
kind = "claude_cli"
max_parallel = 1
[providers.claude.tiers]
smart = "sonnet"
[providers.gemini]
kind = "gemini_cli"
max_parallel = 1
[providers.cloud]
kind = "openrouter"
key_env = "OPENROUTER_API_KEY"
max_parallel = 1
[providers.local]
kind = "local_text"
max_parallel = 1
[providers.local.models]
fast = "/models/text.gguf"
[providers.voice]
kind = "local_voice"
max_parallel = 1
[providers.whisper]
kind = "local_whisper"
max_parallel = 1
binary = "/tools/whisper"
model = "/models/whisper.bin"
ffmpeg = "/tools/ffmpeg"
[routing]
"text.smart" = ["codex:smart"]
image = ["codex:default"]
"#;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("providers.toml"), SOURCE).unwrap();
    root
}

const fn request<'a>(root: &'a Path, tier: &'a str, order: &'a str) -> RouteRequest<'a> {
    RouteRequest {
        root,
        tier,
        order,
        dry_run: true,
    }
}

#[test]
fn inspection_and_preview_write_nothing_then_apply_preserves_other_routes_and_providers() {
    let root = fixture();
    let routes = show(root.path()).unwrap();
    assert_eq!(routes["text.smart"], ["codex:smart"]);
    let mut change = request(root.path(), "text.smart", "cloud:vendor/model,codex:smart");
    set(&change).unwrap();
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    change.dry_run = false;
    let result = set(&change).unwrap();
    let backup = result.backup.unwrap();
    assert_eq!(std::fs::read_to_string(backup).unwrap(), SOURCE);
    assert_preserved(root.path());
}

fn assert_preserved(root: &Path) {
    let routes = show(root).unwrap();
    assert_eq!(routes["text.smart"], ["cloud:vendor/model", "codex:smart"]);
    assert_eq!(routes["image"], ["codex:default"]);
    let bytes = std::fs::read_to_string(root.join("providers.toml")).unwrap();
    let actual: retold_providers::ProvidersConfig = toml::from_str(&bytes).unwrap();
    let original: retold_providers::ProvidersConfig = toml::from_str(SOURCE).unwrap();
    assert_eq!(
        serde_json::to_value(actual.providers).unwrap(),
        serde_json::to_value(original.providers).unwrap()
    );
}

#[test]
fn rejects_unknown_duplicate_malformed_and_incompatible_routes() {
    let root = fixture();
    for (tier, order) in [
        ("tts", "codex:smart"),
        ("vision.smart", "gemini:smart"),
        ("text.local", "local:fast,cloud:model"),
        ("image", "claude:smart"),
        ("transcribe", "voice:default"),
        ("text.smart", "missing:smart"),
        ("text.smart", "codex:smart,codex:smart"),
        ("text.smart", "codex"),
        ("text.smart", ""),
        ("unknown", "codex:smart"),
    ] {
        assert!(set(&request(root.path(), tier, order)).is_err());
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join("providers.toml")).unwrap(),
        SOURCE
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn adapter_capability_matrix_matches_the_wired_implementations() {
    let config: retold_providers::ProvidersConfig = toml::from_str(SOURCE).unwrap();
    for &(id, names) in ADAPTER_TIERS {
        let supported: Vec<Tier> = names
            .split_whitespace()
            .map(|name| name.parse().unwrap())
            .collect();
        for tier in Tier::ALL {
            assert_eq!(
                config.providers[id].supports_tier(tier),
                supported.contains(&tier),
                "{id} {tier}"
            );
        }
    }
}

#[test]
fn route_updates_share_the_import_writer_lock() {
    let root = fixture();
    let lock = std::fs::File::create(root.path().join(".providers.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let mut change = request(root.path(), "text.local", "local:fast");
    change.dry_run = false;
    assert!(matches!(set(&change), Err(AddError::Busy)));
    drop(lock);
    set(&change).unwrap();
    let routes = show(root.path()).unwrap();
    assert_eq!(routes["text.local"], ["local:fast"]);
}

#[test]
fn invalid_or_future_configuration_is_never_replaced_or_backed_up() {
    let root = fixture();
    let path = root.path().join("providers.toml");
    for prefix in ["schema_version = 999\n", "unknown = 'PRIVATE-VALUE'\n"] {
        let contents = format!("{prefix}{SOURCE}");
        std::fs::write(&path, &contents).unwrap();
        assert!(show(root.path()).is_err());
        let mut change = request(root.path(), "text.fast", "codex:fast");
        change.dry_run = false;
        let error = set(&change).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE-VALUE"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    }
}

const ADAPTER_TIERS: &[(&str, &str)] = &[
    (
        "codex",
        "text.fast text.smart text.best vision.fast vision.smart image image.no_refs",
    ),
    (
        "claude",
        "text.fast text.smart text.best vision.fast vision.smart",
    ),
    ("gemini", "text.fast text.smart text.best"),
    (
        "cloud",
        "text.fast text.smart text.best vision.fast vision.smart image image.no_refs tts transcribe",
    ),
    ("local", "text.fast text.smart text.best text.local"),
    ("voice", "tts"),
    ("whisper", "transcribe"),
];

#[test]
fn oversized_and_excessively_long_orders_leave_configuration_untouched() {
    const TOO_MANY: usize = 33;
    const TOO_LARGE: usize = 8193;
    let root = fixture();
    let many = (0..TOO_MANY)
        .map(|index| format!("codex:model-{index}"))
        .collect::<Vec<_>>()
        .join(",");
    for order in [many, "x".repeat(TOO_LARGE)] {
        assert!(set(&request(root.path(), "text.smart", &order)).is_err());
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn removal_refuses_references_and_preserves_exact_backup() {
    let root = fixture();
    let mut change = RemoveRequest {
        root: root.path(),
        id: "claude",
        dry_run: true,
    };
    for id in ["codex", "missing", "../claude"] {
        change.id = id;
        assert!(remove(&change).is_err());
    }
    change.id = "claude";
    assert!(remove(&change).unwrap().backup.is_none());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    change.dry_run = false;
    let result = remove(&change).unwrap();
    assert_eq!(
        std::fs::read_to_string(result.backup.unwrap()).unwrap(),
        SOURCE
    );
    assert_removed(root.path());
}

fn assert_removed(root: &Path) {
    let text = std::fs::read_to_string(root.join("providers.toml")).unwrap();
    let actual: retold_providers::ProvidersConfig = toml::from_str(&text).unwrap();
    let mut expected: retold_providers::ProvidersConfig = toml::from_str(SOURCE).unwrap();
    expected.providers.shift_remove("claude");
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

#[test]
fn removal_obeys_writer_lock_and_rejects_future_files() {
    let root = fixture();
    let change = RemoveRequest {
        root: root.path(),
        id: "claude",
        dry_run: false,
    };
    let lock = std::fs::File::create(root.path().join(".providers.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(matches!(remove(&change), Err(AddError::Busy)));
    drop(lock);
    let future = format!("schema_version = 999\n{SOURCE}");
    std::fs::write(root.path().join("providers.toml"), &future).unwrap();
    assert!(remove(&change).is_err());
    assert_eq!(
        std::fs::read_to_string(root.path().join("providers.toml")).unwrap(),
        future
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}

fn suggested(request: SuggestRequest<'_>) -> retold_engine::provider_routes::Suggested {
    suggest(request).unwrap()
}

#[test]
fn suggestion_previews_write_nothing_and_skip_routed_or_paid_tiers() {
    let root = fixture();
    let preview = suggested(SuggestRequest {
        root: root.path(),
        dry_run: true,
    });
    let saved = std::fs::read_to_string(root.path().join("providers.toml")).unwrap();
    assert_eq!(saved, SOURCE);
    let fast = &preview.routes["text.fast"];
    assert_eq!(fast, &["claude:smart", "codex:default", "gemini:default"]);
    assert_eq!(preview.routes["text.local"], ["local:fast"]);
    assert!(!preview.routes.contains_key("text.smart"));
    assert!(!preview.routes.contains_key("tts"));
}

#[test]
fn applied_suggestions_keep_existing_routes_and_back_up_the_file() {
    let root = fixture();
    let applied = suggested(SuggestRequest {
        root: root.path(),
        dry_run: false,
    });
    assert_eq!(
        std::fs::read_to_string(applied.backup.unwrap()).unwrap(),
        SOURCE
    );
    let routes = show(root.path()).unwrap();
    assert_eq!(routes["text.smart"], ["codex:smart"]);
    assert_eq!(routes["image"], ["codex:default"]);
    assert_eq!(routes["vision.smart"], ["codex:default", "claude:smart"]);
}
