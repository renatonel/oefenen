//! The single place where colours and styles are defined.
//!
//! Screens and modules never write a colour of their own; they compose these
//! styles. The palette is deliberately high contrast: bright colours on black,
//! large type and generous spacing, aimed at young children.
//!
//! Terminals that support 24-bit colour show these exact colours; older
//! terminals fall back to the nearest basic colour, which keeps the contrast.

use ratatui::style::{Color, Modifier, Style};

/// Dark blue background; easier on the eyes than pure black.
pub const BACKGROUND: Color = Color::Rgb(12, 14, 24);
/// Panel background, slightly lighter than [`BACKGROUND`].
pub const SURFACE: Color = Color::Rgb(24, 28, 44);
/// Border of an ordinary panel.
pub const BORDER: Color = Color::Rgb(90, 102, 140);
/// Border and background accent of the focused element.
pub const ACCENT: Color = Color::Rgb(255, 209, 102);
/// Main text.
pub const TEXT: Color = Color::Rgb(240, 244, 255);
/// Secondary text.
pub const TEXT_MUTED: Color = Color::Rgb(168, 180, 208);
/// Positive feedback.
pub const SUCCESS: Color = Color::Rgb(126, 231, 135);
/// Negative feedback.
pub const ERROR: Color = Color::Rgb(255, 122, 122);
/// Selection highlight.
pub const HIGHLIGHT_BG: Color = Color::Rgb(44, 56, 90);
/// Timer digits.
pub const TIMER: Color = Color::Rgb(129, 214, 255);

/// Base style for the whole frame.
pub fn base() -> Style {
    Style::default().bg(BACKGROUND).fg(TEXT)
}

/// Style for ordinary body text.
pub fn body() -> Style {
    base()
}

/// Style for secondary text (hints, subtitles).
pub fn muted() -> Style {
    base().fg(TEXT_MUTED)
}

/// Style for labels such as "Tijd".
pub fn label() -> Style {
    base().fg(TEXT_MUTED).add_modifier(Modifier::BOLD)
}

/// Style for a question: large and bold.
pub fn question() -> Style {
    base()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Style for what the learner types.
pub fn input() -> Style {
    base()
        .fg(TEXT)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
}

/// Style for the running timer.
pub fn timer() -> Style {
    base().fg(TIMER).add_modifier(Modifier::BOLD)
}

/// Style for a panel: background and border.
pub fn panel() -> Style {
    base().bg(SURFACE).fg(BORDER)
}

/// Style for a title.
pub fn title() -> Style {
    base()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Style for a subject heading in the module list.
pub fn heading() -> Style {
    base()
        .fg(TEXT_MUTED)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Style of the selected list item.
pub fn selected() -> Style {
    base()
        .bg(HIGHLIGHT_BG)
        .fg(TEXT)
        .add_modifier(Modifier::BOLD)
}

/// Style of a list item that is not selected.
pub fn unselected() -> Style {
    base()
}

/// Style of a disabled list item.
pub fn disabled() -> Style {
    base().fg(TEXT_MUTED)
}

/// Style for positive feedback.
pub fn success() -> Style {
    base().fg(SUCCESS).add_modifier(Modifier::BOLD)
}

/// Style for negative feedback.
pub fn error() -> Style {
    base().fg(ERROR).add_modifier(Modifier::BOLD)
}

/// Style for the key of a key hint.
pub fn hint_key() -> Style {
    base()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
}

/// Style for the description of a key hint.
pub fn hint_label() -> Style {
    base().fg(TEXT_MUTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every style the interface uses, so none of them can be forgotten here.
    fn all_styles() -> Vec<(&'static str, Style)> {
        vec![
            ("base", base()),
            ("body", body()),
            ("muted", muted()),
            ("label", label()),
            ("question", question()),
            ("input", input()),
            ("timer", timer()),
            ("panel", panel()),
            ("title", title()),
            ("heading", heading()),
            ("selected", selected()),
            ("unselected", unselected()),
            ("disabled", disabled()),
            ("success", success()),
            ("error", error()),
            ("hint_key", hint_key()),
            ("hint_label", hint_label()),
        ]
    }

    #[test]
    fn every_style_sets_a_background_and_a_text_colour() {
        for (name, style) in all_styles() {
            assert!(style.bg.is_some(), "{name} mist een achtergrondkleur");
            assert!(style.fg.is_some(), "{name} mist een tekstkleur");
        }
    }

    #[test]
    fn every_style_uses_a_colour_from_the_palette() {
        let palette = [
            BACKGROUND,
            SURFACE,
            BORDER,
            ACCENT,
            TEXT,
            TEXT_MUTED,
            SUCCESS,
            ERROR,
            HIGHLIGHT_BG,
            TIMER,
        ];
        for (name, style) in all_styles() {
            for colour in [style.bg, style.fg].into_iter().flatten() {
                assert!(
                    palette.contains(&colour),
                    "{name} gebruikt kleur {colour:?}"
                );
            }
        }
    }

    #[test]
    fn feedback_and_selection_styles_are_visually_distinct() {
        assert_ne!(selected().bg, unselected().bg);
        assert_eq!(error().fg, Some(ERROR));
        assert_eq!(success().fg, Some(SUCCESS));
        assert_ne!(success().fg, error().fg);
    }
}
