//! The application state machine.
//!
//! ```text
//! ProfileSelect ──n/w──▶ ProfileForm ──Enter/Esc──▶ ProfileSelect
//!       │                  ▲                          │
//!       └─d─▶ DeleteConfirm┘                          │ Enter
//!       │                                             ▼
//!       └──────────────────── Esc ◀─────────────── ModuleList
//!                                              │ Enter
//!                                              ▼
//!                                         ModuleRunning ── Transition::Exit ──┐
//!                                                                             │
//!                                            ModuleList ◀─────────────────────┘
//! ```
//!
//! This file deliberately contains no `ratatui` and no `crossterm`: it only
//! turns events into state. That keeps every transition testable without a TTY;
//! drawing lives in [`crate::ui`].

use std::path::PathBuf;
use std::time::Duration;

use crate::event::{Event, Key};
use crate::i18n;
use crate::modules::{Module, ModuleContext, Transition, registry};
use crate::profile::{Grade, Profile, ProfileId};
use crate::storage::{ModuleStore, ProfileStore, error::StorageError};

/// How long a message stays on screen, in ticks.
const TOAST_TICKS: u16 = 60;

/// Which screen is currently shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Pick a learner.
    ProfileSelect,
    /// Create a new learner or rename an existing one.
    ProfileForm,
    /// Confirm deleting a learner.
    DeleteConfirm,
    /// Pick a module.
    ModuleList,
    /// A module is running and owns the screen.
    ModuleRunning,
}

impl Screen {
    /// Whether this screen is handled by a module instead of the application.
    pub fn is_module(self) -> bool {
        matches!(self, Screen::ModuleRunning)
    }
}

/// The field the cursor is on in the profile form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Grade,
}

impl Field {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Grade,
            Self::Grade => Self::Name,
        }
    }
}

/// Whether the form creates a profile or renames one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormMode {
    Create,
    Edit(ProfileId),
}

/// State of the create/rename form.
#[derive(Debug, Clone)]
pub struct ProfileForm {
    mode: FormMode,
    field: Field,
    name: String,
    grade: Option<Grade>,
    error: Option<String>,
}

impl ProfileForm {
    fn create() -> Self {
        Self {
            mode: FormMode::Create,
            field: Field::Name,
            name: String::new(),
            grade: Some(Grade::default()),
            error: None,
        }
    }

    fn edit(profile: &Profile) -> Self {
        Self {
            mode: FormMode::Edit(profile.id().clone()),
            field: Field::Name,
            name: profile.name().to_string(),
            grade: profile.grade(),
            error: None,
        }
    }

    pub fn mode(&self) -> &FormMode {
        &self.mode
    }

    pub fn field(&self) -> Field {
        self.field
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The chosen group, or `None` for "geen groep".
    pub fn grade(&self) -> Option<Grade> {
        self.grade
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn type_character(&mut self, character: char) {
        if self.field != Field::Name {
            return;
        }
        if self.name.chars().count() < crate::profile::MAX_NAME_LEN {
            self.name.push(character);
        }
        self.error = None;
    }

    fn backspace(&mut self) {
        if self.field != Field::Name {
            return;
        }
        self.name.pop();
        self.error = None;
    }

    /// Steps the group one step up or down, cycling through the whole range
    /// with "geen groep" at both ends:
    /// `geen groep - 3 - 4 - 5 - 6 - 7 - 8 - geen groep`.
    fn step_grade(&mut self, delta: isize) {
        if self.field != Field::Grade {
            return;
        }
        self.grade = match (self.grade, delta) {
            (None, 1) => Some(Grade::default()),
            (None, _) => Some(Grade::last()),
            (Some(grade), 1) => grade.next(),
            (Some(grade), _) => grade.previous(),
        };
    }
}

/// The two answers of the delete confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Yes,
    No,
}

impl ConfirmChoice {
    fn other(self) -> Self {
        match self {
            Self::Yes => Self::No,
            Self::No => Self::Yes,
        }
    }
}

/// The delete confirmation, for the learner at `index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteConfirm {
    index: usize,
    choice: ConfirmChoice,
}

impl DeleteConfirm {
    pub fn profile_index(self) -> usize {
        self.index
    }

    pub fn choice(self) -> ConfirmChoice {
        self.choice
    }
}

