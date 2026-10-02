//! All user-facing text, in Dutch.
//!
//! Screens, widgets and modules must take their text from this module so that
//! no Dutch literal is scattered through the code base and so that a second
//! language can be added later by mirroring this one file.
//!
//! Fixed strings are `const`s, strings with placeholders are `fn`s that build a
//! [`String`].

use time::{Month, OffsetDateTime};

use crate::modules::Subject;

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

pub const APP_TITLE: &str = "Oefenen";
pub const TAGLINE: &str = "Oefen je tafels, lezen en topografie";

/// Shown when the terminal cannot fit the user interface.
pub const TOO_SMALL: &str = "Het venster is te klein.";
pub const TOO_SMALL_HINT: &str = "Maak het venster groter om verder te gaan.";

// ---------------------------------------------------------------------------
// Key names (used in the key-hint bar)
// ---------------------------------------------------------------------------

pub const KEY_ARROWS: &str = "↑ ↓";
pub const KEY_ENTER: &str = "Enter";
pub const KEY_ESC: &str = "Esc";
pub const KEY_TAB: &str = "Tab";
pub const KEY_BACKSPACE: &str = "Backspace";
pub const KEY_LEFTRIGHT: &str = "← →";

// Hint-bar labels.
pub const HINT_KIEZEN: &str = "kiezen";
pub const HINT_BEVESTIGEN: &str = "bevestigen";
pub const HINT_TERUG: &str = "terug";
pub const HINT_NAAR_LIJST: &str = "terug naar de lijst";
pub const HINT_OPSLAAN: &str = "opslaan";
pub const HINT_WISSEN: &str = "wissen";
pub const HINT_VERWIJDEREN: &str = "verwijderen";
pub const HINT_NIEUW_VELD: &str = "volgend veld";
pub const HINT_GROEP_KIEZEN: &str = "groep kiezen";
pub const HINT_ANTWOORD: &str = "antwoord insturen";
pub const HINT_JA_NEE: &str = "ja of nee";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

pub const ERROR_TITLE: &str = "Er ging iets mis";
pub const ERROR_QUIT: &str = "Druk op q om te stoppen.";
pub const PROFILE_NAME_EMPTY: &str = "Vul eerst een naam in.";
pub const PROFILE_NAME_TOO_LONG: &str = "Deze naam is te lang.";
pub const INVALID_GRADE: &str = "Kies een groep tussen 3 en 8.";

// ---------------------------------------------------------------------------
// Profile selection
// ---------------------------------------------------------------------------

pub const CHOOSE_PROFILE: &str = "Kies je profiel";
pub const NO_PROFILES: &str = "Er zijn nog geen profielen. Maak er eerst één aan.";
pub const NEW_PROFILE: &str = "Nieuw profiel";
pub const RENAME_PROFILE: &str = "Naam wijzigen";
pub const DELETE_PROFILE: &str = "Verwijder profiel";
pub const NO_PROFILES_HINT: &str = "Druk op n om een nieuw profiel aan te maken.";

/// `count` profielen (1: "1 profiel", 2: "2 profielen").
pub fn profile_count(count: usize) -> String {
    if count == 1 {
        "1 profiel".to_string()
    } else {
        format!("{count} profielen")
    }
}

/// `count` oefeningen (1: "1 oefening", 2: "2 oefeningen").
pub fn module_count(count: usize) -> String {
    if count == 1 {
        "1 oefening".to_string()
    } else {
        format!("{count} oefeningen")
    }
}

pub fn grade_label(grade: u8) -> String {
    format!("groep {grade}")
}

pub const NO_GRADE: &str = "geen groep";

pub fn grade_or_none(grade: Option<u8>) -> String {
    match grade {
        Some(grade) => grade_label(grade),
        None => NO_GRADE.to_string(),
    }
}

pub fn created_on(created_at: &OffsetDateTime) -> String {
    format!("aangemaakt op {}", format_date(created_at))
}

// ---------------------------------------------------------------------------
// Creating and editing a profile
// ---------------------------------------------------------------------------

pub const CREATE_PROFILE: &str = "Nieuw profiel";
pub const EDIT_PROFILE: &str = "Naam wijzigen";
pub const FIELD_NAME: &str = "Naam";
pub const FIELD_GRADE: &str = "Groep";
pub const FIELD_SAVE: &str = "Opslaan";

/// Placeholder shown in the empty name input.
pub const NAME_PLACEHOLDER: &str = "typ je naam";

pub fn created(name: &str) -> String {
    format!("Profiel \"{name}\" is aangemaakt.")
}

