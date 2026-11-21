//! Starting an agent on a task in its own git worktree.

use gpui::{AppContext as _, Context, Window};

use super::WorkspaceView;
use crate::config_edit::{Edit, ProjectEntry};
use crate::provider;
use crate::tree::ProjectNode;
use crate::worktree;

impl WorkspaceView {
    /// Creates (or reuses) the worktree of `task_id` for `project` in the
    /// background, adds it as a project of the same workspace and starts
    /// `launcher` there typing `line`.
    pub(super) fn start_in_worktree(
        &mut self,
        project: usize,
        launcher: usize,
        task_id: &str,
        line: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.tree.workspace_of(project) else {
            return;
        };
        let base = self.tree.projects[project].clone();
        let Some((dir, branch)) = worktree::plan(&base.path, task_id) else {
            self.notice = Some(format!(
                "Cannot place a worktree next to {}",
                base.path.display()
            ));
            cx.notify();
            return;
        };
        let path_var = std::env::var_os("PATH");
        let Some(git) = provider::resolve("git", path_var.as_deref(), cfg!(windows)) else {
            self.notice = Some("git not found in PATH: cannot create a worktree".into());
            cx.notify();
            return;
        };
        self.notice = Some(format!("Creating worktree {branch}…"));
        cx.notify();

        let slug = worktree::slug(task_id);
        let workspace_name = self.tree.workspaces[workspace].name.clone();
        let repo_dir = base.path.clone();
        let target = dir.clone();
        let task =
            cx.background_spawn(async move { worktree::create(&git, &repo_dir, &target, &branch) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(err) = result {
                    this.notice = Some(format!("Worktree not created: {err}"));
                    cx.notify();
                    return;
                }
                this.notice = None;
                // The workspace may have been renamed or removed meanwhile.
                let Some(workspace) = this
                    .tree
                    .workspaces
                    .iter()
                    .position(|w| w.name == workspace_name)
                else {
                    this.notice = Some(format!(
                        "Worktree created at {}, but workspace '{workspace_name}' is gone",
                        dir.display()
                    ));
                    cx.notify();
                    return;
                };
                let node = ProjectNode {
                    name: format!("{}@{slug}", base.name),
                    path: dir.clone(),
                    // Same repository: the worktree's tasks are the project's.
                    repo: base.repo.clone(),
                };
                let entry = ProjectEntry::new(&node.name, &node.path, node.repo.as_deref());
                let (index, added) = this.tree.add_project(workspace, node);
                if added {
                    // Saving the implicit workspace writes all its projects.
                    let implicit = this.tree.workspaces[workspace].implicit;
                    let projects = if implicit {
                        this.entries(workspace)
                    } else {
                        vec![entry]
                    };
                    let saved = this.persist(&Edit::AddProjects {
                        workspace: workspace_name.clone(),
                        projects,
                    });
                    if implicit && saved {
                        this.tree.workspaces[workspace].implicit = false;
                    }
                    this.refresh_branches(cx);
                }
                this.launch_with(index, launcher, Some(line), window, cx);
            });
        })
        .detach();
    }
}
