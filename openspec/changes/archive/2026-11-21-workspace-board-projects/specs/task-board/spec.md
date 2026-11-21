## MODIFIED Requirements

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
