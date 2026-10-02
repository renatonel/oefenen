//! The key-hint bar that every screen shows at the bottom.
//!
//! The bar wraps onto a second line when the keys do not fit, so nothing is cut
//! off on a narrow terminal.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::i18n;

use super::theme;

/// One `keys = label` pair in the hint bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyHint {
    pub keys: &'static str,
    pub label: &'static str,
}

impl KeyHint {
    pub const fn new(keys: &'static str, label: &'static str) -> Self {
        Self { keys, label }
    }

    /// `q / Ctrl-C = Stoppen`
    pub fn quit() -> Self {
        Self::new("q / Ctrl-C", i18n::QUIT)
    }

    /// `Ctrl-C = Stoppen`, for screens that reserve `q` for typing.
    pub fn quit_chord() -> Self {
        Self::new("Ctrl-C", i18n::QUIT)
    }

    /// `Enter = kiezen`
    pub fn select() -> Self {
        Self::new(i18n::KEY_ENTER, i18n::HINT_KIEZEN)
    }

    /// `Enter = bevestigen`
    pub fn confirm() -> Self {
        Self::new(i18n::KEY_ENTER, i18n::HINT_BEVESTIGEN)
    }

    /// `Esc = terug`
    pub fn back() -> Self {
        Self::new(i18n::KEY_ESC, i18n::HINT_TERUG)
    }
}

/// How many rows `hints` need in a bar `width` columns wide.
pub fn required_rows(hints: &[KeyHint], width: u16) -> u16 {
    pack(hints, width).len().max(1) as u16
}

/// Draws the hint bar: `↑ ↓ = kiezen   Enter = bevestigen   Esc = terug`.
pub fn render(frame: &mut Frame, area: Rect, hints: &[KeyHint]) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let lines: Vec<Line<'_>> = pack(hints, area.width)
        .into_iter()
        .map(|row| {
            let mut spans = Vec::with_capacity(row.len() * 3);
            for (index, hint) in row.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw("   "));
                }
                spans.push(Span::styled(hint.keys, theme::hint_key()));
                spans.push(Span::styled(
                    format!(" = {}", hint.label),
                    theme::hint_label(),
                ));
            }
            Line::from(spans)
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(theme::base()),
        area,
    );
}

/// Groups the hints into rows that fit in `width` columns.
fn pack(hints: &[KeyHint], width: u16) -> Vec<Vec<KeyHint>> {
    let mut rows: Vec<Vec<KeyHint>> = Vec::new();
    let mut used = 0u16;
    for hint in hints {
        let separator = if rows
            .last()
            .is_some_and(|row: &Vec<KeyHint>| !row.is_empty())
        {
            3
        } else {
            0
        };
        let cost = separator + hint_width(hint);
        if rows.is_empty() || used + cost > width {
            rows.push(Vec::new());
            used = 0;
        }
        if let Some(row) = rows.last_mut() {
            row.push(*hint);
        }
        used += cost;
    }
    rows
}

/// Display width of one hint. All hint text is single-width, so counting
/// characters is enough.
fn hint_width(hint: &KeyHint) -> u16 {
    let width = hint.keys.chars().count() + 3 + hint.label.chars().count();
    u16::try_from(width).unwrap_or(u16::MAX)
}

/// Hints for the screens that are not modules: the extras plus `q` to quit.
pub fn quit_and(extra: &[KeyHint]) -> Vec<KeyHint> {
    let mut hints = extra.to_vec();
    hints.push(KeyHint::quit());
    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_are_labeled_in_dutch() {
        assert_eq!(KeyHint::quit().label, "Stoppen");
        assert_eq!(KeyHint::back().label, "terug");
        assert_eq!(KeyHint::select().label, "kiezen");
        assert_eq!(KeyHint::confirm().label, "bevestigen");
        assert_eq!(KeyHint::quit_chord().keys, "Ctrl-C");
    }

    #[test]
    fn quit_is_always_appended_last() {
        let hints = quit_and(&[KeyHint::back()]);
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0], KeyHint::back());
        assert_eq!(hints[1], KeyHint::quit());
    }

    #[test]
    fn everything_fits_on_one_row_in_a_wide_terminal() {
        let hints = quit_and(&[KeyHint::select(), KeyHint::new("n", i18n::NEW_PROFILE)]);
        assert_eq!(required_rows(&hints, 80), 1);
    }

    #[test]
    fn hints_wrap_onto_a_second_row_in_a_narrow_terminal() {
        let hints = quit_and(&[KeyHint::select(), KeyHint::new("n", i18n::NEW_PROFILE)]);
        let rows = required_rows(&hints, 30);
        assert!(rows >= 2, "verwacht meerdere rijen, kreeg {rows}");
        // Never more rows than hints.
        assert!(rows <= hints.len() as u16);
    }

    #[test]
    fn a_bar_always_has_at_least_one_row() {
        assert_eq!(required_rows(&[], 80), 1);
        assert_eq!(required_rows(&[KeyHint::quit()], 0), 1);
    }
}
