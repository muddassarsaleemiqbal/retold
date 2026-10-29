//! Workspace backups from Settings: choose folders, review, then create, verify or restore inactive.
use std::path::PathBuf;

use gpui_kit::{Context, PathPromptOptions};
use retold_app::desktop::backups::{self, Activation, Reviewed};

use super::{Shell, account_admin::summary};

#[derive(Default)]
pub(super) struct BackupPanel {
    pub backup: Option<Reviewed>,
    pub restore: Option<Reviewed>,
    pub restore_source: Option<PathBuf>,
    pub activation: Option<backups::Activation>,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) enum Choose {
    BackupParent,
    VerifyFolder,
    RestoreSource,
    RestoreParent,
    ActivateFolder,
}

enum Work {
    ReviewBackup(PathBuf),
    CreateBackup,
    Verify(PathBuf),
    ReviewRestore(PathBuf, PathBuf),
    Restore,
    ReviewActivation(PathBuf),
    Activate,
}

enum Reply {
    Backup(Box<Reviewed>),
    Restore(Box<Reviewed>),
    Activation(Box<Activation>),
    Done(String),
}

impl Shell {
    pub(super) fn choose_backup_folder(&mut self, choose: Choose, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let chosen = chooser
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let _ = this.update(cx, |this, cx| {
                let Some(folder) = chosen else {
                    return;
                };
                match choose {
                    Choose::BackupParent => {
                        this.backup_work(Work::ReviewBackup(backups::destination(&folder)), cx)
                    }
                    Choose::VerifyFolder => this.backup_work(Work::Verify(folder), cx),
                    Choose::ActivateFolder => this.backup_work(Work::ReviewActivation(folder), cx),
                    Choose::RestoreSource => {
                        this.backups.restore_source = Some(folder);
                        cx.notify();
                    }
                    Choose::RestoreParent => {
                        if let Some(source) = this.backups.restore_source.clone() {
                            let output = folder.join("retold-restored-workspace");
                            this.backup_work(Work::ReviewRestore(source, output), cx);
                        }
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn confirm_backup(&mut self, cx: &mut Context<Self>) {
        self.backup_work(Work::CreateBackup, cx);
    }

    pub(super) fn confirm_activation(&mut self, cx: &mut Context<Self>) {
        self.backup_work(Work::Activate, cx);
    }

    pub(super) fn confirm_restore(&mut self, cx: &mut Context<Self>) {
        self.backup_work(Work::Restore, cx);
    }

    fn backup_work(&mut self, work: Work, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if self.busy {
            return;
        }
        let library = snapshot.context.library.clone();
        let (backup, restore) = (self.backups.backup.clone(), self.backups.restore.clone());
        let activation = self.backups.activation.clone();
        self.backups.error = None;
        self.backups.result = None;
        self.busy = true;
        let task = cx.background_executor().spawn(async move {
            let missing = || {
                retold_app::protocol::Diagnostic::new(
                    retold_app::protocol::ErrorCode::InputRequired,
                    "Review it first.",
                )
            };
            match work {
                Work::ReviewBackup(output) => {
                    backups::review_backup(&library, &output).map(|r| Reply::Backup(Box::new(r)))
                }
                Work::CreateBackup => backups::create_backup(&backup.ok_or_else(missing)?)
                    .map(|v| Reply::Done(summary(&v))),
                Work::Verify(folder) => backups::verify(&folder).map(|verified| {
                    Reply::Done(format!(
                        "Backup verified offline: {} files, {} bytes.",
                        verified.index.files.len(),
                        verified.total_bytes
                    ))
                }),
                Work::ReviewRestore(source, output) => {
                    backups::review_restore(&source, &output).map(|r| Reply::Restore(Box::new(r)))
                }
                Work::ReviewActivation(folder) => {
                    backups::review_activation(&folder).map(|a| Reply::Activation(Box::new(a)))
                }
                Work::Activate => backups::activate(&activation.ok_or_else(missing)?)
                    .map(|v| Reply::Done(summary(&v))),
                Work::Restore => backups::restore_backup(&restore.ok_or_else(missing)?)
                    .map(|v| Reply::Done(summary(&v))),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let panel = &mut this.backups;
                match result {
                    Ok(Reply::Backup(review)) => panel.backup = Some(*review),
                    Ok(Reply::Restore(review)) => panel.restore = Some(*review),
                    Ok(Reply::Activation(review)) => panel.activation = Some(*review),
                    Ok(Reply::Done(text)) => {
                        panel.backup = None;
                        panel.restore = None;
                        panel.restore_source = None;
                        panel.activation = None;
                        panel.result = Some(text);
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
