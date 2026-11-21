//! Running task providers: external executables that implement the
//! contract in `docs/providers.md`. Blocking; call it off the UI thread.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::shell::resolve_program;

/// A provider run is stopped after this long.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// Larger outputs are rejected.
pub const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;

/// Extensions tried for bare command names on Windows, in order.
const WINDOWS_EXTENSIONS: [&str; 3] = ["exe", "cmd", "bat"];

/// Resolves a provider command to an absolute path, searching only absolute
/// `PATH` entries (never the current directory). On Windows a bare name
/// without extension also tries `.exe`, `.cmd` and `.bat`.
pub fn resolve(command: &str, path_var: Option<&OsStr>, windows: bool) -> Option<PathBuf> {
    if let Some(found) = resolve_program(command, path_var) {
        return Some(found);
    }
    if !windows || Path::new(command).extension().is_some() {
        return None;
    }
    WINDOWS_EXTENSIONS
        .iter()
        .find_map(|ext| resolve_program(&format!("{command}.{ext}"), path_var))
}

/// Runs `program args... list --filter <filter>` and returns its stdout.
pub fn run_list(
    program: &Path,
    args: &[String],
    filter: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<String, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .args(["list", "--filter", filter])
        .env("CASTORTERM_PROVIDER_PROTOCOL", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd.filter(|d| d.is_dir()) {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window flashes from a GUI app.
        command.creation_flags(0x0800_0000);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start provider {}: {e}", program.display()))?;
    let stdout = read_in_background(child.stdout.take(), MAX_OUTPUT + 1);
    let stderr = read_in_background(child.stderr.take(), MAX_STDERR);

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "provider timed out after {} s",
                    timeout.as_secs_f32()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(format!("could not wait for the provider: {e}")),
        }
    };

    // A grandchild may keep the pipes open: do not wait for it forever.
    let grace = Duration::from_secs(2);
    let out = stdout.recv_timeout(grace).unwrap_or_default();
    let err = stderr.recv_timeout(grace).unwrap_or_default();

    if !status.success() {
        let first_line = String::from_utf8_lossy(&err)
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string();
        return Err(match first_line.is_empty() {
            true => format!("provider failed ({status})"),
            false => format!("provider failed ({status}): {first_line}"),
        });
    }
    if out.len() > MAX_OUTPUT {
        return Err(format!(
            "provider output is larger than {} MiB",
            MAX_OUTPUT / 1024 / 1024
        ));
    }
    String::from_utf8(out).map_err(|_| "provider output is not UTF-8".to_string())
}

/// Reads up to `limit` bytes on a thread; the result arrives on the channel.
fn read_in_background<R: Read + Send + 'static>(
    pipe: Option<R>,
    limit: usize,
) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(limit as u64).read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("castorterm-provider-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes a tiny fake provider script and returns its path.
    /// `windows_body` and `unix_body` receive the arguments as usual.
    fn script(dir: &Path, name: &str, windows_body: &str, unix_body: &str) -> PathBuf {
        if cfg!(windows) {
            let path = dir.join(format!("{name}.cmd"));
            fs::write(&path, format!("@echo off\r\n{windows_body}\r\n")).unwrap();
            path
        } else {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\n{unix_body}\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            path
        }
    }

    #[test]
    fn resolves_only_absolute_path_entries() {
        let dir = temp_dir("resolve");
        let program = script(&dir, "fake-provider", "echo x", "echo x");
        let path_var = std::env::join_paths([Path::new("relative"), dir.as_path()]).unwrap();
        assert_eq!(
            resolve("fake-provider", Some(&path_var), cfg!(windows)),
            Some(program)
        );
        assert_eq!(
            resolve("missing-provider", Some(&path_var), cfg!(windows)),
            None
        );
        assert_eq!(
            resolve("sub/fake-provider", Some(&path_var), cfg!(windows)),
            None
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn passes_the_filter_and_returns_stdout() {
        let dir = temp_dir("ok");
        // Arguments: list --filter <filter>; the filter is echoed as the id.
        let program = script(
            &dir,
            "ok",
            "echo {\"version\":1,\"tasks\":[{\"id\":\"%3\"}]}",
            "echo \"{\\\"version\\\":1,\\\"tasks\\\":[{\\\"id\\\":\\\"$3\\\"}]}\"",
        );
        let out = run_list(&program, &[], "repo-x", None, TIMEOUT).unwrap();
        assert!(out.contains("\"id\":\"repo-x\""), "{out}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failure_reports_first_stderr_line() {
        let dir = temp_dir("fail");
        let program = script(
            &dir,
            "fail",
            "echo token missing 1>&2\r\nexit /b 3",
            "echo token missing >&2\nexit 3",
        );
        let err = run_list(&program, &[], "f", None, TIMEOUT).unwrap_err();
        assert!(err.contains("token missing"), "{err}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn slow_provider_times_out() {
        let dir = temp_dir("slow");
        let program = script(&dir, "slow", "ping -n 6 127.0.0.1 >nul", "sleep 5");
        let started = Instant::now();
        let err = run_list(&program, &[], "f", None, Duration::from_millis(500)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(4));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_program_is_an_error() {
        let err = run_list(Path::new("/definitely/not/here"), &[], "f", None, TIMEOUT).unwrap_err();
        assert!(err.contains("could not start"), "{err}");
    }
}
