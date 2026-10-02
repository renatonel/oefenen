//! Dummy multiplication drill: the first module, and the proof that the module
//! contract works end to end.
//!
//! It shows a running timer driven by the application tick, accepts a typed
//! answer for one hard-coded question, and writes exactly one record with
//! per-fact statistics when the learner leaves the module.
//!
//! Everything about the actual exercise content is still a placeholder:
//!
//! * TODO: let the learner pick a tafel (table) to practise,
//! * TODO: generate questions instead of the single sample below,
//! * TODO: weight which facts are asked based on past results.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::event::{Event, Key};
use crate::i18n;
use crate::modules::{Module, ModuleContext, Subject, Transition};
use crate::ui::hints::KeyHint;
use crate::ui::theme;

// Id and directory name of this module's storage.
pub const MULTIPLICATION_ID: &str = "tafel-oefenen";

/// Version of the record schema below. Bump this when the shape changes.
pub const SCHEMA_VERSION: u32 = 1;

/// The single question this placeholder asks.
// TODO: replace with generated questions from a chosen tafel.
const SAMPLE_FACTOR_A: u32 = 7;
const SAMPLE_FACTOR_B: u32 = 8;
const SAMPLE_ANSWER: u32 = SAMPLE_FACTOR_A * SAMPLE_FACTOR_B;

/// Longest accepted answer, so a stray key cannot grow the buffer forever.
const MAX_INPUT_LEN: usize = 6;

/// One answered fact. This is the unit of statistics we want to keep: enough to
/// see which facts are hard, and to rebuild any future score from it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FactResult {
    pub factor_a: u32,
    pub factor_b: u32,
    /// Whether the answer was right. The learner's own answer is not stored.
    pub correct: bool,
    /// Milliseconds between showing the question and submitting an answer.
    pub response_time_ms: u64,
    #[serde(with = "time::serde::rfc3339")]
    pub answered_at: OffsetDateTime,
}

/// One line in `results.jsonl`.
///
/// Deliberately no total score: everything a progress screen needs can be
/// derived from `facts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionRecord {
    pub schema_version: u32,
    pub module_id: String,
    pub session_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
    pub facts: Vec<FactResult>,
}

/// What the learner sees.
#[derive(Debug, Clone, PartialEq)]
enum Phase {
    /// The question is on screen and the answer is being typed.
    Answering,
    /// The answer was submitted; waiting for Enter to go back.
    Feedback { correct: bool, answer: u32 },
}

/// Timed multiplication practice (placeholder version).
pub struct MultiplicationModule {
    phase: Phase,
    input: String,
    facts: Vec<FactResult>,
    session_id: String,
    /// Guards against writing the record twice (submit followed by Esc).
    record_written: bool,
}

impl Default for MultiplicationModule {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiplicationModule {
    pub fn new() -> Self {
        Self {
            phase: Phase::Answering,
            input: String::new(),
            facts: Vec::new(),
            session_id: String::new(),
            record_written: false,
        }
    }

    /// Whether the learner already submitted an answer in this session.
    pub fn is_finished(&self) -> bool {
        self.record_written
    }

    /// The question as shown to the learner (`7 × 8`).
    pub fn question(&self) -> String {
        format!("{SAMPLE_FACTOR_A} × {SAMPLE_FACTOR_B}")
    }

    fn submit(&mut self, ctx: &ModuleContext) {
        let Ok(answer) = self.input.trim().parse::<u32>() else {
            // No digits typed yet; nothing to submit.
            return;
        };
        let correct = answer == SAMPLE_ANSWER;
        self.facts.push(FactResult {
            factor_a: SAMPLE_FACTOR_A,
            factor_b: SAMPLE_FACTOR_B,
            correct,
            response_time_ms: ctx.elapsed_millis(),
            answered_at: ctx.started_at()
                + time::Duration::milliseconds(
                    i64::try_from(ctx.elapsed_millis()).unwrap_or(i64::MAX),
                ),
        });
        self.phase = Phase::Feedback { correct, answer };
        self.input.clear();
    }

    /// Writes the session record. Errors are reported through the context so the
    /// learner sees them instead of losing them in a log file.
    fn write_record(&mut self, ctx: &mut ModuleContext) {
        if self.record_written {
            return;
        }
        let record = SessionRecord {
            schema_version: SCHEMA_VERSION,
            module_id: MULTIPLICATION_ID.to_string(),
            session_id: self.session_id.clone(),
            started_at: ctx.started_at(),
            duration_ms: ctx.elapsed_millis(),
            facts: std::mem::take(&mut self.facts),
        };
        self.record_written = true;
        if let Err(error) = ctx.store().append_record(&record) {
            ctx.report_error(format!("{error}"));
        }
    }

