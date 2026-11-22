# session-tabs Specification

## Purpose
Lets one castorterm window host many terminal sessions organised as Workspace -> Project ->
Session, so the user can run different agents in different projects side by side and switch
between them from a sidebar, a tab bar or the keyboard.

## Requirements

### Requirement: Multiple concurrent sessions
The system SHALL allow several terminal sessions to be open at the same time in one window. Exactly
one session is active and drawn in the terminal area; inactive sessions keep running and keep
their content.

#### Scenario: Background session keeps running
- **WHEN** a long-running command is started in one session and the user activates another session
- **THEN** the command keeps running and its output is visible when the first session is activated again

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

### Requirement: Tab bar
The system SHALL show a tab bar above the terminal area with one tab per open session, labelled
with the session and project names, highlighting the active one. Clicking a tab SHALL activate it
and clicking its close button SHALL close that session.

#### Scenario: Close from the tab bar
- **WHEN** the user clicks the close button of a tab
- **THEN** that session's shell is terminated and the tab disappears from the tab bar and the sidebar

### Requirement: Keyboard shortcuts
The system SHALL handle these shortcuts before forwarding keys to the terminal: Ctrl+Shift+T opens
a new shell in the active session's project, Ctrl+Shift+W closes the active session, Ctrl+Tab
activates the next session and Ctrl+Shift+Tab the previous one, wrapping around, and Ctrl+Shift+E
shows or hides the file explorer.

#### Scenario: Cycle sessions
- **WHEN** three sessions are open, the third is active and the user presses Ctrl+Tab
- **THEN** the first session becomes active

### Requirement: Closing sessions
When a session closes (its shell exits or the user closes it) the system SHALL remove it, and if it
was active SHALL activate the session to its right, or to its left if it was the last one. The
system SHALL exit when the last session closes.

#### Scenario: Last session exits
- **WHEN** the only open session's shell exits
- **THEN** castorterm exits

#### Scenario: Close the active middle tab
- **WHEN** three sessions are open, the second is active and it is closed
- **THEN** the session that was third becomes active

### Requirement: Initial session
On launch the system SHALL open one shell session in the launch directory. If a configured project
has that same directory as its path, the session SHALL belong to that project; otherwise it SHALL
belong to a project named after the directory inside an implicit workspace called "Local".

#### Scenario: Launch inside a configured project
- **WHEN** castorterm is launched from the path of the configured project `castorterm`
- **THEN** the initial session is listed under `castorterm`

#### Scenario: Launch elsewhere
- **WHEN** castorterm is launched from a directory that is not a configured project
- **THEN** the initial session is listed under a project named after that directory in workspace `Local`

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
