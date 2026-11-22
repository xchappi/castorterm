# project-actions Specification

## Purpose
Lets the user run common agent work on a project, such as indexing it or reviewing it, with one
click, keeping the results in the repository unless the configuration sends them elsewhere.

## Requirements

### Requirement: Run a project action
castorterm SHALL show the configured project actions as buttons under the selected project and as
palette entries. Running an action SHALL open a session of its agent (or the first agent that
takes a prompt) on the project, started with the action's prompt where `{project}`, `{path}` and
`{repo}` are replaced. If no suitable agent exists, castorterm SHALL say so and start nothing.

#### Scenario: Index a project
- **WHEN** the user clicks "Index project" under project `app`
- **THEN** a Claude Code session opens on `app` with the indexing prompt

### Requirement: Local defaults, configurable
Without `[[actions]]` in the config, castorterm SHALL offer "Index project" and "Suggest
improvements", whose prompts write only to `docs/project/` in the repository. A config with
`[[actions]]` SHALL replace them; an empty list SHALL turn them off.

#### Scenario: Company setup
- **WHEN** the config defines an action "Index project" whose prompt indexes into the wiki
- **THEN** the button runs that prompt instead of the local one
