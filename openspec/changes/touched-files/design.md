## Decisions

- **Same channel as the status.** The list lives next to the status file and uses its guard: the
  variable must name `<pid>-<n>.status` in a `status` folder, so the hook cannot be pointed at other
  files. Only absolute, single-line paths without invisible characters are recorded or listed.
- **Bounded.** Appends; the file starts over past 512 KiB; castorterm reads only its last 64 KiB
  (dropping a cut first line); hook input over 1 MiB is ignored; files are removed when the session
  closes and stale ones after a day.
- **In the pane.** The button and list belong to the session's terminal view (bottom left), like the
  preview (bottom right), so split view shows each session's own files. Opening reuses the link
  rules: local paths only, nothing executed.
- **Deployment order.** An older castorterm treats `--touched` as a normal start and would open a
  window per tool use, so the hook must only be added once a castorterm with `--touched` is
  installed.
