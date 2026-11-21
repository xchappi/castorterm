## Why

A session starts in its project folder, but shells and agents move around; the user cannot tell
where each session is without looking at its prompt.

## What Changes

- castorterm reads each shell's working directory from its output: OSC 7 (bash, zsh, fish,
  oh-my-posh), Windows Terminal's OSC 9;9, and on Windows a title set by castorterm's PowerShell
  integration (ConPTY drops OSC 7 and 9;9 but forwards titles; verified).
- The PowerShell integration runs after the profile (`-NoExit -EncodedCommand`): it wraps the
  current `prompt` so the title becomes `castorterm:cwd:<path>` after each prompt.
- Tabs and sidebar session rows show the directory when it is not the project root (relative path
  inside the project, shortened elsewhere).

## Capabilities

### New Capabilities
- `session-cwd`: track and show each session's working directory.

### Modified Capabilities

## Impact

- New `src/pty_tap.rs` (PTY wrapper and OSC scanner, depends on `polling` 3 like
  `alacritty_terminal`); `src/shell.rs` (integration), `src/session.rs`, `src/view.rs`,
  `src/tree.rs` (`cwd_label`), `src/workspace_view.rs`; README.
