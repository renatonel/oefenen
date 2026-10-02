//! Input and clock events.
//!
//! The application and the modules only ever see the [`Event`] and [`Key`]
//! types defined here, never `crossterm` types. That keeps the whole state
//! machine and every module testable without a TTY; the translation from
//! crossterm happens in a single place at the bottom of this file.

use std::time::Duration;

use crossterm::event::{
    Event as CtEvent, KeyCode as CtKeyCode, KeyEvent as CtKeyEvent, KeyEventKind, KeyModifiers,
};

/// Duration of one clock tick. Every running timer in the app is driven by
/// these ticks, never by blocking calls.
pub const DEFAULT_TICK_MS: u64 = 75;

/// A single event that the application reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A key press (including Ctrl+C, mapped to [`Key::CtrlC`]).
    Key(Key),
    /// Regular clock tick; timers advance on these.
    Tick,
    /// The terminal was resized.
    Resize { width: u16, height: u16 },
}

/// Terminal-independent representation of a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Backspace,
    Delete,
    /// Ctrl+C, the conventional "quit" chord.
    CtrlC,
    /// Any key we do not act on (function keys, media keys, ...).
    Other,
}

impl Key {
    /// Convenience constructor used by tests and by the crossterm translation.
    pub fn char(value: char) -> Self {
        Self::Char(value)
    }

    /// Returns the letter of a plain character key, ignoring case.
    ///
    /// Lets screens and modules accept both `q` and `Q` without repeating the
    /// case handling.
    pub fn as_letter(&self) -> Option<char> {
        match self {
            Self::Char(c) => Some(c.to_ascii_lowercase()),
            _ => None,
        }
    }
}

/// Convenience helper for `Event::Key`.
pub fn key_event(key: Key) -> Event {
    Event::Key(key)
}

/// Duration of one tick, derived from the `--tick-ms` CLI option.
pub fn tick_duration(tick_ms: u64) -> Duration {
    Duration::from_millis(tick_ms)
}

/// Translates a crossterm event into an application event.
///
/// Key *release* events and the two kinds of key repeat crossterm may emit are
/// folded into a single [`Key`] press, and the modifiers we do not model are
/// dropped.
pub fn translate(event: CtEvent) -> Event {
    match event {
        CtEvent::Key(key) => translate_key(key),
        CtEvent::Resize(width, height) => Event::Resize { width, height },
        CtEvent::FocusGained | CtEvent::FocusLost | CtEvent::Mouse(_) | CtEvent::Paste(_) => {
            // No application behaviour depends on these yet.
            Event::Key(Key::Other)
        }
    }
}

fn translate_key(key: CtKeyEvent) -> Event {
    // `KeyEventState` is a platform capability report, not a user intent.
    if key.kind == KeyEventKind::Release {
        return Event::Key(Key::Other);
    }

    let code = match (key.code, key.modifiers) {
        (CtKeyCode::Char('c' | 'C'), mods) if mods.contains(KeyModifiers::CONTROL) => Key::CtrlC,
        (CtKeyCode::Char(c), _) => Key::Char(c),
        (CtKeyCode::Enter, _) => Key::Enter,
        (CtKeyCode::Esc, _) => Key::Esc,
        (CtKeyCode::Up, _) => Key::Up,
        (CtKeyCode::Down, _) => Key::Down,
        (CtKeyCode::Left, _) => Key::Left,
        (CtKeyCode::Right, _) => Key::Right,
        (CtKeyCode::Home, _) => Key::Home,
        (CtKeyCode::End, _) => Key::End,
        (CtKeyCode::PageUp, _) => Key::PageUp,
        (CtKeyCode::PageDown, _) => Key::PageDown,
        (CtKeyCode::Tab, _) => Key::Tab,
        (CtKeyCode::BackTab, _) => Key::BackTab,
        (CtKeyCode::Backspace, _) => Key::Backspace,
        (CtKeyCode::Delete, _) => Key::Delete,
        _ => Key::Other,
    };

    Event::Key(code)
}

/// Translates a crossterm key event, exposed for tests that build events by hand.
pub fn translate_key_event(key: CtKeyEvent) -> Event {
    translate_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_characters_and_control_chords() {
        assert_eq!(
            translate_key(CtKeyEvent::new(CtKeyCode::Char('7'), KeyModifiers::NONE)),
            Event::Key(Key::Char('7'))
        );
        assert_eq!(
            translate_key(CtKeyEvent::new(CtKeyCode::Char('C'), KeyModifiers::CONTROL)),
            Event::Key(Key::CtrlC)
        );
        assert_eq!(
            translate_key(CtKeyEvent::new(
                CtKeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Event::Key(Key::CtrlC)
        );
        // Alt+q is not Ctrl+C and must stay a plain character.
        assert_eq!(
            translate_key(CtKeyEvent::new(CtKeyCode::Char('c'), KeyModifiers::ALT)),
            Event::Key(Key::Char('c'))
        );
    }

    #[test]
    fn drops_key_releases_and_keeps_other_keys() {
        let release = CtKeyEvent {
            code: CtKeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(translate_key(release), Event::Key(Key::Other));
        assert_eq!(
            translate_key(CtKeyEvent::new(CtKeyCode::F(5), KeyModifiers::NONE)),
            Event::Key(Key::Other)
        );
    }

    #[test]
    fn translates_resize_and_ignores_non_key_events() {
        assert_eq!(
            translate(CtEvent::Resize(80, 24)),
            Event::Resize {
                width: 80,
                height: 24
            }
        );
        assert_eq!(translate(CtEvent::FocusLost), Event::Key(Key::Other));
    }

    #[test]
    fn as_letter_is_case_insensitive_and_only_for_chars() {
        assert_eq!(Key::Char('Q').as_letter(), Some('q'));
        assert_eq!(Key::Char('7').as_letter(), Some('7'));
        assert_eq!(Key::Enter.as_letter(), None);
    }

    #[test]
    fn tick_duration_uses_the_configured_interval() {
        assert_eq!(tick_duration(DEFAULT_TICK_MS), Duration::from_millis(75));
    }
}
