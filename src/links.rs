//! Links in terminal output: URLs and file paths (with `:line:col`), and
//! the rules for opening or previewing them safely.
//!
//! Output is untrusted: a program can print any path. So paths are only
//! read for previews (never executed), network (UNC) paths are refused
//! (touching one makes Windows authenticate to that server), and only
//! document-like files are handed to the system's default program.

use std::path::{Path, PathBuf};

/// What a link points to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Url(String),
    Path {
        path: String,
        line: Option<u32>,
        col: Option<u32>,
    },
}

/// A link on one line of the screen: columns `start..end`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: Target,
}

/// Characters that end a link candidate.
fn is_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '`' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
        )
}

/// Every link candidate on `line` (one char per cell). Paths are only
/// candidates: whether the file exists is checked when it is used.
pub fn find_links(line: &[char]) -> Vec<Link> {
    let mut links = Vec::new();
    let mut i = 0;
    while i < line.len() {
        if is_delimiter(line[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < line.len() && !is_delimiter(line[i]) {
            i += 1;
        }
        let token: String = line[start..i].iter().collect();
        // Trailing punctuation belongs to the sentence, not the link.
        let trimmed = token.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if trimmed.is_empty() {
            continue;
        }
        let end = start + trimmed.chars().count();
        if let Some(target) = classify(trimmed) {
            links.push(Link { start, end, target });
        }
    }
    links
}

/// The link covering column `col` of `line`, if any.
pub fn link_at(line: &[char], col: usize) -> Option<Link> {
    find_links(line)
        .into_iter()
        .find(|l| (l.start..l.end).contains(&col))
}

fn classify(token: &str) -> Option<Target> {
    let lower = token.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return (token.len() > 8).then(|| Target::Url(token.to_string()));
    }
    let (path, line, col) = split_position(token);
    let looks_like_path = path.contains('/') || path.contains('\\') || has_extension(path);
    // `a/b` style fractions and dates are not paths worth underlining.
    let has_letter = path.chars().any(char::is_alphabetic);
    (looks_like_path && has_letter && !path.contains("://")).then(|| Target::Path {
        path: path.to_string(),
        line,
        col,
    })
}

/// `file.rs:12:3` → (`file.rs`, 12, 3). A drive colon (`C:\`) is not a
/// position.
fn split_position(token: &str) -> (&str, Option<u32>, Option<u32>) {
    let mut parts = token.rsplitn(3, ':');
    let last = parts.next();
    let middle = parts.next();
    let first = parts.next();
    let number = |s: Option<&str>| {
        s.filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .and_then(|s| s.parse::<u32>().ok())
    };
    match (first, middle, last) {
        (Some(path), Some(_), Some(_)) if number(middle).is_some() && number(last).is_some() => {
            (path, number(middle), number(last))
        }
        _ => match token.rsplit_once(':') {
            Some((path, n)) if number(Some(n)).is_some() && path.len() > 1 => {
                (path, number(Some(n)), None)
            }
            _ => (token, None, None),
        },
    }
}

fn has_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| (1..=8).contains(&e.len()) && e.chars().any(|c| c.is_ascii_alphabetic()))
        && Path::new(path).file_stem().is_some_and(|s| !s.is_empty())
}

/// Absolute path for a link, relative ones against `base`. `None` for
/// network paths (`\\server\share`, `//server/share`) and anything else that
/// is not a plain local path.
pub fn resolve(raw: &str, base: Option<&Path>) -> Option<PathBuf> {
    if raw.starts_with("\\\\") || raw.starts_with("//") || raw.contains('\0') {
        return None;
    }
    let raw = raw.strip_prefix("file://").unwrap_or(raw);
    let path = Path::new(raw);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        // `\foo` on Windows is drive-relative: not resolved.
        if cfg!(windows) && (raw.starts_with('\\') || raw.starts_with('/')) {
            return None;
        }
        base?.join(path)
    };
    // Normalising may not reach the network either.
    let text = full.to_string_lossy();
    (!text.starts_with("\\\\") && !text.starts_with("//")).then_some(full)
}

/// How a file can be previewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Text,
}

