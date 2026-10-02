//! Everything the tafels module draws.
//!
//! The module state machine in [`super`] decides what is on screen; this file
//! only knows how to paint it. The two agree on one list of rows per screen:
//! [`table_rows`], [`timer_rows`] and [`result_rows`] are the single definition of
//! what a cursor can point at, and are used both for drawing and for key
//! handling, so a row can never be reachable but invisible, or visible but
//! unreachable.
//!
//! Nothing here changes state, and nothing here knows about storage.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Wrap};

use crate::event::Key;
use crate::i18n;
use crate::i18n::tafels;
use crate::ui::hints::KeyHint;
use crate::ui::theme;

use super::bigfont;
use super::questions::{LONGEST_QUESTION, TABLE_COUNT};
use super::records::{AnswerRecord, Outcome};
use super::session::{Session, Summary};
use super::settings::{self, Settings};
use super::timer::TimerStage;

// ---------------------------------------------------------------------------
// The rows of every screen
// ---------------------------------------------------------------------------

/// A row on the table-selection screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRow {
    /// One of the tables, with its check mark.
    Table(u32),
    /// The "ook omgekeerd" line.
    Reverse,
    /// The line that starts the session.
    Start,
}

/// Every row of the table-selection screen, in the order they are shown.
pub fn table_rows() -> Vec<TableRow> {
    let mut rows: Vec<TableRow> = (1..=TABLE_COUNT as u32).map(TableRow::Table).collect();
    rows.push(TableRow::Reverse);
    rows.push(TableRow::Start);
    rows
}

/// A row on the timer screen: one choice, with the time it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerRow {
    /// Seconds per question, by index in [`settings::per_question_presets`].
    PerQuestion(usize),
    /// Seconds per question for the whole session, by index in
    /// [`settings::total_speeds`].
    Speed(usize),
    /// No limit at all.
    None,
    /// A time the learner types themselves.
    Custom,
}

/// Every row of the timer screen.
pub fn timer_rows() -> Vec<TimerRow> {
    let mut rows: Vec<TimerRow> = (0..settings::per_question_presets().len())
        .map(TimerRow::PerQuestion)
        .collect();
    rows.extend((0..settings::total_speeds().len()).map(TimerRow::Speed));
    rows.push(TimerRow::None);
    rows.push(TimerRow::Custom);
    rows
}

/// The row the custom time is typed on: the last one.
pub const CUSTOM_ROW: usize = 8;

/// A row on the results screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultRow {
    /// Start the same round again.
    Replay,
    /// Back to the table selection.
    Settings,
    /// Back to the module list.
    Menu,
}

/// Every row of the results screen.
pub fn result_rows() -> Vec<ResultRow> {
    vec![ResultRow::Replay, ResultRow::Settings, ResultRow::Menu]
}

/// How a question is drawn, decided before anything is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionLayout {
    /// In the big font, at this size.
    Big(bigfont::Scale),
    /// Too small for even the smallest font, but the plain text still fits.
    Plain,
    /// Too small for anything.
    TooSmall,
}

/// Chooses how to draw the question in an area `width` by `height`.
///
/// The size is chosen for [`LONGEST_QUESTION`] rather than for the question at
/// hand, so the digits keep the same size for a whole round instead of jumping
/// around. [`QuestionLayout::Plain`] is the fallback for a window that is too
/// small for the font but big enough for the equation.
pub fn question_layout(width: u16, height: u16) -> QuestionLayout {
    if let Some(scale) = bigfont::choose(LONGEST_QUESTION, width, height) {
        return QuestionLayout::Big(scale);
    }
    let plain_width = u16::try_from(LONGEST_QUESTION.chars().count()).unwrap_or(u16::MAX);
    if height >= 1 && width >= plain_width {
        QuestionLayout::Plain
    } else {
        QuestionLayout::TooSmall
    }
}

/// How many rows the question needs in a chosen layout.
fn question_height(layout: QuestionLayout) -> u16 {
    match layout {
        QuestionLayout::Big(scale) => bigfont::text_height(scale),
        QuestionLayout::Plain => 1,
        // A row, so the complaint about the window can actually be read.
        QuestionLayout::TooSmall => 1,
    }
}

// ---------------------------------------------------------------------------
// Screen 1: table selection
// ---------------------------------------------------------------------------

