//! Workspace -> Project tree shown in the sidebar, and the session launchers.

use std::path::{Path, PathBuf};

use crate::config::{Agent, Workspace};

/// Name of the implicit workspace for a launch directory outside the config.
pub const LOCAL_WORKSPACE: &str = "Local";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectNode {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceNode {
    pub name: String,
    /// Indices into [`ProjectTree::projects`].
    pub projects: Vec<usize>,
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
            };
            for project in &workspace.projects {
                node.projects.push(tree.projects.len());
                tree.projects.push(ProjectNode {
                    name: project.name.clone(),
                    path: project.path.clone(),
                });
            }
            tree.workspaces.push(node);
        }
        tree
    }

    /// Project whose path is `dir`; if none, adds a project named after the
    /// directory to the implicit "Local" workspace. Returns its index.
    pub fn project_for_dir(&mut self, dir: &Path) -> usize {
        if let Some(index) = self.projects.iter().position(|p| same_dir(&p.path, dir)) {
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
        });
        match self
            .workspaces
            .iter_mut()
            .find(|w| w.name == LOCAL_WORKSPACE)
        {
            Some(local) => local.projects.push(index),
            None => self.workspaces.push(WorkspaceNode {
                name: LOCAL_WORKSPACE.into(),
                projects: vec![index],
            }),
        }
        index
    }
}

/// Compares directories after resolving symlinks, `..` and (on Windows)
/// letter case; falls back to a plain comparison if either does not exist.
fn same_dir(a: &Path, b: &Path) -> bool {
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
}

/// The built-in "Shell" launcher followed by one per configured agent.
pub fn launchers(agents: &[Agent]) -> Vec<Launcher> {
    std::iter::once(Launcher {
        name: "Shell".into(),
        command: None,
    })
    .chain(agents.iter().map(|a| Launcher {
        name: a.name.clone(),
        command: Some(a.command.clone()),
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
                projects: vec![
                    Project {
                        name: "here".into(),
                        path: path.to_path_buf(),
                    },
                    Project {
                        name: "other".into(),
                        path: path.join("does-not-exist"),
                    },
                ],
            },
            Workspace {
                name: "Empty".into(),
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
    fn launchers_start_with_shell() {
        let l = launchers(&[Agent {
            name: "OpenCode".into(),
            command: "opencode".into(),
        }]);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, "Shell");
        assert_eq!(l[0].command, None);
        assert_eq!(l[1].command.as_deref(), Some("opencode"));
    }
}
