## Decisions

- **Read what the action writes.** The tab reads `docs/project/*.md` (the built-in Index action's
  output) in a fixed order, then any other Markdown there; at most 20 files of 64 KiB, plain files
  only (no links), on a background thread, again when the tab opens, the project changes or Refresh.
- **Small Markdown reader.** Headings, bullets, code lines and paragraphs; inline markup reduced to
  its text; hand-wrapped lines joined; tables kept as text; invisible characters removed. No HTML,
  images or links: it is a summary view, the files stay the source.
- **README as fallback.** A project that was never indexed still shows something useful and the
  buttons to index it.
