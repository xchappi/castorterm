## Why

Working with several agents means rebuilding the same set of sessions every morning: a shell and
Claude Code in this project, OpenCode in that one. castorterm should reopen what was open, and the
agents should continue their conversations.

## What Changes

- The open sessions (project and launcher, plus the active one) are saved to `sessions.json` next to
  the config file whenever a session is opened, closed or activated.
- At startup they are reopened instead of the single shell; the launch directory still appears in
  the sidebar. Sessions whose project or launcher no longer exists are skipped; at most 20.
- New optional `resume_args` per agent, added to the command when a session is restored (e.g.
  `["--continue"]`).
- New `[ui] restore_sessions` (default `true`).

## Capabilities

### New Capabilities
- `session-restore`: reopen the sessions of the last run.

### Modified Capabilities

## Impact

- New `src/restore.rs`; `src/config.rs` (`resume_args`, `restore_sessions`), `src/tree.rs`,
  `src/workspace_view.rs`; README and `config.example.toml`.
