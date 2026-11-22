# castorterm

A fast, native terminal built for **agentic coding**.

Run Claude Code, OpenCode, Codex or a plain shell side by side, grouped by
**workspace → project → session**, and see at a glance which agent is
working, which one is waiting for you and where each one is running.

> Status: **pre-alpha**. Tabs, a workspace/project sidebar and agent
> launchers work; agent status and cwd/branch display are next.

## Goals

- **Native and light**: Rust + GPU rendering ([GPUI](https://www.gpui.rs/)),
  no Electron, no webview, no telemetry.
- **Windows first-class**, plus macOS and Linux.
- **Agent-aware sidebar**: sessions grouped by project, with live status
  (working / waiting / done) and last activity.
- **Always know where you are**: working directory, git branch and
  worktree visible per session.
- **Workspaces you can save and restore**: open "everything for project X"
  in one go.

## Try it

Every CI run on Windows publishes a `castorterm-windows-preview` artifact
(optimised build) in the Actions tab.

## Usage

The window has three columns:

- **Quest log** (left): your workspaces and projects, with the open sessions
  under each project. Click a project to select it; its launchers
  (`+ Shell`, `+ Claude Code`...) open a new session in the project
  directory.
- **Files** (middle): the file tree of the selected project's workspace, one
  root per project. Click a folder to expand it; click a file to type its
  path into the active session, ready to reference in an agent prompt.
- **Sessions** (right): tabs and the active terminal.

Build your workspaces from the window, no config editing needed:

- **`+ Workspace`** creates a workspace (type its name, Enter) and opens a
  folder picker for its first projects.
- **`+`** on a workspace adds folders (several at once). You can also drop
  folders from your file manager onto the quest log, or press
  `Ctrl+Shift+O` to add to the selected project's workspace.
- A new project is named after its folder, and its `repo` is read from the
  git `origin` remote so the Quests board picks up its tasks.
- Click a workspace's name to **fold** it (its sessions keep running; the
  header shows how many and whether an agent waits there).
- Hover a workspace or project for **rename** (`✎`) and **remove** (`×`).
  Removing never touches files on disk, and anything with open sessions
  cannot be removed.

Every change is saved to your config file, keeping your comments.

Double-click a tab or a session row (or use **Rename session** in the
palette) to name it; the name is kept when castorterm reopens your sessions.
Drag a tab onto another to reorder.

Tabs and session rows show where each shell is when it leaves the project
folder. castorterm reads it from the shell: OSC 7 (bash, zsh, fish,
oh-my-posh) and, on Windows, a small PowerShell integration it adds after
your profile (it wraps your `prompt` to report the location in the window
title, the only channel ConPTY forwards). If a policy blocks PowerShell's
`-EncodedCommand`, the shell still starts (showing the error) and only the
directory label is missing.

castorterm reopens the sessions you had open when you start it again (same
projects, same agents, the same tab active), in a window of the same size
and place. Agents with `resume_args` in the
config (for example `claude --continue`) pick up their last conversation.
Turn it off with `restore_sessions = false` under `[ui]`.

The middle column also has a **Quests** tab: the task board of the
workspace, with the tasks of all its projects together, loaded from an
external **provider** (GitHub Issues, your own tracker...). Select a task to open its link or start an agent on it in the
right project, with the task as its prompt. Providers are small programs
that print JSON; see [docs/providers.md](docs/providers.md) and the GitHub
Issues example in [`examples/`](examples/).

Tick **In a new worktree** on a task to give the agent its own checkout:
castorterm runs `git worktree add` to create `<project>.worktrees/<task>` on
branch `task/<task>` next to the project (or reuses it), adds it to the
workspace as `<project>@<task>` and starts the agent there. Several agents can
then work on the same repository at once without touching each other's files.
Removing that project from the sidebar does not delete the worktree; use
`git worktree remove` when you are done.

### Agent status

Agents can tell castorterm what they are doing: each session shows
`working`, `waiting` or `done`, and the HUD counts the agents waiting for
you. castorterm gives every session a `CASTORTERM_STATUS_FILE` variable;
an agent hook runs `castorterm --status <working|waiting|done>`, which writes
there (and does nothing outside castorterm). For Claude Code, add to
`~/.claude/settings.json` (use the full path to `castorterm` if it is not in
your `PATH`):

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "castorterm --status working" }] }],
    "PreToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "castorterm --status working" }] }],
    "PostToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "castorterm --status working" }] }],
    "Notification": [{ "matcher": "permission_prompt", "hooks": [{ "type": "command", "command": "castorterm --status waiting" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "castorterm --status done" }] }]
  }
}
```

When an agent starts waiting and the window is in the background, its
taskbar button flashes until you come back.

`waiting` is only set for permission prompts (Claude Code also notifies
when it has been idle a while, which would mark finished sessions as
waiting); `PostToolUse` sets `working` again once an approved tool runs. Any
other agent can do the same from its own hooks or scripts.

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+T` | New shell in the current project |
| `Ctrl+Shift+W` | Close the current session |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous session |
| `Ctrl+Shift+PageUp` / `PageDown` | Move the session left / right |
| `Ctrl+Shift+E` | Show / hide the file explorer |
| `Ctrl+Shift+O` | Add folders to the selected workspace |
| `Ctrl+Shift+P` | Command palette: open any agent in any project, go to a session, add folders |
| `Ctrl+Shift+J` | Jump to the next agent waiting for you |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Bigger / smaller / default text (also `Ctrl` + mouse wheel). Like Windows Terminal, `Ctrl+-` no longer reaches the shell (readline's undo is also `Ctrl+_`) |

castorterm exits when the last session closes.

Each launcher has its own colour (the shell is green, agents follow), used
for its sessions in the sidebar, the tab bar and the frame of the active
terminal.

## Install (Windows)

```powershell
pwsh scripts/install-windows.ps1
```

Builds the release binary, installs it in `%LOCALAPPDATA%\Programs\castorterm`,
adds it to your user `PATH` (agent hooks call `castorterm --status`) and
creates a Start menu shortcut (`-Desktop` adds one on the desktop). Run it
again to update. To pin it, right-click castorterm in Start and choose
**Pin to taskbar** (Windows does not let programs pin themselves).

## Configuration

Optional TOML file at `%APPDATA%\castorterm\config.toml` (Windows) or
`~/.config/castorterm/config.toml` (macOS/Linux, honours `XDG_CONFIG_HOME`).
`CASTORTERM_CONFIG` overrides the path. See
[`config.example.toml`](config.example.toml) for every option.

Without a config file castorterm opens a shell in the launch directory. If
the file has errors, castorterm still starts and shows them in the sidebar.

## Building

```sh
cargo run
```

Requires stable Rust (see `rust-toolchain.toml`). On Windows you also need
the MSVC Build Tools ("Desktop development with C++").

Debug builds of GPUI on Windows load shader sources from the local cargo
registry at runtime, so a debug binary only works on the machine that built
it. To share a binary use `cargo build --profile preview` (or `--release`),
which needs `fxc.exe` from the Windows SDK (set `GPUI_FXC_PATH` if it is not
found automatically).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Specs and change proposals live in
[`openspec/`](openspec/).

## Credits

Bundled fonts, both under the SIL Open Font License 1.1 (see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)):
[Chakra Petch](https://github.com/m4rc1e/Chakra-Petch) (UI) and
[Press Start 2P](https://github.com/codeman38/PressStart2P) (logo).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
