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

#### Scenario: Network or removable drive
- **WHEN** a project is on a UNC path, a mapped network drive or removable media
- **THEN** no branch is shown and the periodic refresh never touches that path

#### Scenario: Branch name with invisible characters
- **WHEN** a branch name contains bidi or zero-width characters
- **THEN** they are removed from the name shown

#### Scenario: Transient read failure
- **WHEN** reading `HEAD` fails once while git rewrites it
- **THEN** the last known branch stays on screen

### Requirement: Safe git reading
The branch SHALL be read from git's files without starting any process, following `gitdir:` only
to local paths, never through a symlink or junction (any reparse point, including cloud files)
inside the project folder, never through reserved Windows device names, and reading at most 1 MiB
per file. A `gitdir:` that points outside the project folder is only checked for being local.

#### Scenario: Network gitdir
- **WHEN** a project's `.git` file says `gitdir: \\host\share\repo`
- **THEN** no branch is shown and the network path is never opened
