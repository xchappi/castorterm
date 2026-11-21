//! What each agent is doing (working, waiting for the user, done), reported
//! by the agent itself through hooks.
//!
//! Every session gets `CASTORTERM_STATUS_FILE` in its environment: a file in
//! `<config dir>/status/`. An agent hook runs `castorterm --status waiting`
//! (for example Claude Code's `Notification` hook), which writes the status
//! to that file; castorterm polls the files of its open sessions. Outside
//! castorterm the variable is unset and the command does nothing.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Environment variable with the status file of the session.
pub const ENV_VAR: &str = "CASTORTERM_STATUS_FILE";
/// Folder name, inside the config directory.
pub const DIR_NAME: &str = "status";
const EXTENSION: &str = "status";
/// Status files are tiny; larger ones are ignored.
const MAX_BYTES: u64 = 64;
/// Files of crashed castorterm runs are removed after this long.
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentStatus {
    /// Thinking or running tools.
    Working,
    /// Needs the user: a question, a permission prompt.
    Waiting,
    /// Finished its turn.
    Done,
}

impl AgentStatus {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "working" => Some(Self::Working),
            "waiting" => Some(Self::Waiting),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Done => "done",
        }
    }
}

/// Status file of session `key` of this castorterm process.
pub fn file_for(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{}-{key}.{EXTENSION}", std::process::id()))
}

/// Status in `path`, if it holds a valid one.
pub fn read(path: &Path) -> Option<AgentStatus> {
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    AgentStatus::parse(&text)
}

/// `castorterm --status <working|waiting|done>`: writes the status to the
/// file named by [`ENV_VAR`]. Only files named like castorterm's own status
/// files, in a `status` folder, are written: the variable cannot be used to
/// overwrite anything else.
pub fn write_from_env(value: &str, env_value: Option<&std::ffi::OsStr>) -> Result<(), String> {
    let status = AgentStatus::parse(value)
        .ok_or_else(|| format!("unknown status '{value}': use working, waiting or done"))?;
    let Some(path) = env_value.map(PathBuf::from) else {
        // Not inside castorterm: nothing to report.
        return Ok(());
    };
    let named_ok = path.extension().is_some_and(|e| e == EXTENSION)
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|d| d == DIR_NAME)
        && path.is_absolute();
    if !named_ok {
        return Err(format!("{ENV_VAR} does not name a castorterm status file"));
    }
    std::fs::write(&path, status.as_str()).map_err(|e| format!("could not write status: {e}"))
}

/// Removes status files older than a day (left by crashed runs).
pub fn remove_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != EXTENSION) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > STALE_AFTER);
        if old {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("castorterm-status-{name}-{}", std::process::id()))
            .join(DIR_NAME);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_statuses() {
        assert_eq!(AgentStatus::parse(" Working\n"), Some(AgentStatus::Working));
        assert_eq!(AgentStatus::parse("waiting"), Some(AgentStatus::Waiting));
        assert_eq!(AgentStatus::parse("done"), Some(AgentStatus::Done));
        assert_eq!(AgentStatus::parse("rm -rf"), None);
        assert_eq!(AgentStatus::parse(""), None);
    }

    #[test]
    fn write_and_read() {
        let dir = temp_dir("rw");
        let file = file_for(&dir, 7);
        write_from_env("waiting", Some(file.as_os_str())).unwrap();
        assert_eq!(read(&file), Some(AgentStatus::Waiting));
        write_from_env("done", Some(file.as_os_str())).unwrap();
        assert_eq!(read(&file), Some(AgentStatus::Done));
        // Outside castorterm: no variable, nothing happens.
        write_from_env("done", None).unwrap();
        assert!(write_from_env("bogus", Some(file.as_os_str())).is_err());
        std::fs::write(&file, "x".repeat(200)).unwrap();
        assert_eq!(read(&file), None);
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn only_status_files_are_written() {
        let dir = temp_dir("guard");
        let other = dir.parent().unwrap().join("config.toml");
        assert!(write_from_env("done", Some(other.as_os_str())).is_err());
        assert!(!other.exists());
        let relative = PathBuf::from(DIR_NAME).join("1-1.status");
        assert!(write_from_env("done", Some(relative.as_os_str())).is_err());
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn stale_files_are_removed_and_fresh_kept() {
        let dir = temp_dir("stale");
        let fresh = file_for(&dir, 1);
        std::fs::write(&fresh, "done").unwrap();
        let unrelated = dir.join("keep.txt");
        std::fs::write(&unrelated, "x").unwrap();
        remove_stale(&dir);
        assert!(fresh.exists());
        assert!(unrelated.exists());
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
