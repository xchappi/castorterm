## Decisions

- **The file names, the config types.** `sessions.json` stores only project directories and
  launcher names. What is typed comes from the config (`command` plus `resume_args`), so a tampered
  file can only choose among launchers the user already defined, in projects already listed. Files
  over 64 KiB or invalid JSON are ignored; at most 20 sessions are reopened.
- **Saved on every change** (open, close, activate, next/previous), atomically. Closing the last
  session deletes the file: the user chose to end with nothing open. Closing the window keeps it,
  verified on Windows (shell exits during shutdown do not clear it).
- **Restore replaces the default shell.** If nothing can be restored, the usual shell in the launch
  directory opens.
- **`resume_args` are trusted config** like `command` and are joined with spaces, unquoted, for the
  same reason the command is typed as is. Task sessions are restored as plain agent sessions.
