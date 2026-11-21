//! File explorer state: directory listings, expanded folders and the paths
//! typed into a session. `FileTree` never touches the disk itself: callers
//! run [`load`] (on a background thread) and hand the result back, so a
//! slow or stalled folder cannot freeze the UI.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Entries shown per folder; the rest are summarised in a "N more" row.
pub const MAX_ENTRIES: usize = 500;

/// Folders never shown in the tree.
const HIDDEN: &[&str] = &[".git"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Entries left out because of [`MAX_ENTRIES`].
    pub omitted: usize,
}

/// Lists `dir`: folders first, then files, each sorted by name ignoring case.
/// Entries that cannot be read are skipped; only an unreadable folder fails.
pub fn list_dir(dir: &Path) -> std::io::Result<Listing> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if HIDDEN.contains(&name.as_str()) {
            continue;
        }
        // `file_type` does not follow symlinks; resolve them so linked
        // folders can be expanded (expansion is user-driven, so a symlink
        // loop only grows as far as the user clicks).
        let is_dir = entry
            .file_type()
            .map(|t| t.is_dir() || (t.is_symlink() && entry.path().is_dir()))
            .unwrap_or(false);
        entries.push(Entry {
            name,
            path: entry.path(),
            is_dir,
        });
    }
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase(), e.name.clone()));
    let omitted = entries.len().saturating_sub(MAX_ENTRIES);
    entries.truncate(MAX_ENTRIES);
    Ok(Listing { entries, omitted })
}

/// Listing result as stored in the tree.
pub type LoadResult = Result<Listing, String>;

/// Reads a folder for the tree. Blocking: run it off the UI thread.
pub fn load(dir: &Path) -> LoadResult {
    list_dir(dir).map_err(|err| err.to_string())
}

/// One visible line of the tree under a root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Entry {
        depth: usize,
        entry: Entry,
        expanded: bool,
    },
    /// The folder's listing has not arrived yet.
    Loading { depth: usize, dir: PathBuf },
    /// The folder at this depth could not be read.
    Error {
        depth: usize,
        dir: PathBuf,
        message: String,
    },
    /// Entries omitted from a large folder.
    More {
        depth: usize,
        dir: PathBuf,
        count: usize,
    },
}

/// Identifies one load request, so a slow, older read cannot overwrite a
/// newer one for the same folder.
pub type LoadTicket = u64;

/// Expanded folders and their latest listings.
#[derive(Default)]
pub struct FileTree {
    expanded: BTreeSet<PathBuf>,
    listings: HashMap<PathBuf, LoadResult>,
    /// Latest ticket issued per folder.
    tickets: HashMap<PathBuf, LoadTicket>,
    next_ticket: LoadTicket,
}

impl FileTree {
    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    /// Marks `dir` expanded. The caller must take a [`FileTree::ticket`],
    /// [`load`] the folder and pass both to [`FileTree::set_listing`]; until
    /// then a loading row is shown (or the previous listing, if any).
    pub fn expand(&mut self, dir: &Path) {
        self.expanded.insert(dir.to_path_buf());
    }

    /// Starts a load of `dir`; only the latest ticket's result is kept.
    pub fn ticket(&mut self, dir: &Path) -> LoadTicket {
        self.next_ticket += 1;
        self.tickets.insert(dir.to_path_buf(), self.next_ticket);
        self.next_ticket
    }

    /// Collapses `dir` and forgets its expanded descendants.
    pub fn collapse(&mut self, dir: &Path) {
        self.expanded.retain(|p| !p.starts_with(dir));
        self.listings.retain(|p, _| !p.starts_with(dir));
        self.tickets.retain(|p, _| !p.starts_with(dir));
    }

    /// Expands or collapses `dir`; returns true if it now needs loading.
    pub fn toggle(&mut self, dir: &Path) -> bool {
        if self.is_expanded(dir) {
            self.collapse(dir);
            false
        } else {
            self.expand(dir);
            true
        }
    }

