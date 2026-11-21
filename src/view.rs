//! GPUI view that renders a terminal session and forwards input to it.

use std::ops::Range;

use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::term::cell::Flags;
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

/// Events a terminal reports to its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    /// The shell exited; the session is over.
    Exited,
}

pub struct TerminalView {
    focus_handle: FocusHandle,
    session: Session,
    style: TerminalStyle,
    _events: Task<()>,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl TerminalView {
    pub fn new(
        session: Session,
        events: UnboundedReceiver<SessionEvent>,
        style: TerminalStyle,
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
                    for event in batch {
                        match event {
                            SessionEvent::Wakeup => {}
                            SessionEvent::PtyWrite(text) => view.session.write(text.into_bytes()),
                            SessionEvent::Exit => exited = true,
                        }
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
            _events: events_task,
        }
    }

    /// Terminates the shell. The view emits `Exited` once the PTY is closed.
    pub fn shutdown(&self) {
        self.session.shutdown();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let mods = Mods {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        let mode = KeyMode {
            app_cursor: self.session.app_cursor(),
            alt_sends_escape: cfg!(not(target_os = "macos")),
        };
        if let Some(bytes) = encode_key(&keystroke.key, mods, mode) {
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

        div()
            .size_full()
            .bg(to_hsla(DEFAULT_THEME.background, 1.0))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                canvas(
                    move |bounds, window, _cx| layout_frame(&session, &style, bounds, window),
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
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> Frame {
    let theme = &DEFAULT_THEME;
    let font_size = px(style.size);
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
    for y in scanline_offsets(bounds.size.height / px(1.0)) {
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x, bounds.origin.y + px(y)),
                size(bounds.size.width, px(1.0)),
            ),
            color,
        ));
    }
}

/// Y offsets (in pixels from the top) of the scanlines for a given height.
fn scanline_offsets(height: f32) -> impl Iterator<Item = f32> {
    let count = (height.max(0.0) / SCANLINE_PERIOD).ceil() as usize;
    (0..count)
        .map(|i| i as f32 * SCANLINE_PERIOD + (SCANLINE_PERIOD - 1.0))
        .filter(move |y| *y < height)
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
        if !text.is_empty() {
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
    fn scanlines_cover_the_height_without_overflowing() {
        let ys: Vec<f32> = scanline_offsets(10.0).collect();
        assert_eq!(ys, vec![2.0, 5.0, 8.0]);
        assert!(scanline_offsets(0.0).next().is_none());
        assert!(scanline_offsets(-5.0).next().is_none());
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
