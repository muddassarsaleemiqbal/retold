//! Versioned direction drafts, reviewed impact and one scoped saved-frame player.
pub(super) mod cast;
mod picker;
use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use retold_app::scenes::{self, Document, Draft, ReviewedChange};

use super::{Destination, Shell, player::Player};

pub(super) struct SceneEditor {
    pub library: PathBuf,
    pub reference: String,
    pub document: Option<Document>,
    pub incoming: Option<Document>,
    pub unit: usize,
    pub action: Entity<TextareaState>,
    pub location: Entity<InputState>,
    pub mood: Entity<InputState>,
    pub player: Entity<Player>,
    pub review: Option<ReviewedChange>,
    pub cast: Option<Vec<scenes::CastPose>>,
    pub cast_review: Option<scenes::ReviewedCast>,
    pub error: Option<String>,
    pub history: Option<scenes::HistoryPage>,
    pub history_cursor: Option<scenes::HistoryCursor>,
    pub checkpoint: Option<String>,
    pub frame_available: Option<bool>,
    pub frame_target: Option<retold_app::notes::Target>,
    frame_generation: u64,
    frame_active: bool,
    frame_pending: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy)]
pub(super) enum SceneAction {
    Read,
    Review,
    Save,
    Approve,
    Undo,
    Redo,
    History,
    ReviewCast,
    SaveCast,
}

enum Reply {
    Document(Box<Document>),
    Review(Box<ReviewedChange>),
    CastReview(Box<scenes::ReviewedCast>),
    History(scenes::HistoryPage),
}

