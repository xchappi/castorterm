//! User configuration (TOML): workspaces, projects, agents and terminal font.
//!
//! The file is optional. Mistakes never prevent startup: they become
//! messages shown in the sidebar and the affected entries are skipped.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Environment variable that overrides the config file location.
pub const CONFIG_ENV: &str = "CASTORTERM_CONFIG";

const MIN_FONT_SIZE: f32 = 6.0;
const MAX_FONT_SIZE: f32 = 72.0;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    font: RawFont,
    #[serde(default)]
    agents: Vec<RawAgent>,
    #[serde(default)]
    workspaces: Vec<RawWorkspace>,
    #[serde(default)]
    ui: RawUi,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUi {
    scanlines: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFont {
    family: Option<String>,
    size: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgent {
    name: String,
    command: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspace {
    name: String,
    #[serde(default)]
    projects: Vec<RawProject>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    name: String,
    path: String,
}

/// An agent launcher: a command typed into a fresh shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub name: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    pub projects: Vec<Project>,
}

/// Validated configuration plus the problems found while loading it.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub agents: Vec<Agent>,
    pub workspaces: Vec<Workspace>,
    /// CRT-style scanlines over the terminal (on by default).
    pub scanlines: bool,
    /// Human readable warnings and errors for the sidebar.
    pub messages: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: None,
            agents: Vec::new(),
            workspaces: Vec::new(),
            scanlines: true,
            messages: Vec::new(),
        }
    }
}

/// The environment values that decide where the config lives.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub windows: bool,
    pub config_override: Option<PathBuf>,
    pub appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Env {
    pub fn from_process() -> Self {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let windows = cfg!(windows);
        Self {
            windows,
            config_override: var(CONFIG_ENV),
            appdata: var("APPDATA"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            home: if windows {
                var("USERPROFILE")
            } else {
                var("HOME")
            },
        }
    }
}

/// Where the config file is expected, or `None` if no base directory is known.
pub fn config_path(env: &Env) -> Option<PathBuf> {
    if let Some(path) = &env.config_override {
        return Some(path.clone());
    }
    let base = if env.windows {
        env.appdata.clone()
    } else {
        // The XDG spec says relative values must be ignored.
        env.xdg_config_home
            .clone()
            .filter(|p| p.is_absolute())
            .or_else(|| env.home.as_ref().map(|h| h.join(".config")))
    }?;
    Some(base.join("castorterm").join("config.toml"))
}

/// Reads and validates the config file. Never fails: problems are reported
/// in [`Settings::messages`] and defaults are used.
pub fn load(env: &Env) -> Settings {
    let Some(path) = config_path(env) else {
        return Settings::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text, env.home.as_deref()).unwrap_or_else(|err| Settings {
            messages: vec![format!("Config {}: {err}", path.display())],
            ..Settings::default()
        }),
        Err(err) if err.kind() == ErrorKind::NotFound => Settings::default(),
        Err(err) => Settings {
            messages: vec![format!("Could not read config {}: {err}", path.display())],
            ..Settings::default()
        },
    }
}

/// Parses the TOML text. Syntax and schema errors fail the whole file;
/// invalid individual entries are skipped with a message.
pub fn parse(text: &str, home: Option<&Path>) -> Result<Settings, String> {
    let raw: RawConfig = toml::from_str(text).map_err(|e| match e.span() {
        Some(span) => {
            let line = text[..span.start.min(text.len())].matches('\n').count() + 1;
            format!("line {line}: {}", e.message())
        }
        None => e.message().to_string(),
    })?;
    let mut settings = Settings {
        scanlines: raw.ui.scanlines.unwrap_or(true),
        ..Settings::default()
    };

    settings.font_family = raw
        .font
        .family
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty());
    settings.font_size = match raw.font.size {
        Some(size) if (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&size) => Some(size),
        Some(size) => {
            settings.messages.push(format!(
                "font.size {size} ignored: must be between {MIN_FONT_SIZE} and {MAX_FONT_SIZE}"
            ));
            None
        }
        None => None,
    };

    for agent in raw.agents {
        let name = agent.name.trim();
        let command = agent.command.trim();
        if !valid_text(name) || !valid_text(command) {
            settings.messages.push(format!(
                "Agent '{}' skipped: name and command must be non-empty single lines",
                name.escape_debug()
            ));
            continue;
        }
        settings.agents.push(Agent {
            name: name.into(),
            command: command.into(),
        });
    }

    for workspace in raw.workspaces {
        let workspace_name = workspace.name.trim();
        if !valid_text(workspace_name) {
            settings
                .messages
                .push("Workspace skipped: name must be a non-empty single line".into());
            continue;
        }
        let mut projects = Vec::new();
        for project in workspace.projects {
            let name = project.name.trim();
            if !valid_text(name) {
                settings.messages.push(format!(
                    "Project in '{workspace_name}' skipped: name must be a non-empty single line"
                ));
                continue;
            }
            match expand_home(project.path.trim(), home) {
                Some(path) if path.is_absolute() => projects.push(Project {
                    name: name.into(),
                    path,
                }),
                _ => settings.messages.push(format!(
                    "Project '{}' skipped: path '{}' must be absolute",
                    project.name, project.path
                )),
            }
        }
        settings.workspaces.push(Workspace {
            name: workspace_name.into(),
            projects,
        });
    }

    Ok(settings)
}

