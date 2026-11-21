## Context

The MVP-1 chrome used neutral VS Code-like greys. The maintainer wants a video-game look. A first
attempt drew all chrome text with the Press Start 2P pixel font; it looked right but was tiring to
read at sidebar sizes, so the pixel font was restricted to the logo.

## Decisions

### Fonts

- **Chakra Petch** (Regular + SemiBold) for all chrome text: squared, "HUD" letterforms that stay
  readable at 11-13 px. Chosen over Orbitron/Audiowide (display fonts, poor at small sizes) and
  Oxanium (only distributed as a variable font, whose default instance is not the regular weight).
- **Press Start 2P** only for the 16 px logo (8 px grid keeps it crisp).
- Fonts are embedded with `include_bytes!` and registered with `TextSystem::add_fonts` at startup,
  so no installation is needed. If registration fails the chrome falls back to the system font and
  a message goes to stderr.
- The terminal keeps the platform monospace font (configurable); pixel fonts are not used for
  terminal text.

### Colours

- Chrome colours and the class colours live in `theme.rs` (pure, tested); GPUI helpers (glow
  shadows, status dots, chips, launcher buttons) live next to the view.
- Class colour = `launcher_color(launcher index)`, cycling through six neon colours, so it needs no
  configuration and is stable for a given config. The colour is stored with each open session.
- Glows are blurred `BoxShadow`s with alpha; gradients use `linear_gradient`. GPUI cannot colour a
  single border side, so the active tab accent is an absolutely positioned 2 px bar.

### Scanlines

Painted after the text as 1 px black quads every 3 px at 10 % alpha (about 250 quads for a full
HD terminal, negligible). On by default for the game feel; `[ui] scanlines = false` disables
them for anyone who finds them distracting.

## Risks / Trade-offs

- Glow shadows cost a little GPU time per frame; they are limited to a handful of elements.
- Colour by launcher index means reordering agents in the config changes their colours.
