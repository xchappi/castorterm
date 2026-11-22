//! Ordered list of open sessions and which one is active.
//!
//! Generic over the session handle so the logic is tested without PTYs or GPUI.

/// Stable identifier of an open session.
pub type SessionId = u64;

pub struct Tab<T> {
    pub id: SessionId,
    /// Index of the project in the project tree.
    pub project: usize,
    pub label: String,
    /// Whether the user named it (saved and restored with the session).
    pub renamed: bool,
    pub handle: T,
}

pub struct Tabs<T> {
    tabs: Vec<Tab<T>>,
    active: Option<usize>,
    next_id: SessionId,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_id: 1,
        }
    }
}

impl<T> Tabs<T> {
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab<T>> {
        self.tabs.iter()
    }

    pub fn active(&self) -> Option<&Tab<T>> {
        self.active.map(|i| &self.tabs[i])
    }

    pub fn active_id(&self) -> Option<SessionId> {
        self.active().map(|t| t.id)
    }

    /// First session whose handle matches `pred`.
    pub fn find(&self, pred: impl Fn(&T) -> bool) -> Option<SessionId> {
        self.tabs.iter().find(|t| pred(&t.handle)).map(|t| t.id)
    }

    /// Label for a new session in `project`: `base`, or `base N` with the
    /// smallest N >= 2 that is not taken in that project.
    pub fn unique_label(&self, project: usize, base: &str) -> String {
        let taken = |label: &str| {
            self.tabs
                .iter()
                .any(|t| t.project == project && t.label == label)
        };
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|label| !taken(label))
            .expect("unbounded range always yields a free label")
    }

    /// Appends a session with a unique label and makes it active.
    pub fn open(&mut self, project: usize, base_label: &str, handle: T) -> SessionId {
        let id = self.next_id;
        self.next_id += 1;
        let label = self.unique_label(project, base_label);
        self.tabs.push(Tab {
            id,
            project,
            label,
            renamed: false,
            handle,
        });
        self.active = Some(self.tabs.len() - 1);
        id
    }

    /// Removes a session. If it was active, the one to its right (or left,
    /// if it was the last) becomes active.
    pub fn close(&mut self, id: SessionId) -> Option<T> {
        let index = self.tabs.iter().position(|t| t.id == id)?;
        let tab = self.tabs.remove(index);
        self.active = match self.active {
            _ if self.tabs.is_empty() => None,
            Some(a) if a == index => Some(index.min(self.tabs.len() - 1)),
            Some(a) if a > index => Some(a - 1),
            other => other,
        };
        Some(tab.handle)
    }

    /// Makes `id` active; returns false if it does not exist.
    pub fn activate(&mut self, id: SessionId) -> bool {
        match self.tabs.iter().position(|t| t.id == id) {
            Some(index) => {
                self.active = Some(index);
                true
            }
            None => false,
        }
    }

    /// Activates the next session, wrapping around.
    pub fn next(&mut self) {
        let len = self.tabs.len();
        if let Some(a) = self.active {
            self.active = Some((a + 1) % len);
        }
    }

    /// Activates the previous session, wrapping around.
    pub fn prev(&mut self) {
        let len = self.tabs.len();
        if let Some(a) = self.active {
            self.active = Some((a + len - 1) % len);
        }
    }

    /// Renames a session. The label is trimmed, single-line and at most
    /// [`MAX_LABEL_CHARS`] characters; returns false if it is not usable.
    pub fn rename(&mut self, id: SessionId, label: &str) -> bool {
        let label = label.trim();
        let ok = !label.is_empty()
            && label.chars().count() <= MAX_LABEL_CHARS
            && !label.chars().any(crate::board::is_hidden);
        match self.tabs.iter_mut().find(|t| t.id == id) {
            Some(tab) if ok => {
                tab.label = label.to_string();
                tab.renamed = true;
                true
            }
            _ => false,
        }
    }

    /// Moves a session to position `index` (clamped); the active session
    /// stays active.
    pub fn move_to(&mut self, id: SessionId, index: usize) -> bool {
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            return false;
        };
        let active_id = self.active_id();
        let tab = self.tabs.remove(from);
        let to = index.min(self.tabs.len());
        self.tabs.insert(to, tab);
        self.active = active_id.and_then(|a| self.tabs.iter().position(|t| t.id == a));
        from != to
    }

    /// Moves a session `delta` places left (negative) or right, clamped.
    pub fn move_by(&mut self, id: SessionId, delta: isize) -> bool {
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            return false;
        };
        let to = from.saturating_add_signed(delta).min(self.tabs.len() - 1);
        self.move_to(id, to)
    }
}

/// Longest session name accepted when renaming.
pub const MAX_LABEL_CHARS: usize = 40;

#[cfg(test)]
mod tests {
    use super::*;

