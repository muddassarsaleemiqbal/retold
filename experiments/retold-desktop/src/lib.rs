//! The first native Retold Studio feasibility slice.

pub mod assets;
pub mod media;
pub mod model;
pub mod shell;
pub mod theme;
pub mod view;
pub mod work;
pub mod worker;

use gpui_kit::{
    AppContext, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds, WindowOptions,
    px, size,
};
use shell::commands::{
    CreateVideo, OpenWorkspace, RefreshWorkspace, SearchCommands, ToggleAppearance,
};

gpui_kit::actions!(retold_studio, [Quit]);

pub fn run() -> anyhow::Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--worker")
    {
        anyhow::ensure!(
            arguments.len() == 3 && arguments[1] == "--library",
            "Usage: retold-studio --worker --library PATH"
        );
        return worker::run(std::path::Path::new(&arguments[2]));
    }
    if arguments.first().is_some_and(|arg| arg == "--worker-probe") {
        anyhow::ensure!(
            arguments.len() == 2,
            "Usage: retold-studio --worker-probe EMPTY_DIRECTORY"
        );
        return worker_probe(std::path::Path::new(&arguments[1]));
    }
    anyhow::ensure!(
        arguments.is_empty(),
        "Launch without arguments; use Open workspace to select a workspace"
    );
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            theme::apply(false, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("ctrl-q", Quit, None),
            ]);
            shell::commands::bind_keys(cx);
            cx.set_menus(vec![
                Menu {
                    name: "Retold Studio".into(),
                    items: vec![
                        MenuItem::action("Search commands…", SearchCommands),
                        MenuItem::action("Open workspace…", OpenWorkspace),
                        MenuItem::action("New video", CreateVideo),
                        MenuItem::action("Refresh workspace", RefreshWorkspace),
                        MenuItem::action("Switch appearance", ToggleAppearance),
                        MenuItem::separator(),
                        MenuItem::action("Quit Retold Studio", Quit),
                    ],
                    disabled: false,
                },
                shell::commands::go_menu(),
            ]);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Retold Studio".into()),
                    ..TitlebarOptions::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1440.), px(900.)),
                    cx,
                ))),
                window_min_size: Some(size(px(760.), px(640.))),
                ..WindowOptions::default()
            };
            match gpui_kit::open_window(options, cx, |window, cx| {
                let shell = cx.new(|cx| shell::Shell::new(window, cx));
                shell.update(cx, |shell, cx| shell.restore_preferences(window, cx));
                shell
            }) {
                Ok((handle, shell)) => {
                    let shell = shell.downgrade();
                    cx.on_action(move |_: &Quit, cx| {
                        let result = handle.update(cx, |_, window, cx| {
                            let _ = shell.update(cx, |shell, cx| shell.request_quit(window, cx));
                        });
                        if result.is_err() {
                            cx.quit();
                        }
                    });
                }
                Err(error) => {
                    eprintln!("Could not open Retold Studio: {error}");
                    cx.quit();
                }
            }
            cx.activate(true);
        });
    Ok(())
}

/// Explicitly exercises shared StudioServer initialization without GPUI or executing queue work.
pub fn worker_probe(directory: &std::path::Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        directory.is_dir(),
        "Worker probe requires an existing empty directory"
    );
    anyhow::ensure!(
        std::fs::read_dir(directory)?.next().is_none(),
        "Worker probe refuses a nonempty directory"
    );
    let server = retold_app::studio::StudioServer::bind(directory)?;
    server.run_until(&std::sync::atomic::AtomicBool::new(true))?;
    println!(
        "{}",
        serde_json::json!({"worker_probe": "passed", "protocol": retold_app::protocol::PROTOCOL_VERSION, "dispatched": false})
    );
    Ok(())
}
