## Decisions

- **Names from the task id, sanitized.** `slug` keeps lowercase ASCII letters, digits, `.`, `_`
  and `-` (others become `-`), collapses runs of `-`/`.` (no `..`), never starts with `-` or `.`,
  never ends in `.lock`, prefixes Windows device names with `t-`, and falls back to `task`. When
  the id had to change beyond letter case, a 16-bit hash of it is appended, so `#12` and `12` never
  share a worktree. Path: `<parent>/<name>.worktrees/<slug>`; branch: `task/<slug>`.
- **git runs like the providers:** resolved from absolute PATH entries, no shell, stdin closed,
  no console window, `GIT_TERMINAL_PROMPT=0`, output drained while it runs, 60 s timeout, in the
  background. Hooks are disabled (`core.hooksPath` set to a missing folder): a post-checkout hook
  running an install would hang the creation. New branches start at the project's current HEAD.
- **Reuse only finished worktrees.** An existing folder is reused only if git sees a HEAD there and
  it is on `task/<slug>`; anything else (an interrupted creation, another branch, a plain folder)
  is reported and never deleted. A failed `add` removes what it left. `git worktree prune` runs
  first, so worktrees deleted by hand can be created again.
- **One creation at a time per worktree:** a second click while it is being created only shows a
  notice.
- **Same repo, persisted.** The worktree project keeps the base project's `repo`, so the board
  still matches its tasks, and is saved like any added folder (implicit workspaces are written
  whole). Removing it from the sidebar does not delete the worktree.
