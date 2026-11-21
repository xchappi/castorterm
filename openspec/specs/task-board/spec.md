# task-board Specification

## Purpose
Shows each workspace's tasks from an external tracker inside castorterm and lets the user start an
agent on a task, through a small public provider contract so that castorterm never contains
tracker-specific code or credentials.

## Requirements

### Requirement: Provider contract
A provider SHALL be an executable that castorterm runs as `<command> <args...> list --filter
<filter>` followed by `--project <repo>` for each distinct `repo` of the workspace's projects, in
config order, without a shell, and that prints a UTF-8 JSON object with `version` 1 and a `tasks`
array. When `--project` arguments are given the provider SHOULD return the tasks of all those
projects. Each task SHALL have `id`, `title` and a `status` among `backlog`, `ready`,
`in_progress`, `review` and `done`, and MAY have `priority`, `project` and `url`. The contract SHALL
be documented in `docs/providers.md`.

#### Scenario: Valid listing
- **WHEN** the provider prints `{"version":1,"tasks":[{"id":"PROJ-1","title":"Fix login","status":"ready"}]}`
- **THEN** task `PROJ-1 Fix login` appears in the Ready group

#### Scenario: Invalid tasks are skipped
- **WHEN** the listing contains a task with status `cancelled` or without a title
- **THEN** that task is not shown and the panel states how many tasks were skipped

#### Scenario: Unsupported version
- **WHEN** the provider prints `"version": 2`
- **THEN** no tasks are shown and the panel reports an unsupported protocol version

#### Scenario: Workspace projects are passed to the provider
- **WHEN** workspace `Acme` has projects with `repo = "acme/app"`, `repo = "acme/api"` and one without `repo`, and its board has no filter
- **THEN** the provider runs as `<command> list --filter "" --project acme/app --project acme/api`

#### Scenario: Unsafe repository identifier
- **WHEN** a project has `repo = "--all"` or `repo = "a&calc"`
- **THEN** the repo is ignored with a configuration warning and is never passed to the provider

### Requirement: Safe provider execution
The provider command SHALL be resolved to an absolute path, searching only absolute `PATH` entries,
and SHALL run off the UI thread with its input closed and its own directory as working directory,
at most one run per board at a time. A run SHALL be stopped after 20 seconds together with every
process it started, and its output SHALL be limited to 4 MiB. A non-zero exit code SHALL be
reported with the first line of its error output.

#### Scenario: Provider fails
- **WHEN** the provider exits with code 1 and prints `token missing` on stderr
- **THEN** the panel shows an error containing `token missing` and the terminal keeps working

#### Scenario: Provider hangs
- **WHEN** the provider does not finish within 20 seconds
- **THEN** it is terminated and the panel reports a timeout

### Requirement: Quests panel
The middle column SHALL offer two tabs, Files and Quests. Quests SHALL show the tasks of all the
projects of the selected project's workspace, grouped as In progress, Review, Ready, Backlog and
Done, with Done collapsed by default, each task showing its priority, id and title, and its project
when the workspace has more than one project. Workspaces without a board SHALL show how to
configure one.

#### Scenario: Board of the selected workspace
- **WHEN** the selected project belongs to workspace `Acme`, whose board uses provider `tracker`
- **THEN** the Quests tab lists the tasks returned by that provider for the workspace's filter and projects

#### Scenario: Tasks of several projects
- **WHEN** workspace `Acme` has projects `app` and `api` and the provider returns tasks for both
- **THEN** the Quests tab lists the tasks of both projects together, each row labelled `app` or `api`

### Requirement: Board refresh
The board SHALL be loaded when the Quests tab is shown, when the user presses Refresh and every
`refresh_secs` seconds while it stays visible. A result older than the latest refresh of the same
workspace SHALL be discarded.

#### Scenario: Periodic refresh
- **WHEN** an agent moves a task to `in_progress` in the tracker and the Quests tab stays open
- **THEN** within `refresh_secs` seconds the task appears in the In progress group

### Requirement: Untrusted task data
Text from a provider SHALL be displayed without control or invisible formatting characters, links
SHALL be shown only for `http` and `https` URLs, and any provider text typed into a shell SHALL
reach the agent as a single literal argument, including on Windows when the agent is a `.cmd` or
`.bat` file. Listings SHALL be limited to 500 tasks and titles to 300 characters.

#### Scenario: Crafted title
- **WHEN** a task title contains a newline followed by a command
- **THEN** the title is shown on one line and starting an agent on it never runs that command

#### Scenario: Title attacking a cmd shim
- **WHEN** on Windows a task title is `x" & calc & "y` and the agent is `claude.cmd`
- **THEN** the agent receives the whole title as text and `calc` is not run

### Requirement: Start an agent on a task
Selecting a task SHALL show its project, its link and one button per agent that defines
`task_args`, together with the exact prompt the agent will receive. Activating a button SHALL open a session of that agent in the task's project (matched
by the project's `repo`, then by name; the selected project otherwise) running the agent command
with `task_args`, where `{prompt}` is replaced by the workspace's prompt template rendered for the
task.

#### Scenario: Claude Code on a task
- **WHEN** the agent `Claude Code` has `command = "claude"` and `task_args = ["{prompt}"]`, and the user starts it on task `PROJ-245` whose project is `acme/app`
- **THEN** a `Claude Code` session opens in the directory of the project with `repo = "acme/app"` running `claude 'Work on task PROJ-245: Add a quest board'`

#### Scenario: Agent without task arguments
- **WHEN** an agent has no `task_args`
- **THEN** no button for that agent is shown on tasks

### Requirement: Example provider
The repository SHALL include an example provider for GitHub Issues that implements the contract
using the `gh` command line tool, buildable with `cargo run --example github_issues_provider`. It
SHALL list the issues of every `--project` repository, or of the filter repository when no project
is given.

#### Scenario: GitHub issues on the board
- **WHEN** a workspace board uses the example provider with filter `xchappi/castorterm`
- **THEN** the open issues of that repository appear in the Backlog group

#### Scenario: Issues of every workspace repository
- **WHEN** the workspace projects have `repo = "xchappi/castorterm"` and `repo = "xchappi/other"`
- **THEN** the board shows the issues of both repositories, each with its repository as project
