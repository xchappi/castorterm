## MODIFIED Requirements

### Requirement: Sidebar tree
The system SHALL show a sidebar listing every workspace, under it every project, and under each
project its open sessions. One project is the selected project: clicking a project name SHALL
select it, and activating or launching a session SHALL select that session's project. The selected
project and the active session SHALL be highlighted. Clicking a session SHALL make it the active
session and give it keyboard focus.

#### Scenario: Activate from the sidebar
- **WHEN** the user clicks a session listed under a project
- **THEN** that session becomes active, is drawn in the terminal area and receives keyboard input

#### Scenario: Select a project
- **WHEN** the user clicks the name of project `castorlabs-stack`
- **THEN** `castorlabs-stack` becomes the selected project and its launchers are shown

### Requirement: Agent launchers
The system SHALL show, under the selected project only, one launcher per configured agent preceded
by a built-in "Shell" launcher. Activating a launcher SHALL start the default shell with the
project path as working directory and, for agents, SHALL type the agent command followed by Enter
into it. The new session SHALL become active. Sessions are labelled with the launcher name, with a
numeric suffix when the project already has a session with that name.

#### Scenario: Start Claude Code in a project
- **WHEN** the config defines an agent `Claude Code` with command `claude` and the user activates its launcher on project `castorterm`
- **THEN** a new session labelled `Claude Code` appears under `castorterm`, its shell runs in the project directory and `claude` is executed in it

#### Scenario: Second shell in the same project
- **WHEN** the project already has a session labelled `Shell` and the user starts another shell
- **THEN** the new session is labelled `Shell 2`

#### Scenario: Project directory missing
- **WHEN** the user activates a launcher on a project whose path does not exist
- **THEN** no session is created and an error naming the path is shown in the sidebar

#### Scenario: Launchers of other projects hidden
- **WHEN** project `castorterm` is selected
- **THEN** the launchers of every other project are not shown

### Requirement: Keyboard shortcuts
The system SHALL handle these shortcuts before forwarding keys to the terminal: Ctrl+Shift+T opens
a new shell in the active session's project, Ctrl+Shift+W closes the active session, Ctrl+Tab
activates the next session and Ctrl+Shift+Tab the previous one, wrapping around, and Ctrl+Shift+E
shows or hides the file explorer.

#### Scenario: Cycle sessions
- **WHEN** three sessions are open, the third is active and the user presses Ctrl+Tab
- **THEN** the first session becomes active
