//! castorterm: a native terminal for agentic coding.

// Optimised Windows builds are GUI-subsystem apps: no extra console window
// when launched from Explorer. Debug builds keep the console for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_status;
mod attention;
mod board;
mod commands;
mod config;
mod config_edit;
mod files;
mod git_remote;
mod keys;
mod links;
mod mouse;
mod name_input;
mod palette;
mod provider;
mod pty_tap;
mod restore;
mod session;
mod shell;
mod shortcuts;
mod split;
mod tabs;
mod theme;
mod touched;
mod tree;
mod view;
mod workspace_view;
mod worktree;

use gpui::{
    App, AppContext as _, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point,
    px, size,
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
    // `castorterm --status <working|waiting|done>`, run by agent hooks:
    // report the agent's status to the castorterm window it runs in.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--status") {
        let result = match args.get(1) {
            // `--status touched`, run by Claude Code's PostToolUse hook: note
            // the file the agent used, from the hook's JSON on stdin. A
            // castorterm without it rejects the value (no window opens).
            Some(value) if value == "touched" => touched::record_from_hook(
                std::io::stdin().lock(),
                std::env::var_os(agent_status::ENV_VAR).as_deref(),
            ),
            Some(value) => agent_status::write_from_env(
                value,
                std::env::var_os(agent_status::ENV_VAR).as_deref(),
            ),
            None => Err("usage: castorterm --status <working|waiting|done>".into()),
        };
        if let Err(err) = result {
            eprintln!("castorterm: {err}");
        }
        // Never fail the agent's hook, never open a window from it.
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

/// Geometry of the last run, placed on a connected screen (a monitor may
/// have been unplugged since). Not used when restoring is turned off: the
/// file is not kept up to date then.
fn saved_window_bounds(settings: &config::Settings, cx: &App) -> Option<WindowBounds> {
    if !settings.restore_sessions {
        return None;
    }
    let path = settings
        .config_path
        .as_ref()?
        .parent()?
        .join(restore::FILE_NAME);
    let displays: Vec<(f32, f32, f32, f32)> = cx
        .displays()
        .iter()
        .map(|d| {
            let b = d.bounds();
            (
                b.origin.x.into(),
                b.origin.y.into(),
                b.size.width.into(),
                b.size.height.into(),
            )
        })
        .collect();
    let saved = restore::load(&path).window?.place(&displays)?;
    let bounds = Bounds::new(
        point(px(saved.x), px(saved.y)),
        size(px(saved.width), px(saved.height)),
    );
    Some(if saved.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    })
}

fn open_workspace_window(
    settings: config::Settings,
    launch_dir: std::path::PathBuf,
    cx: &mut App,
) -> anyhow::Result<()> {
    let window_bounds = saved_window_bounds(&settings, cx).unwrap_or_else(|| {
        WindowBounds::Windowed(Bounds::centered(None, size(px(1360.0), px(820.0)), cx))
    });
    let options = WindowOptions {
        window_bounds: Some(window_bounds),
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
