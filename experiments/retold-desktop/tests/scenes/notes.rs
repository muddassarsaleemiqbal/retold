//! A native comment remains on its reviewed direction until the user explicitly adopts a version.
use super::*;
use retold_app::notes::{self, ContextQuery, Target};

const TEXT: &str = "Review this direction हिंदी اردو English.";

pub(super) fn journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    library: &Path,
    reference: &str,
    output: &Path,
) -> anyhow::Result<()> {
    let original = document(library, reference)?;
    let target = Target::scene(&original.board, 0).map_err(edit_error)?;
    click(cx, handle, "desktop-scene-comment")?;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        type_field(window, "Review note", TEXT, cx)
    })?;
    recovery::wait_recovery(cx, shell)?;
    let records = Repository::new(configuration(library))
        .list()
        .map_err(edit_error)?;
    ensure!(records.iter().any(|entry| matches!(&entry.draft.content, Content::Note { target: Some(saved), .. } if saved == &target)), "Note checkpoint lost its reviewed scene version");
    let shot = &original.board.direction.shots[0];
    let review = scenes::review(
        library,
        reference,
        Draft {
            expected_version: original.version.clone(),
            unit: 0,
            action: format!("{} The figure waits.", shot.action),
            location: shot.location.clone(),
            mood: shot.mood.clone(),
        },
    )
    .map_err(edit_error)?;
    let changed = scenes::save(&review)
        .map_err(edit_error)?
        .content
        .context("Missing changed scene")?;
    click(cx, handle, "desktop-notes-save")?;
    wait(cx, shell)?;
    ensure!(
        notes::contextual_document(
            library,
            reference,
            ContextQuery {
                target: &target,
                offset: 0
            }
        )
        .map_err(edit_error)?
        .document
        .total
            == 0,
        "Stale scene received a new comment"
    );
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.find_by_label("Review note").value() == Some(TEXT),
            "Artifact conflict discarded note text"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    click(cx, handle, "desktop-notes-refresh")?;
    wait(cx, shell)?;
    click(cx, handle, "desktop-notes-compare-version")?;
    screenshot(
        cx,
        handle,
        output,
        (1024., 700.),
        "native-note-version-conflict-dark",
    )?;
    recovery::wait_recovery(cx, shell)?;
    click(cx, handle, "desktop-notes-rebind")?;
    wait(cx, shell)?;
    recovery::wait_recovery(cx, shell)?;
    let current = Target::scene(&changed.board, 0).map_err(edit_error)?;
    let records = Repository::new(configuration(library))
        .list()
        .map_err(edit_error)?;
    ensure!(
        records.len() == 1
            && matches!(&records[0].draft.content, Content::Note { target: Some(saved), .. } if saved == &current),
        "Rebinding failed to retain the new context before clearing the old checkpoint"
    );
    click(cx, handle, "desktop-notes-save")?;
    wait(cx, shell)?;
    recovery::wait_recovery(cx, shell)?;
    ensure!(
        notes::contextual_document(
            library,
            reference,
            ContextQuery {
                target: &current,
                offset: 0
            }
        )
        .map_err(edit_error)?
        .document
        .total
            == 1,
        "Current version did not receive the native comment"
    );
    ensure!(
        Repository::new(configuration(library))
            .list()
            .map_err(edit_error)?
            .is_empty(),
        "Saved context note retained stale recovery buffers"
    );
    click(cx, handle, "desktop-notes-back")?;
    open(cx, handle, shell)?;
    click(cx, handle, "desktop-scenes-approve")?;
    wait(cx, shell)
}

pub(super) fn crash_fixture(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
) -> anyhow::Result<()> {
    click(cx, handle, "desktop-scene-comment")?;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        type_field(window, "Review note", TEXT, cx)
    })?;
    recovery::wait_recovery(cx, shell)?;
    std::process::exit(CRASH_EXIT);
}