impl SceneEditor {
    pub(super) fn new(
        library: PathBuf,
        reference: String,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Self {
        let action = cx.new(|cx| TextareaState::new(window, cx));
        let location = cx.new(|cx| InputState::new(window, cx));
        let mood = cx.new(|cx| InputState::new(window, cx));
        let action_subscription = cx.subscribe(&action, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.scene_input_changed(cx);
            }
        });
        let mut subscriptions = vec![action_subscription];
        for field in [&location, &mood] {
            subscriptions.push(cx.subscribe(field, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.scene_input_changed(cx);
                }
            }));
        }
        Self {
            library,
            reference,
            document: None,
            incoming: None,
            unit: 0,
            action,
            location,
            mood,
            player: cx.new(|_| Player::new()),
            review: None,
            cast: None,
            cast_review: None,
            error: None,
            history: None,
            history_cursor: None,
            checkpoint: retold_app::notes::draft_id().ok(),
            frame_available: None,
            frame_target: None,
            frame_generation: 0,
            frame_active: false,
            frame_pending: false,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn content(&self) -> Option<&scenes::SceneSnapshot> {
        self.document.as_ref()?.content.as_ref()
    }

    pub(super) fn draft(&self, cx: &Context<Shell>) -> Option<Draft> {
        if self.bounds_error(cx).is_some() {
            return None;
        }
        Some(Draft {
            expected_version: self.content()?.version.clone(),
            unit: self.unit,
            action: self.action.read(cx).value().to_string(),
            location: self.location.read(cx).value().to_string(),
            mood: self.mood.read(cx).value().to_string(),
        })
    }

    pub(super) fn bounds_error(&self, cx: &Context<Shell>) -> Option<&'static str> {
        let action = self.action.read(cx).value().len() > scenes::MAX_ACTION_BYTES;
        let location = self.location.read(cx).value().len() > scenes::MAX_LOCATION_BYTES;
        let mood = self.mood.read(cx).value().len() > scenes::MAX_MOOD_BYTES;
        (action || location || mood).then_some("Scene text exceeds its limit (action 4 KiB, location 1 KiB, mood 256 bytes). Your text and the previous checkpoint are kept; shorten or copy it before closing.")
    }

    pub(super) fn dirty(&self, cx: &Context<Shell>) -> bool {
        let saved = self.content().and_then(|content| {
            content
                .board
                .direction
                .shots
                .iter()
                .find(|shot| shot.unit == self.unit)
        });
        let Some(saved) = saved else {
            return false;
        };
        self.cast_dirty()
            || saved.action != self.action.read(cx).value().as_ref()
            || saved.location != self.location.read(cx).value().as_ref()
            || saved.mood != self.mood.read(cx).value().as_ref()
    }

    pub(super) fn load_fields(&self, window: &mut Window, cx: &mut Context<Shell>) {
        let Some(shot) = self.content().and_then(|content| {
            content
                .board
                .direction
                .shots
                .iter()
                .find(|shot| shot.unit == self.unit)
        }) else {
            return;
        };
        let (action, location, mood) = (
            shot.action.clone(),
            shot.location.clone(),
            shot.mood.clone(),
        );
        self.action
            .update(cx, |state, cx| state.set_value(action, window, cx));
        self.location
            .update(cx, |state, cx| state.set_value(location, window, cx));
        self.mood
            .update(cx, |state, cx| state.set_value(mood, window, cx));
    }

    fn refresh_message(&self) -> &'static str {
        if self.incoming.is_some() {
            return "The saved sequence changed. Compare it with your retained direction.";
        }
        match self.content() {
            Some(content) if content.approved => "Saved scene direction is approved.",
            Some(content) if content.revision.is_some() => {
                "Saved manual direction requires approval before production."
            }
            Some(_) => "Saved scene sequence loaded.",
            None => "No saved scene sequence yet. Review a run through Scenes to create it.",
        }
    }

    pub(super) fn adopt_incoming(&mut self) -> bool {
        let exists = self
            .incoming
            .as_ref()
            .and_then(|document| document.content.as_ref())
            .is_some_and(|content| {
                content
                    .board
                    .direction
                    .shots
                    .iter()
                    .any(|shot| shot.unit == self.unit)
            });
        if !exists {
            self.error = Some("This scene is no longer in the latest sequence. Your text is kept; copy it before choosing another scene.".into());
            return false;
        }
        self.document = self.incoming.take();
        self.review = None;
        self.cast_review = None;
        true
    }

    fn receive(
        &mut self,
        document: Document,
        preserve: bool,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) {
        self.error = None;
        self.review = None;
        self.cast_review = None;
        if preserve && self.dirty(cx) {
            self.incoming = Some(document);
            return;
        }
        self.document = Some(document);
        self.incoming = None;
        self.cast = None;
        self.history = None;
        self.history_cursor = None;
        if !self.content().is_some_and(|content| {
            content
                .board
                .direction
                .shots
                .iter()
                .any(|shot| shot.unit == self.unit)
        }) {
            self.unit = self
                .content()
                .and_then(|content| content.board.units.first())
                .map_or(0, |unit| unit.id);
        }
        self.load_fields(window, cx);
    }
}

