## Why

While an agent works, the user cannot see which files it read or changed without scrolling its
output; reviewing its work means hunting for paths.

## What Changes

- `castorterm --touched`: run by Claude Code's `PostToolUse` hook (Read, Edit, Write, MultiEdit,
  NotebookEdit), reads the hook's JSON from stdin and appends `{"action","path"}` to
  `<pid>-<n>.files` next to the session's status file. OpenCode's plugin (castorlabs-stack) writes
  the same lines itself.
- castorterm reads each session's list with its status (every second): most recent first, each
  file once with the most significant action (write > edit > read), at most 50.
- A "N files" button bottom left of the session opens the list; hovering a file previews it,
  clicking opens it (same rules as links).

## Capabilities

### New Capabilities
- `touched-files`: files an agent used, per session.

### Modified Capabilities

## Impact

New `src/touched.rs`; `src/main.rs` (`--touched`), `src/agent_status.rs` (shared guard, stale
cleanup), `src/view.rs` (button, list), `src/workspace_view.rs` (polling, cleanup); README. The
hooks themselves are installed by castorlabs-stack's adapters.