/// Draws the table selection: the check boxes, the reversed-forms line and Start.
///
/// `warning` takes the place of the footer when it is there, so a complaint like
/// "kies minstens één tafel" is impossible to miss.
pub fn render_tables(
    frame: &mut Frame,
    area: Rect,
    settings: &Settings,
    cursor: usize,
    warning: Option<&str>,
) {
    let block = panel(tafels::CHOOSE_TABLES);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = table_rows();
    let selected = cursor.min(rows.len() - 1);
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            TableRow::Table(table) => table_line(settings, *table),
            TableRow::Reverse => checkbox_line(settings.reverse, tafels::REVERSE.to_string()),
            TableRow::Start => ListItem::new(Line::from(Span::styled(
                format!(" {}", tafels::START),
                theme::body(),
            ))),
        })
        .collect();

    let footer = match warning {
        Some(warning) => warning.to_string(),
        None => tafels::question_summary(settings.question_count(), settings.reverse),
    };
    frame.render_footer_list(items, inner, selected, &footer, warning.is_some());
}

/// The line of one table: a check mark and the table name.
fn table_line(settings: &Settings, table: u32) -> ListItem<'static> {
    checkbox_line(settings.tables.contains(&table), tafels::table_label(table))
}

/// A line with a check mark in front of it.
fn checkbox_line(on: bool, text: String) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            format!("{} ", check(on)),
            if on { theme::success() } else { theme::muted() },
        ),
        Span::styled(
            text,
            if on {
                theme::body().add_modifier(Modifier::BOLD)
            } else {
                theme::muted()
            },
        ),
    ]))
}

/// `▸` for a row that is on, a space for one that is off.
fn check(on: bool) -> &'static str {
    if on { "▸" } else { " " }
}

// ---------------------------------------------------------------------------
// Screen 2: timer settings
// ---------------------------------------------------------------------------

/// Draws the timer choices, with the custom time typed in place.
///
/// `warning` replaces the footer while it is there, so an unusable time cannot be
/// overlooked.
pub fn render_timer(
    frame: &mut Frame,
    area: Rect,
    settings: &Settings,
    cursor: usize,
    custom: &str,
    warning: Option<&str>,
) {
    let block = panel(tafels::CHOOSE_TIMER);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = timer_rows();
    let selected = cursor.min(rows.len() - 1);
    let questions = settings.question_count();

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| timer_line(*row, selected == CUSTOM_ROW, custom, questions))
        .collect();

    // One line of help under the list, and the chosen tables or the complaint.
    // Two rows of help: the custom-time hint is two lines long on a narrow
    // terminal, and half a sentence is worse than none.
    let [list_area, help_area, footer_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(inner);

    let visible = usize::from(list_area.height);
    let offset = selected.saturating_sub(visible.saturating_sub(1));
    frame.render_scrolled_list(items, list_area, selected, offset);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", timer_help(rows[selected])),
            theme::muted(),
        )))
        .wrap(Wrap { trim: true }),
        help_area,
    );
    let (footer, style) = match warning {
        Some(warning) => (warning.to_string(), theme::error()),
        None => (
            tafels::tables_summary(&settings.tables, settings.reverse),
            theme::muted(),
        ),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {footer}"), style))),
        footer_area,
    );
}

/// One row of the timer screen.
fn timer_line(row: TimerRow, selected: bool, custom: &str, questions: usize) -> ListItem<'static> {
    let label = timer_label(row, questions);
    // On the custom row the digits the learner typed stand in for the time, so
    // it is drawn as a field instead of as a plain label.
    let line = if row == TimerRow::Custom && selected {
        Line::from(vec![
            Span::styled(format!("{label} "), theme::body()),
            Span::styled(format!("{custom}{}", bigfont::CURSOR), theme::input()),
        ])
    } else {
        Line::from(Span::styled(label, theme::body()))
    };
    ListItem::new(line)
}

/// The label of a timer row, with the time it stands for.
fn timer_label(row: TimerRow, questions: usize) -> String {
    match row {
        TimerRow::PerQuestion(index) => {
            let seconds = settings::per_question_presets()[index];
            format!(
                "{} — {}",
                tafels::MODE_PER_QUESTION,
                tafels::seconds(seconds)
            )
        }
        TimerRow::Speed(index) => {
            let speed = settings::total_speeds()[index];
            let total = settings::total_for_speed(speed, questions);
            format!(
                "{} {} — {}",
                tafels::MODE_TOTAL,
                tafels::SPEEDS[index],
                tafels::total_for(total)
            )
        }
        TimerRow::None => tafels::MODE_NONE.to_string(),
        TimerRow::Custom => tafels::CUSTOM.to_string(),
    }
}

