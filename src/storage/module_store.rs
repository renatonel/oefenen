//! Storage handle scoped to one profile *and* one module.
//!
//! Directory: `<data_dir>/profiles/<profile_id>/modules/<module_id>/`.
//!
//! A [`ModuleStore`] can only reach that directory: the profile id and the
//! module id are validated as single path segments and every path is built
//! with [`safe_join`]. A module therefore cannot read another module's results,
//! even if it tries to.

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
}
