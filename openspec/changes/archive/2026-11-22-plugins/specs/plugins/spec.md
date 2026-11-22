## Purpose

Lets a team ship task providers and project actions as a file next to castorterm's config, so its
members get the team's tracker and actions without editing their own configuration.

## ADDED Requirements

### Requirement: Load plugin files
castorterm SHALL read every `*.toml` file in the `plugins` folder next to its config file, in name
order, at most 20 files of at most 256 KiB each. A plugin SHALL only define `[[providers]]` and
`[[actions]]`; any other content SHALL make castorterm skip that plugin and report it.

#### Scenario: Company plugin
- **WHEN** `plugins/company.toml` defines a provider `tasks` and an action "Index into wiki"
- **THEN** boards can use `tasks` and the project shows "Index into wiki" instead of the built-in actions

### Requirement: The user's configuration wins
A provider defined in the user's config SHALL be kept over a plugin provider with the same name.

#### Scenario: Same provider name
- **WHEN** both the config and a plugin define provider `castortasks`
- **THEN** the config's command is used
