## Why

Working with an agent usually means watching it while using a shell (tests, git) in the same
project; switching tabs back and forth hides one of them.

## What Changes

- Ctrl+Shift+D (or **Split view** in the palette) shows the active session next to the previous
  one in tab order (panes keep tab order; refused when the window is too narrow); with a single session it opens a shell in the same project on the right.
  Pressing it again goes back to one session.
- Clicking a pane makes it the active session (keyboard focus, title, sidebar).
- Activating a session outside the pair (tab, sidebar, Ctrl+Tab, a new session) puts it in the
  pane that was active; closing one of the two ends the split.

## Capabilities

### New Capabilities
- `split-view`: two sessions side by side.

### Modified Capabilities

## Impact

New `src/split.rs` (pure pair logic); `src/shortcuts.rs`, `src/commands.rs`,
`src/workspace_view.rs` and the palette; README. The split is not saved across restarts.
