## Context

The maintainer runs several agents per workspace and tracks the work in CastorTasks, a private
tracker. castorterm is public and must stay tracker-agnostic: the CastorLabs integration has to
live elsewhere, and other teams must be able to plug in GitHub Issues, Linear, Jira and so on.

## Goals / Non-Goals

**Goals:** a stable, tiny contract any tracker can implement in any language; a read-only board
per workspace; one click from a task to an agent working on it; no tracker credentials inside
castorterm.

**Non-Goals:** writing to trackers, a plugin ABI, embedding tracker-specific code.

## Decisions

### External provider processes, not plugins or MCP

- **Provider = executable + JSON on stdout** (like git credential helpers). Language-agnostic, no
  ABI, crash-isolated, and the provider owns its own credentials (env vars, a secret manager...),
  so castorterm never sees a token.
- Rejected: Rust plugins compiled in (forces Rust, private forks, tracker code in this repo);
  castorterm as a generic MCP client (no standard task schema in MCP, so every tracker would need
  fragile tool/field mapping; a provider can still use MCP internally).

### Contract v1

Invocation: `<command> <args...> list --filter <filter>`, no shell, stdin closed, working
directory = the workspace's first project (or the launch directory). Environment:
`CASTORTERM_PROVIDER_PROTOCOL=1`.

Output on stdout, UTF-8 JSON:

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "CL-245",
      "title": "Quest board",
      "status": "in_progress",
      "priority": "P2",
      "project": "xchappi/castorterm",
      "url": "https://..."
    }
  ]
}
```

- `id`, `title`, `status` required; `status` in `backlog | ready | in_progress | review | done`.
  Other statuses (e.g. cancelled) must be omitted by the provider; tasks with unknown statuses or
  missing fields are skipped and counted in a warning.
- `priority` optional, free text shown as a chip (`P0`-`P3` get colours).
- `project` optional: matched to a configured project's `repo` (exact, case-insensitive), then to
  its `name`.
- `url` optional: only `http`/`https` links are shown.
- Exit code non-zero = error; the first line of stderr is shown in the panel.
- `version` other than 1 = error ("unsupported provider protocol version").
- Future versions may add actions (`start`, `done`); v1 providers only need `list`.

### Running providers safely

- `command` is resolved like the shell: an absolute path, or a bare name searched only in absolute
  `PATH` entries (never the current directory), trying `.exe`, `.cmd`, `.bat` on Windows. Run with
  `std::process::Command` (no shell); arguments are passed as an argv.
- Off the UI thread (`background_spawn`), with a 20 s timeout (process killed) and a 4 MiB stdout
  cap. Each refresh gets a ticket; results older than the latest refresh of that workspace are
  dropped.
- Everything from the provider is untrusted: control characters are stripped from displayed text,
  links are restricted to http(s), and anything typed into a shell goes through the existing
  `quote_for_shell` (single literal argument, PowerShell typographic quotes doubled).

### Starting an agent on a task

Agents opt in with `task_args`, an argument template with a `{prompt}` placeholder, e.g.
`["{prompt}"]` for `claude` or `["--prompt", "{prompt}"]` for `opencode`. The prompt comes from the
workspace's `board.prompt` template (default `Work on task {id}: {title}`; placeholders `{id}`,
`{title}`, `{url}`). The session is the usual shell in the task's project (or the selected project
when the task has no match) with `command` followed by each quoted argument typed into it. Agents
without `task_args` get no button: guessing how to pass a prompt would be fragile.

### Read-only board

The board never writes to the tracker. Agents with tracker access (for CastorLabs, the CastorTasks
MCP `task_update`) move tasks themselves; the panel refreshes when opened, on the Refresh button
and every `refresh_secs` (default 60, minimum 15) while the Quests tab is visible.

### UI

The middle column becomes a tabbed panel: **Files** | **Quests** (Ctrl+Shift+E still shows/hides
the column). Quests lists groups In progress, Review, Ready, Backlog and Done (collapsed by
default), each row with a priority chip, the id and the title. Clicking a row selects it and shows
its project, link and agent buttons below it. Workspaces without a board show a short hint on how
to configure one.

### Module layout

- `board.rs` (pure, tested): contract types and parsing/validation, grouping by status, project
  matching, prompt rendering, typed command building.
- `provider.rs`: command resolution and process execution with timeout and output cap (tested with
  a tiny fake provider: the test binary itself re-invoked with a special argument).
- `workspace_view.rs`: tabs, board state per workspace, refresh timer, rendering.

## Risks / Trade-offs

- A slow provider only delays its own panel (background + timeout), never the terminal.
- Polling every 60 s costs one process spawn per visible board; acceptable, configurable.
- The prompt is typed as a command-line argument: it ends up in the shell history like any
  command; templates should not contain secrets.
