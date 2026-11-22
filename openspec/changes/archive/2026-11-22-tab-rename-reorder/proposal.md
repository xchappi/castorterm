## Why

With several agents per project, tabs named `Shell 2` or `Claude Code 3` say nothing about what
each session is doing, and their order is fixed by when they were opened.

## What Changes

- Rename a session: double-click its tab or sidebar row, or **Rename session** in the palette.
  The name is edited in place in the tab; Enter saves, Esc cancels.
- Reorder: drag a tab onto another, or Ctrl+Shift+PageUp / PageDown on the active one.
- Names and order are saved with the sessions and restored.

## Capabilities

### New Capabilities
- `session-tabs`: naming and ordering open sessions.

### Modified Capabilities

## Impact

`src/tabs.rs` (`rename`, `move_to`, `move_by`), `src/shortcuts.rs`, `src/restore.rs`
(`label` in the state file), `src/commands.rs`, `src/workspace_view.rs` and submodules; README.
