## MODIFIED Requirements

### Requirement: Show agent status
castorterm SHALL read the status files of its open sessions at least every second, without
blocking the window, and SHALL show the status next to each session in the sidebar and in its tab,
with `waiting` highlighted, and the number of waiting agents in the HUD. A session's file SHALL be
removed when the session closes. The command palette SHALL list each session with its status,
waiting sessions first.

#### Scenario: Agent waiting
- **WHEN** an agent reports `waiting`
- **THEN** within a second its session shows `waiting` and the HUD reads "1 waiting for you"

#### Scenario: Palette shows statuses
- **WHEN** the user opens the palette while one agent waits
- **THEN** "Go to" that session is the first entry and its detail says `waiting`

## ADDED Requirements

### Requirement: Get the user's attention
When an agent starts waiting and the castorterm window is not the active window, castorterm SHALL
request the user's attention; on Windows the taskbar button SHALL flash until the window comes to
the front.

#### Scenario: Window in the background
- **WHEN** the user is in another application and an agent reports `waiting`
- **THEN** castorterm's taskbar button flashes until the user switches back

### Requirement: Jump to a waiting agent
`Ctrl+Shift+J` SHALL activate the next session, after the active one and wrapping around, whose
agent is waiting. With no waiting agent it SHALL do nothing.

#### Scenario: Jump
- **WHEN** the active session is a shell and another session's agent waits
- **THEN** `Ctrl+Shift+J` activates that session
