## Decisions

- **Detect on demand.** Links are found on the one line under the mouse when it moves to another
  cell, not for the whole screen every frame. Candidates are tokens split on whitespace, quotes and
  brackets, without trailing punctuation: `http(s)://` URLs, and paths with a separator or a file
  extension (at least one letter), with an optional `:line:col`.
- **Untrusted output, safe actions.** Paths resolve against the session's directory (the project,
  then what the shell reports). Network paths (`\\server`, `//server`) are refused because touching
  them makes Windows authenticate to that server. Previews only read: at most 64 KiB of text (binary
  files are skipped) or an image up to 20 MiB, on a background thread. Opening never runs anything:
  the system only gets images and documents from an allowlist (no `.exe`, `.bat`, `.ps1`, `.js`,
  `.lnk`, `.hta`...), code goes to the editor, the rest is revealed in the file manager. An editor
  that is a `.cmd` shim never receives an argument with cmd metacharacters.
- **Pasted images.** Agents read the clipboard themselves on Ctrl+V; castorterm only shows what is
  there. A right click with an image sends Ctrl+V instead of text.
- **Floating, dismissable.** The preview sits bottom-right of the pane, follows the hovered link,
  closes on click; a pasted image stays 8 s.
- **Hardened after review.** Nothing that could reach the network is touched: the drive must be a
  local disk (`GetDriveTypeW`) and no component may be a symlink, junction or other reparse point,
  checked from the root down off the UI thread; device names and alternate data streams are
  refused. HTML and SVG left the allowlist (scripts). Images are size-checked from their header
  (at most 16384 px per side and 40 Mpx) before decoding. Paths resolve against the project first,
  and a reported directory is only used if it is a plain absolute path. The pasted-image timer is
  separate from hover previews.
