## Why

Getting an agent to document a project or review it means typing the same long prompt every time,
and the result ends up scattered. castorterm should offer it as one click, keep the result in the
repository by default, and let a team point it at its own systems instead.

## What Changes

- Project actions: a name, a prompt template (`{project}`, `{path}`, `{repo}`) and optionally an
  agent. Running one opens a session of that agent on the project with the prompt.
- Built in (when the config defines none): **Index project** writes `docs/project/` (overview,
  stack, architecture, development, state) and **Suggest improvements** writes
  `docs/project/improvements.md`; nothing leaves the machine.
- `[[actions]]` in the config replaces them (an empty list turns them off), so a company setup can
  index into its wiki instead.
- Buttons under the selected project in the sidebar and palette entries for the selected project.

## Capabilities

### New Capabilities
- `project-actions`: one-click agent actions on a project.

### Modified Capabilities

## Impact

New `src/actions.rs`; `src/config.rs` (`[[actions]]`), `src/workspace_view.rs` (buttons,
`run_action`), palette (`commands.rs`, `command_palette.rs`); README and example config.
