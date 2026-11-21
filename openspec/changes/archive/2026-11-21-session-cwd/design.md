## Decisions

- **Tap the PTY, change nothing.** `TapPty` wraps `tty::Pty` and implements the traits the event
  loop needs (`EventedReadWrite`, `EventedPty`, `OnResize`) by delegation; its reader passes bytes
  through unchanged and feeds an incremental OSC scanner (state machine across reads, payloads over
  4 KiB dropped). The reader holds a raw pointer into the boxed PTY it wraps: both live in the same
  struct, the box is never moved, and the PTY field drops last.
- **Windows needs the title.** Measured: through ConPTY only OSC 0 (title) arrives; OSC 7 and
  OSC 9;9 are dropped. So PowerShell gets an integration that, after the user's prompt runs, sets
  the title to `castorterm:cwd:` + provider path. Only titles with that prefix count; agents' own
  titles are ignored. The script is passed base64 UTF-16LE (`-EncodedCommand`): no quoting issues.
  The prompt's own title is replaced, which is invisible because castorterm titles its window.
- **Coalesced.** A burst of reports produces one event with the last directory.
- **Display.** `cwd_label`: none at the project root; relative path inside it (`src/view`); full
  path when short, else the last two components (`…/a/b`). Case-insensitive on Windows.
