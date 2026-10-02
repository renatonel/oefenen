//! State machine tests: no terminal, no ratatui.

use std::time::Duration;

use tempfile::TempDir;

use crate::app::{App, ConfirmChoice, Field, FormMode, Screen};
use crate::event::{Event, Key};
use crate::modules::Subject;
use crate::modules::tafels::TAFELS_ID;
use crate::modules::tafels::records::{
    ANSWERS_FILE, AnswerRecord, Outcome, SESSIONS_FILE, SessionRecord, SessionStatus,
};
use crate::profile::{Grade, ProfileId};
use crate::storage::{ModuleStore, ProfileStore};

const TICK: Duration = Duration::from_millis(75);

/// An app on a throwaway data directory.
struct Fixture {
    dir: TempDir,
    app: App,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let app = App::new(dir.path(), TICK).unwrap();
        Self { dir, app }
    }

    /// An app that already has the given learners, sitting on the profile list.
    fn with_profiles(names: &[&str]) -> Self {
        let mut fixture = Self::new();
        for name in names {
            fixture.create_profile(name);
        }
        assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
        fixture
    }

    /// Creates a learner through the keyboard, like the real screens do.
    fn create_profile(&mut self, name: &str) {
        if self.app().screen() == Screen::ProfileSelect {
            self.key(Key::Char('n'));
        }
        for character in name.chars() {
            self.key(Key::Char(character));
        }
        self.enter();
    }

    fn app(&self) -> &App {
        &self.app
    }

    fn key(&mut self, key: Key) {
        self.app.handle_event(&Event::Key(key));
    }

    fn enter(&mut self) {
        self.key(Key::Enter);
    }

    fn tick(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.app.handle_event(&Event::Tick);
        }
    }

    /// Selects the learner under the cursor and opens the module list.
    fn select_current_profile(&mut self) {
        self.enter();
        assert_eq!(self.app().screen(), Screen::ModuleList);
    }

    /// Opens the tafels module, picks table 7 and the default time.
    fn start_round(&mut self) {
        self.enter();
        assert_eq!(self.app().screen(), Screen::ModuleRunning);
        // Start from a known list: the choices are remembered between visits, so
        // "7" would switch the table off on the second round.
        self.key(Key::Char('u'));
        self.key(Key::Char('7'));
        self.key(Key::End);
        self.enter();
        // The first time row is five seconds per question.
        self.enter();
    }

    /// Runs one tafels round: chooses table 7, answers once and stops.
    fn play_round(&mut self, answer: &str) {
        self.start_round();
        for character in answer.chars() {
            self.key(Key::Char(character));
        }
        self.enter();
        self.stop_round();
    }

    /// Answers "ja, stoppen" to the stop question Esc opens.
    ///
    /// Esc on its own only asks the question, and the answer starts on "nee,
    /// doorgaan", so a second Esc would carry on instead.
    fn stop_round(&mut self) {
        self.key(Key::Esc);
        self.key(Key::Right);
        self.enter();
    }

    /// Leaves the results screen for the module list, like a learner would.
    fn leave_results(&mut self) {
        // Opnieuw, Andere instellingen, Terug naar menu.
        self.key(Key::Down);
        self.key(Key::Down);
        self.enter();
        assert_eq!(self.app().screen(), Screen::ModuleList);
    }

    /// How many of the written answers were right.
    fn correct(&self, name: &str) -> usize {
        self.answers(name)
            .iter()
            .filter(|answer| answer.outcome == Outcome::Correct)
            .count()
    }

    fn profile_id(&self, name: &str) -> ProfileId {
        self.app()
            .profiles()
            .iter()
            .find(|profile| profile.name() == name)
            .unwrap_or_else(|| panic!("profiel {name} bestaat niet"))
            .id()
            .clone()
    }

    /// The tafels storage of one learner.
    fn store(&self, name: &str) -> ModuleStore {
        ModuleStore::new(self.dir.path(), &self.profile_id(name), TAFELS_ID).unwrap()
    }

    /// The finished rounds of one learner.
    fn records(&self, name: &str) -> Vec<SessionRecord> {
        self.store(name).read_from_lossy(SESSIONS_FILE)
    }

    /// The answers of one learner, one record per question.
    fn answers(&self, name: &str) -> Vec<AnswerRecord> {
        self.store(name).read_from_lossy(ANSWERS_FILE)
    }
}

