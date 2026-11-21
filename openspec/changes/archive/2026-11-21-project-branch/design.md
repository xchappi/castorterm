## Decisions

- **Read `HEAD`, never run git.** `HEAD` lives in the checkout's own git directory (for a linked
  worktree, `.git/worktrees/<name>`), not in `commondir`; `ref: refs/heads/x` gives `x`, a bare
  object id gives `@` plus 7 characters, anything else gives nothing. Names are cut at 60 characters
  and control characters are dropped.
- **Refresh with the board timer** (every 5 s), on a background thread, one refresh at a time; the
  view only repaints when a branch changed.
- **Title set in render, only when it changes**, so refreshes from background tasks (which have no
  window) update it too.
