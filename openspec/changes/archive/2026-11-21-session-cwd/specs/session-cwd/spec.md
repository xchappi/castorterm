## Purpose

Shows where each session's shell is, so the user can tell at a glance which folder every agent or
shell is working in.

## ADDED Requirements

### Requirement: Track the working directory
castorterm SHALL read each session's working directory from the shell's output without altering
it: OSC 7 `file://` URLs, OSC 9;9 paths and titles starting with `castorterm:cwd:`. On Windows
castorterm SHALL start PowerShell with an integration that, after each prompt, sets that title to
the current location. Payloads with control characters or over 4 KiB SHALL be ignored.

#### Scenario: Change directory in PowerShell
- **WHEN** the user runs `Set-Location D:\proyectos` in a session
- **THEN** after the next prompt the session's directory is `D:\proyectos`

#### Scenario: OSC 7 from bash
- **WHEN** a shell prints `ESC ] 7 ; file://host/home/me/src BEL`
- **THEN** the session's directory is `/home/me/src`

#### Scenario: Agent sets its own title
- **WHEN** an agent sets the window title to anything without the castorterm prefix
- **THEN** the session's directory does not change

### Requirement: Show the working directory
Tabs and sidebar session rows SHALL show the session's directory when it is not the project root:
the path relative to the project inside it, otherwise the path shortened to its last two
components.

#### Scenario: Inside the project
- **WHEN** a session of project `app` is in `app/src/view`
- **THEN** its tab reads `· app · src/view`
