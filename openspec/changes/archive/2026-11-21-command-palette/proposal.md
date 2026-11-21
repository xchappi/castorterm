## Why

With several workspaces, projects and agents, clicking through the sidebar to open "Claude Code in
castorlabs-stack" is slow. A keyboard-first palette makes castorterm feel like an IDE.

## What Changes

- `Ctrl+Shift+P` opens a palette: type to fuzzy-filter, Up/Down to choose, Enter to run, Esc or a
  click outside to close.
- Entries: go to each open session; open each launcher in each project (selected project first);
  add folders to each workspace; new workspace; show files; show quests.
- Fix: the name field (`+ Workspace`, rename) got no keys at all because GPUI dispatches nothing
  when no element is focused; the view now has its own focus handle, used by both the field and
  the palette. The space bar, which GPUI reports without text on Windows, now types a space.

## Capabilities

### New Capabilities
- `command-palette`: keyboard palette for the main actions.

### Modified Capabilities

## Impact

- New `src/commands.rs` (entries and fuzzy matching) and `src/workspace_view/command_palette.rs`;
  `src/shortcuts.rs`, `src/name_input.rs`, `src/workspace_view.rs`, `src/workspace_view/manage.rs`;
  README.
