//! Native review screen and session state. Render/input callbacks never perform blocking I/O.

mod components;
mod preview;
mod screen;

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext, Context, Entity, RenderImage, Subscription, Window};

use crate::media::{FrameSlot, Media};
use crate::model::{Edition, ReviewSession};
use crate::work::Cancellation;

pub struct Studio {
    pub session: ReviewSession,
    pub(crate) narration: Entity<TextareaState>,
    pub(crate) note: Entity<TextareaState>,
    pub(crate) light: bool,
    pub(crate) status: String,
    pub(crate) check: Option<Cancellation>,
    pub(crate) media: Option<Media>,
    pub(crate) media_cancel: Cancellation,
    pub(crate) generation: u64,
    pub(crate) loading: bool,
    pub(crate) playback: Option<Playback>,
    pub(crate) position: f64,
    pub(crate) frame: Option<Arc<RenderImage>>,
    pub(crate) show_review: bool,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for Studio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Studio")
            .field("session", &self.session)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct Playback {
    pub cancel: Cancellation,
    pub frames: FrameSlot,
    pub started: std::time::Instant,
    pub offset: f64,
}

impl Studio {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session = ReviewSession::default();
        let narration = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx);
            state.set_value(session.draft().narration.clone(), window, cx);
            state
        });
        let note = cx.new(|cx| TextareaState::new(window, cx).placeholder("What should change?"));
        let subscriptions = [&narration, &note]
            .into_iter()
            .map(|state| {
                cx.subscribe(state, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            session,
            narration,
            note,
            light: false,
            status: "Fictional review workspace. Changes stay in this session.".into(),
            check: None,
            media: None,
            media_cancel: Cancellation::default(),
            generation: 0,
            loading: false,
            playback: None,
            position: 0.0,
            frame: None,
            show_review: false,
            _subscriptions: subscriptions,
        }
    }

    pub(crate) fn capture_edits(&mut self, cx: &Context<Self>) {
        self.session.save(
            self.narration.read(cx).value().to_string(),
            self.note.read(cx).value().to_string(),
        );
    }

    pub(crate) fn load_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.session.draft().clone();
        self.narration
            .update(cx, |state, cx| state.set_value(draft.narration, window, cx));
        self.note
            .update(cx, |state, cx| state.set_value(draft.note, window, cx));
        cx.notify();
    }

    pub(crate) fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.capture_edits(cx);
        self.session.select(index);
        self.clear_media();
        self.load_draft(window, cx);
        self.status = "Scene selected. Draft edits have been kept in this session.".into();
    }

    pub(crate) fn edition(
        &mut self,
        edition: Edition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.capture_edits(cx);
        self.session.edition = edition;
        self.clear_media();
        self.load_draft(window, cx);
        self.status = "Edition changed. Its narration, notes, and approval are independent.".into();
    }

    pub(crate) fn changed(&self, cx: &Context<Self>) -> bool {
        self.narration.read(cx).value().as_ref() != self.session.draft().narration
            || self.note.read(cx).value().as_ref() != self.session.draft().note
    }

    pub(crate) fn start_check(&mut self, cx: &mut Context<Self>) {
        if self.check.is_some() {
            return;
        }
        let cancel = Cancellation::default();
        self.check = Some(cancel.clone());
        self.status =
            "Checking responsiveness in the background. Keep typing or change scenes.".into();
        let work = cx
            .background_executor()
            .spawn(async move { cancel.slow_query() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.check = None;
                this.status = result.into();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn open_media(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.clear_media();
        let generation = self.generation;
        let cancel = self.media_cancel.clone();
        self.loading = true;
        self.status = "Inspecting existing media…".into();
        let work = cx
            .background_executor()
            .spawn(async move { crate::media::inspect(&path, &cancel) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(inspection) => {
                        this.media = Some(inspection.media);
                        this.frame = inspection.frame.and_then(preview::render_image);
                        this.status = "Existing media opened. Press Play to preview it.".into();
                    }
                    Err(error) => this.status = format!("Could not open media: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn clear_media(&mut self) {
        self.media_cancel.cancel();
        if let Some(playback) = self.playback.take() {
            playback.cancel.cancel();
        }
        self.media_cancel = Cancellation::default();
        self.generation = self.generation.wrapping_add(1);
        self.media = None;
        self.frame = None;
        self.position = 0.0;
        self.loading = false;
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        self.media_cancel.cancel();
        if let Some(playback) = &self.playback {
            playback.cancel.cancel();
        }
        if let Some(check) = &self.check {
            check.cancel();
        }
    }
}
