//! Remaining workbench contracts share the production reducer and command validation.
use super::*;
use crate::tui::{defaults, view};
const ACCOUNT_TAB: usize = 3;
const DELIVER_TAB: usize = 7;
const THIRDS: u16 = 3;
use ratatui::{
    crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind},
    layout::{Position, Rect},
};
fn form(command: &str) -> Form {
    Form::new(
        catalog::actions()
            .into_iter()
            .find(|action| action.command() == command)
            .unwrap(),
    )
}
#[test]
fn remembered_choices_keep_each_pair_and_all_editions_without_execution_authority() {
    let first = form("new")
        .with("guide", "guide-one")
        .with("channel", "studio");
    let first = first
        .with("lang", "hi;ur")
        .with("quality", "lean")
        .with("budget", "3");
    let first = first.with("work", "Old title");
    let second = form("new")
        .with("guide", "guide-two")
        .with("channel", "studio")
        .with("quality", "premium");
    let saved = defaults::remember(&defaults::remember(&json!({}), &first), &second);
    let restored = defaults::fill(
        form("new")
            .with("guide", "guide-one")
            .with("channel", "studio"),
        &saved,
    );
    assert_eq!(restored.value("quality").as_deref(), Some("lean"));
    assert_eq!(restored.list_values("lang"), vec!["hi", "ur"]);
    assert_eq!(restored.value("work"), None);
    assert_eq!(restored.value("spend"), None);
    let explicit = defaults::fill(restored.with("quality", "standard"), &saved);
    assert_eq!(explicit.value("quality").as_deref(), Some("standard"));
}
#[test]
fn activation_runs_only_after_loading_a_reviewed_hash() {
    let original = form("storage activate").with("root", "/restored");
    assert!(original.review_problem().is_none());
    let preview = crate::tui::preview::Preview::new(&original).unwrap();
    assert!(preview.executable(&json!({})).is_none());
    let executable = preview
        .executable(&json!({"activation_plan":{"plan_hash":"exact-hash"}}))
        .unwrap();
    assert_eq!(executable.value("plan-hash").as_deref(), Some("exact-hash"));
    assert_eq!(executable.value("dry-run").as_deref(), Some("false"));
    assert!(executable.validation_problem().is_none());
}
#[test]
fn bulk_tags_review_exact_targets_and_cancel_preserves_selection() {
    let mut app = fresh();
    app.go(section::Section::Videos);
    app.store.fill(
        words("video list"),
        result(json!({"items":[{"ref":"one","title":"One"}]})),
        0,
    );
    app.press(key(KeyCode::Char(' ')));
    app.press(key(KeyCode::Char('t')));
    let mut app = type_text(app, "needs review");
    app.press(key(KeyCode::Enter));
    let Some(Modal::Batch(batch)) = &app.modal else {
        panic!("tag review")
    };
    assert_eq!(batch.forms[0].value("add").as_deref(), Some("needs review"));
    assert_eq!(batch.forms[0].value("reference").as_deref(), Some("one"));
    app.press(key(KeyCode::Esc));
    assert_eq!(app.selected_videos.len(), 1);
    assert_eq!(app.jobs(), Vec::new());
}
#[test]
fn account_shortcuts_cannot_dispatch_storage_or_doctor_actions() {
    let mut app = fresh();
    app.go(section::Section::Money);
    app.click_tab(ACCOUNT_TAB);
    app.press(key(KeyCode::Char('f')));
    assert!(app.page().is_none());
    app.press(key(KeyCode::Char('b')));
    assert!(
        matches!(app.page(),Some(Page::Form(form)) if form.action.command() == "account backup")
    );
    assert_eq!(app.jobs(), Vec::new());
}
#[test]
fn failed_review_filter_uses_saved_status() {
    let filter = crate::tui::review_filter::Filter::Failed;
    assert!(filter.includes(&json!({"status":"invalid"}), &[]));
    assert!(!filter.includes(&json!({"status":"ok"}), &[]));
}
fn click(position: Position) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: position.x,
        row: position.y,
        modifiers: KeyModifiers::NONE,
    })
}
#[test]
fn mouse_confirm_and_cancel_share_the_keyboard_confirmation_gate() {
    for confirm in [false, true] {
        let mut app = fresh();
        app.area = Rect::new(0, 0, NORMAL.0, NORMAL.1);
        app.modal = Some(Modal::Confirm(
            form("guides create")
                .with("id", "science")
                .with("language", "en"),
        ));
        let target = if confirm {
            view::ModalHit::Confirm
        } else {
            view::ModalHit::Cancel
        };
        let position = modal_button(&app, target);
        app.react(&click(position));
        assert_eq!(app.jobs().len(), usize::from(confirm));
    }
}
fn modal_button(app: &App, target: view::ModalHit) -> Position {
    for row in 0..app.area.height {
        for column in [app.area.width / THIRDS, app.area.width * 2 / THIRDS] {
            let position = Position::new(column, row);
            let hit = view::modal_hit(app, position);
            let matched = std::mem::discriminant(&target)
                == std::mem::discriminant(&hit.unwrap_or(view::ModalHit::Row(0)));
            if matched {
                return position;
            }
        }
    }
    panic!("visible confirmation button");
}
#[test]
fn delivery_matrix_and_creative_results_render_without_raw_payloads() {
    let mut app = fresh();
    app.open_video("story");
    for _ in 0..DELIVER_TAB {
        app.press(key(KeyCode::Tab));
    }
    app.store.fill(
        words("video paths --kind delivery story"),
        result(json!({"deliveries":[{"id":"en-wide","edition":"en","format":"16x9"}]})),
        0,
    );
    for size in [COMPACT, NORMAL, WIDE] {
        assert!(screen(&app, size.0, size.1).contains("en · 16x9 · ready"));
    }
    app.press(key(KeyCode::Esc));
    app.open_path(
        "creative audit",
        &vec![("text".into(), "A short scene".into())],
    );
    app.store.fill(
        form("creative audit")
            .with("text", "A short scene")
            .arguments(),
        result(json!({"audit":{"script_score":90}})),
        0,
    );
    assert!(screen(&app, NORMAL.0, NORMAL.1).contains("Creative findings"));
}

