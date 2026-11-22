//! GPUI view that renders a terminal session and forwards input to it.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use alacritty_terminal::grid::{Dimensions as _, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Point as GridPoint, Side};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::vte::ansi::CursorShape;
use futures::channel::mpsc::UnboundedReceiver;
use futures::{FutureExt as _, StreamExt as _};
use gpui::{
    App, Bounds, Context, ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, Hsla, KeyDownEvent, PaintQuad, Pixels, Point, Rgba, ShapedLine, SharedString, Task,
    TextRun, UTF16Selection, Window, canvas, div, fill, font, point, prelude::*, px, size,
};

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
    _events: Task<()>,
}

/// A scrollback search: the compiled query and the match shown.
struct Search {
    query: String,
    regex: RegexSearch,
    current: Option<Match>,
}

/// Outcome of [`TerminalView::find`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindResult {
    Found,
    NotFound,
    /// The query could not be searched (too long or empty).
    Invalid,
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
                            SessionEvent::Cwd(dir) => cwd = Some(dir),
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
        if self.search.as_ref().is_none_or(|s| s.query != query) {
            let Ok(regex) = RegexSearch::new(&literal_pattern(query)) else {
                self.clear_search(cx);
                return FindResult::Invalid;
            };
            self.search = Some(Search {
                query: query.to_string(),
                regex,
                current: None,
            });
        }
        let search = self.search.as_mut().expect("set above");
        let mut term = self.session.term().lock();
        let found = next_match(&term, &mut search.regex, search.current.as_ref(), backwards);
        if let Some(m) = &found {
            term.scroll_to_point(*m.start());
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
        let line = px(WHEEL_LINE_PX);
        let lines = self.wheel_lines + f32::from(event.delta.pixel_delta(line).y) / WHEEL_LINE_PX;
        let whole = lines.trunc();
        self.wheel_lines = lines - whole;
        self.scroll_lines(whole as i32, cx);
    }

    /// Back to the live screen when the user types.
    fn follow_output(&self) {
        let mut term = self.session.term().lock();
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
        let mode = KeyMode {
            app_cursor: self.session.app_cursor(),
            alt_sends_escape: cfg!(not(target_os = "macos")),
        };
        if let Some(bytes) = encode_key(&keystroke.key, mods, mode) {
            self.follow_output();
            self.session.write(bytes);
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
        let highlight = self.search.as_ref().and_then(|s| s.current.clone());

        div()
            .size_full()
            .bg(to_hsla(DEFAULT_THEME.background, 1.0))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .child(
                canvas(
                    move |bounds, window, _cx| {
                        layout_frame(&session, &style, highlight.as_ref(), bounds, window)
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
        for query in ["a.b", "[x]", "$HOME", "a|b", "{1}", "^~#&-", "Á"] {
            let mut regex = RegexSearch::new(&literal_pattern(query)).unwrap();
            let _ = &mut regex;
        }
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
