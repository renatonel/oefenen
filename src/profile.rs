//! The learner profile: identity, display name, school year and creation time.
//!
//! The [`ProfileId`] is generated once, when the profile is created, and is
//! never derived from the display name afterwards. Renaming a learner therefore
//! keeps the directory (and all recorded results) intact.
//!
//! The rules that make an id usable as a directory name live in
//! [`crate::storage::paths`]; this module reuses them so that the profile
//! layer and the storage layer can never disagree about what a valid id is.

use std::fmt;

use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::storage::error::StorageError;
use crate::storage::paths::{MAX_ID_LEN, slugify, validate_segment};

/// Longest display name we accept.
pub use crate::storage::paths::MAX_NAME_LEN;

/// First school year that has multiplication tables.
pub const GRADE_MIN: u8 = 3;
/// Last school year we offer exercises for.
pub const GRADE_MAX: u8 = 8;

/// Length of the random part of a generated id.
const ID_SUFFIX_LEN: usize = 8;

/// Why a profile could not be created or renamed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("de naam mag niet leeg zijn")]
    EmptyName,

    #[error("de naam mag maximaal {max} tekens lang zijn")]
    NameTooLong { max: usize },

    #[error("de naam mag geen regeleinde bevatten")]
    InvalidName,

    #[error("groep {value} bestaat niet (gebruik {min} tot en met {max})")]
    GradeOutOfRange { value: u8, min: u8, max: u8 },
}

/// Stable, filesystem-safe identifier of a profile.
///
/// Format: a readable slug of the name at creation time plus eight random hex
/// characters, for example `sem-3f9a2c71`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileId(String);

impl ProfileId {
    /// Validates an existing id.
    pub fn parse(value: &str) -> Result<Self, StorageError> {
        validate_segment("profiel", value)?;
        Ok(Self(value.to_string()))
    }

    /// Generates a new id from `name`.
    ///
    /// The random suffix keeps two children with the same name apart. It is
    /// drawn once, at creation time; [`ProfileStore::create`] retries with a
    /// fresh suffix if the candidate is already taken.
    pub fn generate(name: &str) -> Self {
        let random = Uuid::new_v4().simple().to_string();
        let suffix = &random[..ID_SUFFIX_LEN];
        let max_slug_len = MAX_ID_LEN - ID_SUFFIX_LEN - 1;
        let slug: String = slugify(name).chars().take(max_slug_len).collect();
        // `slugify` falls back to "profiel" for names without usable
        // characters, so `slug` is never empty.
        Self(format!("{slug}-{suffix}"))
    }

    /// The id as a path segment and as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// School year (`groep`) of a learner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Grade(u8);

impl Grade {
    pub const MIN: u8 = GRADE_MIN;
    pub const MAX: u8 = GRADE_MAX;

    /// Creates a grade, rejecting anything outside groep 3..=8.
    pub fn new(value: u8) -> Result<Self, ValidationError> {
        if (GRADE_MIN..=GRADE_MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ValidationError::GradeOutOfRange {
                value,
                min: GRADE_MIN,
                max: GRADE_MAX,
            })
        }
    }

    /// The numeric group, 3 to 8.
    pub fn value(self) -> u8 {
        self.0
    }

    /// Next group, or `None` at the top of the range.
    pub fn next(self) -> Option<Self> {
        Self::new(self.0.saturating_add(1)).ok()
    }

    /// The oldest group, the start of the range.
    pub fn last() -> Self {
        Self(Self::MAX)
    }

    /// Previous group, or `None` at the bottom of the range.
    pub fn previous(self) -> Option<Self> {
        if self.0 <= GRADE_MIN {
            return None;
        }
        Self::new(self.0.saturating_sub(1)).ok()
    }
}

impl Default for Grade {
    fn default() -> Self {
        Self(GRADE_MIN)
    }
}

