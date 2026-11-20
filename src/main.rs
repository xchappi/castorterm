//! castorterm: a native terminal for agentic coding.

mod keys;
mod palette;
mod session;
mod shell;
mod view;

use gpui::{
    App, AppContext as _, Application, Bounds, Focusable as _, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

use crate::session::{GridSize, Session};
use crate::view::TerminalView;

fn main() {
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        println!("castorterm {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    Application::new().run(|cx: &mut App| {
        if let Err(err) = open_terminal_window(cx) {
            eprintln!("castorterm: {err:#}");
            // Nothing is on screen yet: exit right away with a failure code.
            std::process::exit(1);
        }
    });
}

fn open_terminal_window(cx: &mut App) -> anyhow::Result<()> {
    let (events_tx, events_rx) = futures::channel::mpsc::unbounded();
    // Initial size is a guess; the first frame resizes to the real window.
    let session = Session::spawn(
        GridSize {
            cols: 100,
            rows: 30,
        },
        std::env::current_dir().ok(),
        events_tx,
    )?;

    let bounds = Bounds::centered(None, size(px(1000.0), px(650.0)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("castorterm".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let window = cx.open_window(options, |_window, cx| {
        cx.new(|cx| TerminalView::new(session, events_rx, cx))
    })?;

    window.update(cx, |view, window, cx| {
        window.focus(&view.focus_handle(cx));
        cx.activate(true);
    })?;
    Ok(())
}
