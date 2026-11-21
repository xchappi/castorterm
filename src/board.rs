//! Task board: the provider contract (v1), grouping, matching tasks to
//! projects and building the command that starts an agent on a task.
//! Everything a provider returns is untrusted.

use serde::Deserialize;

use crate::config::PROMPT_PLACEHOLDER;
use crate::files::quote_for_shell;

/// Contract version this build understands.
pub const PROTOCOL_VERSION: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Status {
    InProgress,
    Review,
    Ready,
    Backlog,
    Done,
}

impl Status {
    /// Display order on the board.
    pub const ALL: [Status; 5] = [
        Status::InProgress,
        Status::Review,
        Status::Ready,
        Status::Backlog,
        Status::Done,
    ];

    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "in_progress" => Status::InProgress,
            "review" => Status::Review,
            "ready" => Status::Ready,
            "backlog" => Status::Backlog,
            "done" => Status::Done,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::InProgress => "In progress",
            Status::Review => "Review",
            Status::Ready => "Ready",
            Status::Backlog => "Backlog",
            Status::Done => "Done",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Option<String>,
    pub project: Option<String>,
    /// Only `http`/`https` links are kept.
    pub url: Option<String>,
}

/// A parsed provider listing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub tasks: Vec<Task>,
    /// Tasks dropped for missing fields or unknown statuses.
    pub skipped: usize,
}

#[derive(Deserialize)]
struct RawListing {
    version: u64,
    #[serde(default)]
    tasks: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawTask {
    id: String,
    title: String,
    status: String,
    priority: Option<String>,
    project: Option<String>,
    url: Option<String>,
}

/// Parses a provider's stdout. Malformed tasks are skipped, not fatal.
pub fn parse_listing(stdout: &str) -> Result<Listing, String> {
    let raw: RawListing =
        serde_json::from_str(stdout).map_err(|e| format!("invalid provider output: {e}"))?;
    if raw.version != PROTOCOL_VERSION {
        return Err(format!(
            "unsupported provider protocol version {} (expected {PROTOCOL_VERSION})",
            raw.version
        ));
    }
    let mut listing = Listing::default();
    for value in raw.tasks {
        match serde_json::from_value::<RawTask>(value)
            .ok()
            .and_then(validate)
        {
            Some(task) => listing.tasks.push(task),
            None => listing.skipped += 1,
        }
    }
    Ok(listing)
}

fn validate(raw: RawTask) -> Option<Task> {
    let id = clean(&raw.id);
    let title = clean(&raw.title);
    if id.is_empty() || title.is_empty() {
        return None;
    }
    Some(Task {
        id,
        title,
        status: Status::parse(raw.status.trim())?,
        priority: raw.priority.map(|p| clean(&p)).filter(|p| !p.is_empty()),
        project: raw.project.map(|p| clean(&p)).filter(|p| !p.is_empty()),
        url: raw
            .url
            .map(|u| u.trim().to_string())
            .filter(|u| is_web_link(u)),
    })
}

/// Collapses control characters (newlines included) into spaces and trims.
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_web_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Tasks grouped in board order; empty groups are omitted.
pub fn group(tasks: &[Task]) -> Vec<(Status, Vec<&Task>)> {
    Status::ALL
        .iter()
        .filter_map(|&status| {
            let tasks: Vec<&Task> = tasks.iter().filter(|t| t.status == status).collect();
            (!tasks.is_empty()).then_some((status, tasks))
        })
        .collect()
}

/// Index of the project a task belongs to: by `repo`, then by name,
/// ignoring case. `projects` yields `(name, repo)`.
pub fn match_project<'a>(
    task_project: Option<&str>,
    projects: impl Iterator<Item = (usize, &'a str, Option<&'a str>)> + Clone,
) -> Option<usize> {
    let wanted = task_project?;
    let by_repo = projects
        .clone()
        .find(|(_, _, repo)| repo.is_some_and(|r| r.eq_ignore_ascii_case(wanted)));
    by_repo
        .or_else(|| {
            projects
                .clone()
                .find(|(_, name, _)| name.eq_ignore_ascii_case(wanted))
        })
        .map(|(index, _, _)| index)
}

/// Fills `{id}`, `{title}` and `{url}` in a prompt template.
pub fn render_prompt(template: &str, task: &Task) -> String {
    let rendered = template
        .replace("{id}", &task.id)
        .replace("{title}", &task.title)
        .replace("{url}", task.url.as_deref().unwrap_or(""));
    clean(&rendered)
}

