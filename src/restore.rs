//! Sessions of the last run, reopened at startup.
//!
//! Stored as JSON next to the config file (`sessions.json`). The file only
//! says *which* launcher ran in *which* project: what gets typed into the
//! shell always comes from the config, so a tampered file cannot run
//! anything the config does not already define.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::tree::{Launcher, ProjectTree, same_dir};

/// File name, in the config directory.
pub const FILE_NAME: &str = "sessions.json";
/// Sessions restored at most, whatever the file says.
pub const MAX_SESSIONS: usize = 20;
/// Larger files are ignored.
const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSession {
    /// Project directory.
    pub project: PathBuf,
    /// Launcher name ("Shell", "Claude Code"...).
    pub launcher: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedState {
    #[serde(default)]
    pub sessions: Vec<SavedSession>,
    /// Index of the active session in `sessions`.
    #[serde(default)]
    pub active: Option<usize>,
}

/// Reads the state; a missing, oversized or unreadable file is empty.
pub fn load(path: &Path) -> SavedState {
    let Ok(meta) = std::fs::metadata(path) else {
        return SavedState::default();
    };
    if meta.len() > MAX_FILE_BYTES {
        return SavedState::default();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the state atomically (synced temporary file and rename); the
/// temporary file never outlives a failure.
pub fn save(path: &Path, state: &SavedState) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Deletes the state (nothing to restore next time).
pub fn clear(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

/// Line typed to restore an agent: its command and its `resume_args`,
/// space-separated and unquoted like the command itself (both come from
/// the config, which the user wrote for their shell).
pub fn resume_command(command: &str, resume_args: Option<&[String]>) -> String {
    match resume_args {
        Some(args) if !args.is_empty() => format!("{command} {}", args.join(" ")),
        _ => command.to_string(),
    }
}

/// What to reopen: `(project, launcher)` for every saved session whose
/// project is still listed in a workspace and whose launcher still exists,
/// in order, at most [`MAX_SESSIONS`]; plus the position of the active one
/// in that list. A folder listed in two workspaces maps to the first one.
pub fn plan(
    state: &SavedState,
    tree: &ProjectTree,
    launchers: &[Launcher],
) -> (Vec<(usize, usize)>, Option<usize>) {
    let listed: Vec<usize> = tree
        .workspaces
        .iter()
        .flat_map(|w| w.projects.iter().copied())
        .collect();
    let mut sessions = Vec::new();
    let mut active = None;
    for (index, saved) in state.sessions.iter().enumerate() {
        if sessions.len() >= MAX_SESSIONS {
            break;
        }
        let project = listed
            .iter()
            .copied()
            .find(|&p| same_dir(&tree.projects[p].path, &saved.project));
        let launcher = launchers.iter().position(|l| l.name == saved.launcher);
        if let (Some(project), Some(launcher)) = (project, launcher) {
            if state.active == Some(index) {
                active = Some(sessions.len());
            }
            sessions.push((project, launcher));
        }
    }
    (sessions, active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, Project, Workspace};
    use crate::tree::launchers;

    fn setup(dir: &Path) -> (ProjectTree, Vec<Launcher>) {
        let tree = ProjectTree::from_config(&[Workspace {
            name: "W".into(),
            board: None,
            projects: vec![
                Project {
                    name: "a".into(),
                    path: dir.join("a"),
                    repo: None,
                },
                Project {
                    name: "b".into(),
                    path: dir.join("b"),
                    repo: None,
                },
            ],
        }]);
        let launchers = launchers(&[Agent {
            name: "Claude Code".into(),
            command: "claude".into(),
            task_args: None,
            resume_args: Some(vec!["--continue".into()]),
        }]);
        (tree, launchers)
    }

    fn saved(project: &Path, launcher: &str) -> SavedSession {
        SavedSession {
            project: project.to_path_buf(),
            launcher: launcher.into(),
        }
    }

    #[test]
    fn plan_keeps_known_projects_and_launchers() {
        let dir = Path::new("/x");
        let (tree, launchers) = setup(dir);
        let state = SavedState {
            sessions: vec![
                saved(&dir.join("a"), "Shell"),
                saved(&dir.join("gone"), "Shell"),
                saved(&dir.join("b"), "Removed agent"),
                saved(&dir.join("b"), "Claude Code"),
            ],
            active: Some(3),
        };
        let (sessions, active) = plan(&state, &tree, &launchers);
        assert_eq!(sessions, vec![(0, 0), (1, 1)]);
        assert_eq!(active, Some(1));
    }

    #[test]
    fn removed_projects_and_the_cap() {
        let dir = Path::new("/x");
        let (mut tree, launchers) = setup(dir);
        tree.remove_project(0, 0);
        let state = SavedState {
            sessions: (0..MAX_SESSIONS + 5)
                .map(|i| saved(&dir.join(if i == 0 { "a" } else { "b" }), "Shell"))
                .collect(),
            active: Some(0),
        };
        let (sessions, active) = plan(&state, &tree, &launchers);
        assert_eq!(sessions.len(), MAX_SESSIONS);
        assert!(sessions.iter().all(|&(p, _)| p == 1));
        assert_eq!(active, None);
    }

    #[test]
    fn resume_command_appends_args() {
        let args = ["--continue".to_string(), "-v".to_string()];
        assert_eq!(
            resume_command("claude", Some(&args)),
            "claude --continue -v"
        );
        assert_eq!(resume_command("claude", Some(&[])), "claude");
        assert_eq!(resume_command("claude", None), "claude");
    }

    #[test]
    fn save_load_and_bad_files() {
        let dir = std::env::temp_dir().join(format!("castorterm-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(FILE_NAME);
        assert_eq!(load(&path), SavedState::default());
        let state = SavedState {
            sessions: vec![saved(Path::new("/p"), "Shell")],
            active: Some(0),
        };
        save(&path, &state).unwrap();
        assert_eq!(load(&path), state);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), SavedState::default());
        std::fs::write(&path, "x".repeat(MAX_FILE_BYTES as usize + 1)).unwrap();
        assert_eq!(load(&path), SavedState::default());
        clear(&path).unwrap();
        clear(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
