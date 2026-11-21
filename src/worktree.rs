//! One git worktree per task, so several agents can work on the same
//! repository at once without touching each other's files.
//!
//! The worktree of task `CL-7` in project `/src/app` is `/src/app.worktrees/cl-7`
//! on branch `task/cl-7`. git runs without a shell, from the project
//! directory (the user's own repository), with a timeout.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Creating a worktree checks out the whole tree; hooks are disabled, so
/// anything slower than this is stuck.
pub const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_SLUG: usize = 40;
/// git output kept for error messages.
const MAX_STDERR: u64 = 16 * 1024;

/// File-name and branch-safe form of a task id: lowercase ASCII letters,
/// digits, `.`, `_` and `-` (others become `-`), no `..`, never starting
/// with `-` or `.`, never ending in `.lock`, never a Windows device name.
/// When the id had to change, a short hash of it is appended so different
/// ids (`#12` and `12`) never share a worktree.
pub fn slug(id: &str) -> String {
    let id = id.trim();
    let mut out = String::new();
    for c in id.chars() {
        let c = c.to_ascii_lowercase();
        let keep = c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
        let c = if keep { c } else { '-' };
        let repeated = matches!(c, '-' | '.') && (out.ends_with('-') || out.ends_with('.'));
        if !repeated {
            out.push(c);
        }
        if out.len() >= MAX_SLUG {
            break;
        }
    }
    let mut out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    if let Some(stem) = out.strip_suffix(".lock") {
        out = format!("{stem}-lock");
    }
    if out.is_empty() {
        out = "task".into();
    }
    let stem = out.split('.').next().unwrap_or("");
    let reserved = matches!(stem, "con" | "prn" | "aux" | "nul")
        || ((stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        out = format!("t-{out}");
    }
    // Case alone does not count: tracker ids differing only in case are
    // the same task.
    if out != id.to_ascii_lowercase() {
        out = format!("{out}-{:04x}", short_hash(id));
    }
    out
}

/// FNV-1a folded to 16 bits: stable across runs and platforms.
fn short_hash(text: &str) -> u16 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    ((hash >> 16) ^ (hash & 0xffff)) as u16
}

/// Where the worktree of `task_id` lives and its branch.
pub fn plan(project: &Path, task_id: &str) -> Option<(PathBuf, String)> {
    let name = project.file_name()?.to_string_lossy().into_owned();
    let slug = slug(task_id);
    let dir = project
        .parent()?
        .join(format!("{name}.worktrees"))
        .join(&slug);
    Some((dir, format!("task/{slug}")))
}

/// Creates the worktree, or reuses it when it is a complete checkout of
/// `branch`. New branches start at the project's current HEAD.
pub fn create(git: &Path, project: &Path, dir: &Path, branch: &str) -> Result<(), String> {
    if dir.exists() {
        // Only a finished worktree of this very branch is reused: a folder
        // left by an interrupted run, or another branch, is reported.
        let complete = run(
            git,
            dir,
            &["rev-parse".as_ref(), "--verify".as_ref(), "HEAD".as_ref()],
        )
        .is_ok()
            && crate::git_remote::current_branch(dir).as_deref() == Some(branch);
        return if complete {
            Ok(())
        } else {
            Err(format!(
                "{} exists but is not a complete worktree of {branch}; remove it (git worktree remove --force) and try again",
                dir.display()
            ))
        };
    }
    // Entries of worktrees deleted by hand would make `add` fail.
    let _ = run(git, project, &["worktree".as_ref(), "prune".as_ref()]);
    let exists = run(
        git,
        project,
        &[
            "rev-parse".as_ref(),
            "--verify".as_ref(),
            "--quiet".as_ref(),
            format!("refs/heads/{branch}").as_ref(),
        ],
    )
    .is_ok();
    let dir_arg = dir.as_os_str();
    let args: Vec<&std::ffi::OsStr> = if exists {
        vec![
            "worktree".as_ref(),
            "add".as_ref(),
            dir_arg,
            branch.as_ref(),
        ]
    } else {
        vec![
            "worktree".as_ref(),
            "add".as_ref(),
            "-b".as_ref(),
            branch.as_ref(),
            dir_arg,
        ]
    };
    let result = run(git, project, &args).map(|_| ());
    if result.is_err() && dir.exists() {
        // Never leave half a worktree behind.
        let _ = run(
            git,
            project,
            &[
                "worktree".as_ref(),
                "remove".as_ref(),
                "--force".as_ref(),
                dir_arg,
            ],
        );
    }
    result
}

