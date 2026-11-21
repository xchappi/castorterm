//! Running task providers: external executables that implement the
//! contract in `docs/providers.md`. Blocking; call it off the UI thread.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::shell::resolve_program;

/// A provider run is stopped after this long.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// Larger outputs are rejected.
pub const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
/// How long to wait for the output pipes once the provider has exited.
const PIPE_GRACE: Duration = Duration::from_secs(2);

/// Extensions tried for bare command names on Windows, in order.
const WINDOWS_EXTENSIONS: [&str; 3] = ["exe", "cmd", "bat"];

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Resolves a provider command to an absolute path, searching only absolute
/// `PATH` entries (never the current directory). On Windows a bare name
/// without extension tries `.exe`, `.cmd` and `.bat` first: npm installs a
/// POSIX script `foo` next to `foo.cmd`, and only the latter runs there.
pub fn resolve(command: &str, path_var: Option<&OsStr>, windows: bool) -> Option<PathBuf> {
    if windows && Path::new(command).extension().is_none() {
        let with_extension = WINDOWS_EXTENSIONS
            .iter()
            .find_map(|ext| resolve_program(&format!("{command}.{ext}"), path_var));
        if with_extension.is_some() {
            return with_extension;
        }
    }
    resolve_program(command, path_var)
}

/// Runs `program args... list --filter <filter>` and returns its stdout.
///
/// The working directory is the provider's own directory, never a project:
/// a project may be an untrusted clone, and Windows searches the working
/// directory for programs and DLLs.
pub fn run_list(
    program: &Path,
    args: &[String],
    filter: &str,
    timeout: Duration,
) -> Result<String, String> {
    run_list_limited(program, args, filter, timeout, MAX_OUTPUT)
}

fn run_list_limited(
    program: &Path,
    args: &[String],
    filter: &str,
    timeout: Duration,
    max_output: usize,
) -> Result<String, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .args(["list", "--filter", filter])
        .env("CASTORTERM_PROVIDER_PROTOCOL", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = program.parent().filter(|d| d.is_dir()) {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashes from a GUI app.
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group, so a timeout can kill the whole tree.
        command.process_group(0);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start provider {}: {e}", program.display()))?;
    let stdout = read_in_background(child.stdout.take(), max_output);
    let stderr = read_in_background(child.stderr.take(), MAX_STDERR);

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                return Err(format!(
                    "provider timed out after {} s",
                    timeout.as_secs_f32()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => {
                kill_tree(&mut child);
                return Err(format!("could not wait for the provider: {e}"));
            }
        }
    };

    // A grandchild may keep the pipes open: do not wait for it forever.
    let (out, overflow) = stdout.recv_timeout(PIPE_GRACE).unwrap_or_default();
    let (err, _) = stderr.recv_timeout(PIPE_GRACE).unwrap_or_default();

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
    if overflow {
        return Err(format!(
            "provider output is larger than {} KiB",
            max_output / 1024
        ));
    }
    String::from_utf8(out).map_err(|_| "provider output is not UTF-8".to_string())
}

/// Kills the provider and everything it started (e.g. `cmd.exe` -> `node`).
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        use std::os::windows::process::CommandExt;
        let taskkill = PathBuf::from(root).join("System32").join("taskkill.exe");
        let _ = Command::new(taskkill)
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(unix)]
    if let Ok(group) = i32::try_from(child.id()) {
        // The child leads its own process group (see `process_group(0)`).
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads a pipe to the end on a thread, keeping at most `limit` bytes. The
/// rest is drained and discarded so the writer never blocks; the flag says
/// whether anything was dropped.
fn read_in_background<R: Read + Send + 'static>(
    pipe: Option<R>,
    limit: usize,
) -> mpsc::Receiver<(Vec<u8>, bool)> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut overflow = false;
        if let Some(mut pipe) = pipe {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let room = limit.saturating_sub(kept.len());
                        if n > room {
                            overflow = true;
                        }
                        kept.extend_from_slice(&chunk[..n.min(room)]);
                    }
                }
            }
        }
        let _ = tx.send((kept, overflow));
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
        let out = run_list(&program, &[], "repo-x", TIMEOUT).unwrap();
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
        let err = run_list(&program, &[], "f", TIMEOUT).unwrap_err();
        assert!(err.contains("token missing"), "{err}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn slow_provider_times_out() {
        let dir = temp_dir("slow");
        let program = script(&dir, "slow", "ping -n 6 127.0.0.1 >nul", "sleep 5");
        let started = Instant::now();
        let err = run_list(&program, &[], "f", Duration::from_millis(500)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(4));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_program_is_an_error() {
        let err = run_list(Path::new("/definitely/not/here"), &[], "f", TIMEOUT).unwrap_err();
        assert!(err.contains("could not start"), "{err}");
    }

    #[test]
    fn bare_windows_name_prefers_runnable_extensions() {
        let dir = temp_dir("ext");
        // npm puts a POSIX script next to the .cmd shim.
        fs::write(dir.join("tool"), "#!/bin/sh").unwrap();
        fs::write(dir.join("tool.cmd"), "@echo off").unwrap();
        let path_var = std::env::join_paths([dir.as_path()]).unwrap();
        assert_eq!(
            resolve("tool", Some(&path_var), true),
            Some(dir.join("tool.cmd"))
        );
        assert_eq!(
            resolve("tool", Some(&path_var), false),
            Some(dir.join("tool"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_output_is_rejected_without_waiting_for_the_timeout() {
        let dir = temp_dir("big");
        let program = script(
            &dir,
            "big",
            "for /L %%i in (1,1,300) do @echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "i=0; while [ $i -lt 300 ]; do echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; i=$((i+1)); done",
        );
        let started = Instant::now();
        let err = run_list_limited(&program, &[], "f", TIMEOUT, 1024).unwrap_err();
        assert!(err.contains("larger than"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn runs_in_the_provider_directory() {
        let dir = temp_dir("cwd");
        let program = script(&dir, "cwd", "cd", "pwd");
        let out = run_list(&program, &[], "f", TIMEOUT).unwrap();
        let expected = fs::canonicalize(&dir).unwrap();
        let got = fs::canonicalize(out.trim()).unwrap();
        assert_eq!(got, expected);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_grandchildren() {
        let dir = temp_dir("tree");
        let pid_file = dir.join("pid");
        let program = script(
            &dir,
            "tree",
            "",
            &format!(
                "sleep 30 &
echo $! > {}
wait",
                pid_file.display()
            ),
        );
        let err = run_list(&program, &[], "f", Duration::from_millis(500)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        let pid: i32 = fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // Signal 0 only checks whether the process still exists.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        assert!(!alive, "grandchild {pid} survived the timeout");
        fs::remove_dir_all(dir).unwrap();
    }
}
