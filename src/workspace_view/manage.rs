//! Workspace management from the sidebar: create, rename and remove
//! workspaces, add and remove project folders, and save it all to the
//! config file.

use std::path::PathBuf;

use gpui::{
    AnyElement, ClickEvent, Context, Div, ElementId, KeyDownEvent, PathPromptOptions, SharedString,
    Stateful, Window, div, prelude::*, px, rgb, rgba,
};

use super::WorkspaceView;
use crate::config_edit::{self, Edit, ProjectEntry};
use crate::git_remote::detect_repo;
use crate::keys::Mods;
use crate::name_input::{InputAction, NameInput};
use crate::theme::{ACCENT, ERROR, MUTED, SMALL_TEXT_SIZE, TEXT, WORKSPACE, with_alpha};
use crate::tree::project_node;

/// What the inline name field is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditTarget {
    NewWorkspace,
    Rename(usize),
}

/// The inline name field, while it is open.
pub(super) struct NameEditor {
    pub target: EditTarget,
    pub input: NameInput,
    /// Why the last Enter was refused (duplicate name...).
    pub error: Option<String>,
}

impl WorkspaceView {
    pub(super) fn open_editor(
        &mut self,
        target: EditTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match target {
            EditTarget::NewWorkspace => "",
            EditTarget::Rename(w) => self.tree.workspaces[w].name.as_str(),
        };
        self.editor = Some(NameEditor {
            target,
            input: NameInput::new(text),
            error: None,
        });
        // Keys still reach the root capture handler, which feeds the field;
        // the gate makes every terminal ignore input even if one regains
        // focus (a click on it, a tab switch).
        self.input_gate.set_paused(true);
        // Without focus GPUI dispatches no keys at all: focus the view.
        window.focus(&self.focus);
        cx.notify();
    }

