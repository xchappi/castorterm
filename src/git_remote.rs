//! Reads a project's git state from disk: the `owner/repo` of its `origin`
//! remote (so new projects get a `repo` for the Quests board) and its
//! current branch (shown in the sidebar).

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

/// Longest branch name shown.
const MAX_BRANCH_CHARS: usize = 60;

/// Current branch of the repository at `dir`, or the short commit id when
/// HEAD is detached. Same safety rules as [`detect_repo`]; a project on a
/// network path shows no branch (it is polled every few seconds).
pub fn current_branch(dir: &Path) -> Option<String> {
    let head = read_local(dir, &own_git_dir(dir)?.join("HEAD"))?;
    let head = head.lines().next()?.trim();
    let name = match head.strip_prefix("ref:") {
        Some(reference) => {
            let reference = reference.trim();
            reference
                .strip_prefix("refs/heads/")
                .unwrap_or(reference)
                .to_string()
        }
        None if head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit()) => {
            format!("@{}", &head[..7])
        }
        None => return None,
    };
    // Git allows any Unicode in branch names: drop invisible and bidi
    // characters so a crafted name cannot fake what is shown.
    let name: String = name
        .chars()
        .filter(|&c| !crate::board::is_hidden(c))
        .take(MAX_BRANCH_CHARS)
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The repository's directory with the shared files (config): for a linked
/// worktree the main repository's, found through `commondir`.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let git_dir = own_git_dir(dir)?;
    match read_local(dir, &git_dir.join("commondir")) {
        Some(common) => local(git_dir.join(common.trim())),
        None => Some(git_dir),
    }
}

/// The `.git` directory of this checkout (where `HEAD` lives): a directory,
/// or a file with `gitdir: <path>` (worktrees and submodules).
fn own_git_dir(dir: &Path) -> Option<PathBuf> {
    // Checked before any disk access: even a stat of a UNC path sends
    // the user's credentials to its host.
    let dot_git = local(dir.join(".git"))?;
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
    Some(git_dir)
}

/// Whether `dir` is on a fixed local disk. On Windows mapped network
/// drives and removable media are skipped by the periodic branch refresh:
/// they can be slow or reach the network. Elsewhere always true.
pub fn on_fixed_drive(dir: &Path) -> bool {
    if local(dir.to_path_buf()).is_none() {
        return false;
    }
    #[cfg(windows)]
    {
        let Some(Component::Prefix(prefix)) = dir.components().next() else {
            return false;
        };
        let letter = match prefix.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) => l as char,
            _ => return false,
        };
        let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetDriveTypeW(root: *const u16) -> u32;
        }
        const DRIVE_FIXED: u32 = 3;
        // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
        unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
    }
    #[cfg(not(windows))]
    true
}

/// Names Windows maps to devices in any folder (`CON`, `NUL.txt`...).
fn is_device_name(part: &std::ffi::OsStr) -> bool {
    let name = part.to_string_lossy();
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.len() == 4
        && stem.as_bytes()[3].is_ascii_digit())
}

/// `path` if it is on a local drive (not UNC, not a device path, no
/// reserved device names on Windows).
fn local(path: PathBuf) -> Option<PathBuf> {
    let text = path.to_string_lossy();
    if text.starts_with("\\\\") || text.starts_with("//") {
        return None;
    }
    if cfg!(windows)
        && path
            .components()
            .any(|c| matches!(c, Component::Normal(part) if is_device_name(part)))
    {
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
    fn branch_names_are_cleaned_and_capped() {
        let dir = temp_dir("branch-clean");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/HEAD"),
            "ref: refs/heads/a\u{202E}b\u{200B}c\r\n",
        )
        .unwrap();
        assert_eq!(current_branch(&dir).as_deref(), Some("abc"));
        let long = "x".repeat(MAX_BRANCH_CHARS * 2);
        std::fs::write(dir.join(".git/HEAD"), format!("ref: refs/heads/{long}\n")).unwrap();
        assert_eq!(
            current_branch(&dir).unwrap().chars().count(),
            MAX_BRANCH_CHARS
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn network_and_device_paths_are_never_touched() {
        assert_eq!(current_branch(Path::new("//127.0.0.1/share/proj")), None);
        assert!(!on_fixed_drive(Path::new("//127.0.0.1/share/proj")));
        if cfg!(windows) {
            assert_eq!(current_branch(Path::new(r"\\127.0.0.1\share\proj")), None);
            assert!(is_device_name(std::ffi::OsStr::new("con")));
            assert!(is_device_name(std::ffi::OsStr::new("NUL.txt")));
            assert!(is_device_name(std::ffi::OsStr::new("COM1")));
            assert!(!is_device_name(std::ffi::OsStr::new("console")));
            assert_eq!(local(PathBuf::from(r"C:\repo\CON\x")), None);
            assert!(on_fixed_drive(&std::env::temp_dir()));
        }
    }

    #[test]
    fn branches_of_a_checkout_and_a_worktree() {
        let main = temp_dir("branch");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        std::fs::write(main.join(".git/HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        assert_eq!(current_branch(&main).as_deref(), Some("feat/x"));
        std::fs::write(
            main.join(".git/HEAD"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        assert_eq!(current_branch(&main).as_deref(), Some("@0123456"));
        std::fs::write(main.join(".git/HEAD"), "garbage\n").unwrap();
        assert_eq!(current_branch(&main), None);

        // A worktree has its own HEAD in its gitdir.
        let wt = temp_dir("branch-wt");
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/wt").display()),
        )
        .unwrap();
        std::fs::write(
            main.join(".git/worktrees/wt/HEAD"),
            "ref: refs/heads/task-1\n",
        )
        .unwrap();
        std::fs::write(main.join(".git/worktrees/wt/commondir"), "../..\n").unwrap();
        assert_eq!(current_branch(&wt).as_deref(), Some("task-1"));
        assert_eq!(current_branch(&temp_dir("no-git")), None);

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
