//! The choices the learner made last time, per profile.
//!
//! Stored in the module's own directory as `settings.toml`, so the setup screens
//! come back pre-filled. Reading is deliberately forgiving: a missing file, a
//! damaged file, a file from a future version and a file with impossible values
//! all end up as [`Settings::default`]. A learner must never be locked out of
//! the module by a bad settings file.

use serde::{Deserialize, Serialize};

use crate::modules::tafels::questions::{TABLE_MAX, TABLE_MIN};
use crate::modules::tafels::timer::TimerMode;
use crate::storage::{ModuleStore, StorageError};

/// File inside the module directory that holds the last used choices.
pub const SETTINGS_FILE: &str = "settings.toml";

/// Version of the file format written by this build.
pub const SETTINGS_VERSION: u32 = 1;

/// Shortest countdown that still makes sense.
pub const MIN_SECONDS: u64 = 1;
/// Longest countdown a learner can pick (an hour).
pub const MAX_SECONDS: u64 = 3_600;
/// Seconds per question in the per-question mode when nothing was stored.
pub const DEFAULT_SECONDS: u64 = 10;
/// Longest custom entry, in digits.
pub const MAX_CUSTOM_DIGITS: usize = 4;

/// The choices behind one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Selected tables, ascending and within the available range.
    pub tables: Vec<u32>,
    /// Also ask the reversed form of every fact.
    pub reverse: bool,
    pub timer_mode: TimerMode,
    /// Countdown in seconds for the modes that have one; ignored without a
    /// limit, but remembered so switching modes back keeps the old number.
    pub seconds: u64,
}

impl Default for Settings {
    /// No tables (the learner must pick), no reversed facts, and no clock.
    fn default() -> Self {
        Self {
            tables: Vec::new(),
            reverse: false,
            timer_mode: TimerMode::None,
            seconds: DEFAULT_SECONDS,
        }
    }
}

impl Settings {
    /// The settings used when nothing was stored: 10 seconds per question, but
    /// no clock until the learner asks for one.
    pub fn preset(seconds: u64) -> Self {
        Self {
            seconds: clamp_seconds(seconds),
            ..Self::default()
        }
    }

    /// The countdown in seconds, or `None` without a limit.
    ///
    /// This is what goes into the session record, and it never lies about a
    /// mode that cannot expire.
    pub fn limit_seconds(&self) -> Option<u64> {
        self.timer_mode
            .has_limit()
            .then_some(clamp_seconds(self.seconds))
    }

    /// Number of questions these settings will ask.
    pub fn question_count(&self) -> usize {
        super::questions::generate(&self.tables, self.reverse).len()
    }

    /// Drops impossible values: tables outside the range, and a countdown of
    /// zero seconds or more than an hour.
    fn sanitised(&self) -> Self {
        let mut tables: Vec<u32> = self
            .tables
            .iter()
            .copied()
            .filter(|table| (TABLE_MIN..=TABLE_MAX).contains(table))
            .collect();
        tables.sort_unstable();
        tables.dedup();
        Self {
            tables,
            reverse: self.reverse,
            timer_mode: self.timer_mode,
            seconds: clamp_seconds(self.seconds),
        }
    }
}

/// Keeps a countdown inside the range the screen allows.
fn clamp_seconds(seconds: u64) -> u64 {
    seconds.clamp(MIN_SECONDS, MAX_SECONDS)
}

/// The on-disk shape of `settings.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsFile {
    /// Format version, so a file from another build is recognised.
    pub version: u32,
    /// Selected tables.
    #[serde(default)]
    pub tables: Vec<u32>,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default = "default_timer_mode")]
    pub timer_mode: TimerMode,
    #[serde(default = "default_seconds")]
    pub seconds: u64,
}

fn default_timer_mode() -> TimerMode {
    TimerMode::None
}

fn default_seconds() -> u64 {
    DEFAULT_SECONDS
}

impl From<&Settings> for SettingsFile {
    fn from(settings: &Settings) -> Self {
        Self {
            version: SETTINGS_VERSION,
            tables: settings.tables.clone(),
            reverse: settings.reverse,
            timer_mode: settings.timer_mode,
            seconds: settings.seconds,
        }
    }
}

impl SettingsFile {
    /// The stored settings, with impossible values dropped.
    pub fn to_settings(&self) -> Settings {
        if self.version != SETTINGS_VERSION {
            // A file from another build: readable, but not ours to interpret.
            return Settings::default();
        }
        Settings {
            tables: self.tables.clone(),
            reverse: self.reverse,
            timer_mode: self.timer_mode,
            seconds: self.seconds,
        }
        .sanitised()
    }
}

