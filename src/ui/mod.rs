//! Everything that draws: theme, key-hint bar and one function per screen.
//!
//! The application state in [`crate::app`] never draws; this module reads it
//! and paints it. Keeping the two apart is what makes the state machine
//! testable without a terminal.

pub mod hints;
pub mod screens;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::{App, Screen};
use crate::i18n;

use self::hints::KeyHint;

/// Below this size the interface is unreadable, so we ask for a bigger window.
pub const MIN_WIDTH: u16 = 40;
/// Below this height the screens do not fit.
pub const MIN_HEIGHT: u16 = 14;

/// Draws one frame of the current screen.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    // Repaint the whole area: a smaller window must not leave old text behind.
    frame.render_widget(Block::new().style(theme::base()), area);

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(frame, area);
        return;
    }

    let toast_height = u16::from(app.toast().is_some());
    let hints = screen_hints(app);
    let hint_height = hints::required_rows(&hints, area.width);
    let [header_area, body_area, toast_area, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(toast_height),
        Constraint::Length(hint_height),
    ])
    .areas(area);

    render_header(frame, app, header_area);

    match app.screen() {
        Screen::ProfileSelect => screens::profile_select::render(frame, app, body_area),
        Screen::ProfileForm => screens::profile_form::render(frame, app, body_area),
        Screen::DeleteConfirm => screens::delete_confirm::render(frame, app, body_area),
        Screen::ModuleList => screens::module_list::render(frame, app, body_area),
        Screen::ModuleRunning => render_module(frame, app, body_area),
    }

    if toast_height == 1 {
        render_toast(frame, app, toast_area);
    }
    hints::render(frame, hint_area, &hints);
}

/// Draws the title bar with the active learner, if any.
fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    let title = Line::from(Span::styled(
        format!(" {} ", i18n::APP_TITLE),
        theme::title(),
    ));
    frame.render_widget(Paragraph::new(title), left);

    if let Some(profile) = app.active_profile() {
        let label = Line::from(Span::styled(format!("{} ", profile.name()), theme::label()));
        frame.render_widget(
            Paragraph::new(label).alignment(ratatui::layout::Alignment::Right),
            right,
        );
    }
}

/// Hands the whole body area to the running module.
fn render_module(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(index) = app.running_index() else {
        return;
    };
    let Some(context) = app.take_context() else {
        return;
    };
    app.modules_mut()[index].render(frame, area, &context);
    app.put_back_context(context);
}

/// The keys that are valid on the current screen.
fn screen_hints(app: &App) -> Vec<KeyHint> {
    match app.screen() {
        Screen::ProfileSelect => {
            let extra = if app.profiles().is_empty() {
                vec![KeyHint::new("n", i18n::NEW_PROFILE)]
            } else {
                vec![
                    KeyHint::select(),
                    KeyHint::new("n", i18n::NEW_PROFILE),
                    KeyHint::new("w", i18n::RENAME_PROFILE),
                    KeyHint::new("d", i18n::HINT_VERWIJDEREN),
                ]
            };
            hints::quit_and(&extra)
        }
        Screen::ProfileForm => {
            // `q` is a letter here, so quitting needs Ctrl-C.
            let mut hints = vec![
                KeyHint::new(i18n::KEY_TAB, i18n::HINT_NIEUW_VELD),
                KeyHint::new(i18n::KEY_LEFTRIGHT, i18n::HINT_GROEP_KIEZEN),
                KeyHint::new(i18n::KEY_BACKSPACE, i18n::HINT_WISSEN),
                KeyHint::new(i18n::KEY_ENTER, i18n::HINT_OPSLAAN),
                KeyHint::back(),
            ];
            hints.push(KeyHint::quit_chord());
            hints
        }
        Screen::DeleteConfirm => hints::quit_and(&[
            KeyHint::new(i18n::KEY_LEFTRIGHT, i18n::HINT_JA_NEE),
            KeyHint::confirm(),
            KeyHint::back(),
        ]),
        Screen::ModuleList => hints::quit_and(&[KeyHint::select(), KeyHint::back()]),
        Screen::ModuleRunning => {
            let mut hints = match app
                .running_index()
                .and_then(|index| app.modules().get(index))
            {
                Some(module) => module.hints(),
                None => Vec::new(),
            };
            hints.push(KeyHint::quit_chord());
            hints
        }
    }
}

/// Draws the short-lived message above the key-hint bar.
fn render_toast(frame: &mut Frame, app: &App, area: Rect) {
    let Some(toast) = app.toast() else {
        return;
    };
    let style = if toast.is_error {
        theme::error()
    } else {
        theme::success()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(toast.message.clone(), style)))
            .alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

/// Shown instead of any screen when the window is too small.
fn render_too_small(frame: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .title(i18n::APP_TITLE)
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(i18n::TOO_SMALL, theme::error())),
            Line::from(Span::styled(i18n::TOO_SMALL_HINT, theme::muted())),
            Line::from(format!(
                "{}x{} nodig, {}x{} beschikbaar",
                MIN_WIDTH, MIN_HEIGHT, area.width, area.height
            )),
        ])
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::event::{Event, Key};

    fn screen_text(width: u16, height: u16) -> String {
        let dir = tempfile::TempDir::new().unwrap();
        let mut app = App::new(dir.path(), std::time::Duration::from_millis(75)).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        buffer_to_string(terminal.backend())
    }

    fn buffer_to_string(backend: &ratatui::backend::TestBackend) -> String {
        let buffer = backend.buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    #[test]
    fn a_small_window_shows_the_dutch_warning_instead_of_a_screen() {
        // Wide enough for the message, far too short for a screen.
        let text = screen_text(34, 8);
        assert!(text.contains(i18n::TOO_SMALL), "{text}");
        // The hint wraps over two lines, so only the first words are checked.
        assert!(text.contains("Maak het venster"), "{text}");
        assert!(!text.contains(i18n::CHOOSE_PROFILE), "{text}");
    }

    #[test]
    fn a_normal_window_draws_the_create_form() {
        let text = screen_text(80, 24);
        assert!(text.contains(i18n::CREATE_PROFILE), "{text}");
        assert!(text.contains(i18n::KEY_ENTER), "{text}");
        assert!(!text.contains(i18n::TOO_SMALL), "{text}");
    }

    #[test]
    fn every_screen_renders_without_panicking() {
        for (width, height) in [(80, 24), (41, 15), (40, 14)] {
            let dir = tempfile::TempDir::new().unwrap();
            let mut app = App::new(dir.path(), std::time::Duration::from_millis(75)).unwrap();
            // Create a profile, then walk through every screen.
            for character in "Sem".chars() {
                app.handle_event(&Event::Key(Key::Char(character)));
            }
            app.handle_event(&Event::Key(Key::Enter));
            for key in [
                Key::Enter,
                Key::Char('w'),
                Key::Char('d'),
                Key::Esc,
                Key::Char('n'),
            ] {
                let backend = TestBackend::new(width, height);
                let mut terminal = Terminal::new(backend).unwrap();
                terminal.draw(|frame| render(frame, &mut app)).unwrap();
                app.handle_event(&Event::Key(key));
            }
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
        }
    }
}
