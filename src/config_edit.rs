//! Edits to the config file made from the UI (workspaces and projects).
//!
//! Works on the TOML document with `toml_edit`, so comments, formatting and
//! every other setting are kept. Writes are atomic: a temporary file next to
//! the config is renamed over it.

use std::io::ErrorKind;
use std::path::Path;

use toml_edit::{ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value, value};

use crate::config::expand_home;

/// A project as written to the config file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectEntry {
    pub name: String,
    pub path: String,
    pub repo: Option<String>,
}

impl ProjectEntry {
    /// Entry for `path`; on Windows written with forward slashes, which are
    /// valid there and avoid doubled backslashes in TOML.
    pub fn new(name: &str, path: &Path, repo: Option<&str>) -> Self {
        let mut path = path.display().to_string();
        if cfg!(windows) {
            path = path.replace('\\', "/");
        }
        Self {
            name: name.to_string(),
            path,
            repo: repo.map(str::to_string),
        }
    }
}

/// A change to the workspaces in the config file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    AddWorkspace {
        name: String,
    },
    RenameWorkspace {
        from: String,
        to: String,
    },
    RemoveWorkspace {
        name: String,
    },
    /// Appends projects to a workspace, creating it if it is not in the file
    /// (the implicit "Local" workspace). Paths already there are skipped.
    AddProjects {
        workspace: String,
        projects: Vec<ProjectEntry>,
    },
    RemoveProject {
        workspace: String,
        path: String,
    },
}

/// Applies `edit` to the config text and returns the new text.
pub fn apply(text: &str, edit: &Edit, home: Option<&Path>) -> Result<String, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
        format!("the config file has errors: {}", e.message())
    })?;
    match edit {
        Edit::AddWorkspace { name } => {
            if find_workspace(&mut doc, name)?.is_some() {
                return Err(format!("workspace '{name}' already exists"));
            }
            workspaces(&mut doc)?.push(new_workspace(name));
        }
        Edit::RenameWorkspace { from, to } => {
            if find_workspace(&mut doc, to)?.is_some() {
                return Err(format!("workspace '{to}' already exists"));
            }
            let Some(table) = find_workspace(&mut doc, from)? else {
                return Err(format!("workspace '{from}' is not in the config file"));
            };
            table["name"] = value(to.as_str());
        }
        Edit::RemoveWorkspace { name } => {
            let list = workspaces(&mut doc)?;
            let before = list.len();
            list.retain(|t| workspace_name(t) != Some(name.as_str()));
            if list.len() == before {
                return Err(format!("workspace '{name}' is not in the config file"));
            }
        }
        Edit::AddProjects {
            workspace,
            projects,
        } => {
            if find_workspace(&mut doc, workspace)?.is_none() {
                workspaces(&mut doc)?.push(new_workspace(workspace));
            }
            let table = find_workspace(&mut doc, workspace)?.expect("just added");
            for project in projects {
                if !has_project(table, &project.path, home) {
                    push_project(table, project)?;
                }
            }
        }
        Edit::RemoveProject { workspace, path } => {
            let Some(table) = find_workspace(&mut doc, workspace)? else {
                return Err(format!("workspace '{workspace}' is not in the config file"));
            };
            if !remove_project(table, path, home) {
                return Err(format!(
                    "project '{path}' is not in workspace '{workspace}'"
                ));
            }
        }
    }
    Ok(doc.to_string())
}

/// Reads the config file (empty if missing), applies `edit` and writes it
/// back atomically, creating its directory if needed.
pub fn save(path: &Path, edit: &Edit, home: Option<&Path>) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("could not read {}: {err}", path.display())),
    };
    let updated = apply(&text, edit, home)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, updated).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not replace {}: {e}", path.display())
    })
}

fn new_workspace(name: &str) -> Table {
    let mut table = Table::new();
    table["name"] = value(name);
    table
}

/// The `[[workspaces]]` array, created if missing.
fn workspaces(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, String> {
    if !doc.contains_key("workspaces") {
        doc.insert("workspaces", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    doc["workspaces"].as_array_of_tables_mut().ok_or_else(|| {
        "`workspaces` in the config file is not a list of [[workspaces]] tables".into()
    })
}

fn workspace_name(table: &Table) -> Option<&str> {
    table.get("name").and_then(Item::as_str).map(str::trim)
}

fn find_workspace<'a>(
    doc: &'a mut DocumentMut,
    name: &str,
) -> Result<Option<&'a mut Table>, String> {
    Ok(workspaces(doc)?
        .iter_mut()
        .find(|t| workspace_name(t) == Some(name)))
}

/// Same project path, comparing the stored text and its `~` expansion.
fn same_path(stored: &str, wanted: &str, home: Option<&Path>) -> bool {
    let normalize = |p: &str| {
        let expanded = expand_home(p.trim(), home)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| p.trim().to_string());
        let unified = expanded.replace('\\', "/");
        let trimmed = unified.trim_end_matches('/').to_string();
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed
        }
    };
    normalize(stored) == normalize(wanted)
}

