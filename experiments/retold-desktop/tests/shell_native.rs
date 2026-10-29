//! Render and drive the actual native shell against an isolated managed workspace.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, ensure};
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, HeadlessAppContext, MouseButton, Role,
    ScrollDelta, WindowBounds, WindowOptions, point, px, size,
};
use retold_desktop::{assets::Assets, shell::Shell, theme};
mod recovery;
mod scenes;
mod workspaces;

fn main() -> anyhow::Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|argument| {
        argument == "--scene-crash-fixture" || argument == "--scene-note-crash-fixture"
    }) {
        ensure!(
            arguments.len() == 3,
            "Scene crash fixture needs a library and reference"
        );
        return scenes::crash_fixture(
            Path::new(&arguments[1]),
            arguments[2]
                .to_str()
                .context("Invalid scene fixture reference")?,
            arguments[0] == "--scene-note-crash-fixture",
        );
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--recovery-crash-fixture")
    {
        ensure!(
            arguments.len() == 4,
            "Recovery fixture needs a root, reference and editor"
        );
        return recovery::crash_fixture(
            Path::new(&arguments[1]),
            arguments[2].to_str().context("Invalid reference")?,
            arguments[3].to_str().context("Invalid editor")?,
        );
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--media-preview-fixture")
    {
        return isolated_media_journey();
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--source-draft-fixture")
    {
        ensure!(
            arguments.len() == 3,
            "Source draft needs a library and reference"
        );
        return source_draft_journey(
            Path::new(&arguments[1]),
            arguments[2].to_str().context("Invalid reference")?,
        );
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--run-review-fixture")
    {
        ensure!(
            arguments.len() == 3,
            "Run fixture needs a library and reference"
        );
        return native_run_journey(
            Path::new(&arguments[1]),
            arguments[2].to_str().context("Invalid fixture reference")?,
        );
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--ready-export-fixture")
    {
        ensure!(
            arguments.len() == 3,
            "Ready export fixture needs a library and reference"
        );
        let reference = arguments[2]
            .to_str()
            .context("Fixture reference is not UTF-8")?;
        return ready_export_journey(Path::new(&arguments[1]), reference);
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--draft-save-fixture")
    {
        ensure!(
            arguments.len() == 2,
            "Draft fixture needs one isolated root"
        );
        return draft_save_journey(Path::new(&arguments[1]));
    }
    let output = std::env::var_os("RETOLD_GUI_EVIDENCE").map_or_else(
        || PathBuf::from("/tmp/retold-gpui-g0-evidence"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&output)?;
    let fixture = tempfile::tempdir()?;
    install(fixture.path())?;
    for dimensions in [(1440., 900.), (1024., 700.), (760., 640.)] {
        exercise(&output, fixture.path(), dimensions)?;
    }
    activity_journey(&output)?;
    catalog_journey(&output)?;
    isolated_media_journey()?;
    isolated_draft_journey()?;
    hostile_journey(&output)?;
    println!("Native shell workspace, draft, inhibition and rendering checks passed.");
    Ok(())
}

fn isolated_media_journey() -> anyhow::Result<()> {
    let fixture = tempfile::tempdir()?;
    install(fixture.path())?;
    seed_catalog(fixture.path())?;
    let library = fixture.path().join("library");
    let reference = "channel/story-001";
    let media = library.join(reference).join("deliverables/en/saved.mp4");
    std::fs::create_dir_all(media.parent().context("Missing delivery directory")?)?;
    let generated = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=12",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=8000:cl=mono",
            "-t",
            "2",
            "-threads",
            "1",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&media)
        .output()?;
    ensure!(
        generated.status.success(),
        "Synthetic preview failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.open_workspace(fixture.path().into(), cx)
        })
    });
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
    })?;
    let output = std::env::var_os("RETOLD_GUI_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/retold-gpui-g0-evidence"));
    native_media_journey(&mut cx, handle, &shell, &library, reference, &output)?;
    ensure!(
        !library.join(".retold/studio/queue.sqlite").exists(),
        "Media browsing created a queue"
    );
    Ok(())
}

fn isolated_draft_journey() -> anyhow::Result<()> {
    let fixture = tempfile::tempdir()?;
    install(fixture.path())?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .arg("--draft-save-fixture")
        .arg(fixture.path())
        .env("HOME", fixture.path().join("isolated-home"))
        .env(
            "RETOLD_CONFIG_DIR",
            fixture.path().join("isolated-installation"),
        )
        .env_remove("RETOLD_STATE")
        .env_remove("RETOLD_WORKSPACE")
        .env_remove("RETOLD_LIBRARY")
        .env_remove("XDG_DATA_HOME")
        .output()?;
    ensure!(
        output.status.success(),
        "Native draft fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Try to break the app: delete the open workspace, then open one with corrupt files,
/// and visit every page each time. Nothing may panic or leave the app busy.
fn hostile_journey(output: &Path) -> anyhow::Result<()> {
    let vanished = tempfile::tempdir()?;
    install(vanished.path())?;
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.open_workspace(vanished.path().into(), cx)
        })
    });
    wait(&mut cx, &shell)?;
    std::fs::remove_dir_all(vanished.path())?;
    cx.update_window(handle, |_, window, cx| window.press("cmd-r", cx))?;
    wait(&mut cx, &shell)?;
    visit_every_page(&mut cx, handle, &shell)?;
    screenshot(
        &mut cx,
        handle,
        output,
        (1024., 700.),
        "hostile-vanished-workspace",
    )?;

    let corrupt = tempfile::tempdir()?;
    install(corrupt.path())?;
    for file in walk(corrupt.path())? {
        if file
            .extension()
            .is_some_and(|extension| extension == "toml" || extension == "json")
        {
            std::fs::write(&file, "{{ not valid ]] \u{0} 💥")?;
        }
    }
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.open_workspace(corrupt.path().into(), cx)
        })
    });
    wait(&mut cx, &shell)?;
    visit_every_page(&mut cx, handle, &shell)?;
    screenshot(
        &mut cx,
        handle,
        output,
        (1024., 700.),
        "hostile-corrupt-workspace",
    )?;
    let empty = tempfile::tempdir()?;
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.open_workspace(empty.path().into(), cx)
        })
    });
    wait(&mut cx, &shell)?;
    visit_every_page(&mut cx, handle, &shell)?;
    screenshot(
        &mut cx,
        handle,
        output,
        (1024., 700.),
        "hostile-empty-folder",
    )?;

    let usable = tempfile::tempdir()?;
    install(usable.path())?;
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.open_workspace(usable.path().into(), cx)
        })
    });
    wait(&mut cx, &shell)?;
    let wild = "شاہ 🎬 ".repeat(700);
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-new-video".into(), cx)?;
        window.click("desktop-new-video", cx);
        type_field(window, "Title", &wild, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    for destination in retold_desktop::shell::Destination::ALL {
        cx.update_window(handle, |_, window, cx| navigate(window, destination, cx))?;
    }
    wait(&mut cx, &shell)?;
    visit_every_page(&mut cx, handle, &shell)?;
    screenshot(&mut cx, handle, output, (1024., 700.), "hostile-wild-input")?;
    Ok(())
}

fn visit_every_page(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
) -> anyhow::Result<()> {
    for destination in retold_desktop::shell::Destination::ALL {
        cx.update_window(handle, |_, window, cx| navigate(window, destination, cx))?;
        wait(cx, shell)?;
        ensure!(
            !cx.update(|cx| shell.read(cx).busy),
            "The app stayed busy on {destination:?}"
        );
    }
    Ok(())
}

fn walk(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(walk(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}

fn catalog_journey(output: &Path) -> anyhow::Result<()> {
    let fixture = tempfile::tempdir()?;
    install(fixture.path())?;
    seed_catalog(fixture.path())?;
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.open_workspace(fixture.path().into(), cx)
        })
    });
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        catalog_scroll(window, false, cx);
        window.click("desktop-video-first", cx);
    })?;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, false, cx);
        window.click("desktop-video-next", cx);
    })?;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Open Catalogue story 065").role() == Some(Role::Button),
            "Second video page did not load"
        );
        catalog_scroll(window, false, cx);
        window.click("desktop-video-previous", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    catalog_search_journey(&mut cx, handle, &shell, output)?;
    ensure!(
        !fixture
            .path()
            .join("library/.retold/studio/queue.sqlite")
            .exists(),
        "Browsing initialized a queue"
    );
    Ok(())
}

fn catalog_scroll(window: &mut gpui_kit::Window, top: bool, cx: &mut gpui_kit::App) {
    window.scroll(
        "desktop-content",
        ScrollDelta::Pixels(point(px(0.), px(if top { 100_000. } else { -100_000. }))),
        cx,
    );
}

