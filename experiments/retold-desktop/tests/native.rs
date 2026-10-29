//! Main-thread Metal rendering and real component events; no browser/HTML surrogate.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, ensure};
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, HeadlessAppContext, MouseButton, Role,
    WindowBounds, WindowOptions, point, px, size,
};
use retold_desktop::{assets::Assets, theme, view::Studio};

fn main() -> anyhow::Result<()> {
    let output = std::env::var_os("RETOLD_GUI_EVIDENCE").map_or_else(
        || PathBuf::from("/tmp/retold-gpui-g0-evidence"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&output)?;
    for dimensions in [(1440., 900.), (1280., 800.), (1024., 700.)] {
        capture(&output, dimensions, false)?;
        capture(&output, dimensions, true)?;
    }
    interaction(&output)?;
    println!(
        "Native Metal renders and interaction checks passed. Captures: {}",
        output.display()
    );
    Ok(())
}

fn context() -> HeadlessAppContext {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx
}

fn window(
    cx: &mut HeadlessAppContext,
    dimensions: (f32, f32),
) -> anyhow::Result<(AnyWindowHandle, Entity<Studio>)> {
    let handle = cx.update(|cx| {
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
            |window, cx| cx.new(|cx| Studio::new(window, cx)),
        )
    })?;
    cx.run_until_parked();
    cx.update_window(handle.0, |_, window, cx| window.render_frame(cx))?;
    cx.run_until_parked();
    Ok(handle)
}

fn capture(output: &Path, dimensions: (f32, f32), light: bool) -> anyhow::Result<()> {
    let mut cx = context();
    let (handle, _) = window(&mut cx, dimensions)?;
    if light {
        cx.update_window(handle, |_, window, cx| window.click("appearance", cx))?;
    }
    let image = cx.capture_screenshot(handle)?;
    ensure!(
        image.width() > 0 && image.height() > 0,
        "Renderer returned an empty image"
    );
    image.save(output.join(format!(
        "scene-{}x{}-{}.png",
        dimensions.0,
        dimensions.1,
        if light { "light" } else { "dark" }
    )))?;
    cx.update_window(handle, |_, window, _| {
        let field = window.find_by_label("Narration");
        ensure!(
            field.role() == Some(Role::MultilineTextInput),
            "Narration lacks its native input role"
        );
        ensure!(field.visible(), "Narration is clipped at {dimensions:?}");
        ensure!(
            field.bounds().right() <= px(dimensions.0),
            "Narration overflows the window"
        );
        let scene = window.find(("scene", 2_usize));
        ensure!(
            scene.visible() && scene.bounds().bottom() <= px(dimensions.1 - 40.),
            "Scene selection is clipped by the status bar at {dimensions:?}"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    Ok(())
}

fn interaction(output: &Path) -> anyhow::Result<()> {
    let mut cx = context();
    let (handle, studio) = window(&mut cx, (1440., 900.))?;
    cx.update_window(handle, |_, window, cx| {
        let field = window.find_by_label("Narration");
        let position = field.bounds().center();
        window.pointer_move(position, None, cx);
        window.pointer_down(position, MouseButton::Left, cx);
        window.pointer_up(position, MouseButton::Left, cx);
        window.press("cmd-a", cx);
        window.input("English draft — हिन्दी १२३ — اردو 123", cx);
        ensure!(
            window.find_by_label("Narration").value()
                == Some("English draft — हिन्दी १२३ — اردو 123"),
            "Mixed-script input was not retained"
        );
        window.click(("edition", 1_usize), cx);
        ensure!(
            window
                .find_by_label("Narration")
                .value()
                .is_some_and(|v| v.starts_with("शाम")),
            "Hindi draft did not load"
        );
        let position = window.find_by_label("Narration").bounds().center();
        window.pointer_move(position, None, cx);
        window.pointer_down(position, MouseButton::Left, cx);
        window.pointer_up(position, MouseButton::Left, cx);
        window.press("cmd-a", cx);
        window.input("हिन्दी १२३", cx);
        window.press("backspace", cx);
        ensure!(
            window.find_by_label("Narration").value() == Some("हिन्दी १२"),
            "Hindi deletion corrupted text"
        );
        window.press("cmd-z", cx);
        ensure!(
            window.find_by_label("Narration").value() == Some("हिन्दी १२३"),
            "Hindi undo did not restore text"
        );
        window.click(("edition", 0_usize), cx);
        ensure!(
            window.find_by_label("Narration").value()
                == Some("English draft — हिन्दी १२३ — اردو 123"),
            "Switching editions lost the draft"
        );
        window.click("approve", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    ensure!(
        cx.update(|cx| studio.read(cx).session.draft().approved),
        "Approval did not update session state"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click(("scene", 0_usize), cx);
        window.click(("scene", 1_usize), cx);
        ensure!(
            window.find_by_label("Narration").value()
                == Some("English draft — हिन्दी १२३ — اردو 123"),
            "Scene selection lost the draft"
        );
        window.click(("edition", 2_usize), cx);
        ensure!(
            window
                .find_by_label("Narration")
                .value()
                .is_some_and(|v| v.starts_with("شام")),
            "Urdu draft did not load"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    ensure!(
        !cx.update(|cx| studio.read(cx).session.draft().approved),
        "Approval leaked across editions"
    );
    cx.capture_screenshot(handle)?
        .save(output.join("scene-urdu-dark.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.click(("edition", 1_usize), cx)
    })?;
    cx.capture_screenshot(handle)?
        .save(output.join("scene-hindi-dark.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.click("appearance", cx);
        window.render_frame(cx);
    })
    .context("Appearance interaction failed")?;
    Ok(())
}
