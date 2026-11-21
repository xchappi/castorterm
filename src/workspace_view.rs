//! Root view: sidebar (Workspace -> Project -> Session), tab bar and the
//! active terminal.

use std::path::PathBuf;

use futures::channel::mpsc;
use gpui::{
    BoxShadow, ClickEvent, Context, Div, ElementId, Entity, Focusable as _, FontWeight,
    KeyDownEvent, Rgba, SharedString, Stateful, Subscription, Window, div, linear_color_stop,
    linear_gradient, point, prelude::*, px, rgb, rgba,
};

use crate::config::Settings;
use crate::keys::Mods;
use crate::palette::DEFAULT_THEME;
use crate::session::{GridSize, Session};
use crate::shortcuts::{Shortcut, match_shortcut};
use crate::tabs::{SessionId, Tabs};
use crate::theme::{
    ACCENT, BORDER, ERROR, HUD_BG, LOGO_FONT, LOGO_TEXT_SIZE, MUTED, PANEL_BG, SMALL_TEXT_SIZE,
    TEXT, TEXT_SIZE, TITLE, TITLE_TEXT_SIZE, UI_FONT, WORKSPACE, label, launcher_color, with_alpha,
};
use crate::tree::{Launcher, ProjectTree, SHELL_LAUNCHER, launchers};
use crate::view::{TerminalEvent, TerminalStyle, TerminalView};

const SIDEBAR_WIDTH: f32 = 260.0;
const TAB_BAR_HEIGHT: f32 = 36.0;
const HUD_HEIGHT: f32 = 46.0;
/// Placeholder until the first layout measures the real terminal area.
const INITIAL_GRID: GridSize = GridSize {
    cols: 100,
    rows: 30,
};

/// An open session: its terminal view and the subscription to its events.
struct OpenSession {
    view: Entity<TerminalView>,
    /// Class colour of the launcher that started it.
    color: u32,
    _subscription: Subscription,
}

pub struct WorkspaceView {
    tree: ProjectTree,
    launchers: Vec<Launcher>,
    style: TerminalStyle,
    tabs: Tabs<OpenSession>,
    /// Config warnings, shown for the whole run.
    config_messages: Vec<String>,
    /// Last failure to start a session, cleared by the next success.
    launch_error: Option<String>,
}

impl WorkspaceView {
    /// Builds the tree from `settings` and opens a shell in `launch_dir`.
    pub fn new(
        settings: Settings,
        launch_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut tree = ProjectTree::from_config(&settings.workspaces);
        let initial_project = tree.project_for_dir(&launch_dir);
        let mut this = Self {
            tree,
            launchers: launchers(&settings.agents),
            style: TerminalStyle::new(
                settings.font_family.as_deref(),
                settings.font_size,
                settings.scanlines,
            ),
            tabs: Tabs::default(),
            config_messages: settings.messages,
            launch_error: None,
        };
        this.launch(initial_project, SHELL_LAUNCHER, window, cx);
        this
    }

    /// Starts a session for `launcher` in `project` and activates it.
    fn launch(
        &mut self,
        project: usize,
        launcher: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let color = launcher_color(launcher);
        let (Some(node), Some(launcher)) = (
            self.tree.projects.get(project),
            self.launchers.get(launcher).cloned(),
        ) else {
            return;
        };
        cx.notify();
        if !node.path.is_dir() {
            self.launch_error = Some(format!(
                "Project directory not found: {}",
                node.path.display()
            ));
            return;
        }

        let (events_tx, events_rx) = mpsc::unbounded();
        let session = match Session::spawn(INITIAL_GRID, Some(node.path.clone()), events_tx) {
            Ok(session) => session,
            Err(err) => {
                self.launch_error = Some(format!("{err:#}"));
                return;
            }
        };
        if let Some(command) = &launcher.command {
            // The PTY buffers this until the shell reads its input.
            session.write(format!("{command}\r").into_bytes());
        }

        let style = self.style.clone();
        let view = cx.new(|cx| TerminalView::new(session, events_rx, style, cx));
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        self.tabs.open(
            project,
            &launcher.name,
            OpenSession {
                view,
                color,
                _subscription: subscription,
            },
        );
        self.launch_error = None;
        self.focus_active(window, cx);
    }