    fn key_hints(&self) -> Vec<KeyHint> {
        match self.phase {
            Phase::Answering => vec![
                KeyHint::new(i18n::KEY_BACKSPACE, i18n::HINT_WISSEN),
                KeyHint::new(i18n::KEY_ENTER, i18n::HINT_ANTWOORD),
                KeyHint::back(),
            ],
            Phase::Feedback { .. } => vec![
                KeyHint::new(i18n::KEY_ENTER, i18n::HINT_NAAR_LIJST),
                KeyHint::back(),
            ],
        }
    }
}

impl Module for MultiplicationModule {
    fn id(&self) -> &'static str {
        MULTIPLICATION_ID
    }

    fn title(&self) -> String {
        String::from(i18n::multiplication::TITLE)
    }

    fn description(&self) -> String {
        String::from(i18n::multiplication::DESCRIPTION)
    }

    fn subject(&self) -> Subject {
        Subject::Math
    }

    /// Keys accepted right now, drawn by the application in the hint bar.
    fn hints(&self) -> Vec<KeyHint> {
        self.key_hints()
    }

    fn on_enter(&mut self, _ctx: &mut ModuleContext) {
        // Each visit is a fresh session with its own id, timer and input.
        self.phase = Phase::Answering;
        self.input.clear();
        self.facts.clear();
        self.record_written = false;
        self.session_id = Uuid::new_v4().to_string();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut ModuleContext) -> Transition {
        match event {
            Event::Tick => Transition::Stay,
            Event::Resize { .. } => Transition::Stay,
            Event::Key(Key::CtrlC) => {
                self.write_record(ctx);
                Transition::Exit
            }
            Event::Key(Key::Esc) => {
                self.write_record(ctx);
                Transition::Exit
            }
            Event::Key(Key::Enter) => {
                match self.phase {
                    Phase::Answering => self.submit(ctx),
                    Phase::Feedback { .. } => {
                        self.write_record(ctx);
                        return Transition::Exit;
                    }
                }
                Transition::Stay
            }
            Event::Key(Key::Backspace) => {
                if self.phase == Phase::Answering {
                    self.input.pop();
                }
                Transition::Stay
            }
            Event::Key(Key::Char(character)) => {
                // Digits type directly; anything else is ignored on purpose so a
                // child cannot get stuck.
                if self.phase == Phase::Answering
                    && character.is_ascii_digit()
                    && self.input.len() < MAX_INPUT_LEN
                {
                    self.input.push(*character);
                }
                Transition::Stay
            }
            Event::Key(_) => Transition::Stay,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &ModuleContext) {
        let block = Block::bordered()
            .title(i18n::multiplication::PLACEHOLDER)
            .border_type(BorderType::Rounded)
            .style(theme::panel());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let [question_area, answer_area, feedback_area] = Layout::vertical([
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .spacing(1)
        .flex(Flex::Legacy)
        .areas(inner);

        // Question with the learner's answer in front of the cursor.
        let shown_answer = if self.input.is_empty() {
            "_".to_string()
        } else {
            format!("{}_", self.input)
        };
        let question = Line::from(vec![
            Span::styled(self.question(), theme::question()),
            Span::styled(" = ", theme::question()),
            Span::styled(shown_answer, theme::input()),
        ]);
        frame.render_widget(
            Paragraph::new(question)
                .alignment(Alignment::Center)
                .style(theme::body()),
            question_area,
        );

        // Running timer, driven by the application tick.
        let timer = Line::from(vec![
            Span::styled(
                format!("{}  ", i18n::multiplication::ELAPSED_LABEL),
                theme::label(),
            ),
            Span::styled(i18n::format_duration(ctx.elapsed_millis()), theme::timer()),
        ]);
        frame.render_widget(
            Paragraph::new(timer).alignment(Alignment::Center),
            answer_area,
        );

        frame.render_widget(self.feedback_line(), feedback_area);
    }

    fn on_exit(&mut self, ctx: &mut ModuleContext) {
        // Leaving always stores what we know, even if the learner escaped.
        self.write_record(ctx);
        self.phase = Phase::Answering;
        self.input.clear();
        self.facts.clear();
        self.record_written = false;
    }
}

impl MultiplicationModule {
    /// The line under the timer: a hint, or the verdict after submitting.
    fn feedback_line(&self) -> Paragraph<'static> {
        let line = match self.phase {
            Phase::Answering => Line::from(Span::styled(
                i18n::multiplication::TYPE_DIGITS,
                theme::muted(),
            )),
            Phase::Feedback { correct: true, .. } => Line::from(Span::styled(
                i18n::multiplication::CORRECT,
                theme::success(),
            )),
            Phase::Feedback { correct: false, .. } => Line::from(vec![
                Span::styled(i18n::multiplication::INCORRECT, theme::error()),
                Span::raw(" "),
                Span::styled(
                    i18n::multiplication::correct_answer(SAMPLE_ANSWER),
                    theme::muted(),
                ),
            ]),
        };
        Paragraph::new(line).alignment(Alignment::Center)
    }
}

/// Convenience for tests: the type of a stored record.
pub type StoredRecords = Vec<SessionRecord>;

/// Reads back the records of this module for a context.
pub fn load_records(ctx: &ModuleContext) -> StoredRecords {
    ctx.store().read_records_lossy::<SessionRecord>()
}
