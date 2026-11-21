## Why

A workspace groups several projects, and its Quests board should show the tasks of all of them
without hand-writing a tracker filter that repeats the projects' repositories. Today the provider
only receives the opaque `filter`, so a board covers whatever that string says, and the list does
not tell which project each task belongs to.

## What Changes

- castorterm passes the workspace's project repositories to the provider:
  `list --filter <filter> --project <repo> --project <repo>...` (one per distinct `repo`, in
  config order). `filter` becomes optional.
- Project `repo` values that start with `-` or contain whitespace are rejected with a warning.
- Each task row shows the project it belongs to when the workspace has more than one project.
- The GitHub Issues example provider lists the issues of every `--project` repository (or of the
  filter when no project has a `repo`).
- `docs/providers.md` documents the `--project` arguments as part of contract v1.

## Capabilities

### New Capabilities

### Modified Capabilities
- `task-board`: provider invocation includes the workspace projects; the panel shows each task's
  project; the example provider aggregates repositories.

## Impact

- `src/provider.rs`, `src/board.rs`, `src/config.rs`, `src/workspace_view.rs`,
  `examples/github_issues_provider.rs`, `docs/providers.md`, `config.example.toml`, README.
- Contract v1 was published in the previous change with no external providers yet; the extra
  arguments are added to v1 instead of bumping the version.
