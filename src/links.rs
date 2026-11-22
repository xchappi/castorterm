//! Links in terminal output: URLs and file paths (with `:line:col`), and
//! the rules for opening or previewing them safely.
//!
//! Output is untrusted: a program can print any path. So paths are only
//! read for previews (never executed); anything that could reach the
//! network is refused before it is touched (a network path, a mapped
//! network drive, or a link on the way: Windows would authenticate to that
//! server); and only plain documents and images are handed to the system's
//! default program.

use std::path::{Component, Path, PathBuf};

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

/// Characters that end any link: blanks, quotes, angle brackets, pipes and
/// anything invisible or a control character.
fn ends_any(c: char) -> bool {
    c.is_whitespace()
        || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|')
        || crate::board::is_hidden(c)
}

/// Characters that end a path (URLs keep brackets, which they often have).
fn ends_path(c: char) -> bool {
    ends_any(c) || matches!(c, '(' | ')' | '[' | ']' | '{' | '}')
}

fn starts_url(rest: &[char]) -> bool {
    let head: String = rest.iter().take(8).collect::<String>().to_ascii_lowercase();
    head.starts_with("http://") || head.starts_with("https://")
}

/// Every link candidate on `line` (one char per cell). Paths are only
/// candidates: whether the file exists is checked when it is used.
pub fn find_links(line: &[char]) -> Vec<Link> {
    let mut links = Vec::new();
    let mut i = 0;
    while i < line.len() {
        if ends_path(line[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let url = starts_url(&line[i..]);
        let ends: fn(char) -> bool = if url { ends_any } else { ends_path };
        while i < line.len() && !ends(line[i]) {
            i += 1;
        }
        let mut token: String = line[start..i].iter().collect();
        if url {
            // `(https://x/a_(b))`: drop closing brackets that open nowhere.
            while token.ends_with(')') && token.matches(')').count() > token.matches('(').count() {
                token.pop();
            }
        }
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
    if starts_url(&token.chars().collect::<Vec<_>>()) {
        return (token.len() > 8).then(|| Target::Url(token.to_string()));
    }
    // Options and assignments (`--file=a.rs`, `-o`) are not paths.
    if token.starts_with('-') || token.contains('=') || token.contains("://") {
        return None;
    }
    let (path, line, col) = split_position(token);
    let looks_like_path = path.contains('/') || path.contains('\\') || has_extension(path);
    // `3/4` and dates are not paths.
    let has_letter = path.chars().any(char::is_alphabetic);
    (looks_like_path && has_letter).then(|| Target::Path {
        path: path.to_string(),
        line,
        col,
    })
}

/// `file.rs:12:3` → (`file.rs`, 12, 3); `file.rs:12` → (`file.rs`, 12).
/// A drive colon (`C:\`) is not a position; numbers too big for a line
/// leave the token as it is.
fn split_position(token: &str) -> (&str, Option<u32>, Option<u32>) {
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    if let Some((rest, col)) = token.rsplit_once(':')
        && let Some(col) = number(col)
        && let Some((path, line)) = rest.rsplit_once(':')
        && let Some(line) = number(line)
        && path.len() > 1
    {
        return (path, Some(line), Some(col));
    }
    match token.rsplit_once(':') {
        Some((path, line)) if path.len() > 1 => match number(line) {
            Some(line) => (path, Some(line), None),
            None => (token, None, None),
        },
        _ => (token, None, None),
    }
}

fn has_extension(path: &str) -> bool {
    let path = Path::new(path);
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| (1..=8).contains(&e.len()) && e.chars().any(|c| c.is_ascii_alphabetic()))
        && path.file_stem().is_some_and(|s| !s.is_empty())
}

/// Windows device names, which open devices instead of files.
fn is_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.len() == 4
        && stem.as_bytes()[3].is_ascii_digit())
}

