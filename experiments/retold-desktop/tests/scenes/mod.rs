//! The production journey publishes native direction before the existing reviewed Ready run.
mod notes;
use super::*;
use retold_app::{
    desktop::recovery::{Content, Repository},
    scenes::{self, Action, Draft},
};

const LOCAL_ACTION: &str = "An original figure closes the window and holds the curtain.";
const RECOVERED_ACTION: &str =
    "Retained scene: हिंदी اردو English. An original figure opens the gate.";
const CRASH_EXIT: i32 = 23;

fn configuration(library: &Path) -> PathBuf {
    library.join(".retold/native-editing-drafts")
}

fn document(library: &Path, reference: &str) -> anyhow::Result<scenes::SceneSnapshot> {
    scenes::document(library, reference, Action::Read)
        .map_err(edit_error)?
        .content
        .context("Missing saved scenes")
}

pub(super) fn frame_capture(library: &Path, reference: &str, output: &Path) -> anyhow::Result<()> {
    let frame = retold_app::desktop::media::scene_frame(library, reference, 0)
        .map_err(edit_error)?
        .context("Generated scene has no recorded frame")?;
    ensure!(
        !frame.preview_bytes().map_err(edit_error)?.is_empty(),
        "Recorded frame failed its content hash"
    );
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(library.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
    })?;
    open(&mut cx, handle, &shell)?;
    picker_journey(&mut cx, handle, &shell, output)?;
    cx.update_window(handle, |_, window, cx| {
        for control in ["desktop-scene-next", "desktop-scene-previous"] {
            bring_control_into_view(window, control.into(), cx)?;
            window.click(control, cx);
        }
        Ok::<_, anyhow::Error>(())
    })??;
    wait_player(&mut cx, handle, "Media loaded.")?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Scene action").value() == Some(RECOVERED_ACTION),
            "Rapid selection changed the final scene context"
        );
        bring_control_into_view(window, "desktop-media-frame".into(), cx)?;
        ensure!(
            window.find("desktop-media-frame").bounds().size.height > px(0.),
            "Scene player did not render its saved image"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(
        &mut cx,
        handle,
        output,
        (1024., 700.),
        "native-scene-frame-dark",
    )?;
    notes::frame_comment(&mut cx, handle, &shell, library, reference)
}

fn picker_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    output: &Path,
) -> anyhow::Result<()> {
    use gpui_kit::component::WindowExt as _;
    click(cx, handle, "desktop-scene-picker")?;
    cx.update_window(handle, |_, window, cx| window.input("Scene 2 ·", cx))?;
    screenshot(
        cx,
        handle,
        output,
        (1024., 700.),
        "native-scene-picker-dark",
    )?;
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))?;
    ensure!(
        cx.update(|cx| shell.read(cx).status.starts_with("Scene 2 selected")),
        "Search and Enter did not select the matching scene"
    );
    click(cx, handle, "desktop-scene-picker")?;
    cx.update_window(handle, |_, window, cx| {
        window.input("no matching scene xyz", cx);
        window.press("escape", cx);
        window.press("escape", cx);
        ensure!(
            !window.has_active_dialog(cx),
            "Escape did not dismiss the scene picker"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    click(cx, handle, "desktop-scene-previous")?;
    Ok(())
}

pub(super) fn journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    library: &Path,
    reference: &str,
    output: &Path,
) -> anyhow::Result<()> {
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.enable_recovery(configuration(library), cx)
        })
    });
    recovery::wait_recovery(cx, shell)?;
    let original = document(library, reference)?;
    let generated = library
        .join(reference)
        .join("work/03-storyboard/storyboard.json");
    let bytes = std::fs::read(&generated)?;
    open(cx, handle, shell)?;
    type_direction(
        cx,
        handle,
        LOCAL_ACTION,
        "A courtyard in Wisborg",
        "watchful",
    )?;
    ensure!(
        document(library, reference)?.version == original.version,
        "Typing published a scene edit"
    );
    click(cx, handle, "desktop-scenes-review-edit")?;
    wait(cx, shell)?;
    screenshot(
        cx,
        handle,
        output,
        (1024., 700.),
        "native-scene-impact-dark",
    )?;
    click(cx, handle, "desktop-scenes-confirm-save")?;
    wait(cx, shell)?;
    recovery::wait_recovery(cx, shell)?;
    let saved = document(library, reference)?;
    ensure!(
        saved.board.direction.shots[0].action == LOCAL_ACTION && !saved.approved,
        "Native scene save did not publish unapproved direction"
    );
    ensure!(
        std::fs::read(&generated)? == bytes,
        "Native edit overwrote generated originals"
    );
    conflict(cx, handle, shell, library, reference, output)?;
    click(cx, handle, "desktop-scenes-approve")?;
    wait(cx, shell)?;
    ensure!(
        document(library, reference)?.approved,
        "Scene approval did not bind saved content"
    );
    notes::journey(cx, handle, shell, library, reference, output)?;
    recovery::wait_recovery(cx, shell)?;
    click(cx, handle, "desktop-scenes-overview")?;
    restart_journey(library, reference, output)?;
    notes::restart_journey(library, reference, output)?;
    let state = retold_app::studio::resolve_workspace_state(Some(library));
    let queue = retold_app::queue::QueueStore::open_read_only(&state.join("queue.sqlite"))
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    ensure!(
        queue
            .list_page(&Default::default())
            .map_err(|error| anyhow::anyhow!(error.to_string()))?
            .total
            == 1,
        "Direction editing or recovery admitted a production job"
    );
    Ok(())
}

