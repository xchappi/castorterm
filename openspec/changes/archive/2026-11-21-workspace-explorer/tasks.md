## 1. Pure modules (unit-tested)

- [x] 1.1 `files.rs`: `list_dir` (hide `.git`, folders first, case-insensitive sort, 500 cap), `FileTree` (expand, collapse with descendants, toggle, refresh, rows with error and "more" rows), with tests on temp folders
- [x] 1.2 `files.rs`: path for the terminal (relative to the project, quoting for PowerShell and POSIX, reject control characters), with tests
- [x] 1.3 `theme::file_color` by extension category, with tests
- [x] 1.4 `shortcuts.rs`: Ctrl+Shift+E `ToggleFiles`, with tests

## 2. Views

- [x] 2.1 `view.rs`: method to type text into the session
- [x] 2.2 `workspace_view.rs`: selected project (click, launch, activate, close), launchers only under it
- [x] 2.3 Explorer column: header with workspace name and refresh, roots per project, rows with chevrons, indentation and colours, click to expand or insert path
- [x] 2.4 Three-column layout, Ctrl+Shift+E toggle, default window 1360x820

## 3. Docs and verification

- [x] 3.1 README: layout, explorer and shortcut
- [x] 3.2 Screenshots on Windows: layout, expand folders, click a file inserts its path
- [x] 3.3 CI green on Windows, macOS and Linux
