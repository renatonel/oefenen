//! What a tafels session leaves behind: one JSON object per line, per profile.
//!
//! Two logs, both append-only and both in the module's own directory:
//!
//! * [`ANSWERS_FILE`] gets one line the moment a question is resolved, so even
//!   an abandoned session (or a crashed one) leaves usable data behind.
//! * [`SESSIONS_FILE`] gets one line when the session ends, completed or not.
//!
//! Every line starts with `"schema": 1` so a future format change can be
//! detected instead of guessed at, and every line of a session repeats its
//! `session_id`.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::modules::tafels::settings::Settings;
use crate::modules::tafels::timer::TimerMode;

use super::questions::Question;

/// Version of the record shapes below. Bump this when a field changes meaning.
pub const SCHEMA: u32 = 1;

/// One line per resolved question.
pub const ANSWERS_FILE: &str = "answers.jsonl";
/// One line per session.
pub const SESSIONS_FILE: &str = "sessions.jsonl";

/// How a question ended.
///
/// Only [`Outcome::Correct`] counts towards the score; the other three all count
/// as wrong but stay apart in the data, because "answered wrongly" and "never
/// got the chance" are very different things when you want to know what to
/// practise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The learner answered correctly.
    Correct,
    /// The learner answered, and it was wrong.
    Incorrect,
    /// The per-question countdown reached zero first.
    TimedOut,
    /// The total time ran out before this question was answered.
    NotReached,
}

impl Outcome {
    /// Whether this outcome adds a point to the score.
    pub fn is_correct(self) -> bool {
        matches!(self, Self::Correct)
    }

    /// Whether this outcome costs the learner a point.
    pub fn is_wrong(self) -> bool {
        !self.is_correct()
    }

    /// Every variant, so tests and screens cannot forget one.
    pub fn all() -> [Self; 4] {
        [
            Self::Correct,
            Self::Incorrect,
            Self::TimedOut,
            Self::NotReached,
        ]
    }
}

/// One resolved question, exactly as it is written to `answers.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerRecord {
    pub schema: u32,
    /// Shared with the `sessions.jsonl` line of the same round.
    pub session_id: String,
    /// When the question was resolved, in RFC 3339, always UTC.
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    /// Position in the shuffled session, starting at 0.
    pub question_index: usize,
    pub a: u32,
    pub b: u32,
    pub expected: u32,
    /// What the learner typed; `null` when the answer was never given.
    pub given: Option<u32>,
    pub outcome: Outcome,
    /// Milliseconds from showing the question to resolving it. `null` for
    /// [`Outcome::NotReached`], which was never shown.
    pub response_ms: Option<u64>,
}

impl AnswerRecord {
    /// Builds a record for `question`.
    pub fn new(
        session_id: &str,
        timestamp: OffsetDateTime,
        question_index: usize,
        question: Question,
        given: Option<u32>,
        outcome: Outcome,
        response_ms: Option<u64>,
    ) -> Self {
        Self {
            schema: SCHEMA,
            session_id: session_id.to_string(),
            timestamp,
            question_index,
            a: question.a,
            b: question.b,
            expected: question.expected(),
            given,
            outcome,
            response_ms,
        }
    }

    /// The question this record is about.
    pub fn question(&self) -> Question {
        Question::new(self.a, self.b)
    }
}

/// Whether a session ran to the end or was stopped by the learner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Every question was resolved, either by an answer or by the clock.
    Completed,
    /// The learner stopped it; unresolved questions are simply not written.
    Abandoned,
}

/// The settings a session was played with, as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSettings {
    /// The selected tables, ascending.
    pub tables: Vec<u32>,
    pub reverse: bool,
    pub timer_mode: TimerMode,
    /// The countdown in seconds; `null` when there is no limit.
    pub limit_seconds: Option<u64>,
}

impl From<&Settings> for SessionSettings {
    fn from(settings: &Settings) -> Self {
        Self {
            tables: settings.tables.clone(),
            reverse: settings.reverse,
            timer_mode: settings.timer_mode,
            limit_seconds: settings.limit_seconds(),
        }
    }
}

