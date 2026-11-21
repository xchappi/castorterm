//! Task board: the provider contract (v1), grouping, matching tasks to
//! projects and building the command that starts an agent on a task.
//! Everything a provider returns is untrusted.

use std::collections::HashSet;
use std::time::Duration;

use serde::Deserialize;

use crate::config::PROMPT_PLACEHOLDER;
use crate::files::quote_for_shell;

/// Contract version this build understands.
pub const PROTOCOL_VERSION: u64 = 1;
/// Tasks kept per listing; the rest are counted as omitted.
pub const MAX_TASKS: usize = 500;
/// Longer titles are cut (in characters).
pub const MAX_TITLE_CHARS: usize = 300;

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
    /// Tasks dropped for missing fields, unknown statuses or duplicate ids.
    pub skipped: usize,
    /// Valid tasks beyond [`MAX_TASKS`].
    pub omitted: usize,
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
    let mut seen = HashSet::new();
    for value in raw.tasks {
        match serde_json::from_value::<RawTask>(value)
            .ok()
            .and_then(validate)
        {
            Some(task) if !seen.insert(task.id.clone()) => listing.skipped += 1,
            Some(_) if listing.tasks.len() >= MAX_TASKS => listing.omitted += 1,
            Some(task) => listing.tasks.push(task),
            None => listing.skipped += 1,
        }
    }
    Ok(listing)
}

