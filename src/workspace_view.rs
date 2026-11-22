//! Root view: sidebar (Workspace -> Project -> Session), tab bar and the
//! active terminal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::channel::mpsc;
use gpui::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, ElementId, Entity, ExternalPaths, FocusHandle,
    Focusable as _, FontWeight, KeyDownEvent, Rgba, SharedString, Stateful, Subscription, Window,
    WindowBounds, div, linear_color_stop, linear_gradient, point, prelude::*, px, rgb, rgba,
};

mod command_palette;
mod manage;
mod search_bar;
mod task_worktree;
use command_palette::Palette;
use manage::{EditTarget, NameEditor};

use crate::agent_status::{self, AgentStatus};
use crate::board::{self, Status, Task as BoardTask};
use crate::git_remote;
use crate::provider;
use crate::restore::{self, SavedSession, SavedState, SavedWindow};

use crate::config::Settings;
use crate::files::{self, FileTree, Row, path_for_terminal};
use crate::keys::Mods;
use crate::palette::DEFAULT_THEME;
use crate::session::{GridSize, Session};
use crate::shortcuts::{Shortcut, match_shortcut};
use crate::tabs::{SessionId, Tabs};
use crate::theme::{
    ACCENT, BORDER, ERROR, HUD_BG, LOGO_FONT, LOGO_TEXT_SIZE, MUTED, PANEL_BG, SMALL_TEXT_SIZE,
    TEXT, TEXT_SIZE, TITLE, TITLE_TEXT_SIZE, UI_FONT, WORKSPACE, file_color, label, launcher_color,
    priority_color, ui, with_alpha,
};
use crate::tree::{Launcher, ProjectTree, SHELL_LAUNCHER, launchers};
use crate::view::{InputGate, TerminalEvent, TerminalStyle, TerminalView};

const SIDEBAR_WIDTH: f32 = 220.0;
const EXPLORER_WIDTH: f32 = 260.0;
const EXPLORER_BG: u32 = 0x0e0c1f;
/// Indentation per tree level in the explorer.
const TREE_INDENT: f32 = 14.0;
const TAB_BAR_HEIGHT: f32 = 36.0;
const HUD_HEIGHT: f32 = 46.0;
/// Placeholder until the first layout measures the real terminal area.
const INITIAL_GRID: GridSize = GridSize {
    cols: 100,
    rows: 30,
};

/// Zoom changes are saved this long after the last one.
const ZOOM_SAVE_DELAY: Duration = Duration::from_millis(500);
/// Ctrl + wheel distance (pixels) per zoom step.
const WHEEL_PER_ZOOM_STEP: f32 = 40.0;

/// How often agent status files are read.
const STATUS_TICK: Duration = Duration::from_secs(1);

/// How often the board timer checks whether a refresh is due.
const BOARD_TICK: Duration = Duration::from_secs(5);

/// Content of the middle column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MiddleTab {
    Files,
    Quests,
    /// The selected project's index (docs/project/ or README).
    Project,
}

/// Board of one workspace, as last loaded.
#[derive(Default)]
struct BoardState {
    listing: Option<Result<board::Listing, String>>,
    loading: bool,
    /// Latest refresh issued; older results are dropped.
    ticket: u64,
    loaded_at: Option<Instant>,
}

/// An open session: its terminal view and the subscription to its events.
struct OpenSession {
    view: Entity<TerminalView>,
    /// Name of the launcher that started it, saved to restore it.
    launcher: String,
    /// Where its agent reports its status (see `agent_status`).
    status_file: Option<PathBuf>,
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
    /// Project whose launchers are shown and whose workspace the explorer shows.
    selected_project: usize,
    files: FileTree,
    show_files: bool,
    middle_tab: MiddleTab,
    /// Index of each project shown in the PROJECT tab, as last read.
    /// Keyed by project index, which stays valid: removed projects keep
    /// their slot in the tree.
    project_indexes: HashMap<usize, crate::project_index::ProjectIndex>,
    /// Latest index read requested per project: older results are dropped.
    index_tickets: HashMap<usize, u64>,
    /// Boards by workspace index.
    boards: HashMap<usize, BoardState>,
    next_board_ticket: u64,
    /// Task whose details and agent buttons are shown.
    /// `(workspace, task id)`: ids repeat across trackers and repositories.
    selected_task: Option<(usize, String)>,
    show_done: bool,
    _board_timer: gpui::Task<()>,
    /// Where UI changes to workspaces are saved.
    config_path: Option<PathBuf>,
    home: Option<PathBuf>,
    /// Inline name field (new workspace or rename), while open.
    editor: Option<NameEditor>,
    /// Result of the last workspace change that could not be saved.
    notice: Option<String>,
    /// Paused while the name field owns the keyboard.
    input_gate: InputGate,
    /// Current git branch by project, refreshed in the background.
    branches: HashMap<usize, String>,
    branches_loading: bool,
    /// Title last set on the window, to set it only when it changes.
    shown_title: String,
    /// Where the open sessions are saved to restore them next time.
    state_path: Option<PathBuf>,
    /// Last state written, to skip identical writes.
    saved_state: Option<SavedState>,
    /// Current window geometry, saved with the sessions (not on every move).
    window_geometry: Option<SavedWindow>,
    _bounds_subscription: Option<Subscription>,
    /// Command palette, while open.
    palette: Option<Palette>,
    /// Scrollback search bar over the active terminal.
    search: Option<search_bar::SearchBar>,
    /// Start agents on tasks in a git worktree of their own.
    task_worktree: bool,
    /// Ctrl + wheel distance not yet turned into a zoom step.
    wheel_acc: f32,
    /// Pending save of the zoom (replaced, so only the last one runs).
    zoom_save: Option<gpui::Task<()>>,
    /// Project actions (index, suggest improvements...).
    actions: Vec<crate::actions::ProjectAction>,
    /// Editor for Ctrl+click on paths in the terminals, resolved once.
    editor_program: Option<PathBuf>,
    /// Window width at the last frame, to size the side columns.
    viewport_width: f32,
    /// Worktrees being created, so a double click starts only one.
    pending_worktrees: std::collections::HashSet<PathBuf>,
    /// Folder of the per-session status files.
    status_dir: Option<PathBuf>,
    next_status_key: u64,
    /// Last status reported by the agent of each session.
    statuses: HashMap<SessionId, AgentStatus>,
    /// Working directory each shell last reported (OSC 7 / OSC 9;9).
    cwds: HashMap<SessionId, PathBuf>,
    /// Stamp of each session's files list when last read.
    touched_stamps: HashMap<SessionId, crate::touched::Stamp>,
    /// Session whose tab or row got the last click: a double click only
    /// renames when both clicks hit the same session.
    last_click: Option<SessionId>,
    /// Sessions shown side by side, if split.
    split: Option<crate::split::Pair>,
    statuses_loading: bool,
    /// An agent started waiting since the last frame: flash the taskbar if
    /// the window is in the background.
    attention_pending: bool,
    _status_timer: gpui::Task<()>,
    /// Focused while the name field or the palette owns the keyboard: keys
    /// then reach the root capture handler, and no terminal has focus.
    focus: FocusHandle,
}

