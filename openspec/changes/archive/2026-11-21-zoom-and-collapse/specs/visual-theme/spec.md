## ADDED Requirements

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
