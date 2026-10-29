//! Production journey over the review workspaces added after scene direction: narration with its
//! waveform and player, edition lines, materials and the series destination.
use super::*;
use gpui_kit::ElementId;

/// Scroll the page down until a control below the fold is painted, then center it.
fn reveal(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    id: &'static str,
) -> anyhow::Result<()> {
    for _ in 0..12 {
        if present(cx, handle, id.into())? {
            cx.update_window(handle, |_, window, cx| {
                bring_control_into_view(window, id.into(), cx)
            })??;
            cx.run_until_parked();
            return Ok(());
        }
        cx.update_window(handle, |_, window, cx| {
            window.scroll(
                "desktop-content",
                ScrollDelta::Pixels(point(px(0.), px(-240.))),
                cx,
            );
        })?;
        cx.run_until_parked();
    }
    anyhow::bail!("{id} never became visible")
}

fn present(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    id: ElementId,
) -> anyhow::Result<bool> {
    cx.update_window(handle, |_, window, _| window.try_find(id).is_some())
}

fn wait_for(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    id: ElementId,
    what: &str,
) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if present(cx, handle, id.clone())? {
            return Ok(());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(15),
            "{what} did not appear"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn press(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    id: ElementId,
) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, id.clone(), cx)?;
        window.click(id, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    cx.run_until_parked();
    Ok(())
}

fn narration(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
) -> anyhow::Result<()> {
    press(cx, handle, "desktop-open-narration".into())?;
    wait_for(
        cx,
        handle,
        ("desktop-narration-line", 0_usize).into(),
        "Recorded edition",
    )?;
    wait_player(cx, handle, "Media loaded.")?;
    reveal(cx, handle, "desktop-retake-review")?;
    cx.update_window(handle, |_, window, cx| {
        bring_control_into_view(window, ("desktop-narration-line", 0_usize).into(), cx)?;
        window.click(("desktop-narration-line", 0_usize), cx);
        Ok::<_, anyhow::Error>(())
    })??;
    wait_player(cx, handle, "Playing saved media.")?;
    screenshot(cx, handle, output, (1024., 700.), "native-narration-dark")?;
    press(cx, handle, "desktop-narration-back".into())
}

fn lines(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
) -> anyhow::Result<()> {
    press(cx, handle, "desktop-open-lines".into())?;
    let started = Instant::now();
    loop {
        cx.run_until_parked();
        if present(cx, handle, ("desktop-lines-unit", 0_usize).into())? {
            reveal(cx, handle, "desktop-lines-approve")?;
            break;
        }
        if present(cx, handle, "desktop-lines-open-script".into())? {
            break;
        }
        ensure!(
            started.elapsed() < Duration::from_secs(15),
            "Edition lines did not load"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    screenshot(cx, handle, output, (1024., 700.), "native-lines-dark")?;
    press(cx, handle, "desktop-lines-back".into())
}

fn materials(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
) -> anyhow::Result<()> {
    press(cx, handle, "desktop-open-materials".into())?;
    wait_for(
        cx,
        handle,
        ("desktop-material", 0_usize).into(),
        "Saved material",
    )?;
    press(cx, handle, ("desktop-material", 0_usize).into())?;
    wait_for(
        cx,
        handle,
        "desktop-material-verify".into(),
        "Material detail",
    )?;
    screenshot(cx, handle, output, (1024., 700.), "native-materials-dark")?;
    press(cx, handle, "desktop-materials-back".into())
}

/// A video's More actions: open "Video costs" with this video filled in, run it, capture.
fn video_form(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    output: &Path,
) -> anyhow::Result<()> {
    reveal(cx, handle, "desktop-more-actions")?;
    press(cx, handle, "desktop-more-actions".into())?;
    let mut opened = false;
    for position in 0..64_usize {
        let id: ElementId = ("desktop-action", position).into();
        let matches = cx.update_window(handle, |_, window, _| {
            window
                .try_find(id.clone())
                .is_some_and(|button| button.label() == Some("Video costs"))
        })?;
        if matches {
            press(cx, handle, id)?;
            opened = true;
            break;
        }
        if !present(cx, handle, id)? {
            cx.update_window(handle, |_, window, cx| {
                window.scroll(
                    "desktop-content",
                    ScrollDelta::Pixels(point(px(0.), px(-200.))),
                    cx,
                );
            })?;
            cx.run_until_parked();
        }
    }
    ensure!(opened, "Video costs was not offered for this video");
    reveal(cx, handle, "desktop-form-run")?;
    press(cx, handle, "desktop-form-run".into())?;
    wait_for(cx, handle, "desktop-form-close".into(), "Form")?;
    std::thread::sleep(Duration::from_millis(300));
    cx.run_until_parked();
    screenshot(cx, handle, output, (1024., 700.), "native-video-form-dark")?;
    press(cx, handle, "desktop-form-close".into())
}

pub(super) fn journey(library: &Path, output: &Path) -> anyhow::Result<()> {
    let (mut cx, handle, shell) = window((1024., 700.))?;
    cx.update(|cx| shell.update(cx, |shell, cx| shell.open_workspace(library.into(), cx)));
    wait(&mut cx, &shell)?;
    cx.update_window(handle, |_, window, cx| {
        navigate(window, retold_desktop::shell::Destination::Videos, cx);
        window.click(("desktop-video", 0_usize), cx);
    })?;
    cx.run_until_parked();
    narration(&mut cx, handle, output)?;
    lines(&mut cx, handle, output)?;
    materials(&mut cx, handle, output)?;
    video_form(&mut cx, handle, output)?;
    cx.update_window(handle, |_, window, cx| window.press("cmd-3", cx))?;
    wait_for(&mut cx, handle, "desktop-series-new".into(), "Series page")?;
    screenshot(&mut cx, handle, output, (1024., 700.), "native-series-dark")
}
