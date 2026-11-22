//! The project's index as shown in the PROJECT tab: the Markdown files that
//! the "Index project" action writes to `docs/project/`, or the README when
//! the project is not indexed yet; and a small Markdown reader for them.

use std::io::Read as _;
use std::path::Path;
use std::time::SystemTime;

/// Folder the index lives in, inside the project.
pub const INDEX_DIR: &str = "docs/project";
/// Index files listed first, in this order; others follow by name.
const ORDER: [&str; 6] = [
    "overview",
    "stack",
    "architecture",
    "development",
    "state",
    "improvements",
];
/// Files and bytes read at most, and blocks shown per file.
const MAX_FILES: usize = 20;
const MAX_ENTRIES: usize = 200;
const MAX_BYTES: u64 = 64 * 1024;
const MAX_BLOCKS: usize = 600;

/// One Markdown document of the index, already split into display blocks
/// (parsed once, off the UI thread).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    /// File name without extension (`overview`).
    pub name: String,
    pub blocks: Vec<Block>,
}

/// Where the shown documents come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `docs/project/` exists with Markdown files.
    Index,
    /// No index: the project's README.
    Readme,
    /// Neither.
    Nothing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectIndex {
    pub source: Source,
    pub docs: Vec<Doc>,
    /// Newest change among the index files.
    pub updated: Option<SystemTime>,
}

/// Reads the index of the project at `root` (call it off the UI thread).
/// Only plain files and a plain `docs/project` folder are read (no links),
/// each bounded.
pub fn load(root: &Path) -> ProjectIndex {
    let dir = root.join(INDEX_DIR);
    let plain_dir =
        std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir() && !crate::links::is_link(&m));
    let mut files: Vec<(String, std::path::PathBuf)> = if plain_dir {
        std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .take(MAX_ENTRIES)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")))
                    .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    files.sort_by_key(|(name, _)| {
        let rank = ORDER
            .iter()
            .position(|o| o.eq_ignore_ascii_case(name))
            .unwrap_or(ORDER.len());
        (rank, name.to_ascii_lowercase())
    });
    let mut docs = Vec::new();
    let mut updated: Option<SystemTime> = None;
    for (name, path) in files.into_iter().take(MAX_FILES) {
        if let Some((text, cut)) = read_plain(&path) {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                updated = updated.max(Some(modified));
            }
            docs.push(Doc {
                name,
                blocks: blocks_of(&text, cut),
            });
        }
    }
    if !docs.is_empty() {
        return ProjectIndex {
            source: Source::Index,
            docs,
            updated,
        };
    }
    let readme = ["README.md", "readme.md", "Readme.md"]
        .iter()
        .find_map(|n| read_plain(&root.join(n)));
    match readme {
        Some((text, cut)) => ProjectIndex {
            source: Source::Readme,
            docs: vec![Doc {
                name: "README".into(),
                blocks: blocks_of(&text, cut),
            }],
            updated: None,
        },
        None => ProjectIndex {
            source: Source::Nothing,
            docs: Vec::new(),
            updated: None,
        },
    }
}

/// Display blocks of a file, bounded, with a note when it was cut short.
fn blocks_of(text: &str, cut: bool) -> Vec<Block> {
    let mut blocks = parse(text);
    let long = blocks.len() > MAX_BLOCKS;
    blocks.truncate(MAX_BLOCKS);
    if cut || long {
        blocks.push(Block::Gap);
        blocks.push(Block::Text(
            "[shortened: open the file for the rest]".into(),
        ));
    }
    blocks
}

/// Text of a plain file (not a link), at most [`MAX_BYTES`], and whether
/// it was longer.
fn read_plain(path: &Path) -> Option<(String, bool)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || crate::links::is_link(&meta) {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let cut = bytes.len() as u64 > MAX_BYTES;
    bytes.truncate(MAX_BYTES as usize);
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if cut {
        // Drop a character split by the limit.
        while text.ends_with('\u{FFFD}') {
            text.pop();
        }
    }
    Some((text, cut))
}

/// A line of Markdown, simplified for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(u8, String),
    /// A list item: its marker (`•` or `3.`) and text.
    Bullet(String, String),
    Code(String),
    Text(String),
    Gap,
}

/// An ordered list item: `12. text` or `12) text` → (`12.`, `text`).
fn ordered_item(line: &str) -> Option<(String, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 4 {
        return None;
    }
    let rest = &line[digits..];
    let text = rest
        .strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))?;
    Some((format!("{}.", &line[..digits]), text))
}

/// Splits Markdown into display blocks: headings, list items, code lines
/// and text, with inline markup (`**`, backticks, links) reduced to its
/// text and invisible characters removed.
pub fn parse(text: &str) -> Vec<Block> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut blocks = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        let trimmed = line.trim_end();
        let fence = trimmed.trim_start();
        if fence.starts_with("```") || fence.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            blocks.push(Block::Code(visible(trimmed)));
            continue;
        }
        let t = trimmed.trim_start();
        if t.is_empty() {
            if blocks.last().is_some_and(|b| *b != Block::Gap) {
                blocks.push(Block::Gap);
            }
            continue;
        }
        let hashes = t.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            blocks.push(Block::Heading(hashes as u8, inline(t[hashes..].trim())));
        } else if let Some(item) = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
        {
            blocks.push(Block::Bullet("\u{2022}".into(), inline(item)));
        } else if let Some((marker, item)) = ordered_item(t) {
            blocks.push(Block::Bullet(marker, inline(item)));
        } else if t.starts_with('|') {
            // Table rows as text, each on its own; separator rows dropped.
            if !t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                blocks.push(Block::Text(inline(t)));
                blocks.push(Block::Gap);
            }
        } else if t.chars().all(|c| matches!(c, '-' | '=' | '*' | '_' | ' ')) && t.len() >= 3 {
            // A rule.
        } else {
            // A quote is shown as its text.
            let t = t.strip_prefix('>').map_or(t, str::trim_start);
            if t.is_empty() {
                continue;
            }
            let text = inline(t);
            // Lines wrapped by hand belong to the paragraph or item above.
            match blocks.last_mut() {
                Some(Block::Text(previous)) => {
                    previous.push(' ');
                    previous.push_str(&text);
                }
                Some(Block::Bullet(_, previous))
                    if line.starts_with(' ') || line.starts_with('\t') =>
                {
                    previous.push(' ');
                    previous.push_str(&text);
                }
                _ => blocks.push(Block::Text(text)),
            }
        }
    }
    while blocks.last() == Some(&Block::Gap) {
        blocks.pop();
    }
    blocks
}

