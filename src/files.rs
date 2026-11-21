//! File explorer state: directory listings, expanded folders and the paths
//! typed into a session. Pure apart from `read_dir`, so it is unit-tested
//! on temporary folders.

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
pub fn list_dir(dir: &Path) -> std::io::Result<Listing> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if HIDDEN.contains(&name.as_str()) {
            continue;
        }
        // `file_type` does not follow symlinks; resolve them so linked
        // folders can be expanded.
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
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    let omitted = entries.len().saturating_sub(MAX_ENTRIES);
    entries.truncate(MAX_ENTRIES);
    Ok(Listing { entries, omitted })
}

/// One visible line of the tree under a root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Entry {
        depth: usize,
        entry: Entry,
        expanded: bool,
    },
    /// The folder at this depth could not be read.
    Error { depth: usize, message: String },
    /// Entries omitted from a large folder.
    More { depth: usize, count: usize },
}

/// Expanded folders and their cached listings. Rendering only reads the
/// cache; the disk is read on expand and refresh.
#[derive(Default)]
pub struct FileTree {
    expanded: BTreeSet<PathBuf>,
    listings: HashMap<PathBuf, Result<Listing, String>>,
}

impl FileTree {
    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    pub fn expand(&mut self, dir: &Path) {
        self.expanded.insert(dir.to_path_buf());
        self.listings.insert(dir.to_path_buf(), read(dir));
    }

    /// Collapses `dir` and forgets its expanded descendants.
    pub fn collapse(&mut self, dir: &Path) {
        self.expanded.retain(|p| !p.starts_with(dir));
        self.listings.retain(|p, _| !p.starts_with(dir));
    }

    pub fn toggle(&mut self, dir: &Path) {
        if self.is_expanded(dir) {
            self.collapse(dir);
        } else {
            self.expand(dir);
        }
    }

    /// Re-reads every expanded folder.
    pub fn refresh(&mut self) {
        for dir in &self.expanded {
            self.listings.insert(dir.clone(), read(dir));
        }
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
            None => {}
            Some(Err(message)) => rows.push(Row::Error {
                depth,
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
                        count: listing.omitted,
                    });
                }
            }
        }
    }
}

fn read(dir: &Path) -> Result<Listing, String> {
    list_dir(dir).map_err(|err| err.to_string())
}

/// Text typed into a session for `file`: relative to `project` when inside
/// it, quoted for the shell when needed. `None` if it has control characters.
pub fn path_for_terminal(file: &Path, project: &Path, windows: bool) -> Option<String> {
    let path = file.strip_prefix(project).unwrap_or(file);
    let text = path.to_string_lossy();
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(quote_for_shell(&text, windows))
}

/// Single-quotes `text` unless every character is safe unquoted in both
/// PowerShell and POSIX shells.
pub fn quote_for_shell(text: &str, windows: bool) -> String {
    let safe = |c: char| c.is_alphanumeric() || "_-./\\:+".contains(c);
    if text.chars().all(safe) {
        return text.to_string();
    }
    let escaped = if windows {
        text.replace('\'', "''")
    } else {
        text.replace('\'', "'\\''")
    };
    format!("'{escaped}'")
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

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Entry { depth, entry, .. } => format!("{depth}:{}", entry.name),
                Row::Error { depth, .. } => format!("{depth}:<error>"),
                Row::More { depth, count } => format!("{depth}:+{count}"),
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

        tree.expand(&dir);
        assert_eq!(names(&tree.rows(&dir)), ["0:src", "0:Cargo.toml"]);

        tree.toggle(&dir.join("src"));
        assert_eq!(
            names(&tree.rows(&dir)),
            ["0:src", "1:deep", "1:main.rs", "0:Cargo.toml"]
        );
        tree.expand(&dir.join("src/deep"));

        // Collapsing forgets expanded descendants.
        tree.collapse(&dir.join("src"));
        assert!(!tree.is_expanded(&dir.join("src/deep")));
        assert_eq!(names(&tree.rows(&dir)), ["0:src", "0:Cargo.toml"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_picks_up_new_files() {
        let dir = temp_dir("refresh");
        let mut tree = FileTree::default();
        tree.expand(&dir);
        assert!(tree.rows(&dir).is_empty());
        fs::write(dir.join("new.rs"), "").unwrap();
        assert!(tree.rows(&dir).is_empty(), "cache is not re-read on render");
        tree.refresh();
        assert_eq!(names(&tree.rows(&dir)), ["0:new.rs"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_folder_shows_error_row() {
        let dir = temp_dir("missing").join("does-not-exist");
        let mut tree = FileTree::default();
        tree.expand(&dir);
        assert_eq!(names(&tree.rows(&dir)), ["0:<error>"]);
    }

    #[test]
    fn more_row_after_truncated_listing() {
        let dir = temp_dir("more");
        for i in 0..MAX_ENTRIES + 1 {
            fs::write(dir.join(format!("f{i:04}")), "").unwrap();
        }
        let mut tree = FileTree::default();
        tree.expand(&dir);
        let rows = tree.rows(&dir);
        assert_eq!(rows.len(), MAX_ENTRIES + 1);
        assert_eq!(rows.last(), Some(&Row::More { depth: 0, count: 1 }));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paths_are_relative_to_the_project() {
        let project = Path::new("/work/castorterm");
        let inside = project.join("src").join("main.rs");
        let expected = Path::new("src").join("main.rs");
        assert_eq!(
            path_for_terminal(&inside, project, false).as_deref(),
            Some(expected.to_str().unwrap())
        );
        let outside = Path::new("/elsewhere/notes.md");
        assert_eq!(
            path_for_terminal(outside, project, false).as_deref(),
            Some(outside.to_str().unwrap())
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

    #[test]
    fn quoting() {
        assert_eq!(quote_for_shell("src/main.rs", false), "src/main.rs");
        assert_eq!(quote_for_shell(r"C:\src\año.rs", true), r"C:\src\año.rs");
        assert_eq!(quote_for_shell("my notes.md", false), "'my notes.md'");
        assert_eq!(quote_for_shell("my notes.md", true), "'my notes.md'");
        assert_eq!(quote_for_shell("it's.md", true), "'it''s.md'");
        assert_eq!(quote_for_shell("it's.md", false), r"'it'\''s.md'");
        assert_eq!(quote_for_shell("$HOME", false), "'$HOME'");
        assert_eq!(quote_for_shell("a;rm", true), "'a;rm'");
    }
}
