//! Screen 2: create a learner, or change an existing one.
//!
//! The same form is used for both: `Tab` moves between the name and the grade,
//! `←` and `→` pick a groep, `Enter` saves and `Esc` goes back.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::app::{App, Field, FormMode};
use crate::i18n;
use crate::profile::Grade;

use super::super::theme;

/// Draws the form.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let form = app.form();
    let block = Block::bordered()
        .title(title(form.mode()))
        .border_type(BorderType::Rounded)
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let marker = |field: Field| if form.field() == field { "▸" } else { " " };
    let name_style = if form.field() == Field::Name {
        theme::input()
    } else {
        theme::body()
    };
    let grade_style = if form.field() == Field::Grade {
        theme::input()
    } else {
        theme::body()
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", marker(Field::Name)), theme::label()),
            Span::styled(format!("{}: ", i18n::FIELD_NAME), theme::label()),
            Span::styled(
                if form.name().is_empty() {
                    format!("{}▏", i18n::NAME_PLACEHOLDER)
                } else {
                    format!("{}▏", form.name())
                },
                name_style,
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", marker(Field::Grade)), theme::label()),
            Span::styled(format!("{}: ", i18n::FIELD_GRADE), theme::label()),
            Span::styled(format!("{} ◂ ▸", grade_label(form.grade())), grade_style),
        ]),
    ];

    if let Some(error) = form.error() {
        lines.push(Line::from(Span::styled(error.to_string(), theme::error())));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn title(mode: &FormMode) -> &'static str {
    match mode {
        FormMode::Create => i18n::CREATE_PROFILE,
        FormMode::Edit(_) => i18n::EDIT_PROFILE,
    }
}

/// `groep 4`, or `geen groep` when the grade is unset.
fn grade_label(grade: Option<Grade>) -> String {
    i18n::grade_or_none(grade.map(|grade| grade.value()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_title_depends_on_the_mode() {
        assert_eq!(title(&FormMode::Create), "Nieuw profiel");
    }

    #[test]
    fn grades_are_shown_in_dutch() {
        assert_eq!(grade_label(Some(Grade::new(4).unwrap())), "groep 4");
        assert_eq!(grade_label(Some(Grade::default())), "groep 3");
        assert_eq!(grade_label(None), "geen groep");
    }
}
