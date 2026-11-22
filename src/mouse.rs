//! Mouse reporting: the bytes a terminal sends to a program that asked for
//! mouse events (DECSET 1000/1002/1003, SGR encoding 1006).
//!
//! Pure so the encodings are tested without GPUI.

/// Which button an event is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    /// Motion with no button held (mode 1003).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Press,
    Release,
    Motion,
}

/// Modifier keys held during the event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseMods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// What the program asked to receive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reporting {
    /// Presses, releases and wheel (1000).
    pub clicks: bool,
    /// Plus motion while a button is held (1002).
    pub drag: bool,
    /// Plus all motion (1003).
    pub motion: bool,
    /// SGR encoding (1006) instead of the legacy X10 bytes.
    pub sgr: bool,
}

impl Reporting {
    /// Whether the program wants any mouse events.
    pub fn active(self) -> bool {
        self.clicks || self.drag || self.motion
    }

    /// Whether an event of `kind` with `button` should be sent.
    pub fn wants(self, kind: Kind, button: Button) -> bool {
        match kind {
            Kind::Press | Kind::Release => self.active(),
            Kind::Motion if button == Button::None => self.motion,
            Kind::Motion => self.drag || self.motion,
        }
    }
}

/// Encodes one event at cell `col`, `row` (0-based, on the visible screen).
/// `None` when the event cannot be encoded (X10 beyond column/row 223, or a
/// wheel release, which does not exist).
pub fn encode(
    button: Button,
    kind: Kind,
    col: usize,
    row: usize,
    mods: MouseMods,
    sgr: bool,
) -> Option<Vec<u8>> {
    let is_wheel = matches!(button, Button::WheelUp | Button::WheelDown);
    if is_wheel && kind == Kind::Release {
        return None;
    }
    let mut code: u32 = match button {
        Button::Left => 0,
        Button::Middle => 1,
        Button::Right => 2,
        Button::None => 3,
        Button::WheelUp => 64,
        Button::WheelDown => 65,
    };
    if mods.shift {
        code += 4;
    }
    if mods.alt {
        code += 8;
    }
    if mods.ctrl {
        code += 16;
    }
    if kind == Kind::Motion {
        code += 32;
    }
    if sgr {
        let end = if kind == Kind::Release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{};{}{end}", col + 1, row + 1).into_bytes());
    }
    // X10: release has no button of its own, coordinates are one byte.
    if kind == Kind::Release {
        code = (code & !0b11) | 3;
    }
    let byte = |n: usize| u8::try_from(n + 33).ok();
    Some(vec![
        0x1b,
        b'[',
        b'M',
        u8::try_from(code + 32).ok()?,
        byte(col)?,
        byte(row)?,
    ])
}

/// What a click on a menu drawn by a program that does not read the mouse
/// (Claude Code's prompts) turns into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuClick {
    /// Move the highlight this many options (negative: up).
    Move(i32),
    /// The highlighted option was clicked: choose it.
    Choose,
}

/// Whether a click may be read as a menu choice: only in sessions that
/// run an agent, on the live screen (not scrolled back, not a full-screen
/// program), with the cursor hidden as menus have it and shells never do,
/// and in the pane that already had the keyboard (a click that brings a
/// pane forward only does that).
pub fn menu_clicks_allowed(
    agent: bool,
    live: bool,
    alt_screen: bool,
    cursor_shown: bool,
    focused: bool,
) -> bool {
    agent && live && !alt_screen && !cursor_shown && focused
}

/// Marker of the highlighted option.
const MENU_MARKER: char = '\u{276F}';

