//! The built-in board for workspaces without a task provider: tasks kept in
//! the projects themselves, so the board works with no setup and nothing
//! leaves the machine.
//!
//! - `TASKS.md` at a project's root: checklist items (`- [ ] title`,
//!   `- [x] done`), with the status taken from the section they are under
//!   (`## In progress`, `## Review`, `## Backlog`, `## Done`...).
//! - OpenSpec changes in progress (`openspec/changes/<id>/tasks.md`): one
//!   task per change, with how many of its steps are done.

use std::path::Path;

use crate::board::{Listing, Status, Task};
use crate::config::{Board, Provider};

/// Name of the built-in provider (it runs no command).
pub const PROVIDER_NAME: &str = "local";
/// Prompt for local tasks: the title is the task itself.
const PROMPT: &str = "Work on this task: {title} ({id})";
/// Files read at most, and their size limit.
const MAX_BYTES: u64 = 256 * 1024;
const MAX_CHANGES: usize = 100;
/// Task titles are shortened to this.
const MAX_TITLE_CHARS: usize = 200;

/// The board of a workspace that configures none.
pub fn board() -> Board {
    Board {
        provider: Provider {
            name: PROVIDER_NAME.into(),
            command: String::new(),
            args: Vec::new(),
        },
        filter: String::new(),
        prompt: PROMPT.into(),
        refresh_secs: 30,
    }
}

/// Whether `board` is the built-in local one.
pub fn is_local(board: &Board) -> bool {
    board.provider.command.is_empty() && board.provider.name == PROVIDER_NAME
}

/// The local tasks of `projects` (`(name, root)`), read off the UI thread.
pub fn list(projects: &[(String, std::path::PathBuf)]) -> Listing {
    let mut tasks = Vec::new();
    for (name, root) in projects {
        if let Some(text) = read_plain(&root.join("TASKS.md")) {
            tasks.extend(from_tasks_md(&text, name));
        }
        tasks.extend(openspec_changes(root, name));
    }
    let omitted = tasks.len().saturating_sub(crate::board::MAX_TASKS);
    tasks.truncate(crate::board::MAX_TASKS);
    Listing {
        tasks,
        skipped: 0,
        omitted,
    }
}

/// Status of a `TASKS.md` section heading, if it names one.
fn section_status(heading: &str) -> Option<Status> {
    let h = heading.trim().to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| h.contains(w));
    if has(&["progress", "doing", "curso", "wip"]) {
        Some(Status::InProgress)
    } else if has(&["review", "revision", "revisión"]) {
        Some(Status::Review)
    } else if has(&["backlog", "later", "someday", "ideas", "pendiente"]) {
        Some(Status::Backlog)
    } else if has(&["done", "hecho", "completed", "terminad"]) {
        Some(Status::Done)
    } else if has(&["todo", "to do", "ready", "next", "tareas", "tasks"]) {
        Some(Status::Ready)
    } else {
        None
    }
}

/// Tasks of a `TASKS.md`: each checklist item, its status from the
/// enclosing section (ready by default), done when checked. The id is the
/// file and line, so a prompt can point the agent to it.
pub fn from_tasks_md(text: &str, project: &str) -> Vec<Task> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut section = Status::Ready;
    let mut tasks = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let t = line.trim();
        if let Some(heading) = t.strip_prefix('#') {
            section = section_status(heading.trim_start_matches('#')).unwrap_or(Status::Ready);
            continue;
        }
        let Some(item) = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
        else {
            continue;
        };
        let (checked, title) = match item.get(..3) {
            Some("[ ]") => (false, &item[3..]),
            Some("[x]") | Some("[X]") => (true, &item[3..]),
            _ => continue,
        };
        let title = clean_title(title);
        if title.is_empty() {
            continue;
        }
        tasks.push(Task {
            id: format!("{project}/TASKS.md:{}", index + 1),
            title,
            status: if checked { Status::Done } else { section },
            priority: None,
            project: Some(project.to_string()),
            url: None,
        });
    }
    tasks
}

