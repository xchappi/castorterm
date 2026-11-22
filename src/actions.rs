//! Project actions: buttons that open an agent session on a project with a
//! prompt (index the project, suggest improvements...).
//!
//! The agent does the work. By default the actions keep everything in the
//! repository (`docs/project/`), so nothing leaves the machine; a config
//! (for example a company's plugin) can replace them with its own prompts,
//! such as indexing into a wiki.

use crate::tree::Launcher;

/// A configured project action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectAction {
    pub name: String,
    /// Prompt template: `{project}`, `{path}` and `{repo}` are filled in.
    pub prompt: String,
    /// Agent (launcher name) to run it; the first agent that takes a prompt
    /// when not set.
    pub agent: Option<String>,
}

/// Longest action name shown on a button.
pub const MAX_NAME_CHARS: usize = 32;

const INDEX_PROMPT: &str = "Index the project {project} for future work. Explore the repository (README, manifests, source layout, configuration, CI, existing docs and recent git history) and write or update these Markdown files in docs/project/: overview.md (what it is, who it is for, current status), stack.md (languages, frameworks, key dependencies and versions), architecture.md (modules, data flow, entry points, external services), development.md (how to set up, run, test, build and deploy) and state.md (what works, work in progress, known issues, technical debt). Be factual, cite file paths, keep each file short, update existing content instead of duplicating it, and do not change any other file.";

const IMPROVE_PROMPT: &str = "Review the project {project} and suggest improvements. Read docs/project/ first if it exists. Write docs/project/improvements.md with prioritized suggestions (impact, effort and the files involved) covering bugs, security, performance, tests, developer experience and product ideas. Do not change any code.";

/// The actions castorterm offers when the config defines none: everything
/// stays in the repository.
pub fn defaults() -> Vec<ProjectAction> {
    vec![
        ProjectAction {
            name: "Index project".into(),
            prompt: INDEX_PROMPT.into(),
            agent: None,
        },
        ProjectAction {
            name: "Suggest improvements".into(),
            prompt: IMPROVE_PROMPT.into(),
            agent: None,
        },
    ]
}

/// The prompt of `action` for a project.
pub fn render(template: &str, project: &str, path: &str, repo: Option<&str>) -> String {
    template
        .replace("{project}", project)
        .replace("{path}", path)
        .replace("{repo}", repo.unwrap_or(project))
}

/// The launcher that runs `action`: the named agent if it takes a prompt,
/// otherwise the first agent that does. `None` when no agent can.
pub fn pick_launcher(action: &ProjectAction, launchers: &[Launcher]) -> Option<usize> {
    let takes_prompt = |l: &Launcher| l.command.is_some() && l.task_args.is_some();
    if let Some(name) = &action.agent {
        return launchers
            .iter()
            .position(|l| l.name.eq_ignore_ascii_case(name) && takes_prompt(l));
    }
    launchers.iter().position(takes_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher(name: &str, command: Option<&str>, args: bool) -> Launcher {
        Launcher {
            name: name.into(),
            command: command.map(String::from),
            task_args: args.then(|| vec!["{prompt}".into()]),
            resume_args: None,
        }
    }

    #[test]
    fn defaults_stay_local() {
        let d = defaults();
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|a| a.prompt.contains("docs/project/")));
        assert!(d.iter().all(|a| a.name.chars().count() <= MAX_NAME_CHARS));
    }

    #[test]
    fn prompts_are_filled_in() {
        assert_eq!(
            render(
                "{project} at {path} ({repo})",
                "app",
                "/p/app",
                Some("me/app")
            ),
            "app at /p/app (me/app)"
        );
        assert_eq!(render("{repo}", "app", "/p", None), "app");
    }

    #[test]
    fn launcher_choice() {
        let ls = vec![
            launcher("Shell", None, false),
            launcher("OpenCode", Some("opencode"), false),
            launcher("Claude Code", Some("claude"), true),
        ];
        let mut action = defaults().remove(0);
        assert_eq!(pick_launcher(&action, &ls), Some(2));
        action.agent = Some("claude code".into());
        assert_eq!(pick_launcher(&action, &ls), Some(2));
        // A named agent that cannot take a prompt: none (no silent switch).
        action.agent = Some("OpenCode".into());
        assert_eq!(pick_launcher(&action, &ls), None);
        action.agent = None;
        assert_eq!(pick_launcher(&action, &ls[..2]), None);
    }
}
