## Purpose

Makes the mouse useful in castorterm's terminals: clicking in programs that support it, selecting
and copying text, and answering agents' menus with a click.

## ADDED Requirements

### Requirement: Mouse reporting
When the program enables mouse reporting, castorterm SHALL send it presses, releases, wheel events
and, per the mode requested, drags or all motion, encoded as SGR when requested and as X10
otherwise. Holding Shift SHALL select text instead.

#### Scenario: Click in a TUI
- **WHEN** a program enabled modes 1000 and 1006 and the user clicks cell (5, 10)
- **THEN** the program receives `ESC [ < 0 ; 5 ; 10 M` and, on release, `ESC [ < 0 ; 5 ; 10 m`

### Requirement: Select, copy and paste
Without mouse reporting, dragging with the left button SHALL select text (double click: word,
triple click: line). Ctrl+Shift+C, Ctrl+C while text is selected, and a right click on a selection
SHALL copy it; a right click without selection and Ctrl+Shift+V SHALL paste, bracketed when the
program asked for bracketed paste.

#### Scenario: Copy an error
- **WHEN** the user drags over an error message and presses Ctrl+C
- **THEN** the text is in the clipboard and the shell does not receive Ctrl+C

### Requirement: Clickable menus
For programs that do not read the mouse, a left click on an option of a menu (two or more options
aligned under a `❯` marker, cursor hidden, live screen) SHALL move the marker to that option with
arrow keys, and a click on the marked option SHALL choose it with Enter.

#### Scenario: Trust a folder
- **WHEN** Claude Code asks whether to trust a folder and the user clicks "Yes, I trust this
  folder" and then clicks it again
- **THEN** the marker moves to that option and the second click confirms it

#### Scenario: Shell prompt
- **WHEN** a shell prompt starts with `❯` and the user clicks on it
- **THEN** nothing is sent to the shell
