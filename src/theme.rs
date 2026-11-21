//! "Neon HUD" look of the window chrome: colours, bundled fonts and the
//! colour of each launcher ("character class"). The terminal colours live in
//! `palette.rs`.

/// Readable, game-HUD style font for the chrome (SIL Open Font License).
pub const UI_FONT: &str = "Chakra Petch";
/// Pixel font, only for the logo (SIL Open Font License).
pub const LOGO_FONT: &str = "Press Start 2P";

/// Fonts registered at startup.
pub const BUNDLED_FONTS: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/ChakraPetch-Regular.ttf"),
    include_bytes!("../assets/fonts/ChakraPetch-SemiBold.ttf"),
    include_bytes!("../assets/fonts/PressStart2P-Regular.ttf"),
];

pub const TEXT_SIZE: f32 = 13.0;
pub const SMALL_TEXT_SIZE: f32 = 11.0;
pub const TITLE_TEXT_SIZE: f32 = 20.0;
/// The pixel font is drawn on an 8 px grid: keep it a multiple of 8.
pub const LOGO_TEXT_SIZE: f32 = 16.0;

// Chrome colours (0xRRGGBB).
pub const HUD_BG: u32 = 0x0b0a1a;
pub const PANEL_BG: u32 = 0x110f24;
pub const BORDER: u32 = 0x2a2550;
pub const TEXT: u32 = 0xecebff;
pub const MUTED: u32 = 0x8d88b8;
pub const TITLE: u32 = 0xff4fb0;
pub const ACCENT: u32 = 0x2de2e6;
pub const WORKSPACE: u32 = 0xffd319;
pub const ERROR: u32 = 0xff5577;

/// Class colours for launchers: the shell first, then one per agent.
const CLASS_COLORS: [u32; 6] = [
    0x39ff8e, // shell: green
    0xff9f1c, // orange
    0x2de2e6, // cyan
    0xff4fb0, // pink
    0xffd319, // yellow
    0x6e9bff, // blue
];

/// Colour of the launcher at `index` (see `tree::launchers`), cycling when
/// there are more agents than colours.
pub fn launcher_color(index: usize) -> u32 {
    CLASS_COLORS[index % CLASS_COLORS.len()]
}

/// 0xRRGGBB plus an alpha byte, as 0xRRGGBBAA for `gpui::rgba`.
pub fn with_alpha(color: u32, alpha: u8) -> u32 {
    ((color & 0xff_ffff) << 8) | alpha as u32
}

/// Section headers are upper case.
pub fn label(text: &str) -> String {
    text.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_is_green_and_colors_cycle() {
        assert_eq!(launcher_color(0), 0x39ff8e);
        assert_ne!(launcher_color(1), launcher_color(0));
        assert_eq!(launcher_color(CLASS_COLORS.len()), launcher_color(0));
        assert_eq!(launcher_color(CLASS_COLORS.len() + 1), launcher_color(1));
    }

    #[test]
    fn alpha_is_appended() {
        assert_eq!(with_alpha(0x112233, 0x44), 0x1122_3344);
        // Stray high bits are ignored.
        assert_eq!(with_alpha(0xff11_2233, 0xff), 0x1122_33ff);
    }

    #[test]
    fn labels_are_upper_case() {
        assert_eq!(label("Claude Code"), "CLAUDE CODE");
    }

    #[test]
    fn fonts_are_bundled_truetype() {
        for font in BUNDLED_FONTS {
            // TrueType files start with version 1.0 (0x00010000).
            assert_eq!(&font[..4], &[0, 1, 0, 0]);
        }
    }
}
