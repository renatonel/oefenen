//! The drill itself: a state machine that turns keys and elapsed time into
//! scores and records.
//!
//! Nothing in this file draws, writes or sleeps. Every transition takes the
//! current time in milliseconds as an argument, which is what makes expiry,
//! pausing, shuffling and scoring testable exactly, without a terminal and
//! without a single [`std::thread::sleep`]. In the running app that number is
//! [`ModuleContext::elapsed_millis`], a monotonic clock started when the module
//! was entered.
//!
//! ```text
//!                    ┌───────────────────────────────┐
//!                    ▼                               │ feedback over
//!   Running ──answer/timeout──▶ Feedback ────────────┘
//!      │                            │ last question
//!      │ time over                  ▼
//!      └────────────────────────▶ Done
//! ```
//!
//! Time is measured with [`Active`], which stops counting while a feedback pause
//! is on screen. A countdown therefore never costs the learner the second and a
//! bit they are reading the correct answer, and the stored duration is the time
//! actually spent answering.

use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::event::Key;

use super::bigfont;
use super::questions::{self, Question};
use super::records::{AnswerRecord, Outcome, SessionRecord, SessionSettings, SessionStatus};
use super::settings::Settings;
use super::timer::{Timer, TimerMode, TimerStage};

/// How long the verdict stays on screen, in milliseconds.
pub const FEEDBACK_MS: u64 = 1_200;

/// Longest answer we accept, so a stray key cannot grow the buffer forever.
///
/// Three digits is the widest product the available tables can produce.
pub const MAX_INPUT_LEN: usize = 3;

/// A random source a session can be shuffled with.
///
/// Tests pass something deterministic; the application passes [`SystemShuffler`].
pub trait Shuffler {
    /// Puts `questions` in a new, random order.
    fn shuffle(&mut self, questions: &mut [Question]);
}

/// The real shuffler: seeded by the operating system, so two sessions with the
/// same tables never come out in the same order.
#[derive(Debug, Clone)]
pub struct SystemShuffler {
    rng: StdRng,
}

impl SystemShuffler {
    pub fn new() -> Self {
        Self {
            rng: StdRng::from_os_rng(),
        }
    }

    /// A shuffler with a fixed seed: the same order every time it is used.
    pub fn seeded(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

impl Default for SystemShuffler {
    fn default() -> Self {
        Self::new()
    }
}

impl Shuffler for SystemShuffler {
    fn shuffle(&mut self, questions: &mut [Question]) {
        questions.shuffle(&mut self.rng);
    }
}

/// A shuffler that leaves the order alone, for tests about anything else.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoShuffle;

impl Shuffler for NoShuffle {
    fn shuffle(&mut self, _questions: &mut [Question]) {}
}

/// Active time, excluding the pauses between questions.
///
/// A segment runs while the learner is answering and is stopped while feedback
/// is on screen; the pieces are added up instead of being counted per tick, so
/// the outcome does not depend on how often the terminal was redrawn.
#[derive(Debug, Clone, Copy, Default)]
struct Active {
    accumulated_ms: u64,
    /// When the running segment started, or `None` while paused.
    segment_start_ms: Option<u64>,
}

impl Active {
    /// Active time up to `now_ms`.
    fn elapsed_ms(&self, now_ms: u64) -> u64 {
        match self.segment_start_ms {
            Some(start) => self
                .accumulated_ms
                .saturating_add(now_ms.saturating_sub(start)),
            None => self.accumulated_ms,
        }
    }

    /// Stops the clock at `now_ms`.
    fn pause(&mut self, now_ms: u64) {
        if let Some(start) = self.segment_start_ms.take() {
            self.accumulated_ms = self
                .accumulated_ms
                .saturating_add(now_ms.saturating_sub(start));
        }
    }

    /// Starts the clock again at `now_ms`.
    fn resume(&mut self, now_ms: u64) {
        if self.segment_start_ms.is_none() {
            self.segment_start_ms = Some(now_ms);
        }
    }
}

/// What the session is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The question is on screen and the answer is being typed.
    Running,
    /// The question is resolved and the verdict is on screen.
    ///
    /// It carries the question and what was submitted for it, so the whole
    /// screen keeps showing one coherent thing: the equation that was answered,
    /// the answer that was given, and a clock that has stopped where it was.
    /// The next question is not built until this phase ends.
    Feedback {
        outcome: Outcome,
        question: Question,
        submitted: Option<u32>,
    },
    /// Nothing left to answer; the results screen takes over.
    Done,
}

/// The score of a finished session.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// Questions in the session, whether they were reached or not.
    pub total: usize,
    /// Questions answered correctly.
    pub correct: usize,
    /// Active time, feedback pauses excluded.
    pub total_ms: u64,
    /// Every resolved question, in the order it was asked.
    pub answers: Vec<AnswerRecord>,
}

impl Summary {
    /// Percentage of questions answered correctly, rounded to whole numbers.
    pub fn accuracy_percent(&self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        let correct = u32::try_from(self.correct).unwrap_or(u32::MAX);
        let total = u32::try_from(self.total).unwrap_or(u32::MAX).max(1);
        correct.saturating_mul(100) / total
    }

