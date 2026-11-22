## Purpose

Turns paths and URLs printed in a session into links that can be previewed and opened, so the files,
screenshots and pages an agent mentions are one gesture away, without ever running untrusted output.

## ADDED Requirements

### Requirement: Preview linked files
Resting the mouse on a file path in the output SHALL show a floating preview over the terminal: an
excerpt around the given line for text files, or the image for image files. Network paths, mapped
network drives, paths through links, folders, missing, binary and oversized files SHALL show no
preview and SHALL NOT be accessed beyond what is needed to reject them.

#### Scenario: Code location
- **WHEN** the output contains `src/view.rs:120` and the mouse rests on it
- **THEN** a preview shows lines of `src/view.rs` around line 120 with that line highlighted

#### Scenario: Screenshot
- **WHEN** the mouse rests on `out/screenshot.png`
- **THEN** the preview shows the image

### Requirement: Open links safely
Ctrl+click on a link SHALL open URLs in the browser, text files in the configured editor at the
given line, and images and documents with the system's default program. Other files SHALL only be
revealed in the file manager; nothing SHALL be executed from a link.

#### Scenario: Executable path
- **WHEN** the user Ctrl+clicks `tools\setup.exe` in the output
- **THEN** the file is shown in the file manager and not run

### Requirement: Show pasted images
When an image is pasted into an agent session (Ctrl+V or right click), castorterm SHALL show it in
the floating preview for a few seconds.

#### Scenario: Paste a screenshot into Claude Code
- **WHEN** the clipboard holds a screenshot and the user presses Ctrl+V in a Claude Code session
- **THEN** Claude Code receives Ctrl+V and the screenshot is previewed
