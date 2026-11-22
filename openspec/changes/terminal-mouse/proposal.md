## Why

The terminal ignored the mouse: no clicks in TUIs, no text selection, and Claude Code's menus (trust
this folder, permissions) could only be answered with the keyboard.

## What Changes

- Mouse reporting: programs that ask for it (DECSET 1000/1002/1003, SGR 1006 or X10 encoding) get
  presses, releases, drags, motion and the wheel. Shift bypasses it to select text.
- Selection: drag to select (double click: word, triple: line), Ctrl+Shift+C or Ctrl+C with a
  selection copies; right click copies the selection or pastes; Ctrl+Shift+V pastes (bracketed when
  the program asked for it).
- Wheel in full-screen programs without mouse support (less, man) sends arrow keys (alternate
  scroll).
- Clickable menus of programs that do not read the mouse: measured that Claude Code does not enable
  mouse reporting on its trust prompt. A click on an option under the `❯` marker sends the arrows
  to move there; a click on the marked option sends Enter.

## Capabilities

### New Capabilities
- `terminal-mouse`: mouse in the terminal (reporting, selection, clipboard, clickable menus).

### Modified Capabilities

## Impact

New `src/mouse.rs` (pure encodings and menu detection); `src/view.rs` (handlers, grid metrics,
selection rendering, clipboard); README.
