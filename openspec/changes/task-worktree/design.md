## Decisions

- **Names from the task id, sanitized.** `slug` keeps lowercase ASCII letters, digits, `.`, `_`
  and `-` (others become `-`, collapsed), trims leading/trailing dots and dashes so it can be
  neither hidden nor a flag, caps 40 characters, and falls back to `task`. Path:
  `<parent>/<name>.worktrees/<slug>`; branch: `task/<slug>`.
- **git runs like the providers:** resolved from absolute PATH entries, no shell, stdin closed,
  no console window, `GIT_TERMINAL_PROMPT=0`, 30 s timeout, in the background. It runs in the
  user's own repository (their hooks apply, as when they run git themselves). Arguments are
  separate tokens; the slug never starts with `-`.
- **Reuse, never overwrite.** An existing worktree (folder with `.git`) is reused; an existing
  branch is checked out in a new worktree; any other folder in the way is an error.
- **Same repo, persisted.** The worktree project keeps the base project's `repo`, so the board
  still matches its tasks, and is saved like any added folder (implicit workspaces are written
  whole). Removing it from the sidebar does not delete the worktree.
