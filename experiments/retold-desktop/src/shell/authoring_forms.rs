//! Creation forms for export presets and configuration profiles.
use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, Window};
use retold_app::presets::PresetFile;

use super::{Shell, admin::AdminTask};

pub(super) struct CreationForms {
    pub preset_name: Entity<InputState>,
    pub profile_name: Entity<InputState>,
    pub preset: PresetFile,
}

impl CreationForms {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        Self {
            preset_name: cx
                .new(|cx| InputState::new(window, cx).placeholder("Preset name, such as web-1080")),
            profile_name: cx.new(|cx| InputState::new(window, cx).placeholder("Profile name")),
            preset: PresetFile {
                aspects: vec!["16:9".into()],
                container: "mp4".into(),
                video_codec: "h264".into(),
                audio_codec: "aac".into(),
                caption_mode: "burned".into(),
            },
        }
    }
}

/// Which preset field a choice button sets.
#[derive(Clone, Copy)]
pub(super) enum PresetField {
    Aspect,
    Container,
    Video,
    Audio,
    Captions,
}

impl Shell {
    pub(super) fn choose_preset_value(
        &mut self,
        field: PresetField,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        let preset = &mut self.forms.preset;
        match field {
            PresetField::Aspect => {
                let present = preset.aspects.iter().any(|aspect| aspect == value);
                if present && preset.aspects.len() > 1 {
                    preset.aspects.retain(|aspect| aspect != value);
                } else if !present {
                    preset.aspects.push(value.into());
                }
            }
            PresetField::Container => preset.container = value.into(),
            PresetField::Video => preset.video_codec = value.into(),
            PresetField::Audio => preset.audio_codec = value.into(),
            PresetField::Captions => preset.caption_mode = value.into(),
        }
        cx.notify();
    }

    pub(super) fn submit_preset(&mut self, cx: &mut Context<Self>) {
        let name = self.forms.preset_name.read(cx).value().trim().to_owned();
        self.admin_task(AdminTask::CreatePreset(name, self.forms.preset.clone()), cx);
    }

    pub(super) fn submit_profile(&mut self, cx: &mut Context<Self>) {
        let name = self.forms.profile_name.read(cx).value().trim().to_owned();
        self.admin_task(AdminTask::CreateProfile(name), cx);
    }
}
