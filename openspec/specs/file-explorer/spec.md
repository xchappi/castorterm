# file-explorer Specification

## Purpose
Shows the file system of the current workspace next to the workspace panel, so the user can see
what the agents are working on and reference files in a session without typing their paths.

## Requirements

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
instead of its entries.

#### Scenario: Sorted listing
- **WHEN** a folder contains `b.txt`, `A.md` and a folder `src`
- **THEN** the explorer lists `src`, `A.md`, `b.txt`

### Requirement: Non-blocking listing
Listings SHALL be refreshed when a folder is expanded and when the user presses the refresh button.
They SHALL be read without blocking the UI; a folder whose listing has not arrived yet shows a
loading row.

#### Scenario: Refresh
- **WHEN** a file is created in an expanded folder and the user presses refresh
- **THEN** the new file appears in the tree

#### Scenario: Slow folder
- **WHEN** the user expands a folder on a slow network drive
- **THEN** a loading row is shown and the terminal keeps responding until the listing arrives

### Requirement: File colours
Files SHALL be coloured by type: Rust, JavaScript/TypeScript, other source code, documentation,
configuration and assets each have their own colour; other files use the muted text colour.

#### Scenario: Config file colour
- **WHEN** the tree shows `Cargo.toml` and `main.rs`
- **THEN** they are drawn in the configuration and Rust colours respectively

### Requirement: Insert file paths into the active session
Clicking a file SHALL type its path followed by a space into the active session without pressing
Enter, and SHALL give the terminal keyboard focus. The path SHALL be relative to the active
session's project directory when the file is inside it, and absolute otherwise. Nothing SHALL be
typed when there is no active session.

#### Scenario: Reference a file for an agent
- **WHEN** a Claude Code session of project `castorterm` is active and the user clicks `src/main.rs`
- **THEN** `src/main.rs ` (with the platform separator) is typed at the session's prompt

### Requirement: Safe quoting of inserted paths
File names are untrusted. An inserted path SHALL be quoted for the session's shell whenever it
contains spaces or shell metacharacters, so that it is always one literal argument, including
names with PowerShell's typographic quotes. A relative path starting with `-` SHALL be prefixed
with `./`. Paths that are not valid Unicode or contain control characters SHALL NOT be typed.

#### Scenario: Path with spaces
- **WHEN** the user clicks the file `my notes.md` at the project root
- **THEN** `'my notes.md' ` is typed into the active session

#### Scenario: Crafted file name
- **WHEN** the user clicks a file whose name contains a typographic quote followed by a command
- **THEN** the whole name is typed as one quoted literal and no part of it can run as a command

#### Scenario: Leading dash
- **WHEN** the user clicks the file `-rf` at the project root
- **THEN** `./-rf ` is typed into the active session

### Requirement: Toggle the explorer
Ctrl+Shift+E SHALL show or hide the explorer. The explorer SHALL be visible on startup.

#### Scenario: Hide the explorer
- **WHEN** the explorer is visible and the user presses Ctrl+Shift+E
- **THEN** the explorer disappears and the terminal grows to use the freed width
