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

The sidebar lists your workspaces and projects. Each project has one
launcher per agent (`+ Shell`, `+ Claude Code`...): it opens a new session
in the project directory. Sessions show up under their project and in the
tab bar.

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+T` | New shell in the current project |
| `Ctrl+Shift+W` | Close the current session |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous session |

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

Bundled fonts, both under the [SIL Open Font License 1.1](assets/fonts/):
[Chakra Petch](https://github.com/m4rc1e/Chakra-Petch) (UI) and
[Press Start 2P](https://github.com/codeman38/PressStart2P) (logo).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
