# session-restore Specification

## Purpose
Reopens the sessions of the previous run at startup, so the user's set of shells and agents per
project is back without clicking through launchers again, and agents can continue their work.

## Requirements

### Requirement: Save open sessions
castorterm SHALL save the open sessions (project directory and launcher name, in tab order, and the
active one) to `sessions.json` in the config directory whenever a session is opened, closed or
activated. Closing the last session SHALL delete the file.

#### Scenario: Window closed with sessions open
- **WHEN** the user closes the window with a shell in `app` and Claude Code in `api`
- **THEN** `sessions.json` lists both sessions and which one was active

### Requirement: Restore at startup
When `[ui] restore_sessions` is not `false`, castorterm SHALL reopen the saved sessions at startup
instead of the default shell, skipping sessions whose project is no longer in a workspace or whose
launcher no longer exists, at most 20, and SHALL activate the session that was active. Agents
SHALL be started with their command followed by their `resume_args`. Text typed into a shell SHALL
come only from the config, never from the state file. If nothing can be restored, the default
shell SHALL open.

#### Scenario: Agent continues its conversation
- **WHEN** Claude Code was open in `api` and its config has `resume_args = ["--continue"]`
- **THEN** at startup a Claude Code session opens in `api` running `claude --continue`

#### Scenario: Removed project
- **WHEN** a saved session's project was removed from its workspace
- **THEN** that session is not reopened and the others are

#### Scenario: Restore disabled
- **WHEN** the config has `restore_sessions = false`
- **THEN** castorterm starts with a single shell in the launch directory

### Requirement: Restore window geometry
castorterm SHALL save the window position, size and maximized state with the sessions, without
writing on every move or resize, including when the window closes, and SHALL open the window with
them next time when they are plausible and intersect a connected screen; otherwise it SHALL use the
default size centered on the main screen.

#### Scenario: Same place as last time
- **WHEN** the user leaves the window maximized on the second monitor and closes castorterm
- **THEN** next time it opens maximized on that monitor

#### Scenario: Monitor unplugged
- **WHEN** the saved position is on a screen that is no longer connected
- **THEN** the window opens with the default size, centered on the main screen