pub(super) fn restart_journey(
    library: &Path,
    reference: &str,
    output: &Path,
) -> anyhow::Result<()> {
    let crashed = std::process::Command::new(std::env::current_exe()?)
        .arg("--scene-note-crash-fixture")
        .arg(library)
        .arg(reference)
        .output()?;
    ensure!(
        crashed.status.code() == Some(CRASH_EXIT),
        "Context note crash fixture failed: {}",
        String::from_utf8_lossy(&crashed.stderr)
    );
    let records = Repository::new(configuration(library))
        .list()
        .map_err(edit_error)?;
    ensure!(
        records.len() == 1,
        "Context note did not survive process exit"
    );
    let Content::Note {
        target: Some(original),
        ..
    } = &records[0].draft.content
    else {
        anyhow::bail!("Context note lost its target");
    };
    let changed = change_mood(library, reference)?;
    let current = Target::scene(&changed.board, 0).map_err(edit_error)?;
    ensure!(
        *original != current,
        "Restart fixture did not change the reviewed artifact"
    );
    let (mut cx, handle, shell) = window((760., 640.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.enable_recovery(configuration(library), cx)
        })
    });
    recovery::wait_recovery(&mut cx, &shell)?;
    let restore =
        gpui_kit::SharedString::from(format!("desktop-recovery-restore-{}", records[0].id));
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-appearance", cx);
        window.click("desktop-recovery-toggle", cx);
        bring_control_into_view(window, restore.clone().into(), cx)?;
        window.click(restore, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.find_by_label("Review note").value() == Some(TEXT),
            "Restart lost contextual note text"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    click(&mut cx, handle, "desktop-notes-save")?;
    wait(&mut cx, &shell)?;
    ensure!(
        notes::contextual_document(
            library,
            reference,
            ContextQuery {
                target: original,
                offset: 0
            }
        )
        .map_err(edit_error)?
        .document
        .total
            == 0,
        "Restored note silently adopted changed content"
    );
    click(&mut cx, handle, "desktop-notes-refresh")?;
    wait(&mut cx, &shell)?;
    click(&mut cx, handle, "desktop-notes-compare-version")?;
    screenshot(
        &mut cx,
        handle,
        output,
        (760., 640.),
        "native-note-recovered-version-light",
    )?;
    recovery::wait_recovery(&mut cx, &shell)?;
    click(&mut cx, handle, "desktop-notes-rebind")?;
    wait(&mut cx, &shell)?;
    recovery::wait_recovery(&mut cx, &shell)?;
    click(&mut cx, handle, "desktop-notes-save")?;
    wait(&mut cx, &shell)?;
    recovery::wait_recovery(&mut cx, &shell)?;
    ensure!(
        notes::contextual_document(
            library,
            reference,
            ContextQuery {
                target: &current,
                offset: 0
            }
        )
        .map_err(edit_error)?
        .document
        .total
            == 1,
        "Recovered current comment was not saved"
    );
    scenes::document(library, reference, Action::Approve(&changed.version)).map_err(edit_error)?;
    ensure!(
        Repository::new(configuration(library))
            .list()
            .map_err(edit_error)?
            .is_empty(),
        "Restart left saved context checkpoints"
    );
    Ok(())
}

fn change_mood(library: &Path, reference: &str) -> anyhow::Result<scenes::SceneSnapshot> {
    let saved = document(library, reference)?;
    let shot = &saved.board.direction.shots[0];
    let review = scenes::review(
        library,
        reference,
        Draft {
            expected_version: saved.version.clone(),
            unit: 0,
            action: shot.action.clone(),
            location: shot.location.clone(),
            mood: "watchful".into(),
        },
    )
    .map_err(edit_error)?;
    scenes::save(&review)
        .map_err(edit_error)?
        .content
        .context("Missing changed scenes")
}

pub(super) fn frame_comment(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    library: &Path,
    reference: &str,
) -> anyhow::Result<()> {
    let item = retold_app::desktop::media::scene_frame(library, reference, 0)
        .map_err(edit_error)?
        .context("Missing recorded frame")?;
    let bytes = item.preview_bytes().map_err(edit_error)?;
    let target = Target::frame(0, &bytes).map_err(edit_error)?;
    click(cx, handle, "desktop-frame-comment")?;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        type_field(
            window,
            "Review note",
            "Comment on these exact recorded pixels.",
            cx,
        )
    })?;
    click(cx, handle, "desktop-notes-save")?;
    wait(cx, shell)?;
    let saved = notes::contextual_document(
        library,
        reference,
        ContextQuery {
            target: &target,
            offset: 0,
        },
    )
    .map_err(edit_error)?;
    ensure!(
        saved.document.total == 1 && saved.current.as_ref() == Some(&target),
        "Native frame note did not bind verified pixels"
    );
    ensure!(
        std::fs::read(item.path)? == bytes,
        "Frame comment changed media bytes"
    );
    Ok(())
}
