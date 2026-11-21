## Why

The sidebar mixes everything in one column: every project shows all its launcher buttons all the
time, and there is no way to see the files of the projects the agents are working on. The
maintainer wants to see the workspaces and, next to them, the file system of the current
workspace, so the layout is reorganised around three columns.

## What Changes

- New layout, left to right: **workspace panel** (quest log), **file explorer**, **tabs and
  terminal**.
- The workspace panel gets a notion of **selected project**: clicking a project selects it, and
  activating a session selects its project. Launcher buttons are shown only under the selected
  project, so the panel stays compact.
- New **file explorer** for the workspace that contains the selected project: one root per project
  of that workspace, folders expand and collapse lazily, folders first and sorted by name, `.git`
  hidden, very large folders truncated with a "N more" row. The selected project's root is
  expanded automatically. A refresh button re-reads the expanded folders.
- Files are coloured by type ("item rarity": code, docs, config, assets).
- Clicking a file types its path (relative to the active session's project, quoted if needed)
  into the active session, so it can be referenced in an agent prompt.
- Ctrl+Shift+E shows or hides the explorer.

## Out of scope

- Opening or editing files, file operations (create, rename, delete), drag and drop.
- Watching the file system for changes (refresh is manual or on expand).
- Git status decorations in the tree.
- Tracking the session's live cwd (MVP-2); paths are relative to the project directory.

## Capabilities

### New Capabilities

- `file-explorer`: workspace file tree, lazy listing rules, file colours, inserting paths into the
  active session, show/hide shortcut.

### Modified Capabilities

- `session-tabs`: project selection in the sidebar, launchers only for the selected project, and
  the Ctrl+Shift+E shortcut.

## Impact

- New pure module `files.rs` (listing, tree state, path quoting) with tests.
- `workspace_view.rs` gains the explorer column and project selection; `view.rs` gains a method to
  type text into a session.
- Directory listings happen on the UI thread when a folder is expanded or refreshed, capped per
  folder.
