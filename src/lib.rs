//! Oefenen: a terminal learning app for Dutch primary school children.
//!
//! The crate is split so that everything which does not need a terminal (the
//! state machine, the storage layer and the module logic) can be tested without
//! one:
//!
//! * [`app`] is the top-level state machine; it knows nothing about `ratatui`.
//! * [`event`] turns crossterm events into the application's own [`event::Event`].
//! * [`storage`] reads and writes profiles, TOML and JSONL files.
//! * [`modules`] contains the [`modules::Module`] contract, its context and the
//!   registry, plus the first (dummy) module.
//! * [`ui`] draws the screens and the shared widgets.
//! * [`i18n`] holds every Dutch string.
//! * [`profile`] is the learner model.

pub mod app;
pub mod event;
pub mod i18n;
pub mod modules;
pub mod profile;
pub mod storage;
pub mod ui;
