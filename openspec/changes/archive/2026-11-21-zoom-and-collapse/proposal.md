## Why

The text was hard to read for the user at the fixed sizes, and with several workspaces the sidebar
gets long: there was no way to make text bigger or to fold workspaces that are not in use.

## What Changes

- Zoom for the terminal and the interface together: `Ctrl+=` (or `Ctrl++`), `Ctrl+-`, `Ctrl+0` and
  `Ctrl` + mouse wheel, from 70% to 250% in 10% steps, saved as `[ui] zoom` in the config file.
  Column widths grow with the text.
- Clicking a workspace's name folds or unfolds it; a folded workspace shows how many sessions run
  inside and whether an agent there waits. Saved as `collapsed = true` on the workspace.

## Capabilities

### New Capabilities

### Modified Capabilities
- `visual-theme`: text zoom.
- `workspace-management`: fold workspaces.

## Impact

- `src/theme.rs` (zoom, `ui()`), `src/view.rs` (terminal font), `src/config.rs`,
  `src/config_edit.rs` (`SetZoom`, `SetCollapsed`), `src/tree.rs`, `src/shortcuts.rs`,
  `src/workspace_view.rs` and submodules; README and `config.example.toml`.
