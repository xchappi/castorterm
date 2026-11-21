## Decisions

- **Files, not sockets.** A file per session in the config directory is the simplest channel that
  works the same on Windows, macOS and Linux, needs no server, and survives the agent being any
  program that can run a command. The variable carries the path, so a hook needs no knowledge of
  castorterm's internals.
- **The CLI writes only status files:** the value must be `working`, `waiting` or `done`, and the
  path must be absolute, end in `.status` and sit in a folder named `status`; anything else is
  refused, so the variable cannot be abused to overwrite other files. The command always exits 0
  so it never breaks an agent's hook.
- **Polling every second** on a background thread, one read at a time, files capped at 64 bytes;
  the view repaints only when a status changed.
- **Names include the castorterm process id**, so two windows never share files; stale files (a
  crashed run) are removed after a day.
