//! A terminal session: one PTY-backed shell plus its VT state.
//!
//! The PTY is read on a dedicated thread (`alacritty_terminal::event_loop`),
//! which parses the output into the shared `Term`. Events relevant to the UI
//! are forwarded through an unbounded channel as [`SessionEvent`]s.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::tty;
use anyhow::{Context as _, anyhow};
use futures::channel::mpsc::UnboundedSender;

use alacritty_terminal::vte::ansi::Rgb;

use crate::palette::DEFAULT_THEME;
use crate::shell::{default_shell_candidates, quote_windows_program, resolve_program};

/// Visible grid size in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// Events the UI cares about.
#[derive(Debug)]
pub enum SessionEvent {
    /// New content to draw.
    Wakeup,
    /// Text the terminal wants written back to the PTY (DSR/DA replies).
    PtyWrite(String),
    /// The shell exited.
    Exit,
}

/// Forwards terminal events to the UI thread.
#[derive(Clone)]
pub struct Listener {
    events: UnboundedSender<SessionEvent>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let event = match event {
            Event::Wakeup => SessionEvent::Wakeup,
            Event::PtyWrite(text) => SessionEvent::PtyWrite(text),
            // OSC 4/10/11 queries: answer with the theme color so TUIs that
            // probe colors (neovim, bat...) do not wait for a reply.
            Event::ColorRequest(index, format) => {
                let c = DEFAULT_THEME.by_index(index);
                SessionEvent::PtyWrite(format(Rgb {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                }))
            }
            Event::Exit | Event::ChildExit(_) => SessionEvent::Exit,
            _ => return,
        };
        // The receiver is gone only when the window closed; nothing to do then.
        let _ = self.events.unbounded_send(event);
    }
}

/// Grid size plus the cell metrics last sent to the PTY.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PtySize {
    grid: GridSize,
    cell_width: u16,
    cell_height: u16,
}

/// Owns the channel to the PTY event loop and stops the loop (closing the
/// PTY and ending the shell) when the last handle goes away.
struct LoopHandle(EventLoopSender);

impl LoopHandle {
    fn send(&self, msg: Msg) {
        // Fails only if the event loop already stopped.
        let _ = self.0.send(msg);
    }
}

impl Drop for LoopHandle {
    fn drop(&mut self) {
        self.send(Msg::Shutdown);
    }
}

/// Cheap-to-clone handle to a running session (UI thread only).
#[derive(Clone)]
pub struct Session {
    term: Arc<FairMutex<Term<Listener>>>,
    sender: Rc<LoopHandle>,
    size: Rc<Cell<PtySize>>,
}

impl Session {
    /// Starts the default shell in `cwd` and the PTY reader thread.
    /// Starts the shell in `cwd` with the extra environment variables `env`.
    pub fn spawn(
        grid: GridSize,
        cwd: Option<PathBuf>,
        env: Vec<(String, String)>,
        events: UnboundedSender<SessionEvent>,
    ) -> anyhow::Result<Self> {
        tty::setup_env();
        let size = PtySize {
            grid,
            cell_width: 8,
            cell_height: 16,
        };
        let pty = spawn_shell(window_size(size), cwd, env)?;

        let listener = Listener {
            events: events.clone(),
        };
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &grid,
            listener.clone(),
        )));
        let event_loop = EventLoop::new(term.clone(), listener, pty, false, false)
            .context("could not create the PTY event loop")?;
        // From here on, dropping `sender` (also on the error path below)
        // stops the event loop and the shell.
        let sender = LoopHandle(event_loop.channel());
        let reader = event_loop.spawn();

        // The event loop can stop on I/O errors without reporting a child
        // exit; make sure the UI always learns that the session is over.
        std::thread::Builder::new()
            .name("PTY watcher".into())
            .spawn(move || {
                let _ = reader.join();
                let _ = events.unbounded_send(SessionEvent::Exit);
            })
            .context("could not start the PTY watcher thread")?;

        Ok(Self {
            term,
            sender: Rc::new(sender),
            size: Rc::new(Cell::new(size)),
        })
    }

    pub fn term(&self) -> &Arc<FairMutex<Term<Listener>>> {
        &self.term
    }

    /// Sends raw bytes to the shell.
    pub fn write(&self, bytes: Vec<u8>) {
        self.sender.send(Msg::Input(bytes.into()));
    }

    /// Stops the PTY event loop, which closes the PTY and ends the shell
    /// (ConPTY closes the pseudo console; on Unix the child gets SIGHUP).
    pub fn shutdown(&self) {
        self.sender.send(Msg::Shutdown);
    }

    /// Whether the terminal is in application cursor keys mode (DECCKM).
    pub fn app_cursor(&self) -> bool {
        self.term.lock().mode().contains(TermMode::APP_CURSOR)
    }

    /// Resizes the grid and the PTY if the grid or the cell metrics changed
    /// (cell metrics change e.g. when moving to a monitor with another DPI).
    pub fn resize(&self, grid: GridSize, cell_width: u16, cell_height: u16) {
        let new = PtySize {
            grid,
            cell_width,
            cell_height,
        };
        let old = self.size.replace(new);
        if old == new {
            return;
        }
        if old.grid != grid {
            self.term.lock().resize(grid);
        }
        self.sender.send(Msg::Resize(window_size(new)));
    }
}

