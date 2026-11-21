## Context

MVP-0 has one `TerminalView` (GPUI canvas over an `alacritty_terminal::Term`) created in
`main.rs`; the view quits the app when its shell exits. MVP-1 needs many sessions, a sidebar and a
config file, while keeping the terminal rendering untouched.

## Goals / Non-Goals

**Goals:** many concurrent sessions in one window; sidebar organised as Workspace -> Project ->
Session; agent launchers from config; keyboard shortcuts for the common tab operations; an optional,
forgiving TOML config.

**Non-Goals:** persistence of sessions, config hot reload, settings UI, splits, custom keymaps.

## Decisions

### Module layout

- `config.rs` (pure): serde types, path resolution and parsing. Unit-tested.
- `tabs.rs` (pure): `Tabs<T>`, the ordered list of open sessions with the active one, generic over
  the session handle so it is tested without PTYs or GPUI.
- `shortcuts.rs` (pure): maps a keystroke (key + modifiers) to an app `Shortcut`. Unit-tested.
- `view.rs`: `TerminalView` keeps rendering one session, now with a configurable font, and emits
  `TerminalEvent::Exited` instead of calling `cx.quit()`.
- `workspace_view.rs`: root view. Owns the project tree, `Tabs<Entity<TerminalView>>`, the
  subscriptions to each terminal and the status/error message. Renders sidebar, tab bar and the
  active terminal.

### Config format and location

```toml
[font]
family = "Cascadia Mono"
size = 14

[[agents]]
name = "Claude Code"
command = "claude"

[[workspaces]]
name = "CastorLabs"

[[workspaces.projects]]
name = "castorterm"
path = "D:/proyectos/castorlabs/castorterm"
```

Location: `CASTORTERM_CONFIG` if set; otherwise `%APPDATA%\castorterm\config.toml` on Windows and
`$XDG_CONFIG_HOME/castorterm/config.toml` (fallback `~/.config/castorterm/config.toml`) elsewhere,
macOS included (developer tools commonly use `~/.config`). No new crate is needed for this.

A missing file is not an error. A file that fails to parse yields the default config plus an
error message shown in the sidebar. Project paths support a leading `~`; a project whose path is
not absolute after expansion is skipped with a warning (relative paths would depend on the launch
directory). Unknown keys are rejected (`deny_unknown_fields`) so typos surface instead of being
silently ignored.

### Agent sessions = shell + typed command

Agents are started by spawning the default shell (same resolution as MVP-0, absolute PATH only)
in the project directory and writing `command` + `\r` to the PTY. Alternatives considered:

- Spawning the agent binary directly as the PTY program: on Windows most agents are `.cmd`
  shims that `CreateProcessW` cannot run without `cmd.exe`, and the tab would die with the agent.
- Shell-specific `-Command` / `-c` arguments: works but needs per-shell quoting rules.

Typing the command is shell-agnostic and leaves a usable shell after the agent exits. The command
comes from the user's own config file, so no new trust boundary is introduced. Names and commands
with control characters are rejected so a stray newline cannot run extra commands. The agent
binary is resolved by the shell itself: PowerShell never runs commands from the current
directory without `.\`, while POSIX shells only do so if the user's `PATH` contains `.`; that is
the user's own shell policy and is documented rather than overridden.

### Shortcuts via capture phase

The root view handles shortcuts in `capture_key_down`, which runs before the terminal's bubble
phase `on_key_down`, and stops propagation when a shortcut matches. This keeps a single, testable
mapping function and avoids relying on keymap/action dispatch ordering. Chosen keys avoid what
shells and TUIs commonly use: Ctrl+Shift+T/W and Ctrl+(Shift+)Tab.

### Closing sessions

Closing a tab sends `Msg::Shutdown` to the PTY event loop, which drops the PTY: on Windows the
pseudo console is closed, which ends the processes attached to it; on Unix the child receives
SIGHUP. Dropping the last handle of a session sends the same message, so a session can never
outlive its owner by accident. Processes that detach themselves from the console (e.g.
`Start-Process`, daemons) survive, as in Windows Terminal; killing whole process trees with a Job
Object is left for later. The later `Exited` event of a removed session is ignored. Key repeat
never repeats opening or closing sessions, only switching between them. When the closed tab was active, the tab to its
right becomes active (or the one to its left if it was the last). Closing the last tab quits.

### Focus

Activating a session focuses its `TerminalView`. Handlers that change the active session receive
the `Window` (click listeners, `subscribe_in`, key capture), so focus is moved immediately.

## Risks / Trade-offs

- Typing the agent command before the shell has drawn its prompt relies on the PTY buffering
  typeahead; PowerShell/PSReadLine and bash do. Verified manually on Windows.
- Inactive sessions keep receiving output and notifying their own `TerminalView`; cheap, since
  only the active terminal is laid out and painted.
- A command typed before the shell is ready can be lost by slow or interactive shell profiles.
  Commands also end up in the shell history, so the example config warns against inline secrets.
- Output printed right before a shell exits is not shown: the tab closes immediately.
- Config errors are only visible in the sidebar; there is no notification system yet.
