## Decisions

- **Same board, another source.** The local board is a `Board` with the reserved provider `local`
  and no command; loading it reads files instead of running a process, and everything else (statuses,
  starting tasks, prompts) is shared with provider boards.
- **Files the projects already have.** `TASKS.md` checklists and OpenSpec changes are plain text in
  the repository; ids carry the file and line (`app/TASKS.md:12`) or the change (`openspec:add-x`),
  so the agent knows where the task lives.
- **Bounded and safe.** Files at most 256 KiB, at most 100 changes, no links followed (`TASKS.md`,
  `openspec/changes`); titles are cleaned of control characters, as they go into a prompt.
