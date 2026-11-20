//! Color theme and resolution of terminal colors to RGB.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// 8-bit RGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn hex(v: u32) -> Self {
        Self {
            r: ((v >> 16) & 0xff) as u8,
            g: ((v >> 8) & 0xff) as u8,
            b: (v & 0xff) as u8,
        }
    }

    /// Scales the color towards black (used for DIM).
    pub fn dim(self) -> Self {
        let f = |c: u8| ((c as u16 * 2) / 3) as u8;
        Self {
            r: f(self.r),
            g: f(self.g),
            b: f(self.b),
        }
    }
}

impl From<Rgb> for Rgb8 {
    fn from(c: Rgb) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

/// Color theme.
pub struct Theme {
    pub foreground: Rgb8,
    pub background: Rgb8,
    pub cursor: Rgb8,
    pub ansi: [Rgb8; 16],
}

/// Default dark theme.
pub const DEFAULT_THEME: Theme = Theme {
    foreground: Rgb8::hex(0xd4d4d4),
    background: Rgb8::hex(0x1e1e1e),
    cursor: Rgb8::hex(0xaeafad),
    ansi: [
        Rgb8::hex(0x000000),
        Rgb8::hex(0xcd3131),
        Rgb8::hex(0x0dbc79),
        Rgb8::hex(0xe5e510),
        Rgb8::hex(0x2472c8),
        Rgb8::hex(0xbc3fbc),
        Rgb8::hex(0x11a8cd),
        Rgb8::hex(0xe5e5e5),
        Rgb8::hex(0x666666),
        Rgb8::hex(0xf14c4c),
        Rgb8::hex(0x23d18b),
        Rgb8::hex(0xf5f543),
        Rgb8::hex(0x3b8eea),
        Rgb8::hex(0xd670d6),
        Rgb8::hex(0x29b8db),
        Rgb8::hex(0xe5e5e5),
    ],
};

impl Theme {
    /// RGB for an xterm 256-color index (0-15 come from the theme).
    pub fn indexed(&self, index: u8) -> Rgb8 {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
                let i = index - 16;
                Rgb8 {
                    r: LEVELS[(i / 36) as usize],
                    g: LEVELS[((i / 6) % 6) as usize],
                    b: LEVELS[(i % 6) as usize],
                }
            }
            232..=255 => {
                let v = 8 + 10 * (index - 232);
                Rgb8 { r: v, g: v, b: v }
            }
        }
    }

    /// Theme color for a named color, ignoring OSC overrides.
    fn named(&self, named: NamedColor) -> Rgb8 {
        let idx = named as usize;
        match named {
            NamedColor::Foreground | NamedColor::BrightForeground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            NamedColor::DimForeground => self.foreground.dim(),
            _ if idx < 16 => self.ansi[idx],
            // DimBlack..DimWhite map onto the 8 normal colors.
            _ => self.ansi[(idx - NamedColor::DimBlack as usize) % 8].dim(),
        }
    }

    /// Resolves a cell color, honouring OSC 4/10/11 overrides in `colors`.
    pub fn resolve(&self, color: Color, colors: &Colors) -> Rgb8 {
        match color {
            Color::Spec(rgb) => rgb.into(),
            Color::Indexed(i) => colors[i as usize]
                .map(Rgb8::from)
                .unwrap_or_else(|| self.indexed(i)),
            Color::Named(named) => colors[named]
                .map(Rgb8::from)
                .unwrap_or_else(|| self.named(named)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_cube_corners() {
        let t = &DEFAULT_THEME;
        assert_eq!(t.indexed(16), Rgb8::hex(0x000000));
        assert_eq!(t.indexed(196), Rgb8::hex(0xff0000));
        assert_eq!(t.indexed(231), Rgb8::hex(0xffffff));
    }

    #[test]
    fn grayscale_ramp() {
        let t = &DEFAULT_THEME;
        assert_eq!(t.indexed(232), Rgb8::hex(0x080808));
        assert_eq!(t.indexed(255), Rgb8::hex(0xeeeeee));
    }

    #[test]
    fn named_and_truecolor_resolution() {
        let t = &DEFAULT_THEME;
        let colors = Colors::default();
        assert_eq!(t.resolve(Color::Named(NamedColor::Red), &colors), t.ansi[1]);
        assert_eq!(
            t.resolve(Color::Named(NamedColor::Background), &colors),
            t.background
        );
        let spec = Color::Spec(Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(t.resolve(spec, &colors), Rgb8 { r: 1, g: 2, b: 3 });
    }

    #[test]
    fn osc_override_wins() {
        let t = &DEFAULT_THEME;
        let mut colors = Colors::default();
        colors[NamedColor::Foreground] = Some(Rgb { r: 9, g: 9, b: 9 });
        assert_eq!(
            t.resolve(Color::Named(NamedColor::Foreground), &colors),
            Rgb8 { r: 9, g: 9, b: 9 }
        );
    }

    #[test]
    fn dim_named_colors() {
        let t = &DEFAULT_THEME;
        let colors = Colors::default();
        assert_eq!(
            t.resolve(Color::Named(NamedColor::DimRed), &colors),
            t.ansi[1].dim()
        );
    }
}