    /// Stores a listing, unless the folder was collapsed meanwhile or a
    /// newer load of it was started.
    pub fn set_listing(&mut self, dir: &Path, ticket: LoadTicket, listing: LoadResult) {
        if self.is_expanded(dir) && self.tickets.get(dir) == Some(&ticket) {
            self.listings.insert(dir.to_path_buf(), listing);
        }
    }

    /// Folders to re-read on refresh: every expanded folder (all of them are
    /// visible, since collapsing forgets descendants).
    pub fn expanded_dirs(&self) -> Vec<PathBuf> {
        self.expanded.iter().cloned().collect()
    }

    /// Visible rows under `root` (not including the root itself).
    pub fn rows(&self, root: &Path) -> Vec<Row> {
        let mut rows = Vec::new();
        if self.is_expanded(root) {
            self.push_rows(root, 0, &mut rows);
        }
        rows
    }

    fn push_rows(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        match self.listings.get(dir) {
            None => rows.push(Row::Loading {
                depth,
                dir: dir.to_path_buf(),
            }),
            Some(Err(message)) => rows.push(Row::Error {
                depth,
                dir: dir.to_path_buf(),
                message: message.clone(),
            }),
            Some(Ok(listing)) => {
                for entry in &listing.entries {
                    let expanded = entry.is_dir && self.is_expanded(&entry.path);
                    rows.push(Row::Entry {
                        depth,
                        entry: entry.clone(),
                        expanded,
                    });
                    if expanded {
                        self.push_rows(&entry.path, depth + 1, rows);
                    }
                }
                if listing.omitted > 0 {
                    rows.push(Row::More {
                        depth,
                        dir: dir.to_path_buf(),
                        count: listing.omitted,
                    });
                }
            }
        }
    }
}