/// Runs `git args...` in `cwd` with hooks disabled (a slow post-checkout
/// hook would hang the creation); the first stderr line on failure. Output
/// is drained while git runs, so a chatty git never blocks on a full pipe.
fn run(git: &Path, cwd: &Path, args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let no_hooks = std::env::temp_dir().join("castorterm-no-git-hooks");
    let mut command = Command::new(git);
    command
        .arg("-c")
        .arg(format!("core.hooksPath={}", no_hooks.display()))
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start git: {e}"))?;
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = (&mut pipe).take(MAX_STDERR).read_to_end(&mut kept);
                // Keep reading (and dropping) the rest so git never blocks.
                let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            }
            kept
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(format!("could not wait for git: {e}")),
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if status.success() {
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&stderr);
        let line = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("git failed");
        Err(line.chars().filter(|c| !c.is_control()).take(200).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(
            // Only case changed: same task, readable name, no hash.
            slug("CL-246"),
            "cl-246"
        );
        assert_eq!(slug("cl-246"), "cl-246");
        assert_eq!(slug("task-7"), "task-7");
        // Ids that had to change get a hash, so they never collide.
        assert_ne!(slug("#12"), slug("12"));
        assert_ne!(slug("castorterm#5"), slug("castorterm-5"));
        assert!(slug("#12").starts_with("12-"));
        for id in [
            "--rm -rf /",
            "..hidden",
            "v1..2",
            "x.lock",
            "con",
            "COM1",
            "ü/\\:*?",
            &"x".repeat(200),
        ] {
            let s = slug(id);
            assert!(!s.starts_with(['-', '.']), "{id}: {s}");
            assert!(!s.contains(".."), "{id}: {s}");
            assert!(!s.ends_with(".lock"), "{id}: {s}");
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')),
                "{id}: {s}"
            );
            assert!(s.len() <= MAX_SLUG + 5, "{id}: {s}");
        }
        assert!(slug("con").starts_with("t-con"));
        assert!(slug("ü/\\:*?").starts_with("task-"));
    }

    #[test]
    fn plans() {
        let (dir, branch) = plan(Path::new("/src/app"), "cl-7").unwrap();
        assert_eq!(dir, Path::new("/src/app.worktrees/cl-7"));
        assert_eq!(branch, "task/cl-7");
        assert!(plan(Path::new("/"), "x").is_none());
    }

    fn git() -> Option<PathBuf> {
        let path = std::env::var_os("PATH");
        crate::provider::resolve("git", path.as_deref(), cfg!(windows))
    }

    #[test]
    fn creates_and_reuses_a_worktree() {
        let Some(git) = git() else {
            eprintln!("skipped: git not installed");
            return;
        };
        let base = std::env::temp_dir().join(format!("castorterm-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("app");
        std::fs::create_dir_all(&repo).unwrap();
        let git_ok = |args: &[&str]| {
            let args: Vec<&std::ffi::OsStr> = args.iter().map(|a| a.as_ref()).collect();
            run(&git, &repo, &args).unwrap()
        };
        git_ok(&["init", "-q"]);
        git_ok(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ]);

        let (dir, branch) = plan(&repo, "cl-1").unwrap();
        create(&git, &repo, &dir, &branch).unwrap();
        assert!(dir.join(".git").exists());
        // Second time: reused.
        create(&git, &repo, &dir, &branch).unwrap();
        assert_eq!(
            crate::git_remote::current_branch(&dir).as_deref(),
            Some("task/cl-1")
        );

        // Existing branch, worktree removed: added again on that branch.
        git_ok(&["worktree", "remove", "--force", &dir.to_string_lossy()]);
        create(&git, &repo, &dir, &branch).unwrap();
        assert!(dir.join(".git").exists());

        // A plain folder in the way is an error, not overwritten.
        let (other, branch2) = plan(&repo, "CL-2").unwrap();
        std::fs::create_dir_all(&other).unwrap();
        assert!(
            create(&git, &repo, &other, &branch2)
                .unwrap_err()
                .contains("not a complete worktree")
        );
        assert!(other.exists(), "a folder in the way is never deleted");

        // A worktree deleted by hand: prune lets it be created again.
        let (gone, branch3) = plan(&repo, "cl-3").unwrap();
        create(&git, &repo, &gone, &branch3).unwrap();
        std::fs::remove_dir_all(&gone).unwrap();
        create(&git, &repo, &gone, &branch3).unwrap();
        assert!(gone.join(".git").exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}
