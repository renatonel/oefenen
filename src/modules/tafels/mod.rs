//! The tafels module: pick your tables, pick your time, answer as many as you
//! can, and see what still needs practising.
//!
//! The screens are a small state machine, and the drill itself lives in
//! [`session`], which knows nothing about drawing, keys or storage. This module
//! only connects the three: it routes events, hands the session the current time,
//! and writes what the session produced.
//!
//! Time is the one thing that needs care here. The application hands out a
//! monotonic millisecond counter since the module was entered, and the session
//! takes that number as an argument. Pausing the clock (while the stop question
//! is on screen) is therefore just remembering where the counter stood; see
//! [`TafelsModule::pause_clock`].

pub mod bigfont;
pub mod questions;
pub mod records;
pub mod session;
pub mod settings;
pub mod timer;
pub mod ui;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::event::{Event, Key};
use crate::i18n::tafels;
use crate::modules::{Module, ModuleContext, Subject, Transition};
use crate::ui::hints::KeyHint;

use self::records::{ANSWERS_FILE, SESSIONS_FILE};
use self::session::{Session, SystemShuffler};
use self::settings::{MAX_CUSTOM_DIGITS, Settings};
use self::timer::TimerMode;
use self::ui::{CUSTOM_ROW, ResultRow, TableRow, TimerRow};

/// Stable id; also the name of this module's directory in the data folder.
pub const TAFELS_ID: &str = "tafels";

/// Which screen is on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// Choosing the tables.
    Tables,
    /// Choosing the time.
    Timer,
    /// Answering questions.
    Session,
    /// The stop question, on top of the session.
    StopConfirm,
    /// The score and what to practise next.
    Results,
}

/// Multiplication tables, with a clock, for Dutch primary school children.
pub struct TafelsModule {
    screen: Screen,
    /// The choices being edited; written to `settings.toml` when a round starts.
    settings: Settings,
    /// Cursor on the table screen.
    table_cursor: usize,
    /// Cursor on the timer screen.
    timer_cursor: usize,
    /// Cursor on the results screen.
    result_cursor: usize,
    /// The custom time as typed, validated only when it is chosen.
    custom_input: String,
    /// Whether the stop question is on "ja, stoppen".
    stop_yes: bool,
    /// A message that belongs on the current screen, such as an unusable time.
    warning: Option<String>,
    /// The running (or finished) session.
    session: Option<Session>,
    shuffler: SystemShuffler,
    /// Whether the finished session's line is already in `sessions.jsonl`.
    session_written: bool,
    /// Milliseconds the session's clock is behind the application's, because it
    /// was paused.
    clock_offset_ms: u64,
    /// While set, the session clock stands still at this value.
    clock_stopped_at: Option<u64>,
}

impl Default for TafelsModule {
    fn default() -> Self {
        Self {
            screen: Screen::Tables,
            settings: Settings::default(),
            table_cursor: 0,
            timer_cursor: 0,
            result_cursor: 0,
            custom_input: String::new(),
            stop_yes: false,
            warning: None,
            session: None,
            shuffler: SystemShuffler::new(),
            session_written: false,
            clock_offset_ms: 0,
            clock_stopped_at: None,
        }
    }
}

impl Module for TafelsModule {
    fn id(&self) -> &'static str {
        TAFELS_ID
    }

    fn title(&self) -> String {
        tafels::TITLE.to_string()
    }

    fn description(&self) -> String {
        tafels::DESCRIPTION.to_string()
    }

    fn subject(&self) -> Subject {
        Subject::Math
    }

    /// Starts on the table screen, with the choices of last time.
    fn on_enter(&mut self, ctx: &mut ModuleContext) {
        self.settings = settings::load(ctx.store());
        self.table_cursor = 0;
        self.timer_cursor = 0;
        self.result_cursor = 0;
        self.custom_input.clear();
        self.warning = None;
        self.stop_yes = false;
        self.session = None;
        self.session_written = false;
        self.clock_offset_ms = 0;
        self.clock_stopped_at = None;
        self.screen = Screen::Tables;
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut ModuleContext) -> Transition {
        match event {
            Event::Key(key) => self.on_key(key, ctx),
            Event::Tick => {
                let now_ms = self.now(ctx);
                let mut finished = false;
                if let Some(session) = self.session.as_mut() {
                    // A tick is what moves the clock on: no event, no time.
                    session.on_tick(now_ms);
                    finished = session.is_done();
                }
                self.flush(ctx);
                if finished {
                    self.screen = Screen::Results;
                }
                Transition::Stay
            }
            // Nothing to do: every screen is drawn from the area it is given.
            Event::Resize { .. } => Transition::Stay,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &ModuleContext) {
        let now_ms = self.now(ctx);
        match self.screen {
            Screen::Tables => ui::render_tables(
                frame,
                area,
                &self.settings,
                self.table_cursor,
                self.warning.as_deref(),
            ),
            Screen::Timer => ui::render_timer(
                frame,
                area,
                &self.settings,
                self.timer_cursor,
                &self.custom_input,
                self.warning.as_deref(),
            ),
            Screen::Session | Screen::StopConfirm => {
                if let Some(session) = self.session.as_ref() {
                    ui::render_session(frame, area, session, now_ms);
                }
                if self.screen == Screen::StopConfirm {
                    ui::render_stop_confirm(frame, area, self.stop_yes);
                }
            }
            Screen::Results => {
                if let Some(session) = self.session.as_ref() {
                    ui::render_results(frame, area, session, self.result_cursor, now_ms);
                }
            }
        }
    }

    fn hints(&self) -> Vec<KeyHint> {
        match self.screen {
            Screen::Tables => ui::table_hints(),
            Screen::Timer => ui::timer_hints(),
            Screen::Session => ui::session_hints(),
            Screen::StopConfirm => ui::stop_hints(),
            Screen::Results => ui::result_hints(),
        }
    }

    /// Stops a running session and keeps the choices for next time.
    fn on_exit(&mut self, ctx: &mut ModuleContext) {
        self.stop_session(ctx);
        if let Err(error) = settings::save(ctx.store(), &self.settings) {
            ctx.report_error(error.to_string());
        }
    }
}