/// The help line under a timer row.
fn timer_help(row: TimerRow) -> &'static str {
    match row {
        TimerRow::PerQuestion(_) => tafels::MODE_HELP_PER_QUESTION,
        TimerRow::Speed(_) => tafels::MODE_HELP_TOTAL,
        TimerRow::None => tafels::MODE_HELP_NONE,
        TimerRow::Custom => tafels::CUSTOM_HINT,
    }
}

// ---------------------------------------------------------------------------
// Screen 3: the session
// ---------------------------------------------------------------------------

/// Draws the running session: the question, the verdict, the clock and the score.
pub fn render_session(frame: &mut Frame, area: Rect, session: &Session, now_ms: u64) {
    let block = panel(tafels::TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // One row of position, one of verdict, one of clock and one of score.
    let head = 1;
    let foot = 3;
    let available = inner.height.saturating_sub(head + foot);
    let layout = question_layout(inner.width, available);

    let [
        head_area,
        question_area,
        verdict_area,
        timer_area,
        score_area,
    ] = Layout::vertical([
        Constraint::Length(head),
        Constraint::Length(question_height(layout)),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                " {}",
                tafels::question_of(session.cursor(), session.question_count())
            ),
            theme::label(),
        ))),
        head_area,
    );

    render_question(frame, question_area, session, layout);
    render_verdict(frame, verdict_area, session);
    render_clock(frame, timer_area, session, now_ms);

    if score_area.height > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(
                    " {}",
                    tafels::score(session.correct_count(), session.question_count())
                ),
                theme::label(),
            ))),
            score_area,
        );
    }
}

