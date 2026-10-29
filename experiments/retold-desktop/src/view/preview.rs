//! Worker-to-view preview delivery, scoped to the exact selection generation.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::{Context, PathPromptOptions, RenderImage};

use super::{Playback, Studio};
use crate::media::{HEIGHT, WIDTH};
use crate::work::Cancellation;

pub(super) fn render_image(bytes: Vec<u8>) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(WIDTH, HEIGHT, bytes)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}

impl Studio {
    pub(super) fn choose_media(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open existing audio, image, or video".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = paths.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.open_media(path, cx);
                    }
                }
                Ok(Ok(None)) => (),
                _ => {
                    this.status = "The file chooser could not be opened. Try again.".into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(playback) = &self.playback {
            playback.cancel.cancel();
            self.status = "Pausing preview; waiting for the decoder to exit…".into();
            cx.notify();
            return;
        }
        let Some(media) = self.media.clone() else {
            return;
        };
        if self.loading || media.duration <= 0.0 {
            return;
        }
        if self.position >= media.duration {
            self.position = 0.0;
        }
        let offset = self.position;
        let generation = self.generation;
        let cancel = Cancellation::default();
        let frames = Arc::new(Mutex::new(None));
        self.playback = Some(Playback {
            cancel: cancel.clone(),
            frames: Arc::clone(&frames),
            started: Instant::now(),
            offset,
        });
        let work = cx
            .background_executor()
            .spawn(async move { crate::media::play(&media, offset, &cancel, frames) });
        self.status = "Preview playing. A/V synchronization is experimental.".into();
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                if let Some(playback) = this.playback.take() {
                    let duration = this.media.as_ref().map_or(0.0, |m| m.duration);
                    this.position =
                        (playback.offset + playback.started.elapsed().as_secs_f64()).min(duration);
                    this.status = match result {
                        Ok(()) if playback.cancel.is_cancelled() => {
                            "Preview paused. Its processes have exited.".into()
                        }
                        Ok(()) => "Preview finished.".into(),
                        Err(error) => format!("Preview failed: {error}"),
                    };
                }
                cx.notify();
            });
        })
        .detach();
        self.watch_frames(generation, cx);
        cx.notify();
    }

    fn watch_frames(&self, generation: u64, cx: &mut Context<Self>) {
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
                        let Some(playback) = &this.playback else {
                            return false;
                        };
                        if let Ok(mut slot) = playback.frames.lock()
                            && let Some(bytes) = slot.take()
                        {
                            this.frame = render_image(bytes);
                        }
                        let duration = this.media.as_ref().map_or(0.0, |m| m.duration);
                        this.position = (playback.offset
                            + playback.started.elapsed().as_secs_f64())
                        .min(duration);
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

    pub(super) fn seek(&mut self, seconds: f64, cx: &mut Context<Self>) {
        if self.playback.is_some() || self.loading {
            return;
        }
        let Some(media) = self.media.clone() else {
            return;
        };
        if media.duration <= 0.0 {
            return;
        }
        self.position = (self.position + seconds).clamp(0.0, (media.duration - 0.1).max(0.0));
        if !media.video {
            cx.notify();
            return;
        }
        self.media_cancel.cancel();
        self.media_cancel = Cancellation::default();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let cancel = self.media_cancel.clone();
        let position = self.position;
        self.loading = true;
        let work = cx
            .background_executor()
            .spawn(async move { crate::media::frame(&media, position, &cancel) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(bytes) => this.frame = render_image(bytes),
                    Err(error) => this.status = format!("Could not seek: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
