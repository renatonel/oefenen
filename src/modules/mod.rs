//! The module contract and the compile-time registry.
//!
//! A module is a self-contained exercise: it owns its screen, its logic and the
//! schema of its own data. The application only routes events to it and draws
//! whatever it renders.
//!
//! Adding a module:
//!
//! 1. create `src/modules/<name>.rs` with a type that implements [`Module`],
//! 2. add `mod <name>;` below,
//! 3. add one line to [`registry`].
//!
//! There is no dynamic loading: the registry is the single source of truth for
//! which modules exist, and a missing line is a compile error the moment
//! someone tries to use the module.

pub mod multiplication;

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::event::Event;
use crate::profile::Profile;
use crate::storage::ModuleStore;
use crate::ui::hints::KeyHint;

use self::multiplication::MultiplicationModule;

/// Subject a module belongs to; used to group the module list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Subject {
    /// Rekenen: multiplication tables, addition, ...
    Math,
    /// Lezen: reading comprehension.
    Reading,
    /// Topografie: Netherlands and world geography.
    Geography,
}

/// What should happen after a module handled an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Stay in the module; keep routing events to it.
    Stay,
    /// Leave the module and return to the module list.
    Exit,
}

/// Everything a module is allowed to touch while it is running.
///
/// The context is rebuilt after every event by the application, so a module can
/// simply read the fresh [`Self::elapsed`] and [`Self::now`] instead of keeping
/// its own clock. It owns a copy of the profile so the trait stays free of
/// lifetime parameters.
pub struct ModuleContext {
    profile: Profile,
    store: ModuleStore,
    started_at: time::OffsetDateTime,
    elapsed: Duration,
    error: Option<String>,
}

impl ModuleContext {
    /// Builds a context for `profile`, with storage scoped to this module.
    pub fn new(profile: Profile, store: ModuleStore) -> Self {
        Self {
            profile,
            store,
            started_at: time::OffsetDateTime::now_utc(),
            elapsed: Duration::ZERO,
            error: None,
        }
    }

    /// The active learner.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Storage scoped to this profile *and* this module.
    pub fn store(&self) -> &ModuleStore {
        &self.store
    }

    /// Wall-clock time at which the module was entered.
    pub fn started_at(&self) -> time::OffsetDateTime {
        self.started_at
    }

    /// Time since [`Self::started_at`], advanced by the application's tick.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Overrides the start time; used by tests to get reproducible timestamps.
    pub fn set_started_at(&mut self, started_at: time::OffsetDateTime) {
        self.started_at = started_at;
    }

    /// Advances the internal clock. Called by the application on every tick.
    pub fn advance(&mut self, tick: Duration) {
        self.elapsed = self.elapsed.saturating_add(tick);
    }

    /// Elapsed time in whole milliseconds, the unit used in stored records.
    pub fn elapsed_millis(&self) -> u64 {
        u64::try_from(self.elapsed.as_millis()).unwrap_or(u64::MAX)
    }

    /// Reports a problem to the application, which shows it to the learner.
    ///
    /// The module trait has no error return value, so failures to write to
    /// storage travel this way instead of being dropped.
    pub fn report_error(&mut self, message: String) {
        self.error = Some(message);
    }

    /// Takes the reported error, if the module set one.
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

/// One exercise.
///
/// The five methods below are the whole contract; a module implements them and
/// nothing else of the application.
pub trait Module {
    /// Stable, filesystem-safe id; also the name of the module's directory.
    fn id(&self) -> &'static str;

    /// Title shown in the module list.
    fn title(&self) -> String;

    /// One line of explanation shown in the module list.
    fn description(&self) -> String;

    /// Subject used to group modules in the list.
    fn subject(&self) -> Subject;

    /// Called once when the module becomes active.
    fn on_enter(&mut self, ctx: &mut ModuleContext);

    /// Called for every input and tick event.
    fn handle_event(&mut self, event: &Event, ctx: &mut ModuleContext) -> Transition;

    /// Draws the module; only called for [`Transition`] output of a draw pass.
    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &ModuleContext);

    /// Keys the module accepts right now; drawn by the application underneath
    /// the module's screen.
    fn hints(&self) -> Vec<KeyHint> {
        vec![KeyHint::quit()]
    }

    /// Called once when the module is left; must leave a clean state.
    fn on_exit(&mut self, ctx: &mut ModuleContext);
}

/// Lets the application keep its registry as `Vec<Box<dyn Module>>` and still
/// call the trait directly on the elements.
impl Module for Box<dyn Module> {
    fn id(&self) -> &'static str {
        (**self).id()
    }

    fn title(&self) -> String {
        (**self).title()
    }

    fn description(&self) -> String {
        (**self).description()
    }

    fn subject(&self) -> Subject {
        (**self).subject()
    }

    fn on_enter(&mut self, ctx: &mut ModuleContext) {
        (**self).on_enter(ctx);
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut ModuleContext) -> Transition {
        (**self).handle_event(event, ctx)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &ModuleContext) {
        (**self).render(frame, area, ctx);
    }

    fn hints(&self) -> Vec<KeyHint> {
        (**self).hints()
    }

    fn on_exit(&mut self, ctx: &mut ModuleContext) {
        (**self).on_exit(ctx);
    }
}

/// All modules the application knows about.
///
/// Adding a module means adding exactly one line here.
pub fn registry() -> Vec<Box<dyn Module>> {
    vec![Box::new(MultiplicationModule::default())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::multiplication::MULTIPLICATION_ID;
    use crate::profile::{Grade, ProfileId};
    use crate::storage::ProfileStore;
    use tempfile::TempDir;
    use time::macros::datetime;

    #[test]
    fn the_registry_contains_the_multiplication_module() {
        let modules = registry();
        let ids: Vec<&str> = modules.iter().map(|module| module.id()).collect();
        assert_eq!(ids, vec![MULTIPLICATION_ID]);
    }

    #[test]
    fn registry_entries_are_ready_to_use() {
        for module in registry() {
            assert!(!module.id().is_empty());
            assert!(!module.title().is_empty());
            assert!(!module.description().is_empty());
        }
    }

    #[test]
    fn ids_are_valid_path_segments() {
        for module in registry() {
            crate::storage::paths::validate_segment("module", module.id())
                .unwrap_or_else(|error| panic!("ongeldige module-id: {error}"));
        }
    }

    #[test]
    fn the_clock_only_moves_forward_on_ticks() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::new(dir.path());
        let profile = store
            .create_at(
                "Sem",
                Some(Grade::new(4).unwrap()),
                datetime!(2026-10-02 12:00 UTC),
            )
            .unwrap();
        let id = profile.id().clone();
        let module_store = ModuleStore::new(dir.path(), profile.id(), MULTIPLICATION_ID).unwrap();
        let mut ctx = ModuleContext::new(profile, module_store);
        ctx.set_started_at(datetime!(2026-10-02 12:00 UTC));

        assert_eq!(ctx.elapsed(), Duration::ZERO);
        ctx.advance(Duration::from_millis(75));
        ctx.advance(Duration::from_millis(75));
        assert_eq!(ctx.elapsed_millis(), 150);
        assert!(ctx.store().dir().ends_with(MULTIPLICATION_ID));
        assert_eq!(ctx.profile().id(), &id);
        assert_eq!(ctx.profile().name(), "Sem");
        assert_eq!(ProfileId::parse(id.as_str()).unwrap(), id);
    }
}
