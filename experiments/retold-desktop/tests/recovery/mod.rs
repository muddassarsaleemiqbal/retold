//! A real process exits with unsaved editors; another process restores and resolves their text.
use super::*;
use retold_app::desktop::recovery::{Checkpoint, Content, Repository};

const CRASH_EXIT: i32 = 23;

fn configuration(root: &Path) -> PathBuf {
    root.join("editing-recovery")
}

pub(super) fn crash_fixture(root: &Path, reference: &str, editor: &str) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.enable_recovery(configuration(root), cx);
            shell.open_workspace(root.into(), cx);
        })
    });
    wait(&mut cx, &shell)?;
    wait_recovery(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        let action = if editor == "script" {
            "desktop-open-script"
        } else {
            "desktop-open-notes"
        };
        bring_control_into_view(window, action.into(), cx)?;
        window.click(action, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let text = if editor == "script" {
        let document = retold_app::editing::script(
            &root.join("library"),
            reference,
            retold_app::editing::Action::Read,
        )
        .map_err(edit_error)?;
        format!(
            "{} Retained text हिंदी اردو.",
            document.content.context("Missing script")?.script.sections[0].text
        )
    } else {
        "Retained review हिंदी اردو English".into()
    };
    cx.update_window(handle, |_, window, cx| {
        if editor != "script" {
            bring_control_into_view(window, "desktop-notes-write".into(), cx)?;
            window.click("desktop-notes-write", cx);
        }
        type_field(
            window,
            if editor == "script" {
                "Script section text"
            } else {
                "Review note"
            },
            &text,
            cx,
        );
        Ok::<_, anyhow::Error>(())
    })??;
    wait_recovery(&mut cx, &shell)?;
    let records = Repository::new(configuration(root))
        .list()
        .map_err(edit_error)?;
    ensure!(
        records.len() == 1 && records[0].draft.content.text() == text,
        "Typing did not durably retain the latest editor text"
    );
    // Deliberately bypass native close handlers and destructors, as a killed frontend would.
    std::process::exit(CRASH_EXIT);
}

fn crash(root: &Path, reference: &str, editor: &str) -> anyhow::Result<Checkpoint> {
    let result = std::process::Command::new(std::env::current_exe()?)
        .arg("--recovery-crash-fixture")
        .arg(root)
        .arg(reference)
        .arg(editor)
        .output()?;
    ensure!(
        result.status.code() == Some(CRASH_EXIT),
        "Crash fixture failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let records = Repository::new(configuration(root))
        .list()
        .map_err(edit_error)?;
    ensure!(
        records.len() == 1,
        "Crash fixture did not preserve exactly one draft"
    );
    Ok(records[0].clone())
}

pub(super) fn journey(root: &Path, reference: &str, output: &Path) -> anyhow::Result<()> {
    let retained = crash(root, reference, "script")?;
    use retold_app::editing::{self, Action, SectionEdit};
    let library = root.join("library");
    let original = editing::script(&library, reference, Action::Read)
        .map_err(edit_error)?
        .content
        .context("Missing script")?;
    let external = format!("{} External edit.", original.script.sections[0].text);
    let changed = editing::script(
        &library,
        reference,
        Action::Save(SectionEdit {
            expected_version: &original.version,
            section: &original.script.sections[0].section,
            text: &external,
        }),
    )
    .map_err(edit_error)?
    .content
    .context("Missing external edit")?;
    let (mut cx, handle, shell) = reopen(root)?;
    screenshot(
        &mut cx,
        handle,
        output,
        (760., 640.),
        "recovery-retained-light",
    )?;
    restore(&mut cx, handle, &shell, &retained.id)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-script-overview".into(), cx)
    })??;
    screenshot(
        &mut cx,
        handle,
        output,
        (760., 640.),
        "recovery-script-text-light",
    )?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Script section text").value()
                == Some(retained.draft.content.text()),
            "Restart lost script text"
        );
        bring_control_into_view(window, "desktop-script-save".into(), cx)?;
        ensure!(
            window.find("desktop-script-save").disabled() == Some(true),
            "Restoration silently adopted production authority"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    ensure!(
        editing::script(&library, reference, Action::Read)
            .map_err(edit_error)?
            .content
            .context("Missing unchanged script")?
            .version
            == changed.version,
        "Restoration published a script edit"
    );
    screenshot(
        &mut cx,
        handle,
        output,
        (760., 640.),
        "recovery-script-compare-light",
    )?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-script-keep-local".into(), cx)?;
        window.click("desktop-script-keep-local", cx);
        bring_control_into_view(window, "desktop-script-save".into(), cx)?;
        window.click("desktop-script-save", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    wait_recovery(&mut cx, &shell)?;
    ensure!(
        Repository::new(configuration(root))
            .list()
            .map_err(edit_error)?
            .is_empty(),
        "Saved script retained a stale checkpoint"
    );
    let saved = editing::script(&library, reference, Action::Read)
        .map_err(edit_error)?
        .content
        .context("Missing saved script")?;
    ensure!(
        saved.script.sections[0].text == retained.draft.content.text() && !saved.approved,
        "Recovered script was not saved as an unapproved revision"
    );
    note_receipt_journey(root, reference)?;
    ensure!(
        !library.join(".retold/studio/queue.sqlite").exists(),
        "Recovery created a production queue"
    );
    Ok(())
}

fn note_receipt_journey(root: &Path, reference: &str) -> anyhow::Result<()> {
    let retained = crash(root, reference, "note")?;
    let Content::Note {
        target: _,
        append_id,
        source_version,
        text,
    } = &retained.draft.content
    else {
        anyhow::bail!("Expected note");
    };
    let library = root.join("library");
    // Simulate an append committed by the backend whose acknowledgement never reached the GUI.
    let saved = retold_app::notes::append(
        &library,
        reference,
        retold_app::notes::Append {
            expected_version: source_version,
            id: append_id,
            item: "video",
            text,
        },
    )
    .map_err(edit_error)?;
    let (mut cx, handle, shell) = reopen(root)?;
    restore(&mut cx, handle, &shell, &retained.id)?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Review note").value() == Some(text.as_str()),
            "Restart lost note text"
        );
        bring_control_into_view(window, "desktop-notes-save".into(), cx)?;
        window.click("desktop-notes-save", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    wait_recovery(&mut cx, &shell)?;
    let document =
        retold_app::notes::document(&library, reference, &retold_app::notes::Query::default())
            .map_err(edit_error)?;
    ensure!(
        document.total == saved.document.total,
        "Restoration lost the append identifier and duplicated a comment"
    );
    ensure!(
        Repository::new(configuration(root))
            .list()
            .map_err(edit_error)?
            .is_empty(),
        "Saved note retained a stale checkpoint"
    );
    Ok(())
}

fn reopen(root: &Path) -> anyhow::Result<(HeadlessAppContext, AnyWindowHandle, Entity<Shell>)> {
    let (mut cx, handle, shell) = window((760., 640.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.enable_recovery(configuration(root), cx)
        })
    });
    wait_recovery(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-appearance", cx);
        window.click("desktop-recovery-toggle", cx);
    })?;
    Ok((cx, handle, shell))
}

fn restore(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    id: &str,
) -> anyhow::Result<()> {
    let control = gpui_kit::SharedString::from(format!("desktop-recovery-restore-{id}"));
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, control.clone().into(), cx)?;
        window.click(control, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)
}

pub(super) fn wait_recovery(
    cx: &mut HeadlessAppContext,
    shell: &Entity<Shell>,
) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        cx.background_executor
            .advance_clock(Duration::from_millis(300));
        cx.run_until_parked();
        if cx.update(|cx| !shell.read(cx).recovery_pending()) {
            return Ok(());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "Native draft retention timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
