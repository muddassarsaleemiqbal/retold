//! Edition narration: recorded audio with its waveform and aligned lines, read-only.
use std::path::PathBuf;

use gpui_kit::{AppContext as _, Context, Entity};
use retold_app::narration::{self, Document, Waveform};

use super::{Shell, player::Player};

/// Columns drawn for one waveform; enough detail without per-sample rendering.
pub(super) const WAVEFORM_COLUMNS: usize = 240;

pub(super) struct NarrationPanel {
    pub library: PathBuf,
    pub reference: String,
    pub document: Option<Document>,
    pub edition: Option<String>,
    pub waveform: Option<Waveform>,
    pub player: Entity<Player>,
    pub error: Option<String>,
    generation: u64,
}

enum Reply {
    Document(Document),
    Waveform(Box<Waveform>),
}

impl Shell {
    pub(super) fn begin_narration(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        self.narration = Some(NarrationPanel {
            library: snapshot.context.library.clone(),
            reference: reference.clone(),
            document: None,
            edition: None,
            waveform: None,
            player: cx.new(|_| Player::new()),
            error: None,
            generation: 0,
        });
        self.narration_request(None, cx);
    }

    /// Read editions (no edition) or decode one edition's waveform; the latest request wins.
    pub(super) fn narration_request(&mut self, edition: Option<String>, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.narration else {
            return;
        };
        panel.generation = panel.generation.wrapping_add(1);
        panel.error = None;
        if edition.is_some() {
            panel.edition.clone_from(&edition);
            panel.waveform = None;
            panel
                .player
                .update(cx, |player, cx| player.select(None, cx));
        }
        let (library, reference, generation) = (
            panel.library.clone(),
            panel.reference.clone(),
            panel.generation,
        );
        let work = cx.background_executor().spawn(async move {
            match edition {
                None => narration::document(&library, &reference).map(Reply::Document),
                Some(edition) => {
                    narration::waveform(&library, &reference, (&edition, WAVEFORM_COLUMNS))
                        .map(|waveform| Reply::Waveform(Box::new(waveform)))
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(panel) = &mut this.narration else {
                    return;
                };
                if panel.generation != generation {
                    return;
                }
                let first = match result {
                    Ok(Reply::Document(document)) => {
                        let first = document
                            .editions
                            .first()
                            .map(|edition| edition.edition.clone());
                        panel.document = Some(document);
                        first
                    }
                    Ok(Reply::Waveform(waveform)) => {
                        let item = waveform.item.clone();
                        panel.waveform = Some(*waveform);
                        panel
                            .player
                            .update(cx, |player, cx| player.select(Some(item), cx));
                        None
                    }
                    Err(error) => {
                        panel.error = Some(error.message);
                        None
                    }
                };
                if first.is_some() {
                    this.narration_request(first, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