/// Starts the first shell candidate that resolves to an absolute path.
fn spawn_shell(
    window_size: WindowSize,
    cwd: Option<PathBuf>,
    env: Vec<(String, String)>,
) -> anyhow::Result<tty::Pty> {
    let path_var = std::env::var_os("PATH");
    let mut errors = Vec::new();
    for shell in default_shell_candidates() {
        // Never pass a bare name: CreateProcessW would also search the
        // working directory, which may be an untrusted repository.
        let Some(program) = resolve_program(&shell.program, path_var.as_deref()) else {
            errors.push(format!("{}: not found in PATH", shell.program));
            continue;
        };
        let program = program.to_string_lossy().into_owned();
        let program = if cfg!(windows) {
            quote_windows_program(&program)
        } else {
            program
        };

        // `escape_args` exists only on Windows, so the struct update is
        // needed there and redundant elsewhere.
        #[allow(clippy::needless_update)]
        let options = tty::Options {
            shell: Some(tty::Shell::new(program, shell.args.clone())),
            working_directory: cwd.clone(),
            drain_on_exit: false,
            env: env.iter().cloned().collect(),
            ..Default::default()
        };

        match tty::new(&options, window_size, 0) {
            Ok(pty) => return Ok(pty),
            Err(err) => errors.push(format!("{}: {err}", shell.program)),
        }
    }
    Err(anyhow!("could not start a shell ({})", errors.join("; ")))
}

fn window_size(size: PtySize) -> WindowSize {
    WindowSize {
        num_lines: size.grid.rows.min(u16::MAX as usize) as u16,
        num_cols: size.grid.cols.min(u16::MAX as usize) as u16,
        cell_width: size.cell_width,
        cell_height: size.cell_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::NamedColor;

    #[test]
    fn grid_size_dimensions() {
        let g = GridSize { cols: 80, rows: 24 };
        assert_eq!(g.columns(), 80);
        assert_eq!(g.screen_lines(), 24);
        assert_eq!(g.total_lines(), 24);
    }

    #[test]
    fn window_size_clamps_to_u16() {
        let ws = window_size(PtySize {
            grid: GridSize {
                cols: 100_000,
                rows: 30,
            },
            cell_width: 8,
            cell_height: 16,
        });
        assert_eq!(ws.num_cols, u16::MAX);
        assert_eq!(ws.num_lines, 30);
    }

    #[test]
    fn listener_forwards_relevant_events() {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let listener = Listener { events: tx };
        listener.send_event(Event::Wakeup);
        listener.send_event(Event::PtyWrite("\x1b[0n".into()));
        listener.send_event(Event::Bell);
        listener.send_event(Event::ColorRequest(
            NamedColor::Background as usize,
            Arc::new(|c: Rgb| format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b)),
        ));
        listener.send_event(Event::Exit);
        let mut next = || {
            use futures::{FutureExt, StreamExt};
            rx.next().now_or_never().flatten()
        };
        assert!(matches!(next(), Some(SessionEvent::Wakeup)));
        assert!(matches!(next(), Some(SessionEvent::PtyWrite(s)) if s == "\x1b[0n"));
        assert!(matches!(next(), Some(SessionEvent::PtyWrite(s)) if s == "0d0b1e"));
        assert!(matches!(next(), Some(SessionEvent::Exit)));
        assert!(next().is_none());
    }
}
