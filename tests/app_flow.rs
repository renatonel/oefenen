//! End-to-end tests through the public API, mirroring the acceptance criteria.
//!
//! Everything runs against a temporary data directory, so no terminal and no
//! real user data are involved.

use std::time::Duration;

use oefenen::app::{App, Screen};
use oefenen::event::{Event, Key};
use oefenen::modules::tafels::TAFELS_ID;
use oefenen::modules::tafels::records::{
    ANSWERS_FILE, AnswerRecord, SCHEMA, SESSIONS_FILE, SessionRecord, SessionStatus,
};
use oefenen::modules::{ModuleContext, registry};
use oefenen::profile::{Grade, Profile, ProfileId};
use oefenen::storage::{self, ModuleStore, ProfileStore, StorageError};
use tempfile::TempDir;

const TICK: Duration = Duration::from_millis(75);

/// A temp data directory plus a profile store on it.
struct Data {
    dir: TempDir,
}

impl Data {
    fn new() -> Self {
        Self {
            dir: TempDir::new().expect("tijdelijke map"),
        }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    fn profiles(&self) -> ProfileStore {
        ProfileStore::new(self.path())
    }
}

fn key(app: &mut App, key: Key) {
    app.handle_event(&Event::Key(key));
}

/// Creates a learner through the keyboard.
fn create(app: &mut App, name: &str) {
    if app.screen() == Screen::ProfileSelect {
        key(app, Key::Char('n'));
    }
    for character in name.chars() {
        key(app, Key::Char(character));
    }
    key(app, Key::Enter);
}

#[test]
fn a_data_dir_can_be_overridden_and_is_created() {
    let parent = TempDir::new().unwrap();
    let target = parent.path().join("eigen map");
    let resolved = storage::resolve_data_dir(Some(&target)).unwrap();
    assert_eq!(resolved, target);
    assert!(target.is_dir());
}

#[test]
fn profile_create_list_delete_round_trip() {
    let data = Data::new();
    let store = data.profiles();
    let grade = Grade::new(4).unwrap();

    assert!(store.list().unwrap().is_empty());
    let sem = store.create("Sem", Some(grade)).unwrap();
    let jan = store.create("Jan", Some(Grade::new(3).unwrap())).unwrap();
    assert_ne!(sem.id(), jan.id());

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].name(), "Jan");
    assert_eq!(listed[1].name(), "Sem");
    assert_eq!(listed[1].grade(), Some(grade));

    store.delete(sem.id()).unwrap();
    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id(), jan.id());
    assert!(matches!(
        store.delete(sem.id()),
        Err(StorageError::ProfileNotFound(_))
    ));
}

#[test]
fn the_id_is_stable_across_a_rename() {
    let data = Data::new();
    let store = data.profiles();
    let created = store.create("Sem", Some(Grade::new(4).unwrap())).unwrap();

    let renamed = store.rename(created.id(), "Samuel").unwrap();
    assert_eq!(renamed.id(), created.id());
    assert_eq!(renamed.name(), "Samuel");
    assert_eq!(store.list().unwrap()[0].id(), created.id());

    // The directory name, and anything stored in it, is untouched.
    let module_dir = store.profile_dir(created.id()).unwrap().join("modules");
    std::fs::create_dir_all(&module_dir).unwrap();
    store.rename(created.id(), "S. de Vries").unwrap();
    assert!(module_dir.is_dir());
}

#[test]
fn profile_toml_is_written_atomically() {
    let data = Data::new();
    let store = data.profiles();
    let profile = store.create("Sem", Some(Grade::new(4).unwrap())).unwrap();

    let path = store.profile_path(profile.id()).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    assert!(before.contains("version = 1"));
    assert!(before.contains("grade = 4"));

    let mut updated = profile.clone();
    updated.rename("Samuel").unwrap();
    store.save(&updated).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("Samuel"));
    assert_eq!(after.lines().count(), before.lines().count());

    // No `.tmp` file survives a successful write.
    let leftovers: Vec<String> = std::fs::read_dir(store.profile_dir(profile.id()).unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, vec!["profile.toml".to_string()]);

    // And the file it wrote is valid TOML with the documented fields.
    let file: storage::ProfileFile = toml::from_str(&after).unwrap();
    assert_eq!(file.version, storage::PROFILE_FILE_VERSION);
    assert_eq!(file.id, profile.id().as_str());
    assert_eq!(file.grade, Some(4));
    assert_eq!(file.to_profile().unwrap().name(), "Samuel");
}

