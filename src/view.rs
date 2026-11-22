//! GPUI view that renders a terminal session and forwards input to it.

use std::cell::Cell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use alacritty_terminal::grid::{Dimensions as _, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::vte::ansi::CursorShape;
use futures::channel::mpsc::UnboundedReceiver;
use futures::{FutureExt as _, StreamExt as _};
use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, Rgba, ShapedLine, SharedString, Task, TextRun,
    UTF16Selection, Window, canvas, div, fill, font, point, prelude::*, px, size,
};

use gpui::{ClipboardEntry, Image, ImageSource, ObjectFit, img};

use crate::keys::{KeyMode, Mods, encode_key};
use crate::palette::{DEFAULT_THEME, Rgb8};
use crate::session::{GridSize, Session, SessionEvent};

#[cfg(target_os = "windows")]
const FONT_FAMILY: &str = "Consolas";
#[cfg(target_os = "macos")]
const FONT_FAMILY: &str = "Menlo";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const FONT_FAMILY: &str = "DejaVu Sans Mono";

const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 1.3;
/// Pixels of wheel movement per scrolled line.
const WHEEL_LINE_PX: f32 = 18.0;
/// Mouse rest on a path before its preview loads.
const HOVER_PREVIEW_DELAY: std::time::Duration = std::time::Duration::from_millis(450);
/// How long a pasted image stays shown.
const PASTED_PREVIEW_TIME: std::time::Duration = std::time::Duration::from_secs(8);
/// Biggest file read for a text preview, and biggest image shown.
const PREVIEW_TEXT_BYTES: u64 = 64 * 1024;
const PREVIEW_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Lines shown in a text preview.
const PREVIEW_LINES: usize = 14;
const PREVIEW_WIDTH: f32 = 420.0;
const PREVIEW_HEIGHT: f32 = 260.0;
/// Underline of a link under Ctrl.
const LINK_UNDERLINE: Rgb8 = Rgb8 {
    r: 0x4d,
    g: 0xd2,
    b: 0xff,
};

/// The floating preview.
struct Preview {
    title: String,
    content: PreviewContent,
}

enum PreviewContent {
    Image(PathBuf),
    Pasted(Arc<Image>),
    Text {
        lines: Vec<(usize, String)>,
        /// Line the link points at, highlighted.
        line: Option<u32>,
    },
}

/// Reads what the preview of `path` shows (on a background thread): images
/// by path, text as an excerpt around `line`. `None` for folders, missing,
/// huge or binary files.
fn load_preview(path: &std::path::Path, line: Option<u32>) -> Option<Preview> {
    use std::io::Read as _;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let title = path.file_name()?.to_string_lossy().into_owned();
    let content = match crate::links::kind(path) {
        crate::links::Kind::Image => {
            if meta.len() > PREVIEW_IMAGE_BYTES {
                return None;
            }
            PreviewContent::Image(path.to_path_buf())
        }
        crate::links::Kind::Text => {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .ok()?
                .take(PREVIEW_TEXT_BYTES)
                .read_to_end(&mut bytes)
                .ok()?;
            let text = String::from_utf8_lossy(&bytes);
            let lines = crate::links::text_excerpt(&text, line, PREVIEW_LINES)?;
            if lines.is_empty() {
                return None;
            }
            PreviewContent::Text { lines, line }
        }
    };
    Some(Preview { title, content })
}

/// Starts `editor -g target` without a console; false if it could not
/// start (or the target cannot pass through a `.cmd` shim safely).
fn open_in_editor(editor: &std::path::Path, target: &str) -> bool {
    let shim = editor
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if shim && !crate::links::safe_for_cmd(target) {
        return false;
    }
    let mut command = std::process::Command::new(editor);
    command
        .args(["-g", target])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    match command.spawn() {
        Ok(mut child) => {
            // Reap it without blocking the UI.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// Most wheel events reported to a program for one wheel event.
const MAX_WHEEL_EVENTS: u32 = 10;
/// Background of selected text.
const SELECTION: Rgb8 = Rgb8 {
    r: 0x3a,
    g: 0x4f,
    b: 0x7a,
};
/// Most lines scrolled by one wheel event.
const MAX_WHEEL_LINES: f32 = 1000.0;
/// Background of the search match.
const SEARCH_MATCH: Rgb8 = Rgb8 {
    r: 0xff,
    g: 0xc8,
    b: 0x3d,
};
const CURSOR_ALPHA: f32 = 0.7;
/// One dark line every `SCANLINE_PERIOD` pixels, faint enough to keep text readable.
const SCANLINE_PERIOD: f32 = 3.0;
const SCANLINE_ALPHA: f32 = 0.10;

/// Font used to draw a terminal.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalStyle {
    pub family: SharedString,
    pub size: f32,
    /// Draw CRT-style scanlines over the text.
    pub scanlines: bool,
}

impl TerminalStyle {
    /// Platform defaults for anything the config does not set.
    pub fn new(family: Option<&str>, size: Option<f32>, scanlines: bool) -> Self {
        Self {
            family: family.unwrap_or(FONT_FAMILY).to_string().into(),
            size: size.unwrap_or(FONT_SIZE),
            scanlines,
        }
    }
}

/// Shared switch that stops every terminal from taking keyboard input, for
/// as long as another input (a name field) owns the keyboard.
#[derive(Clone, Debug, Default)]
pub struct InputGate(Rc<Cell<bool>>);

impl InputGate {
    pub fn set_paused(&self, paused: bool) {
        self.0.set(paused);
    }

    pub fn paused(&self) -> bool {
        self.0.get()
    }
}

/// Events a terminal reports to its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    /// The shell exited; the session is over.
    Exited,
    /// The shell's working directory changed.
    Cwd(String),
}

pub struct TerminalView {
    focus_handle: FocusHandle,
    session: Session,
    style: TerminalStyle,
    gate: InputGate,
    /// Scrollback search in progress, if any.
    search: Option<Search>,
    /// Wheel movement not yet turned into whole lines.
    wheel_lines: f32,
    /// Grid geometry of the last frame, to map the mouse to cells.
    metrics: Rc<Cell<Option<GridMetrics>>>,
    /// Button whose press was reported to the program (its release is too).
    reported: Option<crate::mouse::Button>,
    /// Cell of the last reported motion (motion is sent once per cell).
    last_motion: Option<(usize, usize)>,
    /// A selection is being dragged with the left button.
    selecting: bool,
    /// The session runs an agent: its menus may be clicked.
    agent: bool,
    /// Menu option pressed; chosen on release in the same cell.
    pending_menu: Option<(usize, usize)>,
    /// Directory relative paths in the output are resolved against: the
    /// project, then wherever the shell reports it is.
    base_dir: Option<PathBuf>,
    /// Editor for Ctrl+click on paths (absolute path), if found.
    editor: Option<PathBuf>,
    /// Link under the mouse: `(row, start, end, target)`.
    hover: Option<(usize, usize, usize, crate::links::Target)>,
    /// Ctrl held during the last mouse move (links are underlined).
    ctrl: bool,
    /// Floating preview over the terminal.
    preview: Option<Preview>,
    /// Pending preview load or auto-hide (dropping it cancels it).
    preview_task: Option<Task<()>>,
    _events: Task<()>,
}

/// Where the grid was painted.
#[derive(Clone, Copy, Debug)]
struct GridMetrics {
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    cols: usize,
    rows: usize,
}

impl GridMetrics {
    /// Cell under `position` (clamped to the grid) and which half of it.
    fn cell(&self, position: Point<Pixels>) -> (usize, usize, Side) {
        let x = f32::from(position.x - self.origin.x) / f32::from(self.cell_width);
        let y = f32::from(position.y - self.origin.y) / f32::from(self.line_height);
        let col = (x.max(0.0) as usize).min(self.cols.saturating_sub(1));
        let row = (y.max(0.0) as usize).min(self.rows.saturating_sub(1));
        let side = if x.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (col, row, side)
    }
}

/// A scrollback search: the compiled query and the match shown.
struct Search {
    query: String,
    regex: RegexSearch,
    current: Option<Match>,
    /// Text of `current` and the history size when it was found, to follow
    /// it as new output pushes lines into the history.
    text: String,
    history: usize,
}

impl Search {
    /// Moves `current` to where its text is now, or drops it.
    fn relocate<T>(&mut self, term: &alacritty_terminal::Term<T>) {
        if let Some(m) = &self.current {
            self.current = relocate(term, m, &self.text, self.history);
            self.history = term.history_size();
        }
    }
}

/// Outcome of [`TerminalView::find`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindResult {
    Found,
    NotFound,
    /// The query could not be searched (too long or empty).
    Invalid,
    /// A full-screen program (vim, less) is showing: it has no history.
    FullScreen,
}

