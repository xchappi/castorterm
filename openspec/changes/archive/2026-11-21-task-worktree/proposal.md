## Why

Running several agents on the same repository at once makes them overwrite each other's changes.
A git worktree per task gives each agent its own checkout and branch.

## What Changes

- Task details in Quests get a toggle "In a new worktree (task/<id>)".
- With it on, starting an agent creates (or reuses) the worktree `<project>.worktrees/<task>` on
  branch `task/<task>` with `git worktree add`, adds it to the same workspace as project
  `<project>@<task>` with the same `repo` (persisted), and starts the agent there with the task
  prompt.
- Errors (git missing, a plain folder in the way, git failing) are shown in the sidebar.

## Capabilities

### New Capabilities
- `task-worktree`: start an agent on a task in a git worktree of its own.

### Modified Capabilities

## Impact

- New `src/worktree.rs` and `src/workspace_view/task_worktree.rs`; `src/workspace_view.rs` (toggle,
  start_task), `src/workspace_view/manage.rs` (visibility); README.
