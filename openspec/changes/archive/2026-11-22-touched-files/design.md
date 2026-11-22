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
- **Safe in any deployment order.** The hook runs `castorterm --status touched`: an older castorterm
  rejects the unknown status and exits without opening a window, so the hook can be installed before
  castorterm is updated.
- **Hardened after review.** The list is never written through a link (symlink, junction); it starts
  over by writing a temporary file and renaming it, so concurrent appends never land mid-line; it is
  only re-read when its size or time changed; list rows are keyed by path, and closing the list or a
  file leaving it drops its preview; the list and the preview share narrow panes.