fn catalog_search_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    output: &Path,
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, true, cx);
        window.click("desktop-video-archived", cx);
    })?;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Open Catalogue story 200").role() == Some(Role::Button),
            "Archived view did not show the archived video"
        );
        window.click("desktop-video-active", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, true, cx);
        type_field(window, "Find a video", "Catalogue story 001", cx);
        window.press("enter", cx);
    })?;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-command-search", cx);
        window.input("Catalogue story 000", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-command-search-library", cx);
    })?;
    wait(cx, shell)?;
    screenshot(
        cx,
        handle,
        output,
        (1024., 700.),
        "catalog-older-search-dark",
    )?;
    cx.update_window(handle, |_, window, cx| {
        window.click(("desktop-video", 0_usize), cx);
    })?;
    ensure!(
        cx.update(|cx| shell.read(cx).selected_video.as_deref() == Some("channel/story-000")),
        "Search could not open the older video"
    );
    Ok(())
}

fn seed_catalog(root: &Path) -> anyhow::Result<()> {
    const VIDEO_COUNT: usize = 201;
    for index in 0..VIDEO_COUNT {
        let name = format!("story-{index:03}");
        let directory = root.join("library/channel").join(&name);
        std::fs::create_dir_all(directory.join(".retold"))?;
        let manifest = serde_json::json!({"schema":1,"id":format!("native-{index:03}"),
            "created":if index == 0 { "2026-10-01T00:00:00Z" } else { "2026-10-18T00:00:00Z" },
            "title":format!("Catalogue story {index:03}"), "guide":{"id":"fixture","version":"1","hash":"fixture"},
            "channel":"channel","inputs":{},"variants":["en:en"],"quality":"standard","budget":0,
            "skip_reviews":false,"folder":[name],"cast_levels":0});
        std::fs::write(
            directory.join(".retold/run.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        if index + 1 == VIDEO_COUNT {
            std::fs::write(directory.join(".archived"), "")?;
        }
    }
    Ok(())
}

fn navigate(
    window: &mut gpui_kit::Window,
    destination: retold_desktop::shell::Destination,
    cx: &mut gpui_kit::App,
) {
    if retold_desktop::shell::navigation::compact(window) {
        window.click("desktop-navigation", cx);
    }
    window.click(
        gpui_kit::SharedString::from(format!("desktop-nav-{}", destination.identifier())),
        cx,
    );
}

fn type_field(window: &mut gpui_kit::Window, label: &str, text: &str, cx: &mut gpui_kit::App) {
    for _ in 0..8 {
        let target = window.find_by_label(label).bounds();
        let viewport = window.find("desktop-content").bounds();
        if target.origin.y >= viewport.origin.y && target.bottom() <= viewport.bottom() {
            break;
        }
        let delta = if target.origin.y < viewport.origin.y {
            viewport.origin.y - target.origin.y + px(8.)
        } else {
            viewport.bottom() - target.bottom() - px(8.)
        };
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), delta)),
            cx,
        );
    }
    let position = window.find_by_label(label).bounds().center();
    window.pointer_move(position, None, cx);
    window.pointer_down(position, MouseButton::Left, cx);
    window.pointer_up(position, MouseButton::Left, cx);
    window.press("cmd-a", cx);
    window.input(text, cx);
    assert_eq!(
        window.find_by_label(label).value(),
        Some(text),
        "Typing did not update {label}"
    );
}

fn draft_save_journey(root: &Path) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((1440., 900.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-new-video".into(), cx)?;
        window.click("desktop-new-video", cx);
        type_field(window, "Title", "Native saved story", cx);
        window.click("desktop-choice-kind-0", cx);
        bring_control_into_view(window, "desktop-save-video".into(), cx)?;
        window.click("desktop-save-video", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let videos = cx
        .update(|cx| {
            shell
                .read(cx)
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.videos.clone())
        })
        .context("Native save lost its workspace")?;
    ensure!(
        videos.len() == 1 && videos[0].title == "Native saved story",
        "Saved draft is not visible in the native library"
    );
    let library = root.join("library");
    let state = retold_app::studio::resolve_workspace_state(Some(&library));
    ensure!(
        !state.join("queue.sqlite").exists(),
        "Offline save unexpectedly admitted a job"
    );
    let manifest_path = library.join(&videos[0].reference).join(".retold/run.json");
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
    ensure!(
        manifest["budget"] == 0,
        "Native draft default is not a zero-dollar cap"
    );
    ensure!(
        manifest["skip_reviews"] == false,
        "Native draft skipped content review"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click(("desktop-video", 0_usize), cx);
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(-1400.))),
            cx,
        );
        ensure!(
            window.find_by_label("Run allowance USD").value() == Some("0"),
            "Run allowance is not zero by default"
        );
        ensure!(
            window.find_by_label("Run maximum paid requests").value() == Some("0"),
            "Run request ceiling is not zero by default"
        );
        window.click("desktop-review-run", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.try_find("desktop-confirm-run").is_none(),
            "An unconfigured fixture unexpectedly offers execution"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    ensure!(
        !state.join("queue.sqlite").exists(),
        "Reviewing a plan admitted a job"
    );
    let output = std::env::var_os("RETOLD_GUI_EVIDENCE").map_or_else(
        || PathBuf::from("/tmp/retold-gpui-g0-evidence"),
        PathBuf::from,
    );
    screenshot(
        &mut cx,
        handle,
        &output,
        (1440., 900.),
        "saved-draft-blocked-plan",
    )?;
    cx.update_window(handle, |_, window, cx| {
        type_field(window, "Video budget in USD", "2.50", cx);
        bring_control_into_view(window, "desktop-video-budget-set".into(), cx)?;
        window.click("desktop-video-budget-set", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    ensure!(
        cx.update(|cx| shell.read(cx).status.contains("$2.50")),
        "Video budget was not set"
    );
    script_edit_journey(&mut cx, handle, &shell, root, &videos[0].reference)?;
    notes_journey(root, &videos[0].reference, &output)?;
    recovery::journey(root, &videos[0].reference, &output)?;
    preferences_journey(root)?;
    providers_journey(root)?;
    delivery_journey(root, &videos[0].reference)?;
    budget_journey(root)?;
    Ok(())
}

fn notes_journey(root: &Path, reference: &str, output: &Path) -> anyhow::Result<()> {
    use retold_app::notes::{self, Append, Query};
    let library = root.join("library");
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        bring_control_into_view(window, "desktop-open-notes".into(), cx)?;
        window.click("desktop-open-notes", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let query = Query {
        target: Some("video".into()),
        offset: 0,
    };
    let empty = notes::document(&library, reference, &query).map_err(edit_error)?;
    ensure!(empty.total == 0, "Notes fixture already had comments");
    let text = "Keep this feedback: हिंदी اردو English\nReview the opening scene.";
    cx.update_window(handle, |_, window, cx| {
        type_field(window, "Review note", text, cx);
        bring_control_into_view(window, "desktop-notes-back".into(), cx)?;
        window.click("desktop-notes-back", cx);
        navigate(window, retold_desktop::shell::Destination::Settings, cx);
        bring_control_into_view(window, "desktop-notes-return".into(), cx)?;
        window.click("desktop-notes-return", cx);
        ensure!(
            window.find_by_label("Review note").value() == Some(text),
            "Navigation discarded the note draft"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    let external_id = notes::draft_id().map_err(edit_error)?;
    notes::append(
        &library,
        reference,
        Append {
            expected_version: &empty.version,
            id: &external_id,
            item: "video",
            text: "External review feedback",
        },
    )
    .map_err(edit_error)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-notes-save".into(), cx)?;
        window.click("desktop-notes-save", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.find_by_label("Review note").value() == Some(text),
            "A conflict discarded the note draft"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    ensure!(
        notes::document(&library, reference, &query)
            .map_err(edit_error)?
            .total
            == 1,
        "Stale note was appended"
    );
    screenshot(
        &mut cx,
        handle,
        output,
        (1024., 700.),
        "notes-conflict-dark",
    )?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-notes-refresh".into(), cx)?;
        window.click("desktop-notes-refresh", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Review note").value() == Some(text),
            "Refresh discarded the note draft"
        );
        bring_control_into_view(window, "desktop-notes-save".into(), cx)?;
        let save_position = window.find("desktop-notes-save").bounds().center();
        window.click("desktop-notes-save", cx);
        window.pointer_down(save_position, MouseButton::Left, cx);
        window.pointer_up(save_position, MouseButton::Left, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let saved = notes::document(&library, reference, &query).map_err(edit_error)?;
    ensure!(
        saved.total == 2 && saved.notes.iter().any(|note| note.text == text),
        "Native note was not saved exactly once"
    );
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-notes-write".into(), cx)?;
        window.click("desktop-notes-write", cx);
        ensure!(
            window.find_by_label("Review note").value() == Some(""),
            "Acknowledged save did not clear its matching draft"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    notes_compact_capture(root, output)?;
    ensure!(
        !library.join(".retold/studio/queue.sqlite").exists(),
        "Review notes started a production queue"
    );
    Ok(())
}

fn notes_compact_capture(root: &Path, output: &Path) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((760., 640.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        bring_control_into_view(window, "desktop-open-notes".into(), cx)?;
        window.click("desktop-open-notes", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-appearance", cx);
        bring_control_into_view(window, "desktop-notes-write".into(), cx)
    })??;
    screenshot(
        &mut cx,
        handle,
        output,
        (760., 640.),
        "notes-reopened-light",
    )
}

fn preferences_journey(root: &Path) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((760., 640.))?;
    cx.update_window(handle, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.restore_preferences(window, cx))
    })?;
    wait(&mut cx, &shell)?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Settings, cx);
        catalog_scroll(window, false, cx);
        window.click(("desktop-color-mode", 1_usize), cx);
        window.click(("desktop-text-size", 1_usize), cx);
    })?;
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, false, cx);
        window.click("desktop-remember-window", cx);
    })?;
    wait_preferences(&mut cx, |saved| {
        saved.appearance.mode == retold_app::desktop::preferences::Appearance::Light
            && saved.zoom == 125
            && saved
                .window
                .is_some_and(|size| size.width == 760 && size.height == 640)
    })?;
    let output = PathBuf::from("/tmp/retold-gpui-g0-evidence");
    screenshot(
        &mut cx,
        handle,
        &output,
        (760., 640.),
        "preferences-125-light",
    )?;
    preferences_reopen(root, &output)?;
    Ok(())
}

fn wait_preferences(
    cx: &mut HeadlessAppContext,
    accepted: impl Fn(&retold_app::desktop::preferences::Preferences) -> bool,
) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        let saved = retold_app::desktop::preferences::load().map_err(edit_error)?;
        if accepted(&saved) {
            return Ok(());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "Presentation preferences were not saved"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn preferences_reopen(root: &Path, output: &Path) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((760., 640.))?;
    cx.update_window(handle, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.restore_preferences(window, cx))
    })?;
    wait(&mut cx, &shell)?;
    let restored = cx.update(|cx| {
        shell.read(cx).light
            && shell.read(cx).snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.context.root == root.canonicalize().unwrap_or_default()
            })
    });
    ensure!(
        restored,
        "Studio did not restore appearance and its last workspace"
    );
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.rem_size() == px(retold_desktop::theme::BASE_FONT * 1.25),
            "Saved text zoom was not restored"
        );
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        catalog_scroll(window, false, cx);
        window.click(("desktop-video", 0_usize), cx);
        ensure!(
            window.find_by_label("Run allowance USD").value() == Some("0"),
            "Reopening restored spend authority"
        );
        ensure!(
            window.find_by_label("Run maximum paid requests").value() == Some("0"),
            "Reopening restored paid request authority"
        );
        navigate(window, retold_desktop::shell::Destination::Settings, cx);
        catalog_scroll(window, false, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    for (index, zoom) in [(2_usize, 150), (3_usize, 200)] {
        large_text_capture(&mut cx, handle, output, (index, zoom))?;
    }
    appearance_captures(&mut cx, handle, &shell, output)?;
    ensure!(
        !root.join("library/.retold/studio/queue.sqlite").exists(),
        "Restoring presentation state created a job queue"
    );
    Ok(())
}

