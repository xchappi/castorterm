//! The built-in board for workspaces without a task provider: tasks kept in
//! the projects themselves, so the board works with no setup and nothing
//! leaves the machine.
//!
//! - `TASKS.md` at a project's root: checklist items (`- [ ] title`,
//!   `- [x] done`), with the status taken from the section they are under
//!   (`## In progress`, `## Review`, `## Backlog`, `## Done`...).
//! - OpenSpec changes in progress (`openspec/changes/<id>/tasks.md`): one
//!   task per change, with how many of its steps are done.

use std::path::{Path, PathBuf};

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
/// Linked git worktrees (a `.git` file, as for task worktrees) are skipped:
/// they hold the same files as their project. Most pressing tasks first
/// when there are too many.
pub fn list(projects: &[(String, PathBuf)]) -> Listing {
    let mut tasks = Vec::new();
    for (name, root) in projects {
        if root.join(".git").is_file() {
            continue;
        }
        if let Some(text) = read_plain(&root.join("TASKS.md")) {
            tasks.extend(from_tasks_md(&text, name));
        }
        tasks.extend(openspec_changes(root, name));
    }
    let mut seen = std::collections::HashSet::new();
    tasks.retain(|t| seen.insert(t.id.clone()));
    let omitted = tasks.len().saturating_sub(crate::board::MAX_TASKS);
    if omitted > 0 {
        // Stable: within a status, file order is kept.
        tasks.sort_by_key(|t| Status::ALL.iter().position(|s| *s == t.status));
        tasks.truncate(crate::board::MAX_TASKS);
    }
    Listing {
        tasks,
        skipped: 0,
        omitted,
    }
}

/// Status of a `TASKS.md` section heading, if one of its words names one.
/// Earlier rows win when a heading names several ("Review / Done").
fn section_status(heading: &str) -> Option<Status> {
    let lower = heading.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |names: &[&str]| words.iter().any(|w| names.contains(w));
    let phrase = |p: &str| lower.contains(p);
    if phrase("in progress") || phrase("en curso") || has(&["doing", "wip", "progress", "ongoing"])
    {
        Some(Status::InProgress)
    } else if has(&["review", "revision", "revisión", "reviewing"]) {
        Some(Status::Review)
    } else if has(&[
        "done",
        "hecho",
        "hechas",
        "hechos",
        "completed",
        "completado",
        "completadas",
        "terminado",
        "terminadas",
        "finished",
    ]) {
        Some(Status::Done)
    } else if has(&["backlog", "later", "someday", "ideas", "futuro", "future"]) {
        Some(Status::Backlog)
    } else if phrase("to do")
        || has(&[
            "todo",
            "ready",
            "next",
            "pendiente",
            "pendientes",
            "tareas",
            "tasks",
        ])
    {
        Some(Status::Ready)
    } else {
        None
    }
}

/// `## Heading` → (level, text); `#tag` is not a heading.
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    let rest = &line[level..];
    ((1..=6).contains(&level) && rest.starts_with(' ')).then(|| (level, rest.trim()))
}

/// A checklist item: (checked, title).
fn checkbox(line: &str) -> Option<(bool, &str)> {
    let t = line.trim_start();
    let item = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .or_else(|| t.strip_prefix("+ "))?;
    match item.get(..3)? {
        "[ ]" => Some((false, &item[3..])),
        "[x]" | "[X]" => Some((true, &item[3..])),
        _ => None,
    }
}

/// Lines outside code fences (``` and ~~~), with their 1-based number.
fn prose_lines(text: &str) -> impl Iterator<Item = &str> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut in_code = false;
    text.lines().filter(move |line| {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
            return false;
        }
        !in_code
    })
}