#[test]
fn jsonl_records_are_appended_and_read_back() {
    let data = Data::new();
    let store = data.profiles();
    let profile = store.create("Sem", Some(Grade::new(4).unwrap())).unwrap();
    let module = ModuleStore::new(data.path(), profile.id(), TAFELS_ID).unwrap();

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Line {
        n: u32,
    }

    assert_eq!(module.append_record(&Line { n: 1 }).unwrap(), 1);
    assert_eq!(module.append_record(&Line { n: 2 }).unwrap(), 2);
    assert_eq!(module.append_record(&Line { n: 3 }).unwrap(), 3);

    let lines = std::fs::read_to_string(module.results_path()).unwrap();
    assert_eq!(lines.lines().count(), 3);
    assert!(lines.ends_with('\n'));

    let read: Vec<Line> = module.read_records().unwrap();
    assert_eq!(read, vec![Line { n: 1 }, Line { n: 2 }, Line { n: 3 }]);
}

#[test]
fn module_storage_cannot_escape_its_directory() {
    let data = Data::new();
    let store = data.profiles();
    let profile = store.create("Sem", Some(Grade::new(4).unwrap())).unwrap();

    for evil in ["../evil", "..", "/etc", "a/b", "", "Tafels"] {
        let result = ModuleStore::new(data.path(), profile.id(), evil);
        assert!(result.is_err(), "{evil:?} moeten geweigerd worden");
    }

    // The only module store that opens is the one inside the profile.
    let module = ModuleStore::new(data.path(), profile.id(), TAFELS_ID).unwrap();
    assert!(
        module
            .dir()
            .starts_with(store.profile_dir(profile.id()).unwrap())
    );
    assert!(module.dir().ends_with(format!("modules/{TAFELS_ID}")));
}

#[test]
fn the_registry_returns_the_tafels_module() {
    let modules = registry();
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].id(), TAFELS_ID);
    assert_eq!(modules[0].subject(), oefenen::modules::Subject::Math);
    assert!(!modules[0].title().is_empty());
    assert!(!modules[0].description().is_empty());
    assert!(!modules[0].hints().is_empty());
}

#[test]
fn a_whole_round_through_the_app_writes_answers_and_one_round_line() {
    let data = Data::new();
    let mut app = App::new(data.path(), TICK).unwrap();
    create(&mut app, "Sem");

    key(&mut app, Key::Enter); // select the learner
    assert_eq!(app.screen(), Screen::ModuleList);
    key(&mut app, Key::Enter); // open the module
    assert_eq!(app.screen(), Screen::ModuleRunning);

    // The clock runs on ticks, and the module sees the same counter.
    for _ in 0..4 {
        app.handle_event(&Event::Tick);
    }
    assert_eq!(app.context().unwrap().elapsed_millis(), 300);

    // Table 7, "Beginnen", and the first time preset.
    key(&mut app, Key::Char('7'));
    key(&mut app, Key::End);
    key(&mut app, Key::Enter);
    key(&mut app, Key::Enter);
    assert_eq!(
        app.context().unwrap().elapsed_millis(),
        300,
        "de klok loopt door"
    );

    for character in "56".chars() {
        key(&mut app, Key::Char(character));
    }
    key(&mut app, Key::Enter); // submit
    key(&mut app, Key::Esc); // ask before stopping
    key(&mut app, Key::Esc); // "nee, doorgaan": the round carries on
    key(&mut app, Key::Esc); // ask again
    key(&mut app, Key::Right); // "ja, stoppen"
    key(&mut app, Key::Enter); // the round ends, the results appear
    key(&mut app, Key::Down); // "Terug naar het menu"
    key(&mut app, Key::Down);
    key(&mut app, Key::Enter);
    assert_eq!(app.screen(), Screen::ModuleList);

    let profile = &app.profiles()[0];
    let store = ModuleStore::new(data.path(), profile.id(), TAFELS_ID).unwrap();

    // The answer reached the disk the moment it was submitted.
    let answers: Vec<AnswerRecord> = store.read_from_lossy(ANSWERS_FILE);
    assert_eq!(answers.len(), 1, "precies één antwoord geschreven");
    assert_eq!(answers[0].schema, SCHEMA);
    assert_eq!(answers[0].question_index, 0);
    assert_eq!(answers[0].given, Some(56));
    assert_eq!(
        answers[0].outcome.is_correct(),
        answers[0].given == Some(answers[0].question().expected()),
        "de uitkomst klopt met het antwoord"
    );

    // And the round line follows when the round ends.
    let rounds: Vec<SessionRecord> = store.read_from_lossy(SESSIONS_FILE);
    assert_eq!(rounds.len(), 1, "precies één ronde geschreven");
    assert_eq!(rounds[0].schema, SCHEMA);
    assert_eq!(rounds[0].status, SessionStatus::Abandoned);
    assert_eq!(rounds[0].settings.tables, vec![7]);
    assert_eq!(rounds[0].question_count, 10);
}

