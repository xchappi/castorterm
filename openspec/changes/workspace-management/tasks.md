## 1. Core

- [x] 1.1 `config_edit`: add, rename and remove workspaces; add and remove projects; atomic save
- [x] 1.2 `git_remote`: detect `owner/repo` from the origin remote
- [x] 1.3 `ProjectTree` mutations with stable indices

## 2. UI

- [x] 2.1 `+ Workspace` with inline name field; rename and remove on workspace headers
- [x] 2.2 Add folders with the picker, drag and drop and `Ctrl+Shift+O`
- [x] 2.3 Remove project; guard rails for open sessions; messages for save errors

## 3. Docs and verification

- [x] 3.1 README and `config.example.toml`
- [x] 3.2 Unit tests for config edits, repo detection and tree mutations
- [ ] 3.3 Verify on Windows: create a workspace, add folders, restart and see them
