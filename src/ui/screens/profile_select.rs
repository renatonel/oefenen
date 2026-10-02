//! Screen 1: pick a learner.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::i18n;
use crate::profile::Profile;

use super::super::theme;

/// Draws the profile list, or an invitation to create the first one.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .title(i18n::CHOOSE_PROFILE)
        .border_type(BorderType::Rounded)
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let profiles = app.profiles();
    if profiles.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(i18n::NO_PROFILES, theme::body())),
                Line::from(Span::styled(i18n::NO_PROFILES_HINT, theme::muted())),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    let visible = inner.height.saturating_sub(1).max(1) as usize;
    let cursor = app.cursor().min(profiles.len().saturating_sub(1));
    // Keep the selection visible without pulling in a scrolling state machine.
    let offset = cursor.saturating_sub(visible - 1);

    let items: Vec<ListItem> = profiles
        .iter()
        .skip(offset)
        .take(visible)
        .map(|profile| ListItem::new(profile_line(profile, inner.width.saturating_sub(2))))
        .collect();

    let list = List::new(items)
        .style(theme::body())
        .highlight_style(theme::selected())
        .highlight_symbol("▸ ")
        .repeat_highlight_symbol(true);
    let mut state = ListState::default().with_offset(offset);
    state.select(Some(cursor - offset));

    let [list_area, footer_area] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(1),
        ratatui::layout::Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_stateful_widget(list, list_area, &mut state);

    let footer = Line::from(Span::styled(
        format!("  {}", i18n::profile_count(profiles.len())),
        theme::muted(),
    ));
    frame.render_widget(Paragraph::new(footer), footer_area);
}

/// One row per learner, in as many columns as fit without cutting words in half.
fn profile_line(profile: &Profile, width: u16) -> Line<'static> {
    const NAME_COLUMN: usize = 20;
    const GRADE_COLUMN: usize = 12;

    let name = truncate(profile.name(), NAME_COLUMN);
    let grade = i18n::grade_or_none(profile.grade().map(|grade| grade.value()));
    let date = i18n::created_on(&profile.created_at());

    let date_column = u16::try_from(date.chars().count()).unwrap_or(u16::MAX);
    let mut spans = vec![Span::raw(format!("{name:<NAME_COLUMN$}"))];

    if width >= NAME_COLUMN as u16 + GRADE_COLUMN as u16 {
        spans.push(Span::styled(
            format!("{grade:<GRADE_COLUMN$}"),
            theme::muted(),
        ));
        if width >= NAME_COLUMN as u16 + GRADE_COLUMN as u16 + date_column {
            spans.push(Span::styled(date, theme::muted()));
        }
    } else if width > 0 {
        // Too narrow for the other columns: show only the name.
        let limit = usize::from(width);
        spans = vec![Span::raw(truncate(profile.name(), limit))];
    }

    Line::from(spans)
}

/// Cuts a name to `max` characters without splitting a character.
fn truncate(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_names_are_cut_and_marked() {
        assert_eq!(truncate("Sem", 20), "Sem");
        assert_eq!(truncate("Jan", 20), "Jan");
        let long = "a".repeat(30);
        let cut = truncate(&long, 20);
        assert_eq!(cut.chars().count(), 21);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn multi_byte_names_are_not_split_in_half() {
        let cut = truncate("Sémantiek", 3);
        assert_eq!(cut, "Sém…");
    }

    #[test]
    fn a_narrow_row_leaves_out_the_columns_that_do_not_fit() {
        use crate::profile::{Grade, ProfileId};
        use time::OffsetDateTime;

        let profile = Profile::new(
            ProfileId::generate("Sem"),
            "Sem",
            Some(Grade::new(4).unwrap()),
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        let rendered = |width: u16| {
            profile_line(&profile, width)
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        };

        assert!(rendered(80).contains(&i18n::created_on(&profile.created_at())));
        assert!(!rendered(36).contains("aangemaakt"));
        assert!(rendered(36).contains("groep 4"));
        assert_eq!(rendered(10), "Sem");
    }
}
