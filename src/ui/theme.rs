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
/// Timer while more than half of the time is left.
pub const TIMER_OK: Color = Color::Rgb(126, 231, 135);
/// Timer once half of the time is gone.
pub const TIMER_WARN: Color = Color::Rgb(255, 189, 68);
/// Timer at the last quarter: also bold, so the colour is never the only cue.
pub const TIMER_CRITICAL: Color = Color::Rgb(255, 95, 95);

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
///
/// No underline here, and that is not an oversight: the question is drawn as a
/// block font, so an underline would put a rule across every one of its rows and
/// the digits would read as if they were printed on ruled paper.
pub fn question() -> Style {
    base().fg(ACCENT).add_modifier(Modifier::BOLD)
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

/// Style for a countdown with plenty of time left.
pub fn timer_ok() -> Style {
    base().fg(TIMER_OK).add_modifier(Modifier::BOLD)
}

/// Style for a countdown that has used up half of its time.
pub fn timer_warn() -> Style {
    base().fg(TIMER_WARN).add_modifier(Modifier::BOLD)
}

/// Style for a countdown in its last quarter.
///
/// Bold on purpose: at this stage the colour alone would carry the warning, and
/// colour on its own is not accessible.
pub fn timer_critical() -> Style {
    base().fg(TIMER_CRITICAL).add_modifier(Modifier::BOLD)
}

/// The empty part of a gauge, and anything else that must not compete with the
/// coloured part next to it.
pub fn meter_track() -> Style {
    base().fg(BORDER)
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

/// Style for a correct answer, named after what it means rather than its
/// colour. Same green as [`success`], so it reads as one system.
pub fn feedback_correct() -> Style {
    success()
}

/// Style for a wrong or too-late answer. Same red as [`error`].
pub fn feedback_wrong() -> Style {
    error()
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
            ("timer_ok", timer_ok()),
            ("timer_warn", timer_warn()),
            ("timer_critical", timer_critical()),
            ("meter_track", meter_track()),
            ("panel", panel()),
            ("title", title()),
            ("heading", heading()),
            ("selected", selected()),
            ("unselected", unselected()),
            ("disabled", disabled()),
            ("success", success()),
            ("error", error()),
            ("feedback_correct", feedback_correct()),
            ("feedback_wrong", feedback_wrong()),
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
            TIMER_OK,
            TIMER_WARN,
            TIMER_CRITICAL,
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
        assert_eq!(feedback_correct(), success());
        assert_eq!(feedback_wrong(), error());
    }

    #[test]
    fn the_question_style_draws_no_line_across_the_digits() {
        // The question is a block font. `UNDERLINED` or `CROSSED_OUT` would draw
        // a rule through every row of it, which is unreadable for a child.
        let question = question();
        assert!(!question.add_modifier.contains(Modifier::UNDERLINED));
        assert!(!question.add_modifier.contains(Modifier::CROSSED_OUT));
        // It still has to stand out from the plain text around it.
        assert!(question.add_modifier.contains(Modifier::BOLD));
        assert_eq!(question.fg, Some(ACCENT));
    }

    #[test]
    fn the_timer_stages_go_from_green_through_yellow_to_red() {
        let stages = [timer_ok(), timer_warn(), timer_critical()];
        assert_eq!(stages[0].fg, Some(TIMER_OK));
        assert_eq!(stages[1].fg, Some(TIMER_WARN));
        assert_eq!(stages[2].fg, Some(TIMER_CRITICAL));
        // Every stage differs from the next, so the colour is a real cue and
        // not just a shade of the same red.
        assert_ne!(stages[0].fg, stages[1].fg);
        assert_ne!(stages[1].fg, stages[2].fg);
        assert_ne!(stages[0].fg, stages[2].fg);
        // All three are bold: the red stage is not carried by colour alone.
        for style in stages {
            assert!(style.add_modifier.contains(Modifier::BOLD));
        }
    }
}