    /// Closes the name field and gives the keyboard back to the terminal.
    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor = None;
        self.input_gate.set_paused(false);
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Keys while the name field is open; they never reach the terminal.
    pub(super) fn on_editor_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let keystroke = &event.keystroke;
        let mods = Mods {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        match editor
            .input
            .handle(&keystroke.key, keystroke.key_char.as_deref(), mods)
        {
            InputAction::Commit => self.commit_editor(window, cx),
            InputAction::Cancel => self.close_editor(window, cx),
            InputAction::Changed => editor.error = None,
            InputAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    editor.input.insert(&text);
                    editor.error = None;
                }
            }
            InputAction::Ignored => {}
        }
        cx.notify();
    }

    fn commit_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let except = match editor.target {
            EditTarget::NewWorkspace => None,
            EditTarget::Rename(w) => Some(w),
        };
        let name = match self.tree.check_workspace_name(&editor.input.text, except) {
            Ok(name) => name,
            Err(message) => {
                editor.error = Some(message);
                return;
            }
        };
        let target = editor.target;
        self.close_editor(window, cx);
        match target {
            EditTarget::NewWorkspace => {
                let workspace = self.tree.add_workspace(name.clone());
                self.persist(&Edit::AddWorkspace { name });
                // A new workspace is empty: offer its first folders right away.
                self.pick_folders(workspace, window, cx);
            }
            EditTarget::Rename(workspace) => {
                let node = &mut self.tree.workspaces[workspace];
                if node.name == name {
                    return;
                }
                let from = std::mem::replace(&mut node.name, name.clone());
                if node.implicit {
                    // Not in the file yet: renaming it is what saves it.
                    let projects = self.entries(workspace);
                    let saved = self.persist(&Edit::AddProjects {
                        workspace: name,
                        projects,
                    });
                    self.tree.workspaces[workspace].implicit = !saved;
                } else {
                    self.persist(&Edit::RenameWorkspace { from, to: name });
                }
            }
        }
    }

    /// Opens the native folder picker and adds the chosen folders.
    pub(super) fn pick_folders(
        &mut self,
        workspace: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.tree.workspaces[workspace].name.clone();
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some(format!("Add to {name}").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.add_folders_to(&name, paths, cx));
        })
        .detach();
    }

    /// Adds folders to the workspace called `name` (it may have been renamed
    /// or removed while a picker or a background task ran).
    fn add_folders_to(&mut self, name: &str, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        match self.tree.workspaces.iter().position(|w| w.name == name) {
            Some(index) => self.add_folders(index, paths, cx),
            None => {
                self.notice = Some(format!(
                    "Folders not added: workspace '{name}' was renamed or removed"
                ));
                cx.notify();
            }
        }
    }

    /// Adds every existing directory in `paths` to `workspace`. Checking the
    /// folders and reading their git remotes touches the disk (maybe a slow
    /// network drive), so it runs on a background thread.
    pub(super) fn add_folders(
        &mut self,
        workspace: usize,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let name = self.tree.workspaces[workspace].name.clone();
        cx.spawn(async move |this, cx| {
            let folders = cx
                .background_spawn(async move {
                    paths
                        .into_iter()
                        .filter(|p| p.is_dir())
                        .map(|p| {
                            let repo = detect_repo(&p);
                            (p, repo)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match this.tree.workspaces.iter().position(|w| w.name == name) {
                    Some(index) => this.add_detected(index, folders, cx),
                    None => {
                        this.notice = Some(format!(
                            "Folders not added: workspace '{name}' was renamed or removed"
                        ));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Adds checked folders (with their detected repository) to `workspace`,
    /// saves the change and selects the last added project.
    fn add_detected(
        &mut self,
        workspace: usize,
        folders: Vec<(PathBuf, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        let mut added = Vec::new();
        let mut last = None;
        for (path, repo) in folders {
            let node = project_node(&path, repo);
            let (index, is_new) = self.tree.add_project(workspace, node);
            last = Some(index);
            if is_new {
                added.push(index);
            }
        }
        if !added.is_empty() {
            let implicit = self.tree.workspaces[workspace].implicit;
            let projects = if implicit {
                // Saving the implicit workspace writes all its projects.
                self.entries(workspace)
            } else {
                added
                    .iter()
                    .map(|&p| {
                        let p = &self.tree.projects[p];
                        ProjectEntry::new(&p.name, &p.path, p.repo.as_deref())
                    })
                    .collect()
            };
            let name = self.tree.workspaces[workspace].name.clone();
            let saved = self.persist(&Edit::AddProjects {
                workspace: name,
                projects,
            });
            if implicit && saved {
                self.tree.workspaces[workspace].implicit = false;
            }
            // New repositories change what the board lists.
            self.boards.remove(&workspace);
            self.refresh_branches(cx);
        }
        if let Some(project) = last {
            self.select_project(project, cx);
        }
        cx.notify();
    }

    pub(super) fn remove_project(
        &mut self,
        workspace: usize,
        project: usize,
        cx: &mut Context<Self>,
    ) {
        if self.has_sessions(&[project]) || !self.tree.remove_project(workspace, project) {
            return;
        }
        if !self.tree.workspaces[workspace].implicit {
            let path = ProjectEntry::new("", &self.tree.projects[project].path, None).path;
            self.persist(&Edit::RemoveProject {
                workspace: self.tree.workspaces[workspace].name.clone(),
                path,
            });
        }
        self.boards.remove(&workspace);
        if self.selected_project == project {
            self.select_fallback(cx);
        }
        cx.notify();
    }

    pub(super) fn remove_workspace(&mut self, workspace: usize, cx: &mut Context<Self>) {
        if self.has_sessions(&self.tree.workspaces[workspace].projects) {
            return;
        }
        let selected_here = self.selected_workspace() == Some(workspace);
        let removed = self.tree.remove_workspace(workspace);
        if !removed.implicit {
            self.persist(&Edit::RemoveWorkspace { name: removed.name });
        }
        // Boards and the selected task are keyed by workspace index.
        self.boards = std::mem::take(&mut self.boards)
            .into_iter()
            .filter(|(w, _)| *w != workspace)
            .map(|(w, state)| (if w > workspace { w - 1 } else { w }, state))
            .collect();
        self.selected_task = None;
        if matches!(
            self.editor.as_ref().map(|e| e.target),
            Some(EditTarget::Rename(_))
        ) {
            self.editor = None;
            self.input_gate.set_paused(false);
        }
        if selected_here {
            self.select_fallback(cx);
        }
        cx.notify();
    }

    /// Whether any open session runs in one of `projects`.
    pub(super) fn has_sessions(&self, projects: &[usize]) -> bool {
        self.tabs.iter().any(|tab| projects.contains(&tab.project))
    }

    /// Selects the first project still in a workspace, if any.
    fn select_fallback(&mut self, cx: &mut Context<Self>) {
        if let Some(&project) = self.tree.workspaces.iter().find_map(|w| w.projects.first()) {
            self.select_project(project, cx);
        }
    }

    /// Config entries for every project of `workspace`.
    fn entries(&self, workspace: usize) -> Vec<ProjectEntry> {
        self.tree.workspaces[workspace]
            .projects
            .iter()
            .map(|&p| {
                let p = &self.tree.projects[p];
                ProjectEntry::new(&p.name, &p.path, p.repo.as_deref())
            })
            .collect()
    }

    /// Saves `edit` to the config file; on failure the change stays for this
    /// run and the sidebar says why. Returns whether it was saved.
    fn persist(&mut self, edit: &Edit) -> bool {
        self.notice = match &self.config_path {
            None => Some("Changes are not saved: no config directory is known".into()),
            Some(path) => config_edit::save(path, edit, self.home.as_deref())
                .err()
                .map(|err| format!("Not saved to the config file: {err}")),
        };
        self.notice.is_none()
    }

    /// The inline name field (also shows why a name was refused).
    pub(super) fn render_editor(&self) -> Option<AnyElement> {
        let editor = self.editor.as_ref()?;
        let placeholder = editor.input.text.is_empty();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
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
                        .text_color(rgb(if placeholder { MUTED } else { TEXT }))
                        .child(SharedString::from(if placeholder {
                            "Workspace name".to_string()
                        } else {
                            editor.input.text.clone()
                        }))
                        .child(div().w(px(2.0)).h(px(14.0)).ml(px(1.0)).bg(rgb(ACCENT))),
                )
                .child(
                    div()
                        .px_1()
                        .text_size(px(SMALL_TEXT_SIZE))
                        .text_color(rgb(if editor.error.is_some() { ERROR } else { MUTED }))
                        .child(SharedString::from(
                            editor
                                .error
                                .clone()
                                .unwrap_or_else(|| "Enter to save · Esc to cancel".into()),
                        )),
                )
                .into_any_element(),
        )
    }

    /// Whether the name field is renaming `workspace`.
    pub(super) fn renaming(&self, workspace: usize) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|e| e.target == EditTarget::Rename(workspace))
    }

    /// Small action shown on hover of the `group` row.
    pub(super) fn row_action(
        id: impl Into<ElementId>,
        group: SharedString,
        text: &'static str,
        color: u32,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .px(px(5.0))
            .rounded_sm()
            .cursor_pointer()
            .text_color(rgb(MUTED))
            .opacity(0.0)
            .group_hover(group, |s| s.opacity(1.0))
            .hover(move |s| s.text_color(rgb(color)).bg(rgba(with_alpha(color, 0x22))))
            .child(text)
    }

    /// Hint shown in an empty workspace.
    pub(super) fn render_empty_workspace(
        &self,
        workspace: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id(("empty-workspace", workspace))
            .mx_2()
            .px_2()
            .py_2()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(rgba(with_alpha(WORKSPACE, 0x55)))
            .text_size(px(SMALL_TEXT_SIZE))
            .text_color(rgb(MUTED))
            .cursor_pointer()
            .hover(|s| {
                s.text_color(rgb(TEXT))
                    .border_color(rgba(with_alpha(WORKSPACE, 0xaa)))
            })
            .child("Drop folders here or click to add")
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.pick_folders(workspace, window, cx)
            }))
            .into_any_element()
    }
}