fn appearance_captures(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    output: &Path,
) -> anyhow::Result<()> {
    use retold_app::desktop::preferences::Appearance;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, ("desktop-text-size", 0_usize).into(), cx)?;
        window.click(("desktop-text-size", 0_usize), cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait_preferences(cx, |saved| saved.zoom == 100)?;
    for (index, mode, label) in [
        (0_usize, Appearance::System, "system"),
        (2_usize, Appearance::Dark, "dark"),
        (1_usize, Appearance::Light, "light"),
    ] {
        cx.update_window(handle, |_, window, cx| {
            bring_control_into_view(window, ("desktop-color-mode", index).into(), cx)?;
            window.click(("desktop-color-mode", index), cx);
            if mode == Appearance::System {
                let light = matches!(
                    window.appearance(),
                    gpui_kit::WindowAppearance::Light | gpui_kit::WindowAppearance::VibrantLight
                );
                ensure!(
                    shell.read(cx).light == light,
                    "System appearance did not follow the native window"
                );
            }
            Ok::<_, anyhow::Error>(())
        })??;
        wait_preferences(cx, |saved| saved.appearance.mode == mode)?;
        screenshot(
            cx,
            handle,
            output,
            (760., 640.),
            &format!("preferences-{label}"),
        )?;
    }
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-high-contrast", cx)
    })?;
    wait_preferences(cx, |saved| saved.appearance.high_contrast)?;
    screenshot(
        cx,
        handle,
        output,
        (760., 640.),
        "preferences-high-contrast-light",
    )?;
    cx.update_window(handle, |_, window, cx| {
        window.click(("desktop-color-mode", 2_usize), cx)
    })?;
    wait_preferences(cx, |saved| {
        saved.appearance.high_contrast && saved.appearance.mode == Appearance::Dark
    })?;
    screenshot(
        cx,
        handle,
        output,
        (760., 640.),
        "preferences-high-contrast-dark",
    )?;
    Ok(())
}

fn bring_control_into_view(
    window: &mut gpui_kit::Window,
    id: gpui_kit::ElementId,
    cx: &mut gpui_kit::App,
) -> anyhow::Result<()> {
    for _ in 0..8 {
        let target = window.find(id.clone()).bounds();
        let viewport = window.find("desktop-content").bounds();
        if target.origin.y >= viewport.origin.y && target.bottom() <= viewport.bottom() {
            return Ok(());
        }
        let delta = if target.origin.y < viewport.origin.y {
            viewport.origin.y - target.origin.y + px(8.)
        } else {
            viewport.bottom() - target.bottom() - px(8.)
        };
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), delta)),
            cx,
        );
    }
    anyhow::bail!("Control could not be scrolled into the content viewport")
}

fn large_text_capture(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
    selection: (usize, u16),
) -> anyhow::Result<()> {
    let (index, percent) = selection;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, ("desktop-text-size", index).into(), cx)?;
        window.click(("desktop-text-size", index), cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait_preferences(cx, |saved| saved.zoom == percent)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, ("desktop-text-size", index).into(), cx)?;
        let stop = window.find("desktop-emergency-stop").bounds();
        ensure!(
            stop.origin.x >= px(0.)
                && stop.origin.y >= px(0.)
                && stop.right() <= px(760.)
                && stop.bottom() <= px(640.),
            "Stop all clipped at {percent} percent text size"
        );
        let zoom = window.find(("desktop-text-size", index)).bounds();
        let content = window.find("desktop-content").bounds();
        ensure!(
            zoom.origin.y >= content.origin.y && zoom.bottom() <= content.bottom(),
            "Text-size controls cannot be reached at {percent} percent zoom"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(
        cx,
        handle,
        output,
        (760., 640.),
        &format!("preferences-{percent}-light"),
    )?;
    Ok(())
}

fn seed_script(root: &Path, reference: &str) -> anyhow::Result<()> {
    let video = root.join("library").join(reference);
    let metadata = video.join(".retold");
    let envelope: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/script-envelope.json"))?;
    let key = envelope["record"]["key"]
        .as_str()
        .context("Missing fixture cache key")?;
    std::fs::create_dir_all(metadata.join("cache"))?;
    std::fs::create_dir_all(video.join("work/02-script"))?;
    std::fs::write(
        metadata.join("state.json"),
        include_str!("fixtures/script-state.json"),
    )?;
    std::fs::write(
        metadata.join("cache").join(format!("script-{key}.json")),
        include_str!("fixtures/script-envelope.json"),
    )?;
    std::fs::write(
        video.join("work/02-script/script.json"),
        serde_json::to_vec_pretty(&envelope["value"])?,
    )?;
    Ok(())
}

fn edit_error(error: retold_app::protocol::Diagnostic) -> anyhow::Error {
    anyhow::anyhow!("{}: {}", error.code.as_str(), error.message)
}

