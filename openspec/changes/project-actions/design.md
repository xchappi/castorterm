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