impl TafelsModule {
    /// The session's clock: the application's counter minus what was paused.
    ///
    /// While the stop question is on screen the clock stands still: it keeps
    /// returning the moment it was frozen at, so however long the learner takes
    /// to decide, none of it counts against them.
    fn now(&self, ctx: &ModuleContext) -> u64 {
        match self.clock_stopped_at {
            Some(frozen) => frozen,
            None => ctx.elapsed_millis().saturating_sub(self.clock_offset_ms),
        }
    }

    /// Freezes the session clock where it stands right now.
    fn pause_clock(&mut self, ctx: &ModuleContext) {
        self.clock_stopped_at = Some(self.now(ctx));
    }

    /// Lets the clock carry on from where it was frozen, not from here.
    ///
    /// The time spent deciding becomes part of the offset, so the session never
    /// sees a jump.
    fn resume_clock(&mut self, ctx: &ModuleContext) {
        if let Some(frozen) = self.clock_stopped_at.take() {
            self.clock_offset_ms = ctx.elapsed_millis().saturating_sub(frozen);
        }
    }

    // --- keys -------------------------------------------------------------

    fn on_key(&mut self, key: &Key, ctx: &mut ModuleContext) -> Transition {
        // Ctrl-C always works, even mid-round: stopping must always be possible.
        if *key == Key::CtrlC {
            self.stop_session(ctx);
            return Transition::Exit;
        }

        match self.screen {
            Screen::Tables => self.on_key_tables(key),
            // The timer screen cannot leave the module, so it stays whatever the
            // other screens say; the same holds for a running round.
            Screen::Timer => {
                self.on_key_timer(key, ctx);
                Transition::Stay
            }
            Screen::Session => {
                self.on_key_session(key, ctx);
                Transition::Stay
            }
            Screen::StopConfirm => {
                self.on_key_stop_confirm(key, ctx);
                Transition::Stay
            }
            Screen::Results => self.on_key_results(key, ctx),
        }
    }

    fn on_key_tables(&mut self, key: &Key) -> Transition {
        let rows = ui::table_rows();
        match key {
            Key::Up | Key::Down => {
                let step: isize = if *key == Key::Down { 1 } else { -1 };
                self.table_cursor = move_cursor(self.table_cursor, step, rows.len());
                self.warning = None;
            }
            Key::Home => self.table_cursor = 0,
            Key::End => self.table_cursor = rows.len() - 1,
            Key::Char('a' | 'A') => {
                self.settings.tables = (questions::TABLE_MIN..=questions::TABLE_MAX).collect();
                self.warning = None;
            }
            Key::Char('u' | 'U') => {
                self.settings.tables.clear();
                self.warning = None;
            }
            Key::Char(digit) if digit.is_ascii_digit() => {
                // The number keys are the shortcut for the check box above them.
                if let Some(table) = ui::digit_of(key)
                    && (questions::TABLE_MIN..=questions::TABLE_MAX).contains(&table)
                {
                    self.toggle_table(table);
                }
            }
            Key::Enter | Key::Char(' ') => match rows[self.table_cursor.min(rows.len() - 1)] {
                TableRow::Table(table) => self.toggle_table(table),
                TableRow::Reverse => {
                    self.settings.reverse = !self.settings.reverse;
                    self.warning = None;
                }
                TableRow::Start => {
                    if self.settings.tables.is_empty() {
                        self.warning = Some(tafels::NO_TABLES.to_string());
                    } else {
                        self.timer_cursor = 0;
                        self.custom_input.clear();
                        self.warning = None;
                        self.screen = Screen::Timer;
                    }
                }
            },
            // The questions never shown are not written at all.
            Key::Esc => return Transition::Exit,
            _ => {}
        }
        Transition::Stay
    }

