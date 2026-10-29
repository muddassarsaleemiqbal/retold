//! Character previews and approval stay bound to one workspace, video and content hash.
use std::{io::Cursor, path::PathBuf, sync::Arc};

use gpui_kit::{Context, RenderImage};
use retold_app::desktop::characters::{self, Action, CharacterDocument};

use super::Shell;

pub(super) struct CharacterPanel {
    pub library: PathBuf,
    pub reference: String,
    pub document: Option<CharacterDocument>,
    pub frame: Option<Arc<RenderImage>>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub(super) enum CharacterAction {
    Read(Option<String>),
    Approve,
}

impl Shell {
    pub(super) fn begin_characters(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.editor.as_ref().is_some_and(|editor| editor.dirty(cx)) {
            self.status = "Save or discard your script changes before reviewing characters.".into();
            self.show_script = true;
            cx.notify();
            return;
        }
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        self.characters = Some(CharacterPanel {
            library: snapshot.context.library.clone(),
            reference: reference.clone(),
            document: None,
            frame: None,
            error: None,
        });
        self.show_script = false;
        self.delivery = None;
        self.character_action(CharacterAction::Read(None), cx);
    }

    pub(super) fn character_action(&mut self, action: CharacterAction, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let approving = matches!(action, CharacterAction::Approve);
        let Some(panel) = &mut self.characters else {
            return;
        };
        let approval = match action {
            CharacterAction::Approve => {
                if panel.frame.is_none() || panel.error.is_some() {
                    return;
                }
                let Some(sheet) = panel
                    .document
                    .as_ref()
                    .and_then(|document| document.selected.as_ref())
                else {
                    return;
                };
                Some((sheet.sheet.name.clone(), sheet.sheet.content_hash.clone()))
            }
            CharacterAction::Read(_) => None,
        };
        let name = match action {
            CharacterAction::Read(name) => name,
            CharacterAction::Approve => None,
        };
        let library = panel.library.clone();
        let reference = panel.reference.clone();
        if approval.is_none() {
            panel.frame = None;
        }
        panel.error = None;
        self.busy = true;
        self.review = None;
        if let Some(delivery) = &mut self.delivery {
            delivery.review = None;
        }
        let scope = (library.clone(), reference.clone());
        let work = cx.background_executor().spawn(async move {
            let action = match &approval {
                Some((name, hash)) => Action::Approve {
                    name,
                    expected_hash: hash,
                },
                None => Action::Read(name.as_deref()),
            };
            let document = characters::document(&library, &reference, action)
                .map_err(|error| error.message)?;
            let frame = document
                .selected
                .as_ref()
                .map(|selected| decode(&selected.bytes))
                .transpose()?;
            Ok::<_, String>((document, frame))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let same = this
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.context.library == scope.0)
                    && this.selected_video.as_deref() == Some(scope.1.as_str());
                let mut advance = None;
                if same
                    && let Some(panel) = &mut this.characters
                    && panel.library == scope.0
                    && panel.reference == scope.1
                {
                    match result {
                        Ok((document, frame)) => {
                            if approving {
                                advance = document
                                    .sheets
                                    .iter()
                                    .find(|sheet| !sheet.approved)
                                    .map(|sheet| sheet.name.clone());
                            }
                            panel.document = Some(document);
                            panel.frame = frame;
                            panel.error = None;
                        }
                        Err(error) => {
                            panel.frame = None;
                            panel.error = Some(error);
                        }
                    }
                }
                if let Some(next) = advance {
                    this.character_action(CharacterAction::Read(Some(next)), cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<Arc<RenderImage>, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut pixels = reader
        .decode()
        .map_err(|error| format!("Cannot preview this sheet: {error}"))?
        .thumbnail(1024, 1024)
        .to_rgba8();
    // GPUI's RenderImage consumes BGRA, matching the existing media adapter.
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new([image::Frame::new(pixels)])))
}

impl Shell {
    /// ⌘↩ approves what the page shows: the open character sheet, or the saved script.
    pub(super) fn approve_current(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if let Some(panel) = &self.characters {
            let already = panel
                .document
                .as_ref()
                .and_then(|document| document.selected.as_ref())
                .is_some_and(|selected| selected.sheet.approved);
            if !already {
                self.character_action(CharacterAction::Approve, cx);
            }
            return;
        }
        let approvable = self.show_script
            && self.editor.as_ref().is_some_and(|editor| {
                !editor.dirty(cx)
                    && editor.incoming.is_none()
                    && editor.content().is_some_and(|content| {
                        !content.approved
                            && content.editable
                            && !content.source_changed
                            && content.problems.is_empty()
                    })
            });
        if approvable {
            self.script_action(super::editing::ScriptAction::Approve, window, cx);
        }
    }
}
