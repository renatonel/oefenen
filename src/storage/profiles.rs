//! Reading and writing the profiles in `<data_dir>/profiles/`.
//!
//! Layout:
//!
//! ```text
//! <data_dir>/profiles/<profile_id>/profile.toml
//! ```
//!
//! `profile.toml` is rewritten atomically (temp file in the same directory,
//! followed by a rename), so an interrupted write can never leave a profile
//! half-written.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::profile::{Grade, Profile, ProfileId};

use super::error::StorageError;
use super::paths::{PROFILE_FILE, PROFILES_DIR, safe_join, validate_segment};
use super::write_atomic;

/// Version of the `profile.toml` format written by this build.
pub const PROFILE_FILE_VERSION: u32 = 1;

/// Number of random-id retries before giving up (practically unreachable).
const ID_ATTEMPTS: usize = 8;

/// On-disk representation of a profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileFile {
    /// Format version, so future changes can migrate old files.
    pub version: u32,
    pub id: String,
    pub name: String,
    /// Optional: a learner without a group simply has no `grade` line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<u8>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl ProfileFile {
    /// Converts an in-memory profile into its file representation.
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            version: PROFILE_FILE_VERSION,
            id: profile.id().as_str().to_string(),
            name: profile.name().to_string(),
            grade: profile.grade().map(|grade| grade.value()),
            created_at: profile.created_at(),
        }
    }

    /// Converts a stored profile back into the domain type.
    pub fn to_profile(&self) -> Result<Profile, StorageError> {
        if self.version != PROFILE_FILE_VERSION {
            return Err(StorageError::UnsupportedSchema {
                found: self.version.to_string(),
                supported: PROFILE_FILE_VERSION,
            });
        }
        let id = ProfileId::parse(&self.id)?;
        let grade = self.grade.map(Grade::new).transpose()?;
        Ok(Profile::new(id, &self.name, grade, self.created_at)?)
    }
}

