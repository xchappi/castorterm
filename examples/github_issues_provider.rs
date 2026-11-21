//! Example castorterm task provider for GitHub Issues.
//!
//! Implements the provider contract v1 (see `docs/providers.md`) on top of
//! the GitHub CLI (`gh`), which handles authentication. Build it with
//! `cargo build --example github_issues_provider` and point a provider at
//! the resulting binary:
//!
//! ```toml
//! [[providers]]
//! name = "github"
//! command = "/path/to/target/debug/examples/github_issues_provider"
//!
//! [[workspaces]]
//! name = "My workspace"
//! board = { provider = "github" }
//!
//! [[workspaces.projects]]
//! name = "app"
//! path = "~/src/app"
//! repo = "owner/app"
//! ```
//!
//! It lists the issues of every repository castorterm passes with
//! `--project` (the `repo` of each workspace project), or of the filter
//! (`owner/repo`) when there is none. Open issues are reported as `backlog`,
//! or `in_progress` when they have an assignee; recently closed ones as
//! `done`.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Issue {
    number: u64,
    title: String,
    url: String,
    state: String,
    #[serde(default)]
    assignees: Vec<serde_json::Value>,
    #[serde(default)]
    labels: Vec<Label>,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str =
    "usage: github_issues_provider list --filter <owner/repo> [--project <owner/repo>]...";

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repos = repositories(&args)?;

    // Never run `gh` by bare name from an arbitrary working directory: on
    // Windows that would also search the current directory, which may be an
    // untrusted clone. Resolve it from absolute PATH entries and run it from
    // a neutral directory.
    let gh = find_gh().ok_or("gh (GitHub CLI) not found in PATH")?;
    let mut tasks = Vec::new();
    for repo in &repos {
        tasks.extend(issues(&gh, repo, &id_prefix(repo, &repos))?);
    }
    Ok(json!({ "version": 1, "tasks": tasks }).to_string())
}

/// Repositories to list: every `--project`, or the filter when none is given.
fn repositories(args: &[String]) -> Result<Vec<String>, String> {
    let (filter, rest) = match args {
        [list, flag, filter, rest @ ..] if list == "list" && flag == "--filter" => (filter, rest),
        _ => return Err(USAGE.into()),
    };
    if rest.len() % 2 != 0 || rest.chunks(2).any(|pair| pair[0] != "--project") {
        return Err(USAGE.into());
    }
    let mut repos: Vec<String> = rest.chunks(2).map(|pair| pair[1].clone()).collect();
    if repos.is_empty() && !filter.is_empty() {
        repos.push(filter.clone());
    }
    if repos.is_empty() {
        return Err("no repository: set `repo` on the workspace projects or a filter".into());
    }
    if let Some(bad) = repos.iter().find(|r| r.is_empty() || r.starts_with('-')) {
        return Err(format!("{bad:?} is not a repository like owner/repo"));
    }
    Ok(repos)
}

/// Ids must be unique across the listing: with several repositories they
/// carry the repository name (`app#12`), or the full `owner/repo` when two
/// repositories share a name.
fn id_prefix(repo: &str, repos: &[String]) -> String {
    if repos.len() < 2 {
        return String::new();
    }
    let short = |r: &str| r.rsplit('/').next().unwrap_or(r).to_ascii_lowercase();
    let shared = repos.iter().filter(|r| short(r) == short(repo)).count() > 1;
    if shared {
        repo.to_string()
    } else {
        repo.rsplit('/').next().unwrap_or(repo).to_string()
    }
}

fn issues(
    gh: &std::path::Path,
    repo: &str,
    prefix: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let output = Command::new(gh)
        .current_dir(std::env::temp_dir())
        .args([
            "issue",
            "list",
            "--repo",
            repo,
            "--state",
            "all",
            "--limit",
            "100",
            "--json",
            "number,title,url,state,assignees,labels",
        ])
        .output()
        .map_err(|e| format!("could not run gh (is the GitHub CLI installed?): {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "gh failed for {repo}: {}",
            stderr.lines().next().unwrap_or("")
        ));
    }
    let issues: Vec<Issue> =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("unexpected gh output: {e}"))?;

    Ok(issues
        .into_iter()
        .map(|issue| {
            json!({
                "id": format!("{prefix}#{}", issue.number),
                "title": issue.title,
                "status": status(&issue),
                "priority": priority(&issue),
                "project": repo,
                "url": issue.url,
            })
        })
        .collect())
}

/// Absolute path of `gh`, searching only absolute `PATH` entries.
fn find_gh() -> Option<PathBuf> {
    let name = if cfg!(windows) { "gh.exe" } else { "gh" };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn status(issue: &Issue) -> &'static str {
    if issue.state.eq_ignore_ascii_case("closed") {
        "done"
    } else if !issue.assignees.is_empty() {
        "in_progress"
    } else {
        "backlog"
    }
}

/// Uses labels named like `P0`..`P3` (or `priority: P1`) when present.
fn priority(issue: &Issue) -> Option<String> {
    issue.labels.iter().find_map(|label| {
        let name = label.name.trim();
        let value = name
            .rsplit(':')
            .next()
            .unwrap_or(name)
            .trim()
            .to_uppercase();
        matches!(value.as_str(), "P0" | "P1" | "P2" | "P3").then_some(value)
    })
}