    /// Mean time to answer, in milliseconds.
    ///
    /// Only questions that reached an answer count: a question that timed out
    /// spent its whole limit on screen and does count, but a question that was
    /// never shown has no reaction time at all, and averaging it in would drag
    /// the number down for no reason.
    pub fn average_response_ms(&self) -> Option<u64> {
        let mut sum = 0_u64;
        let mut count = 0_u64;
        for answer in &self.answers {
            if let Some(millis) = answer.response_ms {
                sum = sum.saturating_add(millis);
                count = count.saturating_add(1);
            }
        }
        (count > 0).then(|| sum / count)
    }

    /// Every question that did not earn a point, in the order it was asked.
    pub fn mistakes(&self) -> Vec<&AnswerRecord> {
        self.answers
            .iter()
            .filter(|answer| answer.outcome.is_wrong())
            .collect()
    }
}

/// One timed multiplication drill.
///
/// Built with [`Session::start`] and driven with [`Session::on_key`] and
/// [`Session::on_tick`], handing in the time each time. Records are collected
/// with [`Session::take_pending`], which the caller must drain after every event:
/// an answer has to reach the disk the moment it is known, so a session that is
/// stopped halfway still leaves the questions that were really asked behind.
pub struct Session {
    id: String,
    settings: Settings,
    /// Every question, shuffled; each one is asked exactly once.
    questions: Vec<Question>,
    /// Index of the question on screen; equal to the number already resolved.
    ///
    /// It only moves on when the verdict ends, so during the pause the screen
    /// still shows the question that was just answered.
    cursor: usize,
    /// One record per resolved question, in the order they were asked.
    answers: Vec<AnswerRecord>,
    phase: Phase,
    input: String,
    timer: Timer,
    active: Active,
    /// When the question on screen appeared, for its response time and its own
    /// countdown.
    question_shown_ms: u64,
    /// When the verdict appeared, for the length of the pause and to hold the
    /// per-question clock still while it is up.
    feedback_started_ms: u64,
    /// Wall-clock start, so records carry real timestamps.
    started_at: OffsetDateTime,
    /// The same instant on the monotonic clock, to line timestamps up with times.
    start_ms: u64,
    /// Set once the session is over; a later event must not change it.
    status: Option<SessionStatus>,
    /// When the session ended, on the monotonic clock.
    ended_ms: Option<u64>,
    /// Records resolved but not yet taken by the caller.
    pending: Vec<AnswerRecord>,
}

impl Session {
    /// Starts a session with a fresh id and the current wall-clock time.
    pub fn start(shuffler: &mut impl Shuffler, settings: &Settings, now_ms: u64) -> Self {
        Self::start_at(
            &Uuid::new_v4().to_string(),
            OffsetDateTime::now_utc(),
            shuffler,
            settings,
            now_ms,
        )
    }

    /// Starts a session with an explicit id and start time, so tests get
    /// reproducible records.
    pub fn start_at(
        id: &str,
        started_at: OffsetDateTime,
        shuffler: &mut impl Shuffler,
        settings: &Settings,
        now_ms: u64,
    ) -> Self {
        let mut questions = questions::generate(&settings.tables, settings.reverse);
        shuffler.shuffle(&mut questions);

        let mut session = Self {
            id: id.to_string(),
            settings: settings.clone(),
            questions,
            cursor: 0,
            answers: Vec::new(),
            phase: Phase::Running,
            input: String::new(),
            timer: Timer::new(settings.timer_mode, settings.seconds),
            active: Active {
                accumulated_ms: 0,
                segment_start_ms: Some(now_ms),
            },
            question_shown_ms: now_ms,
            feedback_started_ms: 0,
            started_at,
            start_ms: now_ms,
            status: None,
            ended_ms: None,
            pending: Vec::new(),
        };

        // Nothing to ask (no table was selected): the results screen can show
        // straight away instead of waiting for a key press.
        if session.questions.is_empty() {
            session.finish(SessionStatus::Completed, now_ms);
        }
        session
    }

    // --- reading the session ---------------------------------------------

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The settings this session was started with.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Whether the session is over and the results can be shown.
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// How the session ended, or `None` while it is still running.
    pub fn status(&self) -> Option<SessionStatus> {
        self.status
    }

    /// The question on screen, or `None` once the session is over.
    pub fn current(&self) -> Option<Question> {
        self.questions.get(self.cursor).copied()
    }

    /// Questions in this session.
    pub fn question_count(&self) -> usize {
        self.questions.len()
    }

    /// Position of the question on screen, starting at 0.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The digits typed so far, or the answer that was just submitted.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Questions answered correctly so far.
    pub fn correct_count(&self) -> usize {
        self.answers
            .iter()
            .filter(|answer| answer.outcome.is_correct())
            .count()
    }

    /// Every resolved question, in the order it was asked.
    pub fn answers(&self) -> &[AnswerRecord] {
        &self.answers
    }

    /// The outcome on screen, if the verdict is up.
    pub fn feedback_outcome(&self) -> Option<Outcome> {
        match self.phase {
            Phase::Feedback { outcome, .. } => Some(outcome),
            _ => None,
        }
    }

    /// The timer, for the countdown or the stopwatch.
    pub fn timer(&self) -> Timer {
        self.timer
    }

