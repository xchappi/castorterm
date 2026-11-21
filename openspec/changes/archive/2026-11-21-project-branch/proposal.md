## Why

With several agents working on several projects, the first thing to know is which branch each
project is on: agents create and switch branches on their own. Today castorterm shows no git state.

## What Changes

- Each project in the sidebar shows its current git branch (or `@<short id>` when HEAD is
  detached), refreshed in the background every few seconds.
- The window title includes the branch of the active session's project.
- Reading the branch follows the same rules as remote detection: files read directly (git is never
  run), only local paths, no links inside the folder, small files only.

The per-session working directory (OSC 7) stays in MVP-2: `alacritty_terminal` does not parse it and
needs a PTY output tap, which is a separate change.

## Capabilities

### New Capabilities
- `project-git`: shows each project's current git branch.

### Modified Capabilities

## Impact

- `src/git_remote.rs` (`current_branch`), `src/workspace_view.rs` (cache, refresh, sidebar, title),
  `src/workspace_view/manage.rs` (refresh after adding folders).
