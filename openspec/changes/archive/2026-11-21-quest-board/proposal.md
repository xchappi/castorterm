## Why

castorterm already groups agent sessions by workspace and project, but the work itself (the tasks
those agents should do) lives in a separate tracker. The maintainer wants to see each workspace's
tasks inside the terminal, as a small board, and start an agent on a task directly. castorterm is
open source, so it must not know about any specific tracker: trackers plug in through external
**providers**, and each team (CastorLabs included) writes its own outside this repository.

## What Changes

- **Provider contract** (public, versioned): a provider is any executable. castorterm runs it with
  `list --filter <filter>` and reads a JSON document with the tasks from stdout. The contract is
  documented in `docs/providers.md`.
- **Config**: `[[providers]]` (name, command, args); a per-workspace `board` table selecting a
  provider, an opaque `filter`, a prompt template and a refresh interval; an optional `repo` per
  project to match tasks to projects; an optional `task_args` per agent describing how to pass an
  initial prompt to it.
- **Quests panel**: the middle column gets two tabs, **Files** and **Quests**. Quests shows the
  selected workspace's tasks grouped by status (In progress, Review, Ready, Backlog, Done
  collapsed), with priority, id and title. The board is read-only: agents update the tracker
  themselves (for example through the tracker's MCP server).
- **Start an agent on a task**: selecting a task shows its project, link and one button per agent
  with `task_args`. The button opens a session of that agent in the task's project with the
  prompt (e.g. `Work on task {id}: {title}`) passed as a quoted argument.
- **Example provider**: a GitHub Issues provider (`cargo run --example github_issues_provider`,
  using the `gh` CLI) as a reference implementation and to test the contract without any private
  tracker.

## Out of scope

- Writing to the tracker from castorterm (moving tasks, comments). The contract reserves room for
  it in a later version.
- Providers for private trackers: they live with their owners, not in this repository.
- Task details beyond title/status/priority/project/link, filtering and search inside the panel,
  drag and drop.
- Per-project config files inside repositories (`.castorterm.toml`).

## Capabilities

### New Capabilities

- `task-board`: provider contract and execution rules, Quests panel, matching tasks to projects,
  starting agents on tasks, example provider.

### Modified Capabilities

- `config-file`: new `providers`, workspace `board`, project `repo` and agent `task_args` keys.

## Impact

- New modules: `board.rs` (pure: contract parsing, grouping, project matching, prompt and command
  building) and `provider.rs` (running the provider process with timeout and output limits).
- `serde_json` dependency (already in the dependency tree).
- `workspace_view.rs`: Files/Quests tabs in the middle column, board state and refresh timer.
- New `docs/providers.md` and `examples/github_issues_provider.rs`.
