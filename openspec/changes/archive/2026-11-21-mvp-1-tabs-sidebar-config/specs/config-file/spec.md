## Purpose

Defines the optional TOML file where the user declares workspaces, projects, agents and the
terminal font: where castorterm looks for it, what it contains and how mistakes in it are reported
without preventing the terminal from starting.

## ADDED Requirements

### Requirement: Config file location
The system SHALL read the config from the path in the `CASTORTERM_CONFIG` environment variable if
set; otherwise from `%APPDATA%\castorterm\config.toml` on Windows and from
`$XDG_CONFIG_HOME/castorterm/config.toml`, falling back to `~/.config/castorterm/config.toml`, on
other platforms.

#### Scenario: Override with environment variable
- **WHEN** `CASTORTERM_CONFIG` points to a file
- **THEN** that file is used instead of the default location

#### Scenario: No config file
- **WHEN** no config file exists
- **THEN** castorterm starts with the default config and shows no error

### Requirement: Config contents
The config file SHALL support: an optional `[font]` table with `family` and `size`; an `agents`
array of `{ name, command }`; and a `workspaces` array of `{ name, projects }` where each project
is `{ name, path }`. Project paths SHALL accept a leading `~` for the home directory. Unknown keys
SHALL be rejected.

#### Scenario: Workspaces and agents shown
- **WHEN** the config defines workspace `CastorLabs` with project `castorterm` and agent `OpenCode`
- **THEN** the sidebar shows `CastorLabs`, under it `castorterm`, with launchers `Shell` and `OpenCode`

#### Scenario: Custom font
- **WHEN** the config sets `font.family = "Cascadia Mono"` and `font.size = 16`
- **THEN** terminals are drawn with that family and size

### Requirement: Config errors do not block startup
If the config file cannot be read or parsed, the system SHALL start with the default config and
show the error in the sidebar. A project whose path is not absolute after `~` expansion SHALL be
skipped and reported the same way.

#### Scenario: Syntax error
- **WHEN** the config file contains invalid TOML
- **THEN** castorterm starts with one shell session and the sidebar shows the parse error

#### Scenario: Relative project path
- **WHEN** a project declares `path = "castorterm"`
- **THEN** that project is not listed and the sidebar shows a warning naming it
