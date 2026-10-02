//! Screen 3: confirm deleting a learner.
//!
//! Deleting removes the profile directory and all recorded results, so it always
//! needs an explicit "ja".

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::app::{App, ConfirmChoice};
use crate::i18n;

use super::super::theme;

/// Draws the question with the two answers.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let Some(confirm) = app.confirm() else {
        return;
    };
    let Some(profile) = app.profiles().get(confirm.profile_index()) else {
        return;
    };

    let block = Block::bordered()
        .title(i18n::DELETE_PROFILE)
        .border_type(BorderType::Rounded)
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let yes_selected = confirm.choice() == ConfirmChoice::Yes;
    let yes = Line::from(Span::styled(
        format!("{} {}", if yes_selected { "▸" } else { " " }, i18n::YES),
        if yes_selected {
            theme::error()
        } else {
            theme::disabled()
        },
    ));
    let no = Line::from(Span::styled(
        format!("{} {}", if yes_selected { " " } else { "▸" }, i18n::NO),
        if yes_selected {
            theme::disabled()
        } else {
            theme::body()
        },
    ));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            i18n::confirm_delete(profile.name()),
            theme::body(),
        )),
        Line::from(""),
        yes,
        no,
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}