    /// Milliseconds that count against the timer right now.
    ///
    /// The per-question mode measures the current question, the other modes the
    /// whole session. The verdict pause is excluded from all three: the screen
    /// stands still while the learner reads, and the timer on it stands still
    /// too.
    pub fn timer_elapsed_ms(&self, now_ms: u64) -> u64 {
        match self.timer.mode() {
            // A per-question clock is restarted every question, so it is simply
            // the age of the question on screen. While the verdict is up that
            // question is the one being judged, and its clock is held at the
            // moment it was resolved, so the countdown does not run on
            // underneath the answer. It is the same value recorded as the
            // response time, which is what makes the screen self-consistent.
            TimerMode::PerQuestion => match self.phase {
                Phase::Feedback { .. } => self
                    .feedback_started_ms
                    .saturating_sub(self.question_shown_ms),
                _ => now_ms.saturating_sub(self.question_shown_ms),
            },
            // The total clock and the stopwatch run over the whole session, and
            // `active` is already paused for the verdict.
            TimerMode::Total | TimerMode::None => self.active.elapsed_ms(now_ms),
        }
    }

    /// Milliseconds left on the countdown, or `None` without a limit.
    pub fn remaining_ms(&self, now_ms: u64) -> Option<u64> {
        self.timer.remaining_ms(self.timer_elapsed_ms(now_ms))
    }

    /// The colour stage of the countdown, or `None` without a limit.
    pub fn stage(&self, now_ms: u64) -> Option<TimerStage> {
        self.timer.stage(self.timer_elapsed_ms(now_ms))
    }

    /// The part of the limit still left, between 0 and 1.
    pub fn remaining_fraction(&self, now_ms: u64) -> Option<f64> {
        self.timer.remaining_fraction(self.timer_elapsed_ms(now_ms))
    }