/// Characters PowerShell treats as single quotes (ASCII plus the
/// typographic ones). Inside a single-quoted string each must be doubled,
/// or a crafted file name could close the string and inject code.
const POWERSHELL_QUOTES: &[char] = &['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// Text typed into a session for `file`: relative to `project` when inside
/// it, quoted for the shell when needed. File names come from the repo and
/// are untrusted, so the result is always a single literal argument.
/// `None` if the path is not valid Unicode or has control characters.
pub fn path_for_terminal(file: &Path, project: &Path, windows: bool) -> Option<String> {
    let relative = file.strip_prefix(project).ok();
    let path = relative.unwrap_or(file);
    let text = path.to_str()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    // A leading '-' would be read as an option by the next command.
    let text = if relative.is_some() && text.starts_with('-') {
        format!("./{text}")
    } else {
        text.to_string()
    };
    Some(quote_for_shell(&text, windows))
}

/// Single-quotes `text` unless every character is safe unquoted in the
/// target shell (PowerShell on Windows, POSIX shells elsewhere).
pub fn quote_for_shell(text: &str, windows: bool) -> String {
    // Unquoted backslashes escape the next character in POSIX shells.
    let extra = if windows { "_-./\\:+" } else { "_-./:+" };
    if text
        .chars()
        .all(|c| c.is_alphanumeric() || extra.contains(c))
    {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        if windows {
            if POWERSHELL_QUOTES.contains(&c) {
                out.push(c);
            }
            out.push(c);
        } else if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Fresh, empty temporary folder for one test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("castorterm-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Expands and loads synchronously, as the view does in the background.
    fn expand_now(tree: &mut FileTree, dir: &Path) {
        tree.expand(dir);
        load_now(tree, dir);
    }

    fn load_now(tree: &mut FileTree, dir: &Path) {
        let ticket = tree.ticket(dir);
        tree.set_listing(dir, ticket, load(dir));
    }

    fn refresh_now(tree: &mut FileTree) {
        for dir in tree.expanded_dirs() {
            load_now(tree, &dir);
        }
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Entry { depth, entry, .. } => format!("{depth}:{}", entry.name),
                Row::Loading { depth, .. } => format!("{depth}:<loading>"),
                Row::Error { depth, .. } => format!("{depth}:<error>"),
                Row::More { depth, count, .. } => format!("{depth}:+{count}"),
            })
            .collect()
    }

    #[test]
    fn listing_sorts_folders_first_and_hides_git() {
        let dir = temp_dir("sort");
        fs::write(dir.join("b.txt"), "").unwrap();
        fs::write(dir.join("A.md"), "").unwrap();
        fs::create_dir(dir.join("src")).unwrap();
        fs::create_dir(dir.join(".git")).unwrap();
        let listing = list_dir(&dir).unwrap();
        let got: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(got, ["src", "A.md", "b.txt"]);
        assert!(listing.entries[0].is_dir);
        assert_eq!(listing.omitted, 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn large_folders_are_truncated() {
        let dir = temp_dir("large");
        for i in 0..MAX_ENTRIES + 3 {
            fs::write(dir.join(format!("f{i:04}")), "").unwrap();
        }
        let listing = list_dir(&dir).unwrap();
        assert_eq!(listing.entries.len(), MAX_ENTRIES);
        assert_eq!(listing.omitted, 3);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tree_rows_follow_expansion() {
        let dir = temp_dir("tree");
        fs::create_dir_all(dir.join("src/deep")).unwrap();
        fs::write(dir.join("src/main.rs"), "").unwrap();
        fs::write(dir.join("Cargo.toml"), "").unwrap();

        let mut tree = FileTree::default();
        assert!(tree.rows(&dir).is_empty());

        expand_now(&mut tree, &dir);
        assert_eq!(names(&tree.rows(&dir)), ["0:src", "0:Cargo.toml"]);

        assert!(tree.toggle(&dir.join("src")));
        load_now(&mut tree, &dir.join("src"));
        assert_eq!(
            names(&tree.rows(&dir)),
            ["0:src", "1:deep", "1:main.rs", "0:Cargo.toml"]
        );
        expand_now(&mut tree, &dir.join("src/deep"));

        // Collapsing forgets expanded descendants.
        assert!(!tree.toggle(&dir.join("src")));
        assert!(!tree.is_expanded(&dir.join("src/deep")));
        assert_eq!(names(&tree.rows(&dir)), ["0:src", "0:Cargo.toml"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loading_row_until_listing_arrives() {
        let dir = temp_dir("loading");
        let mut tree = FileTree::default();
        tree.expand(&dir);
        let ticket = tree.ticket(&dir);
        assert_eq!(names(&tree.rows(&dir)), ["0:<loading>"]);
        tree.set_listing(&dir, ticket, Ok(Listing::default()));
        assert!(tree.rows(&dir).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn late_listing_for_collapsed_folder_is_ignored() {
        let dir = temp_dir("late");
        let mut tree = FileTree::default();
        tree.expand(&dir);
        let ticket = tree.ticket(&dir);
        tree.collapse(&dir);
        tree.set_listing(&dir, ticket, load(&dir));
        tree.expand(&dir);
        assert_eq!(names(&tree.rows(&dir)), ["0:<loading>"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn older_load_cannot_overwrite_newer() {
        let dir = temp_dir("race");
        let mut tree = FileTree::default();
        tree.expand(&dir);
        let old = tree.ticket(&dir);
        let new = tree.ticket(&dir);
        let fresh = Listing {
            entries: vec![Entry {
                name: "fresh.rs".into(),
                path: dir.join("fresh.rs"),
                is_dir: false,
            }],
            omitted: 0,
        };
        tree.set_listing(&dir, new, Ok(fresh));
        // The older, slower read arrives last and must be dropped.
        tree.set_listing(&dir, old, Ok(Listing::default()));
        assert_eq!(names(&tree.rows(&dir)), ["0:fresh.rs"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_picks_up_new_files() {
        let dir = temp_dir("refresh");
        let mut tree = FileTree::default();
        expand_now(&mut tree, &dir);
        assert!(tree.rows(&dir).is_empty());
        fs::write(dir.join("new.rs"), "").unwrap();
        assert!(tree.rows(&dir).is_empty(), "cache is not re-read on render");
        refresh_now(&mut tree);
        assert_eq!(names(&tree.rows(&dir)), ["0:new.rs"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_folder_shows_error_row() {
        let dir = temp_dir("missing").join("does-not-exist");
        let mut tree = FileTree::default();
        expand_now(&mut tree, &dir);
        assert_eq!(names(&tree.rows(&dir)), ["0:<error>"]);
    }

    #[test]
    fn more_row_after_truncated_listing() {
        let dir = temp_dir("more");
        for i in 0..MAX_ENTRIES + 1 {
            fs::write(dir.join(format!("f{i:04}")), "").unwrap();
        }
        let mut tree = FileTree::default();
        expand_now(&mut tree, &dir);
        let rows = tree.rows(&dir);
        assert_eq!(rows.len(), MAX_ENTRIES + 1);
        assert!(matches!(rows.last(), Some(Row::More { count: 1, .. })));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paths_are_relative_to_the_project() {
        let project = Path::new("/work/castorterm");
        let inside = project.join("src").join("main.rs");
        let expected = Path::new("src").join("main.rs");
        assert_eq!(
            path_for_terminal(&inside, project, cfg!(windows)).as_deref(),
            Some(expected.to_str().unwrap())
        );
        let outside = Path::new("/elsewhere/notes.md");
        assert_eq!(
            path_for_terminal(outside, project, cfg!(windows)).as_deref(),
            Some(outside.to_str().unwrap())
        );
    }

    #[test]
    fn leading_dash_is_not_an_option() {
        let project = Path::new("/p");
        assert_eq!(
            path_for_terminal(&project.join("-rf"), project, true).as_deref(),
            Some("./-rf")
        );
        assert_eq!(
            path_for_terminal(&project.join("--force.txt"), project, false).as_deref(),
            Some("./--force.txt")
        );
    }

    #[test]
    fn paths_with_control_characters_are_rejected() {
        let project = Path::new("/p");
        assert_eq!(
            path_for_terminal(&project.join("a\nb"), project, false),
            None
        );
        assert_eq!(path_for_terminal(project, project, false), None);
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_paths_are_rejected() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let project = Path::new("/p");
        let file = project.join(OsStr::from_bytes(b"bad\xff.rs"));
        assert_eq!(path_for_terminal(&file, project, false), None);
    }

    #[test]
    fn quoting_plain_and_spaces() {
        assert_eq!(quote_for_shell("src/main.rs", false), "src/main.rs");
        assert_eq!(quote_for_shell(r"C:\src\año.rs", true), r"C:\src\año.rs");
        assert_eq!(quote_for_shell("my notes.md", false), "'my notes.md'");
        assert_eq!(quote_for_shell("my notes.md", true), "'my notes.md'");
    }

    #[test]
    fn quoting_escapes_quotes_per_shell() {
        assert_eq!(quote_for_shell("it's.md", true), "'it''s.md'");
        assert_eq!(quote_for_shell("it's.md", false), r"'it'\''s.md'");
    }

    #[test]
    fn quoting_powershell_smart_quotes() {
        // Each typographic quote closes a PowerShell string unless doubled.
        let name = "x\u{2019};Start-Process calc;\u{2018}y";
        assert_eq!(
            quote_for_shell(name, true),
            "'x\u{2019}\u{2019};Start-Process calc;\u{2018}\u{2018}y'"
        );
        assert_eq!(
            quote_for_shell("a\u{201A}b\u{201B}c", true),
            "'a\u{201A}\u{201A}b\u{201B}\u{201B}c'"
        );
        // POSIX shells do not treat them as quotes.
        assert_eq!(quote_for_shell(name, false), format!("'{name}'"));
    }

    #[test]
    fn quoting_metacharacters() {
        for text in [
            "$HOME", "a;rm", "a`b", "a\"b", "@args", "a,b", "100%", "a&b", "a|b",
        ] {
            for windows in [true, false] {
                let quoted = quote_for_shell(text, windows);
                assert!(quoted.starts_with('\'') && quoted.ends_with('\''), "{text}");
            }
        }
    }

    #[test]
    fn posix_backslash_is_quoted() {
        assert_eq!(quote_for_shell(r"a\b", false), r"'a\b'");
        assert_eq!(quote_for_shell(r"a\b", true), r"a\b");
    }
}
