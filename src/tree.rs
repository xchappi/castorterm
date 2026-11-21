//! Workspace -> Project tree shown in the sidebar, and the session launchers.

use std::path::{Path, PathBuf};

use crate::config::{Agent, Board, Workspace};

/// Name of the implicit workspace for a launch directory outside the config.
pub const LOCAL_WORKSPACE: &str = "Local";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectNode {
    pub name: String,
    pub path: PathBuf,
    /// Repository identifier used to match tracker tasks.
    pub repo: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceNode {
    pub name: String,
    /// Indices into [`ProjectTree::projects`].
    pub projects: Vec<usize>,
    /// Created by castorterm for directories outside the config.
    pub implicit: bool,
    /// Task board shown in the Quests tab.
    pub board: Option<Board>,
    /// Folded in the sidebar (projects hidden).
    pub collapsed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectTree {
    pub workspaces: Vec<WorkspaceNode>,
    pub projects: Vec<ProjectNode>,
}

impl ProjectTree {
    pub fn from_config(workspaces: &[Workspace]) -> Self {
        let mut tree = Self::default();
        for workspace in workspaces {
            let mut node = WorkspaceNode {
                name: workspace.name.clone(),
                projects: Vec::new(),
                implicit: false,
                board: workspace.board.clone(),
                collapsed: workspace.collapsed,
            };
            for project in &workspace.projects {
                node.projects.push(tree.projects.len());
                tree.projects.push(ProjectNode {
                    name: project.name.clone(),
                    path: project.path.clone(),
                    repo: project.repo.clone(),
                });
            }
            tree.workspaces.push(node);
        }
        tree
    }

    /// Project whose path is `dir`; if none, adds a project named after the
    /// directory to the implicit "Local" workspace. Returns its index.
    pub fn project_for_dir(&mut self, dir: &Path) -> usize {
        // Only projects still in a workspace (removed ones stay as tombstones).
        let listed = self
            .workspaces
            .iter()
            .flat_map(|w| w.projects.iter().copied())
            .find(|&p| same_dir(&self.projects[p].path, dir));
        if let Some(index) = listed {
            return index;
        }
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());
        let index = self.projects.len();
        self.projects.push(ProjectNode {
            name,
            path: dir.to_path_buf(),
            repo: None,
        });
        match self.workspaces.iter_mut().find(|w| w.implicit) {
            Some(local) => local.projects.push(index),
            None => {
                let name = self.unique_name(LOCAL_WORKSPACE);
                self.workspaces.push(WorkspaceNode {
                    name,
                    projects: vec![index],
                    implicit: true,
                    board: None,
                    collapsed: false,
                })
            }
        }
        index
    }
}

/// Longest workspace name accepted from the UI.
pub const MAX_WORKSPACE_NAME: usize = 60;

impl ProjectTree {
    /// Workspace that lists `project`.
    pub fn workspace_of(&self, project: usize) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|w| w.projects.contains(&project))
    }

    /// Trimmed `name` if it can name a new workspace (or rename `except`):
    /// single line, not empty, not too long and not used by another one.
    pub fn check_workspace_name(
        &self,
        name: &str,
        except: Option<usize>,
    ) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("The workspace name cannot be empty".into());
        }
        if name.chars().any(char::is_control) {
            return Err("The workspace name must be a single line".into());
        }
        if name.chars().count() > MAX_WORKSPACE_NAME {
            return Err(format!(
                "The workspace name can have at most {MAX_WORKSPACE_NAME} characters"
            ));
        }
        let taken = self
            .workspaces
            .iter()
            .enumerate()
            .any(|(i, w)| Some(i) != except && w.name.eq_ignore_ascii_case(name));
        if taken {
            return Err(format!("There is already a workspace named '{name}'"));
        }
        Ok(name.to_string())
    }

    /// `base`, or `base 2`, `base 3`... if a workspace already uses it, so
    /// names always identify one workspace.
    fn unique_name(&self, base: &str) -> String {
        let taken = |name: &str| {
            self.workspaces
                .iter()
                .any(|w| w.name.eq_ignore_ascii_case(name))
        };
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .expect("unbounded range")
    }

    /// Appends an empty workspace and returns its index.
    pub fn add_workspace(&mut self, name: String) -> usize {
        self.workspaces.push(WorkspaceNode {
            name,
            projects: Vec::new(),
            implicit: false,
            board: None,
            collapsed: false,
        });
        self.workspaces.len() - 1
    }

    /// Adds `node` to `workspace` unless a project with the same directory
    /// is already there. Returns the project index and whether it was added.
    /// Project indices never change: new projects are appended.
    pub fn add_project(&mut self, workspace: usize, node: ProjectNode) -> (usize, bool) {
        if let Some(&existing) = self.workspaces[workspace]
            .projects
            .iter()
            .find(|&&p| same_dir(&self.projects[p].path, &node.path))
        {
            return (existing, false);
        }
        let index = self.projects.len();
        self.projects.push(node);
        self.workspaces[workspace].projects.push(index);
        (index, true)
    }

    /// Takes `project` out of `workspace`. The node stays in
    /// [`ProjectTree::projects`] so other indices keep their meaning.
    pub fn remove_project(&mut self, workspace: usize, project: usize) -> bool {
        let list = &mut self.workspaces[workspace].projects;
        let before = list.len();
        list.retain(|&p| p != project);
        list.len() != before
    }

    /// Removes `workspace`; later workspaces move down one index.
    pub fn remove_workspace(&mut self, workspace: usize) -> WorkspaceNode {
        self.workspaces.remove(workspace)
    }
}