const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// Preview kind from the extension: images by type, anything else is tried
/// as text (binary files are detected when read).
pub fn kind(path: &Path) -> Kind {
    let ext = extension(path);
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Kind::Image
    } else {
        Kind::Text
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Files the system may open with their default program: documents and
/// media only. Anything that can run code (`.exe`, `.bat`, `.ps1`, `.js`,
/// `.lnk`, `.hta`...) is never handed over.
pub fn safe_to_open(path: &Path) -> bool {
    const SAFE: [&str; 16] = [
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "pdf", "txt", "md", "log", "csv",
        "json", "html", "htm", "mp4",
    ];
    SAFE.contains(&extension(path).as_str())
}

/// Whether `text` can be passed to a `.cmd`/`.bat` program as an argument:
/// cmd.exe would interpret these characters even inside quotes.
pub fn safe_for_cmd(text: &str) -> bool {
    !text
        .chars()
        .any(|c| matches!(c, '"' | '%' | '&' | '|' | '<' | '>' | '^' | '!'))
}

/// The `-g` argument of VS Code-like editors: `path:line:col`.
pub fn goto_argument(path: &Path, line: Option<u32>, col: Option<u32>) -> String {
    let mut text = path.to_string_lossy().into_owned();
    if let Some(line) = line {
        text.push_str(&format!(":{line}"));
        if let Some(col) = col {
            text.push_str(&format!(":{col}"));
        }
    }
    text
}

/// Lines of `text` to preview around `line` (1-based): up to `count`
/// lines starting a little above it, with their numbers. `None` if the text
/// looks binary.
pub fn text_excerpt(text: &str, line: Option<u32>, count: usize) -> Option<Vec<(usize, String)>> {
    if text.contains('\0') {
        return None;
    }
    let first = line.map_or(1, |l| (l as usize).saturating_sub(3).max(1));
    Some(
        text.lines()
            .enumerate()
            .skip(first - 1)
            .take(count)
            .map(|(i, l)| {
                let shown: String = l
                    .chars()
                    .take(200)
                    .map(|c| if c == '\t' { ' ' } else { c })
                    .collect();
                (i + 1, shown)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn targets(s: &str) -> Vec<Target> {
        find_links(&chars(s))
            .into_iter()
            .map(|l| l.target)
            .collect()
    }

    fn path(p: &str, line: Option<u32>, col: Option<u32>) -> Target {
        Target::Path {
            path: p.into(),
            line,
            col,
        }
    }

    #[test]
    fn urls_and_paths() {
        assert_eq!(
            targets("see https://example.com/a?b=1, then"),
            vec![Target::Url("https://example.com/a?b=1".into())]
        );
        assert_eq!(
            targets("error at src/view.rs:120:5: bad"),
            vec![path("src/view.rs", Some(120), Some(5))]
        );
        assert_eq!(
            targets("wrote (out/shot.png)."),
            vec![path("out/shot.png", None, None)]
        );
        assert_eq!(
            targets("C:\\x\\y.txt:3"),
            vec![path("C:\\x\\y.txt", Some(3), None)]
        );
        assert_eq!(targets("README.md"), vec![path("README.md", None, None)]);
        // Not links: versions, fractions, plain words, bare schemes.
        assert!(targets("v1.2.3 and 3/4 of done https://").is_empty());
    }

    #[test]
    fn link_columns() {
        let line = chars("ok  src/a.rs:9 x");
        let link = link_at(&line, 6).unwrap();
        assert_eq!((link.start, link.end), (4, 14));
        assert!(link_at(&line, 2).is_none());
        assert!(link_at(&line, 15).is_none());
    }

    #[test]
    fn positions() {
        assert_eq!(split_position("a.rs:1:2"), ("a.rs", Some(1), Some(2)));
        assert_eq!(split_position("a.rs:7"), ("a.rs", Some(7), None));
        assert_eq!(split_position("C:\\a.rs"), ("C:\\a.rs", None, None));
        assert_eq!(split_position("C:\\a.rs:4"), ("C:\\a.rs", Some(4), None));
        assert_eq!(split_position("a.rs:x"), ("a.rs:x", None, None));
    }

    #[test]
    fn network_paths_are_refused() {
        let base = Path::new(if cfg!(windows) { "C:\\proj" } else { "/proj" });
        assert_eq!(resolve("\\\\evil\\share\\a.png", Some(base)), None);
        assert_eq!(resolve("//evil/share/a.png", Some(base)), None);
        assert_eq!(resolve("src/a.rs", Some(base)), Some(base.join("src/a.rs")));
        assert_eq!(resolve("src/a.rs", None), None);
    }

    #[test]
    fn only_documents_open_with_the_system() {
        for ok in ["a.png", "a.PDF", "notes.md", "page.html"] {
            assert!(safe_to_open(Path::new(ok)), "{ok}");
        }
        for bad in [
            "a.exe", "a.bat", "a.cmd", "a.ps1", "a.js", "a.vbs", "a.lnk", "a.hta", "a", "a.scr",
        ] {
            assert!(!safe_to_open(Path::new(bad)), "{bad}");
        }
        assert_eq!(kind(Path::new("x.PNG")), Kind::Image);
        assert_eq!(kind(Path::new("x.rs")), Kind::Text);
    }

    #[test]
    fn cmd_arguments_and_goto() {
        assert!(safe_for_cmd("C:\\proj\\src\\a.rs:3"));
        assert!(!safe_for_cmd("a&calc.rs"));
        assert!(!safe_for_cmd("100%.txt"));
        assert_eq!(
            goto_argument(Path::new("a.rs"), Some(3), Some(4)),
            "a.rs:3:4"
        );
        assert_eq!(goto_argument(Path::new("a.rs"), None, Some(4)), "a.rs");
    }

    #[test]
    fn excerpts() {
        let text = "1\n2\n3\n4\n5\n6\n7";
        let ex = text_excerpt(text, Some(5), 3).unwrap();
        assert_eq!(ex, vec![(2, "2".into()), (3, "3".into()), (4, "4".into())]);
        assert_eq!(text_excerpt(text, None, 2).unwrap().len(), 2);
        assert_eq!(text_excerpt("a\0b", None, 2), None);
    }
}
