//! Delayed reads cannot erase refresh requests or authorize stale reviewed changes.
use super::*;

#[test]
fn invalidation_during_a_read_keeps_the_last_visible_value_and_requests_a_new_read() {
    let mut store = Store::default();
    let want = [Want::once("providers list")];
    let command = words("providers list");
    store.fill(command.clone(), result(json!({"version":"visible"})), 0);
    store.mark_loading(std::slice::from_ref(&command));
    store.invalidate();
    store.fill(command.clone(), result(json!({"version":"obsolete"})), 1);
    assert_eq!(store.contents(&command).unwrap()["version"], "visible");
    assert_eq!(store.due(&want, 1), vec![command.clone()]);
    store.mark_loading(std::slice::from_ref(&command));
    store.fill(command.clone(), result(json!({"version":"current"})), 2);
    assert_eq!(store.contents(&command).unwrap()["version"], "current");
    assert_eq!(store.due(&want, 2), Vec::<Vec<String>>::new());
}

#[test]
fn removed_inflight_previews_cannot_reappear_from_an_obsolete_reply() {
    let mut store = Store::default();
    let wants = [Want::once("account limit --dry-run")];
    let command = wants[0].words.clone();
    store.mark_loading(std::slice::from_ref(&command));
    store.remove(&command);
    assert_eq!(store.due(&wants, 0), Vec::<Vec<String>>::new());
    store.fill(command.clone(), result(json!({"old_preview":true})), 1);
    assert!(store.contents(&command).is_none());
    assert_eq!(store.due(&wants, 1), vec![command]);
}

#[test]
fn one_frame_coalesces_duplicate_reads_including_mixed_live_policies() {
    let mut store = Store::default();
    let wants = [Want::once("providers list"), Want::live("providers list")];
    let command = words("providers list");
    assert_eq!(store.due(&wants, 0), vec![command.clone()]);
    store.fill(command.clone(), result(json!({})), 0);
    assert_eq!(
        store.due(&wants, crate::tui::store::REFRESH_MILLIS),
        vec![command]
    );
}

fn preview_app() -> App {
    let form = Form::new(
        catalog::actions()
            .into_iter()
            .find(|action| action.command() == "account reconcile")
            .unwrap(),
    )
    .with("attempt", "a-1")
    .with("cost", "1.25");
    let preview = crate::tui::preview::Preview::new(&form).unwrap();
    let mut app = fresh();
    app.store.fill(
        preview.words.clone(),
        result(json!({"reconcile_preview":{"dry_run":true}})),
        0,
    );
    app.modal = Some(Modal::Preview(preview));
    app
}

fn plan_app() -> App {
    let form = Form::new(
        catalog::actions()
            .into_iter()
            .find(|action| action.command() == "providers test")
            .unwrap(),
    )
    .with("id", "claude")
    .with("spend", "0");
    let review = crate::tui::production::Review::new(form).unwrap();
    let mut app = fresh();
    app.store.fill(
        review.words.clone(),
        result(json!({"test_plan":{
            "plan_hash":"reviewed-hash", "blockers":[], "authority":{}
        }})),
        0,
    );
    app.modal = Some(Modal::Plan(review));
    app
}

#[test]
fn a_refreshing_financial_preview_cannot_dispatch_until_the_current_read_is_reviewed() {
    let mut app = preview_app();
    app.store.invalidate();
    app.press(key(KeyCode::Enter));
    assert_eq!(app.jobs(), Vec::new());
    assert!(matches!(app.modal, Some(Modal::Preview(_))));
    let shown = screen(&app, NORMAL.0, NORMAL.1);
    assert!(shown.contains("Refreshing"), "{shown}");
    assert!(!shown.contains("Enter applies to current state"), "{shown}");
}

#[test]
fn a_refreshing_production_plan_cannot_grant_execution_authority() {
    let mut app = plan_app();
    app.store.invalidate();
    app.press(key(KeyCode::Enter));
    assert_eq!(app.jobs(), Vec::new());
    assert!(matches!(app.modal, Some(Modal::Plan(_))));
    let shown = screen(&app, NORMAL.0, NORMAL.1);
    assert!(shown.contains("Refreshing"), "{shown}");
    assert!(!shown.contains("Run reviewed plan"), "{shown}");
}

#[test]
fn refreshing_reviews_resume_only_after_the_current_response_arrives() {
    let cases = [
        (
            preview_app(),
            result(json!({"reconcile_preview":{"dry_run":true}})),
        ),
        (
            plan_app(),
            result(json!({"test_plan":{
                "plan_hash":"current-hash", "blockers":[], "authority":{}
            }})),
        ),
    ];
    for (mut app, payload) in cases {
        let command = match &app.modal {
            Some(Modal::Preview(preview)) => preview.words.clone(),
            Some(Modal::Plan(review)) => review.words.clone(),
            _ => panic!("review"),
        };
        app.store.mark_loading(std::slice::from_ref(&command));
        app.press(key(KeyCode::Enter));
        assert_eq!(app.jobs(), Vec::new());
        app.receive(crate::tui::runtime::Job::Query(command), payload);
        assert!(!screen(&app, NORMAL.0, NORMAL.1).contains("Refreshing"));
        app.press(key(KeyCode::Enter));
        assert_eq!(app.jobs().len(), 1);
    }
}

#[test]
fn an_invalidated_initial_workspace_read_does_not_start_setup_prematurely() {
    let mut app = App::new(Theme::NoColor);
    let command = words("workspace show");
    app.store.mark_loading(std::slice::from_ref(&command));
    app.store.invalidate();
    app.receive(
        crate::tui::runtime::Job::Query(command.clone()),
        result(json!({"old":true})),
    );
    assert!(
        app.modal.is_none(),
        "an obsolete reply cannot establish missing setup"
    );
    app.store.mark_loading(std::slice::from_ref(&command));
    app.receive(
        crate::tui::runtime::Job::Query(command),
        result(json!({"workspace":{"registered":true}})),
    );
    assert!(app.modal.is_none());
}

#[test]
fn reopening_a_production_review_requests_a_new_plan() {
    let mut app = plan_app();
    let command = match &app.modal {
        Some(Modal::Plan(review)) => review.words.clone(),
        _ => panic!("review"),
    };
    app.press(key(KeyCode::Esc));
    let app = through_every_step(app);
    assert!(matches!(&app.modal, Some(Modal::Plan(_))));
    assert!(
        app.store.contents(&command).is_none(),
        "a reopened review cannot reuse its prior plan"
    );
    assert_eq!(app.jobs(), Vec::new());
}