fn validate(raw: RawTask) -> Option<Task> {
    // Ids are identifiers: reject rather than rewrite odd ones, so the id
    // shown and used in prompts is exactly the provider's.
    let id = raw.id.trim().to_string();
    if id.is_empty() || id.chars().any(|c| c.is_whitespace() || is_hidden(c)) {
        return None;
    }
    let title = truncate(&clean(&raw.title), MAX_TITLE_CHARS);
    if title.is_empty() {
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

/// Control characters and invisible formatting characters (bidi overrides,
/// zero-width characters) that could hide or reorder displayed text.
fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Collapses control and invisible formatting characters (newlines
/// included) into spaces and trims.
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if is_hidden(c) { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
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

/// Characters `cmd.exe` interprets, swapped for look-alikes. Agents
/// installed by npm on Windows are `.cmd` shims, and PowerShell passes
/// arguments to them without escaping embedded `"`, so a crafted title could
/// otherwise close the argument and run commands through `cmd.exe`.
fn neutralize_cmd(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '"' => '\u{201D}',
            '%' => '\u{FF05}',
            '&' => '\u{FF06}',
            '|' => '\u{FF5C}',
            '<' => '\u{FF1C}',
            '>' => '\u{FF1E}',
            '^' => '\u{FF3E}',
            '!' => '\u{FF01}',
            other => other,
        })
        .collect()
}

/// The command line typed into the shell: the agent command followed by its
/// `task_args`, with the prompt substituted and every argument quoted as a
/// single literal. The prompt (untrusted) never starts with `-`, so the
/// agent cannot read it as an option.
pub fn agent_command(command: &str, task_args: &[String], prompt: &str, windows: bool) -> String {
    let prompt = shell_safe_prompt(prompt, windows);
    let mut line = command.to_string();
    for arg in task_args {
        let arg = arg.replace(PROMPT_PLACEHOLDER, &prompt);
        line.push(' ');
        line.push_str(&quote_for_shell(&arg, windows));
    }
    line
}

/// The prompt exactly as the agent receives it: on Windows with `cmd.exe`
/// metacharacters neutralized and without a trailing backslash (it would
/// escape the closing quote PowerShell adds), and never starting with `-`.
/// Idempotent, so it can be shown to the user and then typed.
pub fn shell_safe_prompt(prompt: &str, windows: bool) -> String {
    let mut prompt = if windows {
        neutralize_cmd(prompt)
    } else {
        prompt.to_string()
    };
    if windows && prompt.ends_with('\\') {
        prompt.push(' ');
    }
    if prompt.starts_with('-') {
        prompt = format!("Task: {prompt}");
    }
    prompt
}

/// Rendered and shell-safe prompt for `task`; what the task details show.
pub fn final_prompt(template: &str, task: &Task, windows: bool) -> String {
    shell_safe_prompt(&render_prompt(template, task), windows)
}

/// Whether a board should be reloaded: never while a load is running;
/// otherwise when it was never loaded or `every` has passed.
pub fn refresh_due(loading: bool, since_load: Option<Duration>, every: Duration) -> bool {
    !loading && since_load.is_none_or(|elapsed| elapsed >= every)
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
            {"id":"PROJ-1","title":"Fix login","status":"ready","priority":"P1",
             "project":"x/app","url":"https://example.com/PROJ-1"}
        ]}"#;
        let listing = parse_listing(json).unwrap();
        assert_eq!(listing.skipped, 0);
        let t = &listing.tasks[0];
        assert_eq!(t.id, "PROJ-1");
        assert_eq!(t.status, Status::Ready);
        assert_eq!(t.priority.as_deref(), Some("P1"));
        assert_eq!(t.project.as_deref(), Some("x/app"));
        assert_eq!(t.url.as_deref(), Some("https://example.com/PROJ-1"));
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
    fn duplicate_and_odd_ids_are_skipped() {
        let json = r#"{"version":1,"tasks":[
            {"id":"A-1","title":"one","status":"ready"},
            {"id":"A-1","title":"dup","status":"ready"},
            {"id":"A 2","title":"space","status":"ready"},
            {"id":"A-3\u202e","title":"bidi","status":"ready"}
        ]}"#;
        let listing = parse_listing(json).unwrap();
        assert_eq!(listing.tasks.len(), 1);
        assert_eq!(listing.skipped, 3);
    }

    #[test]
    fn large_listings_and_titles_are_capped() {
        let long = "x".repeat(MAX_TITLE_CHARS + 50);
        let tasks: Vec<String> = (0..MAX_TASKS + 7)
            .map(|i| format!(r#"{{"id":"T-{i}","title":"{long}","status":"backlog"}}"#))
            .collect();
        let json = format!(r#"{{"version":1,"tasks":[{}]}}"#, tasks.join(","));
        let listing = parse_listing(&json).unwrap();
        assert_eq!(listing.tasks.len(), MAX_TASKS);
        assert_eq!(listing.omitted, 7);
        assert_eq!(listing.tasks[0].title.chars().count(), MAX_TITLE_CHARS + 1);
        assert!(listing.tasks[0].title.ends_with('…'));
    }

    #[test]
    fn invisible_characters_are_removed() {
        assert_eq!(clean("abc\u{202E}def\u{200B}g"), "abc def g");
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
        let mut t = task("PROJ-9", Status::Ready);
        t.url = Some("https://x/PROJ-9".into());
        assert_eq!(
            render_prompt("Work on task {id}: {title} ({url})", &t),
            "Work on task PROJ-9: Task PROJ-9 (https://x/PROJ-9)"
        );
        t.url = None;
        assert_eq!(render_prompt("{id} {url}", &t), "PROJ-9");
    }

    #[test]
    fn agent_command_quotes_each_argument() {
        let args = vec!["--prompt".to_string(), "{prompt}".to_string()];
        assert_eq!(
            agent_command("opencode", &args, "Work on PROJ-1: it's done", true),
            "opencode --prompt 'Work on PROJ-1: it''s done'"
        );
        assert_eq!(
            agent_command("claude", &["{prompt}".into()], "a;b", false),
            "claude 'a;b'"
        );
    }

    #[test]
    fn cmd_metacharacters_are_neutralized_on_windows() {
        // A .cmd shim would run `calc` through cmd.exe if `"` and `&` survived.
        let line = agent_command(
            "claude",
            &["{prompt}".into()],
            "x\" & calc & \"y %PATH%",
            true,
        );
        assert_eq!(
            line,
            "claude 'x\u{201D} \u{FF06} calc \u{FF06} \u{201D}y \u{FF05}PATH\u{FF05}'"
        );
        for bad in ['"', '%', '&', '|', '<', '>', '^', '!'] {
            assert!(!line.contains(bad), "{bad}");
        }
        // POSIX shells only need the single-quote rules.
        assert_eq!(
            agent_command("claude", &["{prompt}".into()], "a & b", false),
            "claude 'a & b'"
        );
    }

    #[test]
    fn shown_prompt_is_what_the_agent_receives() {
        let mut t = task("X-1", Status::Ready);
        t.title = r#"say "hi" & leave C:\tmp\"#.into();
        let shown = final_prompt("{title}", &t, true);
        assert_eq!(shown, "say \u{201D}hi\u{201D} \u{FF06} leave C:\\tmp\\ ");
        // Typing it changes nothing more: the preview is exact.
        assert_eq!(shell_safe_prompt(&shown, true), shown);
        assert_eq!(
            agent_command(
                "claude",
                &["{prompt}".into()],
                &render_prompt("{title}", &t),
                true
            ),
            agent_command("claude", &["{prompt}".into()], &shown, true)
        );
    }

    #[test]
    fn prompt_never_starts_with_a_dash() {
        assert_eq!(
            agent_command("claude", &["{prompt}".into()], "--dangerous", false),
            "claude 'Task: --dangerous'"
        );
    }

    #[test]
    fn refresh_timing() {
        let every = Duration::from_secs(60);
        assert!(refresh_due(false, None, every));
        assert!(!refresh_due(true, None, every));
        assert!(!refresh_due(false, Some(Duration::from_secs(10)), every));
        assert!(refresh_due(false, Some(Duration::from_secs(60)), every));
        assert!(!refresh_due(true, Some(Duration::from_secs(600)), every));
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
