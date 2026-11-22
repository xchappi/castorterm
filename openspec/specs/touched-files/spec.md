# touched-files Specification

## Purpose
Shows, per session, the files the agent read, edited or wrote, so the user can follow and review its
work without searching its output.

## Requirements

### Requirement: Record files used by the agent
`castorterm --status touched` SHALL read a Claude Code hook's JSON from stdin and, for the Read, Edit,
MultiEdit, NotebookEdit and Write tools with an absolute path, append the action and path to the
session's files list. It SHALL do nothing outside castorterm and SHALL refuse a status variable that
does not name a castorterm status file.

#### Scenario: Claude edits a file
- **WHEN** Claude Code's PostToolUse hook runs `castorterm --status touched` after editing `C:/p/src/a.rs`
- **THEN** the session's list contains `src/a.rs` marked as edited

### Requirement: Show the files of a session
castorterm SHALL show a button with the number of files the agent used in each session that has
any; it SHALL open a list, most recent first, each file once with its most significant action, where
hovering previews a file and clicking opens it under the link rules.

#### Scenario: Review what the agent changed
- **WHEN** the agent read `README.md` and edited `src/a.rs` and the user opens the list
- **THEN** it shows `src/a.rs` (E) above `README.md` (R)
