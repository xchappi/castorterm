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
- Hover a workspace or project for **rename** (`✎`) and **remove** (`×`).
  Removing never touches files on disk, and anything with open sessions
  cannot be removed.

Every change is saved to your config file, keeping your comments.

The middle column also has a **Quests** tab: the task board of the
workspace, with the tasks of all its projects together, loaded from an
external **provider** (GitHub Issues, your own tracker...). Select a task to open its link or start an agent on it in the
right project, with the task as its prompt. Providers are small programs
that print JSON; see [docs/providers.md](docs/providers.md) and the GitHub
Issues example in [`examples/`](examples/).

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+T` | New shell in the current project |
| `Ctrl+Shift+W` | Close the current session |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous session |
| `Ctrl+Shift+E` | Show / hide the file explorer |
| `Ctrl+Shift+O` | Add folders to the selected workspace |

castorterm exits when the last session closes.

Each launcher has its own colour (the shell is green, agents follow), used
for its sessions in the sidebar, the tab bar and the frame of the active
terminal.

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
