## Why

Indexing a project is only useful if the result is at hand: the information about a project should
be visible in the terminal where its agents run, not in a folder the user has to open.

## What Changes

- A PROJECT tab in the middle column shows the selected project's index: the Markdown files of
  `docs/project/` (overview, stack, architecture, development, state, improvements first), rendered
  simply, with how long ago they were written.
- A project without an index shows its README.md and says it is not indexed yet.
- The tab offers the project actions (Index, Suggest improvements, or the plugin's).

## Capabilities

### New Capabilities
- `project-tab`: the project's index inside castorterm.

### Modified Capabilities

## Impact

New `src/project_index.rs` (bounded reading, small Markdown reader, relative time);
`src/workspace_view.rs` (tab, loading, rendering); README. Showing an index kept in a wiki (the
CastorLabs plugin indexes into Outline) needs a document read in castorlabs-core and comes later.
