//! Files an agent reads or edits, reported by its hooks.
//!
//! Next to each session's status file (see `agent_status`) lives
//! `<pid>-<n>.files`: one JSON line per use, `{"action":"read","path":"..."}`.
//! Claude Code's `PostToolUse` hook runs `castorterm --status touched`
//! (an older castorterm rejects that value without opening a window), which reads
//! the hook's JSON from stdin and appends the line; OpenCode's plugin writes
//! it directly. castorterm reads the tail of the file of each open session.

use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

/// The file is started over past this size.
const MAX_FILE_BYTES: u64 = 512 * 1024;
/// Only the end of the file is read.
const TAIL_BYTES: u64 = 64 * 1024;
/// Hook input larger than this is ignored.
const MAX_HOOK_INPUT: u64 = 1024 * 1024;
/// Files listed per session.
pub const MAX_LISTED: usize = 50;

/// What the agent did with a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Edit,
    Write,
}

impl Action {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "read" => Some(Self::Read),
            "edit" => Some(Self::Edit),
            "write" => Some(Self::Write),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Write => "write",
        }
    }

    /// Changing a file matters more than reading it.
    fn rank(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Edit => 1,
            Self::Write => 2,
        }
    }
}

/// A file the agent used, with the most significant thing it did to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Touched {
    pub path: PathBuf,
    pub action: Action,
}

/// The files list next to status file `status`.
pub fn files_for(status: &Path) -> PathBuf {
    status.with_extension("files")
}

/// Claude Code tool name and input → what was done to which file.
fn from_hook(json: &serde_json::Value) -> Option<(Action, String)> {
    let action = match json.get("tool_name")?.as_str()? {
        "Read" => Action::Read,
        "Edit" | "MultiEdit" | "NotebookEdit" => Action::Edit,
        "Write" => Action::Write,
        _ => return None,
    };
    let input = json.get("tool_input")?;
    let path = input
        .get("file_path")
        .or_else(|| input.get("notebook_path"))?
        .as_str()?;
    Some((action, path.to_string()))
}

/// A path worth recording: absolute, one line, not absurdly long.
fn plain_path(path: &str) -> bool {
    Path::new(path).is_absolute()
        && path.len() <= 4096
        && !path.chars().any(crate::board::is_hidden)
}

/// `castorterm --touched`: reads a Claude Code hook's JSON from `input` and
/// appends the file it used to the list of the session named by the status
/// variable. Silent when not inside castorterm or for other tools.
pub fn record_from_hook(
    input: impl std::io::Read,
    status_env: Option<&std::ffi::OsStr>,
) -> Result<(), String> {
    let Some(status) = status_env.map(PathBuf::from) else {
        return Ok(());
    };
    if !crate::agent_status::is_status_file(&status) {
        return Err(format!(
            "{} does not name a castorterm status file",
            crate::agent_status::ENV_VAR
        ));
    }
    let mut text = String::new();
    input
        .take(MAX_HOOK_INPUT)
        .read_to_string(&mut text)
        .map_err(|e| format!("could not read the hook input: {e}"))?;
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    let Some((action, path)) = from_hook(&json) else {
        return Ok(());
    };
    if !plain_path(&path) {
        return Ok(());
    }
    append(&files_for(&status), action, &path).map_err(|e| format!("could not record: {e}"))
}

fn append(files: &Path, action: Action, path: &str) -> std::io::Result<()> {
    let line = serde_json::json!({ "action": action.as_str(), "path": path }).to_string() + "\n";
    // Never write through a link: it could point at a file of the user's.
    match std::fs::symlink_metadata(files) {
        Ok(meta) if crate::links::is_link(&meta) || !meta.is_file() => {
            return Err(std::io::Error::other("the files list is not a plain file"));
        }
        Ok(meta) if meta.len() > MAX_FILE_BYTES => {
            // Start over atomically: rename replaces the name, so a
            // concurrent append lands in the old file, never mid-line.
            let tmp = files.with_extension(format!("files.{}.tmp", std::process::id()));
            std::fs::write(&tmp, line.as_bytes())?;
            return std::fs::rename(&tmp, files).inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            });
        }
        _ => {}
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(files)?;
    file.write_all(line.as_bytes())
}