/// Tasks of a `TASKS.md`: each checklist item, its status from the
/// enclosing section (ready by default; a sub-heading that names no status
/// keeps its parent's), done when checked. The id is the project and a
/// hash of the title, so it survives edits elsewhere in the file.
pub fn from_tasks_md(text: &str, project: &str) -> Vec<Task> {
    // Stack of (heading level, status it sets).
    let mut sections: Vec<(usize, Status)> = Vec::new();
    let mut tasks: Vec<Task> = Vec::new();
    for line in prose_lines(text) {
        let t = line.trim();
        if let Some((level, text)) = heading(t) {
            while sections.last().is_some_and(|(l, _)| *l >= level) {
                sections.pop();
            }
            let inherited = sections.last().map(|(_, s)| *s);
            let status = section_status(text).or(inherited).unwrap_or(Status::Ready);
            sections.push((level, status));
            continue;
        }
        let Some((checked, title)) = checkbox(line) else {
            continue;
        };
        let title = clean_title(title);
        if title.is_empty() {
            continue;
        }
        let section = sections.last().map_or(Status::Ready, |(_, s)| *s);
        let base = format!("{project}/TASKS.md#{:08x}", fnv(&title.to_lowercase()));
        // The same title twice: number the repeats.
        let repeats = tasks.iter().filter(|t| t.id.starts_with(&base)).count();
        let id = if repeats == 0 {
            base
        } else {
            format!("{base}-{}", repeats + 1)
        };
        tasks.push(Task {
            id,
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
    if !plain_dir(&dir) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut changes: Vec<String> = entries
        .flatten()
        .filter(|e| plain_dir(&e.path()))
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|name| name != "archive")
        .collect();
    changes.sort();
    changes.truncate(MAX_CHANGES);
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
    for line in prose_lines(tasks_md) {
        if let Some((checked, _)) = checkbox(line) {
            total += 1;
            if checked {
                done += 1;
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
    let id = crate::board::clean(id);
    Task {
        id: format!("openspec:{project}:{id}"),
        title: clean_title(&format!("OpenSpec change {id} ({done}/{total} tasks done)")),
        status,
        priority: None,
        project: Some(project.to_string()),
        url: None,
    }
}

fn clean_title(title: &str) -> String {
    let clean = crate::board::clean(title.trim());
    match clean.char_indices().nth(MAX_TITLE_CHARS) {
        Some((cut, _)) => format!("{}\u{2026}", &clean[..cut]),
        None => clean,
    }
}

/// 32-bit FNV-1a: a short, stable id part.
fn fnv(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// A folder that is not a link (symlink or junction).
fn plain_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !crate::links::is_link(&m))
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

    fn statuses(md: &str) -> Vec<(String, Status)> {
        from_tasks_md(md, "app")
            .into_iter()
            .map(|t| (t.title, t.status))
            .collect()
    }

    #[test]
    fn tasks_md_sections_and_checks() {
        let md = "# Tasks\n- [ ] write docs\n- not a task\n\n## In progress\n- [ ] add login\n- [x] set up CI\n\n## Backlog\n* [ ] dark mode\n\n## Done\n- [x] init repo\n- [ ]   \n";
        assert_eq!(
            statuses(md),
            vec![
                ("write docs".into(), Status::Ready),
                ("add login".into(), Status::InProgress),
                ("set up CI".into(), Status::Done),
                ("dark mode".into(), Status::Backlog),
                ("init repo".into(), Status::Done),
            ]
        );
        let tasks = from_tasks_md(md, "app");
        assert!(tasks.iter().all(|t| t.project.as_deref() == Some("app")));
        assert!(tasks[0].id.starts_with("app/TASKS.md#"));
        // Control characters never reach a title (it goes into a prompt).
        assert_eq!(from_tasks_md("- [ ] a\u{1b}[31mb", "p")[0].title, "a [31mb");
        // Long titles are cut with a mark.
        let long = from_tasks_md(&format!("- [ ] {}", "x".repeat(300)), "p");
        assert!(long[0].title.ends_with('\u{2026}'));
    }

    #[test]
    fn sub_headings_fences_and_words() {
        let md = "## In progress\n### Auth\n- [ ] login\n```\n# build\n- [ ] not a task\n```\n- [ ] logout\n## Preview builds\n- [ ] a\n## Abandoned\n- [ ] b\n## Tareas pendientes\n- [ ] c\n#tag\n- [ ] d\n";
        assert_eq!(
            statuses(md),
            vec![
                ("login".into(), Status::InProgress),
                ("logout".into(), Status::InProgress),
                ("a".into(), Status::Ready),
                ("b".into(), Status::Ready),
                ("c".into(), Status::Ready),
                ("d".into(), Status::Ready),
            ]
        );
        assert_eq!(
            statuses("\u{FEFF}## Done\r\n- [ ] x\r\n")[0].1,
            Status::Done
        );
    }

    #[test]
    fn ids_survive_edits() {
        let before = from_tasks_md("- [ ] one\n- [ ] two", "app");
        let after = from_tasks_md("- [ ] new first\n\n- [ ] two\n- [ ] one", "app");
        let id =
            |list: &[Task], title: &str| list.iter().find(|t| t.title == title).unwrap().id.clone();
        assert_eq!(id(&before, "one"), id(&after, "one"));
        assert_eq!(id(&before, "two"), id(&after, "two"));
        // Repeated titles get distinct ids.
        let twice = from_tasks_md("- [ ] same\n- [ ] same", "app");
        assert_ne!(twice[0].id, twice[1].id);
    }

    #[test]
    fn openspec_progress() {
        let t = change_task("add-x", "## 1\n- [x] a\n  - [ ] b\n* [ ] c", "app");
        assert_eq!(t.status, Status::InProgress);
        assert_eq!(t.title, "OpenSpec change add-x (1/3 tasks done)");
        assert_eq!(t.id, "openspec:app:add-x");
        assert_eq!(
            change_task("y", "\u{FEFF}- [x] a", "app").status,
            Status::Review
        );
        assert_eq!(change_task("z", "nothing", "app").status, Status::Ready);
    }

    #[test]
    fn listing_from_projects() {
        let base = std::env::temp_dir().join(format!("castorterm-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("app");
        let change = root.join("openspec").join("changes").join("feat-a");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::create_dir_all(root.join("openspec/changes/archive/old")).unwrap();
        std::fs::write(change.join("tasks.md"), "- [ ] one").unwrap();
        std::fs::write(root.join("TASKS.md"), "- [ ] fix bug").unwrap();
        // A linked worktree of the same project: skipped.
        let worktree = base.join("app@task");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "gitdir: x").unwrap();
        std::fs::write(worktree.join("TASKS.md"), "- [ ] fix bug").unwrap();
        let listing = list(&[("app".into(), root.clone()), ("app@task".into(), worktree)]);
        let ids: Vec<&str> = listing.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert!(ids[0].starts_with("app/TASKS.md#"));
        assert_eq!(ids[1], "openspec:app:feat-a");
        assert!(is_local(&board()));
        let _ = std::fs::remove_dir_all(&base);
    }
}
