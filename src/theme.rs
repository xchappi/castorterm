//! "Neon HUD" look of the window chrome: colours, bundled fonts and the
//! colour of each launcher ("character class"). The terminal colours live in
//! `palette.rs`.

/// Readable, game-HUD style font for the chrome (SIL Open Font License).
pub const UI_FONT: &str = "Chakra Petch";
/// Pixel font, only for the logo (SIL Open Font License).
pub const LOGO_FONT: &str = "Press Start 2P";

/// Fonts registered at startup, with the family name each one declares.
pub const BUNDLED_FONTS: [(&str, &[u8]); 3] = [
    (
        UI_FONT,
        include_bytes!("../assets/fonts/ChakraPetch-Regular.ttf"),
    ),
    (
        UI_FONT,
        include_bytes!("../assets/fonts/ChakraPetch-SemiBold.ttf"),
    ),
    (
        LOGO_FONT,
        include_bytes!("../assets/fonts/PressStart2P-Regular.ttf"),
    ),
];

/// Text size factor applied to the terminal and the interface (Ctrl+= /
/// Ctrl+- / Ctrl+0). One window per process, so a global is enough.
static ZOOM_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);
pub const MIN_ZOOM: f32 = 0.7;
pub const MAX_ZOOM: f32 = 2.5;
pub const ZOOM_STEP: f32 = 0.1;

/// Zoom kept in range and on 0.1 steps (non-finite values reset it).
pub fn clamp_zoom(zoom: f32) -> f32 {
    if !zoom.is_finite() {
        return 1.0;
    }
    ((zoom * 10.0).round() / 10.0).clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn zoom() -> f32 {
    f32::from_bits(ZOOM_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

/// Sets the zoom and returns the value applied.
pub fn set_zoom(zoom: f32) -> f32 {
    let zoom = clamp_zoom(zoom);
    ZOOM_BITS.store(zoom.to_bits(), std::sync::atomic::Ordering::Relaxed);
    zoom
}

/// An interface size scaled by the zoom.
pub fn ui(size: f32) -> gpui::Pixels {
    gpui::px((size * zoom()).round())
}

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

/// Colour of a file in the explorer by type, like item rarity in a game.
pub fn file_color(name: &str) -> u32 {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    match ext {
        "rs" => 0xff9f1c,
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => 0xffd319,
        "py" | "go" | "java" | "kt" | "c" | "h" | "cpp" | "hpp" | "cs" | "rb" | "php" | "swift"
        | "sh" | "ps1" | "sql" | "html" | "css" | "scss" | "vue" | "svelte" | "astro" => 0x6e9bff,
        "md" | "mdx" | "txt" | "rst" | "adoc" => 0x2de2e6,
        "toml" | "yaml" | "yml" | "json" | "jsonc" | "lock" | "ini" | "env" | "xml" => 0x39ff8e,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "webp" | "ttf" | "otf" | "woff"
        | "woff2" | "mp3" | "mp4" | "wav" => 0xff4fb0,
        _ => MUTED,
    }
}

/// Colour of a task priority chip: P0 red, P1 orange, P2 yellow, else muted.
pub fn priority_color(priority: &str) -> u32 {
    match priority.trim().to_ascii_uppercase().as_str() {
        "P0" => ERROR,
        "P1" => 0xff9f1c,
        "P2" => WORKSPACE,
        _ => MUTED,
    }
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
    #[test]
    fn zoom_is_clamped_and_stepped() {
        assert_eq!(super::clamp_zoom(1.04), 1.0);
        assert_eq!(super::clamp_zoom(1.26), 1.3);
        assert_eq!(super::clamp_zoom(9.0), super::MAX_ZOOM);
        assert_eq!(super::clamp_zoom(0.1), super::MIN_ZOOM);
        assert_eq!(super::clamp_zoom(f32::NAN), 1.0);
    }

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
    fn file_colors_by_category() {
        assert_eq!(file_color("main.rs"), 0xff9f1c);
        assert_eq!(file_color("Cargo.toml"), 0x39ff8e);
        assert_eq!(file_color("README.MD"), 0x2de2e6);
        assert_eq!(file_color("app.tsx"), 0xffd319);
        assert_eq!(file_color("logo.svg"), 0xff4fb0);
        assert_eq!(file_color("LICENSE"), MUTED);
        assert_eq!(file_color(".gitignore"), MUTED);
    }

    #[test]
    fn priority_colors() {
        assert_eq!(priority_color("P0"), ERROR);
        assert_eq!(priority_color(" p1 "), 0xff9f1c);
        assert_eq!(priority_color("P2"), WORKSPACE);
        assert_eq!(priority_color("low"), MUTED);
    }

    #[test]
    fn labels_are_upper_case() {
        assert_eq!(label("Claude Code"), "CLAUDE CODE");
    }

    #[test]
    fn fonts_are_bundled_truetype() {
        for (_, font) in BUNDLED_FONTS {
            // TrueType files start with version 1.0 (0x00010000).
            assert_eq!(&font[..4], &[0, 1, 0, 0]);
        }
    }

    #[test]
    fn fonts_declare_the_family_names_used_in_code() {
        // The `name` table stores Windows names as UTF-16BE; a wrong family
        // constant would silently fall back to the system font.
        for (family, font) in BUNDLED_FONTS {
            let needle: Vec<u8> = family.encode_utf16().flat_map(u16::to_be_bytes).collect();
            assert!(
                font.windows(needle.len()).any(|w| w == needle.as_slice()),
                "{family} not found in its font file"
            );
        }
    }
}
