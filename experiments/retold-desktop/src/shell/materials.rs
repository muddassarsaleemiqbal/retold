//! Saved materials of one video and the licensed music/effects library, read-only.
use std::path::PathBuf;

use gpui_kit::Context;
use retold_app::assets::{AssetDetail, AssetSummary};
use retold_app::audio::AudioItem;
use retold_app::desktop::materials;

use super::Shell;

pub(super) struct MaterialsPanel {
    pub library: PathBuf,
    pub reference: String,
    pub items: Option<Vec<AssetSummary>>,
    pub audio: Option<Vec<AudioItem>>,
    pub detail: Option<AssetDetail>,
    pub verified: Option<bool>,
    pub error: Option<String>,
    generation: u64,
}

#[derive(Clone)]
pub(super) enum MaterialTask {
    List,
    Detail(String),
    Verify(String),
    Import(std::path::PathBuf, materials::AudioLicenceDeclaration),
}

enum Reply {
    List(Vec<AssetSummary>, Vec<AudioItem>),
    Detail(Box<AssetDetail>),
    Verified(bool),
    Audio(Vec<AudioItem>),
}

impl Shell {
    pub(super) fn begin_materials(&mut self, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        self.materials = Some(MaterialsPanel {
            library: snapshot.context.library.clone(),
            reference: reference.clone(),
            items: None,
            audio: None,
            detail: None,
            verified: None,
            error: None,
            generation: 0,
        });
        self.material_task(MaterialTask::List, cx);
    }

    pub(super) fn material_task(&mut self, task: MaterialTask, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.materials else {
            return;
        };
        panel.generation = panel.generation.wrapping_add(1);
        panel.error = None;
        if matches!(task, MaterialTask::Detail(_)) {
            panel.detail = None;
            panel.verified = None;
        }
        let (library, reference, generation) = (
            panel.library.clone(),
            panel.reference.clone(),
            panel.generation,
        );
        let work = cx.background_executor().spawn(async move {
            match task {
                MaterialTask::List => Ok(Reply::List(
                    materials::list(&library, &reference)?,
                    materials::audio(&library)?,
                )),
                MaterialTask::Detail(id) => {
                    materials::detail(&library, &id).map(|detail| Reply::Detail(Box::new(detail)))
                }
                MaterialTask::Verify(id) => materials::verify(&library, &id).map(Reply::Verified),
                MaterialTask::Import(source, declared) => {
                    materials::import_audio(&library, &source, &declared).map(Reply::Audio)
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(panel) = &mut this.materials else {
                    return;
                };
                if panel.generation != generation {
                    return;
                }
                match result {
                    Ok(Reply::List(items, audio)) => {
                        panel.items = Some(arrange(items));
                        panel.audio = Some(audio);
                    }
                    Ok(Reply::Detail(detail)) => panel.detail = Some(*detail),
                    Ok(Reply::Verified(verified)) => panel.verified = Some(verified),
                    Ok(Reply::Audio(audio)) => panel.audio = Some(audio),
                    Err(error) => panel.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Shell {
    /// Choose an audio file, then import it with the declared licence from the form.
    pub(super) fn import_audio(&mut self, cx: &mut Context<Self>) {
        let declared = materials::AudioLicenceDeclaration {
            category: self.audio_form.category,
            licence: self.audio_form.licence.read(cx).value().to_string(),
            commercial_use: self.audio_form.commercial_use,
        };
        let chooser = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import licensed audio".into()),
        });
        cx.spawn(async move |this, cx| {
            let chosen = chooser
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let _ = this.update(cx, |this, cx| {
                if let Some(source) = chosen {
                    this.material_task(MaterialTask::Import(source, declared), cx);
                }
            });
        })
        .detach();
    }
}

/// The licence declaration filled in before choosing an audio file.
pub(super) struct AudioForm {
    pub category: retold_app::audio::AudioCategory,
    pub licence: gpui_kit::Entity<gpui_kit::component::input::InputState>,
    pub commercial_use: bool,
}

impl AudioForm {
    pub(super) fn new(window: &mut gpui_kit::Window, cx: &mut Context<Shell>) -> Self {
        use gpui_kit::AppContext as _;
        Self {
            category: retold_app::audio::AudioCategory::Music,
            licence: cx.new(|cx| {
                gpui_kit::component::input::InputState::new(window, cx)
                    .placeholder("Licence, such as CC-BY-4.0")
            }),
            commercial_use: false,
        }
    }
}

/// Sort key that orders "Scene 2 frame" before "Scene 10 frame".
pub(super) fn natural_key(name: &str) -> (String, usize, String) {
    let digits: String = name.chars().filter(char::is_ascii_digit).collect();
    let prefix: String = name.chars().take_while(|c| !c.is_ascii_digit()).collect();
    (
        prefix,
        digits.parse().unwrap_or(usize::MAX),
        name.to_owned(),
    )
}

/// A file name without the content hash the engine appends, such as `count-orlok.png`.
pub(super) fn display_name(name: &str) -> String {
    let (stem, extension) = name
        .rsplit_once('.')
        .map_or((name, ""), |(stem, extension)| (stem, extension));
    let readable = stem
        .rsplit_once('-')
        .filter(|(_, tail)| tail.len() >= 12 && tail.chars().all(|c| c.is_ascii_hexdigit()))
        .map_or(stem, |(head, _)| head);
    let words = readable.replace(['-', '_'], " ");
    let words = words
        .strip_prefix("unit ")
        .and_then(|number| number.parse::<usize>().ok())
        .map_or(words.clone(), |number| {
            format!("Scene {} frame", number + 1)
        });
    if extension.is_empty() {
        words
    } else {
        format!("{words} · {}", extension.to_uppercase())
    }
}

/// One row per distinct file, in natural order; done once when the list arrives, not per frame.
fn arrange(items: Vec<AssetSummary>) -> Vec<AssetSummary> {
    let mut seen = std::collections::BTreeSet::new();
    let mut unique: Vec<AssetSummary> = items
        .into_iter()
        .filter(|item| seen.insert((display_name(&item.name), item.kind.clone())))
        .collect();
    unique.sort_by_cached_key(|item| natural_key(&display_name(&item.name)));
    unique
}
