//! Detects a project's `owner/repo` from its git `origin` remote, so new
//! projects get a `repo` for the Quests board without extra setup.

use std::path::{Path, PathBuf};

use crate::config::valid_repo;

/// `owner/repo` of the `origin` remote of the repository at `dir`, if any.
/// Reads git's files directly: no `git` process is started.
pub fn detect_repo(dir: &Path) -> Option<String> {
    let config = std::fs::read_to_string(git_dir(dir)?.join("config")).ok()?;
    let url = origin_url(&config)?;
    repo_from_url(&url).filter(|r| valid_repo(r))
}

/// The `.git` directory: a directory, or a file with `gitdir: <path>`
/// (worktrees and submodules). For a linked worktree the remotes live in the
/// main repository's config, found through `commondir`.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
        let target = Path::new(target);
        if target.is_absolute() {
            target.to_path_buf()
        } else {
            dir.join(target)
        }
    };
    match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(common) => Some(git_dir.join(common.trim())),
        Err(_) => Some(git_dir),
    }
}

/// `url` of `[remote "origin"]` in a git config file.
fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let section: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            in_origin = section == "[remote\"origin\"]";
            continue;
        }
        if in_origin {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case("url") {
                return Some(value.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Last two path segments of a remote URL, without `.git`:
/// `https://host/owner/repo.git`, `ssh://git@host:22/owner/repo` and
/// `git@host:owner/repo.git` all give `owner/repo`.
fn repo_from_url(url: &str) -> Option<String> {
    let path = if let Some((_, rest)) = url.split_once("://") {
        rest.split_once('/')?.1
    } else if let Some((host, rest)) = url.split_once(':') {
        // scp-like syntax; a single letter is a Windows drive, not a host.
        if host.len() <= 1 || host.contains('/') {
            return None;
        }
        rest
    } else {
        return None;
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.rsplit('/').filter(|p| !p.is_empty());
    let repo = parts.next()?;
    let mut owner = parts.next()?;
    // Azure DevOps: org/project/_git/repo.
    if owner == "_git" {
        owner = parts.next()?;
    }
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        for (url, want) in [
            ("https://github.com/acme/app.git", Some("acme/app")),
            ("https://github.com/acme/app", Some("acme/app")),
            ("https://github.com/acme/app/", Some("acme/app")),
            ("git@github.com:acme/app.git", Some("acme/app")),
            (
                "ssh://git@gitlab.example.com:2222/group/sub/app.git",
                Some("sub/app"),
            ),
            (
                "https://user@dev.azure.com/org/proj/_git/app",
                Some("proj/app"),
            ),
            ("C:/repos/app", None),
            ("/srv/git/app.git", None),
            ("https://github.com/", None),
        ] {
            assert_eq!(repo_from_url(url).as_deref(), want, "{url}");
        }
    }

    #[test]
    fn origin_is_found_among_sections() {
        let config = r#"[core]
	bare = false
[remote "upstream"]
	url = https://github.com/other/app.git
[remote "origin"]
	fetch = +refs/heads/*:refs/remotes/origin/*
	url = git@github.com:acme/app.git
[branch "main"]
	remote = origin
"#;
        assert_eq!(
            origin_url(config).as_deref(),
            Some("git@github.com:acme/app.git")
        );
        assert_eq!(origin_url("[core]\n\tbare = false\n"), None);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("castorterm-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_from_a_repository_and_a_worktree() {
        let main = temp_dir("main");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        std::fs::write(
            main.join(".git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/acme/app.git\n",
        )
        .unwrap();
        assert_eq!(detect_repo(&main).as_deref(), Some("acme/app"));

        let wt = temp_dir("wt");
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/wt").display()),
        )
        .unwrap();
        std::fs::write(main.join(".git/worktrees/wt/commondir"), "../..\n").unwrap();
        assert_eq!(detect_repo(&wt).as_deref(), Some("acme/app"));

        std::fs::remove_dir_all(main).unwrap();
        std::fs::remove_dir_all(wt).unwrap();
    }

    #[test]
    fn unsafe_or_missing_remotes_give_none() {
        let dir = temp_dir("bad");
        assert_eq!(detect_repo(&dir), None);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/config"),
            "[remote \"origin\"]\n\turl = https://h/acme/a&b.git\n",
        )
        .unwrap();
        assert_eq!(detect_repo(&dir), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