/// Absolute path for a link, relative ones against `base`. `None` for
/// network paths (`\\server\share`, `//server/share`, `\\?\UNC\...`),
/// device names (`CON`, `NUL`...), alternate data streams (`a.exe:x.txt`)
/// and drive-relative paths. Does not touch the file system.
pub fn resolve(raw: &str, base: Option<&Path>) -> Option<PathBuf> {
    if raw.starts_with("\\\\") || raw.starts_with("//") || raw.contains('\0') {
        return None;
    }
    let raw = raw.strip_prefix("file://").unwrap_or(raw);
    if raw.starts_with("\\\\") || raw.starts_with("//") {
        return None;
    }
    let path = Path::new(raw);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        // `\foo` and `C:foo` are relative to some drive's current folder.
        if cfg!(windows)
            && (raw.starts_with('\\') || raw.starts_with('/') || raw.get(1..2) == Some(":"))
        {
            return None;
        }
        base?.join(path)
    };
    let text = full.to_string_lossy();
    if text.starts_with("\\\\") || text.starts_with("//") {
        return None;
    }
    let plain = full.components().all(|c| match c {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            !name.contains(':') && !(cfg!(windows) && is_device_name(&name))
        }
        Component::Prefix(prefix) => matches!(prefix.kind(), std::path::Prefix::Disk(_)),
        _ => true,
    });
    plain.then_some(full)
}

/// Whether `path` can be touched without reaching the network: on Windows,
/// on a local drive and with no link (symlink, junction, mount point) on
/// the way, checked from the root down so nothing behind a link is ever
/// accessed; elsewhere, that it exists.
/// False for missing paths too. Touches the file system: call it off the UI
/// thread.
pub fn is_local(path: &Path) -> bool {
    if !path.is_absolute() || !local_drive(path) {
        return false;
    }
    // Following links is only a risk on Windows (network paths make it
    // authenticate); elsewhere `/tmp` and `/var` are links themselves.
    if !cfg!(windows) {
        return path.try_exists().unwrap_or(false);
    }
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        if matches!(component, Component::Normal(_)) {
            match std::fs::symlink_metadata(&prefix) {
                Ok(meta) if !is_link(&meta) => {}
                _ => return false,
            }
        }
    }
    true
}

fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

/// On Windows, whether the path's drive is a local disk (not a mapped
/// network drive). Elsewhere network mounts look like local folders.
fn local_drive(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        const DRIVE_REMOVABLE: u32 = 2;
        const DRIVE_FIXED: u32 = 3;
        const DRIVE_RAMDISK: u32 = 6;
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return false;
        };
        let std::path::Prefix::Disk(letter) = prefix.kind() else {
            return false;
        };
        let root: Vec<u16> = std::ffi::OsStr::new(&format!("{}:\\", letter as char))
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: `root` is a NUL-terminated wide string that outlives the call.
        let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
        matches!(kind, DRIVE_REMOVABLE | DRIVE_FIXED | DRIVE_RAMDISK)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        true
    }
}

