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
/// The number keys, as one hint key.
pub const KEY_DIGITS: &str = "0-9";
/// The space bar, as one hint key.
pub const KEY_SPACE: &str = "spatie";

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
pub const HINT_VINKJE: &str = "tafel aan of uit";
pub const HINT_ALLES_AAN: &str = "alles aan";
pub const HINT_ALLES_UIT: &str = "alles uit";
pub const HINT_DOORGAAN: &str = "doorgaan";
pub const HINT_SNELDER: &str = "volgende vraag";
pub const HINT_TYPEN: &str = "tijd intypen";
pub const HINT_CIJFERS: &str = "alleen cijfers";
pub const HINT_NEE: &str = "nee, doorgaan";

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
// Tafels (multiplication tables)
// ---------------------------------------------------------------------------

pub mod tafels {
    use super::format_mmss;

    /// The multiplication sign, as it is shown and stored.
    pub const TIMES: &str = "×";

    pub const TITLE: &str = "Tafels oefenen";
    pub const DESCRIPTION: &str = "Oefen je tafels met een timer.";

    // --- screen 1: tafel selection ----------------------------------------

    pub const CHOOSE_TABLES: &str = "Kies je tafels";
    pub const START: &str = "Beginnen";
    pub const REVERSE: &str = "Ook omgekeerd (bijv. 8 × 7 naast 7 × 8)";
    pub const REVERSE_SHORT: &str = "ook omgekeerd";
    pub const NO_TABLES: &str = "Kies minstens één tafel.";

    /// `Tafel 7`.
    pub fn table_label(table: u32) -> String {
        format!("Tafel {table}")
    }

    /// `3 vragen · omgekeerd nee`, shown at the bottom of the tafel screen.
    pub fn question_summary(count: usize, reverse: bool) -> String {
        let questions = if count == 1 {
            "1 vraag".to_string()
        } else {
            format!("{count} vragen")
        };
        format!(
            "{questions} · omgekeerd {}",
            if reverse { "ja" } else { "nee" }
        )
    }

    // --- screen 2: timer settings -----------------------------------------

    pub const CHOOSE_TIMER: &str = "Kies je tijd";

    pub const MODE_PER_QUESTION: &str = "Seconden per vraag";
    pub const MODE_TOTAL: &str = "Totale tijd";
    pub const MODE_NONE: &str = "Geen tijdslimiet";

    pub const MODE_HELP_PER_QUESTION: &str = "Elke vraag krijgt een eigen klok.";
    pub const MODE_HELP_TOTAL: &str = "Eén klok voor de hele oefening.";
    pub const MODE_HELP_NONE: &str = "Geen klok: antwoord wanneer je wilt.";

    /// `5 seconden`.
    pub fn seconds(count: u64) -> String {
        if count == 1 {
            "1 seconde".to_string()
        } else {
            format!("{count} seconden")
        }
    }

    /// The three speeds of the total countdown, in the same order as the
    /// presets: snel, normaal, rustig.
    pub const SPEEDS: [&str; 3] = ["Snel", "Normaal", "Rustig"];

    /// `01:20 totaal`, the time a total-countdown preset would give.
    pub fn total_for(seconds: u64) -> String {
        format!("{} totaal", format_mmss(seconds * 1000))
    }

    pub const CUSTOM: &str = "Eigen tijd";
    pub const CUSTOM_HINT: &str =
        "1 of 2 cijfers zijn seconden; 4 cijfers zijn minuten en seconden (0130).";
    pub const INVALID_TIME: &str = "Vul een tijd tussen 0:01 en 1:00:00 in.";

    /// `20 vragen`.
    pub fn question_count(count: usize) -> String {
        if count == 1 {
            "1 vraag".to_string()
        } else {
            format!("{count} vragen")
        }
    }

    /// `Tafels: 3, 7 en 8`.
    pub fn tables_summary(tables: &[u32], reverse: bool) -> String {
        let list = match tables {
            [] => "-".to_string(),
            [only] => only.to_string(),
            _ => {
                let head = tables[..tables.len() - 1]
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("{head} en {}", tables[tables.len() - 1])
            }
        };
        format!(
            "Tafels: {list}{}",
            if reverse { " (met omgekeerde)" } else { "" }
        )
    }

    // --- screen 3: the session --------------------------------------------

    pub const TIME_LABEL: &str = "Tijd";

    /// `Score: 7/20`.
    pub fn score(correct: usize, total: usize) -> String {
        format!("Score: {}", score_value(correct, total))
    }

    /// `7/20`, for where the word "Score" is already on screen.
    pub fn score_value(correct: usize, total: usize) -> String {
        format!("{correct}/{total}")
    }

    /// `Vraag 8 van 20`.
    pub fn question_of(index: usize, total: usize) -> String {
        format!("Vraag {} van {total}", index + 1)
    }

    pub const TYPE_DIGITS: &str = "Typ je antwoord en druk op Enter";
    /// Short reminder shown next to the correct answer after a mistake.
    pub const TYPE_DIGITS_HINT: &str = "typ alleen cijfers";
    pub const GOOD: &str = "Goed!";
    /// Wrong or too late: the correct answer is always shown.
    pub fn the_answer_is(answer: u32) -> String {
        format!("Het antwoord is {answer}")
    }

