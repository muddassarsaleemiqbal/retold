//! Workspace health with reviewed repair, export presets and configuration profiles.
use gpui_kit::Context;
use retold_app::desktop::admin;
use retold_app::doctor::DoctorReport;
use retold_app::presets::PresetRecord;
use retold_app::profile::ProfileRecord;

use super::Shell;

#[derive(Default)]
pub(super) struct AdminPanel {
    pub health: Option<DoctorReport>,
    pub presets: Option<Vec<PresetRecord>>,
    pub profiles: Option<(Vec<ProfileRecord>, Option<String>)>,
    pub removing: Option<String>,
    pub error: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone)]
pub(super) enum AdminTask {
    Load,
    Repair,
    RemovePreset(String),
    CreatePreset(String, retold_app::presets::PresetFile),
    CreateProfile(String),
    UseProfile(String),
}

enum Reply {
    Loaded(
        Box<DoctorReport>,
        Vec<PresetRecord>,
        (Vec<ProfileRecord>, Option<String>),
    ),
    Repaired(Box<DoctorReport>),
    Presets(Vec<PresetRecord>),
    Profiles((Vec<ProfileRecord>, Option<String>)),
}

impl Shell {
    pub(super) fn admin_task(&mut self, task: AdminTask, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if self.busy {
            return;
        }
        let (library, config) = (
            snapshot.context.library.clone(),
            snapshot.context.config.clone(),
        );
        let reviewed = self.admin.health.clone();
        self.admin.error = None;
        self.admin.message = None;
        self.admin.removing = None;
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            match task {
                AdminTask::Load => Ok(Reply::Loaded(
                    Box::new(admin::health(&library)?),
                    admin::presets(&library)?,
                    admin::profiles(&config)?,
                )),
                AdminTask::Repair => {
                    let reviewed = reviewed.ok_or_else(|| {
                        retold_app::protocol::Diagnostic::new(
                            retold_app::protocol::ErrorCode::InputRequired,
                            "Check workspace health first.",
                        )
                    })?;
                    admin::repair(&library, &reviewed)
                        .map(|report| Reply::Repaired(Box::new(report)))
                }
                AdminTask::CreatePreset(name, file) => {
                    admin::create_preset(&library, &name, &file).map(Reply::Presets)
                }
                AdminTask::CreateProfile(name) => {
                    admin::create_profile(&config, &name).map(Reply::Profiles)
                }
                AdminTask::RemovePreset(name) => {
                    admin::remove_preset(&library, &name).map(Reply::Presets)
                }
                AdminTask::UseProfile(name) => {
                    admin::use_profile(&config, &name).map(Reply::Profiles)
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let panel = &mut this.admin;
                match result {
                    Ok(Reply::Loaded(health, presets, profiles)) => {
                        panel.health = Some(*health);
                        panel.presets = Some(presets);
                        panel.profiles = Some(profiles);
                    }
                    Ok(Reply::Repaired(report)) => {
                        panel.message =
                            Some(format!("Applied {} repair(s).", report.fixes_applied.len()));
                        panel.health = Some(*report);
                    }
                    Ok(Reply::Presets(presets)) => panel.presets = Some(presets),
                    Ok(Reply::Profiles(profiles)) => {
                        panel.message = Some("Profile selected. New work uses it.".into());
                        panel.profiles = Some(profiles);
                    }
                    Err(error) => panel.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
