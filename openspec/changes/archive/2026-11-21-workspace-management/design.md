## Context

The sidebar tree (`ProjectTree`) is built once from the config. Sessions, boards and selection refer
to workspaces and projects by index.

## Decisions

- **Mutate in memory, persist separately.** UI actions change `ProjectTree` and then apply the same
  edit to the config document with `toml_edit`. Re-parsing the config would rebuild the tree and
  shift indices under open sessions.
- **Stable project indices.** New projects are appended to `ProjectTree::projects`; removing a
  project only drops it from its workspace list (the node stays as a tombstone). Removing a
  workspace re-keys the boards map and clears the selected task.
- **Guard rails.** No removal while sessions are open in the project or workspace; removal never
  touches the disk; workspace names must be single-line, non-empty and unique.
- **Persistence.** `config_edit` loads the file as a `DocumentMut` (empty if missing), finds the
  `[[workspaces]]` entry by name and projects by path, edits and writes atomically (temp file and
  rename). If the file cannot be parsed, the edit is refused with a visible message and the
  in-memory change is kept for the session.
- **Implicit workspace.** Adding to "Local" writes it with all its projects (including the launch
  directory) and it becomes a regular workspace.
- **Repo detection.** Read `<dir>/.git/config` (following a `gitdir:` file for worktrees), take
  `remote "origin"` `url`, and keep the last two path segments without `.git` for
  `https://host/owner/repo(.git)`, `ssh://git@host/owner/repo.git` and `git@host:owner/repo.git`.
  The result must pass the repo allow list; otherwise no repo is set. The host is ignored on
  purpose: trackers identify repositories as `owner/repo`, and the user can edit `repo` if two hosts
  collide. Only local paths are followed (`gitdir:`, `commondir`, no symlinked `.git`), files are
  read up to 1 MiB, and git is never run: the folder may be an untrusted clone, and opening a UNC
  path would send the user's Windows credentials to that host.
- **Off the UI thread.** Checking dropped or picked folders and reading their remotes runs in the
  background, like the file explorer, so a slow network drive never freezes the window.
- **Names identify workspaces.** The config loader skips duplicate workspace names and the
  implicit workspace takes a free name (`Local 2`...), so saving by name is unambiguous. Removals
  in the file affect only the first match.
- **Links.** A folder the user picks or drops may itself be a symlink or junction (their choice), but
  nothing inside it is read through a link: below the project folder every component of a git
  file path must be a plain directory (no symlink, no reparse point, no `..`).
- **Text input.** A minimal inline field handled in the existing key capture (characters,
  Backspace, Enter, Escape, Ctrl+V); while it is open the terminal loses focus, so neither key
  events nor text input reach it. GPUI has no
  built-in text input, and IME support is not needed for names.
- **Picker and drop.** `cx.prompt_for_paths` (directories, multiple) and `on_drop::<ExternalPaths>`
  on the sidebar; only existing directories are added.

## Risks / Trade-offs

- Editing preserves comments, but `toml_edit` may normalise spacing in the edited tables.
- Names with characters special to TOML are stored escaped; only control characters are rejected.
