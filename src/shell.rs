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

/// PowerShell script run after the profile: wraps the current `prompt`
/// (oh-my-posh and others included) so that, after it runs, the window title
/// reports the location to castorterm (see `pty_tap::TITLE_PREFIX`; ConPTY
/// forwards titles but drops OSC 7).
const POWERSHELL_INTEGRATION: &str = r#"if (-not (Get-Variable -Name __castortermPrompt -Scope Global -ErrorAction SilentlyContinue)) {
    $global:__castortermPrompt = $function:prompt
}
function global:prompt {
    $text = & $global:__castortermPrompt
    $title = 'castorterm:cwd:' + $executionContext.SessionState.Path.CurrentLocation.ProviderPath
    $Host.UI.RawUI.WindowTitle = $title
    # Also as the last thing written, after any title the prompt sets itself.
    "$text" + [char]27 + ']0;' + $title + [char]7
}"#;

/// `-EncodedCommand` value for the integration: base64 of UTF-16LE, so no
/// quoting of the command line is involved.
pub fn powershell_integration() -> String {
    let bytes: Vec<u8> = POWERSHELL_INTEGRATION
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    base64(&bytes)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = match chunk {
            [a, b, c] => u32::from(*a) << 16 | u32::from(*b) << 8 | u32::from(*c),
            [a, b] => u32::from(*a) << 16 | u32::from(*b) << 8,
            [a] => u32::from(*a) << 16,
            _ => unreachable!(),
        };
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(TABLE[(n >> shift & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Shells to try, in order, for the given platform.
///
/// On Windows: PowerShell 7 (`pwsh`) first, then Windows PowerShell, both
/// with the location integration.
/// Elsewhere: `$SHELL` if set and not empty, then `/bin/bash`.
pub fn shell_candidates(is_windows: bool, shell_env: Option<&str>) -> Vec<ShellSpec> {
    if is_windows {
        let script = powershell_integration();
        let args = ["-NoLogo", "-NoExit", "-EncodedCommand", script.as_str()];
        return vec![
            ShellSpec::new("pwsh.exe", &args),
            ShellSpec::new("powershell.exe", &args),
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
    /// Test-only decoder for the encoder above.
    fn decode_base64(text: &str) -> Vec<u8> {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let values: Vec<u32> = text
            .bytes()
            .filter(|&b| b != b'=')
            .map(|b| TABLE.iter().position(|&t| t == b).unwrap() as u32)
            .collect();
        let mut out = Vec::new();
        for chunk in values.chunks(4) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0, |acc, (i, v)| acc | v << (18 - 6 * i));
            for k in 0..chunk.len().saturating_sub(1) {
                out.push((n >> (16 - 8 * k)) as u8);
            }
        }
        out
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
        // UTF-16LE of "ls" is 6c 00 73 00.
        assert_eq!(super::base64(&[0x6c, 0, 0x73, 0]), "bABzAA==");
    }

    use super::*;

    #[test]
    fn windows_prefers_pwsh_then_powershell() {
        let c = shell_candidates(true, Some("/bin/zsh"));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].program, "pwsh.exe");
        assert_eq!(c[1].program, "powershell.exe");
        assert_eq!(&c[0].args[..3], ["-NoLogo", "-NoExit", "-EncodedCommand"]);
        assert_eq!(c[0].args[3], powershell_integration());
        // The encoded script decodes back to the integration.
        let decoded = decode_base64(&c[0].args[3]);
        let utf16: Vec<u16> = decoded
            .chunks(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        let script = String::from_utf16(&utf16).unwrap();
        assert!(script.contains("castorterm:cwd:"));
        assert!(script.contains("__castortermPrompt"));
        assert_eq!(c[1].args, c[0].args);
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
