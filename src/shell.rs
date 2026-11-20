//! Default shell selection per platform.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Program and arguments used to start a shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl ShellSpec {
    fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }
}

/// Shells to try, in order, for the given platform.
///
/// On Windows: PowerShell 7 (`pwsh`) first, then Windows PowerShell.
/// Elsewhere: `$SHELL` if set and not empty, then `/bin/bash`.
pub fn shell_candidates(is_windows: bool, shell_env: Option<&str>) -> Vec<ShellSpec> {
    if is_windows {
        return vec![
            ShellSpec::new("pwsh.exe", &["-NoLogo"]),
            ShellSpec::new("powershell.exe", &["-NoLogo"]),
        ];
    }

    let mut candidates = Vec::new();
    if let Some(shell) = shell_env.map(str::trim).filter(|s| !s.is_empty()) {
        candidates.push(ShellSpec::new(shell, &[]));
    }
    if !candidates.iter().any(|c| c.program == "/bin/bash") {
        candidates.push(ShellSpec::new("/bin/bash", &[]));
    }
    candidates
}

/// Shell candidates for the platform this binary runs on.
pub fn default_shell_candidates() -> Vec<ShellSpec> {
    let shell_env = std::env::var("SHELL").ok();
    shell_candidates(cfg!(windows), shell_env.as_deref())
}

/// Resolves `program` to an absolute path of an existing file.
///
/// Absolute paths are accepted as-is if they exist. Bare names are searched
/// only in the absolute entries of `path_var`: relative entries (including
/// `.`) are skipped so a binary planted in the working directory (e.g. a
/// cloned repo) is never picked up. Relative paths with directories are
/// rejected for the same reason.
pub fn resolve_program(program: &str, path_var: Option<&OsStr>) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    if candidate.components().count() != 1 {
        return None;
    }
    std::env::split_paths(path_var?)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(program))
        .find(|path| path.is_file())
}

/// Quotes a program path for a Windows command line.
///
/// The command line is parsed by `CreateProcessW` with no application name,
/// so an unquoted path with spaces would be split (and could resolve to a
/// different executable such as `C:\Program.exe`).
pub fn quote_windows_program(program: &str) -> String {
    if program.contains([' ', '\t']) && !program.starts_with('"') {
        format!("\"{program}\"")
    } else {
        program.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_prefers_pwsh_then_powershell() {
        let c = shell_candidates(true, Some("/bin/zsh"));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].program, "pwsh.exe");
        assert_eq!(c[1].program, "powershell.exe");
        assert_eq!(c[0].args, vec!["-NoLogo".to_string()]);
    }

    #[test]
    fn unix_uses_shell_env_then_bash() {
        let c = shell_candidates(false, Some("/bin/zsh"));
        assert_eq!(c[0].program, "/bin/zsh");
        assert_eq!(c[1].program, "/bin/bash");
    }

    #[test]
    fn unix_without_shell_env_falls_back_to_bash() {
        assert_eq!(shell_candidates(false, None)[0].program, "/bin/bash");
        assert_eq!(shell_candidates(false, Some("  ")).len(), 1);
    }

    #[test]
    fn unix_does_not_duplicate_bash() {
        assert_eq!(shell_candidates(false, Some("/bin/bash")).len(), 1);
    }

    /// Creates `<tmp>/castorterm-<tag>-<pid>/{a,b}/` with `prog` in each.
    fn fixture(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("castorterm-{tag}-{}", std::process::id()));
        let a = root.join("a");
        let b = root.join("b");
        for dir in [&a, &b] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("prog"), b"").unwrap();
        }
        (root, a, b)
    }

    #[test]
    fn resolve_first_absolute_match_wins() {
        let (root, a, b) = fixture("first");
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(resolve_program("prog", Some(&path)), Some(a.join("prog")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_skips_relative_path_entries() {
        let (root, a, _b) = fixture("relative");
        // "." and "a" are relative: a planted binary there must not be used.
        let path = std::env::join_paths([PathBuf::from("."), PathBuf::from("a")]).unwrap();
        assert_eq!(resolve_program("prog", Some(&path)), None);
        let path = std::env::join_paths([PathBuf::from("."), a.clone()]).unwrap();
        assert_eq!(resolve_program("prog", Some(&path)), Some(a.join("prog")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_handles_missing_and_empty_path() {
        assert_eq!(resolve_program("prog", None), None);
        assert_eq!(resolve_program("prog", Some(OsStr::new(""))), None);
        assert_eq!(
            resolve_program("castorterm-missing-binary", Some(OsStr::new(""))),
            None
        );
    }

    #[test]
    fn resolve_rejects_relative_paths_with_dirs() {
        let path = std::env::temp_dir();
        assert_eq!(resolve_program("a/prog", Some(path.as_os_str())), None);
    }

    #[test]
    fn resolve_accepts_existing_absolute_path() {
        let (root, a, _b) = fixture("absolute");
        let prog = a.join("prog");
        assert_eq!(
            resolve_program(prog.to_str().unwrap(), None),
            Some(prog.clone())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quote_windows_program_only_when_needed() {
        assert_eq!(
            quote_windows_program(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            r#""C:\Program Files\PowerShell\7\pwsh.exe""#
        );
        assert_eq!(
            quote_windows_program(r"C:\Windows\System32\cmd.exe"),
            r"C:\Windows\System32\cmd.exe"
        );
    }
}