fn project_paths(table: &Table) -> Vec<String> {
    let path_of = |t: &dyn TableLike| t.path().map(str::to_string);
    match table.get("projects") {
        Some(Item::ArrayOfTables(list)) => list.iter().filter_map(|t| path_of(t)).collect(),
        Some(Item::Value(Value::Array(list))) => list
            .iter()
            .filter_map(Value::as_inline_table)
            .filter_map(|t| path_of(t))
            .collect(),
        _ => Vec::new(),
    }
}

fn has_project(table: &Table, path: &str, home: Option<&Path>) -> bool {
    project_paths(table)
        .iter()
        .any(|p| same_path(p, path, home))
}

fn push_project(table: &mut Table, project: &ProjectEntry) -> Result<(), String> {
    match table.get_mut("projects") {
        None => {
            table.insert("projects", Item::ArrayOfTables(ArrayOfTables::new()));
            push_project(table, project)
        }
        Some(Item::ArrayOfTables(list)) => {
            let mut entry = Table::new();
            entry["name"] = value(project.name.as_str());
            entry["path"] = value(project.path.as_str());
            if let Some(repo) = &project.repo {
                entry["repo"] = value(repo.as_str());
            }
            list.push(entry);
            Ok(())
        }
        Some(Item::Value(Value::Array(list))) => {
            let mut entry = InlineTable::new();
            entry.insert("name", project.name.as_str().into());
            entry.insert("path", project.path.as_str().into());
            if let Some(repo) = &project.repo {
                entry.insert("repo", repo.as_str().into());
            }
            list.push(entry);
            Ok(())
        }
        Some(_) => Err("`projects` of a workspace in the config file is not a list".into()),
    }
}

fn remove_project(table: &mut Table, path: &str, home: Option<&Path>) -> bool {
    match table.get_mut("projects") {
        Some(Item::ArrayOfTables(list)) => {
            let before = list.len();
            list.retain(|t| !t.path().is_some_and(|p| same_path(p, path, home)));
            list.len() != before
        }
        Some(Item::Value(Value::Array(list))) => {
            let before = list.len();
            list.retain(|v| {
                !v.as_inline_table()
                    .and_then(|t| t.path())
                    .is_some_and(|p| same_path(p, path, home))
            });
            list.len() != before
        }
        _ => false,
    }
}

/// `path` of a project entry, as a table or an inline table.
trait TableLike {
    fn path(&self) -> Option<&str>;
}

impl TableLike for Table {
    fn path(&self) -> Option<&str> {
        self.get("path").and_then(Item::as_str)
    }
}

