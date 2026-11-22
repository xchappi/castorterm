## Why

The board needed a task provider (a command and a tracker). For open source use castorterm should
show tasks with no setup, from files the projects already have, without sending anything anywhere.

## What Changes

- Workspaces without a configured board get a built-in local board: no command runs.
- It lists the checklist items of each project's `TASKS.md` (status from the section heading, done
  when checked) and one task per OpenSpec change in progress (`openspec/changes/<id>/tasks.md`) with
  its progress (ready, in progress, or review when all steps are done).
- Starting a task opens an agent with a prompt naming the task and where it is.
- An empty local board explains which files it reads.

## Capabilities

### New Capabilities
- `local-tasks`: a board from the projects' own files.

### Modified Capabilities

## Impact

New `src/local_tasks.rs`; `src/workspace_view.rs` (`board_of`, local listing, empty hint); README.
