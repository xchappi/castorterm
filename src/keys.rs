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

impl Mods {
    /// xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
    fn xterm_param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }
}

/// Terminal and platform state that changes how keys are encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyMode {
    /// DECCKM: application cursor keys.
    pub app_cursor: bool,
    /// Alt+key sends ESC+key (Windows/Linux). On macOS Option composes
    /// characters instead, so this is off there.
    pub alt_sends_escape: bool,
}

/// Returns the bytes for a control/navigation key, or `None` if the key
/// should be left to the text input path.
///
/// `key` uses GPUI key names (`"enter"`, `"up"`, `"f5"`, `"c"`...).
pub fn encode_key(key: &str, mods: Mods, mode: KeyMode) -> Option<Vec<u8>> {
    if let Some(seq) = navigation_key(key, mods, mode.app_cursor) {
        return Some(seq);
    }

    let simple: Option<&[u8]> = match key {
        "enter" => Some(b"\r"),
        "backspace" if mods.ctrl => Some(b"\x08"),
        "backspace" => Some(b"\x7f"),
        "tab" if mods.shift => Some(b"\x1b[Z"),
        "tab" => Some(b"\t"),
        "escape" => Some(b"\x1b"),
        "space" if mods.ctrl && !mods.alt => Some(b"\x00"),
        _ => None,
    };
    if let Some(seq) = simple {
        let mut out = Vec::with_capacity(seq.len() + 1);
        if mods.alt && mode.alt_sends_escape && key != "escape" {
            out.push(0x1b);
        }
        out.extend_from_slice(seq);
        return Some(out);
    }

    // Ctrl+Alt is AltGr on Windows layouts: let the text path handle it.
    if mods.ctrl && mods.alt {
        return None;
    }
    if mods.ctrl {
        return ctrl_char(key).map(|b| vec![b]);
    }
    if mods.alt && mode.alt_sends_escape {
        return alt_char(key, mods.shift);
    }
    None
}

/// Cursor, editing and function keys, with xterm modifier parameters.
fn navigation_key(key: &str, mods: Mods, app_cursor: bool) -> Option<Vec<u8>> {
    let param = mods.xterm_param();

    // Keys of the form CSI <final> / SS3 <final>.
    let letter = match key {
        "up" => Some(b'A'),
        "down" => Some(b'B'),
        "right" => Some(b'C'),
        "left" => Some(b'D'),
        "home" => Some(b'H'),
        "end" => Some(b'F'),
        _ => None,
    };
    if let Some(letter) = letter {
        return Some(if param > 1 {
            format!("\x1b[1;{param}{}", letter as char).into_bytes()
        } else if app_cursor {
            vec![0x1b, b'O', letter]
        } else {
            vec![0x1b, b'[', letter]
        });
    }

    // F1-F4 use SS3 without modifiers, CSI 1;m with them.
    let pf = match key {
        "f1" => Some(b'P'),
        "f2" => Some(b'Q'),
        "f3" => Some(b'R'),
        "f4" => Some(b'S'),
        _ => None,
    };
    if let Some(letter) = pf {
        return Some(if param > 1 {
            format!("\x1b[1;{param}{}", letter as char).into_bytes()
        } else {
            vec![0x1b, b'O', letter]
        });
    }

    // Keys of the form CSI <code> ~ / CSI <code> ; <m> ~.
    let code = match key {
        "insert" => 2,
        "delete" => 3,
        "pageup" => 5,
        "pagedown" => 6,
        "f5" => 15,
        "f6" => 17,
        "f7" => 18,
        "f8" => 19,
        "f9" => 20,
        "f10" => 21,
        "f11" => 23,
        "f12" => 24,
        _ => return None,
    };
    Some(if param > 1 {
        format!("\x1b[{code};{param}~").into_bytes()
    } else {
        format!("\x1b[{code}~").into_bytes()
    })
}

/// Ctrl+letter and a few Ctrl+symbol combinations (C0 control codes).
fn ctrl_char(key: &str) -> Option<u8> {
    let c = single_ascii(key)?;
    match c.to_ascii_lowercase() {
        l @ 'a'..='z' => Some((l as u8) & 0x1f),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '@' | '2' => Some(0x00),
        '/' | '-' => Some(0x1f),
        _ => None,
    }
}

/// Alt+key as ESC followed by the character (readline "meta").
fn alt_char(key: &str, shift: bool) -> Option<Vec<u8>> {
    let c = single_ascii(key)?;
    if c.is_ascii_control() {
        return None;
    }
    let c = if shift { c.to_ascii_uppercase() } else { c };
    Some(vec![0x1b, c as u8])
}

