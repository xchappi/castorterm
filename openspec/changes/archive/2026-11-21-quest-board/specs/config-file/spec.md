## MODIFIED Requirements

### Requirement: Config contents
The config file SHALL support: an optional `[font]` table with `family` and `size`; an optional
`[ui]` table with `scanlines`; an `agents` array of `{ name, command, task_args? }`; a `providers`
array of `{ name, command, args? }`; and a `workspaces` array of `{ name, board?, projects }` where
`board` is `{ provider, filter?, prompt?, refresh_secs? }` and each project is
`{ name, path, repo? }`. Project paths SHALL accept a leading `~` for the home directory. Unknown
keys SHALL be rejected.

#### Scenario: Workspaces and agents shown
- **WHEN** the config defines workspace `CastorLabs` with project `castorterm` and agent `OpenCode`
- **THEN** the sidebar shows `CastorLabs`, under it `castorterm`, with launchers `Shell` and `OpenCode`

#### Scenario: Custom font
- **WHEN** the config sets `font.family = "Cascadia Mono"` and `font.size = 16`
- **THEN** terminals are drawn with that family and size

#### Scenario: Board configured
- **WHEN** the config defines provider `tracker` and workspace `Acme` with `board = { provider = "tracker", filter = "project:acme" }`
- **THEN** the Quests tab of `Acme` lists the tasks of that provider for that filter

#### Scenario: Board with unknown provider
- **WHEN** a workspace board names a provider that is not defined
- **THEN** the workspace has no board and the sidebar shows a warning naming the provider
