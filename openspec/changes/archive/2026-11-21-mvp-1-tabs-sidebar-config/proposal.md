## Why

MVP-0 hosts exactly one shell. The point of castorterm is working with several agents across
several projects at once without losing context, so the next step is to host many sessions in one
window and organise them the way the user thinks about them: Workspace -> Project -> Session. The
success criterion for this phase is that the maintainer can keep three CastorLabs projects open,
with Claude Code and OpenCode in different tabs, started from the sidebar.

## What Changes

- The window is split into a **sidebar** (left) and a **terminal area** (right) with a **tab bar**
  on top of the terminal area.
- The sidebar lists workspaces, their projects and the open sessions of each project. Each project
  row offers one button per configured agent (plus the built-in "Shell") to start a new session in
  that project's directory. Clicking a session activates it.
- Many sessions can be open at once; only the active one is drawn, the others keep running.
- An agent session is the user's default shell started in the project directory with the agent's
  command typed into it, so the shell stays usable when the agent exits.
- Keyboard shortcuts: Ctrl+Shift+T (new shell in the active project), Ctrl+Shift+W (close the
  active session), Ctrl+Tab / Ctrl+Shift+Tab (next / previous session).
- When a session's shell exits its tab closes; castorterm exits when the last session closes.
- A **TOML configuration file** defines workspaces, projects (name + path), agents (name +
  command) and the terminal font. It is optional: without it castorterm behaves like MVP-0 plus
  tabs. An invalid file is reported in the sidebar and defaults are used.
- The initial session starts in the launch directory and is attached to the configured project
  with that path, or to an implicit "Local" workspace otherwise.

## Out of scope

- Persisting open sessions across restarts, editing the config from the UI, hot reload.
- cwd / git branch per session (MVP-2), agent status detection, splits, git worktrees (v1).
- Drag and drop of tabs, renaming sessions, custom keybindings, themes.
- Per-project shell override and environment variables for agents.

## Capabilities

### New Capabilities

- `session-tabs`: several concurrent sessions in one window, tab bar, sidebar tree
  (Workspace -> Project -> Session), agent launchers and keyboard shortcuts.
- `config-file`: location, format, defaults and error handling of the TOML configuration file.

### Modified Capabilities

- `terminal-session`: resize is computed from the terminal area instead of the whole window, and
  a shell exit closes its tab instead of the window.

## Impact

- New dependencies: `serde` (derive) and `toml` (both already in the dependency tree via gpui).
- `view.rs` stops quitting the app on exit and emits an event instead; a new root view owns the
  sidebar, the tab bar and the sessions.
- README gains a configuration section and an example config file.
