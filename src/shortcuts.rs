//! Application shortcuts, handled before keys reach the terminal.

use crate::keys::Mods;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    NewShell,
    CloseSession,
    NextSession,
    PrevSession,
}

/// Maps a GPUI key name plus modifiers to a shortcut. Chosen to avoid keys
/// that shells and TUIs use: Ctrl+Shift+T/W and Ctrl+(Shift+)Tab.
pub fn match_shortcut(key: &str, mods: Mods) -> Option<Shortcut> {
    if !mods.ctrl || mods.alt {
        return None;
    }
    let key = key.to_ascii_lowercase();
    match (key.as_str(), mods.shift) {
        ("t", true) => Some(Shortcut::NewShell),
        ("w", true) => Some(Shortcut::CloseSession),
        ("tab", false) => Some(Shortcut::NextSession),
        ("tab", true) => Some(Shortcut::PrevSession),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            match_shortcut("w", CTRL_SHIFT),
            Some(Shortcut::CloseSession)
        );
        assert_eq!(match_shortcut("tab", CTRL), Some(Shortcut::NextSession));
        assert_eq!(
            match_shortcut("tab", CTRL_SHIFT),
            Some(Shortcut::PrevSession)
        );
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
