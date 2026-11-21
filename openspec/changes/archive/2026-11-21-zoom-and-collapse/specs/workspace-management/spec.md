## ADDED Requirements

### Requirement: Fold workspaces
Clicking a workspace's name SHALL fold or unfold it. A folded workspace SHALL hide its projects and
their sessions in the sidebar, keep those sessions running, and show how many sessions run inside
and whether an agent there waits. The folded state SHALL be saved as `collapsed = true` on the
workspace in the config file.

#### Scenario: Fold a workspace
- **WHEN** the user clicks "THE GARAGE"
- **THEN** its projects are hidden, its sessions keep running and it stays folded after a restart