impl Shell {
    fn scene_input_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.scenes {
            editor.review = None;
        }
        self.checkpoint_scene(cx);
        cx.notify();
    }

    pub(super) fn begin_scenes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        let same = self.scenes.as_ref().is_some_and(|editor| {
            editor.library == snapshot.context.library && editor.reference == *reference
        });
        if !same && self.scenes.as_ref().is_some_and(|editor| editor.dirty(cx)) {
            self.return_to_scenes(cx);
            return;
        }
        if !same {
            self.scenes = Some(SceneEditor::new(
                snapshot.context.library.clone(),
                reference.clone(),
                window,
                cx,
            ));
        }
        self.clear_media(cx);
        self.characters = None;
        self.delivery = None;
        self.show_script = false;
        self.show_notes = false;
        self.show_scenes = true;
        self.scene_action(SceneAction::Read, window, cx);
    }

    pub(super) fn return_to_scenes(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = &self.scenes {
            self.selected_video = Some(editor.reference.clone());
            self.destination = Destination::Videos;
            self.show_notes = false;
            self.show_script = false;
            self.show_scenes = true;
            self.show_lab = false;
        }
        cx.notify();
    }

    pub(super) fn select_scene(
        &mut self,
        unit: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if let Some(editor) = &mut self.scenes {
            if editor.dirty(cx) {
                return;
            }
            editor.unit = unit;
            editor.review = None;
            editor.cast = None;
            editor.cast_review = None;
            editor.checkpoint = retold_app::notes::draft_id().ok();
            editor.load_fields(window, cx);
        }
        self.load_scene_frame(cx);
        self.status = format!(
            "Scene {} selected. Reading its recorded frame…",
            unit.saturating_add(1)
        );
        cx.notify();
    }

    pub(super) fn scene_action(
        &mut self,
        action: SceneAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(editor) = &self.scenes else {
            return;
        };
        if editor.dirty(cx)
            && matches!(
                action,
                SceneAction::Approve | SceneAction::Undo | SceneAction::Redo
            )
        {
            return;
        }
        let (library, reference, unit) = (
            editor.library.clone(),
            editor.reference.clone(),
            editor.unit,
        );
        let scope = (library.clone(), reference.clone());
        let draft = editor.draft(cx);
        let version = editor
            .content()
            .map_or_else(String::new, |content| content.version.clone());
        let review = editor.review.clone();
        let cast_draft = editor.cast_draft();
        let cast_review = editor.cast_review.clone();
        let cursor = editor.history_cursor.clone();
        self.busy = true;
        self.review = None;
        self.status = "Reading or saving scene direction…".into();
        let work = cx.background_executor().spawn(async move {
            match action {
                SceneAction::History => {
                    scenes::history(&library, &reference, cursor.as_ref()).map(Reply::History)
                }
                SceneAction::Review => {
                    scenes::review(&library, &reference, draft.ok_or_else(missing_draft)?)
                        .map(Box::new)
                        .map(Reply::Review)
                }
                SceneAction::Save => scenes::save(&review.ok_or_else(missing_draft)?)
                    .map(Box::new)
                    .map(Reply::Document),
                SceneAction::ReviewCast => {
                    scenes::review_cast(&library, &reference, cast_draft.ok_or_else(missing_draft)?)
                        .map(Box::new)
                        .map(Reply::CastReview)
                }
                SceneAction::SaveCast => scenes::save_cast(&cast_review.ok_or_else(missing_draft)?)
                    .map(Box::new)
                    .map(Reply::Document),
                _ => {
                    let request = match action {
                        SceneAction::Approve => scenes::Action::Approve(&version),
                        SceneAction::Undo => scenes::Action::Undo(&version),
                        SceneAction::Redo => scenes::Action::Redo(&version),
                        _ => scenes::Action::Read,
                    };
                    scenes::document(&library, &reference, request)
                        .map(Box::new)
                        .map(Reply::Document)
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let Some(editor) = &mut this.scenes else {
                    return;
                };
                if editor.library != scope.0 || editor.reference != scope.1 || editor.unit != unit {
                    return;
                }
                match result {
                    Ok(Reply::Document(document)) => {
                        editor.receive(*document, matches!(action, SceneAction::Read), window, cx);
                        this.status = editor.refresh_message().into();
                    }
                    Ok(Reply::Review(review)) => {
                        if editor.draft(cx).as_ref() == Some(&review.draft) {
                            editor.review = Some(*review);
                        }
                        this.status =
                            "Review the affected scenes before saving. Nothing was published."
                                .into();
                    }
                    Ok(Reply::CastReview(review)) => {
                        if editor.cast_draft().as_ref() == Some(&review.draft) {
                            editor.cast_review = Some(*review);
                        }
                        this.status =
                            "Review the recast scenes before saving. Nothing was published.".into();
                    }
                    Ok(Reply::History(page)) => {
                        editor.history_cursor = page.next.clone();
                        editor.history = Some(page);
                    }
                    Err(error) => {
                        editor.error = Some(error.message);
                        this.status =
                            "Your direction was kept. See the note above to continue.".into();
                    }
                }
                if matches!(
                    action,
                    SceneAction::Read
                        | SceneAction::Save
                        | SceneAction::SaveCast
                        | SceneAction::Undo
                        | SceneAction::Redo
                ) {
                    this.load_scene_frame(cx);
                }
                this.checkpoint_scene(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn load_scene_frame(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.scenes else {
            return;
        };
        editor
            .player
            .update(cx, |player, cx| player.select(None, cx));
        editor.frame_generation = editor.frame_generation.wrapping_add(1);
        editor.frame_available = None;
        editor.frame_target = None;
        editor.frame_pending = true;
        self.launch_scene_frame(cx);
    }

    fn launch_scene_frame(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.scenes else {
            return;
        };
        if editor.frame_active || !editor.frame_pending {
            return;
        }
        let Some(version) = editor.content().map(|content| content.version.clone()) else {
            editor.frame_pending = false;
            editor.frame_available = Some(false);
            return;
        };
        editor.frame_active = true;
        editor.frame_pending = false;
        let generation = editor.frame_generation;
        let player_id = editor.player.entity_id();
        let (library, reference, unit) = (
            editor.library.clone(),
            editor.reference.clone(),
            editor.unit,
        );
        let scope = (library.clone(), reference.clone());
        let work = cx.background_executor().spawn(async move {
            let item = scenes::frame(
                &library,
                &reference,
                scenes::FrameSelection {
                    unit,
                    expected_version: &version,
                },
            )?;
            let target = if let Some(item) = &item {
                Some(retold_app::notes::Target::frame(
                    unit,
                    &item.preview_bytes()?,
                )?)
            } else {
                None
            };
            Ok::<_, retold_app::protocol::Diagnostic>((item, target))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(editor) = &mut this.scenes else {
                    return;
                };
                if editor.library != scope.0
                    || editor.reference != scope.1
                    || editor.player.entity_id() != player_id
                {
                    return;
                }
                editor.frame_active = false;
                if editor.frame_generation == generation && editor.unit == unit {
                    match result {
                        Ok((item, target)) => {
                            editor.frame_target = target;
                            editor.frame_available = Some(item.is_some());
                            if this.show_scenes && this.destination == Destination::Videos {
                                this.status = format!(
                                    "Scene {} selected. {}",
                                    unit.saturating_add(1),
                                    if item.is_some() {
                                        "Recorded frame selected."
                                    } else {
                                        "No recorded frame yet."
                                    }
                                );
                            }
                            editor
                                .player
                                .update(cx, |player, cx| player.select(item, cx));
                        }
                        Err(error) => {
                            editor.frame_available = Some(false);
                            editor.error = Some(error.message);
                        }
                    }
                }
                this.launch_scene_frame(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

fn missing_draft() -> retold_app::protocol::Diagnostic {
    retold_app::protocol::Diagnostic::new(
        retold_app::protocol::ErrorCode::InputRequired,
        "Review a saved scene before publishing changes.",
    )
}

impl Shell {
    /// Move to the next or previous scene from the keyboard, under the same guards as the buttons.
    pub(super) fn step_scene(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = &self.scenes else {
            return;
        };
        if self.busy || editor.dirty(cx) || editor.incoming.is_some() {
            return;
        }
        let Some(content) = editor.content() else {
            return;
        };
        let units = &content.board.units;
        let index = units
            .iter()
            .position(|unit| unit.id == editor.unit)
            .unwrap_or(0);
        let target = if forward {
            units.get(index + 1)
        } else {
            index.checked_sub(1).and_then(|index| units.get(index))
        };
        if let Some(unit) = target.map(|unit| unit.id) {
            self.select_scene(unit, window, cx);
        }
    }
}
