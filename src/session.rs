//! A terminal session: one PTY-backed shell plus its VT state.
//!
//! The PTY is read on a dedicated thread (`alacritty_terminal::event_loop`),
//! which parses the output into the shared `Term`. Events relevant to the UI
//! are forwarded through an unbounded channel as [`SessionEvent`]s.

use std::cell::Cell;
use std::collections::HashMap;
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

use crate::shell::default_shell_candidates;

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
            Event::Exit | Event::ChildExit(_) => SessionEvent::Exit,
            _ => return,
        };
        // The receiver is gone only when the window closed; nothing to do then.
        let _ = self.events.unbounded_send(event);
    }
}

/// Cheap-to-clone handle to a running session (UI thread only).
#[derive(Clone)]
pub struct Session {
    term: Arc<FairMutex<Term<Listener>>>,
    sender: Rc<EventLoopSender>,
    grid: Rc<Cell<GridSize>>,
}

impl Session {
    /// Starts the default shell in `cwd` and the PTY reader thread.
    pub fn spawn(
        grid: GridSize,
        cwd: Option<PathBuf>,
        events: UnboundedSender<SessionEvent>,
    ) -> anyhow::Result<Self> {
        tty::setup_env();
        let listener = Listener { events };
        let window_size = window_size(grid, 8, 16);

        let mut last_error = None;
        let mut pty = None;
        for shell in default_shell_candidates() {
            let options = tty::Options {
                shell: Some(tty::Shell::new(shell.program.clone(), shell.args.clone())),
                working_directory: cwd.clone(),
                drain_on_exit: false,
                env: HashMap::new(),
                ..Default::default()
            };
            match tty::new(&options, window_size, 0) {
                Ok(p) => {
                    pty = Some(p);
                    break;
                }
                Err(err) => last_error = Some(format!("{}: {err}", shell.program)),
            }
        }
        let pty = pty.ok_or_else(|| {
            anyhow!(
                "could not start a shell ({})",
                last_error.unwrap_or_else(|| "no candidates".into())
            )
        })?;

        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &grid,
            listener.clone(),
        )));
        let event_loop = EventLoop::new(term.clone(), listener, pty, false, false)
            .context("could not create the PTY event loop")?;
        let sender = event_loop.channel();
        event_loop.spawn();

        Ok(Self {
            term,
            sender: Rc::new(sender),
            grid: Rc::new(Cell::new(grid)),
        })
    }

    pub fn term(&self) -> &Arc<FairMutex<Term<Listener>>> {
        &self.term
    }

    /// Sends raw bytes to the shell.
    pub fn write(&self, bytes: Vec<u8>) {
        // Fails only if the event loop is gone, i.e. the shell already exited.
        let _ = self.sender.send(Msg::Input(bytes.into()));
    }

    /// Whether the terminal is in application cursor keys mode (DECCKM).
    pub fn app_cursor(&self) -> bool {
        self.term.lock().mode().contains(TermMode::APP_CURSOR)
    }

    /// Resizes the grid and the PTY if the size changed.
    pub fn resize(&self, grid: GridSize, cell_width: u16, cell_height: u16) {
        if self.grid.get() == grid {
            return;
        }
        self.grid.set(grid);
        self.term.lock().resize(grid);
        let _ = self
            .sender
            .send(Msg::Resize(window_size(grid, cell_width, cell_height)));
    }
}

fn window_size(grid: GridSize, cell_width: u16, cell_height: u16) -> WindowSize {
    WindowSize {
        num_lines: grid.rows.min(u16::MAX as usize) as u16,
        num_cols: grid.cols.min(u16::MAX as usize) as u16,
        cell_width,
        cell_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_size_dimensions() {
        let g = GridSize { cols: 80, rows: 24 };
        assert_eq!(g.columns(), 80);
        assert_eq!(g.screen_lines(), 24);
        assert_eq!(g.total_lines(), 24);
    }

    #[test]
    fn window_size_clamps_to_u16() {
        let ws = window_size(
            GridSize {
                cols: 100_000,
                rows: 30,
            },
            8,
            16,
        );
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
        listener.send_event(Event::Exit);
        let mut next = || {
            use futures::{FutureExt, StreamExt};
            rx.next().now_or_never().flatten()
        };
        assert!(matches!(next(), Some(SessionEvent::Wakeup)));
        assert!(matches!(next(), Some(SessionEvent::PtyWrite(s)) if s == "\x1b[0n"));
        assert!(matches!(next(), Some(SessionEvent::Exit)));
        assert!(next().is_none());
    }
}