/// Where match `m`, found when the history had `history_then` lines, is
/// now: lines pushed into the history since move it up. `None` when its
/// text `text` is no longer there (cleared, reflowed, rotated out).
pub fn relocate<T>(
    term: &alacritty_terminal::Term<T>,
    m: &Match,
    text: &str,
    history_then: usize,
) -> Option<Match> {
    let grown = term.history_size().saturating_sub(history_then) as i32;
    let shift = |p: &GridPoint| GridPoint::new(Line(p.line.0 - grown), p.column);
    let (start, end) = (shift(m.start()), shift(m.end()));
    let inside = |p: GridPoint| {
        p.line >= term.topmost_line()
            && p.line <= term.bottommost_line()
            && p.column <= term.last_column()
    };
    if !inside(start) || !inside(end) || start > end {
        return None;
    }
    (term.bounds_to_string(start, end) == text).then_some(start..=end)
}

/// Whole lines to scroll for a wheel movement of `pixels` plus what was
/// left over (`carry`), and the new leftover. Bounded: Windows' "one screen
/// at a time" setting reports billions of lines per notch.
pub fn wheel_lines(carry: f32, pixels: f32) -> (i32, f32) {
    let lines = carry + pixels / WHEEL_LINE_PX;
    let whole = lines.trunc();
    let rest = if whole.abs() > MAX_WHEEL_LINES {
        0.0
    } else {
        lines - whole
    };
    (whole.clamp(-MAX_WHEEL_LINES, MAX_WHEEL_LINES) as i32, rest)
}

/// The match after `current` (older when `backwards`), or the nearest one
/// to the bottom (top) when there is none yet. Wraps around the history.
pub fn next_match<T>(
    term: &alacritty_terminal::Term<T>,
    regex: &mut RegexSearch,
    current: Option<&Match>,
    backwards: bool,
) -> Option<Match> {
    let (direction, side) = if backwards {
        (Direction::Left, Side::Left)
    } else {
        (Direction::Right, Side::Right)
    };
    let origin = match (current, backwards) {
        (Some(m), true) => m.start().sub(term, Boundary::None, 1),
        (Some(m), false) => m.end().add(term, Boundary::None, 1),
        (None, true) => GridPoint::new(term.bottommost_line(), term.last_column()),
        (None, false) => GridPoint::new(term.topmost_line(), Column(0)),
    };
    term.search_next(regex, origin, direction, side, None)
}

/// What the program asked for, from the terminal modes.
fn reporting(mode: TermMode) -> crate::mouse::Reporting {
    crate::mouse::Reporting {
        clicks: mode.contains(TermMode::MOUSE_REPORT_CLICK),
        drag: mode.contains(TermMode::MOUSE_DRAG),
        motion: mode.contains(TermMode::MOUSE_MOTION),
        sgr: mode.contains(TermMode::SGR_MOUSE),
    }
}

fn mouse_button(button: MouseButton) -> Option<crate::mouse::Button> {
    match button {
        MouseButton::Left => Some(crate::mouse::Button::Left),
        MouseButton::Middle => Some(crate::mouse::Button::Middle),
        MouseButton::Right => Some(crate::mouse::Button::Right),
        _ => None,
    }
}

fn mouse_mods(m: gpui::Modifiers) -> crate::mouse::MouseMods {
    crate::mouse::MouseMods {
        shift: m.shift,
        alt: m.alt,
        ctrl: m.control,
    }
}

/// Grid point of visible cell `col`, `row` when scrolled `display_offset`
/// lines into the history.
fn grid_point(display_offset: usize, col: usize, row: usize) -> GridPoint {
    GridPoint::new(Line(row as i32 - display_offset as i32), Column(col))
}

/// Bytes for pasting `text`: line breaks as Enter (`\r`), and wrapped in
/// bracketed-paste markers when the program asked for them (any marker in
/// the text itself is removed so it cannot end the paste early).
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    // No escape or control characters from the clipboard: an ESC could
    // end a bracketed paste early (markers can be split to survive a simple
    // replace) or drive the program. Line breaks become Enter, tabs stay.
    let text: String = text
        .replace("\r\n", "\r")
        .replace('\n', "\r")
        .chars()
        .filter(|&c| c == '\r' || c == '\t' || !c.is_control())
        .collect();
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.into_bytes()
    }
}

/// Longest query searched.
pub const MAX_QUERY_CHARS: usize = 200;