    fn on_key_timer(&mut self, key: &Key, ctx: &mut ModuleContext) {
        let rows = ui::timer_rows();
        match key {
            Key::Up | Key::Down => {
                let step: isize = if *key == Key::Down { 1 } else { -1 };
                self.timer_cursor = move_cursor(self.timer_cursor, step, rows.len());
            }
            Key::Home => self.timer_cursor = 0,
            Key::End => self.timer_cursor = rows.len() - 1,
            Key::Char(digit) if digit.is_ascii_digit() => {
                // Only the custom field takes digits: elsewhere they would be a
                // shortcut into a time nobody chose.
                if self.timer_cursor == CUSTOM_ROW && self.custom_input.len() < MAX_CUSTOM_DIGITS {
                    self.custom_input.push(*digit);
                }
            }
            Key::Backspace => {
                self.custom_input.pop();
            }
            Key::Enter | Key::Char(' ') => self.choose_time(ctx),
            Key::Esc => {
                self.warning = None;
                self.custom_input.clear();
                self.screen = Screen::Tables;
            }
            _ => {}
        }
    }

    /// Applies the highlighted time and starts the round.
    fn choose_time(&mut self, ctx: &mut ModuleContext) {
        let row = ui::timer_rows()
            .get(self.timer_cursor)
            .copied()
            .unwrap_or(TimerRow::None);
        let questions = self.settings.question_count();

        let (mode, seconds) = match row {
            TimerRow::PerQuestion(index) => (
                TimerMode::PerQuestion,
                settings::per_question_presets()[index],
            ),
            // A speed is seconds per question, multiplied out over the whole round.
            TimerRow::Speed(index) => (
                TimerMode::Total,
                settings::total_for_speed(settings::total_speeds()[index], questions),
            ),
            TimerRow::None => (TimerMode::None, self.settings.seconds),
            TimerRow::Custom => match settings::parse_custom_time(&self.custom_input) {
                Some(seconds) => (self.custom_mode(), seconds),
                None => {
                    // Nothing starts on an unusable time: the learner stays here
                    // and can correct it.
                    self.warning = Some(tafels::INVALID_TIME.to_string());
                    return;
                }
            },
        };

        self.settings.timer_mode = mode;
        self.settings.seconds = seconds;
        self.custom_input.clear();
        self.warning = None;
        self.start_session(ctx);
    }

    /// The mode a custom time belongs to: the one already chosen, or a countdown
    /// per question.
    fn custom_mode(&self) -> TimerMode {
        match self.settings.timer_mode {
            TimerMode::None => TimerMode::PerQuestion,
            mode => mode,
        }
    }

    fn on_key_session(&mut self, key: &Key, ctx: &mut ModuleContext) {
        if *key == Key::Esc {
            self.stop_yes = false;
            self.pause_clock(ctx);
            self.screen = Screen::StopConfirm;
            return;
        }

        let now_ms = self.now(ctx);
        let mut finished = false;
        if let Some(session) = self.session.as_mut() {
            session.on_key(key, now_ms);
            finished = session.is_done();
        }
        self.flush(ctx);
        if finished {
            self.screen = Screen::Results;
        }
    }

    fn on_key_stop_confirm(&mut self, key: &Key, ctx: &mut ModuleContext) {
        match key {
            Key::Left | Key::Right | Key::Tab | Key::BackTab => self.stop_yes = !self.stop_yes,
            Key::Enter | Key::Char(' ') => {
                if self.stop_yes {
                    self.stop_session(ctx);
                    self.screen = Screen::Results;
                } else {
                    self.resume(ctx);
                }
            }
            Key::Esc => self.resume(ctx),
            _ => {}
        }
    }

    /// Closes the stop question; the clock carries on from where it stopped.
    fn resume(&mut self, ctx: &mut ModuleContext) {
        self.resume_clock(ctx);
        self.screen = Screen::Session;
    }

    fn on_key_results(&mut self, key: &Key, ctx: &mut ModuleContext) -> Transition {
        let rows = ui::result_rows();
        match key {
            Key::Up | Key::Down => {
                let step: isize = if *key == Key::Down { 1 } else { -1 };
                self.result_cursor = move_cursor(self.result_cursor, step, rows.len());
            }
            Key::Enter | Key::Char(' ') => {
                match rows
                    .get(self.result_cursor)
                    .copied()
                    .unwrap_or(ResultRow::Menu)
                {
                    ResultRow::Replay => self.start_session(ctx),
                    ResultRow::Settings => {
                        self.session = None;
                        self.session_written = false;
                        self.table_cursor = 0;
                        self.warning = None;
                        self.screen = Screen::Tables;
                    }
                    ResultRow::Menu => {
                        self.stop_session(ctx);
                        return Transition::Exit;
                    }
                }
            }
            Key::Esc => {
                self.stop_session(ctx);
                return Transition::Exit;
            }
            _ => {}
        }
        Transition::Stay
    }