/// The files in list `files`, most recent first, each once with the most
/// significant action seen; at most [`MAX_LISTED`]. Unreadable lines are
/// skipped.
#[cfg(test)]
pub fn read(files: &Path) -> Vec<Touched> {
    read_changed(files, None)
        .map(|(_, list)| list)
        .unwrap_or_default()
}

/// Size and modification time of a files list, to skip unchanged ones.
pub type Stamp = (u64, std::time::SystemTime);

/// Like [`read`], but `None` when the list has the same stamp as `prev`
/// (nothing new to parse); a missing list reads as empty.
pub fn read_changed(files: &Path, prev: Option<Stamp>) -> Option<(Option<Stamp>, Vec<Touched>)> {
    let Ok(mut file) = std::fs::File::open(files) else {
        // Missing: empty, reported once.
        return prev.is_some().then(|| (None, Vec::new()));
    };
    let meta = file.metadata().ok()?;
    let stamp = meta.modified().ok().map(|m| (meta.len(), m));
    if stamp.is_some() && stamp == prev {
        return None;
    }
    let len = meta.len();
    // One byte before the window tells whether its first line is whole.
    let start = len.saturating_sub(TAIL_BYTES + 1);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES + 1).read_to_end(&mut bytes).ok()?;
    let cut = if start == 0 {
        false
    } else {
        let whole = bytes.first() == Some(&b'\n');
        bytes.remove(0);
        !whole
    };
    Some((stamp, parse(&String::from_utf8_lossy(&bytes), cut)))
}

