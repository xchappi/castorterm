## Decisions

- **A pair, not a layout tree.** Two panes side by side cover the common case (agent + shell)
  with no layout state; deeper splits can come later. The pair is `(left, right)`; the active
  session is always one of them, otherwise the split ends.
- **Following the active session.** Every change of active session goes through `split::follow`
  with the previous one: switching panes keeps the pair, another session replaces the pane that
  was active, anything it cannot place ends the split. Pure and tested.
- **Panes resize themselves.** Each terminal view already fits its PTY to its bounds, so splitting
  only changes the layout; the inactive pane is dimmed and keeps running.
- **Not persisted.** Restoring sessions reopens tabs only; the split is a view choice for the run.
