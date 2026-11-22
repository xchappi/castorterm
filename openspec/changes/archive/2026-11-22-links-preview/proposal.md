## Why

Agents print paths (`src/view.rs:120`), screenshots they save and URLs all the time; using them
meant copying text into another program. Images pasted into an agent were invisible.

## What Changes

- Paths and URLs in the output are detected on the line under the mouse.
- Resting on a file path shows a floating preview over the terminal: an excerpt around the line,
  or the image. Ctrl+hover underlines the link.
- Ctrl+click opens: URLs in the browser; text files in the configured editor at the line
  (`[ui] editor`, default `code`, VS Code-like `-g path:line:col`); images and documents with the
  system; anything else is revealed in the file manager.
- An image pasted into an agent session (Ctrl+V, or right click with an image in the clipboard,
  which sends Ctrl+V so the agent reads it) is previewed for a few seconds.

## Capabilities

### New Capabilities
- `terminal-links`: links in the output, their preview and opening.

### Modified Capabilities

## Impact

New `src/links.rs` (pure detection and safety rules); `src/view.rs` (hover, preview, opening,
pasted images); `src/config.rs` (`[ui] editor`); `src/workspace_view.rs` (base directory and editor
per session); README and example config.