fn script_edit_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    root: &Path,
    reference: &str,
) -> anyhow::Result<()> {
    use retold_app::editing::{self, Action};
    let library = root.join("library");
    seed_script(root, reference)?;
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(3000.))),
            cx,
        );
        window.click("desktop-open-script", cx);
    })?;
    wait(cx, shell)?;
    let original = editing::script(&library, reference, Action::Read)
        .map_err(edit_error)?
        .content
        .context("Missing script")?;
    ensure!(
        original.editable,
        "The seeded script envelope did not verify"
    );
    let local = original.script.sections[0]
        .text
        .replace("vampire", "stranger");
    cx.update_window(handle, |_, window, cx| {
        type_field(window, "Script section text", &local, cx);
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(-350.))),
            cx,
        );
        window.click("desktop-script-save", cx);
    })?;
    wait(cx, shell)?;
    let saved = editing::script(&library, reference, Action::Read)
        .map_err(edit_error)?
        .content
        .context("Missing saved edit")?;
    ensure!(
        saved.script.sections[0].text == local && !saved.approved,
        "Native Save did not persist actual unapproved content"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-script-approve", cx)
    })?;
    wait(cx, shell)?;
    let approved = editing::script(&library, reference, Action::Read)
        .map_err(edit_error)?
        .content
        .context("Missing approved script")?;
    ensure!(
        approved.approved,
        "Native approval did not bind current content"
    );
    script_captures(cx, handle, root)?;
    writing_check_journey(cx, handle, shell)?;
    script_conflict_journey(cx, handle, shell, &library, reference, &approved)?;
    Ok(())
}

/// Check writing audits the open section in process and shows its score.
fn writing_check_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
) -> anyhow::Result<()> {
    click_control(cx, handle, "desktop-script-check-writing".into())?;
    wait(cx, shell)?;
    let checked = cx.update(|cx| shell.read(cx).editor_writing_score().is_some());
    ensure!(checked, "Check writing produced no score");
    let output = PathBuf::from("/tmp/retold-gpui-g0-evidence");
    screenshot(
        cx,
        handle,
        &output,
        (1440., 900.),
        "script-writing-check-dark",
    )
}

fn script_captures(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    root: &Path,
) -> anyhow::Result<()> {
    let output = PathBuf::from("/tmp/retold-gpui-g0-evidence");
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(3000.))),
            cx,
        );
    })?;
    screenshot(cx, handle, &output, (1440., 900.), "script-approved-dark")?;
    let (mut compact, compact_handle, compact_shell) = window((760., 640.))?;
    compact
        .update(|cx| compact_shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut compact, &compact_shell)?;
    compact.update_window(compact_handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        window.click("desktop-open-script", cx);
        window.click("desktop-appearance", cx);
    })?;
    wait(&mut compact, &compact_shell)?;
    screenshot(
        &mut compact,
        compact_handle,
        &output,
        (760., 640.),
        "script-approved-light",
    )?;
    Ok(())
}

fn script_conflict_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    library: &Path,
    reference: &str,
    approved: &retold_app::editing::ScriptSnapshot,
) -> anyhow::Result<()> {
    use retold_app::editing::{self, Action, SectionEdit};
    let local = "English draft — हिन्दी १२३ — اردو 123";
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(3000.))),
            cx,
        );
        type_field(window, "Script section text", local, cx);
        navigate(window, retold_desktop::shell::Destination::Home, cx);
        window.click("desktop-return-script-draft", cx);
        ensure!(
            window.find_by_label("Script section text").value() == Some(local),
            "Returning from Home lost the unsaved script"
        );
        window.click("desktop-script-overview", cx);
        window.click("desktop-all-videos", cx);
        window.click(("desktop-video", 0_usize), cx);
        Ok::<_, anyhow::Error>(())
    })??;
    editing::script(
        library,
        reference,
        Action::Save(SectionEdit {
            expected_version: &approved.version,
            section: "hook",
            text: "Another client's saved version.",
        }),
    )
    .map_err(edit_error)?;
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(-350.))),
            cx,
        );
        window.click("desktop-script-save", cx);
    })?;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(3000.))),
            cx,
        );
        ensure!(
            window.find_by_label("Script section text").value() == Some(local),
            "Conflict lost the local multilingual draft"
        );
        window.click("desktop-script-refresh", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    let output = PathBuf::from("/tmp/retold-gpui-g0-evidence");
    screenshot(cx, handle, &output, (1440., 900.), "script-conflict-dark")?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-script-keep-local".into(), cx)?;
        window.click("desktop-script-keep-local", cx);
        bring_control_into_view(window, "desktop-script-save".into(), cx)?;
        window.click("desktop-script-save", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    let saved = editing::script(library, reference, Action::Read)
        .map_err(edit_error)?
        .content
        .context("Missing resolved script")?;
    ensure!(
        saved.script.sections[0].text == local,
        "Explicit conflict choice did not save the retained draft"
    );
    ensure!(
        !saved.approved,
        "Conflict resolution retained an obsolete approval"
    );
    Ok(())
}

fn wait_control(cx: &mut HeadlessAppContext, shell: &Entity<Shell>) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if cx.update(|cx| !shell.read(cx).control_busy) {
            return Ok(());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "Dedicated control request timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn activity_journey(output: &Path) -> anyhow::Result<()> {
    use retold_app::queue::{JobSpec, QueueStore};
    let fixture = tempfile::tempdir()?;
    install(fixture.path())?;
    let library = fixture.path().join("library");
    let state = retold_app::studio::resolve_workspace_state(Some(&library));
    let queue = QueueStore::open(&state.join("queue.sqlite"))?;
    for index in 0..66 {
        queue.enqueue(
            JobSpec {
                idempotency_key: &format!("native-{index}"),
                video_ref: &format!("fixture-{index}"),
                plan_hash: "fixture-reviewed-hash",
            },
            "2026-10-25T00:00:00Z",
        )?;
    }
    let (mut cx, handle, shell) = window((1440., 900.))?;
    cx.update(|cx| {
        shell.update(cx, |shell, cx| {
            shell.open_workspace(fixture.path().into(), cx)
        })
    });
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Activity, cx)
    })?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-activity-next", cx)
    })?;
    wait(&mut cx, &shell)?;
    let count = cx.update(|cx| {
        shell
            .read(cx)
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.activity.as_ref().ok())
            .map(|activity| activity.page.jobs.len())
    });
    ensure!(
        count == Some(2),
        "Activity did not load the next bounded page"
    );
    cx.update(|cx| shell.update(cx, |shell, _| shell.busy = true));
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-emergency-stop", cx)
    })?;
    ensure!(
        cx.update(|cx| shell.read(cx).busy && shell.read(cx).control_busy),
        "Stop all did not dispatch independently of the occupied normal lane"
    );
    wait_control(&mut cx, &shell)?;
    ensure!(
        cx.update(|cx| shell.read(cx).busy),
        "Control completion incorrectly released the normal request lane"
    );
    cx.update(|cx| shell.update(cx, |shell, _| shell.busy = false));
    ensure!(
        queue.dispatch_inhibited()?,
        "Control did not persist admission inhibition"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click(("desktop-job", 0_usize), cx)
    })?;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.find("desktop-job-events-start").visible(),
            "Saved event controls are not reachable"
        );
        ensure!(
            window.try_find("desktop-stop-job").is_none(),
            "Terminal job still offers a stop control"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(&mut cx, handle, output, (1440., 900.), "job-stopped-dark")?;
    ensure!(
        !state.join("studio.sock").exists(),
        "Observing jobs started a worker"
    );
    Ok(())
}

fn install(root: &Path) -> anyhow::Result<()> {
    let resources =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/retold-guide/tests/fixtures");
    copy(
        &resources.join("guides/narrative-fixture"),
        &root.join("config/guides/narrative-fixture"),
    )?;
    copy(
        &resources.join("channels/test-studio"),
        &root.join("config/channels/test-studio"),
    )?;
    std::fs::create_dir_all(root.join("library"))?;
    std::fs::write(
        root.join("workspace.toml"),
        "schema_version = 1\nid = 'native-fixture'\nname = 'Native fixture'\nconfig_path = 'config'\nlibrary_path = 'library'\n",
    )?;
    Ok(())
}

fn copy(source: &Path, destination: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn window(
    dimensions: (f32, f32),
) -> anyhow::Result<(HeadlessAppContext, AnyWindowHandle, Entity<Shell>)> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update(retold_desktop::shell::commands::bind_keys);
    let (handle, shell) = cx.update(|cx| {
        theme::apply(false, cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(dimensions.0), px(dimensions.1)),
                })),
                focus: false,
                show: false,
                ..WindowOptions::default()
            },
            cx,
            |window, cx| cx.new(|cx| Shell::new(window, cx)),
        )
    })?;
    cx.run_until_parked();
    Ok((cx, handle, shell))
}

