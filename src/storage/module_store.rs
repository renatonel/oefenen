//! Storage handle scoped to one profile *and* one module.
//!
//! Directory: `<data_dir>/profiles/<profile_id>/modules/<module_id>/`.
//!
//! A [`ModuleStore`] can only reach that directory: the profile id and the
//! module id are validated as single path segments and every path is built
//! with [`safe_join`]. A module therefore cannot read another module's results,
//! even if it tries to.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::profile::ProfileId;

use super::error::StorageError;
use super::jsonl;
use super::paths::{MODULES_DIR, PROFILES_DIR, RESULTS_FILE, safe_join, validate_segment};

/// Result log of a single module, for a single profile.
#[derive(Debug, Clone)]
pub struct ModuleStore {
    dir: PathBuf,
}

impl ModuleStore {
    /// Opens (but does not create) the store of `module_id` for `profile_id`.
    ///
    /// Fails when the module id is not a single, harmless path segment or when
    /// the profile does not exist.
    pub fn new(
        data_dir: impl AsRef<Path>,
        profile_id: &ProfileId,
        module_id: &str,
    ) -> Result<Self, StorageError> {
        validate_segment("module", module_id)?;
        let profile_root = safe_join(data_dir.as_ref(), &[PROFILES_DIR, profile_id.as_str()])?;
        if !profile_root.is_dir() {
            return Err(StorageError::ProfileNotFound(profile_id.to_string()));
        }
        let dir = safe_join(&profile_root, &[MODULES_DIR, module_id])?;
        Ok(Self { dir })
    }

    /// The directory this module may use.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The absolute path of one of this module's own files.
    ///
    /// The name goes through [`safe_join`], so even a hard-coded constant stays
    /// inside this directory.
    pub fn path_for(&self, file: &str) -> Result<PathBuf, StorageError> {
        safe_join(&self.dir, &[file])
    }

    /// The append-only results file of this module.
    pub fn results_path(&self) -> PathBuf {
        self.dir.join(RESULTS_FILE)
    }

    /// Appends one record to `results.jsonl` and returns the new record count.
    pub fn append_record<T: Serialize>(&self, record: &T) -> Result<u64, StorageError> {
        jsonl::append_line(&self.results_path(), record)
    }

    /// Reads every record from `results.jsonl`, failing on a damaged line.
    pub fn read_records<T: DeserializeOwned>(&self) -> Result<Vec<T>, StorageError> {
        jsonl::read_all(&self.results_path())
    }

    /// Reads every record, skipping damaged lines.
    pub fn read_records_lossy<T: DeserializeOwned>(&self) -> Vec<T> {
        jsonl::read_all_lossy(&self.results_path())
    }

    /// Number of records written so far.
    pub fn record_count(&self) -> u64 {
        jsonl::count_lines(&self.results_path())
    }

    /// Appends one JSON object to `file` and returns the new record count.
    ///
    /// Lets a module keep more than one append-only log (`answers.jsonl` next
    /// to `sessions.jsonl`, say) without writing files itself.
    pub fn append_to<T: Serialize>(&self, file: &str, record: &T) -> Result<u64, StorageError> {
        jsonl::append_line(&self.path_for(file)?, record)
    }

    /// Reads every record of `file`, skipping damaged lines.
    pub fn read_from_lossy<T: DeserializeOwned>(&self, file: &str) -> Vec<T> {
        match self.path_for(file) {
            Ok(path) => jsonl::read_all_lossy(&path),
            // An unusable file name is a programming error, not learner data:
            // there is nothing to read.
            Err(_) => Vec::new(),
        }
    }

    /// Number of records in `file`.
    pub fn count_in(&self, file: &str) -> u64 {
        match self.path_for(file) {
            Ok(path) => jsonl::count_lines(&path),
            Err(_) => 0,
        }
    }

    /// Writes `value` to `file` as pretty TOML, atomically.
    pub fn write_document<T: Serialize>(&self, file: &str, value: &T) -> Result<(), StorageError> {
        let path = self.path_for(file)?;
        let body = toml::to_string_pretty(value).map_err(|source| StorageError::TomlWrite {
            path: path.clone(),
            source,
        })?;
        super::write_atomic(&path, body.as_bytes())
    }

