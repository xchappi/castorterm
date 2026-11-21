## Why

With several agents open, the user has to check every tab to know which one is waiting for an
answer or a permission. castorterm should show what each agent is doing.

## What Changes

- Every session gets `CASTORTERM_STATUS_FILE`, a file in `<config dir>/status/`.
- New `castorterm --status <working|waiting|done>`: writes the status to that file (no-op outside
  castorterm). Agents call it from their hooks; the README has the Claude Code hooks.
- castorterm reads the files of its sessions every second (off the UI thread) and shows the
  status next to each session in the sidebar and in its tab; the HUD counts agents waiting.
- Status files are removed when their session closes; leftovers older than a day are removed at
  startup.

## Capabilities

### New Capabilities
- `agent-status`: agents report working / waiting / done and castorterm shows it.

### Modified Capabilities

## Impact

- New `src/agent_status.rs`; `src/main.rs` (`--status`), `src/session.rs` (environment),
  `src/workspace_view.rs` (polling and badges); README.
