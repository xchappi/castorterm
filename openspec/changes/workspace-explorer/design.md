## Context

After MVP-1 and the Neon HUD look, the window has a single sidebar with workspaces, projects,
launchers and sessions, plus the tabs and the terminal. The maintainer asked to see the file system
of the workspace next to the workspaces.

## Decisions

### Layout

`HUD` on top; below it three columns: workspace panel (220 px), explorer (260 px, can be hidden
with Ctrl+Shift+E) and the tab bar + terminal taking the rest. The default window grows to
1360x820 so the terminal keeps ~100 columns with both panels open.

### Selected project

`WorkspaceView` keeps `selected_project`. It changes when the user clicks a project name, when a
session is launched or activated (its project) and when the active session changes after a close.
Launchers render only under the selected project; other projects show their name and sessions.
The explorer shows the workspace that contains the selected project.

### Explorer state (`files.rs`, pure)

- `list_dir(path)`: reads one directory, hides `.git`, sorts folders first then by name
  (case-insensitive), keeps at most 500 entries and reports how many were left out.
- `FileTree`: the set of expanded folders plus a cached listing for each. Expanding a folder lists
  it; collapsing removes it and its expanded descendants; `refresh` re-lists every expanded folder.
  `rows(root)` flattens the visible tree under a root into rows with a depth, including error rows
  (unreadable folder) and "N more" rows.
- Rendering only reads the cache, so painting never touches the disk. Listing happens on the UI
  thread on expand/refresh; it is bounded by the 500-entry cap and is a single `read_dir`. Moving
  it to a background task is left for when it shows up as a problem.
- A plain scrolling `div` is used instead of a virtualised list: the visible tree is small in
  practice and capped per folder.

### Inserting paths

Clicking a file writes its path plus a trailing space to the active session (not executed). The
path is relative to the active session's project directory when the file is inside it, absolute
otherwise, and quoted when it contains characters a shell would split or expand: single quotes on
both PowerShell (doubling `'`) and POSIX shells (`'\''`). This is the same kind of input the user
could type, so no new trust boundary appears; file names with control characters are not inserted.

### File colours

`theme::file_color(name)` maps extensions to the neon palette by category: Rust orange, JS/TS
yellow, other code blue, docs cyan, config green, assets pink, everything else muted.

## Risks / Trade-offs

- No file watching: new files appear after a refresh or re-expanding the folder.
- Paths are relative to the project directory, which may differ from the shell's current directory
  if the user `cd`s; live cwd tracking arrives with MVP-2.
