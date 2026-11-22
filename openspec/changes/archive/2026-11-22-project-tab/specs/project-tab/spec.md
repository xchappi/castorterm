## Purpose

Shows the selected project's index inside castorterm, so what an agent learned about the project is
visible next to the sessions that work on it.

## ADDED Requirements

### Requirement: Show the project index
The PROJECT tab SHALL show the Markdown files in the selected project's `docs/project/` folder,
overview, stack, architecture, development, state and improvements first, with how long ago they
were last changed. Without them it SHALL show the project's README.md and state that the project is
not indexed yet.

#### Scenario: Indexed project
- **WHEN** `docs/project/overview.md` and `state.md` exist and the user opens the PROJECT tab
- **THEN** both are shown, overview first, with "Indexed ... ago"

#### Scenario: Not indexed
- **WHEN** the project has no `docs/project/` folder
- **THEN** its README is shown with "Not indexed yet" and the Index button

### Requirement: Act from the tab
The PROJECT tab SHALL offer the project actions for the selected project.

#### Scenario: Index from the tab
- **WHEN** the user clicks Index project in the tab
- **THEN** the same agent session opens as from the sidebar button