#[test]
fn a_module_drives_its_own_context_without_a_terminal() {
    // The module contract works on its own: no `Frame`, no `ratatui`.
    let data = Data::new();
    let store = data.profiles();
    let profile = store.create("Sem", Some(Grade::new(4).unwrap())).unwrap();
    let module_store = ModuleStore::new(data.path(), profile.id(), TAFELS_ID).unwrap();
    let context = ModuleContext::new(profile, module_store);

    let mut module = registry().remove(0);
    let mut context = context;
    module.on_enter(&mut context);
    let stay = oefenen::modules::Transition::Stay;

    // The number keys pick tables, and "Beginnen" leads to the timer.
    assert_eq!(
        module.handle_event(&Event::Key(Key::Char('7')), &mut context),
        stay
    );
    assert_eq!(
        module.handle_event(&Event::Key(Key::End), &mut context),
        stay
    );
    assert_eq!(
        module.handle_event(&Event::Key(Key::Enter), &mut context),
        stay
    );
    // The first preset starts the round.
    assert_eq!(
        module.handle_event(&Event::Key(Key::Enter), &mut context),
        stay
    );

    for character in "56".chars() {
        assert_eq!(
            module.handle_event(&Event::Key(Key::Char(character)), &mut context),
            stay
        );
    }
    assert_eq!(
        module.handle_event(&Event::Key(Key::Enter), &mut context),
        stay
    );
    // A running round cannot be left by Esc, and Ctrl-C always can.
    assert_eq!(
        module.handle_event(&Event::Key(Key::Esc), &mut context),
        stay
    );
    assert_eq!(
        module.handle_event(&Event::Key(Key::CtrlC), &mut context),
        oefenen::modules::Transition::Exit
    );
    module.on_exit(&mut context);

    let module_store = ModuleStore::new(
        data.path(),
        &ProfileId::parse(context.profile().id().as_str()).unwrap(),
        TAFELS_ID,
    )
    .unwrap();
    let rounds: Vec<SessionRecord> = module_store.read_from_lossy(SESSIONS_FILE);
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].status, SessionStatus::Abandoned);
    let answers: Vec<AnswerRecord> = module_store.read_from_lossy(ANSWERS_FILE);
    assert_eq!(answers.len(), 1);
}

#[test]
fn two_profiles_keep_their_own_results() {
    let data = Data::new();
    let mut app = App::new(data.path(), TICK).unwrap();
    create(&mut app, "Sem");
    create(&mut app, "Jan");

    // Play one round as the learner under the cursor.
    key(&mut app, Key::Enter);
    key(&mut app, Key::Enter);
    key(&mut app, Key::Char('7'));
    key(&mut app, Key::End);
    key(&mut app, Key::Enter);
    key(&mut app, Key::Enter);
    key(&mut app, Key::CtrlC);
    assert_eq!(app.screen(), Screen::ModuleList);

    let counts: Vec<u64> = app
        .profiles()
        .iter()
        .map(|profile: &Profile| {
            ModuleStore::new(data.path(), profile.id(), TAFELS_ID)
                .unwrap()
                .count_in(SESSIONS_FILE)
        })
        .collect();
    assert_eq!(counts, vec![1, 0]);
}
