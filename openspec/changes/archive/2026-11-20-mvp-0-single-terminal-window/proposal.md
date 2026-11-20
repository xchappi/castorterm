## Why

castorterm has no functionality yet: `main.rs` only prints the version. Before building the
workspace/project/session sidebar, we need the core building block every later feature depends on:
a native window that hosts one real, interactive terminal session. This also validates the stack
decision (Rust + GPUI + a VT state machine + a PTY) on Windows, the primary platform, as early as
possible.

## What Changes

- Launching `castorterm` opens a native GPUI window instead of printing to stdout.
- The window hosts a single terminal session running the platform default shell
  (PowerShell on Windows, `$SHELL` or `/bin/bash` elsewhere) through a PTY (ConPTY on Windows).
- Shell output is parsed by a VT state machine and rendered as a monospace cell grid with
  ANSI colors (16/256/truecolor), bold, and a visible cursor.
- Keyboard input (printable text, Enter, Backspace, Tab, Escape, arrows, Ctrl+letter) is
  forwarded to the shell.
- Resizing the window resizes the PTY and the terminal grid.
- When the shell exits, the window closes.
- `--version` keeps printing the version without opening a window.

## Out of scope

- Multiple tabs, sidebar, workspaces, projects (MVP-1).
- cwd / git branch display and shell integration (MVP-2).
- Agent status detection, splits, themes, configuration file, scrollback navigation,
  mouse selection, copy/paste, IME composition, font configuration.

## Capabilities

### New Capabilities

- `terminal-session`: a single interactive terminal session hosted in a native window
  (process spawn, output rendering, keyboard input, resize, exit handling).

### Modified Capabilities

None.

## Impact

- New dependencies: `gpui`, a VT state machine crate and a PTY crate (exact choice in design.md),
  `anyhow`.
- CI must install GPUI system dependencies on Linux and will build noticeably slower.
- Windows builds require MSVC Build Tools locally.