fn wait(cx: &mut HeadlessAppContext, shell: &Entity<Shell>) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if cx.update(|cx| !shell.read(cx).busy && !shell.read(cx).control_busy) {
            return Ok(());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "Native background request timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn screenshot(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
    dimensions: (f32, f32),
    state: &str,
) -> anyhow::Result<()> {
    cx.capture_screenshot(handle)?.save(output.join(format!(
        "shell-{}x{}-{state}.png",
        dimensions.0, dimensions.1
    )))?;
    Ok(())
}

fn command_search_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    output: &Path,
    dimensions: (f32, f32),
) -> anyhow::Result<()> {
    use gpui_kit::component::WindowExt as _;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-command-search", cx);
        window.input("Go to Activity", cx);
    })?;
    screenshot(cx, handle, output, dimensions, "command-search-dark")?;
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))?;
    cx.run_until_parked();
    ensure!(
        cx.update(|cx| shell.read(cx).destination == retold_desktop::shell::Destination::Activity),
        "Command search did not navigate to Activity"
    );
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-k", cx);
        ensure!(
            window.has_active_dialog(cx),
            "Keyboard shortcut did not reopen command search"
        );
        window.input("no matching command xyz", cx);
        window.press("escape", cx);
        window.press("escape", cx);
        ensure!(
            !window.has_active_dialog(cx),
            "Escape did not dismiss command search"
        );
        navigate(window, retold_desktop::shell::Destination::Home, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.update_window(handle, |_, window, cx| window.press("cmd-3", cx))?;
    cx.run_until_parked();
    ensure!(
        cx.update(|cx| shell.read(cx).destination == retold_desktop::shell::Destination::Series),
        "Cmd-3 did not open Series"
    );
    cx.update_window(handle, |_, window, cx| window.press("cmd-1", cx))?;
    cx.run_until_parked();
    ensure!(
        cx.update(|cx| shell.read(cx).destination == retold_desktop::shell::Destination::Home),
        "Cmd-1 did not return Home"
    );
    Ok(())
}

/// Forms: open Help's actions, fill the "Explain" form with a choice-free text field, run it,
/// then open a Budget form that has choices and toggles and capture both.
fn tools_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    output: &Path,
    dimensions: (f32, f32),
) -> anyhow::Result<()> {
    open_action(
        cx,
        handle,
        retold_desktop::shell::Destination::Help,
        "Schema",
    )?;
    click_control(cx, handle, "desktop-form-run".into())?;
    wait(cx, shell)?;
    screenshot(cx, handle, output, dimensions, "form-result-dark")?;
    click_control(cx, handle, "desktop-form-close".into())?;
    open_action(
        cx,
        handle,
        retold_desktop::shell::Destination::Budget,
        "Account limit",
    )?;
    click_control(cx, handle, ("desktop-form-choice", 64_usize).into())?;
    screenshot(cx, handle, output, dimensions, "form-choices-dark")?;
    click_control(cx, handle, "desktop-form-close".into())?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Home, cx)
    })?;
    cx.run_until_parked();
    Ok(())
}

fn click_control(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    id: gpui_kit::ElementId,
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, id.clone(), cx)?;
        window.click(id, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.run_until_parked();
    Ok(())
}

/// Navigate, reveal the page's actions and open the one whose button reads `label`.
fn open_action(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    destination: retold_desktop::shell::Destination,
    label: &str,
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| navigate(window, destination, cx))?;
    cx.run_until_parked();
    click_control(cx, handle, "desktop-more-actions".into())?;
    let developer = cx.update_window(handle, |_, window, _| {
        window.try_find("desktop-developer-tools").is_some()
    })?;
    if developer {
        click_control(cx, handle, "desktop-developer-tools".into())?;
    }
    for position in 0..64_usize {
        let found = cx.update_window(handle, |_, window, _| {
            window
                .try_find(("desktop-action", position))
                .is_some_and(|button| button.label() == Some(label))
        })?;
        if found {
            return click_control(cx, handle, ("desktop-action", position).into());
        }
    }
    anyhow::bail!("Action {label} was not offered")
}

/// One capture per destination with the workspace open, for side-by-side visual review.
fn destination_gallery(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
    dimensions: (f32, f32),
) -> anyhow::Result<()> {
    for destination in retold_desktop::shell::Destination::ALL {
        cx.update_window(handle, |_, window, cx| navigate(window, destination, cx))?;
        cx.run_until_parked();
        let name = format!("gallery-{}-dark", destination.identifier());
        screenshot(cx, handle, output, dimensions, &name)?;
    }
    capture_variant(
        cx,
        handle,
        output,
        dimensions,
        (
            "desktop-appearance".into(),
            "desktop-appearance".into(),
            "light",
        ),
    )?;
    if dimensions.0 >= 1400. {
        cx.update_window(handle, |_, window, cx| {
            navigate(window, retold_desktop::shell::Destination::Settings, cx)
        })?;
        cx.run_until_parked();
        capture_variant(
            cx,
            handle,
            output,
            dimensions,
            (
                "desktop-high-contrast".into(),
                "desktop-high-contrast".into(),
                "contrast",
            ),
        )?;
        cx.update_window(handle, |_, window, cx| {
            navigate(window, retold_desktop::shell::Destination::Settings, cx)
        })?;
        cx.run_until_parked();
        capture_variant(
            cx,
            handle,
            output,
            dimensions,
            (
                ("desktop-text-size", 3_usize).into(),
                ("desktop-text-size", 0_usize).into(),
                "text200",
            ),
        )?;
    }
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Home, cx);
    })?;
    Ok(())
}

