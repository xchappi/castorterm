//! Root view: sidebar (Workspace -> Project -> Session), tab bar and the
//! active terminal.

use std::path::PathBuf;

use futures::channel::mpsc;
use gpui::{
    ClickEvent, Context, Entity, Focusable as _, KeyDownEvent, Rgba, SharedString, Subscription,
    Window, div, prelude::*, px, rgb,
};

use crate::config::Settings;
use crate::keys::Mods;
use crate::palette::DEFAULT_THEME;
use crate::session::{GridSize, Session};
use crate::shortcuts::{Shortcut, match_shortcut};
use crate::tabs::{SessionId, Tabs};
use crate::tree::{Launcher, ProjectTree, launchers};
use crate::view::{TerminalEvent, TerminalStyle, TerminalView};

const SIDEBAR_WIDTH: f32 = 240.0;
const TAB_BAR_HEIGHT: f32 = 32.0;
/// Placeholder until the first layout measures the real terminal area.
const INITIAL_GRID: GridSize = GridSize {
    cols: 100,
    rows: 30,
};

// UI chrome colours (the terminal itself uses the theme).
const CHROME_BG: u32 = 0x181818;
const BORDER: u32 = 0x2b2b2b;
const HOVER_BG: u32 = 0x2a2d2e;
const ACTIVE_BG: u32 = 0x37373d;
const TEXT: u32 = 0xcccccc;
const MUTED: u32 = 0x8b8b8b;
const ERROR: u32 = 0xf14c4c;

/// An open session: its terminal view and the subscription to its events.
struct OpenSession {
    view: Entity<TerminalView>,
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
            style: TerminalStyle::new(settings.font_family.as_deref(), settings.font_size),
            tabs: Tabs::default(),
            config_messages: settings.messages,
            launch_error: None,
        };
        this.launch(initial_project, 0, window, cx);
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
        match shortcut {
            Shortcut::NewShell => {
                let project = self.tabs.active().map(|t| t.project).unwrap_or(0);
                self.launch(project, 0, window, cx);
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

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tabs.active_id();
        let messages = self
            .config_messages
            .iter()
            .chain(self.launch_error.as_ref())
            .map(|msg| {
                div()
                    .px_1()
                    .text_xs()
                    .text_color(rgb(ERROR))
                    .child(SharedString::from(msg.clone()))
            });

        let workspaces = self.tree.workspaces.iter().map(|workspace| {
            let projects = workspace.projects.iter().map(|&project| {
                let node = &self.tree.projects[project];
                let launchers = self.launchers.iter().enumerate().map(|(index, launcher)| {
                    div()
                        .id(("launch", project * 1000 + index))
                        .px_1()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(HOVER_BG)).text_color(rgb(TEXT)))
                        .child(SharedString::from(format!("+ {}", launcher.name)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.launch(project, index, window, cx)
                        }))
                });
                let sessions = self
                    .tabs
                    .iter()
                    .filter(|tab| tab.project == project)
                    .map(|tab| {
                        let id = tab.id;
                        div()
                            .id(("session", id))
                            .pl_4()
                            .pr_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(active == Some(id), |d| d.bg(rgb(ACTIVE_BG)))
                            .hover(|s| s.bg(rgb(HOVER_BG)))
                            .child(SharedString::from(tab.label.clone()))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.activate(id, window, cx)
                            }))
                    });
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pb_1()
                    .child(
                        div()
                            .px_1()
                            .text_color(rgb(TEXT))
                            .child(SharedString::from(node.name.clone())),
                    )
                    .child(div().flex().flex_wrap().gap_1().pl_2().children(launchers))
                    .children(sessions)
            });
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pt_2()
                .child(
                    div()
                        .px_1()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(workspace.name.to_uppercase())),
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
            .gap_1()
            .p_2()
            .overflow_y_scroll()
            .bg(rgb(CHROME_BG))
            .border_r_1()
            .border_color(rgb(BORDER))
            .text_sm()
            .text_color(rgb(MUTED))
            .children(messages)
            .children(workspaces)
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tabs.active_id();
        let tabs = self.tabs.iter().map(|tab| {
            let id = tab.id;
            let is_active = active == Some(id);
            let project = &self.tree.projects[tab.project].name;
            div()
                .id(("tab", id))
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .h_full()
                .pl_3()
                .pr_1()
                .border_r_1()
                .border_color(rgb(BORDER))
                .cursor_pointer()
                .text_color(rgb(if is_active { TEXT } else { MUTED }))
                .when(is_active, |d| d.bg(theme_bg()))
                .when(!is_active, |d| d.hover(|s| s.bg(rgb(HOVER_BG))))
                .child(SharedString::from(format!("{} · {project}", tab.label)))
                .child(
                    div()
                        .id(("close", id))
                        .px_1()
                        .rounded_sm()
                        .hover(|s| s.bg(rgb(ACTIVE_BG)).text_color(rgb(TEXT)))
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
            .w_full()
            .h(px(TAB_BAR_HEIGHT))
            .overflow_x_scroll()
            .bg(rgb(CHROME_BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .text_sm()
            .children(tabs)
    }
}

fn theme_bg() -> Rgba {
    let c = DEFAULT_THEME.background;
    rgb(u32::from_be_bytes([0, c.r, c.g, c.b]))
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let terminal = match self.tabs.active() {
            Some(tab) => div().size_full().child(tab.handle.view.clone()),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(MUTED))
                .child("No open sessions. Start one from the sidebar."),
        };

        div()
            .key_context("Workspace")
            .size_full()
            .flex()
            .flex_row()
            .bg(theme_bg())
            .text_color(rgb(TEXT))
            .capture_key_down(cx.listener(Self::on_key_capture))
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_tab_bar(cx))
                    .child(div().flex_1().min_h(px(0.0)).w_full().child(terminal)),
            )
    }
}
