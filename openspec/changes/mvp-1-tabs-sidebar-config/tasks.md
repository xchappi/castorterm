## 1. Pure modules (unit-tested)

- [x] 1.1 Add `serde` (derive) and `toml` dependencies
- [x] 1.2 `config.rs`: types, `deny_unknown_fields`, path resolution (`CASTORTERM_CONFIG`, APPDATA, XDG, HOME), `~` expansion, relative-path validation, load with defaults and error message, with tests
- [x] 1.3 `tabs.rs`: `Tabs<T>` (open, close with neighbour activation, activate, next/prev wrapping, per-project labels with numeric suffix), with tests
- [x] 1.4 `shortcuts.rs`: keystroke -> `Shortcut` mapping, with tests

## 2. Terminal view

- [x] 2.1 `view.rs`: configurable font (`TerminalStyle`), emit `TerminalEvent::Exited` instead of quitting
- [x] 2.2 `session.rs`: `shutdown()` sending `Msg::Shutdown`

## 3. Workspace view

- [x] 3.1 Project tree from config + initial session attached to the matching project or the implicit "Local" workspace, with tests for the matching
- [x] 3.2 Sidebar: workspaces, projects, launchers, sessions, active highlight, error/warning messages
- [x] 3.3 Tab bar with activate and close
- [x] 3.4 Spawning sessions (shell or agent command), missing directory error, focus on activation
- [x] 3.5 Shortcuts in the capture phase; quit when the last session closes
- [x] 3.6 `main.rs`: load config, open the workspace window

## 4. Docs and verification

- [x] 4.1 README configuration section and `config.example.toml`
- [ ] 4.2 CI green on Windows, macOS and Linux
- [ ] 4.3 Manual check on Windows: sidebar, launchers (Shell, Claude Code, OpenCode), tabs, shortcuts (held Ctrl+Shift+T/W do not repeat; no stray character in the new shell), closing a tab leaves no attached processes, exit of last session
