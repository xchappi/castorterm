## Why

castorterm always opened at 1360x820 in the middle of the main screen, so the user had to move and
resize it (or maximize it) every time.

## What Changes

- The window's position, size and maximized state are saved in `sessions.json` with the sessions,
  and the window opens with them next time.
- Moves and resizes are tracked in memory and written with the next save and when the window
  closes, not on every move.
- Saved geometry is used only if plausible and on a connected screen; otherwise the default.

## Capabilities

### New Capabilities

### Modified Capabilities
- `session-restore`: the window geometry is restored too.

## Impact

- `src/restore.rs` (`SavedWindow`), `src/main.rs` (open with saved bounds),
  `src/workspace_view.rs` (track bounds, save on close).