#[test]
fn remembered_choices_survive_restart_and_remain_scoped_to_the_workspace() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let draft = form("new")
        .with("guide", "guide-one")
        .with("quality", "lean");
    let saved = defaults::remember(&json!({"pairs":[]}), &draft);
    defaults::save(Some(first.path()), &saved.to_string()).unwrap();
    let loaded = defaults::load(Some(first.path())).unwrap();
    assert_eq!(
        defaults::fill(form("new"), &loaded)
            .value("quality")
            .as_deref(),
        Some("lean")
    );
    assert_eq!(defaults::load(Some(second.path())).unwrap(), json!({}));
}

#[test]
fn comparison_images_render_two_saved_previews_and_drop_stale_worker_results() {
    let mut app = super::review::review();
    app.theme = Theme::Default;
    app.store.fill(
        words("video paths story"),
        result(json!({"paths":{"root":"/saved"}})),
        0,
    );
    app.press(key(KeyCode::Char('v')));
    let image = result(json!({"image":{"width":1,"height":1,"pixels":[255,0,0]}}));
    app.receive(
        crate::tui::runtime::Job::Local {
            id: u64::MAX,
            task: crate::tui::files::Task::Preview("/saved/frames/one.png".into()),
        },
        image.clone(),
    );
    app.media.preview = Some(image);
    app.press(key(KeyCode::Down));
    app.media.preview = Some(result(
        json!({"image":{"width":1,"height":1,"pixels":[0,255,0]}}),
    ));
    assert!(screen(&app, NORMAL.0, NORMAL.1).contains("Baseline"));
    app.press(key(KeyCode::Char('v')));
    assert!(app.media.comparison.is_none());
}

