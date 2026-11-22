## Decisions

- **Same board, another source.** The local board is a `Board` with the reserved provider `local`
  and no command; loading it reads files instead of running a process, and everything else (statuses,
  starting tasks, prompts) is shared with provider boards.
- **Files the projects already have.** `TASKS.md` checklists and OpenSpec changes are plain text in
  the repository; ids carry the file and line (`app/TASKS.md:12`) or the change (`openspec:add-x`),
  so the agent knows where the task lives.
- **Bounded and safe.** Files at most 256 KiB, at most 100 changes, no links followed (`TASKS.md`,
  `openspec/changes`); titles are cleaned of control characters, as they go into a prompt.
- **Hardened after review.** Ids come from the project and a hash of the title (repeats numbered), not
  the line, so editing the file does not hand a task's worktree to another; OpenSpec ids include the
  project; linked worktrees (a `.git` file) are skipped, so a task worktree does not duplicate its
  project's tasks; code fences are ignored; a sub-heading that names no status keeps its parent's;
  status words match whole words ("Preview" is not "Review"); network drives are skipped; when there
  are too many tasks, done ones go first.
