//! One git worktree per task, so several agents can work on the same
//! repository at once without touching each other's files.
//!
//! The worktree of task `CL-7` in project `/src/app` is `/src/app.worktrees/cl-7`
//! on branch `task/cl-7`. git runs without a shell, from the project
//! directory (the user's own repository), with a timeout.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// git is local and fast; anything slower is stuck.
pub const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SLUG: usize = 40;

/// File-name and branch-safe form of a task id: lowercase ASCII letters,
/// digits, `.`, `_` and `-`; anything else becomes `-`.
pub fn slug(id: &str) -> String {
    let mut out = String::new();
    for c in id.trim().chars() {
        let c = c.to_ascii_lowercase();
        let keep = c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
        let c = if keep { c } else { '-' };
        if !(c == '-' && out.ends_with('-')) {
            out.push(c);
        }
        if out.len() >= MAX_SLUG {
            break;
        }
    }
    // No leading dots or dashes (hidden files, flags), no trailing ones.
    let out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    if out.is_empty() { "task".into() } else { out }
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

/// Creates the worktree (or reuses it if it already exists) with `git`.
pub fn create(git: &Path, project: &Path, dir: &Path, branch: &str) -> Result<(), String> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    if dir.exists() {
        return Err(format!(
            "{} exists and is not a worktree; remove it or pick another task",
            dir.display()
        ));
    }
    let dir_arg = dir.as_os_str();
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
    run(git, project, &args).map(|_| ())
}

/// Runs `git args...` in `cwd`; the first stderr line on failure.
fn run(git: &Path, cwd: &Path, args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let mut command = Command::new(git);
    command
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
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(format!("could not wait for git: {e}")),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("could not read git output: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
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
        assert_eq!(slug("CL-246"), "cl-246");
        assert_eq!(slug("#12"), "12");
        assert_eq!(slug("castorterm#5"), "castorterm-5");
        assert_eq!(slug("--rm -rf /"), "rm-rf");
        assert_eq!(slug("..hidden"), "hidden");
        assert_eq!(slug("ü/\\:*?"), "task");
        assert!(slug(&"x".repeat(200)).len() <= MAX_SLUG);
    }

    #[test]
    fn plans() {
        let (dir, branch) = plan(Path::new("/src/app"), "CL-7").unwrap();
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
            return; // git not installed: nothing to test here
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

        let (dir, branch) = plan(&repo, "CL-1").unwrap();
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
                .contains("not a worktree")
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
