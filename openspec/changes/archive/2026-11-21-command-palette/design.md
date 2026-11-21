## Decisions

- **Model apart from the view.** `commands::score` is a subsequence match (ignoring case and
  spaces in the query) that rewards consecutive letters and word starts; it tries every place the
  first letter appears and keeps the best, so "castor" matches the word "castorterm" rather than
  the `c` of an earlier word. `filter` keeps the 12 best, stable among ties.
- **Entries built when the palette opens.** Actions carry indices; running one re-checks that the
  project is still listed and the workspace still exists.
- **Keyboard ownership.** Opening the palette or the name field focuses the root view (its own
  focus handle) and pauses the terminals' input gate; closing gives focus back to the active
  terminal. Without a focused element GPUI dispatches no key events, which is why `window.blur()`
  did not work.
