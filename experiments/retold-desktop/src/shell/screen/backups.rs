//! Backup, verification and inactive restore controls.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, account_admin::summary, backups::Choose};

impl Shell {
    pub(super) fn backup_controls(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.backups;
        let blocked = self.busy || self.snapshot.is_none();
        div().flex().flex_col().gap_2()
            .child(heading_small("Backups"))
            .child(muted("Backups hold your videos and spending records, never credentials. A restored workspace stays inactive until you activate it.", p))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("desktop-backup-create").self_start().label("Back up this workspace…").disabled(blocked)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_backup_folder(Choose::BackupParent, cx))))
                .child(Button::new("desktop-backup-verify").self_start().label("Verify a backup…").disabled(blocked)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_backup_folder(Choose::VerifyFolder, cx))))
                .child(Button::new("desktop-backup-restore-source").self_start().label("Restore a backup…").disabled(blocked)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_backup_folder(Choose::RestoreSource, cx))))
                .child(Button::new("desktop-backup-activate").self_start().label("Activate a restored workspace…").disabled(blocked)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_backup_folder(Choose::ActivateFolder, cx)))))
            .when_some(panel.error.as_ref(), |body, error| body.child(error_notice(error, p)))
            .when_some(panel.result.as_ref(), |body, result| body.child(notice(result, p)))
            .when_some(panel.backup.as_ref(), |body, review| {
                body.child(muted(&format!("Backup to {}: {}", review.output.display(), summary(&review.preview)), p))
                    .child(Button::new("desktop-backup-confirm").self_start().primary().label("Create reviewed backup").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_backup(cx))))
            })
            .when_some(panel.restore_source.as_ref().filter(|_| panel.restore.is_none()), |body, source| {
                body.child(muted(&format!("Restore {} — now choose where to place it.", source.display()), p))
                    .child(Button::new("desktop-backup-restore-parent").self_start().label("Choose destination folder…").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.choose_backup_folder(Choose::RestoreParent, cx))))
            })
            .when_some(panel.activation.as_ref(), |body, review| {
                body.child(error_notice(&format!("Activating {} makes it the owner of this workspace's videos and accounting and retires the original location. {}", review.root.display(), review.text), p))
                    .child(Button::new("desktop-backup-activate-confirm").self_start().primary().label("Activate reviewed workspace").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_activation(cx))))
            })
            .when_some(panel.restore.as_ref(), |body, review| {
                body.child(muted(&format!("Restore into {}: {}", review.output.display(), summary(&review.preview)), p))
                    .child(Button::new("desktop-backup-restore-confirm").self_start().primary().label("Restore as inactive workspace").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_restore(cx))))
            })
    }
}
