//! Screen 4: pick a module.
//!
//! Modules are grouped by subject; the group heading is inserted as an extra
//! row, so the list keeps a separate mapping between cursor and module.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::i18n;
use crate::modules::{Module, Subject};

use super::super::theme;

/// Draws the module list.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .title(i18n::CHOOSE_MODULE)
        .border_type(BorderType::Rounded)
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let modules = app.modules();
    if modules.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(i18n::NO_MODULES, theme::muted())))
                .alignment(ratatui::layout::Alignment::Center)
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    let rows = build_rows(modules);
    let cursor = app.cursor().min(modules.len() - 1);
    let selected_row = rows
        .iter()
        .position(|row| matches!(row, Row::Module(index) if *index == cursor))
        .unwrap_or(0);

    let visible = inner.height as usize;
    let offset = selected_row.saturating_sub(visible.saturating_sub(1));

    let items: Vec<ListItem> = rows
        .iter()
        .skip(offset)
        .take(visible)
        .map(|row| match row {
            Row::Subject(subject) => ListItem::new(Line::from(Span::styled(
                format!(" {}", i18n::subject_label(*subject)),
                theme::heading(),
            ))),
            Row::Module(index) => ListItem::new(module_line(&modules[*index])),
        })
        .collect();

    let list = List::new(items)
        .style(theme::body())
        .highlight_style(theme::selected())
        .highlight_symbol("▸ ")
        .repeat_highlight_symbol(true);
    let mut state = ListState::default().with_offset(offset);
    state.select(Some(selected_row - offset));

    let [list_area, footer_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    frame.render_stateful_widget(list, list_area, &mut state);

    let footer = Line::from(Span::styled(
        format!("  {}", i18n::module_count(modules.len())),
        theme::muted(),
    ));
    frame.render_widget(Paragraph::new(footer), footer_area);
}

enum Row {
    Subject(Subject),
    Module(usize),
}

/// One row per subject heading followed by its modules.
fn build_rows(modules: &[Box<dyn Module>]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(modules.len() + 1);
    let mut last_subject: Option<Subject> = None;
    for (index, module) in modules.iter().enumerate() {
        let subject = module.subject();
        if last_subject != Some(subject) {
            rows.push(Row::Subject(subject));
            last_subject = Some(subject);
        }
        rows.push(Row::Module(index));
    }
    rows
}

/// `Tafels oefenen  —  Oefen je tafels met een timer.`
fn module_line(module: &dyn Module) -> Line<'static> {
    Line::from(vec![
        Span::styled(module.title(), theme::body()),
        Span::styled(format!("  —  {}", module.description()), theme::muted()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::registry;

    #[test]
    fn every_module_gets_a_row_and_the_first_one_a_heading() {
        let modules = registry();
        let rows = build_rows(&modules);
        assert_eq!(rows.len(), modules.len() + 1);
        assert!(matches!(rows[0], Row::Subject(Subject::Math)));
        assert!(matches!(rows[1], Row::Module(0)));
    }

    #[test]
    fn the_row_order_follows_the_registry() {
        let modules = registry();
        let rows = build_rows(&modules);
        let indexes: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Module(index) => Some(*index),
                Row::Subject(_) => None,
            })
            .collect();
        assert_eq!(indexes, (0..modules.len()).collect::<Vec<usize>>());
    }
}