    /// Reads `file` as TOML; a missing file reads as `None`.
    pub fn read_document<T: DeserializeOwned>(
        &self,
        file: &str,
    ) -> Result<Option<T>, StorageError> {
        let path = self.path_for(file)?;
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path).map_err(|e| StorageError::io(&path, e))?;
        let value = toml::from_str(&raw).map_err(|source| StorageError::Toml { path, source })?;
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Grade, Profile};
    use serde::Deserialize;
    use tempfile::TempDir;
    use time::macros::datetime;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Record {
        score: u32,
    }

    fn profile(dir: &TempDir) -> Profile {
        let store = crate::storage::ProfileStore::new(dir.path());
        store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap()
    }

    #[test]
    fn writes_into_the_expected_directory() {
        let dir = TempDir::new().unwrap();
        let learner = profile(&dir);
        let store = ModuleStore::new(dir.path(), learner.id(), "tafel-gewichten").unwrap();

        let expected = dir
            .path()
            .join("profiles")
            .join(learner.id().as_str())
            .join("modules")
            .join("tafel-gewichten");
        assert_eq!(store.dir(), expected.as_path());

        assert_eq!(store.append_record(&Record { score: 7 }).unwrap(), 1);
        assert_eq!(store.append_record(&Record { score: 9 }).unwrap(), 2);
        assert_eq!(store.read_records::<Record>().unwrap().len(), 2);
        assert_eq!(store.record_count(), 2);
        assert!(store.results_path().starts_with(&expected));
    }

    #[test]
    fn two_modules_cannot_see_each_other() {
        let dir = TempDir::new().unwrap();
        let learner = profile(&dir);
        let first = ModuleStore::new(dir.path(), learner.id(), "tafel-gewichten").unwrap();
        let second = ModuleStore::new(dir.path(), learner.id(), "leesverstand").unwrap();
        assert_ne!(first.dir(), second.dir());

        first.append_record(&Record { score: 5 }).unwrap();
        assert_eq!(first.record_count(), 1);
        assert_eq!(second.record_count(), 0);
        assert!(second.read_records::<Record>().unwrap().is_empty());
    }

    #[test]
    fn refuses_module_ids_that_try_to_escape() {
        let dir = TempDir::new().unwrap();
        let learner = profile(&dir);
        for evil in ["../evil", "..", "/etc", "a/b", "", "Lezen"] {
            let result = ModuleStore::new(dir.path(), learner.id(), evil);
            assert!(result.is_err(), "{evil:?} zou geweigerd moeten worden");
        }
    }

    #[test]
    fn refuses_unknown_profiles() {
        let dir = TempDir::new().unwrap();
        // Creates the profile directory first, so the unknown id is the only
        // reason the store can fail.
        profile(&dir);
        let unknown = ProfileId::parse("sem-00000000").unwrap();
        assert!(ModuleStore::new(dir.path(), &unknown, "tafel-gewichten").is_err());
    }

    #[test]
    fn a_module_can_keep_more_than_one_log() {
        let dir = TempDir::new().unwrap();
        let learner = profile(&dir);
        let store = ModuleStore::new(dir.path(), learner.id(), "tafels").unwrap();

        assert_eq!(
            store
                .append_to("answers.jsonl", &Record { score: 1 })
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .append_to("sessions.jsonl", &Record { score: 2 })
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .append_to("answers.jsonl", &Record { score: 3 })
                .unwrap(),
            2
        );

        assert_eq!(store.count_in("answers.jsonl"), 2);
        assert_eq!(store.count_in("sessions.jsonl"), 1);
        let answers: Vec<Record> = store.read_from_lossy("answers.jsonl");
        assert_eq!(answers, vec![Record { score: 1 }, Record { score: 3 }]);
        // The file the module asks for is a real path inside its own directory.
        assert!(
            store
                .path_for("answers.jsonl")
                .unwrap()
                .starts_with(store.dir())
        );
    }

    #[test]
    fn a_named_file_stays_inside_the_module_directory() {
        let dir = TempDir::new().unwrap();
        let learner = profile(&dir);
        let store = ModuleStore::new(dir.path(), learner.id(), "tafels").unwrap();

        assert!(store.path_for("../evil.jsonl").is_err());
        assert!(store.path_for("nested/file.jsonl").is_err());
        // The forgiving readers report "nothing" instead of panicking.
        assert!(store.read_from_lossy::<Record>("../evil.jsonl").is_empty());
        assert_eq!(store.count_in("../evil.jsonl"), 0);
    }

    #[test]
    fn documents_round_trip_and_a_missing_one_reads_as_none() {
        let dir = TempDir::new().unwrap();
        let learner = profile(&dir);
        let store = ModuleStore::new(dir.path(), learner.id(), "tafels").unwrap();

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Settings {
            version: u32,
            seconds: u64,
        }

        assert_eq!(
            store.read_document::<Settings>("settings.toml").unwrap(),
            None
        );

        store
            .write_document(
                "settings.toml",
                &Settings {
                    version: 1,
                    seconds: 10,
                },
            )
            .unwrap();
        let loaded: Settings = store
            .read_document("settings.toml")
            .unwrap()
            .expect("het bestand is net geschreven");
        assert_eq!(loaded.seconds, 10);

        // Writing replaces the file and leaves no temp file behind.
        store
            .write_document(
                "settings.toml",
                &Settings {
                    version: 1,
                    seconds: 20,
                },
            )
            .unwrap();
        let names: Vec<String> = fs::read_dir(store.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["settings.toml".to_string()]);

        // Damaged TOML is reported with the path it happened on.
        fs::write(store.path_for("settings.toml").unwrap(), "geen toml").unwrap();
        assert!(matches!(
            store.read_document::<Settings>("settings.toml"),
            Err(StorageError::Toml { .. })
        ));
    }
}