impl TableLike for InlineTable {
    fn path(&self) -> Option<&str> {
        self.get("path").and_then(Value::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    const BASE: &str = r#"# My castorterm config
[[agents]]
name = "Claude Code" # the main one
command = "claude"

[[workspaces]]
name = "Acme"
board = { provider = "tasks" }

[[workspaces.projects]]
name = "app"
path = "/src/app" # keep this comment
repo = "acme/app"

[[providers]]
name = "tasks"
command = "tasks"
"#;

    fn entry(name: &str, path: &str, repo: Option<&str>) -> ProjectEntry {
        ProjectEntry {
            name: name.into(),
            path: path.into(),
            repo: repo.map(str::to_string),
        }
    }

    /// `(workspace, [(project, repo)])` as castorterm reads them back.
    type Parsed = Vec<(String, Vec<(String, Option<String>)>)>;

    fn workspaces_of(text: &str) -> Parsed {
        // Paths are written verbatim; parse with a home so `~` works.
        let settings = parse(
            text,
            Some(Path::new(if cfg!(windows) { "C:/home" } else { "/home" })),
        )
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
        settings
            .workspaces
            .into_iter()
            .map(|w| {
                (
                    w.name,
                    w.projects.into_iter().map(|p| (p.name, p.repo)).collect(),
                )
            })
            .collect()
    }

    fn abs(path: &str) -> String {
        if cfg!(windows) {
            format!("C:{path}")
        } else {
            path.to_string()
        }
    }

    #[test]
    fn add_workspace_keeps_comments_and_other_settings() {
        let base = BASE.replace("/src/app", &abs("/src/app"));
        let out = apply(
            &base,
            &Edit::AddWorkspace {
                name: "Freelance".into(),
            },
            None,
        )
        .unwrap();
        assert!(out.contains("# My castorterm config"));
        assert!(out.contains("# the main one"));
        assert!(out.contains("# keep this comment"));
        let ws = workspaces_of(&out);
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[1].0, "Freelance");
        assert!(ws[1].1.is_empty());
        assert!(out.contains("[[providers]]"));
    }

    #[test]
    fn duplicate_workspace_is_refused() {
        let err = apply(
            BASE,
            &Edit::AddWorkspace {
                name: "Acme".into(),
            },
            None,
        )
        .unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        let err = apply(
            BASE,
            &Edit::RenameWorkspace {
                from: "Acme".into(),
                to: "Acme".into(),
            },
            None,
        )
        .unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[test]
    fn rename_and_remove_workspace() {
        let base = BASE.replace("/src/app", &abs("/src/app"));
        let out = apply(
            &base,
            &Edit::RenameWorkspace {
                from: "Acme".into(),
                to: "Acme Corp".into(),
            },
            None,
        )
        .unwrap();
        assert_eq!(workspaces_of(&out)[0].0, "Acme Corp");
        assert!(out.contains("board = { provider = \"tasks\" }"));
        let out = apply(
            &out,
            &Edit::RemoveWorkspace {
                name: "Acme Corp".into(),
            },
            None,
        )
        .unwrap();
        assert!(workspaces_of(&out).is_empty());
        assert!(out.contains("[[providers]]"));
        let err = apply(
            &out,
            &Edit::RemoveWorkspace {
                name: "Nope".into(),
            },
            None,
        )
        .unwrap_err();
        assert!(err.contains("not in the config"), "{err}");
    }

    #[test]
    fn add_projects_skips_known_paths_and_creates_missing_workspaces() {
        let base = BASE.replace("/src/app", &abs("/src/app"));
        let edit = Edit::AddProjects {
            workspace: "Acme".into(),
            projects: vec![
                entry("app", &format!("{}/", abs("/src/app")), Some("acme/app")),
                entry("api", &abs("/src/api"), Some("acme/api")),
            ],
        };
        let out = apply(&base, &edit, None).unwrap();
        let ws = workspaces_of(&out);
        assert_eq!(
            ws[0].1,
            vec![
                ("app".to_string(), Some("acme/app".to_string())),
                ("api".to_string(), Some("acme/api".to_string()))
            ]
        );
        assert!(out.contains("# keep this comment"));

        // A workspace that only existed in memory (implicit "Local").
        let edit = Edit::AddProjects {
            workspace: "Local".into(),
            projects: vec![entry("here", &abs("/here"), None)],
        };
        let out = apply(&out, &edit, None).unwrap();
        let ws = workspaces_of(&out);
        assert_eq!(ws[1].0, "Local");
        assert_eq!(ws[1].1, vec![("here".to_string(), None)]);
    }

    #[test]
    fn projects_in_an_empty_file() {
        let edit = Edit::AddProjects {
            workspace: "W".into(),
            projects: vec![entry("p", &abs("/p"), None)],
        };
        let out = apply("", &edit, None).unwrap();
        assert_eq!(
            workspaces_of(&out),
            vec![("W".into(), vec![("p".into(), None)])]
        );
    }

    #[test]
    fn inline_project_lists_are_supported() {
        let text = format!(
            "[[workspaces]]\nname = \"W\"\nprojects = [{{ name = \"a\", path = \"{}\" }}]\n",
            abs("/a")
        );
        let edit = Edit::AddProjects {
            workspace: "W".into(),
            projects: vec![entry("b", &abs("/b"), Some("o/b"))],
        };
        let out = apply(&text, &edit, None).unwrap();
        assert_eq!(workspaces_of(&out)[0].1.len(), 2);
        let out = apply(
            &out,
            &Edit::RemoveProject {
                workspace: "W".into(),
                path: abs("/a"),
            },
            None,
        )
        .unwrap();
        assert_eq!(
            workspaces_of(&out)[0].1,
            vec![("b".into(), Some("o/b".into()))]
        );
    }

    #[test]
    fn remove_project_matches_home_expansion() {
        let home = Path::new(if cfg!(windows) { "C:/home" } else { "/home" });
        let text = "[[workspaces]]\nname = \"W\"\n\n[[workspaces.projects]]\nname = \"p\"\npath = \"~/src/p\"\n";
        let wanted = format!("{}/src/p", home.display());
        let out = apply(
            text,
            &Edit::RemoveProject {
                workspace: "W".into(),
                path: wanted,
            },
            Some(home),
        )
        .unwrap();
        assert!(workspaces_of(&out)[0].1.is_empty());
    }

    #[test]
    fn broken_file_is_not_touched() {
        let err = apply(
            "[[workspaces]\nname=",
            &Edit::AddWorkspace { name: "W".into() },
            None,
        )
        .unwrap_err();
        assert!(err.contains("has errors"), "{err}");
    }

    #[test]
    fn names_with_quotes_are_escaped() {
        let out = apply(
            "",
            &Edit::AddWorkspace {
                name: "Say \"hi\" \\ ok".into(),
            },
            None,
        )
        .unwrap();
        assert_eq!(workspaces_of(&out)[0].0, "Say \"hi\" \\ ok");
    }

    #[test]
    fn save_creates_and_updates_the_file() {
        let dir = std::env::temp_dir().join(format!("castorterm-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("config.toml");
        save(&path, &Edit::AddWorkspace { name: "W".into() }, None).unwrap();
        save(
            &path,
            &Edit::AddProjects {
                workspace: "W".into(),
                projects: vec![entry("p", &abs("/p"), None)],
            },
            None,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(workspaces_of(&text)[0].1.len(), 1);
        assert!(!path.with_extension("toml.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn windows_paths_use_forward_slashes() {
        let e = ProjectEntry::new("p", Path::new(&abs("/src/p")), None);
        assert!(!e.path.contains('\\'));
    }
}