    fn on_terminal_event(
        &mut self,
        emitter: &Entity<TerminalView>,
        event: &TerminalEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalEvent::Exited => {
                if let Some(id) = self.tabs.find(|s| s.view == *emitter) {
                    self.close(id, window, cx);
                }
            }
        }
    }

    /// Ends a session and removes its tab; quits when none are left.
    fn close(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.tabs.close(id) else {
            return;
        };
        session.view.read(cx).shutdown();
        if self.tabs.is_empty() {
            cx.quit();
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn activate(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.activate(id) {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// Gives keyboard focus to the active terminal and updates the title.
    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.active() else {
            window.set_window_title("castorterm");
            return;
        };
        let handle = tab.handle.view.read(cx).focus_handle(cx);
        window.focus(&handle);
        let project = &self.tree.projects[tab.project].name;
        window.set_window_title(&format!("{} · {project} — castorterm", tab.label));
    }

    /// App shortcuts run in the capture phase, before the terminal sees the key.
    fn on_key_capture(
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
        let Some(shortcut) = match_shortcut(&keystroke.key, mods) else {
            return;
        };
        cx.stop_propagation();
        if event.is_held && !shortcut.repeats() {
            return;
        }
        match shortcut {
            Shortcut::NewShell => {
                let project = self.tabs.active().map(|t| t.project).unwrap_or(0);
                self.launch(project, SHELL_LAUNCHER, window, cx);
            }
            Shortcut::CloseSession => {
                if let Some(id) = self.tabs.active_id() {
                    self.close(id, window, cx);
                }
            }
            Shortcut::NextSession => {
                self.tabs.next();
                self.focus_active(window, cx);
                cx.notify();
            }
            Shortcut::PrevSession => {
                self.tabs.prev();
                self.focus_active(window, cx);
                cx.notify();
            }
        }
    }

    fn render_hud(&self) -> impl IntoElement {
        let sessions = self.tabs.iter().count();
        let content = div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(HUD_HEIGHT - 2.0))
            .px_4()
            .bg(linear_gradient(
                90.0,
                linear_color_stop(rgb(HUD_BG), 0.0),
                linear_color_stop(rgb(PANEL_BG), 1.0),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(dot(TITLE, 10.0, true))
                    .child(
                        div()
                            .font_family(LOGO_FONT)
                            .text_size(px(LOGO_TEXT_SIZE))
                            .text_color(rgb(TITLE))
                            .child("CASTORTERM"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(chip(
                        format!("{sessions} {}", plural(sessions, "session", "sessions")),
                        ACCENT,
                    ))
                    .child(chip(
                        concat!("LVL ", env!("CARGO_PKG_VERSION")).to_string(),
                        WORKSPACE,
                    )),
            );
        // Neon line under the HUD.
        let accent = div().w_full().h(px(2.0)).bg(linear_gradient(
            90.0,
            linear_color_stop(rgb(TITLE), 0.0),
            linear_color_stop(rgba(with_alpha(ACCENT, 0x00)), 1.0),
        ));
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(content)
            .child(accent)
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tabs.active_id();
        let messages = self
            .config_messages
            .iter()
            .chain(self.launch_error.as_ref())
            .map(|msg| {
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgba(with_alpha(ERROR, 0x99)))
                    .bg(rgba(with_alpha(ERROR, 0x14)))
                    .text_size(px(SMALL_TEXT_SIZE))
                    .text_color(rgb(ERROR))
                    .child(SharedString::from(msg.clone()))
            });

        let workspaces = self.tree.workspaces.iter().map(|workspace| {
            let projects = workspace.projects.iter().map(|&project| {
                let node = &self.tree.projects[project];
                let launchers = self.launchers.iter().enumerate().map(|(index, launcher)| {
                    launcher_button(
                        (SharedString::from(format!("launch-{project}")), index),
                        format!("+ {}", launcher.name),
                        launcher_color(index),
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, window, cx| {
                            this.launch(project, index, window, cx)
                        },
                    ))
                });
                let sessions = self
                    .tabs
                    .iter()
                    .filter(|tab| tab.project == project)
                    .map(|tab| {
                        let id = tab.id;
                        let is_active = active == Some(id);
                        let color = tab.handle.color;
                        div()
                            .id(("session", id))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py(px(5.0))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(if is_active { TEXT } else { MUTED }))
                            .when(is_active, |d| {
                                d.bg(linear_gradient(
                                    90.0,
                                    linear_color_stop(rgba(with_alpha(color, 0x38)), 0.0),
                                    linear_color_stop(rgba(with_alpha(color, 0x00)), 1.0),
                                ))
                            })
                            .when(!is_active, |d| {
                                d.hover(|s| s.bg(rgba(with_alpha(TEXT, 0x0d))))
                            })
                            .child(dot(color, 7.0, is_active))
                            .child(SharedString::from(tab.label.clone()))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.activate(id, window, cx)
                            }))
                    });
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pb_3()
                    .child(
                        div()
                            .px_2()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(SharedString::from(node.name.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .px_2()
                            .pb_1()
                            .children(launchers),
                    )
                    .children(sessions)
            });
            div()
                .flex()
                .flex_col()
                .gap_2()
                .pt_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .text_size(px(SMALL_TEXT_SIZE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(WORKSPACE))
                        .child(dot(WORKSPACE, 6.0, true))
                        .child(SharedString::from(label(&workspace.name))),
                )
                .children(projects)
        });

        div()
            .id("sidebar")
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .overflow_y_scroll()
            .bg(rgb(PANEL_BG))
            .border_r_1()
            .border_color(rgb(BORDER))
            .text_size(px(TEXT_SIZE))
            .text_color(rgb(MUTED))
            .child(
                div()
                    .px_2()
                    .text_size(px(SMALL_TEXT_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("QUEST LOG"),
            )
            .children(messages)
            .children(workspaces)
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tabs.active_id();
        let tabs = self.tabs.iter().map(|tab| {
            let id = tab.id;
            let is_active = active == Some(id);
            let color = tab.handle.color;
            let project = &self.tree.projects[tab.project].name;
            div()
                .id(("tab", id))
                .relative()
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .h_full()
                .pl_3()
                .pr_1()
                .rounded_t_md()
                .cursor_pointer()
                .when(is_active, |d| {
                    d.bg(theme_bg()).child(
                        // Glowing accent on top of the active tab.
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(2.0))
                            .bg(rgb(color))
                            .shadow(glow(color, 0x99, 8.0)),
                    )
                })
                .when(!is_active, |d| {
                    d.hover(|s| s.bg(rgba(with_alpha(TEXT, 0x0d))))
                })
                .child(dot(color, 7.0, is_active))
                .child(
                    div()
                        .text_color(rgb(if is_active { TEXT } else { MUTED }))
                        .child(SharedString::from(tab.label.clone())),
                )
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(format!("· {project}"))),
                )
                .child(
                    div()
                        .id(("close", id))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(18.0))
                        .rounded_sm()
                        .text_color(rgb(MUTED))
                        .hover(|s| s.bg(rgba(with_alpha(ERROR, 0x33))).text_color(rgb(ERROR)))
                        .child("×")
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.close(id, window, cx);
                        })),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.activate(id, window, cx)
                }))
        });

        div()
            .id("tab-bar")
            .flex_none()
            .flex()
            .flex_row()
            .items_end()
            .gap_1()
            .w_full()
            .h(px(TAB_BAR_HEIGHT))
            .px_2()
            .pt_1()
            .overflow_x_scroll()
            .bg(rgb(PANEL_BG))
            .text_size(px(TEXT_SIZE))
            .children(tabs)
    }
}

