## Purpose

Shows what each agent is doing (working, waiting for the user, done) so the user knows at a glance
which session needs attention, using a small channel any agent can drive from its hooks.

## ADDED Requirements

### Requirement: Status channel
Every session SHALL get the environment variable `CASTORTERM_STATUS_FILE` naming a file of its own
in the `status` folder of the config directory. `castorterm --status <working|waiting|done>`
SHALL write that status to the file named by the variable, SHALL do nothing when the variable is
unset, SHALL refuse paths that are not castorterm status files, and SHALL always exit with code 0.

#### Scenario: Hook reports waiting
- **WHEN** Claude Code's Notification hook runs `castorterm --status waiting` inside a castorterm session
- **THEN** that session's status file contains `waiting`

#### Scenario: Outside castorterm
- **WHEN** the same hook runs in a terminal that is not castorterm
- **THEN** nothing is written and the hook succeeds

#### Scenario: Abused variable
- **WHEN** `CASTORTERM_STATUS_FILE` points to `config.toml`
- **THEN** the file is not touched

### Requirement: Show agent status
castorterm SHALL read the status files of its open sessions at least every second, without
blocking the window, and SHALL show the status next to each session in the sidebar and in its tab,
with `waiting` highlighted, and the number of waiting agents in the HUD. A session's file SHALL be
removed when the session closes.

#### Scenario: Agent waiting
- **WHEN** an agent reports `waiting`
- **THEN** within a second its session shows `waiting` and the HUD reads "1 waiting for you"
