//! Whether the harnesses Shinatama starts draw with the terminal's own palette.
//!
//! The desktop's "follow skin" maps the terminal's default and 16 ANSI colours to
//! the skin, but a harness that paints with its built-in 24-bit theme never uses
//! them. While the terminals follow the skin, a real session therefore starts on
//! the harness's own theme made of the terminal's ANSI colours, chosen through
//! what the launcher already owns (instance settings, argv). No PTY byte is
//! rewritten and no file in a harness's home is changed, so a theme the user
//! chose inside a harness comes back when the terminals stop following.
//!
//! Claude Code and Codex have such a theme. Pi needs nothing: its default
//! `system` theme falls back to ANSI indices when the terminal's palette is not
//! reported, and under Windows' ConPTY no palette report (OSC 4/10/11) reaches a
//! program. OpenCode has none: its themes take hex colours only, and its `system`
//! theme is offered only once that report arrives.
//!
//! The desktop sets it from the saved appearance and again on every save; a
//! launcher reads it while composing a real session. Qualification trials never
//! read it, and the CLI never sets it, so both keep each harness's own theme.
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HarnessPalette {
    /// Each harness keeps the theme it would choose itself.
    #[default]
    Own,
    /// Harnesses draw with the terminal's palette, whose background is light or dark.
    Terminal { light: bool },
}
impl HarnessPalette {
    /// Claude Code's "ANSI colors only" theme for this background: the `theme`
    /// setting, whose unknown values Claude drops instead of refusing the file.
    pub fn claude_theme(self) -> Option<&'static str> {
        match self {
            Self::Own => None,
            Self::Terminal { light: false } => Some("dark-ansi"),
            Self::Terminal { light: true } => Some("light-ansi"),
        }
    }
    /// Codex already draws its interface with named ANSI colours; only code and
    /// diffs use a 24-bit syntax theme. `ansi` (`tui.theme`) encodes palette
    /// indices, and an unknown name only falls back to the default theme.
    pub fn codex_syntax_theme(self) -> Option<&'static str> {
        match self {
            Self::Own => None,
            Self::Terminal { .. } => Some("ansi"),
        }
    }
}

fn cell() -> &'static RwLock<HarnessPalette> {
    static CELL: OnceLock<RwLock<HarnessPalette>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(HarnessPalette::default()))
}
/// Replaces the palette for launches that start after this call.
pub fn set_current(palette: HarnessPalette) {
    *cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = palette;
}
pub fn current() -> HarnessPalette {
    *cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_palette_changes_no_harness() {
        let own = HarnessPalette::Own;
        assert_eq!(own.claude_theme(), None);
        assert_eq!(own.codex_syntax_theme(), None);
        assert_eq!(HarnessPalette::default(), own);
    }

    #[test]
    fn terminal_palette_picks_each_harness_theme_by_background() {
        let dark = HarnessPalette::Terminal { light: false };
        let light = HarnessPalette::Terminal { light: true };
        assert_eq!(dark.claude_theme(), Some("dark-ansi"));
        assert_eq!(light.claude_theme(), Some("light-ansi"));
        assert_eq!(dark.codex_syntax_theme(), Some("ansi"));
        assert_eq!(light.codex_syntax_theme(), Some("ansi"));
    }

    #[test]
    fn the_current_palette_is_replaced_whole() {
        set_current(HarnessPalette::Terminal { light: false });
        assert_eq!(current(), HarnessPalette::Terminal { light: false });
        set_current(HarnessPalette::Own);
        assert_eq!(current(), HarnessPalette::Own);
    }
}
