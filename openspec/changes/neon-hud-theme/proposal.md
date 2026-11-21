## Why

castorterm should feel like its own app, not a generic terminal: the maintainer asked for a
video-game look. A first pixel-font iteration was hard to read, so the look keeps the game
flavour (neon colours, a pixel-font logo, a HUD, "character class" colours per agent) while the
chrome uses a readable font.

## What Changes

- New "Neon HUD" look for the window chrome: a HUD bar with the pixel-font `CASTORTERM` logo,
  session counter and version, and a neon gradient line under it.
- Each launcher has a class colour (shell green, then one colour per agent). Launcher buttons,
  session lights, the active tab accent and the frame around the active terminal use the colour of
  the launcher that started the session.
- Chrome text uses the bundled Chakra Petch font; the logo uses the bundled Press Start 2P pixel
  font. Both are SIL OFL and their licences ship in `assets/fonts/`.
- New terminal palette "Arcade night": neon ANSI colours on a deep indigo background, yellow
  cursor.
- Optional CRT-style scanlines over the terminal, on by default, disabled with
  `[ui] scanlines = false`.

## Out of scope

- Theme selection or user-defined colours, light theme.
- Custom (borderless) window title bar.
- Animations (blinking, transitions).
- Per-agent colour configuration.

## Capabilities

### New Capabilities

- `visual-theme`: chrome look, class colours per launcher, bundled fonts, terminal palette and
  scanlines option.

### Modified Capabilities

None. The new `[ui] scanlines` key is specified in `visual-theme`.

## Impact

- Binary grows by ~280 KB (three embedded fonts).
- `palette.rs` default colours change; `workspace_view.rs` rendering is rewritten; new
  `theme.rs`.
