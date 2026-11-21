## ADDED Requirements

### Requirement: Restore window geometry
castorterm SHALL save the window position, size and maximized state with the sessions, without
writing on every move or resize, including when the window closes, and SHALL open the window with
them next time when they are plausible and intersect a connected screen; otherwise it SHALL use the
default size centered on the main screen.

#### Scenario: Same place as last time
- **WHEN** the user leaves the window maximized on the second monitor and closes castorterm
- **THEN** next time it opens maximized on that monitor

#### Scenario: Monitor unplugged
- **WHEN** the saved position is on a screen that is no longer connected
- **THEN** the window opens with the default size, centered on the main screen