#[test]
fn switching_guide_replaces_remembered_defaults_and_preserves_explicit_edits() {
    let first = form("new").with("guide", "one").with("channel", "studio");
    let first = first.with("lang", "hi;ur").with("quality", "lean");
    let second = form("new").with("guide", "two").with("channel", "studio");
    let second = second.with("lang", "en").with("quality", "premium");
    let saved = defaults::remember(&defaults::remember(&json!({}), &first), &second);
    let switched = defaults::choose(&first, ("guide", "two"), &saved);
    assert_eq!(switched.list_values("lang"), vec!["en"]);
    assert_eq!(switched.value("quality").as_deref(), Some("premium"));
    let edited = first.with("quality", "standard");
    let switched = defaults::choose(&edited, ("guide", "two"), &saved);
    assert_eq!(switched.value("quality").as_deref(), Some("standard"));
}

#[test]
fn draft_confirmation_summarizes_choices_without_dispatch() {
    let mut app = fresh();
    let draft = form("new")
        .with("guide", "explain")
        .with("channel", "science");
    let draft = draft.with("work", "Clouds").with("lang", "en;ur");
    app.modal = Some(Modal::Preview(
        crate::tui::preview::Preview::new(&draft).unwrap(),
    ));
    for size in [COMPACT, NORMAL, WIDE] {
        let rendered = screen(&app, size.0, size.1);
        assert!(rendered.contains("Review new draft"));
        assert!(rendered.contains("Editions: en, ur"));
        assert!(rendered.contains("Guide: explain"));
    }
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn draft_review_lists_the_providers_each_tier_uses_and_flags_unrouted_ones() {
    let mut app = fresh();
    let draft = form("new").with("guide", "explain").with("work", "Clouds");
    let preview = crate::tui::preview::Preview::new(&draft).unwrap();
    app.store.fill(
        preview.words.clone(),
        result(json!({
            "text":"Would create a video.", "dry_run":true,
            "routes":[
                {"role":"plan", "tier":"text.smart", "chain":["claude:smart", "codex:smart"]},
                {"role":"write", "tier":"text.smart", "chain":["claude:smart", "codex:smart"]},
                {"role":"image", "tier":"image.standard", "chain":[]}
            ]
        })),
        0,
    );
    app.modal = Some(Modal::Preview(preview));
    let rendered = screen(&app, WIDE.0, WIDE.1);
    assert!(rendered.contains("Providers used"), "{rendered}");
    assert!(
        rendered.contains("text.smart → claude:smart › codex:smart (plan, write)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("image.standard → not routed (image)"),
        "{rendered}"
    );
}

#[test]
fn backup_tab_keeps_actions_scoped_to_account() {
    use crate::tui::{
        landing::{self, Tab},
        report,
    };
    for report in [&report::BACKUPS] {
        let mut app = fresh();
        app.go(section::Section::Money);
        let tabs = landing::tabs(section::Section::Money);
        let index = tabs
            .iter()
            .position(|tab| *tab == Tab::Report(report))
            .unwrap();
        app.click_tab(index);
        app.store
            .fill(words(report.command), result(json!({"processes":[]})), 0);
        for size in [COMPACT, NORMAL, WIDE] {
            assert!(screen(&app, size.0, size.1).contains(report.title));
        }
        app.press(key(KeyCode::Char('b')));
        assert!(
            matches!(app.page(), Some(Page::Form(form)) if form.action.command() == "account backup")
        );
        assert_eq!(app.jobs(), Vec::new());
    }
}
#[test]
fn diagnostic_browser_opens_the_exact_selected_code_read_only() {
    use crate::tui::{
        collection,
        landing::{self, Tab},
    };
    let mut app = fresh();
    app.go(section::Section::System);
    let tabs = landing::tabs(section::Section::System);
    let index = tabs
        .iter()
        .position(|tab| *tab == Tab::Collection(&collection::DIAGNOSTICS))
        .unwrap();
    app.click_tab(index);
    app.store
        .fill(words("diagnostics"), retold_app::protocol::diagnostics(), 0);
    app.press(key(KeyCode::Enter));
    assert!(
        matches!(app.page(), Some(Page::Reading(reading)) if reading.words.starts_with(&words("explain RG-ACCOUNT-REQUIRED")))
    );
    assert_eq!(app.jobs(), Vec::new());
}

fn form_app(command: &str) -> App {
    let mut app = fresh();
    app.area = Rect::new(0, 0, NORMAL.0, NORMAL.1);
    app.open_path(command, &Vec::new());
    app
}

fn focused_field(app: &App) -> Option<usize> {
    match app.page() {
        Some(Page::Form(form)) => Some(form.focus),
        _ => None,
    }
}

/// Keyboard focus on the field above scrolls the target into view, then a click focuses it.
fn assert_clickable(command: &str, row: usize, target: usize) {
    let mut app = form_app(command);
    for _ in 1..row {
        app.press(key(KeyCode::Down));
    }
    let Some(Page::Form(form)) = app.page() else {
        unreachable!();
    };
    let form = form.clone();
    let position = (0..app.area.height)
        .map(|y| Position::new(app.area.width / 2, y))
        .find(|position| view::form_focus(&app, &form, *position) == Some(target));
    let position =
        position.unwrap_or_else(|| panic!("{command}: field {target} has no click target"));
    app.react(&click(position));
    assert_eq!(
        focused_field(&app),
        Some(target),
        "{command}: click missed {target}"
    );
    assert_eq!(
        app.jobs(),
        Vec::new(),
        "{command}: a field click dispatched work"
    );
}

/// Every field of every generated form can be focused with a click once it is on screen.
#[test]
fn every_form_field_is_reachable_by_mouse() {
    let mut checked = 0;
    for action in crate::tui::catalog::actions() {
        let command = action.command();
        let app = form_app(&command);
        let Some(Page::Form(form)) = app.page() else {
            continue;
        };
        checked += 1;
        for (row, target) in form.visible_fields().into_iter().enumerate() {
            assert_clickable(&command, row, target);
        }
    }
    assert!(checked >= 60, "only {checked} forms were checked");
}

#[test]
fn palette_rows_select_on_first_click_and_open_on_second() {
    let mut app = fresh();
    app.area = Rect::new(0, 0, NORMAL.0, NORMAL.1);
    app.open_palette();
    assert!(matches!(app.modal, Some(Modal::Palette(_))));
    let position = (0..app.area.height)
        .flat_map(|y| (0..app.area.width).map(move |x| Position::new(x, y)))
        .find(|position| {
            matches!(
                view::modal_hit(&app, *position),
                Some(view::ModalHit::Row(1))
            )
        })
        .expect("visible palette row");
    app.react(&click(position));
    assert!(matches!(&app.modal, Some(Modal::Palette(palette)) if palette.selected == 1));
    app.react(&click(position));
    assert!(!matches!(app.modal, Some(Modal::Palette(_))));
}

/// A browser listing one guide file, opened without a form field to fill.
fn guide_browser() -> crate::tui::files::Browser {
    let field = Option::default();
    let mut browser = crate::tui::files::Browser::new(1, "/guides/explain".into(), field);
    browser.listing = Some(result(json!({"entries":[
        {"name":"guide.toml", "path":"/guides/explain/guide.toml", "directory":false}
    ]})));
    browser
}

#[test]
fn browser_filter_keeps_letters_that_are_also_shortcuts() {
    let mut app = fresh();
    app.modal = Some(Modal::Browser(guide_browser()));
    let app = type_text(app, "go o");
    let Some(Modal::Browser(browser)) = &app.modal else {
        panic!("browser closed");
    };
    assert_eq!(browser.query.text(), "go o");
    let opened = app.jobs().iter().any(|job| {
        matches!(
            job,
            crate::tui::runtime::Job::Local {
                task: crate::tui::files::Task::Open(_) | crate::tui::files::Task::Audio(_),
                ..
            }
        )
    });
    assert!(!opened, "typing a filter opened or played a file");
}

#[test]
fn f6_in_the_file_browser_edits_its_selection() {
    const EDIT_KEY: u8 = 6;
    let mut app = fresh();
    app.modal = Some(Modal::Browser(guide_browser()));
    app.press(key(KeyCode::F(EDIT_KEY)));
    assert!(matches!(
        &app.host.handoff,
        Some(crate::tui::handoff::Handoff::Editor { path, .. }) if path.ends_with("guide.toml")
    ));
}