/// One line per session, written when the session ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub schema: u32,
    pub session_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub ended_at: OffsetDateTime,
    pub status: SessionStatus,
    pub settings: SessionSettings,
    /// Questions in the session, whether they were reached or not.
    pub question_count: usize,
    /// Questions answered correctly.
    pub correct_count: usize,
    /// Milliseconds of actual play; feedback pauses are not counted.
    pub total_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn answer(outcome: Outcome, given: Option<u32>, response_ms: Option<u64>) -> AnswerRecord {
        AnswerRecord::new(
            "sess-1",
            datetime!(2026-10-02 12:00 UTC),
            3,
            Question::new(7, 8),
            given,
            outcome,
            response_ms,
        )
    }

    #[test]
    fn only_correct_counts_as_a_point() {
        assert!(Outcome::Correct.is_correct());
        assert!(!Outcome::Correct.is_wrong());
        for wrong in [Outcome::Incorrect, Outcome::TimedOut, Outcome::NotReached] {
            assert!(!wrong.is_correct());
            assert!(wrong.is_wrong(), "{wrong:?} moet fout tellen");
        }
        assert_eq!(Outcome::all().len(), 4);
    }

    #[test]
    fn an_answer_line_has_the_documented_fields() {
        let line = serde_json::to_string(&answer(Outcome::Correct, Some(56), Some(1_200))).unwrap();
        assert!(!line.contains('\n'));

        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["schema"], 1);
        assert_eq!(value["session_id"], "sess-1");
        assert_eq!(value["question_index"], 3);
        assert_eq!(value["a"], 7);
        assert_eq!(value["b"], 8);
        assert_eq!(value["expected"], 56);
        assert_eq!(value["given"], 56);
        assert_eq!(value["outcome"], "correct");
        assert_eq!(value["response_ms"], 1_200);
        assert_eq!(value["timestamp"], "2026-10-02T12:00:00Z");
    }

    #[test]
    fn a_missing_answer_and_response_time_become_null() {
        let value: serde_json::Value = serde_json::from_str(
            &serde_json::to_string(&answer(Outcome::NotReached, None, None)).unwrap(),
        )
        .unwrap();
        assert!(value["given"].is_null());
        assert!(value["response_ms"].is_null());
        assert_eq!(value["outcome"], "not_reached");
    }

    #[test]
    fn every_outcome_uses_its_documented_spelling() {
        let names: Vec<String> = Outcome::all()
            .into_iter()
            .map(|outcome| serde_json::to_string(&outcome).unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "\"correct\"",
                "\"incorrect\"",
                "\"timed_out\"",
                "\"not_reached\""
            ]
        );
    }

    #[test]
    fn a_session_line_has_the_documented_fields() {
        let record = SessionRecord {
            schema: SCHEMA,
            session_id: "sess-1".to_string(),
            started_at: datetime!(2026-10-02 12:00 UTC),
            ended_at: datetime!(2026-10-02 12:01 UTC),
            status: SessionStatus::Abandoned,
            settings: SessionSettings {
                tables: vec![3, 7],
                reverse: true,
                timer_mode: TimerMode::Total,
                limit_seconds: Some(120),
            },
            question_count: 20,
            correct_count: 14,
            total_ms: 62_000,
        };

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(value["schema"], 1);
        assert_eq!(value["status"], "abandoned");
        assert_eq!(value["question_count"], 20);
        assert_eq!(value["correct_count"], 14);
        assert_eq!(value["total_ms"], 62_000);
        assert_eq!(value["settings"]["tables"], serde_json::json!([3, 7]));
        assert_eq!(value["settings"]["reverse"], true);
        assert_eq!(value["settings"]["timer_mode"], "total");
        assert_eq!(value["settings"]["limit_seconds"], 120);

        // And it reads back into the same shape.
        let parsed: SessionRecord = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, record);
    }

    #[test]
    fn a_session_without_a_limit_stores_null() {
        let settings = Settings {
            tables: vec![7],
            reverse: false,
            timer_mode: TimerMode::None,
            seconds: 10,
        };
        assert_eq!(SessionSettings::from(&settings).limit_seconds, None);

        let line = serde_json::to_string(&SessionSettings::from(&settings)).unwrap();
        assert!(line.contains("\"limit_seconds\":null"), "{line}");
    }
}
