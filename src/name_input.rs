//! Minimal single-line text field for names (workspaces), driven by key
//! events. GPUI has no built-in text input and names need no IME.

use crate::keys::Mods;
use crate::tree::MAX_WORKSPACE_NAME;

/// What a key did to the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputAction {
    Commit,
    Cancel,
    Changed,
    /// Paste request (Ctrl+V): the caller reads the clipboard.
    Paste,
    Ignored,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameInput {
    pub text: String,
}

impl NameInput {
    pub fn new(text: &str) -> Self {
        let mut input = Self::default();
        input.insert(text);
        input
    }

    /// Handles a key: GPUI key name, the text it produces and modifiers.
    pub fn handle(&mut self, key: &str, key_char: Option<&str>, mods: Mods) -> InputAction {
        match key {
            "enter" => return InputAction::Commit,
            "escape" => return InputAction::Cancel,
            "backspace" => {
                if mods.ctrl {
                    // Delete the last word.
                    let trimmed = self.text.trim_end().len();
                    let cut = self.text[..trimmed]
                        .rfind(char::is_whitespace)
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    self.text.truncate(cut);
                } else {
                    self.text.pop();
                }
                return InputAction::Changed;
            }
            _ => {}
        }
        // Ctrl without Alt is a shortcut; Ctrl+Alt is AltGr on Windows.
        if mods.ctrl && !mods.alt {
            return if key.eq_ignore_ascii_case("v") {
                InputAction::Paste
            } else {
                InputAction::Ignored
            };
        }
        // GPUI gives the space bar no text on Windows.
        let text = key_char.or((key == "space").then_some(" "));
        match text {
            Some(text) if self.insert(text) => InputAction::Changed,
            _ => InputAction::Ignored,
        }
    }

    /// Appends `text` without control characters, up to the length limit.
    /// Returns whether anything was added.
    pub fn insert(&mut self, text: &str) -> bool {
        let mut count = self.text.chars().count();
        let mut added = false;
        for c in text.chars() {
            if count >= MAX_WORKSPACE_NAME {
                break;
            }
            if c.is_control() {
                continue;
            }
            self.text.push(c);
            count += 1;
            added = true;
        }
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
    };
    const ALT_GR: Mods = Mods {
        ctrl: true,
        alt: true,
        shift: false,
    };

    #[test]
    fn typing_and_editing() {
        let mut input = NameInput::default();
        for c in ["F", "r", "e", "e", " ", "l"] {
            assert_eq!(
                input.handle(&c.to_lowercase(), Some(c), NONE),
                InputAction::Changed
            );
        }
        assert_eq!(input.text, "Free l");
        assert_eq!(input.handle("backspace", None, NONE), InputAction::Changed);
        assert_eq!(input.text, "Free ");
        assert_eq!(input.handle("backspace", None, CTRL), InputAction::Changed);
        assert_eq!(input.text, "");
        // AltGr produces characters such as @ on Spanish keyboards.
        assert_eq!(input.handle("2", Some("@"), ALT_GR), InputAction::Changed);
        assert_eq!(input.text, "@");
        // Space without text (Windows).
        assert_eq!(input.handle("space", None, NONE), InputAction::Changed);
        assert_eq!(input.text, "@ ");
    }

    #[test]
    fn control_keys() {
        let mut input = NameInput::new("x");
        assert_eq!(input.handle("enter", None, NONE), InputAction::Commit);
        assert_eq!(input.handle("escape", None, NONE), InputAction::Cancel);
        assert_eq!(input.handle("v", Some("v"), CTRL), InputAction::Paste);
        assert_eq!(input.handle("t", Some("t"), CTRL), InputAction::Ignored);
        assert_eq!(input.handle("tab", Some("\t"), NONE), InputAction::Ignored);
        assert_eq!(input.text, "x");
    }

    #[test]
    fn insert_filters_and_caps() {
        let mut input = NameInput::default();
        assert!(input.insert("a\nb\tc"));
        assert_eq!(input.text, "abc");
        input.insert(&"x".repeat(MAX_WORKSPACE_NAME * 2));
        assert_eq!(input.text.chars().count(), MAX_WORKSPACE_NAME);
        assert!(!input.insert("more"));
    }
}
