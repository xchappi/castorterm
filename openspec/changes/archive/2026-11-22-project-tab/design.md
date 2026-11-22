## Decisions

- **Read what the action writes.** The tab reads `docs/project/*.md` (the built-in Index action's
  output) in a fixed order, then any other Markdown there; at most 20 files of 64 KiB, plain files
  only (no links), on a background thread, again when the tab opens, the project changes or Refresh.
- **Small Markdown reader.** Headings, bullets, code lines and paragraphs; inline markup reduced to
  its text; hand-wrapped lines joined; tables kept as text; invisible characters removed. No HTML,
  images or links: it is a summary view, the files stay the source.
- **README as fallback.** A project that was never indexed still shows something useful and the
  buttons to index it.
- **Hardened after review.** Markdown is parsed once, off the UI thread, and kept as blocks (at most
  600 per file, with a note when a file is cut); overlapping reads keep only the newest; a
  `docs/project` that is a link is not followed; ordered lists, task items with links, a BOM, `~~~`
  fences and table rows are handled; while the tab is open the index is re-read with the board timer,
  so an agent's indexing shows up without Refresh.
