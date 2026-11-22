## Purpose

Lets the user name open sessions and arrange their tabs, so several agents on the same project can
be told apart and kept in a meaningful order across restarts.

## ADDED Requirements

### Requirement: Rename sessions
The user SHALL be able to rename an open session by double-clicking its tab or sidebar row, or
from the command palette. Names SHALL be trimmed, 1 to 40 characters, without control or
invisible characters; other names SHALL be refused without changing the session.

#### Scenario: Name a session
- **WHEN** the user double-clicks the tab `Shell 2`, types `Reviewer` and presses Enter
- **THEN** the tab and its sidebar row read `Reviewer`

#### Scenario: Name restored
- **WHEN** castorterm reopens sessions saved with a name
- **THEN** each one shows its saved name, unless that name is not valid

### Requirement: Reorder tabs
The user SHALL be able to move a tab by dragging it onto another (it takes that tab's place) or
with Ctrl+Shift+PageUp / PageDown. The active session SHALL stay active and the order SHALL be
saved.

#### Scenario: Move right with the keyboard
- **WHEN** the active tab is first of three and the user presses Ctrl+Shift+PageDown
- **THEN** it becomes second and is still active
