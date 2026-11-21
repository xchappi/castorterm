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
//! board = { provider = "github", filter = "owner/repo" }
//! ```
//!
//! The filter is the repository (`owner/repo`). Open issues are reported as
//! `backlog`, or `in_progress` when they have an assignee; recently closed
//! ones as `done`.

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

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = match args.as_slice() {
        [list, flag, repo] if list == "list" && flag == "--filter" => repo.clone(),
        _ => return Err("usage: github_issues_provider list --filter <owner/repo>".into()),
    };
    if repo.is_empty() || repo.starts_with('-') {
        return Err("the filter must be a repository like owner/repo".into());
    }

    let output = Command::new("gh")
        .args([
            "issue",
            "list",
            "--repo",
            &repo,
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
            "gh failed: {}",
            stderr.lines().next().unwrap_or("")
        ));
    }
    let issues: Vec<Issue> =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("unexpected gh output: {e}"))?;

    let tasks: Vec<_> = issues
        .into_iter()
        .map(|issue| {
            json!({
                "id": format!("#{}", issue.number),
                "title": issue.title,
                "status": status(&issue),
                "priority": priority(&issue),
                "project": repo,
                "url": issue.url,
            })
        })
        .collect();
    Ok(json!({ "version": 1, "tasks": tasks }).to_string())
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
