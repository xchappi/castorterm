## Purpose

Shows the file system of the current workspace next to the workspace panel, so the user can see
what the agents are working on and reference files in a session without typing their paths.

## ADDED Requirements

### Requirement: Workspace file tree
The system SHALL show a file explorer next to the workspace panel with one root folder per project
of the workspace that contains the selected project. The selected project's root SHALL be
expanded automatically when it becomes selected. Clicking a folder SHALL expand or collapse it.

#### Scenario: Explorer follows the selected project
- **WHEN** the user activates a session of project `castorlabs-stack` in workspace `CastorLabs`
- **THEN** the explorer shows the roots of every `CastorLabs` project and `castorlabs-stack` is expanded

#### Scenario: Expand a folder
- **WHEN** the user clicks the collapsed folder `src`
- **THEN** its entries are listed below it, indented one level

### Requirement: Listing rules
Folder contents SHALL be listed folders first, then files, each group sorted by name ignoring
case. The `.git` folder SHALL be hidden. A folder with more than 500 entries SHALL show the first
500 and a row stating how many more there are. A folder that cannot be read SHALL show an error row
instead of its entries. Listings SHALL be refreshed when a folder is expanded and when the user
presses the refresh button.

#### Scenario: Sorted listing
- **WHEN** a folder contains `b.txt`, `A.md` and a folder `src`
- **THEN** the explorer lists `src`, `A.md`, `b.txt`

#### Scenario: Refresh
- **WHEN** a file is created in an expanded folder and the user presses refresh
- **THEN** the new file appears in the tree

### Requirement: File colours
Files SHALL be coloured by type: Rust, JavaScript/TypeScript, other source code, documentation,
configuration and assets each have their own colour; other files use the muted text colour.

#### Scenario: Config file colour
- **WHEN** the tree shows `Cargo.toml` and `main.rs`
- **THEN** they are drawn in the configuration and Rust colours respectively

### Requirement: Insert file paths into the active session
Clicking a file SHALL type its path followed by a space into the active session without pressing
Enter, and SHALL give the terminal keyboard focus. The path SHALL be relative to the active
session's project directory when the file is inside it, absolute otherwise, and SHALL be quoted for
the shell when it contains spaces or shell metacharacters. Nothing SHALL be typed when there is no
active session or the path contains control characters.

#### Scenario: Reference a file for an agent
- **WHEN** a Claude Code session of project `castorterm` is active and the user clicks `src/main.rs`
- **THEN** `src/main.rs ` (with the platform separator) is typed at the session's prompt

#### Scenario: Path with spaces
- **WHEN** the user clicks the file `my notes.md` at the project root
- **THEN** `'my notes.md' ` is typed into the active session

### Requirement: Toggle the explorer
Ctrl+Shift+E SHALL show or hide the explorer. The explorer SHALL be visible on startup.

#### Scenario: Hide the explorer
- **WHEN** the explorer is visible and the user presses Ctrl+Shift+E
- **THEN** the explorer disappears and the terminal grows to use the freed width
