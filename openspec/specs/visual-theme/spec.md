# visual-theme Specification

## Purpose
Defines castorterm's visual identity, a neon video-game HUD, while keeping text readable: which
fonts are used where, how each agent gets its own colour, the terminal palette and the optional
CRT scanlines.

## Requirements

### Requirement: Bundled fonts
The system SHALL embed its chrome fonts and register them at startup: a readable HUD-style font
for all chrome text and a pixel font used only for the logo. Terminal text SHALL keep using the
configured or platform monospace font. If a font cannot be registered the system SHALL keep
running with the system font and SHALL show the error in the sidebar.

#### Scenario: Fonts not installed on the machine
- **WHEN** castorterm starts on a machine without those fonts installed
- **THEN** the sidebar, tabs and HUD are drawn with the bundled fonts

### Requirement: Class colours per launcher
The system SHALL assign each launcher a colour from a fixed neon palette by its position (the
built-in Shell first, then agents in config order, cycling when there are more launchers than
colours). A session SHALL use the colour of the launcher that started it for its status light in
the sidebar and tab bar, the active tab accent and the frame around the terminal when active.

#### Scenario: Agent session colour
- **WHEN** the user starts the first configured agent in a project
- **THEN** its launcher button, its session light and, while active, its tab accent and terminal frame share the same colour, different from the Shell colour

### Requirement: HUD
The system SHALL show a HUD bar above the sidebar and tabs with the `CASTORTERM` logo, the number
of open sessions and the application version, labelled as a level (`LVL 0.0.1`).

#### Scenario: Session counter
- **WHEN** two sessions are open
- **THEN** the HUD shows `2 sessions`

### Requirement: Terminal palette and scanlines
The system SHALL draw terminals with the "Arcade night" palette (neon ANSI colours on a deep indigo
background) and, unless disabled in the config, SHALL draw faint horizontal scanlines over the
terminal area.

#### Scenario: Scanlines disabled
- **WHEN** the config sets `[ui] scanlines = false`
- **THEN** the terminal is drawn without scanlines

### Requirement: Text zoom
castorterm SHALL scale the terminal text and the interface text together with a zoom from 70% to
250% in 10% steps: `Ctrl+=` or `Ctrl++` and `Ctrl` + wheel up increase it, `Ctrl+-` and `Ctrl` +
wheel down decrease it, and `Ctrl+0` resets it to 100%. Sidebar and explorer widths SHALL grow with
it and terminals SHALL recompute their grid. The zoom SHALL be saved as `[ui] zoom` in the config
file and applied at startup; invalid values SHALL fall back to 100%.

#### Scenario: Bigger text
- **WHEN** the user presses `Ctrl+=` three times
- **THEN** terminal and interface text are 30% bigger and the config file has `zoom = 1.3`

#### Scenario: Reset
- **WHEN** the user presses `Ctrl+0`
- **THEN** text returns to its default size
