//! Detects a project's `owner/repo` from its git `origin` remote, so new
//! projects get a `repo` for the Quests board without extra setup.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf, Prefix};

use crate::config::valid_repo;

/// Git files read here are small; anything bigger is not a git file.
const MAX_GIT_FILE: u64 = 1 << 20;

/// `owner/repo` of the `origin` remote of the repository at `dir`, if any.
/// Reads git's files directly: no `git` process is started.
///
/// The folder may be an untrusted clone, so redirections (`gitdir:`,
/// `commondir`) are only followed to local paths, and nothing is read
/// through a symlink or junction inside the folder: opening a UNC path such
/// as `\\host\share` makes Windows send the user's credentials to that host.
pub fn detect_repo(dir: &Path) -> Option<String> {
    let config = read_local(dir, &git_dir(dir)?.join("config"))?;
    let url = origin_url(&config)?;
    repo_from_url(&url).filter(|r| valid_repo(r))
}

/// The `.git` directory: a directory, or a file with `gitdir: <path>`
/// (worktrees and submodules). For a linked worktree the remotes live in the
/// main repository's config, found through `commondir`.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    let meta = std::fs::symlink_metadata(&dot_git).ok()?;
    let git_dir = if meta.is_dir() {
        dot_git
    } else if meta.is_file() {
        let text = read_local(dir, &dot_git)?;
        let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
        local(dir.join(target))?
    } else {
        // A symlinked .git could point anywhere.
        return None;
    };
    match read_local(dir, &git_dir.join("commondir")) {
        Some(common) => local(git_dir.join(common.trim())),
        None => Some(git_dir),
    }
}

/// `path` if it is on a local drive (not UNC, not a device path).
fn local(path: PathBuf) -> Option<PathBuf> {
    let text = path.to_string_lossy();
    if text.starts_with("\\\\") || text.starts_with("//") {
        return None;
    }
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) => Some(path),
            _ => None,
        },
        _ => Some(path),
    }
}

/// Contents of a regular local file of at most [`MAX_GIT_FILE`] bytes. Under
/// `base` (the folder the user chose, whose content may be untrusted) no
/// component may be a symlink or junction.
fn read_local(base: &Path, path: &Path) -> Option<String> {
    let path = local(path.to_path_buf())?;
    if goes_through_link(base, &path) || !std::fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(&path)
        .ok()?
        .take(MAX_GIT_FILE)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Whether `path`, below `base`, passes through a symlink or a junction (or
/// uses `..`, which could step back through one). Paths outside `base` are
/// the user's own and only checked for being local.
fn goes_through_link(base: &Path, path: &Path) -> bool {
    let Ok(rest) = path.strip_prefix(base) else {
        return false;
    };
    let mut current = base.to_path_buf();
    for component in rest.components() {
        let Component::Normal(part) = component else {
            return true;
        };
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if is_link(&meta) => return true,
            Ok(_) => {}
            // Missing: nothing to follow; the read itself will fail.
            Err(_) => return false,
        }
    }
    false
}

fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        // Junctions and other reparse points, not only symlinks.
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
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
    fn remote_paths_are_never_followed() {
        assert_eq!(local(PathBuf::from(r"\\127.0.0.1\share\x")), None);
        assert_eq!(local(PathBuf::from("//127.0.0.1/share/x")), None);
        assert_eq!(local(PathBuf::from(r"\\?\UNC\host\share")), None);
        assert!(local(PathBuf::from("relative/x")).is_some());

        let dir = temp_dir("unc");
        // Backslashes only make a UNC path on Windows.
        if cfg!(windows) {
            std::fs::write(dir.join(".git"), "gitdir: \\\\127.0.0.1\\share\\repo\n").unwrap();
            assert_eq!(git_dir(&dir), None);
            std::fs::remove_file(dir.join(".git")).unwrap();
        }
        // A local gitdir whose commondir points to the network.
        std::fs::create_dir_all(dir.join("gd")).unwrap();
        std::fs::write(dir.join(".git"), "gitdir: gd\n").unwrap();
        std::fs::write(dir.join("gd/commondir"), "//127.0.0.1/share\n").unwrap();
        assert_eq!(git_dir(&dir), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn links_inside_the_folder_are_not_followed() {
        let dir = temp_dir("link");
        let outside = temp_dir("link-target");
        std::fs::create_dir_all(outside.join("repo")).unwrap();
        std::fs::write(
            outside.join("repo/config"),
            "[remote \"origin\"]\n\turl = https://github.com/acme/app.git\n",
        )
        .unwrap();
        // `.git` file pointing through a link inside the folder.
        let link = dir.join("sub");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, &link).is_ok();
        #[cfg(windows)]
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .is_ok_and(|o| o.status.success());
        if made {
            std::fs::write(dir.join(".git"), "gitdir: sub/repo\n").unwrap();
            assert_eq!(detect_repo(&dir), None);
            // The same repository read directly is fine.
            std::fs::write(
                dir.join(".git"),
                format!("gitdir: {}\n", outside.join("repo").display()),
            )
            .unwrap();
            assert_eq!(detect_repo(&dir).as_deref(), Some("acme/app"));
        }
        assert!(goes_through_link(&dir, &dir.join("../x")));
        assert!(!goes_through_link(&dir, &dir.join("missing/x")));
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(outside);
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