/// Reads the last used settings, falling back to [`Settings::default`].
///
/// Never fails and never panics: a corrupt file is treated like no file.
pub fn load(store: &ModuleStore) -> Settings {
    match store.read_document::<SettingsFile>(SETTINGS_FILE) {
        Ok(Some(file)) => file.to_settings(),
        // No settings yet, an unreadable file or a damaged one: start over.
        Ok(None) | Err(_) => Settings::default(),
    }
}

/// Writes the settings atomically.
pub fn save(store: &ModuleStore, settings: &Settings) -> Result<(), StorageError> {
    store.write_document(SETTINGS_FILE, &SettingsFile::from(settings))
}

/// Reads a custom time typed on the settings screen.
///
/// Up to two digits are seconds (`30` is half a minute); three or four are
/// minutes and seconds (`0130` is a minute and a half). Typing a colon would
/// need Shift on most layouts, so the length of the number decides.
pub fn parse_custom_time(text: &str) -> Option<u64> {
    if text.is_empty()
        || text.len() > MAX_CUSTOM_DIGITS
        || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let seconds = match text.len() {
        1 | 2 => text.parse::<u64>().ok()?,
        _ => {
            let (minutes, rest) = text.split_at(text.len() - 2);
            let rest: u64 = rest.parse().ok()?;
            // 0300 must not mean three minutes of an invalid 100 seconds.
            if rest >= 60 {
                return None;
            }
            minutes
                .parse::<u64>()
                .ok()?
                .saturating_mul(60)
                .saturating_add(rest)
        }
    };
    (MIN_SECONDS..=MAX_SECONDS)
        .contains(&seconds)
        .then_some(seconds)
}

/// Builds the preset list of the per-question mode.
pub fn per_question_presets() -> [u64; 4] {
    [5, 10, 15, 20]
}

/// Seconds per question for the three total-countdown speeds.
pub fn total_speeds() -> [u64; 3] {
    [4, 6, 10]
}