/// The question itself.
fn render_question(frame: &mut Frame, area: Rect, session: &Session, layout: QuestionLayout) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let text = session.question_text();

    let lines: Vec<Line<'static>> = match layout {
        QuestionLayout::Big(scale) => bigfont::render(&text, scale)
            .into_iter()
            .map(|row| Line::from(Span::styled(row, theme::question())))
            .collect(),
        QuestionLayout::Plain => vec![Line::from(Span::styled(
            format!(" {text}"),
            theme::question(),
        ))],
        // Nothing fits: say so rather than show half a question.
        QuestionLayout::TooSmall => vec![Line::from(Span::styled(
            format!(" {}", tafels::QUESTION_TOO_SMALL),
            theme::error(),
        ))],
    };
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// What the learner has to do, or what just happened.
fn render_verdict(frame: &mut Frame, area: Rect, session: &Session) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(question) = session.current() else {
        return;
    };

    let (text, style) = match session.feedback_outcome() {
        Some(Outcome::Correct) => (tafels::GOOD.to_string(), theme::feedback_correct()),
        Some(Outcome::Incorrect) | Some(Outcome::TimedOut) => (
            format!(
                "{} ({})",
                tafels::wrong_answer_is(
                    outcome_label(session.feedback_outcome().unwrap_or(Outcome::Incorrect)),
                    question.expected()
                ),
                tafels::TYPE_DIGITS_HINT
            ),
            theme::feedback_wrong(),
        ),
        Some(Outcome::NotReached) | None => (tafels::TYPE_DIGITS.to_string(), theme::muted()),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {text}"), style)))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// The countdown, or a stopwatch when there is no limit.
fn render_clock(frame: &mut Frame, area: Rect, session: &Session, now_ms: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // Without a limit the clock counts up: the time spent so far, in seconds.
    let Some(remaining) = session.remaining_ms(now_ms) else {
        let elapsed = i64::try_from(session.timer_elapsed_ms(now_ms) / 1000).unwrap_or(i64::MAX);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {} {elapsed}", tafels::TIME_LABEL),
                theme::timer(),
            ))),
            area,
        );
        return;
    };

    let seconds = remaining.div_ceil(1000);
    let style = match session.stage(now_ms) {
        Some(TimerStage::Critical) => theme::timer_critical(),
        Some(TimerStage::Warn) => theme::timer_warn(),
        _ => theme::timer_ok(),
    };
    // The gauge shrinks with the coloured part, so the colour is never the only
    // clue about how much time is left.
    let width = usize::from(area.width) / 3;
    let filled = filled_of(session, now_ms, width);
    let line = Line::from(vec![
        Span::styled(format!(" {} {seconds}", tafels::TIME_LABEL), style),
        Span::raw("  "),
        Span::styled("█".repeat(filled), style),
        Span::styled("░".repeat(width - filled), theme::meter_track()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// How many cells of a gauge `width` are still filled.
fn filled_of(session: &Session, now_ms: u64, width: usize) -> usize {
    let filled = session.remaining_fraction(now_ms).unwrap_or(0.0) * width as f64;
    (filled.max(0.0) as usize).min(width)
}

// ---------------------------------------------------------------------------
// Screen 4: the results
// ---------------------------------------------------------------------------

/// How many mistakes are listed before the rest is counted in one line.
pub const MISTAKES_SHOWN: usize = 6;

/// Draws the results: the score, the time and what to practise next.
pub fn render_results(
    frame: &mut Frame,
    area: Rect,
    session: &Session,
    cursor: usize,
    now_ms: u64,
) {
    let block = panel(tafels::RESULTS_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let summary = session.summary(now_ms);
    let mistakes = summary.mistakes();

    // The score and the total time are always there; the average is a third row.
    let score_height = 2 + u16::from(summary.average_response_ms().is_some());
    let mistakes_height = if mistakes.is_empty() {
        1
    } else {
        // A title, the questions themselves and the "and n more" line.
        u16::try_from(mistakes.len().min(MISTAKES_SHOWN)).unwrap_or(u16::MAX) + 2
    };
    let [score_area, mistakes_area, options_area] = Layout::vertical([
        Constraint::Length(score_height),
        Constraint::Length(mistakes_height),
        Constraint::Min(1),
    ])
    .areas(inner);

    render_score_table(frame, score_area, &summary);
    render_mistakes(frame, mistakes_area, &mistakes);

    let items: Vec<ListItem> = result_rows()
        .iter()
        .map(|row| {
            ListItem::new(Line::from(Span::styled(
                format!(" {}", option_label(*row)),
                theme::body(),
            )))
        })
        .collect();
    let selected = cursor.min(result_rows().len() - 1);
    frame.render_footer_list(items, options_area, selected, "", false);
}

/// The score, the total time and the average reaction time.
fn render_score_table(frame: &mut Frame, area: Rect, summary: &Summary) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", tafels::RESULT_SCORE), theme::label()),
            Span::styled(
                tafels::score_value(summary.correct, summary.total),
                theme::title(),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", tafels::RESULT_TOTAL_TIME), theme::label()),
            Span::styled(i18n::format_duration(summary.total_ms), theme::body()),
        ]),
    ];
    if let Some(average) = summary.average_response_ms() {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", tafels::RESULT_AVERAGE), theme::label()),
            Span::styled(i18n::format_tenths(average), theme::body()),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The questions that still need practising.
fn render_mistakes(frame: &mut Frame, area: Rect, mistakes: &[&AnswerRecord]) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if mistakes.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", tafels::NO_MISTAKES),
                theme::feedback_correct(),
            ))),
            area,
        );
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        format!(" {}", tafels::MISTAKES_TITLE),
        theme::heading(),
    ))];
    let shown = mistakes.len().min(MISTAKES_SHOWN);
    for record in &mistakes[..shown] {
        lines.push(Line::from(Span::styled(
            format!(
                " {}",
                tafels::outcome_with_time(outcome_label(record.outcome), record.response_ms)
            ),
            theme::body(),
        )));
    }
    let rest = mistakes.len() - shown;
    if rest > 0 {
        lines.push(Line::from(Span::styled(
            format!(" {}", tafels::more(rest)),
            theme::muted(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The short name of an outcome, for the list of mistakes.
pub fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Correct => tafels::GOOD,
        Outcome::Incorrect => tafels::OUTCOME_INCORRECT,
        Outcome::TimedOut => tafels::OUTCOME_TIMED_OUT,
        Outcome::NotReached => tafels::OUTCOME_NOT_REACHED,
    }
}

/// The label of one option on the results screen.
fn option_label(row: ResultRow) -> &'static str {
    match row {
        ResultRow::Replay => tafels::OPTION_REPLAY,
        ResultRow::Settings => tafels::OPTION_SETTINGS,
        ResultRow::Menu => tafels::OPTION_MENU,
    }
}

// ---------------------------------------------------------------------------
// The stop question
// ---------------------------------------------------------------------------

/// The overlay that asks whether the learner really wants to stop.
pub fn render_stop_confirm(frame: &mut Frame, area: Rect, yes_selected: bool) {
    let block = panel(tafels::TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        format!(" {}", tafels::STOP_CONFIRM),
        theme::body(),
    ))];
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {} ", tafels::STOP_YES),
            option_style(yes_selected),
        ),
        Span::styled(
            format!(" {} ", tafels::STOP_NO),
            option_style(!yes_selected),
        ),
    ]));
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

