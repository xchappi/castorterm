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
board = { provider = "github", filter = "owner/repo", prompt = "Work on task {id}: {title}", refresh_secs = 60 }

[[workspaces.projects]]
name = "app"
path = "~/src/app"
repo = "owner/repo"                 # tasks whose `project` is this go here
```

- `filter` is opaque to castorterm: it is passed to the provider unchanged.
- `prompt` accepts `{id}`, `{title}` and `{url}`; the default is
  `Work on task {id}: {title}`.
- `refresh_secs` defaults to 60 and cannot be lower than 15.
- Only agents with `task_args` get a button on tasks. `{prompt}` is replaced
  by the rendered prompt and every argument is quoted for the shell.

## Contract, version 1

### Invocation

```
<command> <args...> list --filter <filter>
```

- Run directly, without a shell. Standard input is closed.
- Working directory: the first project of the workspace.
- Environment: `CASTORTERM_PROVIDER_PROTOCOL=1`.
- `command` is resolved to an absolute path searching only absolute `PATH`
  entries (never the current directory). On Windows, a bare name also tries
  `.exe`, `.cmd` and `.bat`.

### Output

UTF-8 JSON on standard output:

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "CL-245",
      "title": "Quest board",
      "status": "in_progress",
      "priority": "P2",
      "project": "owner/repo",
      "url": "https://tracker.example/CL-245"
    }
  ]
}
```

| Field | Required | Notes |
|---|---|---|
| `id` | yes | Shown as is; usually the tracker key. |
| `title` | yes | Control characters are replaced by spaces. |
| `status` | yes | One of `backlog`, `ready`, `in_progress`, `review`, `done`. Omit tasks in other states (e.g. cancelled). |
| `priority` | no | Free text; `P0`-`P3` get colours. |
| `project` | no | Matched to a project's `repo`, then to its `name` (case-insensitive). |
| `url` | no | Only `http` and `https` links are shown. |

Tasks with missing fields or unknown statuses are skipped and counted in a
warning. A `version` other than `1` is an error.

### Errors and limits

- Exit with a non-zero code on failure; the first line of standard error is
  shown in the panel.
- A run is stopped after 20 seconds.
- Output larger than 4 MiB is rejected.

### Security notes for provider authors

- Everything you print is treated as untrusted: titles are cleaned, links are
  limited to http(s) and anything typed into a shell is quoted. Still, do not
  include secrets in titles or links.
- Read credentials from your own environment or secret store; castorterm
  passes none.

## Versioning

New versions may add commands (for example `start` and `done` to move tasks)
and fields. castorterm will keep accepting version 1 output.