/// The command line typed into the shell: the agent command followed by its
/// `task_args`, with the prompt substituted and every argument quoted as a
/// single literal.
pub fn agent_command(command: &str, task_args: &[String], prompt: &str, windows: bool) -> String {
    let mut line = command.to_string();
    for arg in task_args {
        let arg = arg.replace(PROMPT_PLACEHOLDER, prompt);
        line.push(' ');
        line.push_str(&quote_for_shell(&arg, windows));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: Status) -> Task {
        Task {
            id: id.into(),
            title: format!("Task {id}"),
            status,
            priority: None,
            project: None,
            url: None,
        }
    }

    #[test]
    fn parses_a_valid_listing() {
        let json = r#"{"version":1,"tasks":[
            {"id":"CL-1","title":"Fix login","status":"ready","priority":"P1",
             "project":"x/app","url":"https://example.com/CL-1"}
        ]}"#;
        let listing = parse_listing(json).unwrap();
        assert_eq!(listing.skipped, 0);
        let t = &listing.tasks[0];
        assert_eq!(t.id, "CL-1");
        assert_eq!(t.status, Status::Ready);
        assert_eq!(t.priority.as_deref(), Some("P1"));
        assert_eq!(t.project.as_deref(), Some("x/app"));
        assert_eq!(t.url.as_deref(), Some("https://example.com/CL-1"));
    }

    #[test]
    fn invalid_tasks_are_skipped() {
        let json = r#"{"version":1,"tasks":[
            {"id":"a","title":"ok","status":"backlog"},
            {"id":"b","title":"x","status":"cancelled"},
            {"id":"c","status":"ready"},
            {"id":"","title":"empty id","status":"ready"},
            42
        ]}"#;
        let listing = parse_listing(json).unwrap();
        assert_eq!(listing.tasks.len(), 1);
        assert_eq!(listing.skipped, 4);
    }

    #[test]
    fn version_and_syntax_errors() {
        assert!(
            parse_listing(r#"{"version":2,"tasks":[]}"#)
                .unwrap_err()
                .contains("unsupported")
        );
        assert!(parse_listing("not json").is_err());
        assert!(parse_listing(r#"{"tasks":[]}"#).is_err());
        assert_eq!(
            parse_listing(r#"{"version":1}"#).unwrap(),
            Listing::default()
        );
    }

    #[test]
    fn untrusted_text_is_cleaned() {
        let json = "{\"version\":1,\"tasks\":[{\"id\":\"X\",\"title\":\"line\\nrm -rf ~\\u001b[2J\",\"status\":\"ready\",\"url\":\"javascript:alert(1)\"}]}";
        let t = &parse_listing(json).unwrap().tasks[0];
        assert_eq!(t.title, "line rm -rf ~ [2J");
        assert_eq!(t.url, None);
    }

    #[test]
    fn only_web_links() {
        assert!(is_web_link("https://a.b/c"));
        assert!(is_web_link("HTTP://a.b"));
        assert!(!is_web_link("file:///etc/passwd"));
        assert!(!is_web_link("https://a b"));
    }

    #[test]
    fn groups_follow_board_order() {
        let tasks = vec![
            task("1", Status::Done),
            task("2", Status::Backlog),
            task("3", Status::InProgress),
            task("4", Status::Backlog),
        ];
        let groups = group(&tasks);
        let order: Vec<_> = groups.iter().map(|(s, t)| (*s, t.len())).collect();
        assert_eq!(
            order,
            [
                (Status::InProgress, 1),
                (Status::Backlog, 2),
                (Status::Done, 1)
            ]
        );
    }

    #[test]
    fn project_matching_prefers_repo_then_name() {
        let projects = [
            (0usize, "castorterm", Some("xchappi/castorterm")),
            (1, "xchappi/other", None),
            (2, "stack", Some("xchappi/castorlabs-stack")),
        ];
        let iter = || projects.iter().map(|&(i, n, r)| (i, n, r));
        assert_eq!(match_project(Some("XCHAPPI/castorterm"), iter()), Some(0));
        assert_eq!(match_project(Some("xchappi/other"), iter()), Some(1));
        assert_eq!(match_project(Some("stack"), iter()), Some(2));
        assert_eq!(match_project(Some("unknown"), iter()), None);
        assert_eq!(match_project(None, iter()), None);
    }

    #[test]
    fn prompt_rendering() {
        let mut t = task("CL-9", Status::Ready);
        t.url = Some("https://x/CL-9".into());
        assert_eq!(
            render_prompt("Work on task {id}: {title} ({url})", &t),
            "Work on task CL-9: Task CL-9 (https://x/CL-9)"
        );
        t.url = None;
        assert_eq!(render_prompt("{id} {url}", &t), "CL-9");
    }

    #[test]
    fn agent_command_quotes_each_argument() {
        let args = vec!["--prompt".to_string(), "{prompt}".to_string()];
        assert_eq!(
            agent_command("opencode", &args, "Work on CL-1: it's done", true),
            "opencode --prompt 'Work on CL-1: it''s done'"
        );
        assert_eq!(
            agent_command("claude", &["{prompt}".into()], "a;b", false),
            "claude 'a;b'"
        );
    }

    #[test]
    fn crafted_title_cannot_escape_the_argument() {
        let mut t = task("X", Status::Ready);
        t.title = clean("x\u{2019}; Start-Process calc; \u{2018}y\nrm");
        let prompt = render_prompt("{title}", &t);
        let line = agent_command("claude", &["{prompt}".into()], &prompt, true);
        assert_eq!(
            line,
            "claude 'x\u{2019}\u{2019}; Start-Process calc; \u{2018}\u{2018}y rm'"
        );
    }
}