    /// Toggles one table on or off, keeping the list sorted and free of
    /// duplicates.
    fn toggle_table(&mut self, table: u32) {
        match self
            .settings
            .tables
            .iter()
            .position(|other| *other == table)
        {
            Some(index) => {
                self.settings.tables.remove(index);
            }
            None => self.settings.tables.push(table),
        }
        self.settings.tables.sort_unstable();
        self.warning = None;
    }

    // --- starting and writing ---------------------------------------------

    /// Starts a round with the settings as they are now.
    ///
    /// The choices are written first: a session whose settings are lost would be
    /// impossible to interpret later.
    fn start_session(&mut self, ctx: &mut ModuleContext) {
        if self.settings.tables.is_empty() {
            self.warning = Some(tafels::NO_TABLES.to_string());
            self.screen = Screen::Tables;
            return;
        }
        if let Err(error) = settings::save(ctx.store(), &self.settings) {
            ctx.report_error(error.to_string());
        }
        let now_ms = self.now(ctx);
        let session = Session::start(&mut self.shuffler, &self.settings, now_ms);
        self.session = Some(session);
        self.session_written = false;
        self.warning = None;
        self.custom_input.clear();
        self.screen = Screen::Session;
        self.flush(ctx);
    }

    /// Stops the running session, if it is still running.
    fn stop_session(&mut self, ctx: &mut ModuleContext) {
        let now_ms = self.now(ctx);
        if let Some(session) = self.session.as_mut() {
            if !session.is_done() {
                session.abandon(now_ms);
            }
        }
        self.flush(ctx);
    }

    /// Writes everything the session produced since the last call.
    ///
    /// Answers go first, one by one, because a question that has been answered
    /// has to survive a crash; the session line is written once, at the end.
    fn flush(&mut self, ctx: &mut ModuleContext) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let pending = session.take_pending();
        let finished = if session.is_done() && !self.session_written {
            session.to_record()
        } else {
            None
        };

        for record in pending {
            if let Err(error) = ctx.store().append_to(ANSWERS_FILE, &record) {
                ctx.report_error(error.to_string());
            }
        }
        if let Some(record) = finished {
            match ctx.store().append_to(SESSIONS_FILE, &record) {
                // Only call it written once the file really has it.
                Ok(_) => self.session_written = true,
                Err(error) => ctx.report_error(error.to_string()),
            }
        }
    }
}

