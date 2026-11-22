## Purpose

Lets the user scroll back through a session's output and find text in it, so errors, paths and
results printed by agents can be located without copying the log elsewhere.

## ADDED Requirements

### Requirement: Scroll the history
The mouse wheel and Shift+PageUp / PageDown SHALL scroll the active session's history, except when
a full-screen program uses the alternate screen. Sending input to the session SHALL return the view
to the bottom.

#### Scenario: Scroll and type
- **WHEN** the user scrolls up with the wheel and then types a key
- **THEN** the view returns to the bottom and the key reaches the shell

### Requirement: Find in the history
Ctrl+Shift+F SHALL open a search bar for the active session. The query SHALL be matched as literal
text, case-insensitively unless it contains an uppercase letter. Enter SHALL show the next older
match and Shift+Enter the next newer one, wrapping around; the match SHALL be highlighted and
scrolled into view. Esc SHALL close the bar, remove the highlight and return to the bottom.

#### Scenario: Find an error
- **WHEN** the user presses Ctrl+Shift+F and types `error`
- **THEN** the most recent line containing `error` or `Error` is shown and highlighted

#### Scenario: No match
- **WHEN** the query appears nowhere in the history
- **THEN** the bar says there are no matches and the view does not move
