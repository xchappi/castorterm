## Why

castorterm should work as a visual "AI IDE": open the projects you work on and start agents on
them without leaving the app. Today workspaces and projects only come from hand-editing
`config.toml`, which is the biggest friction left for daily use.

## What Changes

- Create a workspace from the sidebar (`+ Workspace`, inline name field), rename it and remove it.
- Add folders to a workspace three ways: the `+` button (native folder picker, several at once),
  dropping folders from the file manager onto the sidebar, and `Ctrl+Shift+O` (selected workspace).
- A new project is named after its folder and its `repo` is detected from the git remote
  `origin` (`owner/repo`), so the Quests board works without extra setup.
- Remove a project from a workspace (never touches files on disk). Workspaces and projects with
  open sessions cannot be removed.
- Every change is saved to `config.toml` preserving comments and formatting (the file is created
  if missing). The implicit "Local" workspace becomes a regular one when you add to it.

## Capabilities

### New Capabilities
- `workspace-management`: create, rename and remove workspaces and add or remove project folders
  from the UI, persisted to the config file.

### Modified Capabilities

## Impact

- New: `src/config_edit.rs` (toml_edit), `src/git_remote.rs`, `src/workspace_view/manage.rs`.
- `src/tree.rs` (mutations), `src/workspace_view.rs` (sidebar, key capture, drop), `src/shortcuts.rs`.
- Dependency: `toml_edit` 0.22 (already compiled through `toml`).
- README and `config.example.toml` mention the UI.
