//! Default shell selection per platform.

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
}