    fn abc() -> (Tabs<()>, [SessionId; 3]) {
        let mut t = Tabs::default();
        let a = t.open(0, "A", ());
        let b = t.open(0, "B", ());
        let c = t.open(0, "C", ());
        (t, [a, b, c])
    }

    fn order(t: &Tabs<()>) -> Vec<String> {
        t.iter().map(|x| x.label.clone()).collect()
    }

    #[test]
    fn rename_validates() {
        let (mut t, [a, ..]) = abc();
        assert!(t.rename(a, "  Reviewer  "));
        assert_eq!(order(&t)[0], "Reviewer");
        assert!(t.iter().next().unwrap().renamed);
        assert!(!t.iter().nth(1).unwrap().renamed);
        assert!(!t.rename(a, "   "));
        assert!(!t.rename(a, "a\nb"));
        assert!(!t.rename(a, "a\u{202E}b"));
        assert!(!t.rename(a, &"x".repeat(MAX_LABEL_CHARS + 1)));
        assert!(!t.rename(999, "x"));
        assert_eq!(order(&t)[0], "Reviewer");
    }

    #[test]
    fn move_keeps_the_active_session() {
        let (mut t, [a, b, c]) = abc();
        t.activate(b);
        assert!(t.move_to(a, 2));
        assert_eq!(order(&t), ["B", "C", "A"]);
        assert_eq!(t.active_id(), Some(b));
        assert!(t.move_by(c, -1));
        assert_eq!(order(&t), ["C", "B", "A"]);
        assert_eq!(t.active_id(), Some(b));
        // Clamped at the ends.
        assert!(!t.move_by(c, -5));
        assert!(t.move_to(c, 99));
        assert_eq!(order(&t), ["B", "A", "C"]);
        assert!(!t.move_to(999, 0));
    }

    fn three() -> (Tabs<&'static str>, [SessionId; 3]) {
        let mut tabs = Tabs::default();
        let a = tabs.open(0, "Shell", "a");
        let b = tabs.open(0, "Shell", "b");
        let c = tabs.open(1, "Shell", "c");
        (tabs, [a, b, c])
    }

    #[test]
    fn empty_tabs() {
        let mut tabs: Tabs<()> = Tabs::default();
        assert!(tabs.is_empty());
        assert!(tabs.active().is_none());
        tabs.next();
        tabs.prev();
        assert!(tabs.close(1).is_none());
        assert!(!tabs.activate(1));
    }

    #[test]
    fn open_activates_and_labels_per_project() {
        let (tabs, [_, b, c]) = three();
        assert_eq!(tabs.active_id(), Some(c));
        let labels: Vec<_> = tabs.iter().map(|t| t.label.as_str()).collect();
        // Second shell of project 0 gets a suffix; project 1 starts fresh.
        assert_eq!(labels, ["Shell", "Shell 2", "Shell"]);
        assert_eq!(tabs.find(|h| *h == "b"), Some(b));
        assert_eq!(tabs.find(|h| *h == "zzz"), None);
    }

    #[test]
    fn freed_label_is_reused() {
        let (mut tabs, [a, ..]) = three();
        tabs.close(a);
        assert_eq!(tabs.unique_label(0, "Shell"), "Shell");
        assert_eq!(tabs.unique_label(0, "OpenCode"), "OpenCode");
    }

    #[test]
    fn next_and_prev_wrap() {
        let (mut tabs, [a, b, c]) = three();
        tabs.next();
        assert_eq!(tabs.active_id(), Some(a));
        tabs.prev();
        assert_eq!(tabs.active_id(), Some(c));
        tabs.prev();
        assert_eq!(tabs.active_id(), Some(b));
    }

    #[test]
    fn close_active_middle_activates_right() {
        let (mut tabs, [_, b, c]) = three();
        tabs.activate(b);
        assert_eq!(tabs.close(b), Some("b"));
        assert_eq!(tabs.active_id(), Some(c));
    }

    #[test]
    fn close_active_last_activates_left() {
        let (mut tabs, [_, b, c]) = three();
        tabs.close(c);
        assert_eq!(tabs.active_id(), Some(b));
    }

    #[test]
    fn close_before_active_keeps_active() {
        let (mut tabs, [a, _, c]) = three();
        tabs.close(a);
        assert_eq!(tabs.active_id(), Some(c));
    }

    #[test]
    fn close_after_active_keeps_active() {
        let (mut tabs, [a, b, _]) = three();
        tabs.activate(a);
        tabs.close(b);
        assert_eq!(tabs.active_id(), Some(a));
    }

    #[test]
    fn close_all() {
        let (mut tabs, ids) = three();
        for id in ids {
            assert!(tabs.close(id).is_some());
        }
        assert!(tabs.is_empty());
        assert!(tabs.active().is_none());
    }

    #[test]
    fn ids_are_not_reused() {
        let (mut tabs, [.., c]) = three();
        tabs.close(c);
        let d = tabs.open(1, "Shell", "d");
        assert!(d > c);
    }
}