#[test]
fn starts_on_the_profile_list_when_profiles_exist() {
    let fixture = Fixture::with_profiles(&["Sem", "Jan"]);
    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
    assert_eq!(fixture.app().profiles().len(), 2);
    // Sorted by name, so the list order is stable between runs.
    assert_eq!(fixture.app().profiles()[0].name(), "Jan");
    assert_eq!(fixture.app().profiles()[1].name(), "Sem");
}

#[test]
fn starts_in_the_create_form_when_there_are_no_profiles() {
    let fixture = Fixture::new();
    assert_eq!(fixture.app().screen(), Screen::ProfileForm);
    assert_eq!(fixture.app().form().mode(), &FormMode::Create);
    assert!(fixture.app().toast().is_none());
}

#[test]
fn creating_a_profile_stores_it_and_returns_to_the_list() {
    let mut fixture = Fixture::new();
    fixture.create_profile("Sem");
    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);

    let profiles = fixture.app().profiles();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].name(), "Sem");
    assert_eq!(profiles[0].grade(), Some(Grade::default()));

    // It really is on disk, not only in memory.
    let reread = ProfileStore::new(fixture.dir.path()).list().unwrap();
    assert_eq!(reread.len(), 1);
    assert_eq!(reread[0].id(), profiles[0].id());

    assert_eq!(
        fixture.app().toast().map(|toast| toast.is_error),
        Some(false)
    );
}

#[test]
fn the_grade_field_uses_left_and_right() {
    let mut fixture = Fixture::new();
    fixture.key(Key::Char('S'));
    fixture.key(Key::Char('e'));
    fixture.key(Key::Char('m'));
    fixture.key(Key::Tab);
    assert_eq!(fixture.app().form().field(), Field::Grade);
    fixture.key(Key::Right);
    fixture.key(Key::Right);
    assert_eq!(form_grade(&fixture), Some(5));
    fixture.key(Key::Left);
    assert_eq!(form_grade(&fixture), Some(4));
    // The stepper wraps around both ends: "geen groep", 3, 4 ... 8, "geen groep".
    for _ in 0..4 {
        fixture.key(Key::Right);
    }
    assert_eq!(form_grade(&fixture), Some(8));
    fixture.key(Key::Right);
    assert_eq!(form_grade(&fixture), None);
    fixture.key(Key::Right);
    assert_eq!(form_grade(&fixture), Some(3));
    fixture.key(Key::Left);
    assert_eq!(form_grade(&fixture), None);
    fixture.key(Key::Left);
    assert_eq!(form_grade(&fixture), Some(8));
    fixture.key(Key::Right);
    fixture.key(Key::Right);

    fixture.enter();
    assert_eq!(fixture.app().profiles()[0].name(), "Sem");
    assert_eq!(
        fixture.app().profiles()[0].grade().map(|g| g.value()),
        Some(3)
    );
}

/// The group currently shown in the form, as a number.
fn form_grade(fixture: &Fixture) -> Option<u8> {
    fixture.app().form().grade().map(|grade| grade.value())
}

#[test]
fn a_profile_can_be_saved_without_a_grade() {
    let mut fixture = Fixture::new();
    for character in "Sem".chars() {
        fixture.key(Key::Char(character));
    }
    fixture.key(Key::Tab);
    // One step left from groep 3 means "geen groep".
    fixture.key(Key::Left);
    assert_eq!(form_grade(&fixture), None);

    fixture.enter();
    assert_eq!(fixture.app().profiles()[0].grade(), None);

    // The file leaves the field out instead of guessing a group.
    let profile = fixture.app().profiles()[0].clone();
    let store = ProfileStore::new(fixture.dir.path());
    let path = store.profile_path(profile.id()).unwrap();
    let raw = std::fs::read_to_string(path).unwrap();
    assert!(!raw.contains("grade"), "{raw}");

    // Reading it back keeps the profile without a grade.
    assert_eq!(store.load(profile.id()).unwrap().grade(), None);
}

#[test]
fn an_empty_name_keeps_the_form_open_and_explains_why() {
    let mut fixture = Fixture::new();
    fixture.enter();
    assert_eq!(fixture.app().screen(), Screen::ProfileForm);
    assert!(fixture.app().profiles().is_empty());
    assert_eq!(
        fixture.app().form().error(),
        Some("de naam mag niet leeg zijn")
    );
}