/// Short label for a session's working directory, or `None` when it is the
/// project root (nothing to add). Inside the project: the relative path
/// (`src/view`); elsewhere: the last two components (`…/a/b`).
pub fn cwd_label(project: &Path, cwd: &Path) -> Option<String> {
    let normalize = |p: &Path| {
        let text = p.to_string_lossy().replace('\\', "/");
        let text = text.trim_end_matches('/').to_string();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let (root, dir) = (normalize(project), normalize(cwd));
    if dir == root {
        return None;
    }
    let shown = cwd.to_string_lossy().replace('\\', "/");
    let shown = shown.trim_end_matches('/');
    if let Some(rest) = dir.strip_prefix(&format!("{root}/")) {
        // Same length in the original spelling: keep its case.
        return Some(shown[shown.len() - rest.len()..].to_string());
    }
    let parts: Vec<&str> = shown.split('/').filter(|p| !p.is_empty()).collect();
    Some(match parts.as_slice() {
        [_] | [_, _] | [] => shown.to_string(),
        [.., a, b] => format!("…/{a}/{b}"),
    })
}

/// Project node for a folder added from the UI: named after the folder.
pub fn project_node(dir: &Path, repo: Option<String>) -> ProjectNode {
    ProjectNode {
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string()),
        path: dir.to_path_buf(),
        repo,
    }
}

/// Compares directories after resolving symlinks, `..` and (on Windows)
/// letter case; falls back to a plain comparison if either does not exist.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// What a sidebar launcher starts: the shell, optionally with a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launcher {
    pub name: String,
    pub command: Option<String>,
    /// How to pass an initial prompt (agents only); see `config::Agent`.
    pub task_args: Option<Vec<String>>,
    /// Extra arguments when restored at startup; see `config::Agent`.
    pub resume_args: Option<Vec<String>>,
}

/// Index of the built-in shell launcher in [`launchers`].
pub const SHELL_LAUNCHER: usize = 0;

