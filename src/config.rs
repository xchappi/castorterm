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

/// Placeholder replaced by the task prompt in an agent's `task_args`.
pub const PROMPT_PLACEHOLDER: &str = "{prompt}";
/// Prompt used when a board does not define one.
pub const DEFAULT_PROMPT: &str = "Work on task {id}: {title}";
const DEFAULT_REFRESH_SECS: u64 = 60;
const MIN_REFRESH_SECS: u64 = 15;

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
    #[serde(default)]
    providers: Vec<RawProvider>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProvider {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBoard {
    provider: String,
    #[serde(default)]
    filter: String,
    prompt: Option<String>,
    refresh_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUi {
    scanlines: Option<bool>,
    restore_sessions: Option<bool>,
    /// Text size factor for the terminal and the interface.
    zoom: Option<f32>,
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
    task_args: Option<Vec<String>>,
    resume_args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspace {
    name: String,
    board: Option<RawBoard>,
    /// Folded in the sidebar.
    #[serde(default)]
    collapsed: bool,
    #[serde(default)]
    projects: Vec<RawProject>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    name: String,
    path: String,
    repo: Option<String>,
}

/// An agent launcher: a command typed into a fresh shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub name: String,
    pub command: String,
    /// Arguments that pass an initial prompt; contain [`PROMPT_PLACEHOLDER`].
    pub task_args: Option<Vec<String>>,
    /// Arguments added when a session is restored at startup, to continue
    /// the previous conversation (e.g. `["--continue"]`).
    pub resume_args: Option<Vec<String>>,
}

/// An external task provider (see `docs/providers.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provider {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Task board of a workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub provider: Provider,
    /// Opaque to castorterm; passed to the provider as `--filter`.
    pub filter: String,
    /// Prompt template with `{id}`, `{title}` and `{url}`.
    pub prompt: String,
    pub refresh_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    /// Repository identifier (e.g. `owner/repo`): passed to the board
    /// provider as `--project` and used to match tasks to this project.
    pub repo: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    pub board: Option<Board>,
    pub projects: Vec<Project>,
    /// Folded in the sidebar.
    pub collapsed: bool,
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
    /// Reopen the sessions of the last run at startup (on by default).
    pub restore_sessions: bool,
    /// Text size factor (`crate::theme::clamp_zoom`), 1.0 by default.
    pub zoom: f32,
    /// Human readable warnings and errors for the sidebar.
    pub messages: Vec<String>,
    /// Where workspace changes made from the UI are saved.
    pub config_path: Option<PathBuf>,
    /// Home directory, to expand `~` in project paths.
    pub home: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: None,
            agents: Vec::new(),
            workspaces: Vec::new(),
            scanlines: true,
            restore_sessions: true,
            zoom: 1.0,
            messages: Vec::new(),
            config_path: None,
            home: None,
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
        return Settings {
            home: env.home.clone(),
            ..Settings::default()
        };
    };
    let settings = match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text, env.home.as_deref()).unwrap_or_else(|err| Settings {
            messages: vec![format!("Config {}: {err}", path.display())],
            ..Settings::default()
        }),
        Err(err) if err.kind() == ErrorKind::NotFound => Settings::default(),
        Err(err) => Settings {
            messages: vec![format!("Could not read config {}: {err}", path.display())],
            ..Settings::default()
        },
    };
    Settings {
        config_path: Some(path),
        home: env.home.clone(),
        ..settings
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
        restore_sessions: raw.ui.restore_sessions.unwrap_or(true),
        zoom: crate::theme::clamp_zoom(raw.ui.zoom.unwrap_or(1.0)),
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
        let task_args = match agent.task_args {
            Some(args)
                if args.iter().any(|a| a.contains(PROMPT_PLACEHOLDER))
                    && args.iter().all(|a| !a.chars().any(char::is_control)) =>
            {
                Some(args)
            }
            Some(_) => {
                settings.messages.push(format!(
                    "Agent '{name}': task_args ignored, they must contain {PROMPT_PLACEHOLDER} and no control characters"
                ));
                None
            }
            None => None,
        };
        let resume_args = match agent.resume_args {
            Some(args) if args.is_empty() => None,
            Some(args) if args.iter().all(|a| valid_text(a.trim())) => {
                Some(args.iter().map(|a| a.trim().to_string()).collect())
            }
            Some(_) => {
                settings.messages.push(format!(
                    "Agent '{name}': resume_args ignored, they must be non-empty single lines"
                ));
                None
            }
            None => None,
        };
        settings.agents.push(Agent {
            name: name.into(),
            command: command.into(),
            task_args,
            resume_args,
        });
    }

    let mut providers: Vec<Provider> = Vec::new();
    for provider in raw.providers {
        let name = provider.name.trim();
        let command = provider.command.trim();
        let args_ok = provider
            .args
            .iter()
            .all(|a| !a.chars().any(char::is_control));
        if !valid_text(name) || !valid_text(command) || !args_ok {
            settings.messages.push(format!(
                "Provider '{}' skipped: name, command and args must be single lines",
                name.escape_debug()
            ));
            continue;
        }
        if providers.iter().any(|p| p.name == name) {
            settings.messages.push(format!(
                "Provider '{name}' defined twice: the first definition is used"
            ));
            continue;
        }
        providers.push(Provider {
            name: name.into(),
            command: command.into(),
            args: provider.args,
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
        // Names identify workspaces when the window saves changes.
        if settings
            .workspaces
            .iter()
            .any(|w| w.name.eq_ignore_ascii_case(workspace_name))
        {
            settings.messages.push(format!(
                "Workspace '{workspace_name}' skipped: another workspace has the same name"
            ));
            continue;
        }
        let board = workspace.board.and_then(|board| {
            parse_board(board, workspace_name, &providers, &mut settings.messages)
        });
        let mut projects = Vec::new();
        for project in workspace.projects {
            let name = project.name.trim();
            if !valid_text(name) {
                settings.messages.push(format!(
                    "Project in '{workspace_name}' skipped: name must be a non-empty single line"
                ));
                continue;
            }
            let repo = project.repo.map(|r| r.trim().to_string());
            let repo = match repo {
                Some(r) if !valid_repo(&r) => {
                    settings.messages.push(format!(
                        "Project '{name}': repo {r:?} ignored, use only letters, digits and . _ - / (not starting with -)"
                    ));
                    None
                }
                repo => repo,
            };
            match expand_home(project.path.trim(), home) {
                Some(path) if path.is_absolute() => projects.push(Project {
                    name: name.into(),
                    path,
                    repo,
                }),
                _ => settings.messages.push(format!(
                    "Project '{}' skipped: path '{}' must be absolute",
                    project.name, project.path
                )),
            }
        }
        settings.workspaces.push(Workspace {
            name: workspace_name.into(),
            board,
            projects,
            collapsed: workspace.collapsed,
        });
    }

    Ok(settings)
}

/// Validates a workspace board against the defined providers.
fn parse_board(
    raw: RawBoard,
    workspace: &str,
    providers: &[Provider],
    messages: &mut Vec<String>,
) -> Option<Board> {
    let Some(provider) = providers.iter().find(|p| p.name == raw.provider.trim()) else {
        messages.push(format!(
            "Workspace '{workspace}': board ignored, provider '{}' is not defined",
            raw.provider
        ));
        return None;
    };
    let prompt = raw
        .prompt
        .map(|p| p.trim().to_string())
        .unwrap_or_else(|| DEFAULT_PROMPT.to_string());
    if !valid_text(&prompt) || raw.filter.chars().any(char::is_control) {
        messages.push(format!(
            "Workspace '{workspace}': board ignored, prompt and filter must be single lines"
        ));
        return None;
    }
    let refresh_secs = match raw.refresh_secs {
        Some(secs) if secs < MIN_REFRESH_SECS => {
            messages.push(format!(
                "Workspace '{workspace}': refresh_secs raised to the minimum of {MIN_REFRESH_SECS}"
            ));
            MIN_REFRESH_SECS
        }
        Some(secs) => secs,
        None => DEFAULT_REFRESH_SECS,
    };
    Some(Board {
        provider: provider.clone(),
        filter: raw.filter.trim().to_string(),
        prompt,
        refresh_secs,
    })
}

/// A repository identifier is passed to providers as a `--project` value.
/// Allow list: ASCII letters, digits and `. _ - /`, not starting with `-`.
/// Nothing a shell or `cmd.exe` interprets (a `.cmd` provider re-parses its
/// arguments), and nothing a provider could take for a flag.
pub(crate) fn valid_repo(repo: &str) -> bool {
    !repo.is_empty()
        && !repo.starts_with('-')
        && repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

/// Non-empty and free of control characters: commands are typed into the
/// shell, so an embedded newline would run extra commands.
pub(crate) fn valid_text(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

/// Expands a leading `~` (alone or followed by a separator) to `home`.
/// Returns `None` when `~` is used but the home directory is unknown.
pub(crate) fn expand_home(path: &str, home: Option<&Path>) -> Option<PathBuf> {
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
                command: "claude".into(),
                task_args: None,
                resume_args: None,
            }]
        );
        assert_eq!(s.workspaces.len(), 1);
        assert_eq!(s.workspaces[0].name, "CastorLabs");
        assert_eq!(
            s.workspaces[0].projects,
            vec![Project {
                name: "castorterm".into(),
                path: PathBuf::from(path),
                repo: None,
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
    fn restore_and_resume_args() {
        let s = parse(
            "[ui]\nrestore_sessions = false\n[[agents]]\nname = \"C\"\ncommand = \"c\"\nresume_args = [\"--continue\"]\n[[agents]]\nname = \"D\"\ncommand = \"d\"\nresume_args = [\"a\\nb\"]\n",
            None,
        )
        .unwrap();
        assert!(!s.restore_sessions);
        assert_eq!(
            s.agents[0].resume_args.as_deref(),
            Some(&["--continue".to_string()][..])
        );
        assert_eq!(s.agents[1].resume_args, None);
        assert_eq!(s.messages.len(), 1, "{:?}", s.messages);
        assert!(parse("", None).unwrap().restore_sessions);
    }

    #[test]
    fn zoom_and_collapsed_are_read() {
        let s = parse(
            "[ui]\nzoom = 9.0\n[[workspaces]]\nname = \"W\"\ncollapsed = true\n",
            None,
        )
        .unwrap();
        assert_eq!(s.zoom, crate::theme::MAX_ZOOM);
        assert!(s.workspaces[0].collapsed);
        assert_eq!(parse("[ui]\nzoom = 1.26\n", None).unwrap().zoom, 1.3);
        assert_eq!(parse("", None).unwrap().zoom, 1.0);
        assert!(
            !parse("[[workspaces]]\nname = \"W\"\n", None)
                .unwrap()
                .workspaces[0]
                .collapsed
        );
    }

    #[test]
    fn scanlines_default_on_and_can_be_disabled() {
        assert!(parse("", None).unwrap().scanlines);
        assert!(!parse("[ui]\nscanlines = false", None).unwrap().scanlines);
        assert!(parse("[ui]\nglow = true", None).is_err());
    }

    #[test]
    fn boards_providers_and_task_args() {
        let text = r#"
            [[providers]]
            name = "tasks"
            command = "tasks-provider"
            args = ["--profile", "work"]

            [[agents]]
            name = "Claude Code"
            command = "claude"
            task_args = ["{prompt}"]

            [[agents]]
            name = "Bad"
            command = "bad"
            task_args = ["--no-placeholder"]

            [[workspaces]]
            name = "W"
            board = { provider = "tasks", filter = "repo:x/*", refresh_secs = 5 }
            [[workspaces.projects]]
            name = "p"
            path = "/p"
            repo = "x/p"
            [[workspaces.projects]]
            name = "flag"
            path = "/p"
            repo = "--all"
            [[workspaces.projects]]
            name = "spaced"
            path = "/p"
            repo = "x/a b"

            [[workspaces]]
            name = "V"
            board = { provider = "missing" }

            [[providers]]
            name = "tasks"
            command = "other"
        "#;
        let home = PathBuf::from(abs("/home/me", "C:/Users/me"));
        let text = text.replace("\"/p\"", &format!("{:?}", abs("/p", "C:/p")));
        let s = parse(&text, Some(&home)).unwrap();
        assert_eq!(
            s.agents[0].task_args.as_deref(),
            Some(&["{prompt}".to_string()][..])
        );
        assert_eq!(s.agents[1].task_args, None);
        let board = s.workspaces[0].board.as_ref().unwrap();
        assert_eq!(board.provider.command, "tasks-provider");
        assert_eq!(board.provider.args, ["--profile", "work"]);
        assert_eq!(board.filter, "repo:x/*");
        assert_eq!(board.prompt, DEFAULT_PROMPT);
        assert_eq!(board.refresh_secs, MIN_REFRESH_SECS);
        assert_eq!(s.workspaces[0].projects[0].repo.as_deref(), Some("x/p"));
        // Unsafe repos are dropped but the projects are kept.
        assert_eq!(s.workspaces[0].projects.len(), 3);
        assert_eq!(s.workspaces[0].projects[1].repo, None);
        assert_eq!(s.workspaces[0].projects[2].repo, None);
        assert_eq!(s.workspaces[1].board, None);
        // Bad task_args, duplicate provider, low refresh, two unsafe repos
        // and unknown provider are reported.
        assert_eq!(s.messages.len(), 6, "{:?}", s.messages);
    }

    #[test]
    fn repo_allow_list() {
        for ok in ["owner/repo", "a.b_c-d/E9", "CL"] {
            assert!(valid_repo(ok), "{ok}");
        }
        for bad in [
            "", "-x", "--all", "a b", "a\nb", "a&b", "a%PATH%", "a\"b", "a^b", "a!b", "a|b", "a<b",
            "a>b", "a;b", "a'b", "a(b)", "a`b", "a$b", "rëpo",
        ] {
            assert!(!valid_repo(bad), "{bad:?}");
        }
    }

    #[test]
    fn duplicate_workspace_names_are_skipped() {
        let s = parse(
            "[[workspaces]]\nname = \"W\"\n[[workspaces]]\nname = \"w\"\n",
            None,
        )
        .unwrap();
        assert_eq!(s.workspaces.len(), 1);
        assert_eq!(s.messages.len(), 1, "{:?}", s.messages);
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
        let path = std::env::temp_dir().join("castorterm-does-not-exist.toml");
        let env = Env {
            config_override: Some(path.clone()),
            ..Env::default()
        };
        // Defaults, remembering where UI changes will be saved.
        assert_eq!(
            load(&env),
            Settings {
                config_path: Some(path),
                ..Settings::default()
            }
        );
    }
}
