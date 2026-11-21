## Purpose

Lets the user build and change workspaces and their project folders from the castorterm window,
so that it works as a visual environment for AI agents without editing the config file by hand.

## ADDED Requirements

### Requirement: Create, rename and remove workspaces
The sidebar SHALL offer a `+ Workspace` action that asks for a name inline and creates an empty
workspace, and each workspace SHALL offer rename and remove actions. Names SHALL be single-line,
non-empty and unique. A workspace with open sessions SHALL NOT be removable.

#### Scenario: New workspace
- **WHEN** the user clicks `+ Workspace`, types `Freelance` and presses Enter
- **THEN** a workspace `Freelance` appears in the sidebar and in `config.toml`

#### Scenario: Duplicate name
- **WHEN** the user tries to create or rename a workspace to the name of another one
- **THEN** nothing changes and the sidebar says the name is taken

#### Scenario: Escape cancels
- **WHEN** the name field is open and the user presses Escape
- **THEN** the field closes without changes and the key never reaches the terminal

#### Scenario: Workspace in use
- **WHEN** a workspace has an open session
- **THEN** its remove action is not available

### Requirement: Add project folders
A workspace SHALL accept folders through a native folder picker (several at once), by dropping
folders onto the sidebar, and with `Ctrl+Shift+O` for the selected workspace. Each folder SHALL
become a project named after the folder, with `repo` detected from its git `origin` remote when it
is a valid repository identifier. A folder already in the workspace SHALL NOT be added twice, and
paths that are not directories SHALL be ignored.

#### Scenario: Folder with a GitHub remote
- **WHEN** the user adds folder `D:/src/app` whose origin is `git@github.com:acme/app.git`
- **THEN** project `app` is added with `repo = "acme/app"` and is selected

#### Scenario: Drop folders
- **WHEN** the user drops two folders on workspace `Acme`
- **THEN** both are added to `Acme`

### Requirement: Remove projects
Each project SHALL offer a remove action that takes it out of its workspace without touching files
on disk. A project with open sessions SHALL NOT be removable.

#### Scenario: Remove a project
- **WHEN** the user removes project `app` from workspace `Acme`
- **THEN** `app` disappears from the sidebar and from `config.toml`, and its folder is untouched

### Requirement: Persist to the config file
Every change SHALL be written to the config file, preserving existing comments and the rest of the
content, creating the file if it does not exist. Adding to the implicit `Local` workspace SHALL
write it with all its projects. If the file cannot be parsed or written, the change SHALL stay for
the current run and the sidebar SHALL show why it was not saved.

#### Scenario: Survives a restart
- **WHEN** the user creates workspace `Freelance`, adds a folder and restarts castorterm
- **THEN** `Freelance` and its project are shown again

#### Scenario: Comments are kept
- **WHEN** the config file has comments and the user adds a project
- **THEN** the comments are still in the file

#### Scenario: Broken config file
- **WHEN** the config file has a syntax error and the user adds a project
- **THEN** the project is shown, the file is not modified and the sidebar explains that it was not saved