/// The total number of seconds a speed preset gives for `questions` questions.
pub fn total_for_speed(speed_seconds: u64, questions: usize) -> u64 {
    let questions = u64::try_from(questions).unwrap_or(u64::MAX);
    speed_seconds
        .saturating_mul(questions)
        .clamp(MIN_SECONDS, MAX_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Grade;
    use crate::storage::ProfileStore;
    use tempfile::TempDir;
    use time::macros::datetime;

    /// A module store in a throwaway directory, plus its data directory.
    fn store() -> (TempDir, ModuleStore) {
        let dir = TempDir::new().unwrap();
        let profile = ProfileStore::new(dir.path())
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let store = ModuleStore::new(dir.path(), profile.id(), "tafels").unwrap();
        (dir, store)
    }

    fn settings() -> Settings {
        Settings {
            tables: vec![3, 7],
            reverse: true,
            timer_mode: TimerMode::PerQuestion,
            seconds: 15,
        }
    }

    #[test]
    fn the_defaults_match_the_documented_fallback() {
        let default = Settings::default();
        assert!(default.tables.is_empty(), "geen tafel voorgeselecteerd");
        assert!(!default.reverse);
        assert_eq!(default.timer_mode, TimerMode::None);
        assert_eq!(default.seconds, DEFAULT_SECONDS);
        // The fallback time is remembered even while there is no clock.
        assert_eq!(default.limit_seconds(), None);
        assert_eq!(Settings::preset(10).seconds, 10);
    }

    #[test]
    fn settings_round_trip_through_the_module_directory() {
        let (_dir, store) = store();
        assert_eq!(load(&store), Settings::default());

        save(&store, &settings()).unwrap();
        assert_eq!(load(&store), settings());

        // The file itself carries the version and lives where it should.
        let raw = std::fs::read_to_string(store.path_for(SETTINGS_FILE).unwrap()).unwrap();
        assert!(raw.contains("version = 1"), "{raw}");
        assert!(raw.contains("timer_mode = \"per_question\""), "{raw}");
        assert!(raw.contains("seconds = 15"), "{raw}");
        assert!(raw.contains("tables = ["), "{raw}");
        assert!(raw.contains("reverse = true"), "{raw}");
        // Only the settings file is written; nothing is appended as a log.
        let files: Vec<String> = std::fs::read_dir(store.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, vec![SETTINGS_FILE.to_string()], "geen logbestanden");
    }

    #[test]
    fn a_corrupt_settings_file_falls_back_to_the_defaults() {
        let (_dir, store) = store();
        store
            .write_document(SETTINGS_FILE, &SettingsFile::from(&settings()))
            .unwrap();

        // Not TOML at all.
        std::fs::write(store.path_for(SETTINGS_FILE).unwrap(), "= dit is geen toml").unwrap();
        assert_eq!(load(&store), Settings::default());

        // TOML, but the wrong shape.
        std::fs::write(
            store.path_for(SETTINGS_FILE).unwrap(),
            "tables = \"drie\"\n",
        )
        .unwrap();
        assert_eq!(load(&store), Settings::default());

        // Valid TOML of a version this build does not know.
        std::fs::write(
            store.path_for(SETTINGS_FILE).unwrap(),
            "version = 99\ntables = [7]\nseconds = 5\n",
        )
        .unwrap();
        assert_eq!(load(&store), Settings::default());
    }

    #[test]
    fn impossible_values_are_dropped_instead_of_kept() {
        let file = SettingsFile {
            version: SETTINGS_VERSION,
            tables: vec![99, 0, 7, 7, 3, 12],
            reverse: true,
            timer_mode: TimerMode::Total,
            seconds: 0,
        };
        let loaded = file.to_settings();
        assert_eq!(
            loaded.tables,
            vec![3, 7],
            "buiten het bereik eruit, dubbel eruit"
        );
        assert_eq!(loaded.seconds, MIN_SECONDS, "nul seconden is onbruikbaar");
        assert!(loaded.reverse);
        assert_eq!(loaded.timer_mode, TimerMode::Total);

        let too_long = SettingsFile {
            version: SETTINGS_VERSION,
            tables: vec![7],
            reverse: false,
            timer_mode: TimerMode::PerQuestion,
            seconds: 99_999,
        };
        assert_eq!(too_long.to_settings().seconds, MAX_SECONDS);
    }

    #[test]
    fn a_settings_file_without_the_newer_fields_still_loads() {
        let (_dir, store) = store();
        // Only version and tables: the other keys must not be required.
        let path = store.path_for(SETTINGS_FILE).unwrap();
        std::fs::create_dir_all(path.parent().expect("modulemap")).unwrap();
        std::fs::write(&path, "version = 1\ntables = [2]\n").unwrap();
        let loaded = load(&store);
        assert_eq!(loaded.tables, vec![2]);
        assert!(!loaded.reverse);
        assert_eq!(loaded.timer_mode, TimerMode::None);
        assert_eq!(loaded.seconds, DEFAULT_SECONDS);
    }

    #[test]
    fn the_question_count_follows_the_selected_tables() {
        let mut settings = Settings::default();
        assert_eq!(settings.question_count(), 0);
        settings.tables = vec![7];
        assert_eq!(settings.question_count(), 10);
        settings.reverse = true;
        assert_eq!(settings.question_count(), 19);
    }

    #[test]
    fn a_custom_time_is_read_as_seconds_or_as_minutes() {
        assert_eq!(parse_custom_time("5"), Some(5));
        assert_eq!(parse_custom_time("30"), Some(30));
        assert_eq!(parse_custom_time("90"), Some(90));
        // Four digits are minutes and seconds.
        assert_eq!(parse_custom_time("0130"), Some(90));
        assert_eq!(parse_custom_time("1230"), Some(750));
        // Three digits: one minute, two seconds.
        assert_eq!(parse_custom_time("130"), Some(90));
    }

    #[test]
    fn an_unusable_custom_time_is_refused() {
        // "1259" is twelve minutes and fifty-nine seconds, so it is *not* in this list.
        for bad in ["", "a", "12a", "0", "0075", "1260", "99999", "1 2"] {
            assert_eq!(
                parse_custom_time(bad),
                None,
                "{bad:?} moet geweigerd worden"
            );
        }
        // Sixty minutes is the longest time the module accepts.
        assert_eq!(parse_custom_time("6000"), Some(MAX_SECONDS));
        assert_eq!(parse_custom_time("6001"), None);
    }

    #[test]
    fn a_speed_preset_is_the_question_count_times_its_speed() {
        assert_eq!(total_for_speed(4, 10), 40);
        assert_eq!(total_for_speed(6, 10), 60);
        assert_eq!(total_for_speed(10, 10), 100);
        assert_eq!(per_question_presets(), [5, 10, 15, 20]);
        assert_eq!(total_speeds(), [4, 6, 10]);
        // A huge question count is capped instead of overflowing.
        assert_eq!(total_for_speed(10, usize::MAX), MAX_SECONDS);
    }
}
