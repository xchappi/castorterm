## Decisions

- **alacritty's search.** `Term::search_next` with a `RegexSearch` walks the whole history and
  wraps; castorterm only picks the origin (after the match shown, or the bottom/top) in a pure
  `next_match`, tested on a real `Term` without a PTY.
- **Literal, smart case.** The query is escaped into a regex (no regex syntax for users) and gets
  `(?i)` unless it has an uppercase letter; at most 200 characters.
- **One bar, one session.** The bar belongs to the session it was opened on and closes (clearing the
  highlight) before another session becomes active or when its session closes, so keyboard focus
  goes to the right terminal. While open, terminals ignore input, like the palette.
- **Scrolling stays out of TUIs.** In the alternate screen the wheel and Shift+PageUp/Down do
  nothing, so full-screen programs keep their own behaviour. Any key sent to the shell returns the
  view to the bottom.
- **Matches follow the output.** A match is stored with its text and the history size; when new
  output pushes lines into the history it is shifted by that many lines and kept only if its text
  is still there (cleared, reflowed or rotated-out matches are dropped, never mis-highlighted).
- **Cheap while typing.** Each search sweeps the history with the terminal locked, so typing
  searches 120 ms after the last key; Enter searches at once. Wheel movement is clamped (Windows'
  "one screen at a time" reports billions of lines per notch).
- **Placement.** The bar is drawn inside the pane of the session it searches (split view), and the
  palette or a name field close it first: one keyboard overlay at a time. In full-screen programs
  the bar says there is no history instead of searching the alternate screen.
