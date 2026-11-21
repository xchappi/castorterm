# Task providers

castorterm can show a task board for each workspace (the **Quests** tab) and
start an agent on a task. It does not talk to any tracker itself: a
**provider** does. A provider is any executable, written in any language,
that implements the small contract below. castorterm never sees tracker
credentials; the provider handles them (environment variables, a secret
manager, a CLI that is already logged in...).

An example for GitHub Issues lives in
[`examples/github_issues_provider.rs`](../examples/github_issues_provider.rs).

## Configuration

```toml
[[providers]]
name = "github"                     # referenced by workspace boards
command = "github_issues_provider"  # absolute path, or a name found in PATH
args = []                           # optional extra arguments, before `list`

[[agents]]
name = "Claude Code"
command = "claude"
task_args = ["{prompt}"]            # how to pass the task prompt to the agent

[[workspaces]]
name = "My workspace"
board = { provider = "github", prompt = "Work on task {id}: {title}", refresh_secs = 60 }

[[workspaces.projects]]
name = "app"
path = "~/src/app"
repo = "owner/app"                  # passed to the provider; its tasks open here

[[workspaces.projects]]
name = "api"
path = "~/src/api"
repo = "owner/api"
```

- The board shows the tasks of every project of the workspace: each distinct
  `repo` is passed to the provider as `--project`. A `repo` may only contain
  ASCII letters, digits and `. _ - /`, and must not start with `-`.
- `filter` is optional and opaque to castorterm: it is passed to the provider
  unchanged, to narrow the listing further (labels, assignee...).
- `prompt` accepts `{id}`, `{title}` and `{url}`; the default is
  `Work on task {id}: {title}`.
- `refresh_secs` defaults to 60 and cannot be lower than 15.
- Only agents with `task_args` get a button on tasks. `{prompt}` is replaced
  by the rendered prompt and every argument is quoted for the shell.

## Contract, version 1

### Invocation

```
<command> <args...> list --filter <filter> [--project <repo>]...
```

- `--project <repo>` is repeated once per distinct `repo` of the workspace's
  projects, in config order (none when no project has a `repo`). The filter
  is always passed, possibly empty. When `--project` is given, return the
  tasks of all those projects and set each task's `project` so castorterm
  knows where it belongs.
- Run directly, without a shell. Standard input is closed.
- Working directory: the directory that contains the provider executable,
  never a project (projects may be untrusted clones, and Windows searches
  the working directory for programs and DLLs). If your provider starts other
  programs, resolve them to absolute paths too.
- The whole process tree is killed when the run times out.
- Environment: `CASTORTERM_PROVIDER_PROTOCOL=1`.
- `command` is resolved to an absolute path searching only absolute `PATH`
  entries (never the current directory). On Windows, a bare name tries
  `.exe`, `.cmd` and `.bat` first, then the name as is.

### Output

UTF-8 JSON on standard output:

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "PROJ-245",
      "title": "Add a quest board",
      "status": "in_progress",
      "priority": "P2",
      "project": "owner/repo",
      "url": "https://tracker.example/PROJ-245"
    }
  ]
}
```

| Field | Required | Notes |
|---|---|---|
| `id` | yes | Shown as is; usually the tracker key. No whitespace or control characters; must be unique across all projects. |
| `title` | yes | Control and invisible formatting characters are replaced by spaces; cut at 300 characters. |
| `status` | yes | One of `backlog`, `ready`, `in_progress`, `review`, `done`. Omit tasks in other states (e.g. cancelled). |
| `priority` | no | Free text, cut at 64 characters; `P0`-`P3` get colours. |
| `project` | no | Matched to a project's `repo`, then to its `name` (case-insensitive); cut at 64 characters. |
| `url` | no | Only `http` and `https` links are shown. |

Tasks with missing fields, unknown statuses or duplicate ids are skipped and
counted in a warning. Only the first 500 valid tasks are shown. A `version`
other than `1` is an error.

### Errors and limits

- Exit with a non-zero code on failure; the first line of standard error is
  shown in the panel.
- A run is stopped after 20 seconds, with every process it started.
- Output larger than 4 MiB is rejected.

### Security notes for provider authors

- Everything you print is treated as untrusted: titles are cleaned, links are
  limited to http(s) and anything typed into a shell is quoted as a single
  argument. On Windows the characters `cmd.exe` interprets (`" % & | < > ^ !`)
  are replaced by look-alikes, because agents installed by npm are `.cmd`
  shims. Still, do not include secrets in titles or links.
- Task titles end up in an agent's prompt, so they are also untrusted input
  for the agent. castorterm shows the exact prompt before the agent is
  started; review it for tasks from sources you do not control.
- Read credentials from your own environment or secret store; castorterm
  passes none.

## Versioning

New versions may add commands (for example `start` and `done` to move tasks)
and fields. castorterm will keep accepting version 1 output.
