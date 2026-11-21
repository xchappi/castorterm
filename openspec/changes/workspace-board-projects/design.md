## Context

Boards are per workspace (`WorkspaceNode.board`); tasks are matched to projects by `repo`, then by
name. The provider is run as `<command> <args> list --filter <filter>`.

## Decisions

- **Pass repositories, not names.** Only projects with a `repo` contribute a `--project` argument:
  names are local labels that a tracker does not know. Values are deduplicated ignoring ASCII case
  and keep config order, so the invocation is stable.
- **Separate tokens** (`--project`, `<repo>`), never a shell. A repo starting with `-` is rejected
  at config load so no provider parser can mistake it for a flag; whitespace is rejected because
  repository identifiers never contain it.
- **Filter stays.** It is still passed (possibly empty) so providers can narrow further (labels,
  assignee...). Providers SHOULD return the union of the projects' tasks when `--project` is given.
- **Project label on rows** only when the workspace has two or more projects; it shows the matched
  project's name, or the raw `project` value when it matches none.
- **Contract version unchanged.** v1 was published hours earlier with no third-party providers;
  adding the arguments to v1 is simpler than carrying a v2 for an unused v1.

## Risks / Trade-offs

- A provider written against the earlier v1 text that rejects unknown arguments would fail; the
  only such provider is the bundled example, updated here.