/// Soft coloured glow around an element.
fn glow(color: u32, alpha: u8, blur: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(with_alpha(color, alpha)).into(),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
    }]
}

/// Round status light, glowing when lit.
fn dot(color: u32, side: f32, lit: bool) -> Div {
    div()
        .flex_none()
        .size(px(side))
        .rounded_full()
        .bg(rgba(with_alpha(color, if lit { 0xff } else { 0x99 })))
        .when(lit, |d| d.shadow(glow(color, 0xaa, 6.0)))
}

/// Small rounded counter in the HUD.
fn chip(text: String, color: u32) -> Div {
    div()
        .px_2()
        .py(px(2.0))
        .rounded_md()
        .border_1()
        .border_color(rgba(with_alpha(color, 0x66)))
        .bg(rgba(with_alpha(color, 0x14)))
        .text_size(px(SMALL_TEXT_SIZE))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(color))
        .child(SharedString::from(text))
}

/// Tinted button in the launcher's class colour that lights up on hover.
fn launcher_button(id: impl Into<ElementId>, text: String, color: u32) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py(px(3.0))
        .rounded_md()
        .border_1()
        .border_color(rgba(with_alpha(color, 0x80)))
        .bg(rgba(with_alpha(color, 0x14)))
        .text_size(px(SMALL_TEXT_SIZE))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(color))
        .cursor_pointer()
        .hover(move |s| {
            s.bg(rgba(with_alpha(color, 0x33)))
                .border_color(rgb(color))
                .shadow(glow(color, 0x66, 10.0))
        })
        .active(move |s| s.bg(rgba(with_alpha(color, 0x55))))
        .child(SharedString::from(text))
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

fn theme_bg() -> Rgba {
    let c = DEFAULT_THEME.background;
    rgb(u32::from_be_bytes([0, c.r, c.g, c.b]))
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let terminal = match self.tabs.active() {
            Some(tab) => {
                let color = tab.handle.color;
                div().size_full().p_2().child(
                    div()
                        .size_full()
                        .p(px(6.0))
                        .rounded_lg()
                        .border_1()
                        .border_color(rgba(with_alpha(color, 0x88)))
                        .bg(theme_bg())
                        .shadow(glow(color, 0x33, 18.0))
                        .child(tab.handle.view.clone()),
                )
            }
            None => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(TITLE_TEXT_SIZE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TITLE))
                        .child("Press + to start"),
                )
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .child("Pick a launcher in the quest log"),
                ),
        };

        div()
            .key_context("Workspace")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(HUD_BG))
            .font_family(UI_FONT)
            .text_size(px(TEXT_SIZE))
            .text_color(rgb(TEXT))
            .capture_key_down(cx.listener(Self::on_key_capture))
            .child(self.render_hud())
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .flex()
                    .flex_row()
                    .child(self.render_sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .bg(rgb(HUD_BG))
                            .child(self.render_tab_bar(cx))
                            .child(div().flex_1().min_h(px(0.0)).w_full().child(terminal)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::plural;

    #[test]
    fn plural_forms() {
        assert_eq!(plural(1, "session", "sessions"), "session");
        assert_eq!(plural(0, "session", "sessions"), "sessions");
        assert_eq!(plural(3, "session", "sessions"), "sessions");
    }
}