/// Inline markup to plain text: `**b**`, `` `c` ``, `[t](url)`.
fn inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        // `[text](target)`: the `]` closing this `[`, then `(`...`)`.
        let link = after.find(']').and_then(|close| {
            let target = after[close + 1..].strip_prefix('(')?;
            let end = target.find(')')?;
            Some((close, close + 2 + end + 1))
        });
        match link {
            Some((close, skip)) => {
                out.push_str(&after[..close]);
                rest = &after[skip..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    visible(&out.replace("**", "").replace('`', ""))
}

fn visible(text: &str) -> String {
    text.chars()
        .map(|c| if crate::board::is_hidden(c) { ' ' } else { c })
        .collect()
}

/// "just now", "5 min ago", "3 h ago", "2 days ago".
pub fn ago(when: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(when).map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        _ => {
            let days = secs / 86400;
            format!("{days} day{} ago", if days == 1 { "" } else { "s" })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn bullet(text: &str) -> Block {
        Block::Bullet("\u{2022}".into(), text.into())
    }

    #[test]
    fn markdown_blocks() {
        let md = "# Title\n\nSome **bold** and `code` with [a link](https://x).\n\n- one\n* two\n\n```\nlet x = 1;\n```\n---\n| a | b |\n|---|---|\n";
        assert_eq!(
            parse(md),
            vec![
                Block::Heading(1, "Title".into()),
                Block::Gap,
                Block::Text("Some bold and code with a link.".into()),
                Block::Gap,
                bullet("one"),
                bullet("two"),
                Block::Gap,
                Block::Code("let x = 1;".into()),
                Block::Text("| a | b |".into()),
            ]
        );
        assert_eq!(
            parse("#not a heading"),
            vec![Block::Text("#not a heading".into())]
        );
        assert_eq!(parse("a\u{202E}b"), vec![Block::Text("a b".into())]);
        assert_eq!(inline("[unclosed (x"), "[unclosed (x");
    }

    #[test]
    fn joining_lists_and_edges() {
        // Hand-wrapped paragraphs and items are joined; quotes are text.
        assert_eq!(
            parse("one\ntwo\n\n- item\n  more\nafter\n\n> quoted\n> line\n>\nnext"),
            vec![
                Block::Text("one two".into()),
                Block::Gap,
                bullet("item more"),
                Block::Text("after".into()),
                Block::Gap,
                Block::Text("quoted line next".into()),
            ]
        );
        // Numbered steps stay apart; a BOM does not hide the first heading.
        assert_eq!(
            parse("\u{FEFF}# T\r\n1. first\r\n2) second\r\n"),
            vec![
                Block::Heading(1, "T".into()),
                Block::Bullet("1.".into(), "first".into()),
                Block::Bullet("2.".into(), "second".into()),
            ]
        );
        // A task item with a link; a table row is not joined to text.
        assert_eq!(
            parse("- [x] done, see [spec](docs/a.md)\n\n| a |\nText"),
            vec![
                bullet("[x] done, see spec"),
                Block::Gap,
                Block::Text("| a |".into()),
                Block::Gap,
                Block::Text("Text".into()),
            ]
        );
        assert_eq!(inline("__init__.py"), "__init__.py");
        // `~~~` fences work too.
        assert_eq!(
            parse("~~~\nx\n~~~\ny"),
            vec![Block::Code("x".into()), Block::Text("y".into())]
        );
    }

    #[test]
    fn index_then_readme_then_nothing() {
        let root = std::env::temp_dir().join(format!("castorterm-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(load(&root).source, Source::Nothing);
        std::fs::write(root.join("README.md"), "# App").unwrap();
        let index = load(&root);
        assert_eq!(index.source, Source::Readme);
        assert_eq!(index.docs[0].blocks, vec![Block::Heading(1, "App".into())]);
        let dir = root.join(INDEX_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["zeta.md", "state.md", "overview.md", "notes.txt"] {
            std::fs::write(dir.join(name), name).unwrap();
        }
        let index = load(&root);
        assert_eq!(index.source, Source::Index);
        let names: Vec<&str> = index.docs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["overview", "state", "zeta"]);
        assert!(index.updated.is_some());
        // A long file is cut, with a note.
        std::fs::write(dir.join("overview.md"), "word ".repeat(20_000)).unwrap();
        let index = load(&root);
        let last = index.docs[0].blocks.last().unwrap();
        assert!(matches!(last, Block::Text(t) if t.starts_with("[shortened")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn relative_times() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let at = |secs: u64| now - Duration::from_secs(secs);
        assert_eq!(ago(at(5), now), "just now");
        assert_eq!(ago(at(300), now), "5 min ago");
        assert_eq!(ago(at(7200), now), "2 h ago");
        assert_eq!(ago(at(86400), now), "1 day ago");
        assert_eq!(ago(at(3 * 86400), now), "3 days ago");
        assert_eq!(ago(now + Duration::from_secs(9), now), "just now");
    }
}
