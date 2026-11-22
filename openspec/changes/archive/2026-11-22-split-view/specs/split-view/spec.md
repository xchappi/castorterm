## Purpose

Shows two sessions side by side so the user can watch an agent while working in another session
of the same or another project.

## ADDED Requirements

### Requirement: Split the view
Ctrl+Shift+D SHALL show the active session next to the previous session in tab order (or the next
one if it is first); with a single session it SHALL open a shell in the same project and show it
on the right. Pressing it while split SHALL go back to one session.

#### Scenario: Split with two sessions
- **WHEN** sessions A and B are open, B is active, and the user presses Ctrl+Shift+D
- **THEN** A is shown on the left and B on the right, and B stays active

#### Scenario: Split with one session
- **WHEN** only session A is open and the user presses Ctrl+Shift+D
- **THEN** a new shell opens in A's project on the right and becomes active

### Requirement: Panes follow the active session
Clicking a pane SHALL make its session active. Activating a session outside the pair SHALL put it
in the pane that was active. Closing either session SHALL end the split.

#### Scenario: Activate another tab while split
- **WHEN** A and B are split, B is active, and the user activates C
- **THEN** A and C are shown, C active
