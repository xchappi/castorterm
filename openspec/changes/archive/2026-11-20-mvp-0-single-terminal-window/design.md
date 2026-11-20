## Context

First real functionality of castorterm (see ADR-001: Rust + GPUI). We need one window hosting one
interactive shell session. Windows is the primary platform, so ConPTY behaviour drives several
choices. Rust is not yet installed on the main developer machine: the CI (Windows/macOS/Linux) is
the compile gate for this change.

## Goals / Non-Goals

**Goals:**
- One GPUI window, one PTY-backed shell, colors, cursor, keyboard input, resize, exit.
- Keep pure logic (key encoding, color palette, shell selection) in GPUI-free modules with unit tests.

**Non-Goals:**
- Tabs, sidebar, config file, scrollback UI, selection/clipboard, IME composition, ligatures.

## Decisions

### PTY + parsing: `alacritty_terminal` (tty + EventLoop) instead of `portable-pty`
`alacritty_terminal` 0.26 ships its own PTY layer (ConPTY on Windows, openpty on Unix) and an
`EventLoop` thread that reads the PTY and feeds the VT parser into `Term`. One crate covers PTY,
parsing and grid state, and it is the path Zed uses in production. `portable-pty` creates ConPTY
with `PSEUDOCONSOLE_INHERIT_CURSOR`, which makes ConPTY wait for a cursor-position reply at
startup; alacritty creates it without that flag.

### Rendering: `canvas()` element, one shaped line per row
In prepaint we lock the `Term`, walk `renderable_content().display_iter`, and per row build a
`String` plus `TextRun`s merged by (foreground, bold). Each row is shaped with
`shape_line(.., force_width = Some(cell_width))` so glyph *i* lands on column *i*. Wide-char
spacer cells are emitted as spaces to keep that invariant. Non-default backgrounds and the cursor
are painted as quads. Ligatures/fallback glyphs may shift a row; acceptable for MVP-0.

### Input: key-down for control keys, input handler for text
`on_key_down` encodes Enter, Backspace, Tab, Escape, arrows, Home/End, PageUp/PageDown,
Insert/Delete, F1-F12 and Ctrl+letter (pure function in `keys.rs`, honouring `APP_CURSOR` mode)
and stops propagation. Everything else reaches an `EntityInputHandler`, whose
`replace_text_in_range` writes the text to the PTY. This covers dead keys, AltGr and basic IME
commits on Windows. Ctrl+Alt is left to the input handler because it is AltGr on Windows.

### Threading
The `EventLoop` runs on its own thread. A `Listener` (EventListener) forwards `Wakeup`,
`PtyWrite` and `Exit/ChildExit` over an unbounded futures channel to a task spawned with
`cx.spawn`, which coalesces bursts, writes `PtyWrite` replies back to the PTY, calls
`cx.notify()` and quits the app on exit. A watcher thread joins the PTY reader thread and also
sends `Exit`, because the event loop can stop on I/O errors without reporting a child exit.
`ColorRequest` (OSC 4/10/11 queries) is answered with the theme color.

### Resize
Prepaint computes `cols = floor(width / cell_width)`, `rows = floor(height / line_height)`. If the
grid changed, it resizes the `Term` and sends `Msg::Resize` to the event loop.

### Shell selection
Windows: `pwsh.exe -NoLogo`, falling back to `powershell.exe -NoLogo` when spawning fails.
Unix: `$SHELL`, falling back to `/bin/bash`. Working directory: the launch directory.

Programs are resolved to an absolute path before spawning, searching only the absolute entries of
`PATH`. ConPTY calls `CreateProcessW` without an application name, which would otherwise also
search the working directory: launching castorterm inside an untrusted repository containing a
`pwsh.exe` must not execute it. On Windows the resolved path is quoted because alacritty does not
quote the program in the command line (`C:\Program Files\...`).

## Risks / Trade-offs

- [GPUI API churn] -> pin `gpui = "0.2.2"`; isolate GPUI code in `view.rs`.
- [Blind implementation, no local compiler] -> CI on three OSes is the gate; pure modules are unit-tested.
- [Linux CI needs system libraries] -> install xkbcommon/wayland/x11/fontconfig/freetype dev packages in CI.
- [Release builds on Windows need `fxc.exe`] -> out of scope; CI builds debug only. Revisit with release tooling.
- [Linux build: `xattr` 0.2.3 (via gpui's http client) does not compile with libc >= 0.2.190] ->
  `Cargo.lock` is committed with libc 0.2.189 / mio 1.2.4 until gpui updates the dependency.
- [Font availability] -> Consolas (Windows), Menlo (macOS), DejaVu Sans Mono (Linux); GPUI falls back otherwise.
