//! Local posting metadata and character cards use the production workflow, validation
//! and durable cache.
#![cfg(all(test, unix))]
#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared support module"
)]
mod support {
    include!("support/mod.rs");
}

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use retold_core::{Tier, Usd};
use retold_engine::{
    context::Context,
    pipeline::Until,
    workflow::{self, Options},
};
use retold_guide::{Guide, Origin};
use retold_providers::{Router, TextProvider, local_text::LocalText, process::Limits};

/// A fresh zero-budget run whose `role` (with its current tier line) routes to the local model.
fn local_context(
    model: &Path,
    binary: &Path,
    role: &str,
) -> (tempfile::TempDir, Arc<Context>, Options, Arc<Context>) {
    let (tmp, original, text, _) = support::setup(&["en"], Usd::ZERO);
    let mut ctx = (*original).clone();
    let mut files = ctx.guide.files().clone();
    let manifest = files.get_mut("guide.toml").unwrap();
    let line = manifest
        .lines()
        .find(|l| l.starts_with(&format!("{role} = ")))
        .unwrap()
        .to_owned();
    *manifest = manifest.replace(&line, &format!("{role} = \"text.local\""));
    ctx.guide = Guide::from_files(Origin::Inline, files).unwrap();
    let source = original
        .layout
        .stage(retold_engine::Stage::Research)
        .join("source-file.txt");
    let (layout, meta) = retold_engine::run::create(
        &tmp.path().join("offline-library"),
        retold_engine::run::NewRun {
            guide: &ctx.guide,
            channel: &ctx.channel,
            inputs: ctx.meta.inputs.clone(),
            variants: ctx.meta.variants.clone(),
            quality: ctx.meta.quality,
            budget: Usd::ZERO,
            skip_reviews: true,
            source_file: Some(&source),
        },
    )
    .unwrap();
    ctx.media.store = Arc::new(tokio::sync::Mutex::new(
        retold_engine::store::Store::open(&layout).unwrap(),
    ));
    ctx.media.ledger = Arc::new(
        retold_engine::ledger::Ledger::open(&layout.meta().join("ledger.json"), Usd::ZERO).unwrap(),
    );
    ctx.layout = layout;
    ctx.meta = meta;
    let local = Arc::new(
        LocalText::new(
            "offline",
            binary.to_owned(),
            &IndexMap::from([("metadata".into(), model.to_owned())]),
            Limits {
                timeout: Duration::from_secs(180),
                ..Limits::default()
            },
            2048,
            8192,
        )
        .unwrap(),
    );
    let mut routes = ctx.media.registry.config.routing.clone();
    routes.insert(Tier::TextLocal, vec!["offline:metadata".parse().unwrap()]);
    ctx.text = Arc::new(Router::from_providers(
        vec![(text as Arc<dyn TextProvider>, 2), (local, 1)],
        routes,
    ));
    let options = Options {
        fonts: Some(Arc::new(support::EstimatedFonts)),
        ..Options::default()
    };
    (tmp, Arc::new(ctx), options, original)
}

