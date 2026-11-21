//! Command palette (Ctrl+Shift+P): open any launcher in any project, jump
//! to a session, add folders, all from the keyboard.

use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, KeyDownEvent, MouseButton, SharedString, Window,
    div, prelude::*, px, rgb, rgba,
};

use super::{EditTarget, MiddleTab, WorkspaceView};
use crate::agent_status::AgentStatus;
use crate::commands::{self, Action, Entry};
use crate::keys::Mods;
use crate::name_input::{InputAction, NameInput};
use crate::theme::{
    ACCENT, BORDER, MUTED, PANEL_BG, SMALL_TEXT_SIZE, TEXT, WORKSPACE, launcher_color, with_alpha,
};

/// The open palette.
pub(super) struct Palette {
    pub input: NameInput,
    /// Every action, built when the palette opens.
    pub entries: Vec<Entry>,
    /// Indices of `entries` matching the query, best first.
    pub shown: Vec<usize>,
    /// Highlighted row in `shown`.
    pub selected: usize,
}

impl WorkspaceView {
    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() {
            return;
        }
        let entries = self.palette_entries();
        let shown = commands::filter(&entries, "");
        self.palette = Some(Palette {
            input: NameInput::default(),
            entries,
            shown,
            selected: 0,
        });
        // Same as the name field: the view takes the keyboard and the
        // terminals ignore input meanwhile.
        self.sync_input_gate();
        window.focus(&self.focus);
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.sync_input_gate();
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Sessions first (the most common jump), then launchers of the selected
    /// project, then every other project, then the rest.
    fn palette_entries(&self) -> Vec<Entry> {
        let mut entries = Vec::new();
        // Agents waiting for the user first.
        let mut tabs: Vec<_> = self.tabs.iter().collect();
        tabs.sort_by_key(|t| self.statuses.get(&t.id) != Some(&AgentStatus::Waiting));
        for tab in tabs {
            let project = &self.tree.projects[tab.project].name;
            entries.push(Entry {
                title: format!("Go to {}", tab.label),
                detail: match self.statuses.get(&tab.id) {
                    Some(status) => format!("{project} · {}", status.as_str()),
                    None => project.clone(),
                },
                action: Action::Activate(tab.id),
            });
        }
        let mut projects: Vec<(usize, usize)> = self
            .tree
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(w, ws)| ws.projects.iter().map(move |&p| (w, p)))
            .collect();
        projects.sort_by_key(|&(_, p)| p != self.selected_project);
        for (workspace, project) in projects {
            for (index, launcher) in self.launchers.iter().enumerate() {
                entries.push(Entry {
                    title: format!("Open {}", launcher.name),
                    detail: format!(
                        "{} · {}",
                        self.tree.projects[project].name, self.tree.workspaces[workspace].name
                    ),
                    action: Action::Launch {
                        project,
                        launcher: index,
                    },
                });
            }
        }
        for (index, workspace) in self.tree.workspaces.iter().enumerate() {
            entries.push(Entry {
                title: "Add folders".into(),
                detail: workspace.name.clone(),
                action: Action::AddFolder(index),
            });
        }
        entries.push(Entry {
            title: "New workspace".into(),
            detail: String::new(),
            action: Action::NewWorkspace,
        });
        entries.push(Entry {
            title: "Show files".into(),
            detail: String::new(),
            action: Action::ShowFiles,
        });
        entries.push(Entry {
            title: "Show quests".into(),
            detail: String::new(),
            action: Action::ShowQuests,
        });
        entries
    }

    /// Keys while the palette is open; they never reach the terminal.
    pub(super) fn on_palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "down" => {
                if !palette.shown.is_empty() {
                    palette.selected = (palette.selected + 1) % palette.shown.len();
                }
                cx.notify();
                return;
            }
            "up" => {
                if !palette.shown.is_empty() {
                    palette.selected =
                        (palette.selected + palette.shown.len() - 1) % palette.shown.len();
                }
                cx.notify();
                return;
            }
            _ => {}
        }
        let mods = Mods {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        match palette
            .input
            .handle(&keystroke.key, keystroke.key_char.as_deref(), mods)
        {
            InputAction::Commit => {
                if let Some(&index) = palette.shown.get(palette.selected) {
                    self.run_palette_entry(index, window, cx);
                }
                return;
            }
            InputAction::Cancel => {
                self.close_palette(window, cx);
                return;
            }
            InputAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    palette.input.insert(&text);
                }
            }
            InputAction::Changed => {}
            // Left/Right, Tab...: nothing changed, keep the selection.
            InputAction::Ignored => return,
        }
        palette.shown = commands::filter(&palette.entries, &palette.input.text);
        palette.selected = 0;
        cx.notify();
    }

    /// Runs entry `index` (of all entries) and closes the palette.
    fn run_palette_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self
            .palette
            .as_ref()
            .and_then(|p| p.entries.get(index))
            .map(|e| e.action.clone())
        else {
            return;
        };
        // Sessions can end while the palette is open.
        if let Action::Activate(id) = action
            && !self.tabs.iter().any(|t| t.id == id)
        {
            self.notice = Some("That session has ended".into());
            cx.notify();
            return;
        }
        self.close_palette(window, cx);
        match action {
            Action::Launch { project, launcher } => {
                // The project may have been removed since the palette opened.
                if self.tree.workspace_of(project).is_some() {
                    self.launch(project, launcher, window, cx);
                }
            }
            Action::Activate(id) => self.activate(id, window, cx),
            Action::AddFolder(workspace) => {
                if workspace < self.tree.workspaces.len() {
                    self.pick_folders(workspace, window, cx);
                }
            }
            Action::NewWorkspace => self.open_editor(EditTarget::NewWorkspace, window, cx),
            Action::ShowFiles => {
                self.show_files = true;
                self.show_tab(MiddleTab::Files, cx);
            }
            Action::ShowQuests => {
                self.show_files = true;
                self.show_tab(MiddleTab::Quests, cx);
            }
        }
    }

    pub(super) fn render_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let palette = self.palette.as_ref()?;
        let query = palette.input.text.clone();
        let rows = palette.shown.iter().enumerate().map(|(row, &index)| {
            let entry = &palette.entries[index];
            let selected = row == palette.selected;
            let color = match entry.action {
                Action::Launch { launcher, .. } => launcher_color(launcher),
                Action::Activate(_) => ACCENT,
                _ => WORKSPACE,
            };
            div()
                .id(("palette-row", row))
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py(px(6.0))
                .rounded_md()
                .cursor_pointer()
                .when(selected, |d| d.bg(rgba(with_alpha(color, 0x26))))
                .hover(|s| s.bg(rgba(with_alpha(TEXT, 0x0d))))
                .child(
                    div()
                        .flex_none()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded_full()
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                        .child(SharedString::from(entry.title.clone())),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(SMALL_TEXT_SIZE))
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(entry.detail.clone())),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.run_palette_entry(index, window, cx)
                }))
        });
        let empty = palette.shown.is_empty();
        Some(
            // Full-window layer: a click outside the box closes the palette.
            div()
                .id("palette-layer")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .pt(px(90.0))
                .bg(rgba(0x0000_0066))
                // Clicks, hover and scroll never reach what is underneath.
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
                )
                .child(
                    div()
                        .id("palette")
                        .w(px(560.0))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(PANEL_BG))
                        // Clicks inside do not close it.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgba(with_alpha(ACCENT, 0xaa)))
                                .text_color(rgb(if query.is_empty() { MUTED } else { TEXT }))
                                .child(SharedString::from(if query.is_empty() {
                                    "Open an agent, go to a session, add folders…".to_string()
                                } else {
                                    query
                                }))
                                .child(div().w(px(2.0)).h(px(16.0)).ml(px(1.0)).bg(rgb(ACCENT))),
                        )
                        .children(rows)
                        .when(empty, |d| {
                            d.child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_color(rgb(MUTED))
                                    .font_weight(FontWeight::NORMAL)
                                    .child("No matches"),
                            )
                        }),
                )
                .into_any_element(),
        )
    }
}
