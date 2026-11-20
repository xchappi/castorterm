# castorterm

A fast, native terminal built for **agentic coding**.

Run Claude Code, OpenCode, Codex or a plain shell side by side, grouped by
**workspace → project → session**, and see at a glance which agent is
working, which one is waiting for you and where each one is running.

> Status: **pre-alpha**. Nothing usable yet — follow along or jump in.

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

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