    /// `Fout: het antwoord is 42`, or `Te laat: het antwoord is 42`.
    pub fn wrong_answer_is(verdict: &str, answer: u32) -> String {
        format!("{verdict}: {}", the_answer_is(answer))
    }
    pub const TOO_LATE: &str = "Te laat!";

    pub const STOP_CONFIRM: &str = "Stoppen? Je voortgang wordt bewaard";
    pub const STOP_YES: &str = "Ja, stoppen";
    pub const STOP_NO: &str = "Nee, doorgaan";

    pub const TERMINAL_TOO_SMALL: &str = "Terminal te klein";
    /// Shorter cousin of [`TERMINAL_TOO_SMALL`], for the question line itself,
    /// where there is barely room for a word.
    pub const QUESTION_TOO_SMALL: &str = "Past niet";
    pub const TERMINAL_TOO_SMALL_HINT: &str = "Maak het venster groter om verder te gaan.";

    // --- screen 4: the results --------------------------------------------

    pub const RESULTS_TITLE: &str = "Je bent klaar";

    pub const RESULT_SCORE: &str = "Score";
    pub const RESULT_ACCURACY: &str = "Goed";
    pub const RESULT_TOTAL_TIME: &str = "Totale tijd";
    pub const RESULT_AVERAGE: &str = "Gemiddelde reactietijd";

    pub const MISTAKES_TITLE: &str = "Nog goed te oefenen";
    pub const NO_MISTAKES: &str = "Alles goed! Geef jezelf een compliment.";
    /// `… en nog 4`.
    pub fn more(count: usize) -> String {
        format!("… en nog {count}")
    }

    /// Short outcome labels for the mistakes list.
    pub const OUTCOME_INCORRECT: &str = "Fout";
    pub const OUTCOME_TIMED_OUT: &str = "Te laat";
    pub const OUTCOME_NOT_REACHED: &str = "Niet gevraagd";
    /// `Fout · 4,2 s`.
    pub fn outcome_with_time(label: &str, millis: Option<u64>) -> String {
        match millis {
            Some(millis) => format!("{label} · {} s", super::format_tenths(millis)),
            None => label.to_string(),
        }
    }

    pub const OPTION_REPLAY: &str = "Opnieuw";
    pub const OPTION_SETTINGS: &str = "Andere instellingen";
    pub const OPTION_MENU: &str = "Terug naar menu";
}

/// Formats a duration as `mm:ss`, used by the total countdown and the results.
pub fn format_mmss(millis: u64) -> String {
    let total_seconds = millis / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

/// Formats a duration as `mm:ss.t` (tenths of a second, Dutch decimal comma).
pub fn format_duration(millis: u64) -> String {
    let total_tenths = millis / 100;
    let minutes = total_tenths / 600;
    let seconds = (total_tenths / 10) % 60;
    let tenths = total_tenths % 10;
    format!("{minutes:02}:{seconds:02},{tenths}")
}

/// Formats a duration in seconds with one decimal and a decimal comma (`2,4`).
pub fn format_tenths(millis: u64) -> String {
    format!("{},{}", millis / 1000, (millis % 1000) / 100)
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
    fn formats_minutes_and_seconds() {
        assert_eq!(format_mmss(0), "00:00");
        assert_eq!(format_mmss(1_000), "00:01");
        assert_eq!(format_mmss(80_000), "01:20");
        // A long session grows past two digits instead of wrapping.
        assert_eq!(format_mmss(7_500_000), "125:00");
    }

    #[test]
    fn formats_tenths_of_a_second() {
        assert_eq!(format_tenths(0), "0,0");
        assert_eq!(format_tenths(4_240), "4,2");
        assert_eq!(format_tenths(999), "0,9");
    }

    #[test]
    fn the_tafels_strings_are_filled_in() {
        assert_eq!(tafels::score(7, 20), "Score: 7/20");
        assert_eq!(tafels::question_of(7, 20), "Vraag 8 van 20");
        assert_eq!(tafels::the_answer_is(56), "Het antwoord is 56");
        assert_eq!(tafels::seconds(1), "1 seconde");
        assert_eq!(tafels::seconds(15), "15 seconden");
        assert_eq!(tafels::question_count(1), "1 vraag");
        assert_eq!(tafels::question_count(20), "20 vragen");
        assert_eq!(
            tafels::question_summary(0, false),
            "0 vragen · omgekeerd nee"
        );
        assert_eq!(tafels::question_summary(1, true), "1 vraag · omgekeerd ja");
        assert_eq!(tafels::tables_summary(&[], false), "Tafels: -");
        assert_eq!(tafels::tables_summary(&[7], false), "Tafels: 7");
        assert_eq!(
            tafels::tables_summary(&[3, 7], true),
            "Tafels: 3 en 7 (met omgekeerde)"
        );
        assert_eq!(tafels::total_for(80), "01:20 totaal");
        assert_eq!(tafels::more(4), "… en nog 4");
        assert_eq!(tafels::START, "Beginnen");
        assert_eq!(tafels::TYPE_DIGITS_HINT, "typ alleen cijfers");
        assert_eq!(
            tafels::outcome_with_time(tafels::OUTCOME_INCORRECT, None),
            "Fout"
        );
        assert_eq!(
            tafels::outcome_with_time(tafels::OUTCOME_TIMED_OUT, Some(4_240)),
            "Te laat · 4,2 s"
        );
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
