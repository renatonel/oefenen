//! End-to-end tests through the public API, mirroring the acceptance criteria.
//!
//! Everything runs against a temporary data directory, so no terminal and no
//! real user data are involved.

use std::time::Duration;

use oefenen::app::{App, Screen};
use oefenen::event::{Event, Key};
use oefenen::modules::multiplication::{MULTIPLICATION_ID, SCHEMA_VERSION, SessionRecord};
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
    let module = ModuleStore::new(data.path(), profile.id(), MULTIPLICATION_ID).unwrap();

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
    let module = ModuleStore::new(data.path(), profile.id(), MULTIPLICATION_ID).unwrap();
    assert!(
        module
            .dir()
            .starts_with(store.profile_dir(profile.id()).unwrap())
    );
    assert!(module.dir().ends_with("modules/tafel-oefenen"));
}

#[test]
fn the_registry_returns_the_multiplication_module() {
    let modules = registry();
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].id(), MULTIPLICATION_ID);
    assert_eq!(modules[0].subject(), oefenen::modules::Subject::Math);
    assert!(!modules[0].title().is_empty());
    assert!(!modules[0].description().is_empty());
    assert!(!modules[0].hints().is_empty());
}

#[test]
fn the_dummy_module_lifecycle_writes_one_record() {
    let data = Data::new();
    let mut app = App::new(data.path(), TICK).unwrap();
    create(&mut app, "Sem");

    key(&mut app, Key::Enter); // select the learner
    assert_eq!(app.screen(), Screen::ModuleList);
    key(&mut app, Key::Enter); // open the module
    assert_eq!(app.screen(), Screen::ModuleRunning);

    // The timer runs on ticks.
    for _ in 0..4 {
        app.handle_event(&Event::Tick);
    }
    assert_eq!(app.context().unwrap().elapsed_millis(), 300);

    for character in "56".chars() {
        key(&mut app, Key::Char(character));
    }
    key(&mut app, Key::Enter); // submit
    key(&mut app, Key::Enter); // back to the list, storing the session
    assert_eq!(app.screen(), Screen::ModuleList);

    let profile = &app.profiles()[0];
    let module = ModuleStore::new(data.path(), profile.id(), MULTIPLICATION_ID).unwrap();
    let records: Vec<SessionRecord> = module.read_records().unwrap();

    assert_eq!(records.len(), 1, "precies één record geschreven");
    let record = &records[0];
    assert_eq!(record.schema_version, SCHEMA_VERSION);
    assert_eq!(record.module_id, MULTIPLICATION_ID);
    assert_eq!(record.duration_ms, 300);
    assert_eq!(record.facts.len(), 1);
    let fact = &record.facts[0];
    assert_eq!(fact.factor_a, 7);
    assert_eq!(fact.factor_b, 8);
    assert!(fact.correct);
    assert_eq!(fact.response_time_ms, 300);
    assert!(fact.answered_at >= record.started_at);
}

#[test]
fn a_module_drives_its_own_context_without_a_terminal() {
    // The module contract works on its own: no `Frame`, no `ratatui`.
    let data = Data::new();
    let store = data.profiles();
    let profile = store.create("Sem", Some(Grade::new(4).unwrap())).unwrap();
    let module_store = ModuleStore::new(data.path(), profile.id(), MULTIPLICATION_ID).unwrap();
    let context = ModuleContext::new(profile, module_store);

    let mut module = registry().remove(0);
    let mut context = context;
    module.on_enter(&mut context);
    assert_eq!(
        module.handle_event(&Event::Key(Key::Char('5')), &mut context),
        oefenen::modules::Transition::Stay
    );
    assert_eq!(
        module.handle_event(&Event::Key(Key::Char('6')), &mut context),
        oefenen::modules::Transition::Stay
    );
    assert_eq!(
        module.handle_event(&Event::Key(Key::Enter), &mut context),
        oefenen::modules::Transition::Stay
    );
    // Escaping ends the session and is reported as an exit.
    assert_eq!(
        module.handle_event(&Event::Key(Key::Esc), &mut context),
        oefenen::modules::Transition::Exit
    );
    module.on_exit(&mut context);

    let module_store = ModuleStore::new(
        data.path(),
        &ProfileId::parse(context.profile().id().as_str()).unwrap(),
        MULTIPLICATION_ID,
    )
    .unwrap();
    let records = module_store.read_records::<SessionRecord>().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].facts.len(), 1);
    assert!(records[0].facts[0].correct);
}

#[test]
fn two_profiles_keep_their_own_results() {
    let data = Data::new();
    let mut app = App::new(data.path(), TICK).unwrap();
    create(&mut app, "Sem");
    create(&mut app, "Jan");

    // Play one session as the learner under the cursor.
    key(&mut app, Key::Enter);
    key(&mut app, Key::Enter);
    key(&mut app, Key::Esc);

    let counts: Vec<u64> = app
        .profiles()
        .iter()
        .map(|profile: &Profile| {
            ModuleStore::new(data.path(), profile.id(), MULTIPLICATION_ID)
                .unwrap()
                .record_count()
        })
        .collect();
    assert_eq!(counts, vec![1, 0]);
}
