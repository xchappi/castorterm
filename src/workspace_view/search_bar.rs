//! Scrollback search bar (Ctrl+Shift+F) over the active terminal.

use gpui::{
    AnyElement, Context, KeyDownEvent, SharedString, Window, div, prelude::*, px, rgb, rgba,
};

use super::WorkspaceView;
use crate::keys::Mods;
use crate::name_input::{InputAction, NameInput};
use crate::tabs::SessionId;
use crate::theme::{ACCENT, BORDER, ERROR, MUTED, PANEL_BG, SMALL_TEXT_SIZE, TEXT, ui, with_alpha};
use crate::view::{FindResult, MAX_QUERY_CHARS};

/// The open search bar.
pub(super) struct SearchBar {
    /// Session searched; the bar closes if another one becomes active.
    pub session: SessionId,
    pub input: NameInput,
    pub result: Option<FindResult>,
}

impl WorkspaceView {
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() || self.palette.is_some() {
            return;
        }
        let Some(session) = self.tabs.active_id() else {
            return;
        };
        // Reopening keeps the query: Enter goes on from the match shown.
        if self.search.as_ref().is_none_or(|s| s.session != session) {
            self.close_search_quietly(cx);
            self.search = Some(SearchBar {
                session,
                input: NameInput::default(),
                result: None,
            });
        }
        // Like the palette: the view takes the keyboard, terminals ignore it.
        self.sync_input_gate();
        window.focus(&self.focus);
        cx.notify();
    }

    /// Closes the bar and clears the highlight, without moving focus.
    pub(super) fn close_search_quietly(&mut self, cx: &mut Context<Self>) {
        let Some(bar) = self.search.take() else {
            return;
        };
        if let Some(tab) = self.tabs.iter().find(|t| t.id == bar.session) {
            tab.handle.view.update(cx, |view, cx| view.clear_search(cx));
        }
        self.sync_input_gate();
        cx.notify();
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_search_quietly(cx);
        self.focus_active(window, cx);
    }

    /// Keys while the bar is open; they never reach the terminal.
    pub(super) fn on_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let mods = Mods {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        // Enter: older match; Shift+Enter: newer.
        if keystroke.key == "enter" {
            self.run_search(!mods.shift, cx);
            return;
        }
        let Some(bar) = self.search.as_mut() else {
            return;
        };
        match bar
            .input
            .handle(&keystroke.key, keystroke.key_char.as_deref(), mods)
        {
            InputAction::Cancel => self.close_search(window, cx),
            InputAction::Changed => self.run_search(true, cx),
            InputAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    // One line: a pasted block searches its first line.
                    let line = text.lines().next().unwrap_or_default().to_string();
                    bar.input.insert(&line);
                    self.run_search(true, cx);
                }
            }
            InputAction::Commit | InputAction::Ignored => {}
        }
    }

    /// Searches the bar's query in its session (see `TerminalView::find`).
    fn run_search(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let Some(bar) = self.search.as_ref() else {
            return;
        };
        let (session, query) = (bar.session, bar.input.text.clone());
        let Some(tab) = self.tabs.iter().find(|t| t.id == session) else {
            return;
        };
        let result = tab
            .handle
            .view
            .update(cx, |view, cx| view.find(&query, backwards, cx));
        if let Some(bar) = self.search.as_mut() {
            bar.result = (!query.is_empty()).then_some(result);
        }
        cx.notify();
    }

    pub(super) fn render_search_bar(&self) -> Option<AnyElement> {
        let bar = self.search.as_ref()?;
        let empty = bar.input.text.is_empty();
        let (status, color) = match bar.result {
            Some(FindResult::Found) => ("Enter older · Shift+Enter newer · Esc", MUTED),
            Some(FindResult::NotFound) => ("No matches", ERROR),
            Some(FindResult::Invalid) if bar.input.text.chars().count() > MAX_QUERY_CHARS => {
                ("Query too long", ERROR)
            }
            Some(FindResult::Invalid) => ("Cannot search that", ERROR),
            None => ("Search the history · Esc to close", MUTED),
        };
        Some(
            div()
                .absolute()
                .top(px(14.0))
                .right(px(22.0))
                .w(px(360.0))
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(PANEL_BG))
                .shadow_lg()
                .occlude()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .px_2()
                        .py(px(4.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgba(with_alpha(ACCENT, 0xaa)))
                        .bg(rgba(with_alpha(ACCENT, 0x10)))
                        .text_color(rgb(if empty { MUTED } else { TEXT }))
                        .child(SharedString::from(if empty {
                            "Find in history".to_string()
                        } else {
                            bar.input.text.clone()
                        }))
                        .child(div().w(px(2.0)).h(px(14.0)).ml(px(1.0)).bg(rgb(ACCENT))),
                )
                .child(
                    div()
                        .px_1()
                        .text_size(ui(SMALL_TEXT_SIZE))
                        .text_color(rgb(color))
                        .child(status),
                )
                .into_any_element(),
        )
    }
}
