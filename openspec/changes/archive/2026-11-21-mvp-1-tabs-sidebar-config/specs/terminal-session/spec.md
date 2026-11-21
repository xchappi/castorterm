## MODIFIED Requirements

### Requirement: Resize
The system SHALL recompute the grid size (columns and rows) from the size of the terminal area and
the font cell size whenever that area changes size (window resize or sidebar/tab bar layout), and
SHALL resize both the terminal grid and the pseudo-terminal accordingly.

#### Scenario: Window resized
- **WHEN** the user resizes the window
- **THEN** the shell is notified of the new size and subsequent output wraps at the new width

### Requirement: Session exit
The system SHALL close a session's tab when its shell process exits, and SHALL exit the
application when no sessions remain open.

#### Scenario: User types exit
- **WHEN** the user types `exit` and presses Enter in the only open session
- **THEN** the window closes and castorterm exits