/// Regex for a literal query: metacharacters escaped; case-insensitive
/// unless the query has an uppercase letter (smart case).
pub fn literal_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 4);
    if !query.chars().any(char::is_uppercase) {
        out.push_str("(?i)");
    }
    for c in query.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl TerminalView {
    pub fn new(
        session: Session,
        events: UnboundedReceiver<SessionEvent>,
        style: TerminalStyle,
        gate: InputGate,
        cx: &mut Context<Self>,
    ) -> Self {
        let events_task = cx.spawn(async move |this, cx| {
            let mut events = events;
            while let Some(first) = events.next().await {
                // Coalesce bursts of events into a single redraw.
                let mut batch = vec![first];
                while let Some(Some(event)) = events.next().now_or_never() {
                    batch.push(event);
                }
                let exited = this.update(cx, |view, cx| {
                    let mut exited = false;
                    let mut cwd = None;
                    for event in batch {
                        match event {
                            SessionEvent::Wakeup => {}
                            SessionEvent::PtyWrite(text) => view.session.write(text.into_bytes()),
                            SessionEvent::Exit => exited = true,
                            // Only the last one of a burst matters.
                            SessionEvent::Cwd(dir) => {
                                view.base_dir = Some(PathBuf::from(&dir));
                                cwd = Some(dir);
                            }
                        }
                    }
                    if let Some(dir) = cwd {
                        cx.emit(TerminalEvent::Cwd(dir));
                    }
                    if exited {
                        cx.emit(TerminalEvent::Exited);
                    }
                    cx.notify();
                    exited
                });
                // Stop once the view is gone or the session is over.
                if exited.unwrap_or(true) {
                    break;
                }
            }
        });

        Self {
            focus_handle: cx.focus_handle(),
            session,
            style,
            gate,
            search: None,
            wheel_lines: 0.0,
            metrics: Rc::new(Cell::new(None)),
            reported: None,
            last_motion: None,
            selecting: false,
            agent: false,
            pending_menu: None,
            base_dir: None,
            editor: None,
            hover: None,
            ctrl: false,
            preview: None,
            preview_task: None,
            _events: events_task,
        }
    }

    /// Finds `query` in the scrollback and scrolls to it: older matches
    /// when `backwards`, newer otherwise, starting after the match shown (or
    /// from the bottom when the query changed).
    pub fn find(&mut self, query: &str, backwards: bool, cx: &mut Context<Self>) -> FindResult {
        if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
            self.clear_search(cx);
            return FindResult::Invalid;
        }
        if self.alt_screen() {
            self.clear_search(cx);
            return FindResult::FullScreen;
        }
        if self.search.as_ref().is_none_or(|s| s.query != query) {
            let Ok(regex) = RegexSearch::new(&literal_pattern(query)) else {
                self.clear_search(cx);
                return FindResult::Invalid;
            };
            self.search = Some(Search {
                query: query.to_string(),
                regex,
                current: None,
                text: String::new(),
                history: 0,
            });
        }
        let search = self.search.as_mut().expect("set above");
        let mut term = self.session.term().lock();
        search.relocate(&term);
        let found = next_match(&term, &mut search.regex, search.current.as_ref(), backwards);
        if let Some(m) = &found {
            term.scroll_to_point(*m.start());
            search.text = term.bounds_to_string(*m.start(), *m.end());
            search.history = term.history_size();
        }
        drop(term);
        let result = if found.is_some() {
            FindResult::Found
        } else {
            FindResult::NotFound
        };
        search.current = found;
        cx.notify();
        result
    }

    /// Ends the search: no highlight, back to the bottom.
    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        if self.search.take().is_some() {
            self.session.term().lock().scroll_display(Scroll::Bottom);
            cx.notify();
        }
    }

    /// Whether the shell shows a full-screen program (vim, less...), which
    /// handles scrolling itself.
    fn alt_screen(&self) -> bool {
        self.session
            .term()
            .lock()
            .mode()
            .contains(TermMode::ALT_SCREEN)
    }

    /// Scrolls the history by whole lines (positive: older).
    fn scroll_lines(&mut self, lines: i32, cx: &mut Context<Self>) {
        if lines != 0 && !self.alt_screen() {
            self.session
                .term()
                .lock()
                .scroll_display(Scroll::Delta(lines));
            cx.notify();
        }
    }

    fn on_scroll_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Ctrl + wheel is zoom, handled by the window.
        if event.modifiers.control {
            return;
        }
        if matches!(event.touch_phase, gpui::TouchPhase::Started) {
            self.wheel_lines = 0.0;
        }
        let pixels = f32::from(event.delta.pixel_delta(px(WHEEL_LINE_PX)).y);
        let (whole, rest) = wheel_lines(self.wheel_lines, pixels);
        self.wheel_lines = rest;
        if whole == 0 || self.gate.paused() {
            return;
        }
        let (reporting, mode) = {
            let term = self.session.term().lock();
            (reporting(*term.mode()), *term.mode())
        };
        // Programs that read the mouse get wheel events (Shift scrolls here).
        let steps = whole.unsigned_abs().min(MAX_WHEEL_EVENTS);
        if reporting.active() && !event.modifiers.shift {
            if let Some(metrics) = self.metrics.get() {
                let (col, row, _) = metrics.cell(event.position);
                let button = if whole > 0 {
                    crate::mouse::Button::WheelUp
                } else {
                    crate::mouse::Button::WheelDown
                };
                let mods = mouse_mods(event.modifiers);
                if let Some(bytes) = crate::mouse::encode(
                    button,
                    crate::mouse::Kind::Press,
                    col,
                    row,
                    mods,
                    reporting.sgr,
                ) {
                    self.session.write(bytes.repeat(steps as usize));
                }
            }
            return;
        }
        // Full-screen programs without mouse support (less, man) scroll
        // with the arrow keys, like in other terminals.
        if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            let arrow: &[u8] = match (whole > 0, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            self.session.write(arrow.repeat(steps as usize));
            return;
        }
        self.scroll_lines(whole, cx);
    }

    /// Mouse press: reported to programs that read the mouse; otherwise the
    /// left button selects (double click: word, triple: line) and the right
    /// one copies the selection or pastes.
    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gate.paused() {
            return;
        }
        // The click that brings a pane forward (split view) is only that.
        let focused = self.focus_handle.is_focused(window);
        let Some(metrics) = self.metrics.get() else {
            return;
        };
        let Some(button) = mouse_button(event.button) else {
            return;
        };
        let (col, row, side) = metrics.cell(event.position);
        // Ctrl+click opens a link, in any program.
        if button == crate::mouse::Button::Left
            && event.modifiers.control
            && focused
            && let Some(link) = crate::links::link_at(&self.row_text(row), col)
        {
            self.open_link(&link.target, cx);
            return;
        }
        let reporting = reporting(*self.session.term().lock().mode());
        if reporting.active() && !event.modifiers.shift {
            if !focused {
                return;
            }
            let mods = mouse_mods(event.modifiers);
            if let Some(bytes) = crate::mouse::encode(
                button,
                crate::mouse::Kind::Press,
                col,
                row,
                mods,
                reporting.sgr,
            ) {
                self.session.write(bytes);
                self.reported = Some(button);
                self.last_motion = Some((col, row));
            }
            return;
        }
        // Menus of programs that do not read the mouse (Claude Code's
        // prompts): a click moves to the option, a click on the chosen one
        // confirms it.
        if button == crate::mouse::Button::Left
            && event.click_count == 1
            && focused
            && self.menu_action(row).is_some()
        {
            self.pending_menu = Some((col, row));
            return;
        }
        match button {
            crate::mouse::Button::Left => {
                let ty = match event.click_count {
                    0 | 1 => SelectionType::Simple,
                    2 => SelectionType::Semantic,
                    _ => SelectionType::Lines,
                };
                let mut term = self.session.term().lock();
                let point = grid_point(term.grid().display_offset(), col, row);
                term.selection = Some(Selection::new(ty, point, side));
                self.selecting = true;
                cx.notify();
            }
            crate::mouse::Button::Right if !self.copy_selection(cx) => self.paste(cx),
            _ => {}
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(metrics) = self.metrics.get() else {
            return;
        };
        let (col, row, side) = metrics.cell(event.position);
        // A press reported to the program whose release happened outside the
        // view: report it now, so the program does not think it is held.
        if event.pressed_button.is_none()
            && let Some(button) = self.reported.take()
        {
            self.report_release(button, col, row, event.modifiers);
        }
        if event.pressed_button.is_none() && !self.selecting {
            self.hover_link(col, row, event.modifiers.control, cx);
        }
        if self.selecting {
            if event.pressed_button != Some(MouseButton::Left) || self.gate.paused() {
                self.selecting = false;
                return;
            }
            let mut term = self.session.term().lock();
            let point = grid_point(term.grid().display_offset(), col, row);
            if let Some(selection) = term.selection.as_mut() {
                selection.update(point, side);
            }
            cx.notify();
            return;
        }
        if self.gate.paused() || self.last_motion == Some((col, row)) {
            return;
        }
        let reporting = reporting(*self.session.term().lock().mode());
        let button = event
            .pressed_button
            .and_then(mouse_button)
            .unwrap_or(crate::mouse::Button::None);
        if reporting.wants(crate::mouse::Kind::Motion, button) && !event.modifiers.shift {
            let mods = mouse_mods(event.modifiers);
            if let Some(bytes) = crate::mouse::encode(
                button,
                crate::mouse::Kind::Motion,
                col,
                row,
                mods,
                reporting.sgr,
            ) {
                self.session.write(bytes);
                self.last_motion = Some((col, row));
            }
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.last_motion = None;
        if let Some((col, row)) = self.pending_menu.take() {
            // Chosen only if released where it was pressed (no drag).
            let here = self.metrics.get().map(|m| m.cell(event.position));
            if event.button == MouseButton::Left
                && here.is_some_and(|(c, r, _)| (c, r) == (col, row))
                && let Some((action, app_cursor)) = self.menu_action(row)
            {
                self.send_menu(action, app_cursor);
            }
            return;
        }
        if self.selecting {
            if event.button != MouseButton::Left {
                return;
            }
            self.selecting = false;
            // A plain click leaves no selection behind.
            let mut term = self.session.term().lock();
            if term.selection.as_ref().is_some_and(Selection::is_empty) {
                term.selection = None;
                cx.notify();
            }
            return;
        }
        let Some(button) = self.reported.take() else {
            return;
        };
        let Some(metrics) = self.metrics.get() else {
            return;
        };
        let (col, row, _) = metrics.cell(event.position);
        self.report_release(button, col, row, event.modifiers);
    }

    /// The menu click `row` would make (see `mouse::menu_click`), with
    /// whether the program uses application cursor keys; `None` when clicks
    /// are not menu choices here (see `mouse::menu_clicks_allowed`).
    fn menu_action(&self, row: usize) -> Option<(crate::mouse::MenuClick, bool)> {
        let term = self.session.term().lock();
        let mode = *term.mode();
        let allowed = crate::mouse::menu_clicks_allowed(
            self.agent,
            term.grid().display_offset() == 0,
            mode.contains(TermMode::ALT_SCREEN),
            mode.contains(TermMode::SHOW_CURSOR),
            true,
        );
        if !allowed {
            return None;
        }
        let screen: Vec<String> = (0..term.screen_lines())
            .map(|r| {
                let line = &term.grid()[Line(r as i32)];
                (0..term.columns()).map(|c| line[Column(c)].c).collect()
            })
            .collect();
        let action = crate::mouse::menu_click(&screen, row)?;
        Some((action, mode.contains(TermMode::APP_CURSOR)))
    }

    /// Sends the keys for a menu click: arrows to move, Enter to choose.
    fn send_menu(&self, action: crate::mouse::MenuClick, app_cursor: bool) {
        let bytes: Vec<u8> = match action {
            crate::mouse::MenuClick::Choose => b"\r".to_vec(),
            crate::mouse::MenuClick::Move(delta) => {
                let arrow: &[u8] = match (delta > 0, app_cursor) {
                    (true, true) => b"\x1bOB",
                    (true, false) => b"\x1b[B",
                    (false, true) => b"\x1bOA",
                    (false, false) => b"\x1b[A",
                };
                arrow.repeat(delta.unsigned_abs() as usize)
            }
        };
        self.session.write(bytes);
    }

    /// Reports the release of `button` if the program still reads the mouse.
    fn report_release(
        &self,
        button: crate::mouse::Button,
        col: usize,
        row: usize,
        modifiers: gpui::Modifiers,
    ) {
        let reporting = reporting(*self.session.term().lock().mode());
        if reporting.active()
            && let Some(bytes) = crate::mouse::encode(
                button,
                crate::mouse::Kind::Release,
                col,
                row,
                mouse_mods(modifiers),
                reporting.sgr,
            )
        {
            self.session.write(bytes);
        }
    }

    /// Where relative paths are resolved (the project) and which editor
    /// Ctrl+click opens files in.
    pub fn set_links(&mut self, base_dir: PathBuf, editor: Option<PathBuf>) {
        self.base_dir = Some(base_dir);
        self.editor = editor;
    }

    /// Text of visible row `row`, one char per cell.
    fn row_text(&self, row: usize) -> Vec<char> {
        let term = self.session.term().lock();
        if row >= term.screen_lines() {
            return Vec::new();
        }
        let offset = term.grid().display_offset() as i32;
        let line = &term.grid()[Line(row as i32 - offset)];
        (0..term.columns()).map(|c| line[Column(c)].c).collect()
    }

    /// Updates the link under the mouse; schedules a preview of a file
    /// after the mouse rests on it.
    fn hover_link(&mut self, col: usize, row: usize, ctrl: bool, cx: &mut Context<Self>) {
        let link = crate::links::link_at(&self.row_text(row), col);
        let key = link.as_ref().map(|l| (row, l.start, l.end));
        let same = self.hover.as_ref().map(|h| (h.0, h.1, h.2)) == key;
        if same && ctrl == self.ctrl {
            return;
        }
        self.ctrl = ctrl;
        if same {
            cx.notify();
            return;
        }
        self.hover = link.map(|l| (row, l.start, l.end, l.target));
        // A pasted image stays until it times out; other previews follow
        // the mouse.
        if !matches!(
            self.preview.as_ref().map(|p| &p.content),
            Some(PreviewContent::Pasted(_))
        ) {
            self.preview = None;
            self.preview_task = None;
        }
        if let Some((_, _, _, crate::links::Target::Path { path, line, .. })) = &self.hover
            && let Some(full) = crate::links::resolve(path, self.base_dir.as_deref())
        {
            let line = *line;
            let key = key.expect("hovering a link");
            self.preview_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(HOVER_PREVIEW_DELAY).await;
                let loaded = cx
                    .background_executor()
                    .spawn(async move { load_preview(&full, line) })
                    .await;
                let _ = this.update(cx, |view, cx| {
                    let still = view.hover.as_ref().map(|h| (h.0, h.1, h.2)) == Some(key);
                    if still && let Some(preview) = loaded {
                        view.preview = Some(preview);
                        cx.notify();
                    }
                });
            }));
        }
        cx.notify();
    }

    /// Opens a link: URLs in the browser; files in the editor (code) or
    /// with the system (documents and images only); anything else is shown
    /// in the file manager, never run.
    fn open_link(&self, target: &crate::links::Target, cx: &mut Context<Self>) {
        use crate::links::{Kind, Target};
        match target {
            Target::Url(url) => cx.open_url(url),
            Target::Path { path, line, col } => {
                let Some(full) = crate::links::resolve(path, self.base_dir.as_deref()) else {
                    return;
                };
                if !full.exists() {
                    return;
                }
                let image = crate::links::kind(&full) == Kind::Image;
                if !image
                    && full.is_file()
                    && let Some(editor) = &self.editor
                    && open_in_editor(editor, &crate::links::goto_argument(&full, *line, *col))
                {
                    return;
                }
                if full.is_file() && crate::links::safe_to_open(&full) {
                    cx.open_with_system(&full);
                } else {
                    cx.reveal_path(&full);
                }
            }
        }
    }

    /// Shows the image just pasted into an agent for a few seconds.
    fn show_pasted_image(&mut self, image: Arc<Image>, cx: &mut Context<Self>) {
        self.preview = Some(Preview {
            title: "Pasted image".into(),
            content: PreviewContent::Pasted(image),
        });
        self.preview_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PASTED_PREVIEW_TIME).await;
            let _ = this.update(cx, |view, cx| {
                if matches!(
                    view.preview.as_ref().map(|p| &p.content),
                    Some(PreviewContent::Pasted(_))
                ) {
                    view.preview = None;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// The image in the clipboard, if it holds one.
    fn clipboard_image(cx: &mut Context<Self>) -> Option<Arc<Image>> {
        cx.read_from_clipboard()?
            .entries()
            .iter()
            .find_map(|e| match e {
                ClipboardEntry::Image(image) => Some(Arc::new(image.clone())),
                _ => None,
            })
    }

    /// The floating preview, bottom right of the terminal.
    fn render_preview(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        use crate::theme::{
            ACCENT, BORDER, MUTED, PANEL_BG, SMALL_TEXT_SIZE, TEXT, ui, with_alpha,
        };
        let preview = self.preview.as_ref()?;
        let body = match &preview.content {
            PreviewContent::Image(path) => div().child(
                img(path.clone())
                    .max_w(px(PREVIEW_WIDTH))
                    .max_h(px(PREVIEW_HEIGHT))
                    .object_fit(ObjectFit::Contain),
            ),
            PreviewContent::Pasted(image) => div().child(
                img(ImageSource::Image(image.clone()))
                    .max_w(px(PREVIEW_WIDTH))
                    .max_h(px(PREVIEW_HEIGHT))
                    .object_fit(ObjectFit::Contain),
            ),
            PreviewContent::Text { lines, line } => div()
                .flex()
                .flex_col()
                .font_family(self.style.family.clone())
                .text_size(ui(SMALL_TEXT_SIZE))
                .children(lines.iter().map(|(n, text)| {
                    let mark = Some(*n as u32) == *line;
                    div()
                        .flex()
                        .gap_2()
                        .when(mark, |d| d.bg(gpui::rgba(with_alpha(ACCENT, 0x22))))
                        .child(
                            div()
                                .w(px(36.0))
                                .flex_none()
                                .text_color(gpui::rgb(MUTED))
                                .child(SharedString::from(n.to_string())),
                        )
                        .child(
                            div()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_color(gpui::rgb(TEXT))
                                .child(SharedString::from(text.clone())),
                        )
                })),
        };
        Some(
            div()
                .id("preview")
                .absolute()
                .bottom(px(8.0))
                .right(px(8.0))
                .max_w(px(PREVIEW_WIDTH + 24.0))
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(gpui::rgb(BORDER))
                .bg(gpui::rgb(PANEL_BG))
                .shadow_lg()
                .occlude()
                .overflow_hidden()
                .child(
                    div()
                        .text_size(ui(SMALL_TEXT_SIZE))
                        .text_color(gpui::rgb(MUTED))
                        .child(SharedString::from(format!(
                            "{} · click to close",
                            preview.title
                        ))),
                )
                .child(body)
                .on_click(cx.listener(|view, _: &gpui::ClickEvent, _, cx| {
                    view.preview = None;
                    view.preview_task = None;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// Marks the session as an agent's, whose menus can be clicked.
    pub fn set_agent(&mut self, agent: bool) {
        self.agent = agent;
    }

    /// Copies the selection to the clipboard and clears it; false if there
    /// was nothing selected.
    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let text = {
            let mut term = self.session.term().lock();
            let text = term.selection_to_string().filter(|t| !t.is_empty());
            if text.is_some() {
                term.selection = None;
            }
            text
        };
        match text {
            Some(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                cx.notify();
                true
            }
            None => false,
        }
    }

    /// Pastes the clipboard text into the session (bracketed if the program
    /// asked for it, so a pasted command is not run line by line).
    fn paste(&mut self, cx: &mut Context<Self>) {
        if self.gate.paused() {
            return;
        }
        // An image for an agent: it reads the clipboard on Ctrl+V itself.
        if self.agent
            && let Some(image) = Self::clipboard_image(cx)
        {
            self.follow_output();
            self.session.write(vec![0x16]);
            self.show_pasted_image(image, cx);
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return;
        };
        let bracketed = self
            .session
            .term()
            .lock()
            .mode()
            .contains(TermMode::BRACKETED_PASTE);
        self.follow_output();
        self.session.write(paste_bytes(&text, bracketed));
    }

    /// Back to the live screen when the user types; the selection goes.
    fn follow_output(&self) {
        let mut term = self.session.term().lock();
        term.selection = None;
        if term.grid().display_offset() != 0 {
            term.scroll_display(Scroll::Bottom);
        }
    }

    /// Types `text` into the shell as if the user had typed it.
    pub fn type_text(&self, text: &str) {
        self.session.write(text.as_bytes().to_vec());
    }

    /// Terminates the shell. The view emits `Exited` once the PTY is closed.
    pub fn shutdown(&self) {
        self.session.shutdown();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.gate.paused() {
            cx.stop_propagation();
            return;
        }
        let keystroke = &event.keystroke;
        let mods = Mods {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        // Shift+PageUp/PageDown scroll the history, like most terminals.
        if mods.shift && !mods.ctrl && !mods.alt && !self.alt_screen() {
            let page = match keystroke.key.as_str() {
                "pageup" => Some(Scroll::PageUp),
                "pagedown" => Some(Scroll::PageDown),
                _ => None,
            };
            if let Some(page) = page {
                self.session.term().lock().scroll_display(page);
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        // Copy and paste: Ctrl+Shift+C / Ctrl+Shift+V. Ctrl+C always
        // interrupts (an agent must stop even with text selected).
        let key = keystroke.key.to_ascii_lowercase();
        if mods.ctrl && !mods.alt {
            let copied = match (key.as_str(), mods.shift) {
                ("c", true) => {
                    self.copy_selection(cx);
                    true
                }
                ("v", true) => {
                    self.paste(cx);
                    true
                }
                _ => false,
            };
            if copied {
                cx.stop_propagation();
                return;
            }
        }
        let mode = KeyMode {
            app_cursor: self.session.app_cursor(),
            alt_sends_escape: cfg!(not(target_os = "macos")),
        };
        if let Some(bytes) = encode_key(&keystroke.key, mods, mode) {
            self.follow_output();
            self.session.write(bytes);
            // Agents read images from the clipboard on Ctrl+V: show it.
            if self.agent
                && mods.ctrl
                && !mods.shift
                && key == "v"
                && let Some(image) = Self::clipboard_image(cx)
            {
                self.show_pasted_image(image, cx);
            }
            // Stop here so the key does not also arrive as text input.
            cx.stop_propagation();
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session.clone();
        let style = self.style.clone();
        let scanlines = style.scanlines;
        let focus_handle = self.focus_handle.clone();
        let entity = cx.entity();
        let metrics = self.metrics.clone();
        let underline = self
            .hover
            .as_ref()
            .filter(|_| self.ctrl)
            .map(|h| (h.0, h.1, h.2));
        let preview = self.render_preview(cx);
        // Follow the match as output scrolls it; drop it if it is gone.
        let highlight = self.search.as_mut().and_then(|search| {
            search.relocate(&session.term().lock());
            search.current.clone()
        });

        div()
            .id("terminal")
            .relative()
            .size_full()
            .bg(to_hsla(DEFAULT_THEME.background, 1.0))
            .when(underline.is_some(), |d| d.cursor_pointer())
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !*hovered && view.hover.take().is_some() {
                    if !matches!(
                        view.preview.as_ref().map(|p| &p.content),
                        Some(PreviewContent::Pasted(_))
                    ) {
                        view.preview = None;
                        view.preview_task = None;
                    }
                    cx.notify();
                }
            }))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .child(
                canvas(
                    move |bounds, window, _cx| {
                        layout_frame(
                            &session,
                            &style,
                            highlight.as_ref(),
                            underline,
                            &metrics,
                            bounds,
                            window,
                        )
                    },
                    move |bounds, frame, window, cx| {
                        paint_frame(bounds, frame, window, cx);
                        if scanlines {
                            paint_scanlines(bounds, window);
                        }
                        window.handle_input(
                            &focus_handle,
                            ElementInputHandler::new(bounds, entity),
                            cx,
                        );
                    },
                )
                .size_full(),
            )
            .children(preview)
    }
}

/// Everything needed to paint one frame.
struct Frame {
    quads: Vec<PaintQuad>,
    lines: Vec<ShapedLine>,
    line_height: Pixels,
}

/// Measures the font, resizes the session to fit `bounds` and lays out the grid.
fn layout_frame(
    session: &Session,
    style: &TerminalStyle,
    highlight: Option<&Match>,
    underline: Option<(usize, usize, usize)>,
    metrics: &Cell<Option<GridMetrics>>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> Frame {
    let theme = &DEFAULT_THEME;
    let font_size = px(style.size * crate::theme::zoom());
    let regular = font(style.family.clone());
    let bold = regular.clone().bold();
    let text_system = window.text_system().clone();
    let font_id = text_system.resolve_font(&regular);
    let cell_width = text_system
        .advance(font_id, font_size, 'm')
        .map(|s| s.width)
        .unwrap_or(px(8.0));
    let line_height = (font_size * LINE_HEIGHT).round();

    let cols = ((bounds.size.width / cell_width).floor() as usize).max(2);
    let rows = ((bounds.size.height / line_height).floor() as usize).max(1);
    metrics.set(Some(GridMetrics {
        origin: bounds.origin,
        cell_width,
        line_height,
        cols,
        rows,
    }));
    session.resize(
        GridSize { cols, rows },
        (cell_width / px(1.0)) as u16,
        (line_height / px(1.0)) as u16,
    );

    let cell_origin = |col: usize, row: usize| {
        point(
            bounds.origin.x + cell_width * col as f32,
            bounds.origin.y + line_height * row as f32,
        )
    };

    let mut quads = Vec::new();
    let mut texts: Vec<String> = vec![String::new(); rows];
    let mut runs: Vec<Vec<TextRun>> = vec![Vec::new(); rows];
    let mut run_keys: Vec<Option<(Rgb8, bool)>> = vec![None; rows];

    {
        let term = session.term().lock();
        let grid_cols = term.columns();
        let content = term.renderable_content();
        let colors = content.colors;
        let offset = content.display_offset as i32;
        let cursor = content.cursor;
        let selection = content.selection;

        for indexed in content.display_iter {
            let line = indexed.point.line.0 + offset;
            let col = indexed.point.column.0;
            if line < 0 || line as usize >= rows || col >= cols.min(grid_cols) {
                continue;
            }
            let row = line as usize;
            let cell = indexed.cell;
            let flags = cell.flags;

            let mut fg = theme.resolve(cell.fg, colors);
            let mut bg = theme.resolve(cell.bg, colors);
            if flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if flags.contains(Flags::DIM) {
                fg = fg.dim();
            }
            if selection.is_some_and(|s| s.contains(indexed.point)) {
                bg = SELECTION;
            }
            if highlight.is_some_and(|m| m.contains(&indexed.point)) {
                (fg, bg) = (theme.background, SEARCH_MATCH);
            }
            let is_bold = flags.contains(Flags::BOLD);

            // Spacers become spaces so glyph index == column index.
            let hidden = Flags::HIDDEN | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER;
            let ch = if flags.intersects(hidden) || cell.c.is_control() {
                ' '
            } else {
                cell.c
            };

            texts[row].push(ch);
            let key = (fg, is_bold);
            match runs[row].last_mut() {
                Some(run) if run_keys[row] == Some(key) => run.len += ch.len_utf8(),
                _ => {
                    runs[row].push(TextRun {
                        len: ch.len_utf8(),
                        font: if is_bold {
                            bold.clone()
                        } else {
                            regular.clone()
                        },
                        color: to_hsla(fg, 1.0),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                    run_keys[row] = Some(key);
                }
            }

            if bg != theme.background {
                quads.push(fill(
                    Bounds::new(cell_origin(col, row), size(cell_width, line_height)),
                    to_hsla(bg, 1.0),
                ));
            }
        }

        // Ctrl over a link: underline it.
        if let Some((row, start, end)) = underline
            && row < rows
        {
            let end = end.min(cols);
            if start < end {
                let origin = cell_origin(start, row);
                quads.push(fill(
                    Bounds::new(
                        point(origin.x, origin.y + line_height - px(2.0)),
                        size(cell_width * (end - start) as f32, px(1.0)),
                    ),
                    to_hsla(LINK_UNDERLINE, 1.0),
                ));
            }
        }

        if !matches!(cursor.shape, CursorShape::Hidden) {
            let line = cursor.point.line.0 + offset;
            let col = cursor.point.column.0;
            if line >= 0 && (line as usize) < rows && col < cols {
                quads.push(fill(
                    Bounds::new(
                        cell_origin(col, line as usize),
                        size(cell_width, line_height),
                    ),
                    to_hsla(theme.cursor, CURSOR_ALPHA),
                ));
            }
        }
    }

    let lines = texts
        .into_iter()
        .zip(runs)
        .map(|(text, runs)| {
            text_system.shape_line(SharedString::from(text), font_size, &runs, Some(cell_width))
        })
        .collect();

    Frame {
        quads,
        lines,
        line_height,
    }
}

fn paint_frame(bounds: Bounds<Pixels>, frame: Frame, window: &mut Window, cx: &mut App) {
    // Backgrounds and cursor first, text on top.
    for quad in frame.quads {
        window.paint_quad(quad);
    }
    for (row, line) in frame.lines.iter().enumerate() {
        let origin = point(
            bounds.origin.x,
            bounds.origin.y + frame.line_height * row as f32,
        );
        // A failed line paint only affects that row; keep drawing the rest.
        let _ = line.paint(origin, frame.line_height, window, cx);
    }
}

/// Horizontal dark lines over the whole terminal, like an old CRT.
fn paint_scanlines(bounds: Bounds<Pixels>, window: &mut Window) {
    let color = to_hsla(Rgb8::hex(0x000000), SCANLINE_ALPHA);
    let scale = window.scale_factor();
    // Lines are snapped to whole device pixels so they stay sharp at
    // fractional scale factors (125 %, 150 %).
    let line_height = scale.round().max(1.0) / scale;
    let top = bounds.origin.y / px(1.0);
    for y in scanline_offsets(bounds.size.height / px(1.0)) {
        let y = snap_to_device(top + y, scale);
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x, px(y)),
                size(bounds.size.width, px(line_height)),
            ),
            color,
        ));
    }
}

/// Rounds a logical coordinate to the nearest device pixel.
fn snap_to_device(value: f32, scale: f32) -> f32 {
    (value * scale).round() / scale
}

/// Y offsets (in pixels from the top) of the scanlines for a given height.
fn scanline_offsets(height: f32) -> impl Iterator<Item = f32> {
    let count = (height.max(0.0) / SCANLINE_PERIOD).ceil() as usize;
    (0..count)
        .map(|i| i as f32 * SCANLINE_PERIOD + (SCANLINE_PERIOD - 1.0))
        // Only lines that fit completely, so nothing is drawn below the area.
        .filter(move |y| *y + 1.0 <= height)
}

fn to_hsla(color: Rgb8, alpha: f32) -> Hsla {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: alpha,
    }
    .into()
}

/// Typed text (including dead keys, AltGr and IME commits) goes to the PTY.
/// The terminal has no editable text model, so the query methods return nothing.
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if !text.is_empty() && !self.gate.paused() {
            self.follow_output();
            self.session.write(text.as_bytes().to_vec());
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        _new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // IME composition preview is out of scope for MVP-0.
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_patterns() {
        assert_eq!(literal_pattern("error"), "(?i)error");
        assert_eq!(literal_pattern("Error"), "Error");
        assert_eq!(literal_pattern("a.b*(c)"), "(?i)a\\.b\\*\\(c\\)");
        assert_eq!(literal_pattern("C:\\x"), "C:\\\\x");
        // Every metacharacter compiles and matches only itself.
        use alacritty_terminal::event::VoidListener;
        use alacritty_terminal::term::{Config, Term};
        use alacritty_terminal::vte::ansi::Processor;
        let size = crate::session::GridSize { cols: 40, rows: 2 };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(
            &mut term,
            "axb a|c ñandú\r\na.b [x] $HOME {1} ^~#&- (y)".as_bytes(),
        );
        let found = |query: &str| {
            let mut regex = RegexSearch::new(&literal_pattern(query)).unwrap();
            next_match(&term, &mut regex, None, true)
                .map(|m| term.bounds_to_string(*m.start(), *m.end()))
        };
        for query in [
            "a.b", "[x]", "$HOME", "{1}", "^~#&-", "(y)", "a|c", "ñandú", "home",
        ] {
            assert!(found(query).is_some(), "{query}");
        }
        assert_eq!(found("a.b").as_deref(), Some("a.b"));
        assert_eq!(found("x|y"), None);
        assert_eq!(found("Ñandú"), None);
        assert_eq!(found("home").as_deref(), Some("HOME"));
    }

    #[test]
    fn next_match_walks_the_history() {
        use alacritty_terminal::event::VoidListener;
        use alacritty_terminal::term::{Config, Term};
        use alacritty_terminal::vte::ansi::Processor;

        let size = crate::session::GridSize { cols: 20, rows: 3 };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser: Processor = Processor::new();
        // Five lines on a 3-line screen: two are in the history.
        parser.advance(
            &mut term,
            b"one ERR\r\ntwo\r\nthree err\r\nfour\r\nfive err",
        );
        let mut regex = RegexSearch::new(&literal_pattern("err")).unwrap();
        let line = |m: &Match| m.start().line.0;

        // Backwards from the bottom: newest first, then older, then wraps.
        let a = next_match(&term, &mut regex, None, true).unwrap();
        let b = next_match(&term, &mut regex, Some(&a), true).unwrap();
        let c = next_match(&term, &mut regex, Some(&b), true).unwrap();
        let d = next_match(&term, &mut regex, Some(&c), true).unwrap();
        assert!(line(&a) > line(&b) && line(&b) > line(&c));
        assert!(line(&c) < 0, "the oldest match is in the scrollback");
        assert_eq!(d, a);
        // Forwards goes back to the newer one.
        assert_eq!(next_match(&term, &mut regex, Some(&c), false), Some(b));

        let mut missing = RegexSearch::new(&literal_pattern("zzz")).unwrap();
        assert_eq!(next_match(&term, &mut missing, None, true), None);
        // Smart case: an uppercase query matches only that case.
        let mut upper = RegexSearch::new(&literal_pattern("ERR")).unwrap();
        let only = next_match(&term, &mut upper, None, true).unwrap();
        assert_eq!(next_match(&term, &mut upper, Some(&only), true), Some(only));
    }

    #[test]
    fn matches_follow_new_output() {
        use alacritty_terminal::event::VoidListener;
        use alacritty_terminal::term::{Config, Term};
        use alacritty_terminal::vte::ansi::Processor;

        let size = crate::session::GridSize { cols: 20, rows: 3 };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, b"a\r\nb err\r\nc");
        let mut regex = RegexSearch::new(&literal_pattern("err")).unwrap();
        let m = next_match(&term, &mut regex, None, true).unwrap();
        let text = term.bounds_to_string(*m.start(), *m.end());
        let history = term.history_size();
        assert_eq!(relocate(&term, &m, &text, history), Some(m.clone()));

        // Two more lines push it up by two.
        parser.advance(&mut term, b"\r\nd\r\ne");
        let moved = relocate(&term, &m, &text, history).unwrap();
        assert_eq!(moved.start().line.0, m.start().line.0 - 2);
        // Another text there (wrong history size): dropped, no panic.
        assert_eq!(relocate(&term, &m, &text, history + 2), None);
    }

    #[test]
    fn previews_load_text_and_skip_binaries() {
        let dir = std::env::temp_dir().join(format!("castorterm-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("a.rs");
        std::fs::write(
            &text,
            (1..=30).map(|n| format!("line {n}\n")).collect::<String>(),
        )
        .unwrap();
        let bin = dir.join("b.dat");
        std::fs::write(&bin, b"ab\0cd").unwrap();
        match load_preview(&text, Some(10)).map(|p| p.content) {
            Some(PreviewContent::Text { lines, line }) => {
                assert_eq!(line, Some(10));
                assert_eq!(lines[0].0, 7);
                assert_eq!(lines.len(), PREVIEW_LINES);
            }
            _ => panic!("text preview expected"),
        }
        assert!(load_preview(&bin, None).is_none());
        assert!(load_preview(&dir, None).is_none());
        assert!(load_preview(&dir.join("missing.rs"), None).is_none());
        let png = dir.join("c.png");
        std::fs::write(&png, b"not really").unwrap();
        assert!(matches!(
            load_preview(&png, None).map(|p| p.content),
            Some(PreviewContent::Image(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paste_encoding() {
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc");
        assert_eq!(paste_bytes("ls", true), b"\x1b[200~ls\x1b[201~");
        assert_eq!(
            paste_bytes("x\x1b[201~rm -rf", true),
            b"\x1b[200~x[201~rm -rf\x1b[201~"
        );
        // A marker split to survive one replace cannot reappear.
        let nested = paste_bytes("\x1b[20\x1b[201~1~rm -rf ~", true);
        let text = String::from_utf8(nested).unwrap();
        assert_eq!(text.matches('\x1b').count(), 2);
        assert!(text.ends_with("\x1b[201~"));
        // Other control characters are dropped, tabs kept.
        assert_eq!(paste_bytes("a\tb\x07\x03c", false), b"a\tbc");
    }

    #[test]
    fn mouse_cells_are_clamped() {
        let m = GridMetrics {
            origin: point(px(10.0), px(20.0)),
            cell_width: px(8.0),
            line_height: px(16.0),
            cols: 80,
            rows: 24,
        };
        assert_eq!(m.cell(point(px(10.0), px(20.0))), (0, 0, Side::Left));
        assert_eq!(m.cell(point(px(25.0), px(37.0))), (1, 1, Side::Right));
        assert_eq!(m.cell(point(px(0.0), px(0.0))), (0, 0, Side::Left));
        assert_eq!(m.cell(point(px(9999.0), px(9999.0))).0, 79);
        assert_eq!(m.cell(point(px(9999.0), px(9999.0))).1, 23);
        assert_eq!(grid_point(0, 3, 2), GridPoint::new(Line(2), Column(3)));
        assert_eq!(grid_point(5, 3, 2), GridPoint::new(Line(-3), Column(3)));
    }

    #[test]
    fn wheel_is_bounded() {
        assert_eq!(wheel_lines(0.0, 36.0), (2, 0.0));
        let (lines, rest) = wheel_lines(0.5, 9.0);
        assert_eq!((lines, rest), (1, 0.0));
        assert_eq!(wheel_lines(0.0, -18.0).0, -1);
        assert_eq!(wheel_lines(0.0, 4.0e12).0, MAX_WHEEL_LINES as i32);
        assert_eq!(wheel_lines(0.0, -4.0e12), (-(MAX_WHEEL_LINES as i32), 0.0));
    }

    #[test]
    fn scanlines_cover_the_height_without_overflowing() {
        let ys: Vec<f32> = scanline_offsets(10.0).collect();
        assert_eq!(ys, vec![2.0, 5.0, 8.0]);
        assert!(scanline_offsets(0.0).next().is_none());
        assert!(scanline_offsets(-5.0).next().is_none());
        // Fractional heights never overflow the area.
        let ys: Vec<f32> = scanline_offsets(8.5).collect();
        assert_eq!(ys, vec![2.0, 5.0]);
        let ys: Vec<f32> = scanline_offsets(9.0).collect();
        assert_eq!(ys, vec![2.0, 5.0, 8.0]);
    }

    #[test]
    fn snapping_to_device_pixels() {
        assert_eq!(snap_to_device(10.3, 1.0), 10.0);
        assert_eq!(snap_to_device(10.3, 1.5), 10.0);
        assert_eq!(snap_to_device(10.4, 1.5), 16.0 / 1.5);
        assert_eq!(snap_to_device(10.5, 2.0), 10.5);
    }

    #[test]
    fn style_defaults() {
        let style = TerminalStyle::new(None, None, true);
        assert_eq!(style.family.as_ref(), FONT_FAMILY);
        assert_eq!(style.size, FONT_SIZE);
        let style = TerminalStyle::new(Some("VT323"), Some(18.0), false);
        assert_eq!(style.family.as_ref(), "VT323");
        assert!(!style.scanlines);
    }
}