impl fmt::Display for Grade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A learner, as stored in `profile.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    id: ProfileId,
    name: String,
    grade: Option<Grade>,
    created_at: OffsetDateTime,
}

impl Profile {
    /// Builds a profile, validating the name.
    ///
    /// The grade is optional: a profile without a group is valid, and its file
    /// simply leaves the field out.
    pub fn new(
        id: ProfileId,
        name: &str,
        grade: Option<Grade>,
        created_at: OffsetDateTime,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            id,
            name: validate_name(name)?.to_string(),
            grade,
            created_at,
        })
    }

    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The group, or `None` when the learner has no group yet.
    pub fn grade(&self) -> Option<Grade> {
        self.grade
    }

    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    /// Changes the display name; the id and therefore the stored results stay.
    pub fn rename(&mut self, name: &str) -> Result<(), ValidationError> {
        self.name = validate_name(name)?.to_string();
        Ok(())
    }

    /// Changes the school year.
    pub fn set_grade(&mut self, grade: Option<Grade>) {
        self.grade = grade;
    }
}

/// Trims the name and rejects anything we cannot show in the interface.
fn validate_name(name: &str) -> Result<&str, ValidationError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::EmptyName);
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(ValidationError::NameTooLong { max: MAX_NAME_LEN });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(ValidationError::InvalidName);
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn profile(name: &str) -> Profile {
        Profile::new(
            ProfileId::parse("sem-1a2b3c4d").unwrap(),
            name,
            Some(Grade::new(4).unwrap()),
            datetime!(2026-10-02 12:00 UTC),
        )
        .unwrap()
    }

    #[test]
    fn generated_ids_are_valid_and_readable() {
        let id = ProfileId::generate("Sém van Dijk");
        assert!(id.as_str().starts_with("s-m-van-dijk-"));
        assert_eq!(id.as_str().len(), "s-m-van-dijk-".len() + ID_SUFFIX_LEN);
        // Validating the generated id must succeed.
        ProfileId::parse(id.as_str()).unwrap();
    }

    #[test]
    fn generated_ids_stay_within_the_maximum_length() {
        let id = ProfileId::generate(&"a very long name that will not fit ".repeat(10));
        assert_eq!(id.as_str().len(), MAX_ID_LEN);
        ProfileId::parse(id.as_str()).unwrap();
    }

    #[test]
    fn generated_ids_differ_for_the_same_name() {
        let first = ProfileId::generate("Sem");
        let second = ProfileId::generate("Sem");
        assert_ne!(first, second);
        assert!(first.as_str().starts_with("sem-"));
        assert!(second.as_str().starts_with("sem-"));
    }

    #[test]
    fn renaming_keeps_the_id() {
        let mut learner = profile("Sem");
        let id = learner.id().clone();
        learner.rename("Samuel").unwrap();
        assert_eq!(learner.id(), &id);
        assert_eq!(learner.name(), "Samuel");
    }

    #[test]
    fn names_are_trimmed_and_validated() {
        assert_eq!(profile("  Sem  ").name(), "Sem");
        assert_eq!(
            profile("Sem").rename("   ").unwrap_err(),
            ValidationError::EmptyName
        );
        assert_eq!(
            profile("Sem")
                .rename(&"x".repeat(MAX_NAME_LEN + 1))
                .unwrap_err(),
            ValidationError::NameTooLong { max: MAX_NAME_LEN }
        );
        assert_eq!(
            profile("Sem").rename("Sem\nDijk").unwrap_err(),
            ValidationError::InvalidName
        );
    }

    #[test]
    fn a_profile_without_a_grade_is_valid() {
        let profile = Profile::new(
            ProfileId::generate("Sem"),
            "Sem",
            None,
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        assert_eq!(profile.grade(), None);
    }

    #[test]
    fn setting_the_grade_to_none_clears_it() {
        let mut profile = Profile::new(
            ProfileId::generate("Sem"),
            "Sem",
            Some(Grade::new(4).unwrap()),
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        profile.set_grade(None);
        assert_eq!(profile.grade(), None);
    }

    #[test]
    fn grades_are_limited_to_the_basisschool_range() {
        assert!(Grade::new(2).is_err());
        assert!(Grade::new(9).is_err());
        assert_eq!(Grade::new(3).unwrap().value(), 3);
        assert_eq!(Grade::new(8).unwrap().value(), 8);
        assert!(Grade::new(8).unwrap().next().is_none());
        assert!(Grade::new(3).unwrap().previous().is_none());
        assert_eq!(Grade::new(4).unwrap().next().unwrap().value(), 5);
        assert_eq!(Grade::last().value(), 8);
        assert_eq!(Grade::default().value(), Grade::MIN);
        assert_eq!(Grade::new(4).unwrap().previous().unwrap().value(), 3);
    }
}