/// Non-empty and free of control characters: commands are typed into the
/// shell, so an embedded newline would run extra commands.
fn valid_text(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

/// Expands a leading `~` (alone or followed by a separator) to `home`.
/// Returns `None` when `~` is used but the home directory is unknown.
fn expand_home(path: &str, home: Option<&Path>) -> Option<PathBuf> {
    let rest = match path.strip_prefix('~') {
        None => return Some(PathBuf::from(path)),
        Some(rest) => rest,
    };
    if rest.is_empty() {
        return home.map(Path::to_path_buf);
    }
    if let Some(rest) = rest.strip_prefix(['/', '\\']) {
        return home.map(|h| h.join(rest));
    }
    // `~user` is not supported; keep it literal so validation rejects it.
    Some(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(unix: &str, windows: &str) -> String {
        if cfg!(windows) { windows } else { unix }.to_string()
    }

    #[test]
    fn path_override_wins() {
        let env = Env {
            config_override: Some("/tmp/x.toml".into()),
            appdata: Some("C:/AppData".into()),
            windows: true,
            ..Env::default()
        };
        assert_eq!(config_path(&env), Some(PathBuf::from("/tmp/x.toml")));
    }

    #[test]
    fn path_windows_uses_appdata() {
        let env = Env {
            windows: true,
            appdata: Some("C:/AppData".into()),
            home: Some("C:/Users/me".into()),
            ..Env::default()
        };
        assert_eq!(
            config_path(&env),
            Some(PathBuf::from("C:/AppData/castorterm/config.toml"))
        );
        let env = Env {
            windows: true,
            ..Env::default()
        };
        assert_eq!(config_path(&env), None);
    }

    #[test]
    fn path_unix_prefers_absolute_xdg_then_home() {
        let base = abs("/xdg", "C:/xdg");
        let env = Env {
            xdg_config_home: Some(base.clone().into()),
            home: Some("/home/me".into()),
            ..Env::default()
        };
        assert_eq!(
            config_path(&env),
            Some(PathBuf::from(base).join("castorterm/config.toml"))
        );
        let env = Env {
            xdg_config_home: Some("relative".into()),
            home: Some("/home/me".into()),
            ..Env::default()
        };
        assert_eq!(
            config_path(&env),
            Some(PathBuf::from("/home/me/.config/castorterm/config.toml"))
        );
    }

    #[test]
    fn empty_config_is_default() {
        assert_eq!(parse("", None).unwrap(), Settings::default());
    }

    #[test]
    fn full_config() {
        let path = abs("/src/castorterm", "D:/src/castorterm");
        let text = format!(
            r#"
            [font]
            family = "Cascadia Mono"
            size = 16

            [[agents]]
            name = "Claude Code"
            command = "claude"

            [[workspaces]]
            name = "CastorLabs"

            [[workspaces.projects]]
            name = "castorterm"
            path = "{path}"
            "#
        );
        let s = parse(&text, None).unwrap();
        assert_eq!(s.font_family.as_deref(), Some("Cascadia Mono"));
        assert_eq!(s.font_size, Some(16.0));
        assert_eq!(
            s.agents,
            vec![Agent {
                name: "Claude Code".into(),
                command: "claude".into()
            }]
        );
        assert_eq!(s.workspaces.len(), 1);
        assert_eq!(s.workspaces[0].name, "CastorLabs");
        assert_eq!(
            s.workspaces[0].projects,
            vec![Project {
                name: "castorterm".into(),
                path: PathBuf::from(path)
            }]
        );
        assert!(s.messages.is_empty());
    }

    #[test]
    fn syntax_and_unknown_keys_are_errors() {
        assert!(parse("[[workspaces]\nname = 1", None).is_err());
        assert!(parse("colour = \"red\"", None).is_err());
        assert!(parse("[[agents]]\nname = \"x\"\ncmd = \"y\"", None).is_err());
    }

    #[test]
    fn relative_project_path_is_skipped_with_message() {
        let text = "[[workspaces]]\nname = \"W\"\n[[workspaces.projects]]\nname = \"p\"\npath = \"castorterm\"";
        let s = parse(text, None).unwrap();
        assert!(s.workspaces[0].projects.is_empty());
        assert_eq!(s.messages.len(), 1);
        assert!(s.messages[0].contains("'p'"));
    }

    #[test]
    fn home_expansion() {
        let home = PathBuf::from(abs("/home/me", "C:/Users/me"));
        assert_eq!(expand_home("~", Some(&home)), Some(home.clone()));
        assert_eq!(expand_home("~/src", Some(&home)), Some(home.join("src")));
        assert_eq!(expand_home("~\\src", Some(&home)), Some(home.join("src")));
        assert_eq!(expand_home("~/src", None), None);
        assert_eq!(
            expand_home("~bob/src", Some(&home)),
            Some("~bob/src".into())
        );
        assert_eq!(expand_home("/abs", Some(&home)), Some("/abs".into()));
    }

    #[test]
    fn invalid_entries_are_reported() {
        let text = "[font]\nsize = 200\n[[agents]]\nname = \" \"\ncommand = \"x\"";
        let s = parse(text, None).unwrap();
        assert_eq!(s.font_size, None);
        assert!(s.agents.is_empty());
        assert_eq!(s.messages.len(), 2);
    }

    #[test]
    fn font_size_bounds() {
        for (size, ok) in [
            ("6.0", true),
            ("72.0", true),
            ("5.9", false),
            ("72.5", false),
            ("nan", false),
            ("inf", false),
        ] {
            let s = parse(&format!("[font]\nsize = {size}"), None).unwrap();
            assert_eq!(s.font_size.is_some(), ok, "size {size}");
            assert_eq!(s.messages.is_empty(), ok, "size {size}");
        }
    }

    #[test]
    fn control_characters_and_empty_names_are_rejected() {
        let text = r#"
            [[agents]]
            name = "Evil"
            command = "claude\nrm -rf ~"

            [[workspaces]]
            name = ""

            [[workspaces]]
            name = "W"
            [[workspaces.projects]]
            name = " "
            path = "/x"
        "#;
        let s = parse(text, None).unwrap();
        assert!(s.agents.is_empty());
        assert_eq!(s.workspaces.len(), 1);
        assert!(s.workspaces[0].projects.is_empty());
        assert_eq!(s.messages.len(), 3);
    }

    #[test]
    fn scanlines_default_on_and_can_be_disabled() {
        assert!(parse("", None).unwrap().scanlines);
        assert!(!parse("[ui]\nscanlines = false", None).unwrap().scanlines);
        assert!(parse("[ui]\nglow = true", None).is_err());
    }

    #[test]
    fn example_config_is_valid() {
        let home = PathBuf::from(abs("/home/me", "C:/Users/me"));
        let s = parse(include_str!("../config.example.toml"), Some(&home)).unwrap();
        assert!(s.messages.is_empty(), "{:?}", s.messages);
        assert_eq!(s.agents.len(), 2);
        assert_eq!(s.workspaces[0].projects.len(), 2);
        assert_eq!(
            s.workspaces[0].projects[0].path,
            home.join("src/castorterm")
        );
    }

    #[test]
    fn missing_file_is_silent_default() {
        let env = Env {
            config_override: Some(std::env::temp_dir().join("castorterm-does-not-exist.toml")),
            ..Env::default()
        };
        assert_eq!(load(&env), Settings::default());
    }
}
