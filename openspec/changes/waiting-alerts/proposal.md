## Why

Seeing `waiting` in the sidebar only helps while the user looks at castorterm. When they are in
another window, an agent can wait for minutes unnoticed; and with many sessions, finding the one
that waits takes clicks.

## What Changes

- When an agent starts waiting and the window is in the background, the taskbar button flashes
  until the window comes to the front (Windows; elsewhere nothing yet).
- `Ctrl+Shift+J` activates the next session (after the active one, wrapping) whose agent waits.
- The palette lists sessions with their status, waiting ones first.

## Capabilities

### New Capabilities

### Modified Capabilities
- `agent-status`: attention request, jump shortcut and status in the palette.

## Impact

- New `src/attention.rs` (FlashWindowEx through `raw-window-handle`); `src/workspace_view.rs`,
  `src/workspace_view/command_palette.rs`, `src/shortcuts.rs`; README.
