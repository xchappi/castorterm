//! Encoding of non-text keys into terminal byte sequences.
//!
//! Printable text is not handled here: it reaches the PTY through the
//! platform text input path (input handler), which also covers dead keys,
//! AltGr and IME commits.

/// Modifier state relevant for encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Returns the bytes for a control/navigation key, or `None` if the key
/// should be left to the text input path.
///
/// `key` uses GPUI key names (`"enter"`, `"up"`, `"f5"`, `"c"`...).
/// `app_cursor` is the terminal DECCKM mode (application cursor keys).
pub fn encode_key(key: &str, mods: Mods, app_cursor: bool) -> Option<Vec<u8>> {
    // Ctrl+Alt is AltGr on Windows layouts: let the text path handle it.
    if mods.ctrl && mods.alt {
        return None;
    }

    let seq: &[u8] = match key {
        "enter" => b"\r",
        "backspace" if mods.ctrl => b"\x08",
        "backspace" => b"\x7f",
        "tab" if mods.shift => b"\x1b[Z",
        "tab" => b"\t",
        "escape" => b"\x1b",
        "up" => cursor_key(0, app_cursor),
        "down" => cursor_key(1, app_cursor),
        "right" => cursor_key(2, app_cursor),
        "left" => cursor_key(3, app_cursor),
        "home" => cursor_key(4, app_cursor),
        "end" => cursor_key(5, app_cursor),
        "insert" => b"\x1b[2~",
        "delete" => b"\x1b[3~",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        "f1" => b"\x1bOP",
        "f2" => b"\x1bOQ",
        "f3" => b"\x1bOR",
        "f4" => b"\x1bOS",
        "f5" => b"\x1b[15~",
        "f6" => b"\x1b[17~",
        "f7" => b"\x1b[18~",
        "f8" => b"\x1b[19~",
        "f9" => b"\x1b[20~",
        "f10" => b"\x1b[21~",
        "f11" => b"\x1b[23~",
        "f12" => b"\x1b[24~",
        "space" if mods.ctrl => b"\x00",
        _ => return ctrl_char(key, mods),
    };
    Some(seq.to_vec())
}

/// Cursor keys: up, down, right, left, home, end (by index).
fn cursor_key(index: usize, app_cursor: bool) -> &'static [u8] {
    const NORMAL: [&[u8]; 6] = [
        b"\x1b[A", b"\x1b[B", b"\x1b[C", b"\x1b[D", b"\x1b[H", b"\x1b[F",
    ];
    const APP: [&[u8]; 6] = [
        b"\x1bOA", b"\x1bOB", b"\x1bOC", b"\x1bOD", b"\x1bOH", b"\x1bOF",
    ];
    if app_cursor {
        APP[index]
    } else {
        NORMAL[index]
    }
}

/// Ctrl+letter and a few Ctrl+symbol combinations (C0 control codes).
fn ctrl_char(key: &str, mods: Mods) -> Option<Vec<u8>> {
    if !mods.ctrl {
        return None;
    }
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() || !c.is_ascii() {
        return None;
    }
    let byte = match c.to_ascii_lowercase() {
        l @ 'a'..='z' => (l as u8) & 0x1f,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '@' | '2' => 0x00,
        '/' | '-' => 0x1f,
        _ => return None,
    };
    Some(vec![byte])
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

    #[test]
    fn enter_backspace_escape() {
        assert_eq!(encode_key("enter", NONE, false), Some(b"\r".to_vec()));
        assert_eq!(encode_key("backspace", NONE, false), Some(vec![0x7f]));
        assert_eq!(encode_key("escape", NONE, false), Some(vec![0x1b]));
    }

    #[test]
    fn tab_and_shift_tab() {
        assert_eq!(encode_key("tab", NONE, false), Some(b"\t".to_vec()));
        let shift = Mods {
            shift: true,
            ..NONE
        };
        assert_eq!(encode_key("tab", shift, false), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn arrows_respect_app_cursor_mode() {
        assert_eq!(encode_key("up", NONE, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode_key("up", NONE, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(encode_key("left", NONE, false), Some(b"\x1b[D".to_vec()));
        assert_eq!(encode_key("end", NONE, true), Some(b"\x1bOF".to_vec()));
    }

    #[test]
    fn ctrl_letters_map_to_c0() {
        assert_eq!(encode_key("c", CTRL, false), Some(vec![0x03]));
        assert_eq!(encode_key("C", CTRL, false), Some(vec![0x03]));
        assert_eq!(encode_key("d", CTRL, false), Some(vec![0x04]));
        assert_eq!(encode_key("[", CTRL, false), Some(vec![0x1b]));
        assert_eq!(encode_key("space", CTRL, false), Some(vec![0x00]));
    }

    #[test]
    fn printable_keys_are_left_to_text_input() {
        assert_eq!(encode_key("a", NONE, false), None);
        assert_eq!(encode_key("space", NONE, false), None);
        assert_eq!(encode_key("\u{f1}", CTRL, false), None);
    }

    #[test]
    fn altgr_is_left_to_text_input() {
        let altgr = Mods {
            ctrl: true,
            alt: true,
            shift: false,
        };
        assert_eq!(encode_key("2", altgr, false), None);
        assert_eq!(encode_key("e", altgr, false), None);
    }

    #[test]
    fn function_keys() {
        assert_eq!(encode_key("f1", NONE, false), Some(b"\x1bOP".to_vec()));
        assert_eq!(encode_key("f12", NONE, false), Some(b"\x1b[24~".to_vec()));
        assert_eq!(encode_key("f13", NONE, false), None);
    }
}
