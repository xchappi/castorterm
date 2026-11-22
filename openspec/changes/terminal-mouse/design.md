## Decisions

- **Standard first.** When the program enabled mouse modes, events are encoded exactly like other
  terminals (SGR when 1006 is on, else X10 with its 223-cell limit) and nothing else happens; Shift
  overrides, as in xterm and Windows Terminal.
- **Selection is alacritty's.** `Term::selection` (simple, semantic, lines) is kept in grid
  coordinates, so it follows scrolling; it is drawn from `renderable_content().selection` and cleared
  when the user types.
- **Clickable menus, strictly gated.** Claude Code draws its menus with Ink and does not request the
  mouse (measured: no mouse mode set on the trust prompt). castorterm treats a click as a menu choice
  only when: the program did not ask for the mouse, the cursor is hidden (Ink hides it; shells show
  it), the live screen is shown, and the click is on one of at least two options aligned under the
  bottom-most `❯` line. A click moves the marker (arrow keys); only a click on the already marked
  option sends Enter, so a single stray click never confirms anything.
- **Paste safety.** Bracketed paste strips any end marker inside the text, so pasted text cannot
  break out of the paste and run.
