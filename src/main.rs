//! castorterm: a native terminal for agentic coding.

// Optimised Windows builds are GUI-subsystem apps: no extra console window
// when launched from Explorer. Debug builds keep the console for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod board;
mod config;
mod config_edit;
mod files;
mod git_remote;
mod keys;
mod name_input;
mod palette;
mod provider;
mod restore;
mod session;
mod shell;
mod shortcuts;
mod tabs;
mod theme;
mod tree;
mod view;
mod workspace_view;

use gpui::{
    App, AppContext as _, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px,
    size,
};

use crate::workspace_view::WorkspaceView;

fn main() {
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        println!("castorterm {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let settings = config::load(&config::Env::from_process());
    // A launch directory that cannot be read falls back to the home or root.
    let launch_dir = std::env::current_dir()
        .ok()
        .or_else(|| config::Env::from_process().home)
        .unwrap_or_else(|| std::path::PathBuf::from(std::path::MAIN_SEPARATOR_STR));

    let mut settings = settings;
    Application::new().run(move |cx: &mut App| {
        // Without the bundled fonts the chrome falls back to the system font.
        // One call per font: on Windows a failing font aborts the whole batch.
        for (family, bytes) in theme::BUNDLED_FONTS {
            if let Err(err) = cx
                .text_system()
                .add_fonts(vec![std::borrow::Cow::Borrowed(bytes)])
            {
                // Release builds have no console: report it in the sidebar.
                settings
                    .messages
                    .push(format!("Could not load the {family} font: {err:#}"));
            }
        }
        if let Err(err) = open_workspace_window(settings, launch_dir, cx) {
            eprintln!("castorterm: {err:#}");
            // Nothing is on screen yet: exit right away with a failure code.
            std::process::exit(1);
        }
    });
}

fn open_workspace_window(
    settings: config::Settings,
    launch_dir: std::path::PathBuf,
    cx: &mut App,
) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(1360.0), px(820.0)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("castorterm".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        cx.new(|cx| WorkspaceView::new(settings, launch_dir, window, cx))
    })?;
    cx.activate(true);
    Ok(())
}
