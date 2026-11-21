## 1. Config

- [ ] 1.1 `config.rs`: `providers`, workspace `board` (provider, filter, prompt, refresh_secs >= 15), project `repo`, agent `task_args` with `{prompt}`; unknown provider warning; tests

## 2. Pure board logic (`board.rs`)

- [ ] 2.1 Contract types and parsing: version check, required fields, status set, skipped count, control characters stripped, http(s)-only links; tests
- [ ] 2.2 Grouping by status, project matching (repo, then name, case-insensitive), prompt rendering, typed command building with quoting; tests

## 3. Provider execution (`provider.rs`)

- [ ] 3.1 Command resolution (absolute PATH entries, `.exe`/`.cmd`/`.bat` on Windows), run without shell, stdin closed, 20 s timeout, 4 MiB cap, stderr first line on failure; tests with a fake provider

## 4. UI

- [ ] 4.1 Files | Quests tabs in the middle column
- [ ] 4.2 Board state per workspace with tickets, background loading, refresh button and timer while visible
- [ ] 4.3 Grouped task list with priority chips, Done collapsed, skipped/error/hint rows
- [ ] 4.4 Task selection with project, link and agent buttons; start agent session with the rendered prompt

## 5. Docs, example and verification

- [ ] 5.1 `docs/providers.md` (contract v1) and README/config example sections
- [ ] 5.2 `examples/github_issues_provider.rs` using `gh issue list --json`
- [ ] 5.3 Screenshots on Windows with the example provider: board, refresh, start an agent on a task
- [ ] 5.4 CI green on Windows, macOS and Linux