fn parse(text: &str, cut: bool) -> Vec<Touched> {
    let mut lines: Vec<&str> = text.lines().collect();
    if cut && !lines.is_empty() {
        // The first line may start mid-way.
        lines.remove(0);
    }
    let mut out: Vec<Touched> = Vec::new();
    for line in lines.iter().rev() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let (Some(action), Some(path)) = (
            json.get("action")
                .and_then(|a| a.as_str())
                .and_then(Action::parse),
            json.get("path").and_then(|p| p.as_str()),
        ) else {
            continue;
        };
        if !plain_path(path) {
            continue;
        }
        let path = PathBuf::from(path);
        match out.iter().position(|t| t.path == path) {
            Some(i) => {
                if action.rank() > out[i].action.rank() {
                    out[i].action = action;
                }
            }
            None if out.len() < MAX_LISTED => out.push(Touched { path, action }),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn setup(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir()
            .join(format!("castorterm-touched-{name}-{}", std::process::id()))
            .join("status");
        std::fs::create_dir_all(&dir).unwrap();
        (dir.clone(), dir.join("123-1.status"))
    }

    #[test]
    fn hooks_are_recorded_and_listed() {
        let (dir, status) = setup("rec");
        let a = abs("a.rs");
        let b = abs("b.rs");
        let hook = |tool: &str, path: &str| {
            serde_json::json!({ "tool_name": tool, "tool_input": { "file_path": path } })
                .to_string()
        };
        for (tool, path) in [("Read", &a), ("Read", &b), ("Edit", &a), ("Read", &a)] {
            record_from_hook(hook(tool, path).as_bytes(), Some(status.as_os_str())).unwrap();
        }
        // Other tools, relative paths and garbage are ignored.
        record_from_hook(hook("Bash", &a).as_bytes(), Some(status.as_os_str())).unwrap();
        record_from_hook(hook("Read", "rel.rs").as_bytes(), Some(status.as_os_str())).unwrap();
        record_from_hook(&b"not json"[..], Some(status.as_os_str())).unwrap();
        let list = read(&files_for(&status));
        assert_eq!(
            list,
            vec![
                Touched {
                    path: a.clone().into(),
                    action: Action::Edit
                },
                Touched {
                    path: b.clone().into(),
                    action: Action::Read
                },
            ]
        );
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn only_inside_castorterm() {
        let (dir, _) = setup("guard");
        let hook =
            serde_json::json!({ "tool_name": "Read", "tool_input": { "file_path": abs("a.rs") } })
                .to_string();
        assert!(record_from_hook(hook.as_bytes(), None).is_ok());
        let bad = dir.parent().unwrap().join("config.toml");
        assert!(record_from_hook(hook.as_bytes(), Some(bad.as_os_str())).is_err());
        assert!(!dir.parent().unwrap().join("config.files").exists());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn hook_shapes() {
        let path = abs("n.ipynb");
        let j = |v: serde_json::Value| from_hook(&v);
        assert_eq!(
            j(serde_json::json!({"tool_name":"MultiEdit","tool_input":{"file_path":path}})),
            Some((Action::Edit, path.clone()))
        );
        assert_eq!(
            j(serde_json::json!({"tool_name":"NotebookEdit","tool_input":{"notebook_path":path}})),
            Some((Action::Edit, path.clone()))
        );
        assert_eq!(
            j(serde_json::json!({"tool_name":"Grep","tool_input":{"file_path":path}})),
            None
        );
        assert_eq!(j(serde_json::json!({"tool_name":"Read"})), None);
        assert_eq!(
            j(serde_json::json!({"tool_name":"Read","tool_input":{"file_path":3}})),
            None
        );
    }

    #[test]
    fn plain_paths_only() {
        assert!(plain_path(&abs("a.rs")));
        assert!(!plain_path("a.rs"));
        assert!(!plain_path(&abs(&"x".repeat(5000))));
        assert!(!plain_path(&abs("a\u{202E}b.rs")));
        assert!(!plain_path(&abs("a\nb.rs")));
    }

    #[test]
    fn big_input_and_rotation() {
        let (dir, status) = setup("rot");
        let a = abs("a.rs");
        let hook = serde_json::json!({ "tool_name": "Read", "tool_input": { "file_path": a } })
            .to_string();
        // Oversized hook input is cut and does not parse: nothing recorded.
        let huge = format!("{}{hook}", " ".repeat(MAX_HOOK_INPUT as usize));
        record_from_hook(huge.as_bytes(), Some(status.as_os_str())).unwrap();
        let files = files_for(&status);
        assert!(!files.exists() || read(&files).is_empty());
        // Past the limit the list starts over with the new line.
        std::fs::write(&files, "x".repeat(MAX_FILE_BYTES as usize + 10)).unwrap();
        record_from_hook(hook.as_bytes(), Some(status.as_os_str())).unwrap();
        assert!(std::fs::metadata(&files).unwrap().len() < 1000);
        assert_eq!(read(&files).len(), 1);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn unchanged_lists_are_not_reread() {
        let (dir, status) = setup("stamp");
        let files = files_for(&status);
        assert!(read_changed(&files, None).is_none_or(|(_, l)| l.is_empty()));
        append(&files, Action::Read, &abs("a.rs")).unwrap();
        let (stamp, list) = read_changed(&files, None).unwrap();
        assert_eq!(list.len(), 1);
        assert!(read_changed(&files, stamp).is_none());
        std::fs::remove_file(&files).unwrap();
        assert_eq!(read_changed(&files, stamp).map(|(_, l)| l.len()), Some(0));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn a_cut_on_a_line_break_keeps_the_line() {
        let (dir, status) = setup("cut");
        let files = files_for(&status);
        let line = |i: usize| {
            format!(
                "{{\"action\":\"read\",\"path\":{:?}}}\n",
                abs(&format!("f{i}.rs"))
            )
        };
        // Lines of equal length; pad so the window starts exactly after a break.
        let one = line(0).len() as u64;
        let count = (TAIL_BYTES / one) as usize + 3;
        let text: String = (0..count).map(line).collect();
        let pad = (TAIL_BYTES - (TAIL_BYTES / one) * one) as usize;
        std::fs::write(&files, format!("{}\n{text}", "p".repeat(pad))).unwrap();
        let list = read(&files);
        assert!(!list.is_empty());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn parsing_is_tolerant() {
        let a = abs("a.rs");
        let text = format!(
            "{{\"action\":\"read\",\"path\":{a:?}}}\nbroken\n{{\"action\":\"delete\",\"path\":{a:?}}}\n{{\"action\":\"write\",\"path\":\"rel\"}}\n"
        );
        assert_eq!(parse(&text, false).len(), 1);
        // A cut first line is dropped.
        assert!(parse(&text, true).is_empty());
        let many: String = (0..80)
            .map(|i| {
                format!(
                    "{{\"action\":\"read\",\"path\":{:?}}}\n",
                    abs(&format!("f{i}.rs"))
                )
            })
            .collect();
        let list = parse(&many, false);
        assert_eq!(list.len(), MAX_LISTED);
        assert!(list[0].path.ends_with("f79.rs"));
    }
}
