## Why

Agents print long logs; finding an error or a path in them meant scrolling a terminal that could
not scroll back at all.

## What Changes

- The history scrolls: mouse wheel and Shift+PageUp / PageDown (not in full-screen programs such as
  vim or less, which scroll themselves). Typing returns to the live screen.
- Ctrl+Shift+F (or **Find in history** in the palette) opens a search bar over the active
  terminal. Typing searches as you type from the bottom; Enter goes to the next older match,
  Shift+Enter to the next newer one, wrapping around; the match is highlighted and scrolled into
  view. Esc closes it and returns to the bottom.
- Queries are literal text, case-insensitive unless they contain an uppercase letter.

## Capabilities

### New Capabilities
- `scrollback-search`: scrolling and searching a session's history.

### Modified Capabilities

## Impact

`src/view.rs` (scrolling, `find`, `next_match`, highlight), new `src/workspace_view/search_bar.rs`,
`src/shortcuts.rs`, `src/commands.rs`, palette, `src/workspace_view.rs`; README.