#[test]
fn a_name_stops_at_the_maximum_length() {
    let mut fixture = Fixture::new();
    for _ in 0..40 {
        fixture.key(Key::Char('a'));
    }
    // Extra keystrokes are ignored instead of showing an error.
    assert_eq!(fixture.app().form().name().chars().count(), 32);
    assert!(fixture.app().form().error().is_none());
    fixture.enter();
    assert_eq!(fixture.app().profiles()[0].name().len(), 32);
}

#[test]
fn escape_leaves_the_form_without_saving() {
    let mut fixture = Fixture::new();
    fixture.key(Key::Char('S'));
    fixture.key(Key::Esc);
    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
    assert!(fixture.app().profiles().is_empty());
}

#[test]
fn the_name_field_accepts_a_q_but_the_list_quits_on_it() {
    let mut fixture = Fixture::new();
    fixture.key(Key::Char('q'));
    fixture.enter();
    assert_eq!(fixture.app().profiles()[0].name(), "q");
    assert!(!fixture.app().should_quit());

    // On the profile list `q` means quit.
    fixture.key(Key::Char('q'));
    assert!(fixture.app().should_quit());
}

#[test]
fn selecting_a_profile_opens_the_module_list() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();
    assert_eq!(
        fixture.app().active_profile().map(|p| p.name()),
        Some("Sem")
    );
    assert!(!fixture.app().modules().is_empty());
}

#[test]
fn escape_from_the_module_list_switches_profile() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();
    fixture.key(Key::Esc);
    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
    assert!(fixture.app().active_profile().is_none());
}

#[test]
fn the_cursor_wraps_around() {
    let mut fixture = Fixture::with_profiles(&["Sem", "Jan", "Piet"]);
    // Sorted by name: Jan, Piet, Sem. The cursor sits on the profile that was
    // created last.
    assert_eq!(fixture.app().cursor(), 1);
    fixture.key(Key::Up);
    assert_eq!(fixture.app().cursor(), 0);
    fixture.key(Key::Up);
    assert_eq!(fixture.app().cursor(), 2);
    fixture.key(Key::Down);
    assert_eq!(fixture.app().cursor(), 0);
}

#[test]
fn ctrl_c_quits_from_any_application_screen() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.key(Key::CtrlC);
    assert!(fixture.app().should_quit());

    let mut other = Fixture::new();
    other.key(Key::CtrlC);
    assert!(other.app().should_quit());
}

#[test]
fn renaming_keeps_the_id_of_the_profile() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    let id = fixture.app().profiles()[0].id().clone();

    fixture.key(Key::Char('w'));
    assert_eq!(fixture.app().screen(), Screen::ProfileForm);
    assert_eq!(fixture.app().form().mode(), &FormMode::Edit(id.clone()));

    for _ in 0..3 {
        fixture.key(Key::Backspace);
    }
    for character in "Samuel".chars() {
        fixture.key(Key::Char(character));
    }
    fixture.enter();

    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
    let profiles = fixture.app().profiles();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].name(), "Samuel");
    assert_eq!(profiles[0].id(), &id);

    // The directory did not move, so recorded results survive a rename.
    let store = ProfileStore::new(fixture.dir.path());
    assert!(store.profile_dir(&id).unwrap().is_dir());
}

#[test]
fn the_cursor_stays_on_the_profile_you_just_renamed() {
    let mut fixture = Fixture::with_profiles(&["Sem", "Jan"]);
    // Sorted by name: Jan, Sem. The cursor follows the last created profile.
    assert_eq!(fixture.app().cursor(), 0);
    fixture.key(Key::Down);
    assert_eq!(fixture.app().cursor(), 1);

    // Renaming Sem to "Alexander" moves it to the top of the list, and the
    // cursor follows so the learner keeps editing the same profile.
    fixture.key(Key::Char('w'));
    for _ in 0..3 {
        fixture.key(Key::Backspace);
    }
    for character in "Alexander".chars() {
        fixture.key(Key::Char(character));
    }
    fixture.enter();

    assert_eq!(fixture.app().cursor(), 0);
    assert_eq!(fixture.app().profiles()[0].name(), "Alexander");
    assert_eq!(fixture.app().profiles()[1].name(), "Jan");
}