/// The selected answer of the stop question stands out; the other one does not.
fn option_style(selected: bool) -> Style {
    if selected {
        theme::selected()
    } else {
        theme::unselected()
    }
}

// ---------------------------------------------------------------------------
// Shared pieces
// ---------------------------------------------------------------------------

/// A titled panel in the module's own colours.
fn panel(title: &str) -> Block<'static> {
    Block::bordered()
        .title(title.to_string())
        .border_type(BorderType::Rounded)
        .style(theme::panel())
}

/// Draws a scrolling list with `selected` highlighted, with a footer line below.
///
/// The list scrolls just far enough to keep the selected row on screen, so the
/// last row of a long list is reachable on a short window. `alert` styles the
/// footer as a complaint instead of as information.
trait RenderList {
    fn render_footer_list(
        &mut self,
        items: Vec<ListItem<'static>>,
        area: Rect,
        selected: usize,
        footer: &str,
        alert: bool,
    );

    /// Like [`Self::render_footer_list`], without a footer.
    fn render_scrolled_list(
        &mut self,
        items: Vec<ListItem<'static>>,
        area: Rect,
        selected: usize,
        offset: usize,
    );
}

impl RenderList for Frame<'_> {
    fn render_footer_list(
        &mut self,
        items: Vec<ListItem<'static>>,
        area: Rect,
        selected: usize,
        footer: &str,
        alert: bool,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        // One row for the footer, so it never covers the last row of the list.
        let [list_area, footer_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let visible = usize::from(list_area.height);
        let offset = selected.saturating_sub(visible.saturating_sub(1));
        self.render_scrolled_list(items, list_area, selected, offset);
        if !footer.is_empty() {
            let style = if alert {
                theme::error()
            } else {
                theme::muted()
            };
            self.render_widget(
                Paragraph::new(Line::from(Span::styled(format!(" {footer}"), style)))
                    .wrap(Wrap { trim: true }),
                footer_area,
            );
        }
    }

    fn render_scrolled_list(
        &mut self,
        items: Vec<ListItem<'static>>,
        area: Rect,
        selected: usize,
        offset: usize,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let mut state = ListState::default().with_offset(offset);
        state.select(Some(selected.saturating_sub(offset)));
        self.render_stateful_widget(
            List::new(items)
                .style(theme::body())
                .highlight_style(theme::selected())
                .highlight_symbol("▸ "),
            area,
            &mut state,
        );
    }
}

/// Keys the table screen accepts. The application adds the quit chord itself.
pub fn table_hints() -> Vec<KeyHint> {
    vec![
        KeyHint::new(i18n::KEY_ARROWS, i18n::HINT_KIEZEN),
        KeyHint::new(i18n::KEY_SPACE, i18n::HINT_VINKJE),
        KeyHint::new("a", i18n::HINT_ALLES_AAN),
        KeyHint::new("u", i18n::HINT_ALLES_UIT),
        KeyHint::new(i18n::KEY_ENTER, i18n::HINT_DOORGAAN),
        KeyHint::back(),
    ]
}

/// Keys the timer screen accepts.
pub fn timer_hints() -> Vec<KeyHint> {
    vec![
        KeyHint::new(i18n::KEY_ARROWS, i18n::HINT_KIEZEN),
        KeyHint::new(i18n::KEY_DIGITS, i18n::HINT_CIJFERS),
        KeyHint::new(i18n::KEY_BACKSPACE, i18n::HINT_WISSEN),
        KeyHint::new(i18n::KEY_ENTER, i18n::HINT_DOORGAAN),
        KeyHint::back(),
    ]
}

