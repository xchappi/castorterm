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
    /// Name the user gave the session, if any (validated on restore).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedState {
    #[serde(default)]
    pub sessions: Vec<SavedSession>,
    /// Index of the active session in `sessions`.
    #[serde(default)]
    pub active: Option<usize>,
    /// Window geometry of the last run.
    #[serde(default)]
    pub window: Option<SavedWindow>,
}

/// Window position and size (logical pixels); `maximized` restores it
/// maximized, with these bounds for when it is restored down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWindow {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

impl SavedWindow {
    /// Smallest window worth restoring; smaller ones fall back to the default.
    const MIN_WIDTH: f32 = 400.0;
    const MIN_HEIGHT: f32 = 300.0;
    const MAX: f32 = 20_000.0;
    /// Part of the window that must be on a screen to restore it there.
    const MIN_VISIBLE: f32 = 120.0;
    /// Height of the title bar area that must stay reachable.
    const TITLE_BAR: f32 = 40.0;

    /// Plausible values (a hand-edited or corrupt file never makes an
    /// unusable window).
    pub fn is_sane(&self) -> bool {
        let finite = [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite());
        finite
            && (Self::MIN_WIDTH..=Self::MAX).contains(&self.width)
            && (Self::MIN_HEIGHT..=Self::MAX).contains(&self.height)
            && self.x.abs() <= Self::MAX * 4.0
            && self.y.abs() <= Self::MAX * 4.0
    }
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

/// A session to reopen: project index, launcher index and its saved name.
pub type PlannedSession = (usize, usize, Option<String>);

/// What to reopen: `(project, launcher, name)` for every saved session whose
/// project is still listed in a workspace and whose launcher still exists,
/// in order, at most [`MAX_SESSIONS`]; plus the position of the active one
/// in that list. A folder listed in two workspaces maps to the first one.
pub fn plan(
    state: &SavedState,
    tree: &ProjectTree,
    launchers: &[Launcher],
) -> (Vec<PlannedSession>, Option<usize>) {
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
            sessions.push((project, launcher, saved.label.clone()));
        }
    }
    (sessions, active)
}

impl SavedWindow {
    /// Where to restore the window among `displays` (`x, y, width, height`):
    /// on the display it overlaps most, with at least [`Self::MIN_VISIBLE`]
    /// pixels visible each way and its title bar on that display, shrunk to
    /// fit it. `None` when it is on no connected display.
    pub fn place(&self, displays: &[(f32, f32, f32, f32)]) -> Option<SavedWindow> {
        if !self.is_sane() {
            return None;
        }
        let overlap = |&(dx, dy, dw, dh): &(f32, f32, f32, f32)| {
            let w = (self.x + self.width).min(dx + dw) - self.x.max(dx);
            let h = (self.y + self.height).min(dy + dh) - self.y.max(dy);
            (w, h)
        };
        let &(dx, dy, dw, dh) = displays
            .iter()
            .filter(|d| {
                let (w, h) = overlap(d);
                let (_, dy, _, dh) = **d;
                w >= Self::MIN_VISIBLE
                    && h >= Self::MIN_VISIBLE
                    && self.y >= dy
                    && self.y <= dy + dh - Self::TITLE_BAR
            })
            .max_by(|a, b| {
                let area = |d| {
                    let (w, h) = overlap(d);
                    w * h
                };
                area(a).total_cmp(&area(b))
            })?;
        let width = self.width.min(dw);
        let height = self.height.min(dh);
        Some(SavedWindow {
            x: self.x.clamp(dx, dx + dw - width),
            y: self.y.clamp(dy, dy + dh - height),
            width,
            height,
            maximized: self.maximized,
        })
    }
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
            collapsed: false,
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
            label: None,
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
            window: None,
        };
        let (sessions, active) = plan(&state, &tree, &launchers);
        assert_eq!(sessions, vec![(0, 0, None), (1, 1, None)]);
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
            window: None,
        };
        let (sessions, active) = plan(&state, &tree, &launchers);
        assert_eq!(sessions.len(), MAX_SESSIONS);
        assert!(sessions.iter().all(|(p, _, _)| *p == 1));
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
    fn window_geometry_sanity_and_round_trip() {
        let ok = SavedWindow {
            x: -1200.0,
            y: 40.0,
            width: 1360.0,
            height: 820.0,
            maximized: true,
        };
        assert!(ok.is_sane());
        for bad in [
            SavedWindow { width: 10.0, ..ok },
            SavedWindow {
                height: f32::NAN,
                ..ok
            },
            SavedWindow { x: 1e9, ..ok },
            SavedWindow { width: 1e7, ..ok },
        ] {
            assert!(!bad.is_sane(), "{bad:?}");
        }
        // Old files without a window still load.
        let old: SavedState = serde_json::from_str(r#"{"sessions":[],"active":null}"#).unwrap();
        assert_eq!(old.window, None);
        let state = SavedState {
            window: Some(ok),
            ..SavedState::default()
        };
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<SavedState>(&text).unwrap(), state);
    }

    #[test]
    fn placement_on_connected_displays() {
        let main = (0.0, 0.0, 1920.0, 1080.0);
        let left = (-1366.0, 200.0, 1366.0, 768.0);
        let w = |x, y, width, height| SavedWindow {
            x,
            y,
            width,
            height,
            maximized: false,
        };
        // Fully on the main display: unchanged.
        assert_eq!(
            w(100.0, 100.0, 900.0, 600.0).place(&[main]),
            Some(w(100.0, 100.0, 900.0, 600.0))
        );
        // On the left monitor (negative x).
        assert!(
            w(-1200.0, 300.0, 900.0, 600.0)
                .place(&[main, left])
                .is_some()
        );
        // That monitor unplugged.
        assert_eq!(w(-1200.0, 300.0, 900.0, 600.0).place(&[main]), None);
        // Only a sliver visible: not restored there.
        assert_eq!(w(1900.0, 100.0, 900.0, 600.0).place(&[main]), None);
        // Title bar above the screen: not restored.
        assert_eq!(w(100.0, -500.0, 900.0, 600.0).place(&[main]), None);
        // Bigger than the display: shrunk and kept on it.
        let big = w(-1366.0, 200.0, 1920.0, 1080.0).place(&[left]).unwrap();
        assert_eq!((big.width, big.height), (1366.0, 768.0));
        assert_eq!((big.x, big.y), (-1366.0, 200.0));
        // Insane values never place.
        assert_eq!(w(f32::NAN, 0.0, 900.0, 600.0).place(&[main]), None);
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
            window: None,
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

    #[test]
    fn labels_are_optional_in_the_file() {
        let old: SavedState =
            serde_json::from_str(r#"{"sessions":[{"project":"/a","launcher":"Shell"}]}"#).unwrap();
        assert_eq!(old.sessions[0].label, None);
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("label"));
        let mut named = old.clone();
        named.sessions[0].label = Some("Reviewer".into());
        let json = serde_json::to_string(&named).unwrap();
        assert!(json.contains(r#""label":"Reviewer""#));
        let back: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, named);
    }
}
