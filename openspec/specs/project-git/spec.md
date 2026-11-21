# project-git Specification

## Purpose
Shows the git state of each project, starting with its current branch, so the user can see at a
glance where every agent is working.

## Requirements

### Requirement: Current branch per project
The sidebar SHALL show the current git branch of each project that is a git checkout, refreshed in
the background at least every 5 seconds, and the window title SHALL include the branch of the
active session's project. A detached HEAD SHALL be shown as `@` followed by the first 7 characters
of the commit id. Projects that are not git checkouts SHALL show no branch.

#### Scenario: Agent switches branch
- **WHEN** an agent runs `git switch -c feat/x` in project `app`
- **THEN** within a few seconds the sidebar shows `feat/x` next to `app`

#### Scenario: Linked worktree
- **WHEN** a project is a linked worktree on branch `task-1`
- **THEN** the sidebar shows `task-1`, not the branch of the main checkout

#### Scenario: Not a repository
- **WHEN** a project folder has no `.git`
- **THEN** no branch is shown and nothing fails

### Requirement: Safe git reading
The branch SHALL be read from git's files without starting any process, following `gitdir:` only
to local paths, never through a symlink or junction inside the project folder, and reading at most
1 MiB per file.

#### Scenario: Network gitdir
- **WHEN** a project's `.git` file says `gitdir: \\host\share\repo`
- **THEN** no branch is shown and the network path is never opened