async fn exercise(
    model: &Path,
    binary: &Path,
    expected: Result<Option<&str>, &str>,
) -> tempfile::TempDir {
    let (tmp, ctx, options, original) = local_context(model, binary, "package");
    assert_eq!(ctx.guide.manifest().roles.package, Tier::TextLocal);
    let result = workflow::run(Arc::clone(&ctx), Until::Ready, &options).await;
    let expected_title = match expected {
        Ok(title) => {
            result.unwrap();
            title
        }
        Err(fragment) => {
            assert!(result.unwrap_err().to_string().contains(fragment));
            assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
            return tmp;
        }
    };
    let post_dir = ctx.layout.post_dir(&"en".parse().unwrap());
    let posts: Vec<_> = std::fs::read_dir(&post_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    assert_eq!(posts.len(), 4);
    for path in posts {
        let post: retold_engine::languages::Post =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        if let Some(title) = expected_title {
            assert_eq!(post.title, title);
        }
        assert_eq!(post.hashtags.len(), 5);
    }
    // Disable every text provider and prove warm reuse from the durable cache.
    let cache = std::fs::read_dir(ctx.layout.meta().join("cache"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("package-en-")
        })
        .unwrap();
    let before = std::fs::read(&cache).unwrap();
    let mut resumed: Context = (*ctx).clone();
    resumed.text = Arc::new(Router::from_providers(vec![], IndexMap::new()));
    drop(ctx);
    drop(original);
    workflow::run(Arc::new(resumed), Until::Ready, &options)
        .await
        .unwrap();
    assert_eq!(before, std::fs::read(&cache).unwrap());
    tmp
}

#[tokio::test]
async fn local_metadata_reaches_ready_and_survives_warm_resume() {
    let fixture = tempfile::tempdir().unwrap();
    let model = fixture.path().join("test.gguf");
    std::fs::write(&model, b"test fixture").unwrap();
    let binary = fixture.path().join("completion");
    std::fs::write(&binary,r"#!/usr/bin/env python3
import json
print(json.dumps({'posts':[{'platform':p,'title':'Local metadata','description':'A local description.','hashtags':['#One','#Two','#Three','#Four','#Five']} for p in ['youtube','tiktok','instagram','facebook']]}))
").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    exercise(&model, &binary, Ok(Some("Local metadata"))).await;
}

#[tokio::test]
async fn local_metadata_still_rejects_invalid_platforms_and_hashtags() {
    let fixture = tempfile::tempdir().unwrap();
    let model = fixture.path().join("test.gguf");
    std::fs::write(&model, b"test fixture").unwrap();
    let binary = fixture.path().join("completion");
    std::fs::write(&binary,r"#!/usr/bin/env python3
import json
print(json.dumps({'posts':[{'platform':'unknown','title':'Bad metadata','description':'A description.','hashtags':['#Same']*5}]}))
").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    exercise(
        &model,
        &binary,
        Err("posting metadata failed platform/title/hashtag checks"),
    )
    .await;
}

#[tokio::test]
async fn local_failure_cannot_enter_paid_fallback_even_with_permission() {
    let (_tmp, ctx, _, _) = support::setup(&["en"], Usd::from_micros(1_000_000));
    let result = ctx
        .ask::<retold_engine::languages::Packaging>(
            "package-local-failure",
            Tier::TextLocal,
            "Write metadata".into(),
            vec![],
        )
        .await;
    assert!(result.is_err());
    assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
    assert!(
        !ctx.media
            .store
            .lock()
            .await
            .state
            .receipts
            .contains_key("package-local-failure")
    );
    let paid = retold_engine::paid_text::PaidText {
        registry: Arc::clone(&ctx.media.registry),
        ledger: Arc::clone(&ctx.media.ledger),
        allow_paid: true,
    };
    let req = retold_providers::TextRequest {
        system: None,
        prompt: "metadata".into(),
        schema: serde_json::json!({}),
        images: vec![],
        web: false,
    };
    let error = paid
        .generate::<serde_json::Value>(
            &ctx.text,
            Tier::TextLocal,
            ("test", "key"),
            &req,
            &ctx.media.cancel,
        )
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("text.local forbids paid fallback")
    );
}

// Acceptance: requires RETOLD_LOCAL_TEXT_MODEL and an installed llama-completion; no cloud or video export
#[cfg(feature = "acceptance")]
#[tokio::test]
async fn installed_model_generates_valid_posting_metadata() {
    let model = std::env::var_os("RETOLD_LOCAL_TEXT_MODEL").expect("set RETOLD_LOCAL_TEXT_MODEL");
    let tmp = exercise(Path::new(&model), Path::new("llama-completion"), Ok(None)).await;
    if std::env::var_os("RETOLD_KEEP_FIXTURE").is_some() {
        eprintln!("Local metadata evidence: {}", tmp.keep().display());
    }
}

/// Runs the workflow with character design on the local model; returns the cast cards.
async fn design(binary: &Path, model: &Path) -> Result<serde_json::Value, String> {
    let (tmp, ctx, options, _original) = local_context(model, binary, "design");
    assert_eq!(ctx.guide.manifest().roles.design, Tier::TextLocal);
    let result = workflow::run(Arc::clone(&ctx), Until::Ready, &options).await;
    assert_eq!(ctx.media.ledger.total().unwrap(), Usd::ZERO);
    if let Err(e) = result {
        return Err(e.to_string());
    }
    let receipts = ctx.media.store.lock().await.state.receipts.clone();
    let receipt = receipts
        .iter()
        .find(|(k, _)| k.starts_with("design-cards-"))
        .map(|(_, r)| r.clone())
        .unwrap();
    assert_eq!(receipt.provider, "offline");
    let cards: serde_json::Value = serde_json::from_slice(
        &std::fs::read(ctx.layout.cast_dir().join("characters.json")).unwrap(),
    )
    .unwrap();
    if std::env::var_os("RETOLD_KEEP_FIXTURE").is_some() {
        eprintln!("Local design evidence: {}", tmp.keep().display());
    }
    Ok(cards)
}

fn fake_model(script: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let fixture = tempfile::tempdir().unwrap();
    let model = fixture.path().join("test.gguf");
    std::fs::write(&model, b"test fixture").unwrap();
    let binary = fixture.path().join("completion");
    std::fs::write(&binary, script).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    (fixture, model, binary)
}

const NAMES: &str = r"import json, pathlib, sys
a = sys.argv
prompt = pathlib.Path(a[a.index('--file') + 1]).read_text()
start = prompt.index('names exactly once: ') + len('names exactly once: ')
names = json.JSONDecoder().raw_decode(prompt[start:])[0]
";

#[tokio::test]
async fn local_character_cards_drive_the_cast() {
    let (_f, model, binary) = fake_model(&format!(
        "#!/usr/bin/env python3\n{NAMES}print(json.dumps({{'characters':[{{'name':n,'appearance':'Round face, short dark hair','clothes':'Grey wool coat','palette':['#%06x' % (sum(map(ord, n)) * 7919 % 0xffffff),'#EEDDCC'],'distinguishing_detail':'A brass pocket watch','age':'about forty','build':'7.5 heads tall, broad, relaxed','face':'Round head, dot eyes, heavy brows','hair':'Short dark cropped hair','voice':'male, about forty, gravelly','colors':[{{'part':'skin','hex':'#E0B894'}},{{'part':'coat','hex':'#%06x' % (sum(map(ord, n)) * 7919 % 0xffffff)}},{{'part':'watch','hex':'#C9A54A'}}]}} for n in names]}}))\n"
    ));
    let cards = design(&binary, &model).await.unwrap();
    let cards = cards.as_object().unwrap();
    assert!(!cards.is_empty());
    assert!(cards.values().all(|c| c["clothes"] == "Grey wool coat"));
}

#[tokio::test]
async fn local_character_cards_must_cover_the_cast() {
    // Drops a character and leaves a field empty: the existing check must reject it.
    let (_f, model, binary) = fake_model(&format!(
        "#!/usr/bin/env python3\n{NAMES}print(json.dumps({{'characters':[{{'name':n,'appearance':'','clothes':'Coat','palette':[],'distinguishing_detail':'Scar'}} for n in names[1:] or ['Nobody']]}}))\n"
    ));
    let error = design(&binary, &model).await.unwrap_err();
    assert!(
        error.contains("character cards do not cover exactly the requested cast"),
        "{error}"
    );
}

// Acceptance: requires RETOLD_LOCAL_TEXT_MODEL and an installed llama-completion; no cloud or video export
#[cfg(feature = "acceptance")]
#[tokio::test]
async fn installed_model_designs_valid_character_cards() {
    let model = std::env::var_os("RETOLD_LOCAL_TEXT_MODEL").expect("set RETOLD_LOCAL_TEXT_MODEL");
    let cards = design(Path::new("llama-completion"), Path::new(&model))
        .await
        .unwrap();
    eprintln!("{}", serde_json::to_string_pretty(&cards).unwrap());
}