#[test]
fn renaming_to_an_empty_name_changes_nothing() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.key(Key::Char('w'));
    for _ in 0..3 {
        fixture.key(Key::Backspace);
    }
    fixture.enter();
    assert_eq!(fixture.app().screen(), Screen::ProfileForm);
    assert_eq!(fixture.app().profiles()[0].name(), "Sem");
}

#[test]
fn deleting_asks_for_confirmation_first() {
    let mut fixture = Fixture::with_profiles(&["Sem", "Jan"]);
    // Sorted by name, so the cursor starts on Jan.
    let victim = fixture.profile_id("Jan");

    fixture.key(Key::Char('d'));
    assert_eq!(fixture.app().screen(), Screen::DeleteConfirm);
    let confirm = fixture.app().confirm().unwrap();
    assert_eq!(confirm.profile_index(), 0);
    // The default answer is "nee", so an accidental Enter keeps the profile.
    assert_eq!(confirm.choice(), ConfirmChoice::No);
    fixture.enter();
    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
    assert_eq!(fixture.app().profiles().len(), 2);

    // Escape cancels too.
    fixture.key(Key::Char('d'));
    fixture.key(Key::Esc);
    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
    assert_eq!(fixture.app().profiles().len(), 2);

    // Choosing "ja" deletes, and removes the directory.
    fixture.key(Key::Char('d'));
    fixture.key(Key::Right);
    assert_eq!(
        fixture.app().confirm().map(|confirm| confirm.choice()),
        Some(ConfirmChoice::Yes)
    );
    fixture.enter();

    assert_eq!(fixture.app().profiles().len(), 1);
    assert_eq!(fixture.app().profiles()[0].name(), "Sem");
    let store = ProfileStore::new(fixture.dir.path());
    assert!(!store.profile_dir(&victim).unwrap().exists());
    assert!(fixture.app().toast().is_some());
}

#[test]
fn the_cursor_stays_valid_after_deleting() {
    let mut fixture = Fixture::with_profiles(&["A", "B"]);
    fixture.key(Key::Down);
    fixture.key(Key::Char('d'));
    fixture.key(Key::Right);
    fixture.enter();
    assert_eq!(fixture.app().profiles().len(), 1);
    assert!(fixture.app().cursor() < fixture.app().profiles().len());
}

#[test]
fn opening_and_leaving_a_module_walks_through_module_running() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();

    fixture.enter();
    assert_eq!(fixture.app().screen(), Screen::ModuleRunning);
    assert!(fixture.app().screen().is_module());
    assert_eq!(fixture.app().running_index(), Some(0));
    assert!(fixture.app().context().is_some());

    fixture.key(Key::Esc);
    assert_eq!(fixture.app().screen(), Screen::ModuleList);
    assert!(fixture.app().context().is_none());
    assert!(fixture.app().running_index().is_none());
}

#[test]
fn the_module_lifecycle_writes_exactly_one_round() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();
    fixture.start_round();

    fixture.tick(8);
    for character in "56".chars() {
        fixture.key(Key::Char(character));
    }
    fixture.enter();
    assert_eq!(
        fixture.answers("Sem").len(),
        1,
        "het antwoord staat er meteen"
    );
    assert!(fixture.records("Sem").is_empty(), "de ronde loopt nog");

    // Esc asks before it stops, and the answer starts on "nee, doorgaan", so
    // cancelling leaves the round exactly where it was.
    fixture.key(Key::Esc);
    fixture.key(Key::Esc);
    assert!(fixture.records("Sem").is_empty(), "afgebroken telt niet");

    fixture.stop_round();
    fixture.leave_results();

    let records = fixture.records("Sem");
    assert_eq!(records.len(), 1, "precies één regel geschreven");
    let record = &records[0];
    assert_eq!(record.schema, 1);
    assert_eq!(record.status, SessionStatus::Abandoned);
    assert_eq!(record.settings.tables, vec![7]);
    assert_eq!(record.question_count, 10);
    // The round was stopped after one question, so only that one was written.
    assert_eq!(record.correct_count, fixture.correct("Sem"));
    assert_eq!(fixture.answers("Sem").len(), 1);

    // The clock counts ticks, and the verdict pauses it.
    assert!(record.total_ms > 0, "{record:?}");
}