impl WorkspaceView {
    /// Builds the tree from `settings` and opens a shell in `launch_dir`.
    pub fn new(
        settings: Settings,
        launch_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let restore_sessions = settings.restore_sessions;
        crate::theme::set_zoom(settings.zoom);
        // Agents report their status through files; poll them every second.
        let status_timer = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(STATUS_TICK).await;
                if this
                    .update(cx, |this, cx| this.refresh_statuses(cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut tree = ProjectTree::from_config(&settings.workspaces);
        let initial_project = tree.project_for_dir(&launch_dir);
        // Refreshes the visible board when it is due; stops with the view.
        let board_timer = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BOARD_TICK).await;
                if this
                    .update(cx, |this, cx| {
                        this.refresh_board_if_due(cx);
                        this.refresh_branches(cx);
                        // An agent may be writing the index: keep it fresh.
                        if this.show_files && this.middle_tab == MiddleTab::Project {
                            this.load_project_index(this.selected_project, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut this = Self {
            tree,
            launchers: launchers(&settings.agents),
            actions: settings.actions.clone(),
            style: TerminalStyle::new(
                settings.font_family.as_deref(),
                settings.font_size,
                settings.scanlines,
            ),
            tabs: Tabs::default(),
            config_messages: settings.messages,
            launch_error: None,
            selected_project: initial_project,
            files: FileTree::default(),
            show_files: true,
            middle_tab: MiddleTab::Files,
            project_indexes: HashMap::new(),
            index_tickets: HashMap::new(),
            boards: HashMap::new(),
            next_board_ticket: 0,
            selected_task: None,
            show_done: false,
            _board_timer: board_timer,
            config_path: settings.config_path,
            home: settings.home,
            editor: None,
            notice: None,
            input_gate: InputGate::default(),
            branches: HashMap::new(),
            branches_loading: false,
            shown_title: String::new(),
            state_path: None,
            saved_state: None,
            window_geometry: Some(geometry(window.window_bounds())),
            _bounds_subscription: None,
            palette: None,
            search: None,
            focus: cx.focus_handle(),
            task_worktree: false,
            wheel_acc: 0.0,
            viewport_width: f32::INFINITY,
            zoom_save: None,
            editor_program: crate::provider::resolve(
                settings.editor.as_deref().unwrap_or("code"),
                std::env::var_os("PATH").as_deref(),
                cfg!(windows),
            ),
            pending_worktrees: Default::default(),
            status_dir: None,
            next_status_key: 0,
            statuses: HashMap::new(),
            cwds: HashMap::new(),
            touched_stamps: HashMap::new(),
            last_click: None,
            split: None,
            statuses_loading: false,
            attention_pending: false,
            _status_timer: status_timer,
        };
        if let Some(node) = this.tree.projects.get(initial_project) {
            this.expand_dir(node.path.clone(), cx);
        }
        this.status_dir = this
            .config_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|dir| dir.join(agent_status::DIR_NAME))
            .filter(|dir| std::fs::create_dir_all(dir).is_ok());
        if let Some(dir) = &this.status_dir {
            agent_status::remove_stale(dir);
        }
        let state_path = this
            .config_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|dir| dir.join(restore::FILE_NAME));
        if !(restore_sessions && this.restore_from(state_path.as_deref(), window, cx)) {
            this.launch(initial_project, SHELL_LAUNCHER, window, cx);
        }
        // Track moves and resizes in memory; written with the next save and
        // when the window closes.
        this._bounds_subscription = Some(cx.observe_window_bounds(window, |this, window, _| {
            this.window_geometry = Some(geometry(window.window_bounds()));
        }));
        // Saving starts once the startup sessions are open; with restoring
        // off the file is never touched.
        if restore_sessions {
            this.state_path = state_path;
            this.save_state();
        }
        this.refresh_branches(cx);
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
        self.launch_with(project, launcher, None, window, cx);
    }

    /// Like [`Self::launch`], typing `typed` instead of the launcher's own
    /// command (used to pass a task prompt to an agent). Returns the new
    /// session, or `None` if it could not start.
    fn launch_with(
        &mut self,
        project: usize,
        launcher: usize,
        typed: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<SessionId> {
        let color = launcher_color(launcher);
        let (Some(node), Some(launcher)) = (
            self.tree.projects.get(project),
            self.launchers.get(launcher).cloned(),
        ) else {
            return None;
        };
        cx.notify();
        if !node.path.is_dir() {
            self.launch_error = Some(format!(
                "Project directory not found: {}",
                node.path.display()
            ));
            return None;
        }

        let (events_tx, events_rx) = mpsc::unbounded();
        self.next_status_key += 1;
        let status_file = self
            .status_dir
            .as_ref()
            .map(|dir| agent_status::file_for(dir, self.next_status_key));
        if let Some(file) = &status_file {
            // A recycled process id could match an old file.
            let _ = std::fs::remove_file(file);
            let _ = std::fs::remove_file(crate::touched::files_for(file));
        }
        let env = status_file
            .iter()
            .map(|f| (agent_status::ENV_VAR.to_string(), f.display().to_string()))
            .collect();
        let session = match Session::spawn(INITIAL_GRID, Some(node.path.clone()), env, events_tx) {
            Ok(session) => session,
            Err(err) => {
                self.launch_error = Some(format!("{err:#}"));
                return None;
            }
        };
        if let Some(command) = typed.or(launcher.command.clone()) {
            // The PTY buffers this until the shell reads its input.
            session.write(format!("{command}\r").into_bytes());
        }

        let style = self.style.clone();
        let gate = self.input_gate.clone();
        // Sessions that run a command (agents) get clickable menus.
        let agent = launcher.command.is_some();
        let project_dir = node.path.clone();
        let editor = self.editor_program.clone();
        let view = cx.new(|cx| {
            let mut view = TerminalView::new(session, events_rx, style, gate, cx);
            view.set_agent(agent);
            view.set_links(project_dir, editor);
            view
        });
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        self.drop_session_editor_unless(None);
        self.close_search_quietly(cx);
        let prev = self.tabs.active_id();
        let id = self.tabs.open(
            project,
            &launcher.name,
            OpenSession {
                view,
                launcher: launcher.name.clone(),
                status_file,
                color,
                _subscription: subscription,
            },
        );
        self.launch_error = None;
        self.follow_split(prev);
        self.save_state();
        self.select_project(project, cx);
        self.focus_active(window, cx);
        Some(id)
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
            TerminalEvent::Cwd(dir) => {
                if let Some(id) = self.tabs.find(|s| s.view == *emitter) {
                    self.cwds.insert(id, PathBuf::from(dir));
                    cx.notify();
                }
            }
        }
    }

    /// Ends a session and removes its tab; quits when none are left.
    fn close(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.as_ref().is_some_and(|s| s.session == id) {
            self.close_search_quietly(cx);
        }
        let closed_project = self.tabs.iter().find(|t| t.id == id).map(|t| t.project);
        let Some(session) = self.tabs.close(id) else {
            return;
        };
        if self.split.is_some_and(|(l, r)| l == id || r == id) {
            self.split = None;
        }
        session.view.read(cx).shutdown();
        if let Some(file) = &session.status_file {
            let _ = std::fs::remove_file(file);
            let _ = std::fs::remove_file(crate::touched::files_for(file));
        }
        self.statuses.remove(&id);
        self.cwds.remove(&id);
        self.touched_stamps.remove(&id);
        self.last_click = None;
        if self.renaming_session(id) {
            self.editor = None;
            self.sync_input_gate();
        }
        self.save_state();
        if self.tabs.is_empty() {
            cx.quit();
            return;
        }
        if closed_project == Some(self.selected_project) {
            self.select_active_project(cx);
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn activate(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.iter().any(|t| t.id == id) {
            self.drop_session_editor_unless(Some(id));
        }
        if self.search.as_ref().is_some_and(|s| s.session != id) {
            self.close_search_quietly(cx);
        }
        let prev = self.tabs.active_id();
        if self.tabs.activate(id) {
            self.follow_split(prev);
            // Show it: unfold its workspace for this run (not saved).
            if let Some(project) = self.tabs.active().map(|t| t.project)
                && let Some(w) = self.tree.workspace_of(project)
            {
                self.tree.workspaces[w].collapsed = false;
            }
            self.select_active_project(cx);
            self.focus_active(window, cx);
            self.save_state();
            cx.notify();
        }
    }

    /// Reopens the sessions of the last run; agents get their `resume_args`
    /// so they continue where they were. Returns whether any was opened.
    fn restore_from(
        &mut self,
        path: Option<&Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(path) = path else {
            return false;
        };
        let (sessions, active) = restore::plan(&restore::load(path), &self.tree, &self.launchers);
        // One entry per planned session, `None` when it could not start, so
        // `active` (a position in the plan) still points at the right one.
        let mut opened = Vec::new();
        for (project, launcher, label) in sessions {
            // Typed text comes from the config only, never from the file.
            let typed = self.launchers[launcher].command.as_deref().map(|command| {
                restore::resume_command(command, self.launchers[launcher].resume_args.as_deref())
            });
            let id = self.launch_with(project, launcher, typed, window, cx);
            // The name is only shown; `rename` rejects anything unusable.
            if let (Some(id), Some(label)) = (id, label) {
                self.tabs.rename(id, &label);
            }
            opened.push(id);
        }
        if let Some(id) = active.and_then(|i| opened.get(i).copied().flatten()) {
            let prev = self.tabs.active_id();
            self.tabs.activate(id);
            self.follow_split(prev);
            self.select_active_project(cx);
            self.focus_active(window, cx);
        }
        opened.iter().any(Option::is_some)
    }

    /// Reads the status files of the open sessions on a background thread.
    fn refresh_statuses(&mut self, cx: &mut Context<Self>) {
        if self.statuses_loading {
            return;
        }
        let files: Vec<(SessionId, PathBuf, Option<crate::touched::Stamp>)> = self
            .tabs
            .iter()
            .filter_map(|t| {
                let stamp = self.touched_stamps.get(&t.id).copied();
                t.handle.status_file.clone().map(|f| (t.id, f, stamp))
            })
            .collect();
        if files.is_empty() {
            return;
        }
        self.statuses_loading = true;
        cx.spawn(async move |this, cx| {
            let reads = cx
                .background_spawn(async move {
                    files
                        .into_iter()
                        .map(|(id, f, stamp)| {
                            let touched =
                                crate::touched::read_changed(&crate::touched::files_for(&f), stamp);
                            (id, agent_status::read(&f), touched)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.statuses_loading = false;
                let mut statuses = HashMap::new();
                for (id, read, touched) in reads {
                    // Closed meanwhile: drop. Unreadable for a moment: keep
                    // the last status, so it neither flickers nor re-alerts.
                    let Some(tab) = this.tabs.iter().find(|t| t.id == id) else {
                        continue;
                    };
                    if let Some((stamp, list)) = touched {
                        match stamp {
                            Some(stamp) => this.touched_stamps.insert(id, stamp),
                            None => this.touched_stamps.remove(&id),
                        };
                        tab.handle
                            .view
                            .update(cx, |view, cx| view.set_touched(list, cx));
                    }
                    if let Some(status) = read.or_else(|| this.statuses.get(&id).copied()) {
                        statuses.insert(id, status);
                    }
                }
                let newly_waiting = statuses
                    .iter()
                    .any(|(id, s)| agent_status::newly_waiting(this.statuses.get(id).copied(), *s));
                if newly_waiting {
                    this.attention_pending = true;
                }
                if this.statuses != statuses {
                    this.statuses = statuses;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Changes the zoom by `step`, or back to 100% with `None`; repaints
    /// everything (terminals recompute their grid) and saves it.
    fn change_zoom(&mut self, step: Option<f32>, cx: &mut Context<Self>) {
        let current = crate::theme::zoom();
        let next = crate::theme::set_zoom(step.map_or(1.0, |s| current + s));
        if next == current {
            return;
        }
        for tab in self.tabs.iter() {
            tab.handle.view.update(cx, |_, cx| cx.notify());
        }
        // Save once the user stops zooming: a wheel gesture makes many steps.
        self.zoom_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ZOOM_SAVE_DELAY).await;
            let _ = this.update(cx, |this, _| this.save_zoom());
        }));
        cx.notify();
    }

    /// Width of a side column: grows with the zoom, but never takes more than
    /// `max_share` of the window, so the terminal keeps most of the space.
    fn column_width(&self, base: f32, max_share: f32) -> gpui::Pixels {
        let zoomed = base * crate::theme::zoom();
        let cap = (self.viewport_width * max_share).max(base);
        px(zoomed.min(cap).round())
    }

    /// Writes the current zoom to the config file (pending save done).
    fn save_zoom(&mut self) {
        self.zoom_save = None;
        self.persist_quietly(&crate::config_edit::Edit::SetZoom(crate::theme::zoom()));
    }

    /// Folds or unfolds `workspace` in the sidebar and saves it.
    fn toggle_collapsed(&mut self, workspace: usize, cx: &mut Context<Self>) {
        let Some(node) = self.tree.workspaces.get_mut(workspace) else {
            return;
        };
        node.collapsed = !node.collapsed;
        if !node.implicit {
            let edit = crate::config_edit::Edit::SetCollapsed {
                workspace: node.name.clone(),
                collapsed: node.collapsed,
            };
            self.persist_quietly(&edit);
        }
        cx.notify();
    }

    /// Saves a UI preference; a failure shows in the sidebar like other saves.
    fn persist_quietly(&mut self, edit: &crate::config_edit::Edit) {
        if let Some(path) = &self.config_path
            && let Err(err) = crate::config_edit::save(path, edit, self.home.as_deref())
        {
            self.notice = Some(format!("Not saved to the config file: {err}"));
        }
    }

    /// Activates the next session (after the active one, wrapping) whose
    /// agent waits for the user.
    fn next_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<SessionId> = self.tabs.iter().map(|t| t.id).collect();
        let next = agent_status::next_waiting(&ids, self.tabs.active_id(), |id| {
            self.statuses.get(&id) == Some(&AgentStatus::Waiting)
        });
        if let Some(id) = next {
            self.activate(id, window, cx);
        }
    }

    /// Where a session's shell is, relative to its project, when it is not
    /// the project root.
    fn session_cwd_label(&self, tab: &crate::tabs::Tab<OpenSession>) -> Option<String> {
        let cwd = self.cwds.get(&tab.id)?;
        crate::tree::cwd_label(&self.tree.projects[tab.project].path, cwd)
    }

    /// Click on a session's tab or sidebar row: activate it; a double click
    /// also renames it.
    fn on_session_click(
        &mut self,
        id: SessionId,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Clicks inside the open name field belong to the field.
        if self.renaming_session(id) {
            return;
        }
        let same = self.last_click == Some(id);
        self.last_click = Some(id);
        self.activate(id, window, cx);
        if event.click_count() >= 2 && same {
            self.open_editor(EditTarget::RenameSession(id), window, cx);
        }
    }

    /// A tab dropped on another: it takes that tab's place.
    fn drop_tab(&mut self, dragged: SessionId, target: SessionId, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|t| t.id == target) else {
            return;
        };
        if self.tabs.move_to(dragged, index) {
            self.save_state();
            cx.notify();
        }
    }

    /// Sessions whose agent is waiting for the user.
    fn waiting_count(&self) -> usize {
        self.statuses
            .values()
            .filter(|s| **s == AgentStatus::Waiting)
            .count()
    }

    /// Terminals take input only while neither the name field nor the
    /// palette owns the keyboard.
    pub(crate) fn sync_input_gate(&self) {
        self.input_gate
            .set_paused(self.editor.is_some() || self.palette.is_some() || self.search.is_some());
    }

    /// Writes the open sessions (project and launcher) for the next start,
    /// only when they changed since the last write.
    fn save_state(&mut self) {
        let Some(path) = self.state_path.clone() else {
            return;
        };
        let active = self.tabs.active_id();
        let mut state = SavedState {
            // Never write a value that would not load back (NaN is written as
            // null and would make the whole file unreadable).
            window: self.window_geometry.filter(SavedWindow::is_sane),
            ..SavedState::default()
        };
        for (index, tab) in self.tabs.iter().enumerate() {
            if Some(tab.id) == active {
                state.active = Some(index);
            }
            state.sessions.push(SavedSession {
                project: self.tree.projects[tab.project].path.clone(),
                launcher: tab.handle.launcher.clone(),
                label: tab.renamed.then(|| tab.label.clone()),
            });
        }
        if self.saved_state.as_ref() == Some(&state) {
            return;
        }
        // Best effort: a failure only means starting with one shell (or
        // reopening sessions the user closed), so it is reported but not fatal.
        // Without sessions only the window geometry is kept.
        let result = if state.sessions.is_empty() && state.window.is_none() {
            restore::clear(&path)
        } else {
            restore::save(&path, &state)
        };
        match result {
            Ok(()) => self.saved_state = Some(state),
            Err(err) => {
                self.notice = Some(format!("Could not save the open sessions: {err}"));
            }
        }
    }

    /// Keeps the split pair in step after the active session changed from
    /// `prev`.
    fn follow_split(&mut self, prev: Option<SessionId>) {
        let ids: Vec<SessionId> = self.tabs.iter().map(|t| t.id).collect();
        self.split = crate::split::follow(self.split, prev, self.tabs.active_id(), |id| {
            ids.contains(&id)
        });
    }

    /// Splits the view (active session next to the previous one, or a new
    /// shell in the same project) or goes back to one session.
    fn toggle_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.split.take().is_some() {
            cx.notify();
            return;
        }
        let Some(active) = self.tabs.active() else {
            return;
        };
        let (active, project) = (active.id, active.project);
        // Two terminals need room: at least ~40 columns each.
        if self.viewport_width < MIN_SPLIT_WIDTH * crate::theme::zoom() {
            self.notice = Some("Window too narrow to split".into());
            cx.notify();
            return;
        }
        let order: Vec<SessionId> = self.tabs.iter().map(|t| t.id).collect();
        match crate::split::split_at(&order, active) {
            Some(pair) => {
                self.split = Some(pair);
                cx.notify();
            }
            None => {
                // One session: the new shell opens on the right, active.
                if let Some(new) = self.launch_with(project, SHELL_LAUNCHER, None, window, cx) {
                    self.split = Some((active, new));
                    cx.notify();
                }
            }
        }
    }

    /// One terminal in its frame; the active one glows.
    fn render_pane(
        &self,
        tab: &crate::tabs::Tab<OpenSession>,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let color = tab.handle.color;
        let id = tab.id;
        div()
            .id(("pane", id))
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .p(px(6.0))
            .rounded_lg()
            .border_1()
            .border_color(rgba(with_alpha(color, if active { 0x88 } else { 0x33 })))
            .bg(theme_bg())
            .when(active, |d| d.shadow(glow(color, 0x33, 18.0)))
            // A click in a pane makes it the active session.
            .relative()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.tabs.active_id() != Some(id) {
                        this.activate(id, window, cx);
                    }
                }),
            )
            .child(tab.handle.view.clone())
            .children(self.render_search_bar(id))
    }

    /// Gives keyboard focus to the active terminal and updates the title.
    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        // The name field keeps the keyboard until it closes.
        if let (Some(tab), None, None, None) = (
            self.tabs.active(),
            &self.editor,
            &self.palette,
            &self.search,
        ) {
            let handle = tab.handle.view.read(cx).focus_handle(cx);
            window.focus(&handle);
        }
    }

    /// `session · project (branch) — castorterm`.
    fn window_title(&self) -> String {
        let Some(tab) = self.tabs.active() else {
            return "castorterm".into();
        };
        let name = &self.tree.projects[tab.project].name;
        match self.branches.get(&tab.project) {
            Some(branch) => format!("{} · {name} ({branch}) — castorterm", tab.label),
            None => format!("{} · {name} — castorterm", tab.label),
        }
    }

    /// Selects a project. When the selection changes, its root is expanded
    /// in the explorer; a root the user collapsed stays collapsed otherwise.
    fn select_project(&mut self, project: usize, cx: &mut Context<Self>) {
        let Some(node) = self.tree.projects.get(project) else {
            return;
        };
        let changed = project != self.selected_project;
        self.selected_project = project;
        if changed && !self.files.is_expanded(&node.path) {
            self.expand_dir(node.path.clone(), cx);
        }
        if changed && self.middle_tab == MiddleTab::Project {
            self.load_project_index(project, cx);
        }
        if changed {
            // Another workspace may now be on screen.
            self.refresh_board_if_due(cx);
        }
    }

    fn select_active_project(&mut self, cx: &mut Context<Self>) {
        if let Some(project) = self.tabs.active().map(|t| t.project) {
            self.select_project(project, cx);
        }
    }

    fn expand_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        self.files.expand(&dir);
        self.load_dirs(vec![dir], cx);
    }

    fn toggle_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if self.files.toggle(&dir) {
            self.load_dirs(vec![dir], cx);
        }
        cx.notify();
    }