fn open(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
) -> anyhow::Result<()> {
    click(cx, handle, "desktop-open-scenes")?;
    wait(cx, shell)
}

fn type_direction(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    action: &str,
    location: &str,
    mood: &str,
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| {
        type_field(window, "Scene action", action, cx);
        type_field(window, "Scene location", location, cx);
        type_field(window, "Scene mood", mood, cx);
    })?;
    Ok(())
}

fn click(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    id: &'static str,
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, id.into(), cx)?;
        window.click(id, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    Ok(())
}

fn conflict(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    library: &Path,
    reference: &str,
    output: &Path,
) -> anyhow::Result<()> {
    let text = format!("{LOCAL_ACTION} The figure steps back.");
    type_direction(cx, handle, &text, "A courtyard in Wisborg", "watchful")?;
    click(cx, handle, "desktop-scenes-overview")?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Settings, cx);
        bring_control_into_view(window, "desktop-return-scene-draft".into(), cx)?;
        window.click("desktop-return-scene-draft", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    let current = document(library, reference)?;
    let external = draft(
        &current,
        "External direction: an original figure pauses.",
        "A courtyard in Wisborg",
        "grave",
    );
    let review = scenes::review(library, reference, external).map_err(edit_error)?;
    scenes::save(&review).map_err(edit_error)?;
    click(cx, handle, "desktop-scenes-review-edit")?;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.find_by_label("Scene action").value() == Some(text.as_str()),
            "Conflict lost local direction"
        );
        ensure!(
            window.try_find("desktop-scenes-confirm-save").is_none(),
            "Stale draft offered a save confirmation"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(
        cx,
        handle,
        output,
        (1024., 700.),
        "native-scene-conflict-dark",
    )?;
    click(cx, handle, "desktop-scenes-refresh")?;
    wait(cx, shell)?;
    click(cx, handle, "desktop-scenes-keep-local")?;
    click(cx, handle, "desktop-scenes-review-edit")?;
    wait(cx, shell)?;
    let current = document(library, reference)?;
    let reviewed = scenes::review(
        library,
        reference,
        draft(&current, &text, "A courtyard in Wisborg", "watchful"),
    )
    .map_err(edit_error)?;
    let mut tampered = reviewed.clone();
    tampered.draft.mood = "tampered".into();
    ensure!(
        scenes::save(&tampered).is_err(),
        "Mutated reviewed fields were published"
    );
    click(cx, handle, "desktop-scenes-confirm-save")?;
    wait(cx, shell)?;
    ensure!(
        scenes::save(&reviewed).is_err(),
        "Retry duplicated a saved revision"
    );
    ensure!(
        document(library, reference)?.board.direction.shots[0].action == text,
        "Conflict resolution did not save local direction"
    );
    Ok(())
}

