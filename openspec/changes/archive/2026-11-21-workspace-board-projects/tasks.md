## 1. Core

- [x] 1.1 `board::workspace_repos` collects distinct project repos in order
- [x] 1.2 `provider::run_list` passes `--project <repo>` for each repo
- [x] 1.3 Config rejects `repo` values starting with `-` or containing whitespace; `filter` optional

## 2. UI

- [x] 2.1 Load the board with the workspace repos
- [x] 2.2 Show the task's project on rows when the workspace has several projects

## 3. Example provider and docs

- [x] 3.1 Example provider aggregates every `--project` repository
- [x] 3.2 Update `docs/providers.md`, `config.example.toml` and README

## 4. Verification

- [x] 4.1 Unit tests for repos, invocation and config validation
- [x] 4.2 Verify on Windows with a multi-project workspace
