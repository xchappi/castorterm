## 1. Project setup

- [ ] 1.1 Add dependencies (`gpui` 0.2.2, `alacritty_terminal` 0.26, `futures`, `anyhow`) and bump `rust-version` to 1.88
- [ ] 1.2 Install GPUI system dependencies in the Linux CI job

## 2. Pure modules (unit-tested)

- [ ] 2.1 `shell.rs`: platform shell candidates (pwsh -> powershell on Windows, `$SHELL` -> `/bin/bash` on Unix) with tests
- [ ] 2.2 `keys.rs`: control-key encoding (Enter, Backspace, Tab/Shift+Tab, Escape, arrows with APP_CURSOR, Home/End, PageUp/PageDown, Insert/Delete, F1-F12, Ctrl+letter) with tests
- [ ] 2.3 `palette.rs`: theme colors, 256-color cube/greys and resolution of `Color` (named/indexed/truecolor, OSC overrides) with tests

## 3. Terminal session

- [ ] 3.1 `session.rs`: `GridSize` (Dimensions), `Listener` (EventListener -> channel), spawn PTY with shell fallback, `Term` + `EventLoop`, write and resize helpers

## 4. GPUI view

- [ ] 4.1 `view.rs`: `TerminalView` with focus, key-down handling and event task (notify, PtyWrite replies, quit on exit)
- [ ] 4.2 Canvas rendering: metrics, per-row shaped lines with color runs, background quads, cursor
- [ ] 4.3 Resize from canvas bounds (Term + PTY)
- [ ] 4.4 `EntityInputHandler` forwarding typed text to the PTY

## 5. Entry point and verification

- [ ] 5.1 `main.rs`: `--version`/`-V` flag, spawn session, open window, focus
- [ ] 5.2 CI green on Windows, macOS and Linux (fmt, clippy -D warnings, tests)
- [ ] 5.3 Manual check on Windows: prompt, colors, typing, Ctrl+C, resize, `exit` closes the window