fn single_ascii(key: &str) -> Option<char> {
    let mut chars = key.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c.is_ascii()).then_some(c)
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
    const ALT: Mods = Mods {
        ctrl: false,
        alt: true,
        shift: false,
    };
    const NORMAL: KeyMode = KeyMode {
        app_cursor: false,
        alt_sends_escape: true,
    };
    const APP: KeyMode = KeyMode {
        app_cursor: true,
        alt_sends_escape: true,
    };

    fn enc(key: &str, mods: Mods, mode: KeyMode) -> Option<Vec<u8>> {
        encode_key(key, mods, mode)
    }

    #[test]
    fn enter_backspace_escape() {
        assert_eq!(enc("enter", NONE, NORMAL), Some(b"\r".to_vec()));
        assert_eq!(enc("backspace", NONE, NORMAL), Some(vec![0x7f]));
        assert_eq!(enc("backspace", CTRL, NORMAL), Some(vec![0x08]));
        assert_eq!(enc("escape", NONE, NORMAL), Some(vec![0x1b]));
    }

    #[test]
    fn tab_and_shift_tab() {
        assert_eq!(enc("tab", NONE, NORMAL), Some(b"\t".to_vec()));
        let shift = Mods {
            shift: true,
            ..NONE
        };
        assert_eq!(enc("tab", shift, NORMAL), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn arrows_respect_app_cursor_mode() {
        assert_eq!(enc("up", NONE, NORMAL), Some(b"\x1b[A".to_vec()));
        assert_eq!(enc("up", NONE, APP), Some(b"\x1bOA".to_vec()));
        assert_eq!(enc("left", NONE, NORMAL), Some(b"\x1b[D".to_vec()));
        assert_eq!(enc("end", NONE, APP), Some(b"\x1bOF".to_vec()));
    }

    #[test]
    fn modified_navigation_keys_use_xterm_params() {
        assert_eq!(enc("left", CTRL, NORMAL), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(enc("right", CTRL, APP), Some(b"\x1b[1;5C".to_vec()));
        let shift = Mods {
            shift: true,
            ..NONE
        };
        assert_eq!(enc("up", shift, NORMAL), Some(b"\x1b[1;2A".to_vec()));
        assert_eq!(enc("delete", CTRL, NORMAL), Some(b"\x1b[3;5~".to_vec()));
        assert_eq!(enc("f1", ALT, NORMAL), Some(b"\x1b[1;3P".to_vec()));
        assert_eq!(enc("f5", shift, NORMAL), Some(b"\x1b[15;2~".to_vec()));
    }

    #[test]
    fn ctrl_letters_map_to_c0() {
        assert_eq!(enc("c", CTRL, NORMAL), Some(vec![0x03]));
        assert_eq!(enc("C", CTRL, NORMAL), Some(vec![0x03]));
        assert_eq!(enc("d", CTRL, NORMAL), Some(vec![0x04]));
        assert_eq!(enc("[", CTRL, NORMAL), Some(vec![0x1b]));
        assert_eq!(enc("space", CTRL, NORMAL), Some(vec![0x00]));
    }

    #[test]
    fn alt_letters_send_escape_prefix() {
        assert_eq!(enc("b", ALT, NORMAL), Some(b"\x1bb".to_vec()));
        let alt_shift = Mods { shift: true, ..ALT };
        assert_eq!(enc("b", alt_shift, NORMAL), Some(b"\x1bB".to_vec()));
        assert_eq!(enc(".", ALT, NORMAL), Some(b"\x1b.".to_vec()));
        assert_eq!(enc("enter", ALT, NORMAL), Some(b"\x1b\r".to_vec()));
    }

    #[test]
    fn alt_is_left_to_text_input_when_option_composes() {
        let mac = KeyMode {
            app_cursor: false,
            alt_sends_escape: false,
        };
        assert_eq!(enc("b", ALT, mac), None);
    }

    #[test]
    fn printable_keys_are_left_to_text_input() {
        assert_eq!(enc("a", NONE, NORMAL), None);
        assert_eq!(enc("space", NONE, NORMAL), None);
        assert_eq!(enc("\u{f1}", CTRL, NORMAL), None);
    }

    #[test]
    fn altgr_is_left_to_text_input() {
        let altgr = Mods {
            ctrl: true,
            alt: true,
            shift: false,
        };
        assert_eq!(enc("2", altgr, NORMAL), None);
        assert_eq!(enc("e", altgr, NORMAL), None);
    }

    #[test]
    fn function_keys() {
        assert_eq!(enc("f1", NONE, NORMAL), Some(b"\x1bOP".to_vec()));
        assert_eq!(enc("f12", NONE, NORMAL), Some(b"\x1b[24~".to_vec()));
        assert_eq!(enc("f13", NONE, NORMAL), None);
    }
}