/// Toggle a presentation control, capture Home and Series, then restore it.
fn capture_variant(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
    dimensions: (f32, f32),
    (enable, restore, label): (gpui_kit::ElementId, gpui_kit::ElementId, &str),
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| {
        let _header_controls_need_no_scrolling =
            bring_control_into_view(window, enable.clone(), cx);
        window.click(enable, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.run_until_parked();
    for destination in [
        retold_desktop::shell::Destination::Home,
        retold_desktop::shell::Destination::Series,
    ] {
        cx.update_window(handle, |_, window, cx| navigate(window, destination, cx))?;
        cx.run_until_parked();
        let name = format!("gallery-{}-{label}", destination.identifier());
        screenshot(cx, handle, output, dimensions, &name)?;
    }
    if label != "light" {
        cx.update_window(handle, |_, window, cx| {
            navigate(window, retold_desktop::shell::Destination::Settings, cx)
        })?;
        cx.run_until_parked();
    }
    cx.update_window(handle, |_, window, cx| {
        let _header_controls_need_no_scrolling =
            bring_control_into_view(window, restore.clone(), cx);
        window.click(restore, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.run_until_parked();
    Ok(())
}

fn exercise(output: &Path, root: &Path, dimensions: (f32, f32)) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window(dimensions)?;
    screenshot(&mut cx, handle, output, dimensions, "welcome-dark")?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut cx, &shell)?;
    ensure!(
        cx.update(|cx| shell.read(cx).snapshot.is_some()),
        "Workspace did not load"
    );
    screenshot(&mut cx, handle, output, dimensions, "home-dark")?;
    command_search_journey(&mut cx, handle, &shell, output, dimensions)?;
    destination_gallery(&mut cx, handle, output, dimensions)?;
    tools_journey(&mut cx, handle, &shell, output, dimensions)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-new-video".into(), cx)?;
        window.click("desktop-new-video", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.update_window(handle, |_, window, cx| {
        let title = window.find_by_label("Title");
        ensure!(
            title.role() == Some(Role::MultilineTextInput),
            "Guide question lacks input semantics"
        );
        let position = title.bounds().center();
        window.pointer_move(position, None, cx);
        window.pointer_down(position, MouseButton::Left, cx);
        window.pointer_up(position, MouseButton::Left, cx);
        window.input("A story in हिन्दी and اردو", cx);
        ensure!(
            window.find_by_label("Title").value() == Some("A story in हिन्दी and اردو"),
            "Guide question did not accept text"
        );
        ensure!(
            window.find_by_label("Video safety cap USD").value() == Some("0.00"),
            "Draft cap is not zero by default"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(&mut cx, handle, output, dimensions, "draft-dark")?;
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            "desktop-content",
            ScrollDelta::Pixels(point(px(0.), px(-1400.))),
            cx,
        );
        ensure!(
            window.find("desktop-save-video").visible(),
            "Draft save action cannot be reached by scrolling"
        );
        window.click("desktop-cancel-video", cx);
        navigate(window, retold_desktop::shell::Destination::Activity, cx);
        window.click("desktop-stop-all", cx);
        window.click("desktop-stop-all-confirm", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let inhibited = cx.update(|cx| {
        shell
            .read(cx)
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.activity.as_ref().ok())
            .is_some_and(|activity| activity.dispatch.inhibited_by.is_some())
    });
    ensure!(inhibited, "Stop all did not persist a workspace hold");
    screenshot(&mut cx, handle, output, dimensions, "held-dark")?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-appearance", cx)
    })?;
    screenshot(&mut cx, handle, output, dimensions, "held-light")?;
    ensure!(
        !retold_app::studio::resolve_workspace_state(Some(&root.join("library")))
            .join("studio.sock")
            .exists(),
        "Native control unexpectedly started a worker"
    );
    let _ = std::fs::metadata(root).context("fixture was lost")?;
    Ok(())
}

fn providers_journey(root: &Path) -> anyhow::Result<()> {
    use retold_app::desktop::providers::{self, Change};
    let config = root.join("config");
    ensure!(
        !config.join("providers.toml").exists(),
        "Provider fixture must begin unconfigured"
    );
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Providers, cx)
    })?;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-provider-add", cx);
        type_field(window, "Provider ID", "native-voice", cx);
        type_field(window, "Parallel requests", "2", cx);
        catalog_scroll(window, false, cx);
        window.click("desktop-provider-preview", cx);
    })?;
    wait(&mut cx, &shell)?;
    ensure!(
        !config.join("providers.toml").exists(),
        "Provider review changed settings before confirmation"
    );
    let output = PathBuf::from("/tmp/retold-gpui-g0-evidence");
    screenshot(
        &mut cx,
        handle,
        &output,
        (1024., 700.),
        "provider-reviewed-change-dark",
    )?;
    let current = providers::inspect(&config).map_err(edit_error)?;
    let another = Change::Add {
        id: "other-client".into(),
        definition: "kind = 'local_voice'\nmax_parallel = 1\n".into(),
    };
    let review = providers::preview(&config, &current.version, &another).map_err(edit_error)?;
    providers::apply(&review).map_err(edit_error)?;
    let their_bytes = std::fs::read(config.join("providers.toml"))?;
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, false, cx);
        window.click("desktop-provider-apply", cx);
    })?;
    wait(&mut cx, &shell)?;
    ensure!(
        std::fs::read(config.join("providers.toml"))? == their_bytes,
        "A stale provider review overwrote another client"
    );
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, true, cx);
        window.click("desktop-providers-refresh", cx);
    })?;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.find_by_label("Provider ID").value() == Some("native-voice"),
            "Conflict discarded the provider draft"
        );
        catalog_scroll(window, false, cx);
        window.click("desktop-provider-preview", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, false, cx);
        window.click("desktop-provider-apply", cx);
    })?;
    wait(&mut cx, &shell)?;
    let saved = providers::inspect(&config).map_err(edit_error)?;
    ensure!(
        saved
            .providers
            .iter()
            .any(|provider| provider.id == "native-voice" && provider.max_parallel == 2),
        "Confirmed provider was not saved"
    );
    let tts = retold_app::providers::routing_tiers()
        .iter()
        .position(|tier| *tier == "tts")
        .context("No speech tier")?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-provider-show-all".into(), cx)?;
        window.click("desktop-provider-show-all", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, ("desktop-provider-route", tts).into(), cx)?;
        window.click(("desktop-provider-route", tts), cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.update_window(handle, |_, window, cx| {
        type_field(window, "Fallback order", "native-voice:default", cx);
        catalog_scroll(window, false, cx);
        window.click("desktop-provider-preview", cx);
    })?;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, false, cx);
        window.click("desktop-provider-apply", cx);
    })?;
    wait(&mut cx, &shell)?;
    let saved = providers::inspect(&config).map_err(edit_error)?;
    ensure!(
        saved.routing["tts"] == ["native-voice:default"],
        "Confirmed fallback chain was not saved"
    );
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-provider-observe".into(), cx)?;
        window.click("desktop-provider-observe", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, false, cx);
    })?;
    screenshot(
        &mut cx,
        handle,
        &output,
        (1024., 700.),
        "provider-offline-health-dark",
    )?;
    ensure!(
        !config.join("provider-health.json").exists(),
        "Offline inspection created provider health state"
    );
    ensure!(
        !root.join("library/.retold/studio/queue.sqlite").exists(),
        "Provider setup admitted a job"
    );
    Ok(())
}

fn budget_journey(root: &Path) -> anyhow::Result<()> {
    use retold_app::account::{
        self, Account,
        report::{Grouping, Query},
        types::{Limit, Scope, Unit, Window},
    };
    let directory = account::canonical_directory()?;
    ensure!(
        !directory.exists(),
        "Budget fixture must use an isolated empty account"
    );
    let query = Query {
        window: Window::Month,
        by: Grouping::Provider,
    };
    ensure!(
        retold_app::desktop::budget::inspect(query).is_err(),
        "Missing account read unexpectedly succeeded"
    );
    ensure!(!directory.exists(), "Budget read initialized accounting");
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Budget, cx)
    })?;
    wait(&mut cx, &shell)?;
    ensure!(!directory.exists(), "Native Budget initialized accounting");
    let account = Account::initialize(&directory, "Asia/Karachi")?;
    account.set_limits(vec![Limit {
        scope: Scope::Account,
        window: Window::Month,
        resources: [(Unit::UsdMicros, 1_234_567), (Unit::Requests, 10)].into(),
    }])?;
    let database = directory.join("accounting.sqlite");
    let original = std::fs::read(&database)?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-budget-refresh", cx);
    })?;
    wait(&mut cx, &shell)?;
    let report = retold_app::desktop::budget::inspect(query).map_err(edit_error)?;
    ensure!(
        report.ceiling.as_ref().context("Missing ceiling")?[&Unit::UsdMicros].available
            == 1_234_567,
        "Native report lost exact headroom"
    );
    let output = PathBuf::from("/tmp/retold-gpui-g0-evidence");
    screenshot(
        &mut cx,
        handle,
        &output,
        (1024., 700.),
        "budget-month-headroom-dark",
    )?;
    cx.update_window(handle, |_, window, cx| {
        window.click(("desktop-budget-period", 2_usize), cx);
    })?;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        window.click(("desktop-budget-group", 2_usize), cx);
    })?;
    wait(&mut cx, &shell)?;
    ensure!(
        std::fs::read(database)? == original,
        "Budget browsing changed accounting"
    );
    ensure!(
        !root.join("library/.retold/studio/queue.sqlite").exists(),
        "Budget browsing admitted a job"
    );
    Ok(())
}