    /// Lists folders on a background thread so a slow or stalled drive
    /// never freezes the window; the tree shows a loading row meanwhile.
    fn load_dirs(&mut self, dirs: Vec<PathBuf>, cx: &mut Context<Self>) {
        for dir in dirs {
            let ticket = self.files.ticket(&dir);
            cx.spawn(async move |this, cx| {
                let target = dir.clone();
                let listing = cx
                    .background_spawn(async move { files::load(&target) })
                    .await;
                // The view may be gone if the window closed meanwhile.
                let _ = this.update(cx, |this, cx| {
                    this.files.set_listing(&dir, ticket, listing);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Workspace that contains the selected project.
    fn selected_workspace(&self) -> Option<usize> {
        self.tree.workspace_of(self.selected_project)
    }

    fn show_tab(&mut self, tab: MiddleTab, cx: &mut Context<Self>) {
        self.middle_tab = tab;
        self.refresh_board_if_due(cx);
        if tab == MiddleTab::Project {
            self.load_project_index(self.selected_project, cx);
        }
        cx.notify();
    }

    /// The board of `workspace`: its configured one, or the built-in local
    /// board (TASKS.md and OpenSpec changes of its projects).
    fn board_of(&self, workspace: usize) -> Option<crate::config::Board> {
        let node = self.tree.workspaces.get(workspace)?;
        Some(node.board.clone().unwrap_or_else(crate::local_tasks::board))
    }

    /// Reads the index of `project` on a background thread.
    fn load_project_index(&mut self, project: usize, cx: &mut Context<Self>) {
        let Some(node) = self.tree.projects.get(project) else {
            return;
        };
        let root = node.path.clone();
        let ticket = self.index_tickets.get(&project).copied().unwrap_or(0) + 1;
        self.index_tickets.insert(project, ticket);
        cx.spawn(async move |this, cx| {
            let index = cx
                .background_spawn(async move { crate::project_index::load(&root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                // A newer read was asked for meanwhile: this one is stale.
                if this.index_tickets.get(&project) != Some(&ticket) {
                    return;
                }
                if this.project_indexes.get(&project) != Some(&index) {
                    this.project_indexes.insert(project, index);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Reads the git branch of every listed project on a background thread
    /// (agents switch branches, so this runs with the board timer).
    pub(crate) fn refresh_branches(&mut self, cx: &mut Context<Self>) {
        if self.branches_loading {
            return;
        }
        let projects: Vec<(usize, PathBuf)> = self
            .tree
            .workspaces
            .iter()
            .flat_map(|w| w.projects.iter().copied())
            .map(|p| (p, self.tree.projects[p].path.clone()))
            .collect();
        self.branches_loading = true;
        cx.spawn(async move |this, cx| {
            // (project, branch, still a repository): a failed read of a
            // repository (git rewriting HEAD, an antivirus) keeps the last
            // known branch instead of flickering.
            let reads = cx
                .background_spawn(async move {
                    projects
                        .into_iter()
                        // Network and removable drives can be slow; skip them.
                        .filter(|(_, path)| git_remote::on_fixed_drive(path))
                        .map(|(p, path)| {
                            let branch = git_remote::current_branch(&path);
                            let repo = branch.is_some()
                                || std::fs::symlink_metadata(path.join(".git")).is_ok();
                            (p, branch, repo)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.branches_loading = false;
                let mut branches = HashMap::new();
                for (project, branch, repo) in reads {
                    let known = || this.branches.get(&project).cloned();
                    if let Some(branch) = branch.or_else(|| repo.then(known).flatten()) {
                        branches.insert(project, branch);
                    }
                }
                if this.branches != branches {
                    this.branches = branches;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Loads the visible board if it was never loaded or `refresh_secs` passed.
    fn refresh_board_if_due(&mut self, cx: &mut Context<Self>) {
        if !self.show_files || self.middle_tab != MiddleTab::Quests {
            return;
        }
        let Some(workspace) = self.selected_workspace() else {
            return;
        };
        let Some(config) = self.board_of(workspace) else {
            return;
        };
        let every = Duration::from_secs(config.refresh_secs);
        let due = self.boards.get(&workspace).is_none_or(|state| {
            board::refresh_due(state.loading, state.loaded_at.map(|at| at.elapsed()), every)
        });
        if due {
            self.load_board(workspace, cx);
        }
    }

    /// Runs the workspace's provider on a background thread.
    fn load_board(&mut self, workspace: usize, cx: &mut Context<Self>) {
        let Some(node) = self.tree.workspaces.get(workspace) else {
            return;
        };
        let Some(config) = self.board_of(workspace) else {
            return;
        };
        let local_projects: Vec<(String, PathBuf)> = node
            .projects
            .iter()
            .map(|&p| {
                (
                    self.tree.projects[p].name.clone(),
                    self.tree.projects[p].path.clone(),
                )
            })
            .collect();
        let repos = board::workspace_repos(
            node.projects
                .iter()
                .map(|&p| self.tree.projects[p].repo.as_deref()),
        );
        // One provider process per board at a time.
        if self.boards.get(&workspace).is_some_and(|s| s.loading) {
            return;
        }
        self.next_board_ticket += 1;
        let ticket = self.next_board_ticket;
        let state = self.boards.entry(workspace).or_default();
        state.loading = true;
        state.ticket = ticket;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    // The built-in board reads the projects; no command runs.
                    if crate::local_tasks::is_local(&config) {
                        // Never block the board on a network drive.
                        let local: Vec<(String, PathBuf)> = local_projects
                            .into_iter()
                            .filter(|(_, path)| crate::git_remote::on_fixed_drive(path))
                            .collect();
                        return Ok(crate::local_tasks::list(&local));
                    }
                    let path_var = std::env::var_os("PATH");
                    let program = provider::resolve(
                        &config.provider.command,
                        path_var.as_deref(),
                        cfg!(windows),
                    )
                    .ok_or_else(|| {
                        format!("provider '{}' not found in PATH", config.provider.command)
                    })?;
                    let stdout = provider::run_list(
                        &program,
                        &config.provider.args,
                        &config.filter,
                        &repos,
                        provider::TIMEOUT,
                    )?;
                    board::parse_listing(&stdout)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                // By ticket, not by index: removing a workspace while this
                // ran shifts the indices. Tickets are unique.
                if let Some(state) = this.boards.values_mut().find(|s| s.ticket == ticket) {
                    state.listing = Some(result);
                    state.loading = false;
                    state.loaded_at = Some(Instant::now());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Opens a session of `launcher` on `task`, in the task's project when it
    /// matches one of the workspace, otherwise in the selected project.
    fn start_task(
        &mut self,
        task: &BoardTask,
        launcher: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.selected_workspace() else {
            return;
        };
        let node = &self.tree.workspaces[workspace];
        let Some(config) = self.board_of(workspace) else {
            return;
        };
        let Some(agent) = self.launchers.get(launcher) else {
            return;
        };
        let (Some(command), Some(task_args)) = (&agent.command, &agent.task_args) else {
            return;
        };
        let projects = node.projects.iter().map(|&p| {
            let project = &self.tree.projects[p];
            (p, project.name.as_str(), project.repo.as_deref())
        });
        let project = board::match_project(task.project.as_deref(), projects)
            .unwrap_or(self.selected_project);
        let prompt = board::render_prompt(&config.prompt, task);
        let line = board::agent_command(command, task_args, &prompt, cfg!(windows));
        if self.task_worktree {
            self.start_in_worktree(project, launcher, &task.id, line, window, cx);
        } else {
            self.launch_with(project, launcher, Some(line), window, cx);
        }
    }

    /// Runs project action `index` on `project`: a session of its agent
    /// with the action's prompt.
    pub(super) fn run_action(
        &mut self,
        project: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.actions.get(index) else {
            return;
        };
        let Some(launcher) = crate::actions::pick_launcher(action, &self.launchers) else {
            self.notice = Some(match &action.agent {
                Some(agent) => format!("'{}' needs agent '{agent}' with task_args", action.name),
                None => "Add task_args to an agent to run project actions".into(),
            });
            cx.notify();
            return;
        };
        // Removed meanwhile (the palette can outlive a project).
        if self.tree.workspace_of(project).is_none() {
            return;
        }
        // Actions work on the project's checkout, not a task worktree: what
        // they write (docs/project/) belongs to the project.
        let node = &self.tree.projects[project];
        let prompt = crate::actions::render(
            &action.prompt,
            &node.name,
            &node.path.to_string_lossy(),
            node.repo.as_deref(),
        );
        let agent = &self.launchers[launcher];
        let (Some(command), Some(task_args)) = (&agent.command, &agent.task_args) else {
            return;
        };
        let line = board::agent_command(command, task_args, &prompt, cfg!(windows));
        self.launch_with(project, launcher, Some(line), window, cx);
    }

    /// Types the path of `file` into the active session (not executed).
    fn insert_path(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.active() else {
            return;
        };
        // Relative to the session's project when the file is inside it,
        // absolute otherwise (e.g. a file of another project in the workspace).
        let project = &self.tree.projects[tab.project].path;
        if let Some(text) = path_for_terminal(file, project, cfg!(windows)) {
            tab.handle.view.read(cx).type_text(&format!("{text} "));
        }
        self.focus_active(window, cx);
    }

    /// App shortcuts run in the capture phase, before the terminal sees the key.
    fn on_key_capture(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The name field and the palette take every key while open.
        if self.editor.is_some() {
            cx.stop_propagation();
            self.on_editor_key(event, window, cx);
            return;
        }
        if self.search.is_some() {
            cx.stop_propagation();
            let k = &event.keystroke;
            let toggle = k.modifiers.control
                && k.modifiers.shift
                && !k.modifiers.alt
                && k.key.eq_ignore_ascii_case("f");
            if toggle {
                self.close_search(window, cx);
            } else {
                self.on_search_key(event, window, cx);
            }
            return;
        }
        if self.palette.is_some() {
            cx.stop_propagation();
            let k = &event.keystroke;
            let toggle = k.modifiers.control
                && k.modifiers.shift
                && !k.modifiers.alt
                && k.key.eq_ignore_ascii_case("p");
            if toggle {
                self.close_palette(window, cx);
            } else {
                self.on_palette_key(event, window, cx);
            }
            return;
        }
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
                let prev = self.tabs.active_id();
                self.tabs.next();
                self.follow_split(prev);
                self.select_active_project(cx);
                self.focus_active(window, cx);
                self.save_state();
                cx.notify();
            }
            Shortcut::PrevSession => {
                let prev = self.tabs.active_id();
                self.tabs.prev();
                self.follow_split(prev);
                self.select_active_project(cx);
                self.focus_active(window, cx);
                self.save_state();
                cx.notify();
            }
            Shortcut::ToggleFiles => {
                self.show_files = !self.show_files;
                cx.notify();
            }
            Shortcut::AddFolder => {
                if let Some(workspace) = self.selected_workspace() {
                    self.pick_folders(workspace, window, cx);
                }
            }
            Shortcut::Palette => self.open_palette(window, cx),
            Shortcut::NextWaiting => self.next_waiting(window, cx),
            Shortcut::ZoomIn => self.change_zoom(Some(crate::theme::ZOOM_STEP), cx),
            Shortcut::ZoomOut => self.change_zoom(Some(-crate::theme::ZOOM_STEP), cx),
            Shortcut::ZoomReset => self.change_zoom(None, cx),
            Shortcut::MoveSessionLeft | Shortcut::MoveSessionRight => {
                let delta = if shortcut == Shortcut::MoveSessionLeft {
                    -1
                } else {
                    1
                };
                if let Some(id) = self.tabs.active_id()
                    && self.tabs.move_by(id, delta)
                {
                    // Not on every repeat of a held key (the state file is
                    // fsynced); it is saved again on exit and on the next change.
                    if !event.is_held {
                        self.save_state();
                    }
                    cx.notify();
                }
            }
            Shortcut::ToggleSplit => self.toggle_split(window, cx),
            Shortcut::Find => self.open_search(window, cx),
        }
    }

    fn render_hud(&self) -> impl IntoElement {
        let sessions = self.tabs.iter().count();
        let waiting = self.waiting_count();
        let content = div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(ui(HUD_HEIGHT - 2.0))
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
                            .text_size(ui(LOGO_TEXT_SIZE))
                            .text_color(rgb(TITLE))
                            .child("CASTORTERM"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(waiting > 0, |d| {
                        d.child(chip(format!("{waiting} waiting for you"), TITLE))
                    })
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
            .chain(self.notice.as_ref())
            .map(|msg| {
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgba(with_alpha(ERROR, 0x99)))
                    .bg(rgba(with_alpha(ERROR, 0x14)))
                    .text_size(ui(SMALL_TEXT_SIZE))
                    .text_color(rgb(ERROR))
                    .child(SharedString::from(msg.clone()))
            });

        let workspaces = self
            .tree
            .workspaces
            .iter()
            .enumerate()
            .map(|(w_index, workspace)| {
                let projects = workspace.projects.iter().map(|&project| {
                    let node = &self.tree.projects[project];
                    let selected = project == self.selected_project;
                    let group = SharedString::from(format!("project-row-{project}"));
                    let removable = !self.has_sessions(&[project]);
                    let launchers = self.launchers.iter().enumerate().filter(|_| selected).map(
                        |(index, launcher)| {
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
                        },
                    );
                    let actions = self.actions.iter().enumerate().filter(|_| selected).map(
                        |(index, action)| {
                            launcher_button(
                                (SharedString::from(format!("action-{project}")), index),
                                format!("\u{25C6} {}", action.name),
                                ACCENT,
                            )
                            .on_click(cx.listener(
                                move |this, _: &ClickEvent, window, cx| {
                                    this.run_action(project, index, window, cx)
                                },
                            ))
                        },
                    );
                    let sessions =
                        self.tabs
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
                                    .ml_4()
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
                                    .children(self.session_cwd_label(tab).map(|dir| {
                                        div()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .text_size(ui(SMALL_TEXT_SIZE))
                                            .text_color(rgb(MUTED))
                                            .child(SharedString::from(dir))
                                    }))
                                    .children(self.statuses.get(&id).map(|s| status_badge(*s)))
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, window, cx| {
                                            this.on_session_click(id, event, window, cx)
                                        },
                                    ))
                            });
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .pb_2()
                        .child(
                            div()
                                .id(("project", project))
                                .group(group.clone())
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py(px(4.0))
                                .rounded_md()
                                .cursor_pointer()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(if selected { TEXT } else { MUTED }))
                                .when(selected, |d| d.bg(rgba(with_alpha(TEXT, 0x0f))))
                                .when(!selected, |d| {
                                    d.hover(|s| s.bg(rgba(with_alpha(TEXT, 0x0a))))
                                })
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(10.0))
                                        .text_color(rgb(WORKSPACE))
                                        .child(if selected { "›" } else { "" }),
                                )
                                .child(
                                    // The name wins over the branch when space is short.
                                    div()
                                        .flex_shrink_0()
                                        .max_w(ui(130.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(SharedString::from(node.name.clone())),
                                )
                                // Always present: it also pushes the actions right.
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_size(ui(SMALL_TEXT_SIZE))
                                        .text_color(rgba(with_alpha(WORKSPACE, 0xaa)))
                                        .children(
                                            self.branches
                                                .get(&project)
                                                .map(|b| SharedString::from(b.clone())),
                                        ),
                                )
                                .when(removable, |d| {
                                    d.child(
                                        Self::row_action(
                                            ("remove-project", project),
                                            group.clone(),
                                            "×",
                                            ERROR,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                                cx.stop_propagation();
                                                this.remove_project(w_index, project, cx);
                                            }),
                                        ),
                                    )
                                })
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.select_project(project, cx);
                                    cx.notify();
                                })),
                        )
                        .when(selected, |d| {
                            d.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_1()
                                    .pl_5()
                                    .pr_1()
                                    .pb_1()
                                    .children(launchers),
                            )
                            // Actions on their own row, apart from new sessions.
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_1()
                                    .pl_5()
                                    .pr_1()
                                    .pb_1()
                                    .children(actions),
                            )
                        })
                        .children(sessions)
                });
                let group = SharedString::from(format!("workspace-row-{w_index}"));
                let removable = !self.has_sessions(&workspace.projects);
                let collapsed = workspace.collapsed;
                let open_sessions = self
                    .tabs
                    .iter()
                    .filter(|t| workspace.projects.contains(&t.project))
                    .count();
                let waiting_inside = self.tabs.iter().any(|t| {
                    workspace.projects.contains(&t.project)
                        && self.statuses.get(&t.id) == Some(&AgentStatus::Waiting)
                });
                let header = if self.renaming(w_index) {
                    self.render_editor()
                        .unwrap_or_else(|| div().into_any_element())
                } else {
                    div()
                        .id(("workspace-header", w_index))
                        .group(group.clone())
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py(px(2.0))
                        .text_size(ui(SMALL_TEXT_SIZE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(WORKSPACE))
                        .child(dot(WORKSPACE, 6.0, true))
                        .child(
                            // Click the name to fold or unfold the workspace.
                            div()
                                .id(("workspace-name", w_index))
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .items_center()
                                .gap_1()
                                .cursor_pointer()
                                .hover(|s| s.text_color(rgb(TEXT)))
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(rgb(MUTED))
                                        .child(chevron(!collapsed)),
                                )
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(SharedString::from(label(&workspace.name))),
                                )
                                // Folded: how many sessions run inside, and
                                // whether an agent there waits for the user.
                                .when(collapsed && open_sessions > 0, |d| {
                                    d.child(
                                        div()
                                            .flex_none()
                                            .text_color(rgb(MUTED))
                                            .font_weight(FontWeight::NORMAL)
                                            .child(SharedString::from(format!(
                                                "· {open_sessions}"
                                            ))),
                                    )
                                })
                                .when(collapsed && waiting_inside, |d| {
                                    d.child(status_badge(AgentStatus::Waiting))
                                })
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.toggle_collapsed(w_index, cx)
                                })),
                        )
                        .child(
                            Self::row_action(("add-folder", w_index), group.clone(), "+", ACCENT)
                                .opacity(1.0)
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.pick_folders(w_index, window, cx)
                                })),
                        )
                        .child(
                            Self::row_action(
                                ("rename-workspace", w_index),
                                group.clone(),
                                "✎",
                                ACCENT,
                            )
                            .on_click(cx.listener(
                                move |this, _: &ClickEvent, window, cx| {
                                    this.open_editor(EditTarget::Rename(w_index), window, cx)
                                },
                            )),
                        )
                        .when(removable, |d| {
                            d.child(
                                Self::row_action(
                                    ("remove-workspace", w_index),
                                    group.clone(),
                                    "×",
                                    ERROR,
                                )
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| {
                                        this.remove_workspace(w_index, cx)
                                    },
                                )),
                            )
                        })
                        .into_any_element()
                };
                let empty = workspace.projects.is_empty();
                div()
                    .id(("workspace", w_index))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .pt_2()
                    .pb_1()
                    .rounded_md()
                    .drag_over::<ExternalPaths>(|style, _, _, _| {
                        style.bg(rgba(with_alpha(WORKSPACE, 0x18)))
                    })
                    .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                        this.add_folders(w_index, paths.paths().to_vec(), cx)
                    }))
                    .child(header)
                    .when(empty && !collapsed, |d| {
                        d.child(self.render_empty_workspace(w_index, cx))
                    })
                    .when(!collapsed, |d| d.children(projects))
            });
        let new_workspace = self
            .editor
            .as_ref()
            .filter(|e| e.target == EditTarget::NewWorkspace)
            .and_then(|_| self.render_editor());

        div()
            .id("sidebar")
            .flex_none()
            .w(self.column_width(SIDEBAR_WIDTH, 0.24))
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .overflow_y_scroll()
            .bg(rgb(PANEL_BG))
            .border_r_1()
            .border_color(rgb(BORDER))
            .text_size(ui(TEXT_SIZE))
            .text_color(rgb(MUTED))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .text_size(ui(SMALL_TEXT_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("QUEST LOG")
                    .child(
                        div()
                            .id("new-workspace")
                            .px_2()
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(ACCENT))
                            .hover(|s| s.bg(rgba(with_alpha(ACCENT, 0x22))))
                            .child("+ Workspace")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                if this.editor.is_some() {
                                    this.close_editor(window, cx);
                                } else {
                                    this.open_editor(EditTarget::NewWorkspace, window, cx);
                                }
                            })),
                    ),
            )
            .children(messages)
            .children(workspaces)
            .children(new_workspace)
            // Folders dropped outside any workspace go to the selected one.
            .child(
                div()
                    .id("sidebar-drop")
                    .flex_1()
                    .min_h(ui(40.0))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                        if let Some(workspace) = this.selected_workspace() {
                            this.add_folders(workspace, paths.paths().to_vec(), cx);
                        }
                    })),
            )
    }

    fn render_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self
            .tree
            .workspaces
            .iter()
            .find(|w| w.projects.contains(&self.selected_project));

        let tab = |id: &'static str, text: &'static str, tab: MiddleTab| {
            let active = self.middle_tab == tab;
            div()
                .id(id)
                .px_2()
                .py(px(2.0))
                .rounded_md()
                .cursor_pointer()
                .text_color(rgb(if active { TEXT } else { MUTED }))
                .when(active, |d| d.bg(rgba(with_alpha(ACCENT, 0x22))))
                .when(!active, |d| d.hover(|s| s.text_color(rgb(TEXT))))
                .child(text)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.show_tab(tab, cx)))
        };

        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_2()
            .text_size(ui(SMALL_TEXT_SIZE))
            .font_weight(FontWeight::SEMIBOLD)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(tab("tab-files", "FILES", MiddleTab::Files))
                    .child(tab("tab-quests", "QUESTS", MiddleTab::Quests))
                    .child(tab("tab-project", "PROJECT", MiddleTab::Project)),
            )
            .child(
                div()
                    .id("refresh-middle")
                    .px_2()
                    .rounded_md()
                    .text_color(rgb(MUTED))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(with_alpha(ACCENT, 0x22))).text_color(rgb(ACCENT)))
                    .child("Refresh")
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| match this.middle_tab {
                            MiddleTab::Files => {
                                let dirs = this.files.expanded_dirs();
                                this.load_dirs(dirs, cx);
                            }
                            MiddleTab::Quests => {
                                if let Some(workspace) = this.selected_workspace() {
                                    this.load_board(workspace, cx);
                                }
                            }
                            MiddleTab::Project => {
                                this.load_project_index(this.selected_project, cx);
                            }
                        }),
                    ),
            );

        let workspace_label = workspace.map(|w| {
            div()
                .px_3()
                .pb_1()
                .text_size(ui(SMALL_TEXT_SIZE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(WORKSPACE))
                .child(SharedString::from(label(&w.name)))
        });

        let rows = match self.middle_tab {
            MiddleTab::Quests => self.render_quest_rows(cx),
            MiddleTab::Project => self.render_project_rows(cx),
            MiddleTab::Files => self.render_file_rows(cx),
        };

        div()
            .flex_none()
            .w(self.column_width(EXPLORER_WIDTH, 0.28))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(EXPLORER_BG))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(header)
            .children(workspace_label)
            .child(
                div()
                    .id("explorer-list")
                    .flex_1()
                    .min_h(px(0.0))
                    .pb_2()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }

    /// The PROJECT tab: the selected project's index (or README), when it
    /// was written, and its actions.
    fn render_project_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        use crate::project_index::{Block, Source};
        let project = self.selected_project;
        let Some(node) = self.tree.projects.get(project) else {
            return Vec::new();
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        let index = self.project_indexes.get(&project);
        let status = match index {
            None => "Reading...".to_string(),
            Some(i) => match (i.source, i.updated) {
                (Source::Index, Some(when)) => format!(
                    "Indexed {} \u{b7} {}",
                    crate::project_index::ago(when, std::time::SystemTime::now()),
                    crate::project_index::INDEX_DIR
                ),
                (Source::Index, None) => {
                    format!("Indexed \u{b7} {}", crate::project_index::INDEX_DIR)
                }
                (Source::Readme, _) => "Not indexed yet: showing README.md".into(),
                (Source::Nothing, _) => "Not indexed yet".into(),
            },
        };
        rows.push(
            div()
                .px_3()
                .pt_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .child(SharedString::from(node.name.clone())),
                )
                .child(
                    div()
                        .text_size(ui(SMALL_TEXT_SIZE))
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(status)),
                )
                .child(div().flex().flex_wrap().gap_1().pt_1().children(
                    self.actions.iter().enumerate().map(|(i, action)| {
                        launcher_button(
                            (SharedString::from(format!("project-action-{project}")), i),
                            format!("\u{25C6} {}", action.name),
                            ACCENT,
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, window, cx| {
                                this.run_action(project, i, window, cx)
                            },
                        ))
                    }),
                ))
                .into_any_element(),
        );
        let Some(index) = index else {
            return rows;
        };
        for doc in &index.docs {
            if index.source == Source::Index {
                rows.push(
                    div()
                        .px_3()
                        .pt_3()
                        .text_size(ui(SMALL_TEXT_SIZE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(WORKSPACE))
                        .child(SharedString::from(doc.name.to_uppercase()))
                        .into_any_element(),
                );
            }
            for block in doc.blocks.iter().cloned() {
                let row = div().px_3();
                let row = match block {
                    Block::Heading(level, text) => row
                        .pt_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if level <= 2 { TEXT } else { MUTED }))
                        .child(SharedString::from(text)),
                    Block::Bullet(marker, text) => row
                        .flex()
                        .gap_1()
                        .text_color(rgb(TEXT))
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(ACCENT))
                                .child(SharedString::from(marker)),
                        )
                        .child(div().min_w(px(0.0)).child(SharedString::from(text))),
                    Block::Code(text) => row
                        .font_family(self.style.family.clone())
                        .text_size(ui(SMALL_TEXT_SIZE))
                        .text_color(rgb(MUTED))
                        .bg(rgba(with_alpha(TEXT, 0x08)))
                        .child(SharedString::from(text)),
                    Block::Text(text) => row.text_color(rgb(TEXT)).child(SharedString::from(text)),
                    Block::Gap => row.h(px(6.0)),
                };
                rows.push(row.text_size(ui(SMALL_TEXT_SIZE)).into_any_element());
            }
        }
        rows
    }

    fn render_file_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let workspace = self
            .tree
            .workspaces
            .iter()
            .find(|w| w.projects.contains(&self.selected_project));
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(workspace) = workspace {
            for &project in &workspace.projects {
                let node = &self.tree.projects[project];
                let root = node.path.clone();
                let expanded = self.files.is_expanded(&root);
                let is_selected = project == self.selected_project;
                rows.push(
                    tree_row(row_id("root", &root), 0, chevron(expanded))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if is_selected { TEXT } else { MUTED }))
                        .child(SharedString::from(node.name.clone()))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.toggle_dir(root.clone(), cx);
                        }))
                        .into_any_element(),
                );
                for row in self.files.rows(&node.path) {
                    rows.push(
                        match row {
                            Row::Entry {
                                depth,
                                entry,
                                expanded,
                            } => {
                                let path = entry.path.clone();
                                let base = tree_row(
                                    row_id("entry", &path),
                                    depth + 1,
                                    if entry.is_dir { chevron(expanded) } else { "" },
                                );
                                if entry.is_dir {
                                    base.text_color(rgb(TEXT))
                                        .child(SharedString::from(entry.name))
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _, cx| {
                                                this.toggle_dir(path.clone(), cx);
                                            },
                                        ))
                                } else {
                                    base.text_color(rgb(file_color(&entry.name)))
                                        .child(SharedString::from(entry.name))
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, window, cx| {
                                                this.insert_path(&path, window, cx)
                                            },
                                        ))
                                }
                            }
                            Row::Loading { depth, dir } => {
                                tree_row(row_id("loading", &dir), depth + 1, "")
                                    .text_color(rgb(MUTED))
                                    .child("Loading…")
                            }
                            Row::Error {
                                depth,
                                dir,
                                message,
                            } => tree_row(row_id("error", &dir), depth + 1, "")
                                .text_color(rgb(ERROR))
                                .child(SharedString::from(format!("Cannot read: {message}"))),
                            Row::More { depth, dir, count } => {
                                tree_row(row_id("more", &dir), depth + 1, "")
                                    .text_color(rgb(MUTED))
                                    .child(SharedString::from(format!("… {count} more")))
                            }
                        }
                        .into_any_element(),
                    );
                }
            }
        }

        rows
    }

    fn render_quest_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let note = |text: String, color: u32| {
            div()
                .px_3()
                .py_1()
                .text_size(ui(SMALL_TEXT_SIZE))
                .text_color(rgb(color))
                .child(SharedString::from(text))
                .into_any_element()
        };
        let Some(workspace) = self.selected_workspace() else {
            return Vec::new();
        };
        let Some(config) = self.board_of(workspace) else {
            return Vec::new();
        };
        let config = &config;
        let mut rows = Vec::new();
        let state = self.boards.get(&workspace);
        if state.is_none_or(|s| s.loading) {
            rows.push(note(
                if crate::local_tasks::is_local(config) {
                    "Reading TASKS.md and OpenSpec changes…".to_string()
                } else {
                    format!("Loading from {}…", config.provider.name)
                },
                MUTED,
            ));
        }
        let listing = match state.and_then(|s| s.listing.as_ref()) {
            None => return rows,
            Some(Err(message)) => {
                rows.push(note(message.clone(), ERROR));
                return rows;
            }
            Some(Ok(listing)) => listing,
        };
        if listing.omitted > 0 {
            rows.push(note(
                format!(
                    "Showing the first {} tasks; {} more not shown",
                    board::MAX_TASKS,
                    listing.omitted
                ),
                MUTED,
            ));
        }
        if listing.skipped > 0 {
            rows.push(note(
                format!(
                    "{} task(s) skipped: invalid data from the provider",
                    listing.skipped
                ),
                ERROR,
            ));
        }
        if listing.tasks.is_empty() {
            let hint = if crate::local_tasks::is_local(config) {
                "No tasks yet. This board reads TASKS.md (\"- [ ] task\" items, sections such as \"## In progress\") and OpenSpec changes in the workspace's projects; or set a provider in config.toml."
            } else {
                "No tasks."
            };
            rows.push(note(hint.into(), MUTED));
        }

        for (status, tasks) in board::group(&listing.tasks) {
            let collapsible = status == Status::Done;
            let collapsed = collapsible && !self.show_done;
            let color = status_color(status);
            rows.push(
                div()
                    .id(SharedString::from(format!(
                        "quest-group-{}",
                        status.label()
                    )))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .pt_2()
                    .pb_1()
                    .text_size(ui(SMALL_TEXT_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .when(collapsible, |d| {
                        d.cursor_pointer()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.show_done = !this.show_done;
                                cx.notify();
                            }))
                    })
                    .child(dot(color, 6.0, status == Status::InProgress))
                    .child(SharedString::from(format!(
                        "{} {}{}",
                        label(status.label()),
                        tasks.len(),
                        if collapsible {
                            if collapsed { " ▸" } else { " ▾" }
                        } else {
                            ""
                        }
                    )))
                    .into_any_element(),
            );
            if collapsed {
                continue;
            }
            for task in tasks {
                rows.push(self.render_task(task, workspace, cx));
            }
        }
        rows
    }

    fn render_task(
        &self,
        task: &BoardTask,
        workspace: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self
            .selected_task
            .as_ref()
            .is_some_and(|(w, id)| *w == workspace && *id == task.id);
        let key = (workspace, task.id.clone());
        let mut row = div()
            .flex()
            .flex_col()
            .mx_2()
            .rounded_md()
            .when(selected, |d| d.bg(rgba(with_alpha(TEXT, 0x0f))))
            .child(
                div()
                    .id(SharedString::from(format!("quest:{}", task.id)))
                    .flex()
                    .items_start()
                    .gap_2()
                    .px_2()
                    .py(px(4.0))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(with_alpha(TEXT, 0x0d))))
                    .children(task.priority.as_ref().map(|p| {
                        let color = priority_color(p);
                        div()
                            .flex_none()
                            .px_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgba(with_alpha(color, 0x80)))
                            .text_size(ui(SMALL_TEXT_SIZE))
                            .text_color(rgb(color))
                            .child(SharedString::from(p.clone()))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .text_size(ui(SMALL_TEXT_SIZE))
                                    .text_color(rgb(MUTED))
                                    .child(SharedString::from(task.id.clone()))
                                    .children(self.task_project_label(task, workspace).map(
                                        |name| {
                                            div()
                                                .min_w(px(0.0))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_color(rgb(WORKSPACE))
                                                .child(SharedString::from(name))
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .text_color(rgb(TEXT))
                                    .child(SharedString::from(task.title.clone())),
                            ),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.selected_task = match this.selected_task.as_ref() == Some(&key) {
                            true => None,
                            false => Some(key.clone()),
                        };
                        cx.notify();
                    })),
            );
        if selected {
            row = row.child(self.render_task_details(task, workspace, cx));
        }
        row.into_any_element()
    }

    /// Project shown on a task row: only in workspaces with several
    /// projects, the matched project's name or the provider's raw value.
    fn task_project_label(&self, task: &BoardTask, workspace: usize) -> Option<String> {
        let node = &self.tree.workspaces[workspace];
        if node.projects.len() < 2 {
            return None;
        }
        let projects = node.projects.iter().map(|&p| {
            let project = &self.tree.projects[p];
            (p, project.name.as_str(), project.repo.as_deref())
        });
        match board::match_project(task.project.as_deref(), projects) {
            Some(p) => Some(self.tree.projects[p].name.clone()),
            None => task.project.clone(),
        }
    }

    fn render_task_details(
        &self,
        task: &BoardTask,
        workspace: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let node = &self.tree.workspaces[workspace];
        let projects = node.projects.iter().map(|&p| {
            let project = &self.tree.projects[p];
            (p, project.name.as_str(), project.repo.as_deref())
        });
        let project = board::match_project(task.project.as_deref(), projects);
        let project_text = match project {
            Some(p) => format!("Project: {}", self.tree.projects[p].name),
            None => format!(
                "Project: {} (selected)",
                self.tree.projects[self.selected_project].name
            ),
        };
        let buttons = self
            .launchers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.task_args.is_some())
            .map(|(index, launcher)| {
                let task = task.clone();
                launcher_button(
                    (SharedString::from(format!("start:{}", task.id)), index),
                    format!("+ {}", launcher.name),
                    launcher_color(index),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.start_task(&task, index, window, cx)
                }))
            })
            .collect::<Vec<_>>();
        let has_buttons = !buttons.is_empty();
        // Show exactly what an agent will receive before it is started: the
        // title comes from the tracker and is untrusted input for the agent.
        let prompt = self
            .board_of(workspace)
            .map(|config| board::final_prompt(&config.prompt, task, cfg!(windows)));
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .pb_2()
            .text_size(ui(SMALL_TEXT_SIZE))
            .text_color(rgb(MUTED))
            .child(SharedString::from(project_text))
            .children(prompt.filter(|_| has_buttons).map(|prompt| {
                div()
                    .text_color(rgb(TEXT))
                    .child(SharedString::from(format!("Prompt: {prompt}")))
            }))
            .children(task.url.clone().map(|url| {
                div()
                    .id(SharedString::from(format!("quest-link:{}", task.id)))
                    .text_color(rgb(ACCENT))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .child("Open link")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)))
            }))
            .when(has_buttons, |d| {
                let on = self.task_worktree;
                let branch = format!("task/{}", crate::worktree::slug(&task.id));
                d.child(
                    div()
                        .id(SharedString::from(format!("worktree-toggle:{}", task.id)))
                        .flex()
                        .items_center()
                        .gap_2()
                        .pt_1()
                        .cursor_pointer()
                        .text_color(rgb(if on { ACCENT } else { MUTED }))
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .child(if on { "[x]" } else { "[ ]" })
                        .child(SharedString::from(format!("In a new worktree ({branch})")))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.task_worktree = !this.task_worktree;
                            cx.notify();
                        })),
                )
            })
            .child(div().flex().flex_wrap().gap_1().pt_1().children(buttons))
            .when(!has_buttons, |d| {
                d.child("Add task_args to an agent to start it on tasks.")
            })
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
                .child(if self.renaming_session(id) {
                    self.render_editor()
                        .unwrap_or_else(|| div().into_any_element())
                } else {
                    div()
                        .text_color(rgb(if is_active { TEXT } else { MUTED }))
                        .child(SharedString::from(tab.label.clone()))
                        .into_any_element()
                })
                .children(self.statuses.get(&id).map(|s| status_badge(*s)))
                .child(div().text_color(rgb(MUTED)).child(SharedString::from(
                    match self.session_cwd_label(tab) {
                        Some(dir) => format!("· {project} · {dir}"),
                        None => format!("· {project}"),
                    },
                )))
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
                        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            // A double click closes one session, not its neighbour too.
                            if event.click_count() >= 2 {
                                return;
                            }
                            this.close(id, window, cx);
                        })),
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.on_session_click(id, event, window, cx)
                }))
                // Drag a tab onto another to take its place (not while
                // renaming it: pressing in the field would drag the tab).
                .when(!self.renaming_session(id), |d| {
                    d.on_drag(
                        DraggedTab {
                            id,
                            label: tab.label.clone().into(),
                            color,
                        },
                        |dragged, _, _, cx| cx.new(|_| dragged.clone()),
                    )
                })
                .drag_over::<DraggedTab>(move |style, dragged, _, _| {
                    if dragged.id == id {
                        style
                    } else {
                        style.bg(rgba(with_alpha(color, 0x33)))
                    }
                })
                .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| {
                    this.drop_tab(dragged.id, id, cx)
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
            .h(ui(TAB_BAR_HEIGHT))
            .px_2()
            .pt_1()
            .overflow_x_scroll()
            .bg(rgb(PANEL_BG))
            .text_size(ui(TEXT_SIZE))
            .children(tabs)
    }
}