/// A short-lived message shown above the key-hint bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub message: String,
    pub is_error: bool,
    remaining_ticks: u16,
}

impl Toast {
    fn info(message: String) -> Self {
        Self {
            message,
            is_error: false,
            remaining_ticks: TOAST_TICKS,
        }
    }

    fn error(message: String) -> Self {
        Self {
            message,
            is_error: true,
            remaining_ticks: TOAST_TICKS,
        }
    }

    fn tick(&mut self) {
        self.remaining_ticks = self.remaining_ticks.saturating_sub(1);
    }

    fn is_visible(&self) -> bool {
        self.remaining_ticks > 0
    }
}

/// The whole application state.
pub struct App {
    data_dir: PathBuf,
    profiles: ProfileStore,
    /// Cached profile list, refreshed after every change.
    loaded_profiles: Vec<Profile>,
    modules: Vec<Box<dyn Module>>,
    screen: Screen,
    /// Cursor on the profile or module list, depending on the screen.
    cursor: usize,
    form: ProfileForm,
    confirm: Option<DeleteConfirm>,
    active_profile: Option<Profile>,
    running_index: Option<usize>,
    context: Option<ModuleContext>,
    tick: Duration,
    toast: Option<Toast>,
    quit_requested: bool,
}

impl App {
    /// Loads the profiles from `data_dir` and builds the module registry.
    ///
    /// With no profiles yet the learner starts straight in the create form.
    pub fn new(data_dir: impl Into<PathBuf>, tick: Duration) -> Result<Self, StorageError> {
        let data_dir = data_dir.into();
        let profiles = ProfileStore::new(&data_dir);
        let loaded = profiles.list()?;
        let screen = if loaded.is_empty() {
            Screen::ProfileForm
        } else {
            Screen::ProfileSelect
        };

        Ok(Self {
            data_dir,
            profiles,
            loaded_profiles: loaded,
            modules: registry(),
            screen,
            cursor: 0,
            form: ProfileForm::create(),
            confirm: None,
            active_profile: None,
            running_index: None,
            context: None,
            tick,
            toast: None,
            quit_requested: false,
        })
    }

    // --- accessors for the interface --------------------------------------

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.loaded_profiles
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn form(&self) -> &ProfileForm {
        &self.form
    }

    pub fn confirm(&self) -> Option<DeleteConfirm> {
        self.confirm
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        self.active_profile.as_ref()
    }

    pub fn modules(&self) -> &[Box<dyn Module>] {
        &self.modules
    }

    /// Mutable access to the registry, for the `&mut self` of
    /// [`Module::render`].
    pub fn modules_mut(&mut self) -> &mut [Box<dyn Module>] {
        &mut self.modules
    }

    /// Takes the module context out of the application.
    ///
    /// Drawing a module needs the registry mutably and the context immutably at
    /// the same time; moving the context out and back is the simplest way to
    /// express that without borrowing all of `App`.
    pub fn take_context(&mut self) -> Option<ModuleContext> {
        self.context.take()
    }

    /// Puts back a context obtained with [`Self::take_context`].
    pub fn put_back_context(&mut self, context: ModuleContext) {
        self.context = Some(context);
    }

    /// Index of the running module, if any.
    pub fn running_index(&self) -> Option<usize> {
        self.running_index
    }

    pub fn context(&self) -> Option<&ModuleContext> {
        self.context.as_ref()
    }

    pub fn toast(&self) -> Option<&Toast> {
        self.toast.as_ref().filter(|toast| toast.is_visible())
    }

    pub fn tick(&self) -> Duration {
        self.tick
    }

    /// Whether the application has been asked to quit.
    pub fn should_quit(&self) -> bool {
        self.quit_requested
    }

    // --- input ------------------------------------------------------------

