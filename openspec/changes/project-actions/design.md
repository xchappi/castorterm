## Decisions

- **The agent does the work.** castorterm only opens a session with a prompt, through the same
  path as starting a board task (`task_args`, `board::agent_command`, which neutralizes shell
  metacharacters), so actions inherit its quoting rules and agent choice.
- **Local by default, replaceable.** The built-in prompts write Markdown inside the repository, in
  the spirit of OpenSpec, for open source use. A config's `[[actions]]` replaces the whole list,
  which is how a company plugin (castorlabs-stack's adapter) points them at its wiki and tracker.
- **Explicit agent.** An action naming an agent that cannot take a prompt does not silently fall
  back to another agent: castorterm says what is missing.
- **Trusted input only.** Prompts come from the config; placeholders are filled with the project's
  configured name, path and repo, never with terminal output.
- **Project checkout, not a task worktree.** With "worktree per task" on, actions still run in the
  project's own folder: what they write (`docs/project/`) describes the project, not a task branch.
- **Hardened after review.** Names and prompts must be one line without control characters and
  prompts at most 4000 characters (agent shims go through cmd.exe, limited to about 8 KiB);
  placeholders are filled in one pass and the result is cleaned before it is typed; actions sit on
  their own row under the launchers.