/// The file a path link means: against the project first, then the
/// directory the shell reported; only local, existing paths.
pub fn locate(raw: &str, project: Option<&Path>, cwd: Option<&Path>) -> Option<PathBuf> {
    [project, cwd]
        .into_iter()
        .flatten()
        .chain(Path::new(raw).is_absolute().then_some(Path::new("")))
        .filter_map(|base| resolve(raw, Some(base)))
        .find(|full| is_local(full))
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
    if IMAGE_EXTENSIONS.contains(&extension(path).as_str()) {
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

/// Files the system may open with their default program: images and plain
/// documents only. Anything that can run code (`.exe`, `.bat`, `.ps1`,
/// `.js`, `.lnk`, `.hta`, and `.html`/`.svg`, which run scripts in the
/// browser) is never handed over.
pub fn safe_to_open(path: &Path) -> bool {
    const SAFE: [&str; 12] = [
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "pdf", "txt", "md", "log", "csv", "mp4",
    ];
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    !name.contains(':') && SAFE.contains(&extension(path).as_str())
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
/// lines starting a little above it, with their numbers, without control
/// or invisible characters. `None` if the text looks binary.
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
                    .map(|c| {
                        if crate::board::is_hidden(c) || c == '\t' {
                            ' '
                        } else {
                            c
                        }
                    })
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
        // Not links: versions, fractions, options, bare schemes.
        assert!(targets("v1.2.3 and 3/4 of done https:// --file=src/a.rs:3 -o").is_empty());
    }

    #[test]
    fn urls_keep_balanced_brackets() {
        assert_eq!(
            targets("(see https://en.wikipedia.org/wiki/Foo_(bar))"),
            vec![Target::Url(
                "https://en.wikipedia.org/wiki/Foo_(bar)".into()
            )]
        );
    }

    #[test]
    fn control_characters_end_links() {
        assert_eq!(
            targets("https://ok.example/a\u{1b}[31m"),
            vec![Target::Url("https://ok.example/a".into())]
        );
        assert_eq!(
            targets("a.rs\u{202E}txt.exe"),
            vec![path("a.rs", None, None), path("txt.exe", None, None)]
        );
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
        assert_eq!(
            split_position("C:\\a.rs:3:4"),
            ("C:\\a.rs", Some(3), Some(4))
        );
        assert_eq!(split_position("a.rs:x"), ("a.rs:x", None, None));
        // The last two numbers are the position.
        assert_eq!(split_position("a.rs:3:4:5"), ("a.rs:3", Some(4), Some(5)));
        assert_eq!(
            split_position("a.rs:99999999999"),
            ("a.rs:99999999999", None, None)
        );
    }

    #[test]
    fn network_and_device_paths_are_refused() {
        let base = Path::new(if cfg!(windows) { "C:\\proj" } else { "/proj" });
        assert_eq!(resolve("\\\\evil\\share\\a.png", Some(base)), None);
        assert_eq!(resolve("//evil/share/a.png", Some(base)), None);
        assert_eq!(resolve("file://\\\\evil\\s\\a.png", Some(base)), None);
        assert_eq!(resolve("file:////evil/s/a.png", Some(base)), None);
        assert_eq!(resolve("src/a.rs", Some(base)), Some(base.join("src/a.rs")));
        assert_eq!(resolve("src/a.rs", None), None);
        assert_eq!(resolve("a.exe:x.txt", Some(base)), None);
        if cfg!(windows) {
            assert_eq!(resolve("\\\\?\\UNC\\evil\\s\\a.png", Some(base)), None);
            assert_eq!(resolve("\\\\.\\pipe\\x", Some(base)), None);
            assert_eq!(resolve("\\foo\\a.rs", Some(base)), None);
            assert_eq!(resolve("C:a.png", Some(base)), None);
            assert_eq!(resolve("CON.txt", Some(base)), None);
            assert_eq!(resolve("logs\\nul", Some(base)), None);
            assert_eq!(resolve("COM1.png", Some(base)), None);
            // A network base is refused too.
            assert_eq!(resolve("a.png", Some(Path::new("\\\\srv\\sh"))), None);
        }
    }

    #[test]
    fn only_local_existing_paths() {
        let dir = std::env::temp_dir();
        assert!(is_local(&dir));
        assert!(!is_local(&dir.join("castorterm-surely-missing-file.rs")));
        assert!(!is_local(Path::new("relative.rs")));
        let file = dir.join(format!("castorterm-local-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(locate(&name, Some(&dir), None), Some(dir.join(&name)));
        assert_eq!(
            locate(&name, Some(Path::new("/nowhere-at-all")), Some(&dir)),
            Some(dir.join(&name))
        );
        assert_eq!(
            locate(&file.to_string_lossy(), None, None),
            Some(file.clone())
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn only_documents_open_with_the_system() {
        for ok in ["a.png", "a.PDF", "notes.md", "log.txt"] {
            assert!(safe_to_open(Path::new(ok)), "{ok}");
        }
        for bad in [
            "a.exe",
            "a.bat",
            "a.cmd",
            "a.ps1",
            "a.js",
            "a.vbs",
            "a.lnk",
            "a.hta",
            "a",
            "a.scr",
            "page.html",
            "page.htm",
            "logo.svg",
            "a.url",
            "a.exe:x.txt",
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
        assert_eq!(
            text_excerpt(text, Some(0), 1).unwrap(),
            vec![(1, "1".into())]
        );
        assert!(text_excerpt(text, Some(99), 3).unwrap().is_empty());
        assert_eq!(text_excerpt("a\0b", None, 2), None);
        assert_eq!(text_excerpt("a\u{202E}b", None, 1).unwrap()[0].1, "a b");
    }
}
