## Purpose

Gives keyboard access to castorterm's main actions (open an agent in a project, go to a session,
add folders, create a workspace) through a fuzzy-search palette.

## ADDED Requirements

### Requirement: Open and close the palette
`Ctrl+Shift+P` SHALL open a palette over the window. Esc or a click outside it SHALL close it.
While it is open no key or text SHALL reach any terminal; when it closes the active terminal SHALL
get the keyboard back.

#### Scenario: Keys stay out of the terminal
- **WHEN** the palette is open and the user types `claude` and presses Esc
- **THEN** the palette closes and the shell received nothing

### Requirement: Fuzzy filtering
Typing SHALL filter the entries by the letters of the query in order, ignoring case and spaces,
best matches first, showing at most 12. Up and Down SHALL move the selection and Enter SHALL run
the selected entry.

#### Scenario: Agent in a project
- **WHEN** the user types `claude stack`
- **THEN** the first entry is "Open Claude Code" for project `castorlabs-stack`

### Requirement: Palette entries
The palette SHALL offer: go to each open session, open each launcher in each project (the
selected project first), add folders to each workspace, create a workspace, show files and show
quests.

#### Scenario: New workspace from the keyboard
- **WHEN** the user runs "New workspace"
- **THEN** the workspace name field opens and accepts typed names with spaces