    /// Whether the countdown has run out.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.timer.is_expired(self.timer_elapsed_ms(now_ms))
    }

    /// Active time up to `now_ms`, feedback pauses excluded.
    pub fn total_ms(&self, now_ms: u64) -> u64 {
        self.active.elapsed_ms(now_ms)
    }

    /// How long the verdict has been on screen, or `None` outside feedback.
    pub fn feedback_elapsed_ms(&self, now_ms: u64) -> Option<u64> {
        match self.phase {
            Phase::Feedback { .. } => Some(now_ms.saturating_sub(self.feedback_started_ms)),
            _ => None,
        }
    }

    /// Records resolved since the last call, oldest first.
    pub fn take_pending(&mut self) -> Vec<AnswerRecord> {
        std::mem::take(&mut self.pending)
    }

    /// The question as it is drawn: the equation, the typed digits and the
    /// cursor. During the verdict the same spot shows what was submitted, next
    /// to the question it belongs to.
    pub fn question_text(&self) -> String {
        match self.phase {
            Phase::Feedback {
                question,
                submitted,
                ..
            } => {
                format!("{} = {}", question.label(), Self::submitted_text(submitted))
            }
            _ => {
                let Some(question) = self.current() else {
                    return String::new();
                };
                format!("{} = {}{}", question.label(), self.input, bigfont::CURSOR)
            }
        }
    }

    /// What was submitted for the question now being judged.
    ///
    /// A question that ran out of time has no answer to show, which is drawn as
    /// a dash so the verdict has something to sit next to.
    fn submitted_text(submitted: Option<u32>) -> String {
        match submitted {
            Some(given) => given.to_string(),
            None => "-".to_string(),
        }
    }

    /// The score of the session so far.
    pub fn summary(&self, now_ms: u64) -> Summary {
        Summary {
            total: self.questions.len(),
            correct: self.correct_count(),
            total_ms: self.total_ms(now_ms),
            answers: self.answers.clone(),
        }
    }

    /// The line for `sessions.jsonl`, once the session has ended.
    pub fn to_record(&self) -> Option<SessionRecord> {
        let status = self.status?;
        let ended_ms = self.ended_ms?;
        Some(SessionRecord {
            schema: super::records::SCHEMA,
            session_id: self.id.clone(),
            started_at: self.started_at,
            ended_at: self.wall_clock(ended_ms),
            status,
            settings: SessionSettings::from(&self.settings),
            question_count: self.questions.len(),
            correct_count: self.correct_count(),
            total_ms: self.total_ms(ended_ms),
        })
    }

    // --- input ------------------------------------------------------------

    /// Handles one key press. Keys that mean nothing to the session are ignored.
    pub fn on_key(&mut self, key: &Key, now_ms: u64) {
        match self.phase {
            Phase::Running => self.on_key_running(key, now_ms),
            Phase::Feedback { .. } => self.on_key_feedback(key, now_ms),
            Phase::Done => {}
        }
    }

    fn on_key_running(&mut self, key: &Key, now_ms: u64) {
        match key {
            Key::Char(character) if character.is_ascii_digit() => {
                // Digits type into the answer and never submit it: a learner may
                // change their mind, and a mistyped key must not cost a question.
                if self.input.len() < MAX_INPUT_LEN {
                    self.input.push(*character);
                }
            }
            Key::Backspace => {
                self.input.pop();
            }
            Key::Enter => self.submit(now_ms),
            _ => {}
        }
    }

    fn on_key_feedback(&mut self, key: &Key, now_ms: u64) {
        // While the verdict is up the only useful key is one that skips the
        // pause; every other key is deliberately ignored so a held Enter cannot
        // skip past the next question.
        if matches!(key, Key::Enter | Key::Char(' ')) {
            self.end_feedback(now_ms);
        }
    }

    /// Submits the typed answer. An empty answer is not an answer.
    fn submit(&mut self, now_ms: u64) {
        let Ok(given) = self.input.parse::<u32>() else {
            return;
        };
        let Some(question) = self.current() else {
            return;
        };
        let outcome = if given == question.expected() {
            Outcome::Correct
        } else {
            Outcome::Incorrect
        };
        self.resolve(question, Some(given), outcome, now_ms);
    }

    // --- the clock --------------------------------------------------------

    /// Checks the clock: expiry, and whether the verdict has been up long enough.
    ///
    /// Nothing is advanced here; the caller decides how much time has passed. One
    /// tick can end the verdict *and* run the clock out, so the checks run in that
    /// order: a learner never gets a fresh question on a round that is already over.
    pub fn on_tick(&mut self, now_ms: u64) {
        if matches!(self.phase, Phase::Feedback { .. })
            && now_ms.saturating_sub(self.feedback_started_ms) >= FEEDBACK_MS
        {
            self.end_feedback(now_ms);
        }

        match self.phase {
            Phase::Running => match self.timer.mode() {
                // One question ran out of time.
                TimerMode::PerQuestion if self.is_expired(now_ms) => {
                    if let Some(question) = self.current() {
                        self.resolve(question, None, Outcome::TimedOut, now_ms);
                    }
                }
                // The whole session ran out: what is left was never asked.
                TimerMode::Total if self.is_expired(now_ms) => self.run_out(now_ms),
                _ => {}
            },
            Phase::Feedback { .. } | Phase::Done => {}
        }
    }

    /// The total time ran out.
    ///
    /// The question that was on screen was shown and not answered, so it counts
    /// as too late; everything behind it was never shown at all.
    fn run_out(&mut self, now_ms: u64) {
        if let Some(question) = self.current() {
            self.resolve(question, None, Outcome::TimedOut, now_ms);
            // The verdict for the question that was on screen is still owed, but
            // there is no time left to show it in.
            self.end_feedback(now_ms);
        }
        self.finish(SessionStatus::Completed, now_ms);
        // Whatever is left behind the cursor was never asked, but it does belong
        // in the log, each under its own question number.
        while let Some(question) = self.questions.get(self.cursor).copied() {
            let record = self.build_record(question, None, Outcome::NotReached, None);
            self.answers.push(record.clone());
            self.pending.push(record);
            self.cursor = self.cursor.saturating_add(1);
        }
    }

    // --- resolving a question ---------------------------------------------

    /// Records one question and shows the verdict.
    fn resolve(&mut self, question: Question, given: Option<u32>, outcome: Outcome, now_ms: u64) {
        let response_ms = now_ms.saturating_sub(self.question_shown_ms);
        let record = self.build_record(question, given, outcome, Some(response_ms));
        self.answers.push(record.clone());
        self.pending.push(record);
        self.input.clear();
        // The question and the answer go into the phase, because the typed digits
        // are cleared while the verdict is up and the two still have to stay
        // readable next to it.
        self.phase = Phase::Feedback {
            outcome,
            question,
            submitted: given,
        };
        self.feedback_started_ms = now_ms;
        // The learner is reading, not answering: the clock waits for them.
        self.active.pause(now_ms);
    }

    /// Builds the record of one question, with its timestamp on the wall clock.
    fn build_record(
        &self,
        question: Question,
        given: Option<u32>,
        outcome: Outcome,
        response_ms: Option<u64>,
    ) -> AnswerRecord {
        AnswerRecord::new(
            &self.id,
            self.wall_clock(self.now_for(response_ms)),
            self.cursor,
            question,
            given,
            outcome,
            response_ms,
        )
    }

    /// The moment a record describes: when the question was resolved.
    fn now_for(&self, response_ms: Option<u64>) -> u64 {
        match response_ms {
            // Resolved while the question was on screen: the question's own
            // start plus the time it took to answer it.
            Some(millis) => self.question_shown_ms.saturating_add(millis),
            // Never resolved at a moment of its own, so it belongs to the end of
            // the session: the moment the learner stopped, or the moment the
            // clock ran out.
            None => self.ended_ms.unwrap_or(self.start_ms),
        }
    }

    /// Leaves the feedback pause, either with the next question or with the
    /// results screen.
    ///
    /// This is the only place the cursor moves, so the pause is exactly the
    /// window in which the answered question stays on screen.
    fn end_feedback(&mut self, now_ms: u64) {
        self.cursor = self.cursor.saturating_add(1);
        if self.cursor >= self.questions.len() {
            self.finish(SessionStatus::Completed, now_ms);
            return;
        }
        self.phase = Phase::Running;
        self.input.clear();
        // The next question starts its own countdown from here, not from where
        // the one just answered stopped.
        self.question_shown_ms = now_ms;
        self.active.resume(now_ms);
    }

    fn finish(&mut self, status: SessionStatus, now_ms: u64) {
        if self.status.is_some() {
            // Only the first ending counts; a later event cannot change it.
            return;
        }
        self.active.pause(now_ms);
        self.status = Some(status);
        self.ended_ms = Some(now_ms);
        self.phase = Phase::Done;
        self.input.clear();
    }

    /// Stops the session early, because the learner asked to.
    ///
    /// Questions that were never shown are not written: the answer log only ever
    /// holds questions that really were asked.
    pub fn abandon(&mut self, now_ms: u64) {
        self.finish(SessionStatus::Abandoned, now_ms);
    }

    /// Wall-clock time for `now_ms` on the monotonic clock.
    fn wall_clock(&self, now_ms: u64) -> OffsetDateTime {
        let millis = i64::try_from(now_ms.saturating_sub(self.start_ms)).unwrap_or(i64::MAX);
        self.started_at + time::Duration::milliseconds(millis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const START: OffsetDateTime = datetime!(2026-10-02 12:00 UTC);
    /// Time between two digits while typing an answer.
    const PER_DIGIT_MS: u64 = 10;

    fn settings(tables: &[u32], mode: TimerMode, seconds: u64) -> Settings {
        Settings {
            tables: tables.to_vec(),
            reverse: false,
            timer_mode: mode,
            seconds,
        }
    }

    /// One table of ten questions, so a test knows every expected answer.
    fn table_settings(mode: TimerMode, seconds: u64) -> Settings {
        settings(&[7], mode, seconds)
    }

    fn session(settings: &Settings) -> Session {
        Session::start_at("sess-1", START, &mut NoShuffle, settings, 0)
    }

    /// Types `answer` and presses Enter, and returns when it was submitted.
    fn answer(session: &mut Session, now_ms: u64, answer: u32) -> u64 {
        let mut time = now_ms;
        for digit in answer.to_string().chars() {
            session.on_key(&Key::Char(digit), time);
            time += PER_DIGIT_MS;
        }
        session.on_key(&Key::Enter, time);
        time
    }

    /// Skips the verdict without waiting for it to time out.
    fn skip(session: &mut Session, now_ms: u64) {
        session.on_key(&Key::Enter, now_ms);
    }

    #[test]
    fn a_fresh_session_starts_on_the_first_question() {
        let session = session(&table_settings(TimerMode::None, 10));

        assert_eq!(session.phase(), Phase::Running);
        assert!(!session.is_done());
        assert_eq!(session.status(), None);
        assert_eq!(session.question_count(), 10);
        assert_eq!(session.cursor(), 0);
        assert_eq!(session.current(), Some(Question::new(7, 1)));
        assert_eq!(session.input(), "");
        assert_eq!(session.question_text(), "7 × 1 = _");
        assert_eq!(session.remaining_ms(1_000), None);
        assert_eq!(session.stage(1_000), None);
    }

    #[test]
    fn the_session_is_shuffled_but_the_same_seed_shuffles_the_same_way() {
        let settings = settings(&[3, 7], TimerMode::None, 10);
        let first = Session::start_at("a", START, &mut SystemShuffler::seeded(7), &settings, 0);
        let again = Session::start_at("b", START, &mut SystemShuffler::seeded(7), &settings, 0);

        let order = |session: &Session| {
            session
                .questions
                .iter()
                .map(|question| format!("{} × {}", question.a, question.b))
                .collect::<Vec<String>>()
        };
        assert_eq!(order(&first), order(&again));
        assert_eq!(
            first.questions.len(),
            questions::generate(&settings.tables, settings.reverse).len()
        );

        // Without a seed the order is free, so only the questions are guaranteed.
        let third = Session::start_at("c", START, &mut NoShuffle, &settings, 0);
        assert_eq!(
            third.questions,
            questions::generate(&settings.tables, settings.reverse)
        );
    }

    #[test]
    fn a_correct_answer_scores_and_moves_to_the_next_question() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        let submitted = answer(&mut session, 0, 7);

        let q = session.questions[0];
        assert_eq!(
            session.phase(),
            Phase::Feedback {
                outcome: Outcome::Correct,
                question: q,
                submitted: Some(7)
            }
        );
        assert_eq!(session.correct_count(), 1);
        assert_eq!(session.cursor(), 0);
        // The typed answer stays on screen next to the equation that was just answered.
        assert_eq!(session.question_text(), "7 × 1 = 7");

        let records = session.take_pending();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].expected, 7);
        assert_eq!(records[0].given, Some(7));
        assert_eq!(records[0].outcome, Outcome::Correct);
        assert_eq!(records[0].response_ms, Some(submitted));
        assert_eq!(records[0].question_index, 0);
        assert_eq!(records[0].session_id, "sess-1");
        // Taken records are not handed out twice.
        assert!(session.take_pending().is_empty());
    }

    #[test]
    fn a_wrong_answer_is_wrong_but_still_recorded() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        answer(&mut session, 0, 3);

        let q = session.questions[0];
        assert_eq!(
            session.phase(),
            Phase::Feedback {
                outcome: Outcome::Incorrect,
                question: q,
                submitted: Some(3)
            }
        );
        assert_eq!(session.correct_count(), 0);
        assert_eq!(session.feedback_outcome(), Some(Outcome::Incorrect));
        let records = session.take_pending();
        assert_eq!(records[0].expected, 7);
        assert_eq!(records[0].given, Some(3));
        assert_eq!(records[0].outcome, Outcome::Incorrect);
    }

    #[test]
    fn digits_can_be_corrected_and_the_answer_is_capped() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        for digit in "700".chars() {
            session.on_key(&Key::Char(digit), 0);
        }
        assert_eq!(
            session.input(),
            "700",
            "meer dan drie cijfers past er niet in"
        );

        session.on_key(&Key::Backspace, 0);
        assert_eq!(session.input(), "70");
        session.on_key(&Key::Backspace, 0);
        session.on_key(&Key::Backspace, 0);
        assert_eq!(session.input(), "");
        // Backspace on an empty answer is harmless.
        session.on_key(&Key::Backspace, 0);
        assert_eq!(session.input(), "");

        // Only digits are accepted: letters and other keys are ignored.
        session.on_key(&Key::Char('a'), 0);
        session.on_key(&Key::Up, 0);
        session.on_key(&Key::Delete, 0);
        assert_eq!(session.input(), "");
    }

    #[test]
    fn enter_without_an_answer_does_nothing() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        session.on_key(&Key::Enter, 500);
        assert_eq!(session.phase(), Phase::Running);
        assert_eq!(session.cursor(), 0);
        assert!(session.take_pending().is_empty());
    }

    #[test]
    fn the_verdict_pauses_the_clock_and_then_the_next_question_starts() {
        let mut session = session(&table_settings(TimerMode::Total, 60));

        let first = answer(&mut session, 0, 7);
        // Five seconds of reading the verdict: none of it counts.
        session.on_tick(5_000);
        assert_eq!(session.phase(), Phase::Running);
        assert_eq!(session.cursor(), 1);
        assert_eq!(session.total_ms(5_000), first);
        assert_eq!(session.remaining_ms(5_000), Some(60_000 - first));

        let second = answer(&mut session, 5_000, 14);
        assert_eq!(session.total_ms(7_000), first + (second - 5_000));
        assert_eq!(
            session.remaining_ms(7_000),
            Some(60_000 - first - (second - 5_000))
        );
    }

    #[test]
    fn the_verdict_stays_up_for_a_second_and_a_half_and_can_be_skipped() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        let first = answer(&mut session, 0, 7);

        session.on_tick(first + FEEDBACK_MS - 1);
        assert_eq!(
            session.phase(),
            Phase::Feedback {
                outcome: Outcome::Correct,
                question: session.questions[0],
                submitted: Some(7)
            },
            "nog niet voorbij"
        );
        assert_eq!(
            session.feedback_elapsed_ms(first + FEEDBACK_MS - 1),
            Some(FEEDBACK_MS - 1)
        );

        session.on_tick(first + FEEDBACK_MS);
        assert_eq!(session.phase(), Phase::Running);
        assert_eq!(session.feedback_elapsed_ms(first + FEEDBACK_MS), None);

        // Skipping with a key ends it right away...
        let second = answer(&mut session, 2_000, 14);
        assert_eq!(
            session.phase(),
            Phase::Feedback {
                outcome: Outcome::Correct,
                question: session.questions[1],
                submitted: Some(14)
            }
        );
        session.on_key(&Key::Char(' '), 2_010);
        assert_eq!(session.phase(), Phase::Running);
        assert_eq!(session.cursor(), 2);

        // ...and while the verdict is up, a key that means nothing to it does
        // not skip: only Enter and Space do.
        let third = answer(&mut session, 3_000, 21);
        session.on_key(&Key::Char('7'), third + 10);
        session.on_key(&Key::Backspace, third + 20);
        assert_eq!(
            session.phase(),
            Phase::Feedback {
                outcome: Outcome::Correct,
                question: session.questions[2],
                submitted: Some(21)
            }
        );
        session.on_key(&Key::Enter, third + 30);
        assert_eq!(session.phase(), Phase::Running);
        let _ = second;
    }

    #[test]
    fn a_per_question_clock_starts_over_for_every_question() {
        let mut session = session(&table_settings(TimerMode::PerQuestion, 5));

        // Three seconds on the first question.
        session.on_tick(1_000);
        assert_eq!(session.remaining_ms(1_000), Some(4_000));
        assert_eq!(session.stage(1_000), Some(TimerStage::Ok));
        // Two fifths left is no longer green.
        session.on_tick(3_000);
        assert_eq!(session.remaining_ms(3_000), Some(2_000));
        assert_eq!(session.stage(3_000), Some(TimerStage::Warn));
        assert_eq!(session.cursor(), 0);

        let submitted = answer(&mut session, 3_000, 7);
        // The verdict itself does not eat into the next question's time.
        session.on_tick(submitted + FEEDBACK_MS);
        assert_eq!(session.cursor(), 1);
        assert_eq!(session.remaining_ms(submitted + FEEDBACK_MS), Some(5_000));
        assert_eq!(session.stage(submitted + FEEDBACK_MS), Some(TimerStage::Ok));
    }

    #[test]
    fn the_verdict_holds_the_whole_screen_still_and_then_gives_a_fresh_clock() {
        let mut session = session(&table_settings(TimerMode::PerQuestion, 5));

        // Two seconds and a bit into the first question, answered correctly.
        let submitted = answer(&mut session, 2_100, 7);
        let answered = session.question_text();
        let left = session.remaining_ms(submitted).expect("er is een klok");
        let stage = session.stage(submitted).expect("er is een klok");
        assert_eq!(
            Some(left),
            Some(5_000 - submitted),
            "vastgezet op het antwoordmoment"
        );

        // Reading the verdict does not move anything on screen: not the equation
        // with the submitted answer, not the position in the round, and not the
        // countdown that was ticking a moment ago.
        for moment in [1, 600, FEEDBACK_MS - 1] {
            let now = submitted + moment;
            session.on_tick(now);
            assert_eq!(
                session.question_text(),
                answered,
                "dezelfde som en hetzelfde antwoord op {now} ms"
            );
            assert_eq!(session.cursor(), 0, "de volgende vraag wacht op {now} ms");
            assert_eq!(
                session.remaining_ms(now),
                Some(left),
                "de countdown loopt niet door op {now} ms"
            );
            assert_eq!(session.stage(now), Some(stage), "op {now} ms");
        }

        // Then, and only then, the next question with a clock of its own.
        let next = submitted + FEEDBACK_MS;
        session.on_tick(next);
        assert_eq!(session.cursor(), 1);
        assert_eq!(session.question_text(), "7 × 2 = _");
        assert_eq!(session.remaining_ms(next), Some(5_000));
        assert_eq!(session.stage(next), Some(TimerStage::Ok));
    }

    #[test]
    fn a_timed_out_question_keeps_its_empty_clock_while_the_verdict_is_up() {
        let mut session = session(&table_settings(TimerMode::PerQuestion, 5));
        session.on_tick(5_000);

        assert_eq!(session.question_text(), "7 × 1 = -");
        assert_eq!(session.remaining_ms(5_000), Some(0));
        // The clock that ran out does not carry on into negative territory while
        // the learner reads the answer they should have given.
        let now = 5_000 + FEEDBACK_MS - 1;
        session.on_tick(now);
        assert_eq!(session.question_text(), "7 × 1 = -");
        assert_eq!(session.remaining_ms(now), Some(0));
        assert_eq!(session.stage(now), Some(TimerStage::Critical));
    }

    #[test]
    fn a_question_that_runs_out_of_time_is_recorded_as_too_late() {
        let mut session = session(&table_settings(TimerMode::PerQuestion, 5));
        session.on_tick(4_999);
        assert_eq!(session.phase(), Phase::Running, "nog net op tijd");

        session.on_tick(5_000);
        assert_eq!(
            session.phase(),
            Phase::Feedback {
                outcome: Outcome::TimedOut,
                question: session.questions[0],
                submitted: None
            }
        );
        assert_eq!(session.cursor(), 0);

        let records = session.take_pending();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].outcome, Outcome::TimedOut);
        assert_eq!(records[0].given, None);
        assert_eq!(records[0].response_ms, Some(5_000));
        // The verdict is the one that mentions the right answer.
        assert_eq!(session.question_text(), "7 × 1 = -");
    }

    #[test]
    fn the_total_clock_ends_the_session_and_leaves_the_rest_unasked() {
        let mut session = session(&table_settings(TimerMode::Total, 20));
        let submitted = answer(&mut session, 0, 7);
        session.on_tick(submitted + FEEDBACK_MS);
        assert_eq!(session.cursor(), 1);

        // Time is up: twenty seconds of answering, with the verdict in between
        // not counted, so the round ends 1_200 ms after the second question came.
        let end = submitted + FEEDBACK_MS + 20_000;
        session.on_tick(end);
        assert!(session.is_done());
        assert_eq!(session.status(), Some(SessionStatus::Completed));

        let records = session.take_pending();
        assert_eq!(records.len(), 10, "alle tien vragen staan in het log");
        assert_eq!(records[0].outcome, Outcome::Correct);
        assert_eq!(records[1].outcome, Outcome::TimedOut);
        assert_eq!(
            records[1].response_ms,
            Some(end - (submitted + FEEDBACK_MS))
        );
        assert!(
            records[2..]
                .iter()
                .all(|record| record.outcome == Outcome::NotReached)
        );
        assert!(
            records[2..]
                .iter()
                .all(|record| record.response_ms.is_none())
        );
        assert!(
            records[2..]
                .iter()
                .all(|record| record.timestamp == records[1].timestamp)
        );
        assert_eq!(session.correct_count(), 1);
    }

    #[test]
    fn a_stopwatch_never_expires_however_long_it_runs() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        session.on_tick(u64::MAX);
        assert_eq!(session.phase(), Phase::Running);
        assert!(!session.is_expired(u64::MAX));
        assert_eq!(session.stage(u64::MAX), None);
        assert_eq!(session.remaining_fraction(u64::MAX), None);
        assert_eq!(session.total_ms(u64::MAX), u64::MAX);
    }

    #[test]
    fn the_session_ends_after_the_last_question() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        for index in 0..10_u32 {
            assert_eq!(session.cursor(), index as usize);
            // 7 × 1 up to 7 × 10, all right.
            let submitted = answer(&mut session, index as u64 * 100, 7 * (index + 1));
            skip(&mut session, submitted);
        }

        assert!(session.is_done());
        assert_eq!(session.status(), Some(SessionStatus::Completed));
        assert_eq!(session.correct_count(), 10);
        assert_eq!(session.answers().len(), 10);
        assert_eq!(session.take_pending().len(), 10);
        // Nothing on screen once the session is over.
        assert_eq!(session.question_text(), "");
        assert_eq!(session.current(), None);
        // Keys after the end change nothing.
        session.on_key(&Key::Enter, 10_000);
        assert!(session.is_done());
    }

    #[test]
    fn stopping_early_keeps_the_answers_that_were_already_given() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        let first = answer(&mut session, 0, 7);
        session.on_tick(first + FEEDBACK_MS);
        let second = answer(&mut session, first + FEEDBACK_MS, 13);
        assert_eq!(session.take_pending().len(), 2);

        session.abandon(second + 100);
        assert!(session.is_done());
        assert_eq!(session.status(), Some(SessionStatus::Abandoned));

        // Only what really happened is written.
        assert!(
            session.take_pending().is_empty(),
            "er viel niets nieuws te schrijven"
        );
        assert_eq!(session.answers().len(), 2);
        assert_eq!(session.correct_count(), 1);

        let record = session.to_record().expect("de sessie is klaar");
        assert_eq!(record.question_count, 10, "de sessie telt alle tien vragen");
        assert_eq!(record.correct_count, 1);
        assert_eq!(record.status, SessionStatus::Abandoned);
    }

    #[test]
    fn the_score_and_the_session_record_match_what_happened() {
        let mut session = session(&table_settings(TimerMode::None, 10));
        let end = 10_000;
        let first = answer(&mut session, 0, 7);
        session.on_tick(first + FEEDBACK_MS);
        let second = answer(&mut session, first + FEEDBACK_MS, 99);
        session.abandon(end);

        let summary = session.summary(end);
        assert_eq!(summary.total, 10);
        assert_eq!(summary.correct, 1);
        assert_eq!(summary.accuracy_percent(), 10);
        assert_eq!(summary.mistakes().len(), 1);
        assert_eq!(summary.mistakes()[0].given, Some(99));

        let record = session.to_record().expect("de sessie is klaar");
        assert_eq!(record.schema, 1);
        assert_eq!(record.session_id, "sess-1");
        assert_eq!(record.status, SessionStatus::Abandoned);
        assert_eq!(record.settings.tables, vec![7]);
        assert_eq!(record.settings.timer_mode, TimerMode::None);
        assert_eq!(record.settings.limit_seconds, None);
        assert_eq!(record.correct_count, 1);
        assert_eq!(record.question_count, 10);
        // Two answers; the verdict in between does not count, however long the
        // learner took to read it.
        assert_eq!(record.total_ms, first + (second - (first + FEEDBACK_MS)));
        // Wall-clock timestamps follow the monotonic ones.
        assert_eq!(record.started_at, START);
        assert_eq!(
            record.ended_at,
            START + time::Duration::milliseconds(i64::try_from(end).unwrap())
        );
    }

    #[test]
    fn the_average_reaction_time_ignores_questions_that_were_never_asked() {
        let mut session = session(&table_settings(TimerMode::Total, 2));
        let first = answer(&mut session, 0, 7);
        session.on_tick(first + FEEDBACK_MS);
        let second = answer(&mut session, 3_000, 14);
        session.on_tick(second + FEEDBACK_MS);
        assert_eq!(session.cursor(), 2, "de derde vraag staat op");

        // The two seconds of answering time are spent, so the round stops with
        // the third question on screen.
        let end = second + FEEDBACK_MS + 2_000;
        session.on_tick(end);
        assert!(session.is_done());

        let summary = session.summary(end);
        assert_eq!(summary.total, 10);
        assert_eq!(summary.correct, 2);
        assert_eq!(summary.accuracy_percent(), 20);
        // 10 ms and 1810 ms of answering; the verdict in between does not count,
        // and neither do the questions that were never shown.
        assert_eq!(
            summary.total_ms,
            first + (second - (first + FEEDBACK_MS)) + 2_000
        );
        // Three questions reached an answer: two typed ones and the one that
        // used up its two seconds. The seven never shown have no reaction time.
        assert_eq!(
            summary.average_response_ms(),
            Some((10 + 1_810 + 2_000) / 3)
        );

        // The question on screen ran out; the seven behind it never came up.
        let mistakes = summary.mistakes();
        assert_eq!(mistakes.len(), 8);
        assert_eq!(mistakes[0].outcome, Outcome::TimedOut);
        assert_eq!(mistakes[0].response_ms, Some(2_000));
        assert!(
            mistakes[1..]
                .iter()
                .all(|record| record.outcome == Outcome::NotReached)
        );
        assert!(
            mistakes[1..]
                .iter()
                .all(|record| record.response_ms.is_none())
        );
    }

    #[test]
    fn an_empty_session_is_done_before_it_starts() {
        let mut session = session(&Settings::default());
        assert!(session.is_done());
        assert_eq!(session.question_count(), 0);
        assert_eq!(session.correct_count(), 0);
        assert_eq!(session.summary(0).accuracy_percent(), 0);
        assert_eq!(session.summary(0).average_response_ms(), None);
        assert!(session.take_pending().is_empty());
        assert!(
            session.to_record().is_some(),
            "een lege sessie is wel afgelopen"
        );
    }

    #[test]
    fn a_new_session_gets_its_own_id_and_the_records_follow_it() {
        let settings = table_settings(TimerMode::None, 10);
        let mut session = Session::start(&mut SystemShuffler::new(), &settings, 0);
        assert!(!session.id().is_empty());
        answer(&mut session, 0, 7);
        assert_eq!(session.take_pending()[0].session_id, session.id());
    }
}
