//! One decoder lane serves the current production selection and one pending replacement.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::{Button, ButtonRounded, ButtonVariants as _},
};
use gpui_kit::{Context, ObjectFit, Render, RenderImage, Role, Window, div, img, prelude::*, px};
use retold_app::desktop::media::{Item, Kind};

use crate::media::{self, FrameSlot, Media};
use crate::work::Cancellation;

pub(super) struct Player {
    selected: Option<PathBuf>,
    media: Option<Media>,
    frame: Option<Arc<RenderImage>>,
    text: Option<String>,
    status: String,
    position: f64,
    generation: u64,
    active: Option<Active>,
    pending: Option<Work>,
}

struct Active {
    cancel: Cancellation,
    playback: Option<(Instant, f64, FrameSlot)>,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

enum Work {
    Open(Item),
    Seek(Media, f64),
    Play(Media, f64, FrameSlot),
}

enum ResultView {
    Loaded {
        media: Option<Media>,
        frame: Option<Arc<RenderImage>>,
        text: Option<String>,
    },
    Frame(Option<Arc<RenderImage>>),
    Played,
}

impl Work {
    fn execute(self, cancel: &Cancellation) -> anyhow::Result<ResultView> {
        if cancel.is_cancelled() {
            anyhow::bail!("Preview selection changed");
        }
        match self {
            Self::Open(item) => match item.kind {
                Kind::Image => {
                    let bytes = item
                        .preview_bytes()
                        .map_err(|error| anyhow::anyhow!("{}", error.message))?;
                    let frame = super::characters::decode(&bytes).map_err(anyhow::Error::msg)?;
                    Ok(ResultView::Loaded {
                        media: None,
                        frame: Some(frame),
                        text: None,
                    })
                }
                Kind::Captions | Kind::Project => {
                    let bytes = item
                        .preview_bytes()
                        .map_err(|error| anyhow::anyhow!("{}", error.message))?;
                    let text = String::from_utf8(bytes)?;
                    Ok(ResultView::Loaded {
                        media: None,
                        frame: None,
                        text: Some(text),
                    })
                }
                Kind::Audio | Kind::Video => {
                    let path = item
                        .validated_path()
                        .map_err(|error| anyhow::anyhow!("{}", error.message))?;
                    let inspected = media::inspect(&path, cancel)?;
                    item.validated_path()
                        .map_err(|error| anyhow::anyhow!("{}", error.message))?;
                    Ok(ResultView::Loaded {
                        media: Some(inspected.media),
                        frame: inspected.frame.and_then(frame),
                        text: None,
                    })
                }
            },
            Self::Seek(media, position) => Ok(ResultView::Frame(frame(media::frame(
                &media, position, cancel,
            )?))),
            Self::Play(media, position, frames) => {
                media::play(&media, position, cancel, frames)?;
                Ok(ResultView::Played)
            }
        }
    }
}

impl Player {
    pub(super) fn new() -> Self {
        Self {
            selected: None,
            media: None,
            frame: None,
            text: None,
            status: "Choose saved media to review.".into(),
            position: 0.,
            generation: 0,
            active: None,
            pending: None,
        }
    }

    pub(super) fn image_loaded(&self) -> bool {
        self.frame.is_some() && self.media.is_none() && self.active.is_none()
    }

    pub(super) fn select(&mut self, item: Option<Item>, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(active) = &self.active {
            active.cancel.cancel();
        }
        self.selected = item.as_ref().map(|item| item.path.clone());
        self.media = None;
        self.frame = None;
        self.text = None;
        self.position = 0.;
        self.status = if item.is_some() {
            "Loading media…"
        } else {
            "Choose saved media to review."
        }
        .into();
        self.pending = item.map(Work::Open);
        self.launch(cx);
        cx.notify();
    }

    fn launch(&mut self, cx: &mut Context<Self>) {
        if self.active.is_some() {
            return;
        }
        let Some(work) = self.pending.take() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let cancel = Cancellation::default();
        let playback = match &work {
            Work::Play(_, offset, frames) => Some((Instant::now(), *offset, Arc::clone(frames))),
            _ => None,
        };
        self.active = Some(Active {
            cancel: cancel.clone(),
            playback,
        });
        let generation = self.generation;
        let (send, receive) = futures_channel::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("retold-media-preview".into())
            .spawn(move || {
                let _ = send.send(work.execute(&cancel));
            });
        if let Err(error) = spawned {
            self.active = None;
            self.status = format!("Could not start the preview worker: {error}");
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = receive.await.unwrap_or_else(|_| Err(anyhow::anyhow!("Preview worker exited before returning a result")));
            let _ = this.update(cx, |this, cx| {
                let previous = this.active.take();
                if this.generation == generation {
                    match result {
                        Ok(ResultView::Loaded { media, frame, text }) => {
                            this.media = media; this.frame = frame; this.text = text;
                            this.status = "Media loaded.".into();
                        }
                        Ok(ResultView::Frame(frame)) => { this.frame = frame; this.status = "Preview position updated.".into(); }
                        Ok(ResultView::Played) => {
                            if let Some((started, offset, _)) = previous.as_ref().and_then(|active| active.playback.as_ref()) {
                                this.position = (offset + started.elapsed().as_secs_f64()).min(this.media.as_ref().map_or(0., |media| media.duration));
                            }
                            this.status = "Preview stopped. Its processes have exited.".into();
                        }
                        Err(error) => { this.frame = None; this.status = format!("Preview unavailable: {error}. Choose other media or refresh the list."); }
                    }
                }
                this.launch(cx);
                cx.notify();
            });
        }).detach();
        self.watch(generation, cx);
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(active) = &self.active {
            active.cancel.cancel();
            self.status = "Stopping the preview…".into();
        } else if let Some(media) = self.media.clone() {
            if self.position >= media.duration {
                self.position = 0.;
            }
            self.pending = Some(Work::Play(media, self.position, Arc::new(Mutex::new(None))));
            self.status = "Playing saved media.".into();
            self.launch(cx);
        }
        cx.notify();
    }

