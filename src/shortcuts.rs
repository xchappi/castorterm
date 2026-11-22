//! Application shortcuts, handled before keys reach the terminal.

use crate::keys::Mods;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    NewShell,
    CloseSession,
    NextSession,
    PrevSession,
    ToggleFiles,
    /// Add folders to the selected workspace.
    AddFolder,
    /// Open the command palette.
    Palette,
    /// Go to the next session whose agent waits for the user.
    NextWaiting,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Move the active session one place left / right.
    MoveSessionLeft,
    MoveSessionRight,
}

impl Shortcut {
    /// Whether a held key may repeat the shortcut. Opening or closing
    /// sessions must not repeat: holding Ctrl+Shift+W would close them all.
    pub fn repeats(self) -> bool {
        matches!(
            self,
            Shortcut::NextSession
                | Shortcut::PrevSession
                | Shortcut::MoveSessionLeft
                | Shortcut::MoveSessionRight
        )
    }
}

/// Maps a GPUI key name plus modifiers to a shortcut. Chosen to avoid keys
/// that shells and TUIs use: Ctrl+Shift+T/W/E/O/P/J and Ctrl+(Shift+)Tab.
pub fn match_shortcut(key: &str, mods: Mods) -> Option<Shortcut> {
    if !mods.ctrl || mods.alt {
        return None;
    }
    let key = key.to_ascii_lowercase();
    // Zoom like Windows Terminal and browsers: Ctrl+= (or +), Ctrl+-, Ctrl+0.
    match key.as_str() {
        "=" | "+" => return Some(Shortcut::ZoomIn),
        "-" => return Some(Shortcut::ZoomOut),
        "0" if !mods.shift => return Some(Shortcut::ZoomReset),
        _ => {}
    }
    match (key.as_str(), mods.shift) {
        ("t", true) => Some(Shortcut::NewShell),
        ("w", true) => Some(Shortcut::CloseSession),
        ("e", true) => Some(Shortcut::ToggleFiles),
        ("o", true) => Some(Shortcut::AddFolder),
        ("p", true) => Some(Shortcut::Palette),
        ("j", true) => Some(Shortcut::NextWaiting),
        ("tab", false) => Some(Shortcut::NextSession),
        ("tab", true) => Some(Shortcut::PrevSession),
        ("pageup", true) => Some(Shortcut::MoveSessionLeft),
        ("pagedown", true) => Some(Shortcut::MoveSessionRight),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE_MODS: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: true,
    };

    #[test]
    fn shortcuts() {
        assert_eq!(match_shortcut("t", CTRL_SHIFT), Some(Shortcut::NewShell));
        assert_eq!(match_shortcut("T", CTRL_SHIFT), Some(Shortcut::NewShell));
        assert_eq!(match_shortcut("e", CTRL_SHIFT), Some(Shortcut::ToggleFiles));
        assert_eq!(match_shortcut("e", CTRL), None);
        assert_eq!(match_shortcut("o", CTRL_SHIFT), Some(Shortcut::AddFolder));
        assert_eq!(match_shortcut("o", CTRL), None);
        assert_eq!(match_shortcut("p", CTRL_SHIFT), Some(Shortcut::Palette));
        assert_eq!(match_shortcut("p", CTRL), None);
        assert_eq!(match_shortcut("j", CTRL_SHIFT), Some(Shortcut::NextWaiting));
        assert_eq!(match_shortcut("=", CTRL), Some(Shortcut::ZoomIn));
        assert_eq!(match_shortcut("+", CTRL_SHIFT), Some(Shortcut::ZoomIn));
        assert_eq!(match_shortcut("-", CTRL), Some(Shortcut::ZoomOut));
        assert_eq!(match_shortcut("0", CTRL), Some(Shortcut::ZoomReset));
        assert_eq!(match_shortcut("0", CTRL_SHIFT), None);
        assert_eq!(match_shortcut("=", NONE_MODS), None);
        assert_eq!(
            match_shortcut("w", CTRL_SHIFT),
            Some(Shortcut::CloseSession)
        );
        assert_eq!(match_shortcut("tab", CTRL), Some(Shortcut::NextSession));
        assert_eq!(
            match_shortcut("pageup", CTRL_SHIFT),
            Some(Shortcut::MoveSessionLeft)
        );
        assert_eq!(
            match_shortcut("pagedown", CTRL_SHIFT),
            Some(Shortcut::MoveSessionRight)
        );
        assert_eq!(match_shortcut("pageup", CTRL), None);
        assert_eq!(
            match_shortcut("tab", CTRL_SHIFT),
            Some(Shortcut::PrevSession)
        );
    }

    #[test]
    fn only_navigation_repeats() {
        assert!(!Shortcut::NewShell.repeats());
        assert!(!Shortcut::CloseSession.repeats());
        assert!(!Shortcut::ToggleFiles.repeats());
        assert!(Shortcut::NextSession.repeats());
        assert!(Shortcut::PrevSession.repeats());
    }

    #[test]
    fn terminal_keys_are_not_shortcuts() {
        // Ctrl+T / Ctrl+W are used by shells (transpose, delete word).
        assert_eq!(match_shortcut("t", CTRL), None);
        assert_eq!(match_shortcut("w", CTRL), None);
        assert_eq!(match_shortcut("tab", Mods::default()), None);
        let altgr = Mods {
            ctrl: true,
            alt: true,
            shift: true,
        };
        assert_eq!(match_shortcut("t", altgr), None);
    }
}
