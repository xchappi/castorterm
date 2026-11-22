## Decisions

- **Names are display only.** A name from the state file is untrusted: it goes through the same
  `Tabs::rename` validation as typed names (trimmed, 1-40 characters, no control or invisible
  characters) and is never typed into a terminal. Only names the user set are saved
  (`renamed`), so default labels keep being numbered as before.
- **Same inline editor.** Renaming reuses the workspace name field (keys captured, terminals
  gated), drawn compact inside the tab; a refused name turns its border red.
- **Drop takes the target's place.** Dragging onto a tab moves the dragged one to that index; the
  active session stays active. Keyboard moves clamp at the ends and repeat when held.
