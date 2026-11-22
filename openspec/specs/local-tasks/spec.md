# local-tasks Specification

## Purpose
Gives every workspace a task board with no setup, built from files kept in its projects, so open
source users can plan and start agent work without a tracker.

## Requirements

### Requirement: Local board
A workspace without a configured board SHALL show a board built from its projects: each checklist
item of a project's `TASKS.md`, with the status of the section it is under (ready by default) and
done when checked, and one task per OpenSpec change in progress with its progress. No command SHALL
run for it.

#### Scenario: TASKS.md
- **WHEN** `TASKS.md` has `- [ ] Dark mode` under `## In progress`
- **THEN** the board shows "Dark mode" as in progress

#### Scenario: OpenSpec change
- **WHEN** `openspec/changes/add-login/tasks.md` has one of three steps checked
- **THEN** the board shows "OpenSpec change add-login (1/3 tasks done)" as in progress

### Requirement: Explain an empty local board
An empty local board SHALL say which files it reads.

#### Scenario: Nothing yet
- **WHEN** no project has a `TASKS.md` or an OpenSpec change
- **THEN** the board explains how to add tasks