fn delivery_journey(root: &Path, reference: &str) -> anyhow::Result<()> {
    use retold_app::{
        desktop,
        plan::{ExportOptions, PlanOptions},
    };
    let library = root.join("library");
    let mut options = ExportOptions {
        editions: vec!["en".into()],
        formats: vec!["16x9".into()],
        project_only: true,
        ..ExportOptions::default()
    };
    let project = desktop::review_export(
        &library,
        reference,
        &desktop::ExportRequest {
            delivery: options.clone(),
            ..desktop::ExportRequest::default()
        },
    )
    .map_err(edit_error)?;
    ensure!(
        project.plan().authority.usd_micros == 0 && project.plan().authority.requests == 0,
        "Delivery inherited paid run authority"
    );
    ensure!(
        !project.plan().is_unblocked(),
        "Unprepared video unexpectedly ready for export"
    );
    ensure!(project.submit().is_err(), "Blocked delivery was submitted");
    ensure!(
        desktop::review_run(
            &library,
            reference,
            &PlanOptions {
                export: Some(options.clone()),
                ..PlanOptions::default()
            }
        )
        .is_err(),
        "Run review accepted delivery scope"
    );
    options.project_only = false;
    let rendered = desktop::review_export(
        &library,
        reference,
        &desktop::ExportRequest {
            delivery: options.clone(),
            ..desktop::ExportRequest::default()
        },
    )
    .map_err(edit_error)?;
    ensure!(
        project.plan().plan_hash != rendered.plan().plan_hash,
        "Rendering reused project-only confirmation"
    );
    let capped = desktop::review_export(
        &library,
        reference,
        &desktop::ExportRequest {
            delivery: options.clone(),
            spend: "0.01".into(),
            max_requests: 1,
        },
    )
    .map_err(edit_error)?;
    ensure!(
        capped.plan().plan_hash != rendered.plan().plan_hash
            && capped.plan().authority.usd_micros == 10_000
            && capped.plan().authority.requests == 1,
        "Export allowance was not pinned in its own review"
    );
    for dimensions in [(1024., 700.), (760., 640.)] {
        let (mut cx, handle, shell) = window(dimensions)?;
        cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(root.into(), cx)));
        wait(&mut cx, &shell)?;
        cx.update_window(handle, |_, window, cx| {
            navigate(window, retold_desktop::shell::Destination::Videos, cx);
            window.click(("desktop-video", 0_usize), cx);
            bring_control_into_view(window, "desktop-open-export".into(), cx)?;
            window.click("desktop-open-export", cx);
            bring_control_into_view(window, "desktop-export-review".into(), cx)?;
            window.click("desktop-export-review", cx);
            Ok::<_, anyhow::Error>(())
        })??;
        wait(&mut cx, &shell)?;
        cx.update_window(handle, |_, window, cx| {
            if window.try_find("desktop-export-confirm").is_some() {
                bring_control_into_view(window, "desktop-export-confirm".into(), cx)?;
                window.click("desktop-export-confirm", cx);
            }
            Ok::<_, anyhow::Error>(())
        })??;
        let output = PathBuf::from("/tmp/retold-gpui-g0-evidence");
        screenshot(
            &mut cx,
            handle,
            &output,
            dimensions,
            "export-blocked-project-dark",
        )?;
        cx.update_window(handle, |_, window, cx| {
            bring_control_into_view(window, ("desktop-export-output", 1_usize).into(), cx)?;
            window.click(("desktop-export-output", 1_usize), cx);
            ensure!(
                window.try_find_by_label("Confirm project export").is_none(),
                "Changed export scope retained its prior confirmation"
            );
            bring_control_into_view(window, "desktop-export-review".into(), cx)?;
            window.click("desktop-export-review", cx);
            Ok::<_, anyhow::Error>(())
        })??;
        wait(&mut cx, &shell)?;
        cx.update_window(handle, |_, window, cx| {
            if window.try_find("desktop-export-confirm").is_some() {
                bring_control_into_view(window, "desktop-export-confirm".into(), cx)?;
                window.click("desktop-export-confirm", cx);
            }
            window.click("desktop-appearance", cx);
            Ok::<_, anyhow::Error>(())
        })??;
        screenshot(
            &mut cx,
            handle,
            &output,
            dimensions,
            "export-blocked-render-light",
        )?;
    }
    ensure!(
        !library.join(".retold/studio/queue.sqlite").exists(),
        "Blocked export created a queue"
    );
    Ok(())
}