/// One task per OpenSpec change in progress, with its progress.
fn openspec_changes(root: &Path, project: &str) -> Vec<Task> {
    let dir = root.join("openspec").join("changes");
    let plain =
        std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir() && !crate::links::is_link(&m));
    if !plain {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut changes: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|name| name != "archive")
        .take(MAX_CHANGES)
        .collect();
    changes.sort();
    changes
        .into_iter()
        .filter_map(|change| {
            let text = read_plain(&dir.join(&change).join("tasks.md"))?;
            Some(change_task(&change, &text, project))
        })
        .collect()
}

/// The task of OpenSpec change `id` from its `tasks.md`.
pub fn change_task(id: &str, tasks_md: &str, project: &str) -> Task {
    let (mut done, mut total) = (0, 0);
    for line in tasks_md.lines() {
        let t = line.trim_start();
        if let Some(item) = t.strip_prefix("- ") {
            match item.get(..3) {
                Some("[ ]") => total += 1,
                Some("[x]") | Some("[X]") => {
                    total += 1;
                    done += 1;
                }
                _ => {}
            }
        }
    }
    let status = if total > 0 && done == total {
        Status::Review
    } else if done > 0 {
        Status::InProgress
    } else {
        Status::Ready
    };
    Task {
        id: format!("openspec:{id}"),
        title: clean_title(&format!("OpenSpec change {id} ({done}/{total} tasks done)")),
        status,
        priority: None,
        project: Some(project.to_string()),
        url: None,
    }
}

fn clean_title(title: &str) -> String {
    let clean = crate::board::clean(title.trim());
    clean.chars().take(MAX_TITLE_CHARS).collect()
}

/// Text of a plain file (not a link), bounded.
fn read_plain(path: &Path) -> Option<String> {
    use std::io::Read as _;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || crate::links::is_link(&meta) {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_md_sections_and_checks() {
        let md = "# Tasks\n- [ ] write docs\n- not a task\n\n## In progress\n- [ ] add login\n- [x] set up CI\n\n## Backlog\n* [ ] dark mode\n\n## Done\n- [x] init repo\n- [ ]   \n";
        let tasks = from_tasks_md(md, "app");
        let got: Vec<(&str, Status, &str)> = tasks
            .iter()
            .map(|t| (t.title.as_str(), t.status, t.id.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("write docs", Status::Ready, "app/TASKS.md:2"),
                ("add login", Status::InProgress, "app/TASKS.md:6"),
                ("set up CI", Status::Done, "app/TASKS.md:7"),
                ("dark mode", Status::Backlog, "app/TASKS.md:10"),
                ("init repo", Status::Done, "app/TASKS.md:13"),
            ]
        );
        assert!(tasks.iter().all(|t| t.project.as_deref() == Some("app")));
        // Control characters never reach a title (it goes into a prompt).
        assert_eq!(from_tasks_md("- [ ] a\u{1b}[31mb", "p")[0].title, "a [31mb");
    }

    #[test]
    fn openspec_progress() {
        let t = change_task("add-x", "## 1\n- [x] a\n  - [ ] b\n- [ ] c", "app");
        assert_eq!(t.status, Status::InProgress);
        assert_eq!(t.title, "OpenSpec change add-x (1/3 tasks done)");
        assert_eq!(change_task("y", "- [x] a", "app").status, Status::Review);
        assert_eq!(change_task("z", "nothing", "app").status, Status::Ready);
    }

    #[test]
    fn listing_from_projects() {
        let root = std::env::temp_dir().join(format!("castorterm-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let change = root.join("openspec").join("changes").join("feat-a");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::create_dir_all(root.join("openspec/changes/archive/old")).unwrap();
        std::fs::write(change.join("tasks.md"), "- [ ] one").unwrap();
        std::fs::write(root.join("TASKS.md"), "- [ ] fix bug").unwrap();
        let listing = list(&[("app".into(), root.clone())]);
        let ids: Vec<&str> = listing.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["app/TASKS.md:1", "openspec:feat-a"]);
        assert!(is_local(&board()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