/// Owns the `profiles` directory and performs all profile file operations.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    /// Creates a store rooted at `<data_dir>/profiles`.
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        let root = data_dir.as_ref().join(PROFILES_DIR);
        Self { root }
    }

    /// The `profiles` directory itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// All profiles, sorted by display name so the list order is stable.
    ///
    /// A directory without a `profile.toml` (for example one left behind by an
    /// interrupted create) is skipped instead of aborting the whole listing.
    pub fn list(&self) -> Result<Vec<Profile>, StorageError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StorageError::io(&self.root, error)),
        };

        let mut profiles = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| StorageError::io(&self.root, e))?;
            if !entry
                .file_type()
                .map_err(|e| StorageError::io(entry.path(), e))?
                .is_dir()
            {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            // A directory name that is not a valid id cannot be one of ours.
            if validate_segment("profiel", &name).is_err() {
                continue;
            }
            if !entry.path().join(PROFILE_FILE).is_file() {
                continue;
            }
            profiles.push(self.load(&ProfileId::parse(&name)?)?);
        }

        profiles.sort_by(|left, right| {
            left.name()
                .to_lowercase()
                .cmp(&right.name().to_lowercase())
                .then_with(|| left.id().cmp(right.id()))
        });
        Ok(profiles)
    }

    /// Loads a single profile.
    pub fn load(&self, id: &ProfileId) -> Result<Profile, StorageError> {
        let path = self.profile_path(id)?;
        let raw = fs::read_to_string(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StorageError::ProfileNotFound(id.to_string())
            } else {
                StorageError::io(&path, error)
            }
        })?;
        let file: ProfileFile = toml::from_str(&raw).map_err(|source| StorageError::Toml {
            path: path.clone(),
            source,
        })?;
        if file.id != id.as_str() {
            return Err(StorageError::InvalidId {
                kind: "profiel",
                value: file.id,
                reason: format!("hoort bij profiel {id}"),
            });
        }
        file.to_profile()
    }

    /// Writes a profile, creating its directory when needed.
    pub fn save(&self, profile: &Profile) -> Result<(), StorageError> {
        let dir = self.profile_dir(profile.id())?;
        fs::create_dir_all(&dir).map_err(|e| StorageError::io(&dir, e))?;
        let path = self.profile_path(profile.id())?;
        let body =
            toml::to_string_pretty(&ProfileFile::from_profile(profile)).map_err(|source| {
                StorageError::TomlWrite {
                    path: path.clone(),
                    source,
                }
            })?;
        write_atomic(&path, body.as_bytes())
    }

    /// Creates a new profile with a fresh id and the current time.
    /// Creates a profile with `name` and an optional group.
    pub fn create(&self, name: &str, grade: Option<Grade>) -> Result<Profile, StorageError> {
        self.create_at(name, grade, OffsetDateTime::now_utc())
    }

    /// Creates a new profile with an explicit creation time (used by tests).
    pub fn create_at(
        &self,
        name: &str,
        grade: Option<Grade>,
        created_at: OffsetDateTime,
    ) -> Result<Profile, StorageError> {
        // Validate before touching the filesystem so an invalid name cannot
        // leave an empty directory behind.
        let taken = self.existing_ids()?;
        let mut id = None;
        for _ in 0..ID_ATTEMPTS {
            let candidate = ProfileId::generate(name);
            if taken.contains(candidate.as_str()) {
                continue;
            }
            id = Some(candidate);
            break;
        }
        let id = id.ok_or_else(|| StorageError::DuplicateId(name.trim().to_lowercase()))?;

        let profile = Profile::new(id, name, grade, created_at)?;
        self.save(&profile)?;
        Ok(profile)
    }

    /// Renames a profile in place. The id, and therefore all results, is kept.
    pub fn rename(&self, id: &ProfileId, new_name: &str) -> Result<Profile, StorageError> {
        let mut profile = self.load(id)?;
        profile.rename(new_name)?;
        self.save(&profile)?;
        Ok(profile)
    }

    /// Deletes a profile and everything stored for it.
    pub fn delete(&self, id: &ProfileId) -> Result<(), StorageError> {
        let dir = self.profile_dir(id)?;
        if !dir.is_dir() {
            return Err(StorageError::ProfileNotFound(id.to_string()));
        }
        fs::remove_dir_all(&dir).map_err(|e| StorageError::io(&dir, e))
    }

    /// Directory of a single profile: `<data_dir>/profiles/<id>`.
    pub fn profile_dir(&self, id: &ProfileId) -> Result<PathBuf, StorageError> {
        safe_join(&self.root, &[id.as_str()])
    }

    /// File of a single profile: `<data_dir>/profiles/<id>/profile.toml`.
    pub fn profile_path(&self, id: &ProfileId) -> Result<PathBuf, StorageError> {
        safe_join(&self.root, &[id.as_str(), PROFILE_FILE])
    }

    fn existing_ids(&self) -> Result<HashSet<String>, StorageError> {
        let mut ids = HashSet::new();
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.insert(name.to_string());
                }
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ValidationError;
    use tempfile::TempDir;
    use time::macros::datetime;

    fn store() -> (TempDir, ProfileStore) {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn create_list_and_delete_round_trip() {
        let (dir, store) = store();
        assert!(store.list().unwrap().is_empty());

        let sem = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let jan = store
            .create_at(
                "Jan",
                Some(Grade::new(3).unwrap()),
                datetime!(2026-10-03 12:00 UTC),
            )
            .unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 2);
        // Sorted by name.
        assert_eq!(listed[0].name(), "Jan");
        assert_eq!(listed[1].name(), "Sem");
        assert_eq!(listed[1].created_at(), datetime!(2026-10-02 12:00 UTC));

        // On-disk shape: <data_dir>/profiles/<id>/profile.toml
        let path = store.profile_path(sem.id()).unwrap();
        assert!(path.starts_with(dir.path()), "{path:?}");
        assert!(path.ends_with(PROFILE_FILE), "{path:?}");
        assert_eq!(
            path.parent()
                .and_then(Path::file_name)
                .and_then(|n| n.to_str()),
            Some(sem.id().as_str())
        );
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("version = 1"), "{raw}");
        assert!(raw.contains(&format!("id = \"{}\"", sem.id().as_str())));
        assert!(raw.contains("grade = 4"), "{raw}");

        store.delete(jan.id()).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id(), sem.id());
        assert!(!store.profile_dir(jan.id()).unwrap().exists());

        // Deleting twice is an error, not a panic.
        assert!(matches!(
            store.delete(jan.id()),
            Err(StorageError::ProfileNotFound(_))
        ));
    }

    #[test]
    fn renaming_keeps_the_id_and_the_directory() {
        let (_dir, store) = store();
        let profile = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let module_dir = store.profile_dir(profile.id()).unwrap().join("modules");
        fs::create_dir_all(&module_dir).unwrap();

        let renamed = store.rename(profile.id(), "Samuel").unwrap();
        assert_eq!(renamed.id(), profile.id());
        assert_eq!(renamed.name(), "Samuel");
        // The directory (and everything in it) is untouched.
        assert!(module_dir.exists());
        assert_eq!(store.load(profile.id()).unwrap().name(), "Samuel");
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn renaming_to_an_invalid_name_changes_nothing() {
        let (_dir, store) = store();
        let profile = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        assert!(matches!(
            store.rename(profile.id(), "  "),
            Err(StorageError::InvalidProfile(ValidationError::EmptyName))
        ));
        assert_eq!(store.load(profile.id()).unwrap().name(), "Sem");
    }

    #[test]
    fn writing_a_profile_is_atomic_and_leaves_no_temp_files() {
        let (dir, store) = store();
        let profile = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();

        let profile_dir = store.profile_dir(profile.id()).unwrap();
        let mut updated = profile.clone();
        updated.rename("Samuel").unwrap();
        store.save(&updated).unwrap();

        // Only the profile file itself, no leftover `.tmp`.
        let names: Vec<String> = fs::read_dir(&profile_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![PROFILE_FILE.to_string()]);

        let raw = fs::read_to_string(store.profile_path(profile.id()).unwrap()).unwrap();
        let parsed: ProfileFile = toml::from_str(&raw).unwrap();
        assert_eq!(parsed.name, "Samuel");
        assert_eq!(parsed.grade, Some(4));
        assert_eq!(parsed.version, PROFILE_FILE_VERSION);
        assert!(!raw.contains('\0'));
        assert!(dir.path().join(PROFILES_DIR).is_dir());
    }

    #[test]
    fn a_profile_file_without_a_grade_loads_as_a_profile_without_a_grade() {
        let (_dir, store) = store();
        let profile = store
            .create_at("Sem", None, datetime!(2026-10-02 12:00 UTC))
            .unwrap();
        let path = store.profile_path(profile.id()).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("grade"), "{raw}");

        let loaded = store.load(profile.id()).unwrap();
        assert_eq!(loaded.grade(), None);
        assert_eq!(loaded.name(), "Sem");
    }

    #[test]
    fn a_grade_outside_the_basisschool_range_is_rejected() {
        let (_dir, store) = store();
        let profile = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let path = store.profile_path(profile.id()).unwrap();
        let mut file: ProfileFile = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        file.grade = Some(99);
        fs::write(&path, toml::to_string_pretty(&file).unwrap()).unwrap();

        assert!(matches!(
            store.load(profile.id()),
            Err(StorageError::InvalidProfile(
                ValidationError::GradeOutOfRange { value: 99, .. }
            ))
        ));
    }

    #[test]
    fn a_file_with_the_wrong_version_is_rejected() {
        let (_dir, store) = store();
        let profile = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let path = store.profile_path(profile.id()).unwrap();
        let mut file: ProfileFile = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        file.version = 99;
        fs::write(&path, toml::to_string_pretty(&file).unwrap()).unwrap();

        assert!(matches!(
            store.load(profile.id()),
            Err(StorageError::UnsupportedSchema { supported: 1, .. })
        ));
    }

    #[test]
    fn a_file_that_claims_another_id_is_rejected() {
        let (_dir, store) = store();
        let first = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let second = store
            .create_at(
                "Jan",
                Some(Grade::new(3).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let path = store.profile_path(first.id()).unwrap();
        let raw = fs::read_to_string(&path)
            .unwrap()
            .replace(first.id().as_str(), second.id().as_str());
        fs::write(&path, raw).unwrap();

        assert!(matches!(
            store.load(first.id()),
            Err(StorageError::InvalidId { .. })
        ));
    }

    #[test]
    fn creating_the_same_name_twice_yields_two_profiles() {
        let (_dir, store) = store();
        let first = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let second = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        assert_ne!(first.id(), second.id());
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn directories_without_a_profile_file_are_ignored() {
        let (_dir, store) = store();
        store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        fs::create_dir_all(store.root().join("half-written")).unwrap();
        fs::write(store.root().join("not-a-profile.txt"), "x").unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name(), "Sem");
    }

    #[test]
    fn loading_an_unknown_profile_is_an_error() {
        let (_dir, store) = store();
        let id = ProfileId::parse("sem-00000000").unwrap();
        assert!(matches!(
            store.load(&id),
            Err(StorageError::ProfileNotFound(_))
        ));
        assert!(matches!(
            store.delete(&id),
            Err(StorageError::ProfileNotFound(_))
        ));
    }
}
