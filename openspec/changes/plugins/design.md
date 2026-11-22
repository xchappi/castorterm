## Decisions

- **Narrow surface.** Plugins cannot change agents, workspaces or the interface: only providers (a
  command run to list tasks, same trust as the config folder they live in) and actions (prompts).
- **User wins.** The user's provider of the same name is kept, so a personal setup is never
  overridden by a team file.
- **Replace, not merge, the defaults.** If anyone defines actions, castorterm's local defaults step
  aside: a company plugin offers "Index into the wiki" instead of also "Index locally".