#[test]
fn a_wrong_answer_is_recorded_as_incorrect() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();
    fixture.start_round();
    fixture.tick(2);

    // Ten is not in table seven, so this is wrong whichever question comes up.
    for character in "10".chars() {
        fixture.key(Key::Char(character));
    }
    fixture.enter();
    fixture.stop_round();

    let answers = fixture.answers("Sem");
    assert_eq!(answers.len(), 1);
    assert_ne!(answers[0].outcome, Outcome::Correct);
    assert!(answers[0].outcome.is_wrong(), "{:?}", answers[0].outcome);
    assert_eq!(answers[0].given, Some(10));
    assert_eq!(answers[0].question_index, 0);
}

#[test]
fn leaving_the_module_still_writes_the_questions_that_were_asked() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();
    fixture.start_round();
    for character in "56".chars() {
        fixture.key(Key::Char(character));
    }
    fixture.enter();

    // Ctrl-C stops the round and leaves the module at once.
    assert!(!fixture.app().should_quit());
    fixture.key(Key::CtrlC);
    assert_eq!(fixture.app().screen(), Screen::ModuleList);

    // The answer reached the disk before the module was closed, and the round
    // is written as abandoned.
    assert_eq!(fixture.answers("Sem").len(), 1);
    let records = fixture.records("Sem");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, SessionStatus::Abandoned);
    assert_eq!(records[0].correct_count, fixture.correct("Sem"));
}

#[test]
fn a_round_that_was_never_started_writes_nothing() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();

    // The table screen can be left without choosing anything at all.
    fixture.enter();
    fixture.key(Key::Esc);
    assert_eq!(fixture.app().screen(), Screen::ModuleList);
    assert!(fixture.records("Sem").is_empty());
    assert!(fixture.answers("Sem").is_empty());
}

#[test]
fn each_visit_is_its_own_session() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();
    for _ in 0..2 {
        fixture.play_round("56");
        fixture.leave_results();
    }
    let records = fixture.records("Sem");
    assert_eq!(records.len(), 2);
    assert_ne!(records[0].session_id, records[1].session_id);
}

#[test]
fn results_are_stored_per_profile() {
    let mut fixture = Fixture::with_profiles(&["Sem", "Jan"]);
    // Sorted by name, so the cursor starts on Jan.
    fixture.select_current_profile();
    assert_eq!(
        fixture.app().active_profile().map(|p| p.name()),
        Some("Jan")
    );
    fixture.play_round("56");
    assert_eq!(fixture.records("Jan").len(), 1);
    fixture.leave_results();

    // Switch learner: the other profile has its own, empty, storage.
    fixture.key(Key::Down);
    fixture.key(Key::Down);
    fixture.enter();
    fixture.key(Key::Esc);
    assert_eq!(fixture.app().screen(), Screen::ModuleList);
    fixture.key(Key::Esc);
    fixture.key(Key::Down);
    fixture.select_current_profile();
    assert_eq!(
        fixture.app().active_profile().map(|p| p.name()),
        Some("Sem")
    );
    assert_eq!(fixture.records("Sem").len(), 0);
    assert_eq!(fixture.records("Jan").len(), 1);
}

#[test]
fn the_registry_is_visible_from_the_module_list() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.select_current_profile();
    let modules = fixture.app().modules();
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].id(), TAFELS_ID);
    assert_eq!(modules[0].subject(), Subject::Math);
    assert!(!modules[0].title().is_empty());
    assert!(!modules[0].description().is_empty());
}

#[test]
fn a_toast_disappears_after_a_few_ticks() {
    let mut fixture = Fixture::new();
    fixture.create_profile("Sem");
    assert!(fixture.app().toast().is_some());
    fixture.tick(100);
    assert!(fixture.app().toast().is_none());
}

#[test]
fn a_resize_event_changes_nothing() {
    let mut fixture = Fixture::with_profiles(&["Sem"]);
    fixture.app.handle_event(&Event::Resize {
        width: 200,
        height: 60,
    });
    assert_eq!(fixture.app().screen(), Screen::ProfileSelect);
    assert_eq!(fixture.app().cursor(), 0);
}
