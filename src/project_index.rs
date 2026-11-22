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
/// Files and bytes read at most.
const MAX_FILES: usize = 20;
const MAX_BYTES: u64 = 64 * 1024;

/// One Markdown document of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    /// File name without extension (`overview`).
    pub name: String,
    pub text: String,
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
/// Only plain files are read (no links), each bounded.
pub fn load(root: &Path) -> ProjectIndex {
    let dir = root.join(INDEX_DIR);
    let mut files: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")))
                .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
                .collect()
        })
        .unwrap_or_default();
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
        if let Some(text) = read_plain(&path) {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                updated = updated.max(Some(modified));
            }
            docs.push(Doc { name, text });
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
        Some(text) => ProjectIndex {
            source: Source::Readme,
            docs: vec![Doc {
                name: "README".into(),
                text,
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

/// Text of a plain file (not a link), at most [`MAX_BYTES`].
fn read_plain(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || crate::links::is_link(&meta) {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A line of Markdown, simplified for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(u8, String),
    Bullet(String),
    Code(String),
    Text(String),
    Gap,
}

/// Splits Markdown into display blocks: headings, bullets, code lines and
/// text, with inline markup (`**`, backticks, links) reduced to its text
/// and invisible characters removed.
pub fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
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
            blocks.push(Block::Bullet(inline(item)));
        } else if t.starts_with("|") || t.chars().all(|c| matches!(c, '-' | '=' | '*' | '_' | ' '))
        {
            // Tables and rules: shown as plain text, rules dropped.
            if t.starts_with('|') && !t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                blocks.push(Block::Text(inline(t)));
            }
        } else {
            // A quote is shown as its text.
            let t = t.strip_prefix('>').map_or(t, str::trim_start);
            let text = inline(t);
            // Lines wrapped by hand belong to the paragraph or item above.
            match blocks.last_mut() {
                Some(Block::Text(previous)) => {
                    previous.push(' ');
                    previous.push_str(&text);
                }
                Some(Block::Bullet(previous))
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

/// Inline markup to plain text: `**b**`, `*i*`, `` `c` ``, `[t](url)`.
fn inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find("](").and_then(|close| {
            let target = &after[close + 2..];
            target.find(')').map(|end| (close, close + 2 + end + 1))
        }) {
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
    visible(&out.replace("**", "").replace("__", "").replace('`', ""))
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
                Block::Bullet("one".into()),
                Block::Bullet("two".into()),
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
        // Hand-wrapped paragraphs and items are joined; quotes are text.
        assert_eq!(
            parse("one\ntwo\n\n- item\n  more\nafter\n\n> quoted\n> line"),
            vec![
                Block::Text("one two".into()),
                Block::Gap,
                Block::Bullet("item more".into()),
                Block::Text("after".into()),
                Block::Gap,
                Block::Text("quoted line".into()),
            ]
        );
        assert_eq!(inline("[unclosed (x"), "[unclosed (x");
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
        assert_eq!(index.docs[0].text, "# App");
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