/// The menu option under `row` in the visible `screen`, if the click lands
/// on a menu: the bottom-most line with the marker plus the lines right
/// above and below whose text starts at the same column after blank space.
/// At least two options are needed, so a lone marker (a shell prompt) is
/// not a menu.
pub fn menu_click(screen: &[String], row: usize) -> Option<MenuClick> {
    let chars: Vec<Vec<char>> = screen.iter().map(|l| l.chars().collect()).collect();
    let (marker_row, text_col) = chars.iter().enumerate().rev().find_map(|(r, line)| {
        let at = line.iter().position(|c| !c.is_whitespace())?;
        if line[at] != MENU_MARKER {
            return None;
        }
        let text = at + 1 + line[at + 1..].iter().position(|c| !c.is_whitespace())?;
        Some((r, text))
    })?;
    let is_option = |r: usize| {
        chars.get(r).is_some_and(|line| {
            line.len() > text_col
                && line[..text_col].iter().all(|c| c.is_whitespace())
                && !line[text_col].is_whitespace()
        })
    };
    let mut first = marker_row;
    while first > 0 && is_option(first - 1) {
        first -= 1;
    }
    let mut last = marker_row;
    while is_option(last + 1) {
        last += 1;
    }
    if last == first || row < first || row > last {
        return None;
    }
    // A text box between rules (Claude Code's input, where lines continue
    // under the `❯`) is not a menu: a click must never submit it.
    let is_rule = |r: Option<usize>| {
        r.and_then(|r| chars.get(r)).is_some_and(|line| {
            let mut drawn = line.iter().filter(|c| !c.is_whitespace()).peekable();
            drawn.peek().is_some() && drawn.all(|c| matches!(c, '\u{2500}' | '\u{2501}' | '-'))
        })
    };
    if is_rule(first.checked_sub(1)) || is_rule(Some(last + 1)) {
        return None;
    }
    Some(match row as i32 - marker_row as i32 {
        0 => MenuClick::Choose,
        delta => MenuClick::Move(delta),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_MODS: MouseMods = MouseMods {
        shift: false,
        alt: false,
        ctrl: false,
    };

    #[test]
    fn sgr_encoding() {
        let e = |b, k, m| String::from_utf8(encode(b, k, 4, 9, m, true).unwrap()).unwrap();
        assert_eq!(e(Button::Left, Kind::Press, NO_MODS), "\x1b[<0;5;10M");
        assert_eq!(e(Button::Left, Kind::Release, NO_MODS), "\x1b[<0;5;10m");
        assert_eq!(e(Button::Right, Kind::Press, NO_MODS), "\x1b[<2;5;10M");
        assert_eq!(e(Button::Left, Kind::Motion, NO_MODS), "\x1b[<32;5;10M");
        assert_eq!(e(Button::None, Kind::Motion, NO_MODS), "\x1b[<35;5;10M");
        assert_eq!(e(Button::WheelUp, Kind::Press, NO_MODS), "\x1b[<64;5;10M");
        assert_eq!(e(Button::WheelDown, Kind::Press, NO_MODS), "\x1b[<65;5;10M");
        let ctrl_alt = MouseMods {
            shift: false,
            alt: true,
            ctrl: true,
        };
        assert_eq!(e(Button::Left, Kind::Press, ctrl_alt), "\x1b[<24;5;10M");
        // Far cells are fine in SGR.
        assert!(encode(Button::Left, Kind::Press, 500, 300, NO_MODS, true).is_some());
        assert_eq!(
            encode(Button::WheelUp, Kind::Release, 0, 0, NO_MODS, true),
            None
        );
    }

    #[test]
    fn x10_encoding() {
        assert_eq!(
            encode(Button::Left, Kind::Press, 0, 0, NO_MODS, false),
            Some(b"\x1b[M !!".to_vec())
        );
        // Release reports button 3.
        assert_eq!(
            encode(Button::Right, Kind::Release, 1, 2, NO_MODS, false),
            Some(b"\x1b[M#\"#".to_vec())
        );
        // One byte per coordinate: too far is not sent.
        assert!(encode(Button::Left, Kind::Press, 222, 0, NO_MODS, false).is_some());
        assert_eq!(
            encode(Button::Left, Kind::Press, 223, 0, NO_MODS, false),
            None
        );
        // Motion with the middle button, release keeps the modifiers.
        assert_eq!(
            encode(Button::Middle, Kind::Motion, 0, 0, NO_MODS, false),
            Some(b"\x1b[MA!!".to_vec())
        );
        let shift = MouseMods {
            shift: true,
            ..NO_MODS
        };
        assert_eq!(
            encode(Button::Left, Kind::Release, 0, 0, shift, false),
            Some(b"\x1b[M'!!".to_vec())
        );
        assert_eq!(
            String::from_utf8(encode(Button::WheelDown, Kind::Press, 0, 0, shift, true).unwrap())
                .unwrap(),
            "\x1b[<69;1;1M"
        );
    }

    fn screen(text: &str) -> Vec<String> {
        text.lines().map(String::from).collect()
    }

    #[test]
    fn menu_clicks() {
        let s = screen(
            "Claude Code'll be able to read, edit, and execute files here.\n\
             \n\
             \u{276F} No, exit\n  Yes, I trust this folder\n\
             \n\
             Enter to confirm \u{b7} Esc to cancel",
        );
        assert_eq!(menu_click(&s, 2), Some(MenuClick::Choose));
        assert_eq!(menu_click(&s, 3), Some(MenuClick::Move(1)));
        assert_eq!(menu_click(&s, 0), None);
        assert_eq!(menu_click(&s, 5), None);
        // Numbered options, marker in the middle.
        let s = screen("   1. Yes\n \u{276F} 2. Always\n   3. No\nfoot");
        assert_eq!(menu_click(&s, 0), Some(MenuClick::Move(-1)));
        assert_eq!(menu_click(&s, 2), Some(MenuClick::Move(1)));
        assert_eq!(menu_click(&s, 3), None);
    }

    #[test]
    fn menu_gate() {
        assert!(menu_clicks_allowed(true, true, false, false, true));
        assert!(!menu_clicks_allowed(false, true, false, false, true));
        assert!(!menu_clicks_allowed(true, false, false, false, true));
        assert!(!menu_clicks_allowed(true, true, true, false, true));
        assert!(!menu_clicks_allowed(true, true, false, true, true));
        assert!(!menu_clicks_allowed(true, true, false, false, false));
    }

    #[test]
    fn input_boxes_are_not_menus() {
        // Claude Code's prompt box with a second line: between rules.
        let s = screen(
            "\u{2500}\u{2500}\u{2500}\u{2500}\n\u{276F} fix the bug\n  in view.rs\n\u{2500}\u{2500}\u{2500}\u{2500}",
        );
        assert_eq!(menu_click(&s, 1), None);
        assert_eq!(menu_click(&s, 2), None);
    }

    #[test]
    fn prompts_are_not_menus() {
        // A shell prompt with the same marker and output below it.
        let s = screen("\u{276F} cargo test\nrunning 3 tests\nok");
        assert_eq!(menu_click(&s, 0), None);
        assert_eq!(menu_click(&s, 1), None);
        // Only the bottom-most marker counts.
        let s = screen("\u{276F} old\n  menu\n\n\u{276F} cmd");
        assert_eq!(menu_click(&s, 0), None);
        assert_eq!(menu_click(&[], 0), None);
    }

    #[test]
    fn what_each_mode_wants() {
        let clicks = Reporting {
            clicks: true,
            ..Reporting::default()
        };
        assert!(clicks.wants(Kind::Press, Button::Left));
        assert!(!clicks.wants(Kind::Motion, Button::Left));
        let drag = Reporting {
            drag: true,
            ..Reporting::default()
        };
        assert!(drag.wants(Kind::Motion, Button::Left));
        assert!(!drag.wants(Kind::Motion, Button::None));
        let motion = Reporting {
            motion: true,
            ..Reporting::default()
        };
        assert!(motion.wants(Kind::Motion, Button::None));
        assert!(!Reporting::default().active());
    }
}
