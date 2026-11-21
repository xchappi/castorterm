# terminal-session Specification

## Purpose
Defines how castorterm hosts a single interactive terminal session in a native window: which
shell it starts, how output is rendered, how keyboard input reaches the shell, and how resize
and exit are handled. Later capabilities (tabs, sidebar, agent sessions) build on this one.

## Requirements

### Requirement: Default shell session on launch
The system SHALL open a native window on launch and start the platform default shell inside a
pseudo-terminal: `pwsh` if available on Windows, otherwise `powershell.exe`; on other platforms
the value of `$SHELL`, falling back to `/bin/bash`. The shell's working directory SHALL be the
directory castorterm was launched from.

#### Scenario: Launch on Windows with PowerShell 7 installed
- **WHEN** the user runs `castorterm` on Windows and `pwsh` is on the PATH
- **THEN** a window opens showing a `pwsh` prompt in the launch directory

#### Scenario: Launch on Windows without PowerShell 7
- **WHEN** the user runs `castorterm` on Windows and `pwsh` is not on the PATH
- **THEN** a window opens showing a `powershell.exe` prompt

#### Scenario: Version flag
- **WHEN** the user runs `castorterm --version`
- **THEN** the version is printed to stdout and no window is opened

### Requirement: Render terminal output
The system SHALL parse the shell output as a VT/ANSI stream and render the visible grid using a
monospace font, honouring foreground and background colors (16-color, 256-color and truecolor),
bold text and the cursor position.

#### Scenario: Colored output
- **WHEN** the shell prints text with ANSI color escape sequences
- **THEN** the text is drawn with the corresponding foreground and background colors

#### Scenario: Cursor visible
- **WHEN** the shell is waiting at a prompt
- **THEN** a cursor is drawn at the terminal cursor position

### Requirement: Keyboard input
The system SHALL forward keyboard input to the shell: printable characters as UTF-8, and Enter,
Backspace, Tab, Escape, arrow keys, Home, End, Delete and Ctrl+letter combinations as their
standard terminal byte sequences.

#### Scenario: Typing a command
- **WHEN** the user types `echo hola` and presses Enter
- **THEN** the shell receives the text followed by a carriage return and the output `hola` is rendered

#### Scenario: Interrupt
- **WHEN** the user presses Ctrl+C while a command is running
- **THEN** the byte 0x03 is sent to the shell

### Requirement: Resize
The system SHALL recompute the grid size (columns and rows) from the size of the terminal area and
the font cell size whenever that area changes size (window resize or sidebar/tab bar layout), and
SHALL resize both the terminal grid and the pseudo-terminal accordingly.

#### Scenario: Window resized
- **WHEN** the user resizes the window
- **THEN** the shell is notified of the new size and subsequent output wraps at the new width

### Requirement: Session exit
The system SHALL close a session's tab when its shell process exits, and SHALL exit the
application when no sessions remain open.

#### Scenario: User types exit
- **WHEN** the user types `exit` and presses Enter in the only open session
- **THEN** the window closes and castorterm exits
