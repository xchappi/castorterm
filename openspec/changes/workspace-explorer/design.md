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

### Explorer state (`files.rs`)

- `list_dir(path)`: reads one directory, hides `.git`, sorts folders first then by name
  (case-insensitive), keeps at most 500 entries and reports how many were left out.
- `FileTree`: the set of expanded folders plus the latest listing for each. Collapsing removes a
  folder and its expanded descendants; refresh re-lists every expanded folder. `rows(root)`
  flattens the visible tree under a root into rows with a depth, including loading, error
  (unreadable folder) and "N more" rows.
- `FileTree` never reads the disk itself: expanding marks the folder and the view lists it with
  `files::load` on a background thread (`cx.background_spawn`), then stores the result. Until it
  arrives the tree shows a "Loading…" row, so a stalled network drive cannot freeze the window or
  the terminal. Each load gets a ticket; results for folders collapsed in the meantime, or older
  than the latest load of that folder (double refresh), are dropped. Rendering only reads the
  cache.
- Entries that fail to read are skipped; only an unreadable folder shows an error row.
- Symlinked folders can be expanded. Expansion is user-driven, so a symlink loop only grows as far
  as the user clicks.
- Row element ids are keyed by path, so hover state does not jump when rows shift.
- A plain scrolling `div` is used instead of a virtualised list: the visible tree is small in
  practice and capped per folder.

### Inserting paths

Clicking a file writes its path plus a trailing space to the active session (not executed). The
path is relative to the active session's project directory when the file is inside it, absolute
otherwise (files of other projects in the workspace).

File names come from the repository, possibly a third-party clone, so they are **untrusted input
typed into a shell**. The inserted text must always be a single literal argument:

- Single quotes unless every character is alphanumeric or one of `_-./:+` (plus `\` on Windows,
  where it is the path separator; on POSIX an unquoted backslash escapes the next character).
- PowerShell treats the typographic quotes U+2018, U+2019, U+201A and U+201B as single quotes, so
  all of them are doubled inside the string, not only ASCII `'`. POSIX shells only need `'\''`.
- A relative path starting with `-` is prefixed with `./` so it is not read as an option.
- Paths that are not valid Unicode or contain control characters are not inserted.
- Known limit: PowerShell parsing is safe, but Windows PowerShell 5.1 does not escape embedded
  `"` when it passes an argument to a native program, so a name containing `"` can be split into
  several arguments by that program. This is argument splitting in the next command the user runs,
  not code execution.
- The quoting flavour follows the platform: on Windows sessions always run PowerShell (see
  `shell.rs`); on other platforms a POSIX-like shell is assumed (fish quoting is not special-cased).

Selection and focus are separate: focusing a terminal never changes the selected project, so
clicking a file of another project keeps that project selected.

### File colours

`theme::file_color(name)` maps extensions to the neon palette by category: Rust orange, JS/TS
yellow, other code blue, docs cyan, config green, assets pink, everything else muted.

## Risks / Trade-offs

- No file watching: new files appear after a refresh or re-expanding the folder.
- Paths are relative to the project directory, which may differ from the shell's current directory
  if the user `cd`s; live cwd tracking arrives with MVP-2.