fn source_draft_journey(library: &Path, reference: &str) -> anyhow::Result<()> {
    let original = std::env::var_os("RETOLD_NATIVE_SOURCE").context("Missing source fixture")?;
    let bytes = std::fs::read(original)?;
    let source = library
        .parent()
        .context("Missing sandbox")?
        .join("Story source اردو.txt ");
    std::fs::write(&source, &bytes)?;
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(library.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click("desktop-new-video", cx);
        type_field(window, "Title", "Nosferatu", cx);
        window.click("desktop-choice-kind-0", cx);
        type_field(window, "Year (optional)", "1922", cx);
        bring_control_into_view(window, "desktop-source-preview".into(), cx)?;
        type_field(
            window,
            "Story source file",
            source.to_str().context("Invalid source path")?,
            cx,
        );
        window.click("desktop-source-preview", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    std::fs::write(&source, "Changed after review")?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-save-video".into(), cx)?;
        type_field(window, "Video safety cap USD", "5", cx);
        window.click("desktop-save-video", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    ensure!(
        !library.join(reference).exists(),
        "Changed source published a draft"
    );
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.find("desktop-save-video").role() == Some(Role::Button),
            "Source conflict discarded the form"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    std::fs::write(&source, &bytes)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-source-preview".into(), cx)?;
        window.click("desktop-source-preview", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-save-video".into(), cx)?;
        window.click("desktop-save-video", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let captured = library
        .join(reference)
        .join("work/01-research/source-file.txt");
    ensure!(
        std::fs::read(&captured).with_context(|| format!(
            "Source draft missing {}: {}",
            captured.display(),
            cx.update(|cx| shell.read(cx).status.clone())
        ))? == bytes,
        "Draft did not preserve reviewed source"
    );
    std::fs::write(source, "Original file can now change")?;
    ensure!(
        !library.join(".retold/studio/queue.sqlite").exists(),
        "Draft save created a queue"
    );
    Ok(())
}

fn native_run_journey(library: &Path, reference: &str) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(library.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        ensure!(
            shell.read(cx).selected_video.as_deref() == Some(reference),
            "Wrong production video"
        );
        bring_control_into_view(window, "desktop-review-run".into(), cx)?;
        ensure!(
            window.find_by_label("Run allowance USD").value() == Some("0"),
            "Run inherited authority"
        );
        type_field(window, "Run allowance USD", "1", cx);
        type_field(window, "Run maximum paid requests", "2000", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    native_submit_run(&mut cx, handle, &shell)?;
    let first = wait_native_job(&mut cx, library, 1)?;
    ensure!(
        first.error_code.as_deref() == Some("RG-REVIEW-REQUIRED"),
        "Expected character gate: {first:?}"
    );
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        bring_control_into_view(window, "desktop-open-characters".into(), cx)?;
        window.click("desktop-open-characters", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let document = retold_app::desktop::characters::document(
        library,
        reference,
        retold_app::desktop::characters::Action::Read(None),
    )
    .map_err(edit_error)?;
    ensure!(
        !document.sheets.is_empty(),
        "Production made no reviewable sheets"
    );
    let output = std::env::var_os("RETOLD_GUI_EVIDENCE")
        .map(PathBuf::from)
        .context("Native production evidence directory missing")?;
    std::fs::create_dir_all(&output)?;
    native_character_compact_capture(library, reference, &output)?;
    native_sheet_conflict(
        &mut cx,
        handle,
        &shell,
        (library, reference, &document.sheets[0].name),
        &output,
    )?;
    for index in 0..document.sheets.len() {
        cx.update_window(handle, |_, window, cx| {
            bring_control_into_view(window, ("desktop-character", index).into(), cx)?;
            window.click(("desktop-character", index), cx);
            Ok::<_, anyhow::Error>(())
        })??;
        wait(&mut cx, &shell)?;
        cx.update_window(handle, |_, window, cx| {
            bring_control_into_view(window, "desktop-character-approve".into(), cx)?;
            ensure!(
                window.find("desktop-character-approve").disabled() != Some(true),
                "Sheet not available for approval"
            );
            Ok::<_, anyhow::Error>(())
        })??;
        screenshot(
            &mut cx,
            handle,
            &output,
            (1024., 700.),
            &format!("native-character-{index}"),
        )?;
        cx.update_window(handle, |_, window, cx| {
            window.click("desktop-character-approve", cx)
        })?;
        wait(&mut cx, &shell)?;
        let approved = retold_app::desktop::characters::document(
            library,
            reference,
            retold_app::desktop::characters::Action::Read(Some(&document.sheets[index].name)),
        )
        .map_err(edit_error)?;
        ensure!(
            approved
                .selected
                .is_some_and(|review| review.sheet.approved),
            "Native approval did not persist"
        );
    }
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-characters-back".into(), cx)?;
        window.click("desktop-characters-back", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    scenes::journey(&mut cx, handle, &shell, library, reference, &output)?;
    native_submit_run(&mut cx, handle, &shell)?;
    let ready = wait_native_job(&mut cx, library, 2)?;
    ensure!(
        ready.status == retold_app::queue::JobStatus::Completed,
        "Native generation failed: {ready:?}"
    );
    scenes::frame_capture(library, reference, &output)?;
    workspaces::journey(library, &output)?;
    ready_export_journey_with_count(library, reference, 3)?;
    Ok(())
}

fn native_character_compact_capture(
    library: &Path,
    reference: &str,
    output: &Path,
) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((760., 640.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(library.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        ensure!(
            shell.read(cx).selected_video.as_deref() == Some(reference),
            "Wrong compact fixture video"
        );
        bring_control_into_view(window, "desktop-open-characters".into(), cx)?;
        window.click("desktop-open-characters", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-appearance", cx);
        ensure!(
            window.viewport_size() == size(px(760.), px(640.)),
            "Compact character window did not resize"
        );
        ensure!(
            shell.read(cx).light,
            "Compact character capture did not use light appearance"
        );
        bring_control_into_view(window, "desktop-character-approve".into(), cx)
    })??;
    screenshot(
        &mut cx,
        handle,
        output,
        (760., 640.),
        "native-character-light",
    )
}

fn native_sheet_conflict(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    scope: (&Path, &str, &str),
    output: &Path,
) -> anyhow::Result<()> {
    let (library, reference, name) = scope;
    let stage = library.join(reference).join("work/04-characters");
    let record = stage.join("review.json");
    let original = std::fs::read(&record)?;
    let mut changed: serde_json::Value = serde_json::from_slice(&original)?;
    let assets = changed.as_object_mut().context("Invalid sheet records")?;
    let renamed = assets.remove(name).context("Missing reviewed character")?;
    assets.insert(format!("{name} revised"), renamed);
    std::fs::write(&record, serde_json::to_vec(&changed)?)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-character-approve".into(), cx)?;
        window.click("desktop-character-approve", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    ensure!(
        !stage.join("approved.json").exists(),
        "Stale sheet was approved"
    );
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.try_find_by_label("Approve sheet").is_none(),
            "Failed preview retained approval"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(
        cx,
        handle,
        output,
        (1024., 700.),
        "native-character-conflict",
    )?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-characters-refresh".into(), cx)?;
        window.click("desktop-characters-refresh", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.try_find_by_label("Approve sheet").is_some(),
            "Refresh stayed bound to the removed character name"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    std::fs::write(&record, &original)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-characters-refresh".into(), cx)?;
        window.click("desktop-characters-refresh", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    Ok(())
}

fn native_submit_run(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-review-run".into(), cx)?;
        window.click("desktop-review-run", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-confirm-run".into(), cx)?;
        ensure!(
            window.find("desktop-confirm-run").disabled() != Some(true),
            "Native reviewed run is blocked: {}",
            shell.read(cx).status
        );
        window.click("desktop-confirm-run", cx);
        window.click("desktop-confirm-run", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)
}

fn wait_native_job(
    cx: &mut HeadlessAppContext,
    library: &Path,
    count: usize,
) -> anyhow::Result<retold_app::queue::JobRecord> {
    let queue = retold_app::queue::QueueStore::open_read_only(
        &library.join(".retold/studio/queue.sqlite"),
    )?;
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        let page = queue.list_page(&retold_app::queue::ListOptions::default())?;
        ensure!(
            page.jobs.len() == count,
            "Expected exactly {count} native jobs: {page:?}"
        );
        let job = page
            .jobs
            .iter()
            .max_by_key(|job| &job.created_at)
            .context("No native job")?;
        if job.status.is_terminal() {
            return Ok(job.clone());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(120),
            "Native job did not settle: {job:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn ready_export_journey(library: &Path, reference: &str) -> anyhow::Result<()> {
    ready_export_journey_with_count(library, reference, 1)
}

fn ready_export_journey_with_count(
    library: &Path,
    reference: &str,
    expected_jobs: usize,
) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(library.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
        ensure!(
            shell.read(cx).selected_video.as_deref() == Some(reference),
            "Native fixture selected a different video"
        );
        bring_control_into_view(window, "desktop-open-export".into(), cx)?;
        window.click("desktop-open-export", cx);
        bring_control_into_view(window, ("desktop-export-format", 1_usize).into(), cx)?;
        window.click(("desktop-export-format", 1_usize), cx);
        bring_control_into_view(window, ("desktop-export-output", 1_usize).into(), cx)?;
        window.click(("desktop-export-output", 1_usize), cx);
        bring_control_into_view(window, "desktop-export-authority".into(), cx)?;
        ensure!(
            window.find_by_label("Export allowance USD").value() == Some("0"),
            "Export inherited an allowance"
        );
        ensure!(
            window.find_by_label("Export maximum paid requests").value() == Some("0"),
            "Export inherited request authority"
        );
        type_field(window, "Export allowance USD", "1", cx);
        type_field(window, "Export maximum paid requests", "2000", cx);
        bring_control_into_view(window, "desktop-export-review".into(), cx)?;
        window.click("desktop-export-review", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    let output = std::env::var_os("RETOLD_GUI_EVIDENCE")
        .map(PathBuf::from)
        .context("Native production evidence directory is missing")?;
    std::fs::create_dir_all(&output)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-export-authority".into(), cx)?;
        type_field(window, "Export allowance USD", "0", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        ensure!(
            window.try_find_by_label("Confirm video render").is_none(),
            "Changed allowance retained export confirmation"
        );
        type_field(window, "Export allowance USD", "1", cx);
        bring_control_into_view(window, "desktop-export-review".into(), cx)?;
        window.click("desktop-export-review", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-export-confirm".into(), cx)
    })??;
    screenshot(
        &mut cx,
        handle,
        &output,
        (1024., 700.),
        "native-ready-export-review",
    )?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-export-confirm", cx);
    })?;
    wait(&mut cx, &shell)?;
    let queue = retold_app::queue::QueueStore::open_read_only(
        &library.join(".retold/studio/queue.sqlite"),
    )?;
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        let page = queue.list_page(&retold_app::queue::ListOptions::default())?;
        ensure!(
            page.jobs.len() == expected_jobs,
            "Native export must admit exactly one additional job: {page:?}"
        );
        let job = page
            .jobs
            .iter()
            .max_by_key(|job| &job.created_at)
            .context("No export job")?;
        if job.status.is_terminal() {
            ensure!(
                job.status == retold_app::queue::JobStatus::Completed,
                "Native export failed for {reference}: {job:?}"
            );
            break;
        }
        ensure!(
            started.elapsed() < Duration::from_secs(120),
            "Native export did not settle: {job:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    cx.update_window(handle, |_, window, cx| {
        catalog_scroll(window, true, cx);
        window.click("desktop-refresh-activity", cx);
    })?;
    wait(&mut cx, &shell)?;
    screenshot(
        &mut cx,
        handle,
        &output,
        (1024., 700.),
        "native-export-completed",
    )?;
    println!("Native reviewed export completed for {reference}");
    native_media_journey(&mut cx, handle, &shell, library, reference, &output)?;
    Ok(())
}

fn native_media_journey(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    shell: &Entity<Shell>,
    library: &Path,
    reference: &str,
    output: &Path,
) -> anyhow::Result<()> {
    let page = retold_app::desktop::media::page(
        library,
        reference,
        &retold_app::desktop::media::Query::default(),
    )
    .map_err(edit_error)?;
    let video = page
        .items
        .iter()
        .position(|item| item.kind == retold_app::desktop::media::Kind::Video)
        .context("No exported video on the saved media page")?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        bring_control_into_view(window, "desktop-open-media".into(), cx)?;
        window.click("desktop-open-media", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, ("desktop-media-item", video).into(), cx)?;
        window.click(("desktop-media-item", video), cx);
        bring_control_into_view(window, "desktop-media-play".into(), cx)?;
        Ok::<_, anyhow::Error>(())
    })??;
    wait_player(cx, handle, "Media loaded.")?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-media-play".into(), cx)?;
        window.click("desktop-media-play", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait_player(cx, handle, "Playing saved media.")?;
    cx.advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, _| {
        ensure!(
            window.find("desktop-media-frame").bounds().bottom()
                <= window.find("desktop-media-play").bounds().origin.y,
            "Media frame overlaps its playback controls"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    screenshot(
        cx,
        handle,
        output,
        (1024., 700.),
        "native-saved-video-playback",
    )?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-media-play", cx)
    })?;
    wait_player(cx, handle, "Preview stopped. Its processes have exited.")?;
    cx.update_window(handle, |_, window, cx| {
        window.click("desktop-media-forward", cx)
    })?;
    wait_player(cx, handle, "Preview position updated.")?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-media-all".into(), cx)?;
        window.click("desktop-media-all", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait(cx, shell)?;
    wait_player(cx, handle, "Choose saved media to review.")?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, "desktop-media-close".into(), cx)?;
        window.click("desktop-media-close", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    Ok(())
}

fn wait_player(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    label: &str,
) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if cx.update_window(handle, |_, window, _| {
            window.try_find_by_label(label).is_some()
        })? {
            return Ok(());
        }
        let statuses = cx.update_window(handle, |_, window, _| {
            window
                .find_all_by_role(Role::Status)
                .iter()
                .filter_map(|element| element.label().map(str::to_owned))
                .collect::<Vec<_>>()
        })?;
        ensure!(
            started.elapsed() < Duration::from_secs(15),
            "Player did not reach {label}: {statuses:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