pub fn renamed(name: &str) -> String {
    format!("Profiel \"{name}\" is hernoemd.")
}

pub fn deleted(name: &str) -> String {
    format!("Profiel \"{name}\" is verwijderd.")
}

// ---------------------------------------------------------------------------
// Deleting a profile
// ---------------------------------------------------------------------------

pub fn confirm_delete(name: &str) -> String {
    format!("Wil je \"{name}\" echt verwijderen? Dit kan niet ongedaan gemaakt worden.")
}

pub const YES: &str = "Ja, verwijder";
pub const NO: &str = "Nee, teru";

// ---------------------------------------------------------------------------
// Module list
// ---------------------------------------------------------------------------

pub const CHOOSE_MODULE: &str = "Kies een oefening";
pub const NO_MODULES: &str = "Er zijn nog geen oefeningen beschikbaar.";
pub const BACK_TO_PROFILES: &str = "Terug naar je profielen";
pub const QUIT: &str = "Stoppen";

/// Subject heading used to group the module list.
pub fn subject_label(subject: Subject) -> &'static str {
    match subject {
        Subject::Math => "Rekenen",
        Subject::Reading => "Lezen",
        Subject::Geography => "Topografie",
    }
}

// ---------------------------------------------------------------------------
// Multiplication module (dummy drill)
// ---------------------------------------------------------------------------

pub mod multiplication {
    pub const TITLE: &str = "Tafels oefenen";
    pub const DESCRIPTION: &str = "Oefen je tafels met een timer.";
    pub const PLACEHOLDER: &str = "Oefening in wording";
    pub const QUESTION_LABEL: &str = "Vraag";
    pub const ANSWER_LABEL: &str = "Jouw antwoord";
    pub const ELAPSED_LABEL: &str = "Tijd";
    pub const TYPE_DIGITS: &str = "Typ je antwoord en druk op Enter";
    pub const CORRECT: &str = "Goed gedaan!";
    pub const INCORRECT: &str = "Niet goed.";

    /// Feedback after a wrong answer, including the right answer.
    pub fn correct_answer(answer: u32) -> String {
        format!("Het juiste antwoord is {answer}.")
    }

    pub const PRESS_ENTER: &str = "Druk op Enter om terug te gaan naar de oefeningen.";
}

/// Formats a running time as `mm:ss.t` (tenths of a second, Dutch decimal comma).
pub fn format_duration(millis: u64) -> String {
    let total_tenths = millis / 100;
    let minutes = total_tenths / 600;
    let seconds = (total_tenths / 10) % 60;
    let tenths = total_tenths % 10;
    format!("{minutes:02}:{seconds:02},{tenths}")
}

/// Formats a date as `2 oktober 2026`.
pub fn format_date(value: &OffsetDateTime) -> String {
    let month = month_name(value.month());
    format!("{} {} {}", value.day(), month, value.year())
}

fn month_name(month: Month) -> &'static str {
    match month {
        Month::January => "januari",
        Month::February => "februari",
        Month::March => "maart",
        Month::April => "april",
        Month::May => "mei",
        Month::June => "juni",
        Month::July => "juli",
        Month::August => "augustus",
        Month::September => "september",
        Month::October => "oktober",
        Month::November => "november",
        Month::December => "december",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn counts_profiles_with_and_without_plural() {
        assert_eq!(profile_count(0), "0 profielen");
        assert_eq!(profile_count(1), "1 profiel");
        assert_eq!(profile_count(4), "4 profielen");
        assert_eq!(module_count(1), "1 oefening");
        assert_eq!(module_count(3), "3 oefeningen");
    }

    #[test]
    fn formats_durations_with_a_decimal_comma() {
        assert_eq!(format_duration(0), "00:00,0");
        assert_eq!(format_duration(1_500), "00:01,5");
        assert_eq!(format_duration(75_000), "01:15,0");
        assert_eq!(format_duration(600_000), "10:00,0");
    }

    #[test]
    fn formats_dutch_dates() {
        assert_eq!(
            format_date(&datetime!(2026-01-05 0:00 UTC)),
            "5 januari 2026"
        );
        assert_eq!(
            format_date(&datetime!(2026-10-02 12:00 UTC)),
            "2 oktober 2026"
        );
        assert_eq!(
            format_date(&datetime!(2026-12-31 12:00 UTC)),
            "31 december 2026"
        );
    }

    #[test]
    fn every_subject_has_a_dutch_label() {
        for subject in [Subject::Math, Subject::Reading, Subject::Geography] {
            assert!(!subject_label(subject).is_empty());
        }
        assert_eq!(subject_label(Subject::Math), "Rekenen");
    }
}