/// The built-in "Shell" launcher followed by one per configured agent.
pub fn launchers(agents: &[Agent]) -> Vec<Launcher> {
    std::iter::once(Launcher {
        name: "Shell".into(),
        command: None,
        task_args: None,
        resume_args: None,
    })
    .chain(agents.iter().map(|a| Launcher {
        name: a.name.clone(),
        command: Some(a.command.clone()),
        task_args: a.task_args.clone(),
        resume_args: a.resume_args.clone(),
    }))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Project;

    fn config_tree(path: &Path) -> ProjectTree {
        ProjectTree::from_config(&[
            Workspace {
                name: "CastorLabs".into(),
                board: None,
                collapsed: false,
                projects: vec![
                    Project {
                        name: "here".into(),
                        path: path.to_path_buf(),
                        repo: None,
                    },
                    Project {
                        name: "other".into(),
                        path: path.join("does-not-exist"),
                        repo: None,
                    },
                ],
            },
            Workspace {
                name: "Empty".into(),
                board: None,
                collapsed: false,
                projects: vec![],
            },
        ])
    }

    #[test]
    fn tree_from_config_indexes_projects() {
        let tree = config_tree(Path::new("/x"));
        assert_eq!(tree.workspaces.len(), 2);
        assert_eq!(tree.workspaces[0].projects, vec![0, 1]);
        assert!(tree.workspaces[1].projects.is_empty());
        assert_eq!(tree.projects[1].name, "other");
    }

    #[test]
    fn launch_dir_matches_configured_project() {
        let dir = std::env::temp_dir();
        let mut tree = config_tree(&dir);
        // A different spelling of the same directory still matches.
        let spelled = dir.join("..").join(dir.file_name().unwrap());
        assert_eq!(tree.project_for_dir(&spelled), 0);
        assert_eq!(tree.workspaces.len(), 2);
    }

    #[test]
    fn unknown_launch_dir_goes_to_local_workspace() {
        let dir = std::env::temp_dir();
        let mut tree = config_tree(&dir.join("not-a-project"));
        let index = tree.project_for_dir(&dir);
        assert_eq!(index, 2);
        let local = tree.workspaces.last().unwrap();
        assert_eq!(local.name, LOCAL_WORKSPACE);
        assert_eq!(local.projects, vec![2]);
        assert_eq!(
            tree.projects[2].name,
            dir.file_name().unwrap().to_string_lossy()
        );
        // Asking again reuses the project.
        assert_eq!(tree.project_for_dir(&dir), 2);
    }

    #[test]
    fn user_workspace_named_local_is_not_reused() {
        let dir = std::env::temp_dir();
        let mut tree = ProjectTree::from_config(&[Workspace {
            name: LOCAL_WORKSPACE.into(),
            board: None,
            collapsed: false,
            projects: vec![],
        }]);
        tree.project_for_dir(&dir);
        assert_eq!(tree.workspaces.len(), 2);
        assert!(tree.workspaces[0].projects.is_empty());
        assert!(tree.workspaces[1].implicit);
        // Names stay unique: saving the implicit one never hits the user's.
        assert_eq!(tree.workspaces[1].name, "Local 2");
    }

    #[test]
    fn cwd_labels() {
        let root = Path::new("/src/app");
        assert_eq!(cwd_label(root, Path::new("/src/app")), None);
        assert_eq!(cwd_label(root, Path::new("/src/app/")), None);
        assert_eq!(
            cwd_label(root, Path::new("/src/app/src/view")).as_deref(),
            Some("src/view")
        );
        assert_eq!(
            cwd_label(root, Path::new("/src/apple")).as_deref(),
            Some("/src/apple")
        );
        assert_eq!(cwd_label(root, Path::new("/tmp")).as_deref(), Some("/tmp"));
        assert_eq!(
            cwd_label(root, Path::new("/a/b/c/d")).as_deref(),
            Some("…/c/d")
        );
        if cfg!(windows) {
            let root = Path::new(r"D:\proyectos\castorterm");
            assert_eq!(cwd_label(root, Path::new(r"d:\Proyectos\castorterm")), None);
            assert_eq!(
                cwd_label(root, Path::new(r"D:\proyectos\castorterm\src")).as_deref(),
                Some("src")
            );
        }
    }

    #[test]
    fn workspace_names_are_checked() {
        let tree = config_tree(Path::new("/x"));
        assert_eq!(tree.check_workspace_name("  New  ", None).unwrap(), "New");
        assert!(tree.check_workspace_name(" ", None).is_err());
        assert!(
            tree.check_workspace_name(
                "a
b", None
            )
            .is_err()
        );
        assert!(
            tree.check_workspace_name(&"x".repeat(MAX_WORKSPACE_NAME + 1), None)
                .is_err()
        );
        assert!(
            tree.check_workspace_name("castorlabs", None)
                .unwrap_err()
                .contains("already")
        );
        // Renaming a workspace to itself (other case) is fine.
        assert_eq!(
            tree.check_workspace_name("CASTORLABS", Some(0)).unwrap(),
            "CASTORLABS"
        );
    }

    #[test]
    fn projects_are_added_once_and_removed_without_moving_indices() {
        let dir = std::env::temp_dir();
        let mut tree = config_tree(&dir.join("a"));
        let w = tree.add_workspace("New".into());
        assert_eq!(w, 2);
        let (p, added) = tree.add_project(w, project_node(&dir, Some("o/r".into())));
        assert!(added);
        assert_eq!(p, 2);
        assert_eq!(
            tree.projects[p].name,
            dir.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(tree.workspace_of(p), Some(w));
        // Same folder, other spelling: not added again.
        let spelled = dir.join("..").join(dir.file_name().unwrap());
        assert_eq!(
            tree.add_project(w, project_node(&spelled, None)),
            (p, false)
        );
        // The same folder may live in another workspace.
        assert!(tree.add_project(1, project_node(&dir, None)).1);

        assert!(tree.remove_project(0, 0));
        assert!(!tree.remove_project(0, 0));
        // A removed project is not found again by its directory.
        let mut other = tree.clone();
        let again = other.project_for_dir(&dir.join("a"));
        assert_ne!(again, 0);
        assert_eq!(tree.workspaces[0].projects, vec![1]);
        assert_eq!(tree.projects[1].name, "other");
        assert_eq!(tree.workspace_of(0), None);

        let removed = tree.remove_workspace(0);
        assert_eq!(removed.name, "CastorLabs");
        assert_eq!(tree.workspaces[1].name, "New");
        assert_eq!(tree.workspace_of(p), Some(1));
    }

    #[test]
    fn launchers_start_with_shell() {
        let l = launchers(&[Agent {
            name: "OpenCode".into(),
            command: "opencode".into(),
            task_args: None,
            resume_args: None,
        }]);
        assert_eq!(l.len(), 2);
        assert_eq!(l[SHELL_LAUNCHER].name, "Shell");
        assert_eq!(l[0].command, None);
        assert_eq!(l[1].command.as_deref(), Some("opencode"));
    }
}
