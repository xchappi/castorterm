## Context

The maintainer runs several agents per workspace and tracks the work in a private tracker.
castorterm is public and must stay tracker-agnostic: that integration has to live elsewhere, and
other teams must be able to plug in GitHub Issues, Linear, Jira and so on.

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
directory = the provider's own directory (never a project: projects may be untrusted clones and
Windows searches the working directory for programs and DLLs). Environment:
`CASTORTERM_PROVIDER_PROTOCOL=1`.

Output on stdout, UTF-8 JSON:

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "PROJ-245",
      "title": "Add a quest board",
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
  `PATH` entries (never the current directory). On Windows `.exe`, `.cmd` and `.bat` are tried
  before the bare name, because npm installs a POSIX script `foo` next to `foo.cmd`. Run with
  `std::process::Command` (no shell); arguments are passed as an argv.
- Off the UI thread (`background_spawn`), one run per board at a time, with a 20 s timeout that
  kills the whole process tree (`taskkill /T` on Windows, a process group on Unix). Output pipes are
  drained to the end but only 4 MiB of stdout and 64 KiB of stderr are kept, so an oversized output
  is reported as such instead of blocking until the timeout. Each refresh gets a ticket; results
  older than the latest refresh of that workspace are dropped.
- Everything from the provider is untrusted: control and invisible formatting characters (bidi
  overrides, zero-width) are stripped from displayed text, ids with whitespace or duplicates are
  skipped, titles are cut at 300 characters and listings at 500 tasks, links are restricted to
  http(s), and anything typed into a shell goes through `quote_for_shell` (single literal argument,
  PowerShell typographic quotes doubled).

### Starting an agent on a task

Agents opt in with `task_args`, an argument template with a `{prompt}` placeholder, e.g.
`["{prompt}"]` for `claude` or `["--prompt", "{prompt}"]` for `opencode`. The prompt comes from the
workspace's `board.prompt` template (default `Work on task {id}: {title}`; placeholders `{id}`,
`{title}`, `{url}`). The session is the usual shell in the task's project (or the selected project
when the task has no match) with `command` followed by each quoted argument typed into it. Agents
without `task_args` get no button: guessing how to pass a prompt would be fragile.

Two extra rules protect the typed command:

- On Windows, PowerShell passes arguments to `.cmd`/`.bat` files (npm shims such as `claude.cmd`)
  without escaping embedded `"`, and `cmd.exe` then interprets `&`, `|`, `%`... A title like
  `x" & calc & "y` would run `calc` (reproduced). The prompt's `" % & | < > ^ !` are therefore
  replaced by Unicode look-alikes on Windows before quoting.
- A prompt starting with `-` is prefixed with `Task: ` so the agent cannot read it as an option.

The title is also untrusted input for the agent itself (prompt injection), which quoting cannot
prevent. The task details show the exact rendered prompt before the user starts an agent.

### Read-only board

The board never writes to the tracker. Agents with tracker access (for example through an MCP
server of the tracker) move tasks themselves; the panel refreshes when opened, on the Refresh button
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