/// Keys the session accepts.
pub fn session_hints() -> Vec<KeyHint> {
    vec![
        KeyHint::new(i18n::KEY_DIGITS, i18n::HINT_CIJFERS),
        KeyHint::new(i18n::KEY_BACKSPACE, i18n::HINT_WISSEN),
        KeyHint::new(i18n::KEY_ENTER, i18n::HINT_ANTWOORD),
        KeyHint::back(),
    ]
}

/// Keys the results screen accepts.
pub fn result_hints() -> Vec<KeyHint> {
    vec![
        KeyHint::new(i18n::KEY_ARROWS, i18n::HINT_KIEZEN),
        KeyHint::confirm(),
    ]
}

/// Keys the stop question accepts.
pub fn stop_hints() -> Vec<KeyHint> {
    vec![
        KeyHint::new(i18n::KEY_LEFTRIGHT, i18n::HINT_JA_NEE),
        KeyHint::confirm(),
        KeyHint::new(i18n::KEY_ESC, i18n::HINT_NEE),
    ]
}

/// The table a number key stands for, or `None` for any other key.
///
/// Zero is table ten: there is no table zero, and a child counting the check
/// boxes on screen presses the key under the tenth one.
pub fn digit_of(key: &Key) -> Option<u32> {
    match key {
        Key::Char('0') => Some(10),
        Key::Char(character) => character.to_digit(10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use time::macros::datetime;

    use crate::modules::tafels::session::{FEEDBACK_MS, SystemShuffler};
    use crate::modules::tafels::settings::MAX_CUSTOM_DIGITS;
    use crate::modules::tafels::timer::TimerMode;

    /// Draws one frame and returns the text inside the panel, row by row.
    ///
    /// The border is left out: no test is about the frame, and leaving it in
    /// would break up any sentence the terminal wrapped.
    fn draw(width: u16, height: u16, paint: impl FnOnce(&mut Frame, Rect)) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("testbackend werkt");
        terminal
            .draw(|frame| paint(frame, frame.area()))
            .expect("tekenen lukt");
        let buffer = terminal.backend().buffer().clone();
        let last = buffer.area.width.saturating_sub(1);
        (0..buffer.area.height)
            .map(|y| {
                (1..last)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// Whether `text` holds `needle`, however the terminal wrapped it.
    ///
    /// A sentence of help can end up over two rows, and the test should care
    /// about the words, not about where the line happened to break.
    fn holds(text: &str, needle: &str) -> bool {
        let flatten = |s: &str| s.split_whitespace().collect::<Vec<&str>>().join(" ");
        let (text, needle) = (flatten(text), flatten(needle));
        text.contains(&needle)
    }

    fn settings() -> Settings {
        Settings {
            tables: vec![3, 7],
            reverse: true,
            timer_mode: TimerMode::PerQuestion,
            seconds: 15,
        }
    }

    fn session() -> Session {
        Session::start_at(
            "sess-1",
            datetime!(2026-10-02 12:00 UTC),
            &mut SystemShuffler::seeded(1),
            &settings(),
            0,
        )
    }

    #[test]
    fn the_table_screen_shows_every_table_and_both_lines() {
        let settings = settings();
        let text = draw(60, 16, |frame, area| {
            render_tables(frame, area, &settings, 0, None)
        });

        assert!(text.contains("Tafel 1"), "{text}");
        assert!(text.contains("Tafel 10"), "{text}");
        assert!(text.contains(tafels::REVERSE), "{text}");
        assert!(text.contains(tafels::START), "{text}");
        // The selected row is the one the cursor points at.
        assert!(text.contains("▸ "), "{text}");
        // The footer says how many questions the choices add up to.
        assert!(
            text.contains(&tafels::question_count(settings.question_count())),
            "{text}"
        );
    }

    #[test]
    fn the_table_screen_survives_a_cursor_on_every_row() {
        let settings = settings();
        for cursor in 0..table_rows().len() {
            let text = draw(40, 14, |frame, area| {
                render_tables(frame, area, &settings, cursor, None);
            });
            assert!(text.contains("Tafel"), "rij {cursor}: {text}");
        }
    }

    #[test]
    fn the_timer_screen_shows_every_preset_and_the_custom_time() {
        let settings = settings();
        let text = draw(60, 20, |frame, area| {
            render_timer(frame, area, &settings, CUSTOM_ROW, "0130", None)
        });

        assert!(text.contains(tafels::MODE_PER_QUESTION), "{text}");
        assert!(text.contains(tafels::MODE_TOTAL), "{text}");
        assert!(text.contains(tafels::MODE_NONE), "{text}");
        assert!(text.contains(tafels::CUSTOM), "{text}");
        // The custom row shows what has been typed, with the cursor behind it.
        assert!(text.contains("0130_"), "{text}");
        assert!(holds(&text, tafels::CUSTOM_HINT), "{text}");
        assert!(text.contains("Tafels: 3 en 7"), "{text}");
    }

    #[test]
    fn the_timer_screen_scrolls_so_the_last_row_stays_reachable() {
        let settings = settings();
        let text = draw(44, 10, |frame, area| {
            render_timer(frame, area, &settings, CUSTOM_ROW, "", None)
        });
        // Nine rows, four visible at a time: the list scrolls to the end.
        assert!(text.contains(tafels::CUSTOM), "{text}");
        assert!(!text.contains(tafels::MODE_PER_QUESTION), "{text}");
    }

    #[test]
    fn the_session_shows_the_question_the_clock_and_the_score() {
        let session = session();
        let text = draw(60, 16, |frame, area| {
            render_session(frame, area, &session, 0)
        });

        assert!(text.contains(&tafels::question_of(0, 36)), "{text}");
        assert!(text.contains(&tafels::score(0, 36)), "{text}");
        // The equation is drawn in the big font, so it is made of blocks.
        assert!(text.contains('█'), "{text}");
        assert!(text.contains(tafels::TIME_LABEL), "{text}");
        assert!(text.contains(tafels::TYPE_DIGITS), "{text}");
    }

    #[test]
    fn the_session_shows_the_verdict_and_the_right_answer() {
        let mut session = session();
        let question = session.current().expect("er is een vraag");
        for digit in '9'.to_string().chars() {
            session.on_key(&Key::Char(digit), 100);
        }
        session.on_key(&Key::Enter, 110);
        let text = draw(60, 16, |frame, area| {
            render_session(frame, area, &session, 110)
        });

        // The answer they typed is still there, not an empty line.
        assert!(text.contains("7 × 6 = 9"), "{text}");
        assert!(text.contains(&question.expected().to_string()), "{text}");
        assert!(text.contains(tafels::OUTCOME_INCORRECT), "{text}");
    }

    #[test]
    fn a_window_without_room_for_the_font_says_so() {
        // Too narrow even for the plain equation, so the question line says so.
        let session = session();
        let text = draw(12, 8, |frame, area| {
            render_session(frame, area, &session, 0)
        });
        assert!(text.contains(tafels::QUESTION_TOO_SMALL), "{text}");
    }

    #[test]
    fn the_font_is_chosen_for_the_longest_question_not_the_current_one() {
        // 7 × 1 fits a much smaller window than 10 × 10 = 100_, and both must
        // land on the same size, or the digits would jump during a round.
        let short = question_layout(60, 8);
        assert_eq!(short, question_layout(60, 8));
        assert_eq!(short, question_layout(60, 16));

        // Too small for the font, but wide enough for the plain equation.
        let plain = question_layout(20, 2);
        assert_eq!(plain, QuestionLayout::Plain);
        // Too small for anything at all.
        assert_eq!(question_layout(10, 1), QuestionLayout::TooSmall);
        assert_eq!(question_layout(0, 0), QuestionLayout::TooSmall);
    }

    #[test]
    fn the_results_show_the_score_the_mistakes_and_the_options() {
        let mut session = session();
        // Answer the first question wrong and the second one right, then stop:
        // one mistake, one point, and a running average.
        session.on_key(&Key::Char('4'), 100);
        session.on_key(&Key::Enter, 200);
        session.on_tick(200 + FEEDBACK_MS);
        let right = session
            .current()
            .expect("er is een vraag")
            .expected()
            .to_string();
        for digit in right.chars() {
            session.on_key(&Key::Char(digit), 300);
        }
        session.on_key(&Key::Enter, 400);
        session.on_tick(400 + FEEDBACK_MS);
        session.abandon(410 + FEEDBACK_MS);

        let text = draw(60, 20, |frame, area| {
            render_results(frame, area, &session, 0, 410 + FEEDBACK_MS)
        });

        assert!(text.contains(tafels::RESULTS_TITLE), "{text}");
        assert!(text.contains(&tafels::score_value(1, 36)), "{text}");
        assert!(text.contains(tafels::RESULT_TOTAL_TIME), "{text}");
        assert!(text.contains(tafels::RESULT_AVERAGE), "{text}");
        assert!(text.contains(tafels::MISTAKES_TITLE), "{text}");
        assert!(text.contains(tafels::OUTCOME_INCORRECT), "{text}");
        // Stopping early is not a list of extra mistakes: it simply ends.
        assert!(!text.contains(tafels::OUTCOME_NOT_REACHED), "{text}");
        assert!(text.contains(tafels::OPTION_REPLAY), "{text}");
        assert!(text.contains(tafels::OPTION_SETTINGS), "{text}");
        assert!(text.contains(tafels::OPTION_MENU), "{text}");
    }

    #[test]
    fn a_session_without_mistakes_says_so_instead_of_a_list() {
        let session = session();
        let text = draw(60, 20, |frame, area| {
            render_results(frame, area, &session, 2, 0)
        });
        assert!(text.contains(tafels::NO_MISTAKES), "{text}");
        // The cursor is on the last option.
        assert!(
            text.contains(&format!("▸  {}", tafels::OPTION_MENU)),
            "{text}"
        );
    }

    #[test]
    fn the_stop_question_shows_both_answers() {
        let text = draw(60, 16, |frame, area| render_stop_confirm(frame, area, true));
        assert!(text.contains(tafels::STOP_CONFIRM), "{text}");
        assert!(text.contains(tafels::STOP_YES), "{text}");
        assert!(text.contains(tafels::STOP_NO), "{text}");
    }

    #[test]
    fn the_row_lists_are_exactly_what_the_cursor_can_reach() {
        assert_eq!(table_rows().len(), TABLE_COUNT + 2);
        assert_eq!(timer_rows().len(), CUSTOM_ROW + 1);
        assert_eq!(result_rows().len(), 3);
        assert_eq!(timer_rows()[CUSTOM_ROW], TimerRow::Custom);
        assert_eq!(table_rows()[TABLE_COUNT], TableRow::Reverse);
        assert_eq!(table_rows()[TABLE_COUNT + 1], TableRow::Start);
    }

    #[test]
    fn digit_shortcuts_map_the_number_keys_to_table_numbers() {
        assert_eq!(digit_of(&Key::Char('1')), Some(1));
        assert_eq!(digit_of(&Key::Char('9')), Some(9));
        // Zero is table ten, not table zero.
        assert_eq!(digit_of(&Key::Char('0')), Some(10));
        assert_eq!(digit_of(&Key::Char('a')), None);
        assert_eq!(digit_of(&Key::Char('×')), None);
        assert_eq!(digit_of(&Key::Enter), None);
    }

    #[test]
    fn the_custom_field_shows_the_longest_time_that_is_accepted() {
        // Four digits is the longest custom time the module accepts.
        assert_eq!(MAX_CUSTOM_DIGITS, 4);
        let settings = settings();
        let text = draw(60, 20, |frame, area| {
            render_timer(frame, area, &settings, CUSTOM_ROW, "3600", None)
        });
        assert!(text.contains("3600_"), "{text}");
    }

    #[test]
    fn every_screen_renders_on_the_smallest_window_the_application_allows() {
        let settings = settings();
        let session = session();
        for (width, height) in [(40, 14), (41, 14), (80, 24)] {
            draw(width, height, |frame, area| {
                render_tables(frame, area, &settings, table_rows().len() - 1, None)
            });
            draw(width, height, |frame, area| {
                render_timer(frame, area, &settings, CUSTOM_ROW, "1", None)
            });
            draw(width, height, |frame, area| {
                render_session(frame, area, &session, 0)
            });
            draw(width, height, |frame, area| {
                render_results(frame, area, &session, result_rows().len() - 1, 0)
            });
            draw(width, height, |frame, area| {
                render_stop_confirm(frame, area, false)
            });
        }
    }
}