/// Moves a cursor by `step` rows, wrapping around at both ends.
fn move_cursor(cursor: usize, step: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = isize::try_from(len).unwrap_or(isize::MAX);
    let position = isize::try_from(cursor).unwrap_or(0);
    usize::try_from((position + step).rem_euclid(len)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Grade, ProfileId};
    use crate::storage::ModuleStore;
    use crate::storage::ProfileStore;
    use tempfile::TempDir;
    use time::macros::datetime;

    /// A module with a store of its own, in a throwaway data directory.
    struct Harness {
        module: TafelsModule,
        ctx: ModuleContext,
        dir: TempDir,
    }

    impl Harness {
        fn new() -> Self {
            let dir = TempDir::new().expect("tijdelijke map");
            let profile = ProfileStore::new(dir.path())
                .create_at(
                    "Sem",
                    Some(Grade::new(4).expect("groep 4")),
                    datetime!(2026-10-02 12:00 UTC),
                )
                .expect("profiel aangemaakt");
            let store = ModuleStore::new(dir.path(), profile.id(), TAFELS_ID).expect("modulemap");
            let mut module = TafelsModule::default();
            let mut ctx = ModuleContext::new(profile, store);
            module.on_enter(&mut ctx);
            Self { module, ctx, dir }
        }

        /// Sends one key to the module.
        fn press(&mut self, key: Key) -> Transition {
            self.module.handle_event(&Event::Key(key), &mut self.ctx)
        }

        /// Sends one tick.
        fn tick(&mut self) {
            self.module.handle_event(&Event::Tick, &mut self.ctx);
        }

        /// Lets `millis` pass on the application's clock.
        fn advance(&mut self, millis: u64) {
            self.ctx.advance(std::time::Duration::from_millis(millis));
        }

        /// A second handle on the same directory, to read what was written.
        fn store(&self) -> ModuleStore {
            ModuleStore::new(self.dir.path(), self.profile_id(), TAFELS_ID).expect("modulemap")
        }

        fn profile_id(&self) -> &ProfileId {
            self.ctx.profile().id()
        }

        /// The settings as they are on disk.
        fn stored_settings(&self) -> Option<settings::SettingsFile> {
            self.store()
                .read_document(settings::SETTINGS_FILE)
                .expect("instellingen lezen")
        }

        fn answers(&self) -> Vec<records::AnswerRecord> {
            self.store().read_from_lossy(ANSWERS_FILE)
        }

        fn sessions(&self) -> Vec<records::SessionRecord> {
            self.store().read_from_lossy(SESSIONS_FILE)
        }

        /// Chooses table 7 and walks to the timer screen.
        fn move_to_timer(&mut self) {
            self.press(Key::Char('7'));
            assert_eq!(self.module.settings.tables, vec![7]);
            self.press(Key::End);
            self.press(Key::Enter);
            assert_eq!(self.module.screen, Screen::Timer);
        }

        /// Chooses a time and lands in the session.
        fn move_to_session(&mut self) {
            self.move_to_timer();
            // The first row is five seconds per question.
            self.press(Key::Enter);
            assert_eq!(self.module.screen, Screen::Session);
            assert!(self.module.session.is_some());
        }

        /// Answers the question on screen with its right answer, through the
        /// module, and skips the verdict.
        fn answer_next(&mut self) {
            let expected = self
                .module
                .session
                .as_ref()
                .and_then(Session::current)
                .map(|question| question.expected())
                .expect("er is een vraag");
            for digit in expected.to_string().chars() {
                self.press(Key::Char(digit));
            }
            self.press(Key::Enter);
            self.press(Key::Enter);
        }
    }

    #[test]
    fn the_module_is_a_tafels_module_under_rekenen() {
        let module = TafelsModule::default();
        assert_eq!(module.id(), "tafels");
        assert_eq!(module.title(), "Tafels oefenen");
        assert!(!module.description().is_empty());
        assert_eq!(module.subject(), Subject::Math);
    }

    #[test]
    fn entering_the_module_starts_on_the_table_screen() {
        let harness = Harness::new();
        assert_eq!(harness.module.screen, Screen::Tables);
        assert!(harness.module.settings.tables.is_empty());
        assert!(harness.module.session.is_none());
        assert_eq!(harness.module.hints(), ui::table_hints());
    }

    #[test]
    fn tables_are_toggled_by_a_number_key_and_by_space() {
        let mut harness = Harness::new();

        // The number keys are the shortcut for the check box with that number.
        harness.press(Key::Char('7'));
        harness.press(Key::Char('3'));
        assert_eq!(harness.module.settings.tables, vec![3, 7]);

        // Space on the row of table 3 turns it off again.
        harness.module.table_cursor = 2;
        harness.press(Key::Char(' '));
        assert_eq!(harness.module.settings.tables, vec![7]);

        // And on again, in its place in the sorted list.
        harness.press(Key::Char(' '));
        assert_eq!(harness.module.settings.tables, vec![3, 7]);

        // A third press turns it off again, leaving no duplicate behind.
        harness.press(Key::Char(' '));
        assert_eq!(harness.module.settings.tables, vec![7]);

        // Zero is table ten: there is no table zero.
        harness.press(Key::Char('0'));
        assert!(harness.module.settings.tables.contains(&10));
        // And a letter is not a table at all.
        harness.press(Key::Char('x'));
        assert!(!harness.module.settings.tables.contains(&11));
    }

    #[test]
    fn a_and_u_fill_and_empty_the_list_of_tables() {
        let mut harness = Harness::new();

        harness.press(Key::Char('a'));
        assert_eq!(harness.module.settings.tables.len(), 10);
        assert_eq!(harness.module.settings.tables.first(), Some(&1));
        assert_eq!(harness.module.settings.tables.last(), Some(&10));

        harness.press(Key::Char('u'));
        assert!(harness.module.settings.tables.is_empty());
    }

    #[test]
    fn a_round_cannot_start_without_a_table() {
        let mut harness = Harness::new();
        harness.module.table_cursor = ui::table_rows().len() - 1;
        harness.press(Key::Enter);

        assert_eq!(
            harness.module.screen,
            Screen::Tables,
            "geen timer zonder tafel"
        );
        assert_eq!(harness.module.warning.as_deref(), Some(tafels::NO_TABLES));
        assert!(harness.module.session.is_none());

        // Choosing a table clears the complaint.
        harness.press(Key::Char('7'));
        assert_eq!(harness.module.warning, None);
    }

    #[test]
    fn the_cursor_wraps_around_and_can_jump_to_the_ends() {
        let mut harness = Harness::new();
        let rows = ui::table_rows().len();

        harness.press(Key::Up);
        assert_eq!(harness.module.table_cursor, rows - 1, "omhoog vanaf boven");
        harness.press(Key::Down);
        assert_eq!(harness.module.table_cursor, 0, "omlaag vanaf onder");

        harness.press(Key::End);
        assert_eq!(harness.module.table_cursor, rows - 1);
        harness.press(Key::Home);
        assert_eq!(harness.module.table_cursor, 0);
    }

    #[test]
    fn a_preset_time_starts_the_round_right_away() {
        let mut harness = Harness::new();
        harness.move_to_session();

        assert_eq!(harness.module.settings.timer_mode, TimerMode::PerQuestion);
        assert_eq!(harness.module.settings.seconds, 5);
        assert_eq!(
            harness
                .module
                .session
                .as_ref()
                .expect("sessie")
                .timer()
                .mode(),
            TimerMode::PerQuestion
        );
        assert_eq!(harness.stored_settings().expect("opgeslagen").seconds, 5);
    }

    #[test]
    fn a_total_speed_becomes_a_countdown_over_the_whole_round() {
        let mut harness = Harness::new();
        harness.move_to_timer();

        // The first speed row: four seconds per question over ten questions.
        harness.module.timer_cursor = 4;
        harness.press(Key::Enter);

        assert_eq!(harness.module.screen, Screen::Session);
        assert_eq!(harness.module.settings.timer_mode, TimerMode::Total);
        assert_eq!(harness.module.settings.seconds, 40);
    }

    #[test]
    fn a_custom_time_is_typed_and_only_accepted_when_it_makes_sense() {
        let mut harness = Harness::new();
        harness.move_to_timer();
        harness.module.timer_cursor = CUSTOM_ROW;

        // A letter is not a time.
        harness.press(Key::Char('a'));
        harness.press(Key::Enter);
        assert_eq!(harness.module.screen, Screen::Timer);
        assert_eq!(
            harness.module.warning.as_deref(),
            Some(tafels::INVALID_TIME)
        );
        assert!(harness.module.session.is_none());

        // Four digits are minutes and seconds.
        for digit in "0130".chars() {
            harness.press(Key::Char(digit));
        }
        assert_eq!(harness.module.custom_input, "0130");
        harness.press(Key::Enter);
        assert_eq!(harness.module.screen, Screen::Session);
        assert_eq!(harness.module.settings.seconds, 90);

        // Backspace fixes a typo instead of losing the whole screen.
        harness.module.screen = Screen::Timer;
        harness.module.timer_cursor = CUSTOM_ROW;
        harness.module.custom_input = "0130".to_string();
        harness.press(Key::Backspace);
        assert_eq!(harness.module.custom_input, "013");
        harness.press(Key::Char('0'));
        assert_eq!(harness.module.custom_input, "0130");
    }

    #[test]
    fn the_custom_field_takes_at_most_four_digits() {
        let mut harness = Harness::new();
        harness.move_to_timer();
        harness.module.timer_cursor = CUSTOM_ROW;

        for digit in "123456".chars() {
            harness.press(Key::Char(digit));
        }
        assert_eq!(harness.module.custom_input, "1234");
    }

    #[test]
    fn digits_are_only_typed_on_the_custom_row() {
        let mut harness = Harness::new();
        harness.move_to_timer();

        harness.module.timer_cursor = 0;
        harness.press(Key::Char('5'));
        assert!(harness.module.custom_input.is_empty());
        assert_eq!(harness.module.timer_cursor, 0, "een cijfer kiest geen tijd");
    }

    #[test]
    fn the_timer_screen_can_be_left_without_starting_anything() {
        let mut harness = Harness::new();
        harness.move_to_timer();
        harness.press(Key::Esc);

        assert_eq!(harness.module.screen, Screen::Tables);
        assert!(harness.module.session.is_none());
    }

    #[test]
    fn every_answer_is_written_the_moment_it_is_known() {
        let mut harness = Harness::new();
        harness.move_to_session();
        assert_eq!(harness.store().count_in(ANSWERS_FILE), 0);

        let expected = harness
            .module
            .session
            .as_ref()
            .and_then(Session::current)
            .map(|question| question.expected())
            .expect("vraag");
        for digit in expected.to_string().chars() {
            harness.press(Key::Char(digit));
        }
        assert_eq!(
            harness.store().count_in(ANSWERS_FILE),
            0,
            "nog niet ingediend"
        );

        harness.press(Key::Enter);
        assert_eq!(
            harness.store().count_in(ANSWERS_FILE),
            1,
            "meteen geschreven"
        );
        assert_eq!(
            harness.store().count_in(SESSIONS_FILE),
            0,
            "de ronde loopt nog"
        );

        // A wrong answer next.
        harness.press(Key::Enter);
        harness.press(Key::Char('1'));
        harness.press(Key::Enter);
        assert_eq!(harness.store().count_in(ANSWERS_FILE), 2);

        let answers = harness.answers();
        assert_eq!(answers[0].outcome, records::Outcome::Correct);
        assert_eq!(answers[1].outcome, records::Outcome::Incorrect);
        assert_eq!(answers[0].session_id, answers[1].session_id);
        assert_eq!(answers[0].question_index, 0);
        assert_eq!(answers[1].question_index, 1);
    }

    #[test]
    fn a_finished_round_shows_the_score_and_writes_one_session_line() {
        let mut harness = Harness::new();
        harness.move_to_session();

        // Ten questions, all answered through the module itself; the module has
        // to pick the result up on its next tick.
        for _ in 0..10 {
            let expected = harness
                .module
                .session
                .as_ref()
                .and_then(Session::current)
                .map(|question| question.expected())
                .expect("er is een vraag");
            for digit in expected.to_string().chars() {
                harness.press(Key::Char(digit));
            }
            harness.press(Key::Enter);
            harness.press(Key::Enter);
        }
        assert!(harness.module.session.as_ref().expect("sessie").is_done());
        // The module turns to the results screen itself, as soon as it sees the
        // round is over.
        assert_eq!(harness.module.screen, Screen::Results);

        assert_eq!(harness.store().count_in(ANSWERS_FILE), 10);
        assert_eq!(harness.store().count_in(SESSIONS_FILE), 1);

        let sessions = harness.sessions();
        assert_eq!(sessions[0].status, records::SessionStatus::Completed);
        assert_eq!(sessions[0].correct_count, 10);
        assert_eq!(sessions[0].question_count, 10);
        assert_eq!(sessions[0].settings.tables, vec![7]);
        assert_eq!(sessions[0].settings.limit_seconds, Some(5));
        assert_eq!(harness.module.hints(), ui::result_hints());

        // A second tick does not write the same session line twice.
        harness.tick();
        assert_eq!(harness.store().count_in(SESSIONS_FILE), 1);
    }

    #[test]
    fn the_results_screen_can_replay_change_the_settings_or_leave() {
        let mut harness = Harness::new();
        harness.move_to_session();
        harness.answer_next();
        harness.module.session.as_mut().expect("sessie").abandon(0);
        harness.tick();
        assert_eq!(harness.module.screen, Screen::Results);

        // Opnieuw: the same tables, a new round with a new id.
        let first = harness
            .module
            .session
            .as_ref()
            .expect("sessie")
            .id()
            .to_string();
        harness.press(Key::Enter);
        assert_eq!(harness.module.screen, Screen::Session);
        let second = harness
            .module
            .session
            .as_ref()
            .expect("sessie")
            .id()
            .to_string();
        assert_ne!(first, second, "een nieuwe ronde heeft een nieuwe id");
        assert_eq!(harness.module.settings.tables, vec![7]);

        // Andere instellingen: back to the tables, with nothing running.
        harness.module.screen = Screen::Results;
        harness.module.result_cursor = 1;
        harness.press(Key::Enter);
        assert_eq!(harness.module.screen, Screen::Tables);
        assert!(harness.module.session.is_none());

        // Terug naar menu.
        harness.module.screen = Screen::Results;
        harness.module.result_cursor = 2;
        assert_eq!(harness.press(Key::Enter), Transition::Exit);
    }

    #[test]
    fn esc_asks_before_it_stops_and_the_clock_waits_for_the_answer() {
        let mut harness = Harness::new();
        harness.move_to_session();

        harness.advance(1_000);
        harness.press(Key::Esc);
        assert_eq!(harness.module.screen, Screen::StopConfirm);
        assert_eq!(harness.module.hints(), ui::stop_hints());

        // Half a minute passes while the question is on screen...
        harness.advance(30_000);
        let before = harness
            .module
            .session
            .as_ref()
            .expect("sessie")
            .remaining_ms(harness.module.now(&harness.ctx));

        // ...and none of it counts: the clock was frozen with the question.
        harness.press(Key::Esc);
        assert_eq!(harness.module.screen, Screen::Session);
        let after = harness
            .module
            .session
            .as_ref()
            .expect("sessie")
            .remaining_ms(harness.module.now(&harness.ctx));
        assert_eq!(before, after);
    }

    #[test]
    fn saying_yes_to_the_stop_question_keeps_what_was_answered() {
        let mut harness = Harness::new();
        harness.move_to_session();
        let expected = harness
            .module
            .session
            .as_ref()
            .and_then(Session::current)
            .map(|question| question.expected())
            .expect("vraag");
        for digit in expected.to_string().chars() {
            harness.press(Key::Char(digit));
        }
        harness.press(Key::Enter);
        assert_eq!(harness.store().count_in(ANSWERS_FILE), 1);

        harness.press(Key::Esc);
        harness.press(Key::Left);
        assert!(harness.module.stop_yes, "links kiest 'ja, stoppen'");
        harness.press(Key::Enter);

        assert_eq!(harness.module.screen, Screen::Results);
        assert_eq!(harness.store().count_in(SESSIONS_FILE), 1);
        let sessions = harness.sessions();
        assert_eq!(sessions[0].status, records::SessionStatus::Abandoned);
        assert_eq!(sessions[0].correct_count, 1);
        assert_eq!(sessions[0].question_count, 10);
        // The questions that were never shown are not written.
        assert_eq!(harness.store().count_in(ANSWERS_FILE), 1);
    }

    #[test]
    fn ctrl_c_stops_the_round_and_leaves_the_module() {
        let mut harness = Harness::new();
        harness.move_to_session();
        assert_eq!(harness.press(Key::CtrlC), Transition::Exit);

        assert_eq!(harness.store().count_in(SESSIONS_FILE), 1);
        let sessions = harness.sessions();
        assert_eq!(sessions[0].status, records::SessionStatus::Abandoned);
    }

    #[test]
    fn leaving_the_module_saves_a_running_round_and_the_choices() {
        let mut harness = Harness::new();
        harness.move_to_session();
        harness.press(Key::Char('9'));

        // A round that is still running when the module is closed is saved as
        // abandoned, and the choices are kept for next time.
        harness.module.on_exit(&mut harness.ctx);

        let sessions = harness.sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, records::SessionStatus::Abandoned);

        let saved = harness.stored_settings().expect("instellingen opgeslagen");
        assert_eq!(saved.tables, vec![7]);
        assert_eq!(saved.timer_mode, TimerMode::PerQuestion);
        assert_eq!(saved.seconds, 5);
    }

    #[test]
    fn the_next_visit_starts_with_the_choices_of_last_time() {
        let mut harness = Harness::new();
        harness.press(Key::Char('8'));
        harness.module.table_cursor = ui::table_rows().len() - 2;
        harness.press(Key::Enter);
        assert!(harness.module.settings.reverse);
        harness.press(Key::End);
        harness.press(Key::Enter);
        harness.press(Key::Enter);
        assert_eq!(harness.module.screen, Screen::Session);

        // Leaving and coming back, as the application does between two rounds.
        harness.module.on_exit(&mut harness.ctx);
        harness.module.on_enter(&mut harness.ctx);

        assert_eq!(harness.module.screen, Screen::Tables);
        assert_eq!(harness.module.settings.tables, vec![8]);
        assert!(harness.module.settings.reverse);
        assert_eq!(harness.module.settings.timer_mode, TimerMode::PerQuestion);
        assert_eq!(harness.module.settings.seconds, 5);
        assert!(harness.module.session.is_none());
    }

    #[test]
    fn a_settings_file_that_cannot_be_read_does_not_lock_the_learner_out() {
        let mut harness = Harness::new();
        harness.press(Key::Char('4'));
        harness.module.on_exit(&mut harness.ctx);

        let path = harness
            .store()
            .path_for(settings::SETTINGS_FILE)
            .expect("pad");
        std::fs::write(path, "= geen toml").expect("overschrijven");

        harness.module.on_enter(&mut harness.ctx);
        assert!(harness.module.settings.tables.is_empty());
        assert_eq!(harness.module.screen, Screen::Tables);

        // And the module still works.
        harness.press(Key::Char('4'));
        assert_eq!(harness.module.settings.tables, vec![4]);
    }

    #[test]
    fn a_storage_failure_is_reported_instead_of_being_swallowed() {
        let mut harness = Harness::new();
        // A directory where the log should be: appending to it cannot work.
        std::fs::create_dir_all(harness.store().path_for(ANSWERS_FILE).expect("pad"))
            .expect("map aanmaken");

        harness.move_to_session();
        harness.press(Key::Char('1'));
        harness.press(Key::Enter);

        let reported = harness.ctx.take_error().expect("de fout wordt gemeld");
        assert!(!reported.is_empty(), "{reported}");
    }

    #[test]
    fn ticks_and_resizes_without_a_session_change_nothing() {
        let mut harness = Harness::new();
        assert_eq!(
            harness.module.handle_event(&Event::Tick, &mut harness.ctx),
            Transition::Stay
        );
        assert_eq!(
            harness.module.handle_event(
                &Event::Resize {
                    width: 20,
                    height: 5
                },
                &mut harness.ctx
            ),
            Transition::Stay
        );
        assert_eq!(harness.module.screen, Screen::Tables);
    }

    #[test]
    fn esc_leaves_the_table_screen_without_a_session() {
        let mut harness = Harness::new();
        assert_eq!(harness.press(Key::Esc), Transition::Exit);
        assert_eq!(harness.store().count_in(SESSIONS_FILE), 0);
    }
}