/// Colour of a board group.
fn status_color(status: Status) -> u32 {
    match status {
        Status::InProgress => ACCENT,
        Status::Review => TITLE,
        Status::Ready => 0x39ff8e,
        Status::Backlog | Status::Done => MUTED,
    }
}

/// Folder state marker in the explorer.
fn chevron(expanded: bool) -> &'static str {
    if expanded { "▾" } else { "▸" }
}

/// Element id for an explorer row, stable across expand/collapse because
/// it is keyed by path rather than by position.
fn row_id(kind: &str, path: &Path) -> ElementId {
    SharedString::from(format!("tree-{kind}:{}", path.display())).into()
}

/// One explorer line, indented by `depth`, with a fixed-width marker column.
fn tree_row(id: ElementId, depth: usize, marker: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1()
        .h(ui(22.0))
        .pl(ui(8.0 + depth as f32 * TREE_INDENT))
        .pr_2()
        .whitespace_nowrap()
        .overflow_hidden()
        .cursor_pointer()
        .hover(|s| s.bg(rgba(with_alpha(TEXT, 0x0d))))
        .child(
            div()
                .flex_none()
                .w(ui(12.0))
                .text_color(rgb(MUTED))
                .child(marker),
        )
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
        .text_size(ui(SMALL_TEXT_SIZE))
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
        .text_size(ui(SMALL_TEXT_SIZE))
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

/// Whole zoom steps in `distance` (pixels) and what is left over.
fn wheel_steps(distance: f32, per_step: f32) -> (i32, f32) {
    if !distance.is_finite() {
        return (0, 0.0);
    }
    let steps = (distance / per_step).trunc();
    (steps as i32, distance - steps * per_step)
}

/// Saved form of a window's bounds; full screen is restored as maximized.
fn geometry(bounds: WindowBounds) -> SavedWindow {
    let rect = bounds.get_bounds();
    SavedWindow {
        x: rect.origin.x.into(),
        y: rect.origin.y.into(),
        width: rect.size.width.into(),
        height: rect.size.height.into(),
        maximized: !matches!(bounds, WindowBounds::Windowed(_)),
    }
}

/// Small label with what an agent is doing; waiting stands out.
fn status_badge(status: AgentStatus) -> Div {
    let (text, color) = match status {
        AgentStatus::Working => ("working", ACCENT),
        AgentStatus::Waiting => ("waiting", TITLE),
        AgentStatus::Done => ("done", MUTED),
    };
    div()
        .flex_none()
        .px_1()
        .rounded_sm()
        .text_size(ui(SMALL_TEXT_SIZE))
        .text_color(rgb(color))
        .border_1()
        .border_color(rgba(with_alpha(color, 0x88)))
        .when(status == AgentStatus::Waiting, |d| {
            d.bg(rgba(with_alpha(color, 0x22)))
                .shadow(glow(color, 0x66, 6.0))
        })
        .child(text)
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

fn theme_bg() -> Rgba {
    let c = DEFAULT_THEME.background;
    rgb(u32::from_be_bytes([0, c.r, c.g, c.b]))
}

impl Drop for WorkspaceView {
    /// Closing the window does not close the sessions one by one: remove
    /// their status files here.
    fn drop(&mut self) {
        // The last geometry (moves and resizes are not written as they happen).
        self.save_state();
        if self.zoom_save.is_some() {
            self.save_zoom();
        }
        for tab in self.tabs.iter() {
            if let Some(file) = &tab.handle.status_file {
                let _ = std::fs::remove_file(file);
                let _ = std::fs::remove_file(crate::touched::files_for(file));
            }
        }
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.attention_pending) && !window.is_window_active() {
            crate::attention::request(window);
        }
        self.viewport_width = window.viewport_size().width.into();
        let title = self.window_title();
        if title != self.shown_title {
            window.set_window_title(&title);
            self.shown_title = title;
        }
        let active_id = self.tabs.active_id();
        let pair = self.split.and_then(|(l, r)| {
            let find = |id| self.tabs.iter().find(|t| t.id == id);
            Some((find(l)?, find(r)?))
        });
        let terminal = match (self.tabs.active(), pair) {
            (Some(_), Some((left, right))) => div()
                .size_full()
                .p_2()
                .flex()
                .flex_row()
                .gap_2()
                .child(self.render_pane(left, active_id == Some(left.id), cx))
                .child(self.render_pane(right, active_id == Some(right.id), cx)),
            (Some(tab), None) => div()
                .size_full()
                .p_2()
                .flex()
                .child(self.render_pane(tab, true, cx)),
            (None, _) => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .text_size(ui(TITLE_TEXT_SIZE))
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
            .text_size(ui(TEXT_SIZE))
            .text_color(rgb(TEXT))
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::on_key_capture))
            // Ctrl + wheel zooms, anywhere in the window.
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                if !event.modifiers.control {
                    this.wheel_acc = 0.0;
                    return;
                }
                // Touchpads send many small events per gesture: one step per
                // WHEEL_PER_ZOOM_STEP pixels, not one per event.
                if matches!(event.touch_phase, gpui::TouchPhase::Started) {
                    this.wheel_acc = 0.0;
                }
                let dy: f32 = event.delta.pixel_delta(px(16.0)).y.into();
                let (steps, rest) = wheel_steps(this.wheel_acc + dy, WHEEL_PER_ZOOM_STEP);
                this.wheel_acc = rest;
                if steps != 0 {
                    this.change_zoom(Some(crate::theme::ZOOM_STEP * steps as f32), cx);
                }
            }))
            .relative()
            .child(self.render_hud())
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .flex()
                    .flex_row()
                    .child(self.render_sidebar(cx))
                    .when(self.show_files, |d| d.child(self.render_explorer(cx)))
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
            .children(self.render_palette(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{geometry, plural, wheel_steps};

    #[test]
    fn wheel_distance_becomes_whole_steps() {
        assert_eq!(wheel_steps(10.0, 40.0), (0, 10.0));
        assert_eq!(wheel_steps(95.0, 40.0), (2, 15.0));
        assert_eq!(wheel_steps(-50.0, 40.0), (-1, -10.0));
        assert_eq!(wheel_steps(f32::NAN, 40.0), (0, 0.0));
    }
    use gpui::{Bounds, WindowBounds, point, px, size};

    #[test]
    fn window_geometry_from_bounds() {
        let rect = Bounds::new(point(px(10.0), px(20.0)), size(px(900.0), px(600.0)));
        let windowed = geometry(WindowBounds::Windowed(rect));
        assert_eq!(
            (windowed.x, windowed.y, windowed.width, windowed.height),
            (10.0, 20.0, 900.0, 600.0)
        );
        assert!(!windowed.maximized);
        // Maximized and full screen keep the restore bounds and reopen maximized.
        for bounds in [
            WindowBounds::Maximized(rect),
            WindowBounds::Fullscreen(rect),
        ] {
            let g = geometry(bounds);
            assert!(g.maximized);
            assert_eq!((g.width, g.height), (900.0, 600.0));
        }
    }

    #[test]
    fn plural_forms() {
        assert_eq!(plural(1, "session", "sessions"), "session");
        assert_eq!(plural(0, "session", "sessions"), "sessions");
        assert_eq!(plural(3, "session", "sessions"), "sessions");
    }
}

/// Narrowest window (at 100 % zoom) that is split in two terminals.
const MIN_SPLIT_WIDTH: f32 = 1100.0;

/// A tab being dragged; also its drag preview.
#[derive(Clone)]
struct DraggedTab {
    id: SessionId,
    label: SharedString,
    color: u32,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(self.color))
            .bg(theme_bg())
            .text_color(rgb(TEXT))
            .child(self.label.clone())
    }
}