fn draft(current: &scenes::SceneSnapshot, action: &str, location: &str, mood: &str) -> Draft {
    Draft {
        expected_version: current.version.clone(),
        unit: current.board.units[0].id,
        action: action.into(),
        location: location.into(),
        mood: mood.into(),
    }
}

pub(super) fn crash_fixture(library: &Path, reference: &str, note: bool) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.enable_recovery(configuration(library), cx);
            shell.open_workspace(library.into(), cx);
        })
    });
    wait(&mut cx, &shell)?;
    recovery::wait_recovery(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        ensure!(
            shell.read(cx).selected_video.as_deref() == Some(reference),
            "Wrong scene crash production"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    open(&mut cx, handle, &shell)?;
    if note {
        notes::crash_fixture(&mut cx, handle, &shell)?;
    }
    type_direction(
        &mut cx,
        handle,
        RECOVERED_ACTION,
        "A courtyard in Wisborg",
        "hopeful",
    )?;
    recovery::wait_recovery(&mut cx, &shell)?;
    let records = Repository::new(configuration(library))
        .list()
        .map_err(edit_error)?;
    ensure!(
        records.len() == 1,
        "Scene crash fixture did not retain exactly one draft"
    );
    std::process::exit(CRASH_EXIT);
}

fn restart_journey(library: &Path, reference: &str, output: &Path) -> anyhow::Result<()> {
    let crashed = std::process::Command::new(std::env::current_exe()?)
        .arg("--scene-crash-fixture")
        .arg(library)
        .arg(reference)
        .output()?;
    ensure!(
        crashed.status.code() == Some(CRASH_EXIT),
        "Scene crash fixture failed: {}",
        String::from_utf8_lossy(&crashed.stderr)
    );
    let entries = Repository::new(configuration(library))
        .list()
        .map_err(edit_error)?;
    ensure!(entries.len() == 1, "Retained scene draft missing");
    ensure!(
        matches!(&entries[0].draft.content, Content::Scene { draft, .. } if draft.action == RECOVERED_ACTION && draft.location == "A courtyard in Wisborg" && draft.mood == "hopeful"),
        "Not all scene fields survived the process exit"
    );
    let before = document(library, reference)?.version;
    let (mut cx, handle, shell) = window((760., 640.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.enable_recovery(configuration(library), cx)
        })
    });
    recovery::wait_recovery(&mut cx, &shell)?;
    let restore =
        gpui_kit::SharedString::from(format!("desktop-recovery-restore-{}", entries[0].id));
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-appearance", cx);
        window.click("desktop-recovery-toggle", cx);
        bring_control_into_view(window, restore.clone().into(), cx)?;
        window.click(restore, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    ensure!(
        document(library, reference)?.version == before,
        "Restoration published scene direction"
    );
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Scene action").value() == Some(RECOVERED_ACTION),
            "Restoration lost action text"
        );
        ensure!(
            window.find_by_label("Scene location").value() == Some("A courtyard in Wisborg"),
            "Restoration lost location"
        );
        ensure!(
            window.find_by_label("Scene mood").value() == Some("hopeful"),
            "Restoration lost mood"
        );
        bring_control_into_view(window, "desktop-scenes-review-edit".into(), cx)?;
        ensure!(
            window.find("desktop-scenes-review-edit").disabled() == Some(true),
            "Recovery bypassed comparison"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(
        &mut cx,
        handle,
        output,
        (760., 640.),
        "native-scene-recovered-light",
    )?;
    click(&mut cx, handle, "desktop-scenes-keep-local")?;
    click(&mut cx, handle, "desktop-scenes-review-edit")?;
    wait(&mut cx, &shell)?;
    click(&mut cx, handle, "desktop-scenes-confirm-save")?;
    wait(&mut cx, &shell)?;
    click(&mut cx, handle, "desktop-scenes-approve")?;
    wait(&mut cx, &shell)?;
    recovery::wait_recovery(&mut cx, &shell)?;
    ensure!(
        Repository::new(configuration(library))
            .list()
            .map_err(edit_error)?
            .is_empty(),
        "Recovered scene save retained a stale checkpoint"
    );
    ensure!(
        document(library, reference)?.approved,
        "Recovered direction was not explicitly approved"
    );
    Ok(())
}