    /// Play from an absolute position, replacing any playback in progress.
    pub(super) fn play_from(&mut self, seconds: f64, cx: &mut Context<Self>) {
        let Some(media) = self.media.clone() else {
            return;
        };
        if let Some(active) = &self.active {
            active.cancel.cancel();
        }
        self.position = seconds.clamp(0., (media.duration - 0.1).max(0.));
        self.pending = Some(Work::Play(media, self.position, Arc::new(Mutex::new(None))));
        self.status = "Playing saved media.".into();
        self.launch(cx);
        cx.notify();
    }

    fn seek(&mut self, seconds: f64, cx: &mut Context<Self>) {
        if self.active.is_some() {
            return;
        }
        let Some(media) = self.media.clone() else {
            return;
        };
        self.position = (self.position + seconds).clamp(0., (media.duration - 0.1).max(0.));
        if media.video {
            self.pending = Some(Work::Seek(media, self.position));
            self.launch(cx);
        }
        cx.notify();
    }

    fn watch(&self, generation: u64, cx: &mut Context<Self>) {
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.playback.is_none())
        {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let active = this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return false;
                        }
                        let Some((started, offset, frames)) = this
                            .active
                            .as_ref()
                            .and_then(|active| active.playback.as_ref())
                        else {
                            return false;
                        };
                        if let Ok(mut slot) = frames.lock()
                            && let Some(bytes) = slot.take()
                        {
                            this.frame = frame(bytes);
                        }
                        this.position = (offset + started.elapsed().as_secs_f64())
                            .min(this.media.as_ref().map_or(0., |media| media.duration));
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
    }
}

fn frame(bytes: Vec<u8>) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(media::WIDTH, media::HEIGHT, bytes)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}

impl Render for Player {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors;
        let playing = self
            .active
            .as_ref()
            .is_some_and(|active| active.playback.is_some());
        let ready = self.media.as_ref().is_some_and(|media| media.duration > 0.);
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .text_color(colors.foreground)
            .child(
                div()
                    .id("desktop-player-status")
                    .test_support()
                    .text_xs()
                    .opacity(0.7)
                    .role(Role::Status)
                    .aria_label(self.status.clone())
                    .child(self.status.clone()),
            );
        if let Some(frame) = &self.frame {
            body = body.child(
                div()
                    .id("desktop-media-frame")
                    .test_support()
                    .relative()
                    .h(px(300.))
                    .flex_shrink_0()
                    .w_full()
                    .overflow_hidden()
                    .bg(colors.muted)
                    .child(
                        img(Arc::clone(frame))
                            .absolute()
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    ),
            );
        }
        if let Some(text) = &self.text {
            body = body.child(
                div()
                    .id("desktop-media-text")
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .p_3()
                    .child(text.clone()),
            );
        }
        let time = self
            .media
            .as_ref()
            .map(|media| format!("{} / {}", clock(self.position), clock(media.duration)));
        body.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("desktop-media-backward")
                        .ghost()
                        .icon(IconName::SkipBack)
                        .accessibility_label("Back 5 seconds")
                        .disabled(!ready || self.active.is_some())
                        .on_click(cx.listener(|this, _, _, cx| this.seek(-5., cx))),
                )
                .child(
                    Button::new("desktop-media-play")
                        .primary()
                        .rounded(ButtonRounded::Large)
                        .icon(if playing {
                            IconName::Pause
                        } else {
                            IconName::Play
                        })
                        .accessibility_label(if playing { "Pause" } else { "Play" })
                        .disabled(!ready || (self.active.is_some() && !playing))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle(cx))),
                )
                .child(
                    Button::new("desktop-media-forward")
                        .ghost()
                        .icon(IconName::SkipForward)
                        .accessibility_label("Forward 5 seconds")
                        .disabled(!ready || self.active.is_some())
                        .on_click(cx.listener(|this, _, _, cx| this.seek(5., cx))),
                )
                .when_some(time, |row, time| {
                    row.child(div().ml_2().text_sm().child(time))
                }),
        )
    }
}

/// Minutes and seconds, such as `1:05`.
fn clock(seconds: f64) -> String {
    let whole = seconds.max(0.).floor();
    format!("{}:{:02}", (whole / 60.).floor(), whole % 60.)
}
