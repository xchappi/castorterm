## Purpose

Lets the user start an agent on a task in a git worktree and branch of its own, so several agents
can work on the same repository in parallel without interfering.

## ADDED Requirements

### Requirement: Worktree toggle
Task details SHALL offer a toggle "In a new worktree" that shows the branch that will be used.
When it is on, starting an agent on the task SHALL use a worktree instead of the project folder.

#### Scenario: Toggle shows the branch
- **WHEN** the user selects task `CL-7`
- **THEN** the toggle reads "In a new worktree (task/cl-7)"

### Requirement: Create or reuse the worktree
castorterm SHALL create the worktree `<project>.worktrees/<task>` next to the project on branch
`task/<task>` with `git worktree add`, running git without a shell, in the background, with a
timeout. An existing worktree SHALL be reused, an existing branch SHALL be checked out, and any
other folder at that path SHALL be left untouched and reported as an error. Task ids SHALL be
reduced to safe names that never start with `-` or `.`.

#### Scenario: First start on a task
- **WHEN** the toggle is on and the user starts Claude Code on task `CL-7` of project `app`
- **THEN** `app.worktrees/cl-7` exists on branch `task/cl-7`, project `app@cl-7` appears in the workspace and Claude Code starts there with the task prompt

#### Scenario: Second start on the same task
- **WHEN** the worktree of `CL-7` already exists
- **THEN** it is reused and no new project is added

#### Scenario: Folder in the way
- **WHEN** `app.worktrees/cl-7` exists but is not a worktree
- **THEN** nothing is created or deleted and the sidebar shows the error

### Requirement: Worktree project
The worktree project SHALL keep the base project's `repo` and SHALL be saved to the config file like
any added folder. Removing it from the sidebar SHALL NOT delete the worktree.

#### Scenario: Board still matches
- **WHEN** a worktree project of `acme/app` exists
- **THEN** tasks of `acme/app` keep showing on the workspace board
