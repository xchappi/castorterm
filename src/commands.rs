//! Command palette model: the actions it offers and fuzzy matching.

/// An action the palette can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open launcher `launcher` in project `project`.
    Launch {
        project: usize,
        launcher: usize,
    },
    /// Switch to an open session.
    Activate(crate::tabs::SessionId),
    /// Add folders to a workspace (folder picker).
    AddFolder(usize),
    NewWorkspace,
    /// Rename an open session.
    RenameSession(crate::tabs::SessionId),
    /// Run project action `action` on `project`.
    RunOnProject {
        project: usize,
        action: usize,
    },
    /// Split the view or go back to one session.
    ToggleSplit,
    /// Search the active session's history.
    Find,
    ShowFiles,
    ShowQuests,
}

/// A palette entry: what is shown and what it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub title: String,
    /// Smaller text on the right (project, workspace...).
    pub detail: String,
    pub action: Action,
}

/// Most entries shown at once.
pub const MAX_SHOWN: usize = 12;

/// Score of `query` against `text`: `None` if the query letters do not all
/// appear in order (ignoring case); higher is better. Consecutive letters,
/// word starts and an early first match score more.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    // Greedy matching from each place the first letter appears, keeping the
    // best: "castor" should match the word "castorterm", not the `c` of an
    // earlier word followed by scattered letters.
    text.iter()
        .enumerate()
        .filter(|&(_, &c)| c == query[0])
        .filter_map(|(start, _)| score_from(&query, &text, start))
        .max()
}

/// Greedy score of `query` in `text` with its first letter at `start`.
fn score_from(query: &[char], text: &[char], start: usize) -> Option<i32> {
    let mut score = -(start.min(10) as i32);
    let mut next = 0;
    let mut previous: Option<usize> = None;
    for (index, &c) in text.iter().enumerate().skip(start) {
        if next == query.len() {
            break;
        }
        if c != query[next] {
            continue;
        }
        score += 1;
        if previous.is_some_and(|p| p + 1 == index) {
            score += 5;
        }
        if index == 0 || !text[index - 1].is_alphanumeric() {
            score += 3;
        }
        previous = Some(index);
        next += 1;
    }
    (next == query.len()).then_some(score)
}

/// Indices of `entries` matching `query` (title and detail), best first,
/// keeping the original order among equal scores; at most [`MAX_SHOWN`].
pub fn filter(entries: &[Entry], query: &str) -> Vec<usize> {
    let mut scored: Vec<(i32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let text = format!("{} {}", e.title, e.detail);
            score(query, &text).map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(MAX_SHOWN).map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, detail: &str) -> Entry {
        Entry {
            title: title.into(),
            detail: detail.into(),
            action: Action::ShowFiles,
        }
    }

    #[test]
    fn subsequence_matching() {
        assert!(score("cc", "Claude Code").is_some());
        assert!(score("CLAUDE", "claude code").is_some());
        assert!(score("xyz", "Claude Code").is_none());
        // Letters must appear in order.
        assert!(score("ec", "Claude Code").is_some());
        assert!(score("dlc", "Claude Code").is_none());
        assert_eq!(score("", "anything"), Some(0));
        assert_eq!(score("  ", "anything"), Some(0));
    }

    #[test]
    fn better_matches_rank_first() {
        // Consecutive letters at a word start beat scattered ones.
        assert!(score("cas", "castorterm").unwrap() > score("cas", "claude agents").unwrap());
        let entries = vec![
            entry("Open Shell", "landing-freddie"),
            entry("Open Claude Code", "castorterm"),
            entry("Open Shell", "castorterm"),
        ];
        assert_eq!(filter(&entries, "castor"), vec![1, 2]);
        assert_eq!(filter(&entries, "claude cast"), vec![1]);
        // Empty query keeps everything in order.
        assert_eq!(filter(&entries, ""), vec![0, 1, 2]);
    }

    #[test]
    fn at_most_max_shown() {
        let entries: Vec<Entry> = (0..MAX_SHOWN + 5)
            .map(|i| entry(&format!("e{i}"), ""))
            .collect();
        assert_eq!(filter(&entries, "e").len(), MAX_SHOWN);
    }
}