    /// Handles one event.
    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Tick => self.on_tick(),
            Event::Resize { .. } => {}
            Event::Key(key) => self.on_key(key),
        }
    }

    fn on_tick(&mut self) {
        if let Some(toast) = self.toast.as_mut() {
            toast.tick();
            if !toast.is_visible() {
                self.toast = None;
            }
        }

        if self.screen != Screen::ModuleRunning {
            return;
        }
        if let Some(context) = self.context.as_mut() {
            context.advance(self.tick);
        }
        self.forward_to_module(Event::Tick);
    }

    fn on_key(&mut self, key: &Key) {
        // While a module is running it owns the keyboard, so it can react to
        // Esc and Ctrl+C and still save the session.
        if self.screen == Screen::ModuleRunning {
            self.forward_to_module(Event::Key(key.clone()));
            return;
        }

        match key {
            Key::CtrlC => self.request_quit(),
            // `q` quits from the list screens, but is a normal letter in the
            // profile name field.
            Key::Char('q') if matches!(self.screen, Screen::ProfileSelect | Screen::ModuleList) => {
                self.request_quit();
            }
            _ => match self.screen {
                Screen::ProfileSelect => self.on_key_profile_select(key),
                Screen::ProfileForm => self.on_key_profile_form(key),
                Screen::DeleteConfirm => self.on_key_delete_confirm(key),
                Screen::ModuleList => self.on_key_module_list(key),
                Screen::ModuleRunning => {}
            },
        }
    }

    /// Sends an event to the running module and handles its transition.
    fn forward_to_module(&mut self, event: Event) {
        let Some(index) = self.running_index else {
            return;
        };
        let mut context = match self.context.take() {
            Some(context) => context,
            None => return,
        };
        let transition = self.modules[index].handle_event(&event, &mut context);
        if let Some(message) = context.take_error() {
            self.set_error(message);
        }
        self.context = Some(context);
        if transition == Transition::Exit {
            self.close_module();
        }
    }

    fn on_key_profile_select(&mut self, key: &Key) {
        let profiles = self.loaded_profiles.clone();
        match key {
            Key::Up => self.move_cursor(-1, profiles.len()),
            Key::Down => self.move_cursor(1, profiles.len()),
            Key::Enter => {
                if profiles.is_empty() {
                    self.open_create_form();
                } else {
                    self.select_profile();
                }
            }
            Key::Char('n') => self.open_create_form(),
            Key::Char('w') => {
                if let Some(profile) = profiles.get(self.cursor) {
                    self.form = ProfileForm::edit(profile);
                    self.screen = Screen::ProfileForm;
                }
            }
            Key::Char('d') if profiles.get(self.cursor).is_some() => {
                self.confirm = Some(DeleteConfirm {
                    index: self.cursor,
                    choice: ConfirmChoice::No,
                });
                self.screen = Screen::DeleteConfirm;
            }
            _ => {}
        }
    }

    fn on_key_profile_form(&mut self, key: &Key) {
        match key {
            Key::Tab | Key::Down | Key::Up => self.form.field = self.form.field.next(),
            Key::Left => self.form.step_grade(-1),
            Key::Right => self.form.step_grade(1),
            Key::Backspace => self.form.backspace(),
            Key::Char(character) if !character.is_control() => self.form.type_character(*character),
            Key::Esc => {
                self.screen = Screen::ProfileSelect;
            }
            Key::Enter => self.save_form(),
            _ => {}
        }
    }

    fn on_key_delete_confirm(&mut self, key: &Key) {
        let Some(confirm) = self.confirm else {
            self.screen = Screen::ProfileSelect;
            return;
        };
        match key {
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Tab => {
                let choice = confirm.choice.other();
                self.confirm = Some(DeleteConfirm { choice, ..confirm });
            }
            Key::Esc => {
                self.confirm = None;
                self.screen = Screen::ProfileSelect;
            }
            Key::Enter => {
                self.confirm = None;
                self.screen = Screen::ProfileSelect;
                if confirm.choice == ConfirmChoice::Yes {
                    self.delete_profile(confirm.index);
                }
            }
            _ => {}
        }
    }

    fn on_key_module_list(&mut self, key: &Key) {
        match key {
            Key::Up => self.move_cursor(-1, self.modules.len()),
            Key::Down => self.move_cursor(1, self.modules.len()),
            Key::Enter => {
                if self.cursor < self.modules.len() {
                    self.open_module(self.cursor);
                }
            }
            Key::Esc => {
                // Back to profile selection: switch learner or quit.
                self.active_profile = None;
                self.screen = Screen::ProfileSelect;
            }
            _ => {}
        }
    }

    // --- actions ----------------------------------------------------------

    fn request_quit(&mut self) {
        self.quit_requested = true;
    }

    fn open_create_form(&mut self) {
        self.form = ProfileForm::create();
        self.screen = Screen::ProfileForm;
    }

    fn save_form(&mut self) {
        let name = self.form.name().trim().to_string();
        let grade = self.form.grade();
        let result = match self.form.mode().clone() {
            FormMode::Create => self
                .profiles
                .create(&name, grade)
                .map(|profile| (i18n::created(profile.name()), profile.id().clone())),
            FormMode::Edit(id) => self.rename_profile(&id, &name, grade),
        };

        match result {
            Ok((message, id)) => {
                self.screen = Screen::ProfileSelect;
                self.reload_profiles();
                // Keep the cursor on the profile we just saved, also when the
                // list order changed because of the new name.
                self.cursor = self.position_of(&id);
                self.toast = Some(Toast::info(message));
            }
            Err(error) => {
                // Keep the form open with the typed name, and explain why.
                self.form.error = Some(validation_message(&error));
            }
        }
    }

    fn rename_profile(
        &self,
        id: &ProfileId,
        name: &str,
        grade: Option<Grade>,
    ) -> Result<(String, ProfileId), StorageError> {
        let mut profile = self.profiles.load(id)?;
        profile.rename(name)?;
        profile.set_grade(grade);
        self.profiles.save(&profile)?;
        Ok((i18n::renamed(profile.name()), profile.id().clone()))
    }

    fn delete_profile(&mut self, index: usize) {
        let profiles = self.loaded_profiles.clone();
        let Some(profile) = profiles.get(index).cloned() else {
            return;
        };
        match self.profiles.delete(profile.id()) {
            Ok(()) => {
                self.reload_profiles();
                self.clamp_cursor();
                self.toast = Some(Toast::info(i18n::deleted(profile.name())));
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn select_profile(&mut self) {
        let profiles = self.loaded_profiles.clone();
        let Some(profile) = profiles.get(self.cursor).cloned() else {
            return;
        };
        self.active_profile = Some(profile);
        self.cursor = 0;
        self.screen = Screen::ModuleList;
    }

    fn open_module(&mut self, index: usize) {
        let Some(profile) = self.active_profile.clone() else {
            return;
        };
        let module_id = self.modules[index].id();
        let store = match ModuleStore::new(&self.data_dir, profile.id(), module_id) {
            Ok(store) => store,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let mut context = ModuleContext::new(profile, store);
        self.modules[index].on_enter(&mut context);
        self.context = Some(context);
        self.running_index = Some(index);
        self.screen = Screen::ModuleRunning;
    }

    fn close_module(&mut self) {
        let (Some(index), Some(mut context)) = (self.running_index, self.context.take()) else {
            return;
        };
        self.modules[index].on_exit(&mut context);
        if let Some(message) = context.take_error() {
            self.set_error(message);
        }
        self.running_index = None;
        self.cursor = index;
        self.screen = Screen::ModuleList;
    }

    // --- helpers ----------------------------------------------------------

    /// Position of `id` in the (sorted) profile list.
    fn position_of(&self, id: &ProfileId) -> usize {
        self.loaded_profiles
            .iter()
            .position(|profile| profile.id() == id)
            .unwrap_or(0)
    }

    /// Re-reads the profiles from disk after a create, rename or delete.
    fn reload_profiles(&mut self) {
        match self.profiles.list() {
            Ok(profiles) => self.loaded_profiles = profiles,
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn clamp_cursor(&mut self) {
        let max = match self.screen {
            Screen::ModuleList => self.modules.len(),
            _ => self.loaded_profiles.len(),
        };
        if max == 0 {
            self.cursor = 0;
        } else if self.cursor >= max {
            self.cursor = max - 1;
        }
    }

    fn move_cursor(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.cursor = 0;
            return;
        }
        let len = isize::try_from(len).unwrap_or(isize::MAX);
        let position = isize::try_from(self.cursor).unwrap_or(0);
        self.cursor = usize::try_from((position + delta).rem_euclid(len)).unwrap_or(0);
    }

    fn set_error(&mut self, message: String) {
        self.toast = Some(Toast::error(message));
    }
}

/// Turns a storage error into the message shown next to the form.
fn validation_message(error: &StorageError) -> String {
    match error {
        StorageError::InvalidProfile(validation) => validation.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests;
