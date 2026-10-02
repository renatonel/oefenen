# Oefenen

A Dutch terminal application for primary-school practice (basisschool). Pick a
learner, pick an exercise, practise with a timer, and keep the results per
learner.

This repository is a scaffold: the shell around the app is finished and tested,
and the "Tafels oefenen" drill is a deliberately small dummy module that marks
the place where real modules go.

## Requirements

- Rust 1.85 or newer (edition 2024). `rust-toolchain.toml` selects the channel,
  so `rustup` installs it for you.
- A terminal with at least 40x14 cells.

## Running

```sh
cargo run --release
```

Command-line options:

| Option | Meaning | Default |
| --- | --- | --- |
| `--data-dir <PATH>` | Where profiles and results live | the platform data directory |
| `--tick-ms <MS>` | Milliseconds between timer ticks | 100 |

```sh
cargo run -- --data-dir ./data --tick-ms 200
```

The application only writes inside its data directory, so `--data-dir` is a
complete reset button: point it somewhere else and you start with an empty
install.

## Key bindings

| Screen | Keys |
| --- | --- |
| Profile list | `↑` `↓` select, `Enter` open, `n` new, `w` rename, `d` delete, `q` / `Ctrl-C` quit |
| Profile form | type a name, `Tab` switch field, `←` `→` pick a group, `Backspace` delete a character, `Enter` save, `Esc` cancel, `Ctrl-C` quit |
| Delete confirmation | `←` `→` choose yes or no, `Enter` confirm, `Esc` cancel, `q` / `Ctrl-C` quit |
| Module list | `↑` `↓` select, `Enter` start, `Esc` back to the profiles, `q` / `Ctrl-C` quit |
| Module | depends on the module, see its hint bar |

`q` quits everywhere except while typing a name, where `Ctrl-C` is the way out.
Every screen shows the keys that work on it in the bar at the bottom; that bar
wraps onto a second line when the terminal is narrow.

## Screens

1. **Profile list** - the learners on this machine, or an invitation to create
   the first one. From here: new, rename, delete (with confirmation), open.
2. **Profile form** - the same form for creating and renaming: a name and an
   optional group. The group stepper wraps around both ends, so `geen groep` is
   always one step away.
3. **Module list** - the exercises, grouped by subject, with a one-line
   description each.
4. **Module** - the exercise itself, with a running timer and a hint bar.

A window smaller than 40x14 shows a short message instead of a broken layout.

## Where things live on disk

```
<data-dir>/
└── profiles/
    └── <profile-id>/            e.g. sem-3f9a2c71
        ├── profile.toml         the learner
        └── modules/
            └── <module-id>/
                └── results.jsonl   one session per line
```

`profile.toml`:

```toml
version = 1
id = "sem-3f9a2c71"
name = "Sem"
grade = 4                       # optional: absent means "geen groep"
created_at = "2026-10-02T12:00:00Z"
```

A profile id is a readable slug of the name plus eight random hex characters,
drawn once at creation time. Renaming keeps the id, so recorded results stay
with the learner. Every profile file starts with a `version`, so a future
format change can be detected instead of guessed at.

`results.jsonl` holds one JSON object per finished session:

```json
{"schema_version":1,"module_id":"tafel-oefenen","session_id":"...","started_at":"...","duration_ms":1120,"facts":[{"factor_a":7,"factor_b":8,"correct":true,"response_time_ms":760,"answered_at":"..."}]}
```

The profile id is not repeated in the line: it is already the directory the line
lives in.

Both files are written without appending: profiles are written to a temporary
file in the same directory, flushed with `sync_all`, and then renamed over the
old one, so an interrupted write cannot leave half a profile behind. Results are
appended as JSON lines, one session per line, which keeps writing cheap and
reading trivial.

Ids are validated as single path segments, so nothing outside the data
directory can be reached through a crafted profile or module id.

## How it is put together

| Path | Job |
| --- | --- |
| `src/main.rs` | CLI, terminal setup, panic hook, event loop |
| `src/app.rs` | the state machine: screens, keys, ticks, toasts - no terminal access |
| `src/event.rs` | one `Event`/`Key` type for the app, translated from crossterm in one place |
| `src/profile.rs` | `Profile`, `ProfileId`, `Grade`, validation |
| `src/storage/` | data directory, profile TOML, JSONL, atomic writes, path safety |
| `src/modules/` | the `Module` trait, the registry, and the modules |
| `src/ui/` | ratatui rendering: screens, theme, hint bar |
| `src/i18n.rs` | every Dutch string in the app, in one place |

Two rules keep the tests fast and the design honest:

- **The app never touches the terminal.** `App` takes `Event`s and returns
  state changes; `ui::render` turns that state into widgets. So the whole
  interface, including deleting a profile and recording a result, is tested
  without a terminal.
- **Storage is an interface, not a global.** `App` is built with a data
  directory, so tests get a temporary one.

Screens are a small enum (`ProfileSelect`, `ProfileForm`, `DeleteConfirm`,
`ModuleList`, `ModuleRunning`) rather than a stack, because the flow only ever
moves forward or back along one known path. `ModuleRunning` hosts a `Box<dyn
Module>`, which is what keeps new modules from touching the app.

## How to add a new module

1. Create `src/modules/<name>.rs` with a unit struct and implement `Module`:

   ```rust
   impl Module for MyModule {
       fn id(&self) -> &'static str { "mijn-module" }        // stable, becomes a directory
       fn title(&self) -> String { i18n::my_module::TITLE.to_string() }
       fn description(&self) -> String { i18n::my_module::DESCRIPTION.to_string() }
       fn subject(&self) -> Subject { Subject::Math }
       fn hints(&self) -> Vec<KeyHint> { vec![KeyHint::back()] }
       fn on_enter(&mut self, ctx: &mut ModuleContext) { /* reset session */ }
       fn on_key(&mut self, key: Key, ctx: &mut ModuleContext) -> Vec<Transition> { ... }
       fn render(&self, frame: &mut Frame, area: Rect, ctx: &ModuleContext) { ... }
   }
   ```

2. Add its Dutch strings to `src/i18n.rs`.
3. Register it in `registry()` in `src/modules/mod.rs`. The registry is
   exhaustive, so a module that is not registered cannot compile unnoticed.
4. Let the module render itself; the application only provides the profile, a
   `ModuleStore` for results, and a timer that ticks every `--tick-ms`.

Notes for module authors:

- Store results with `ctx.records.store_session(&record)`; use `ctx.records`
  instead of writing files yourself.
- Read the elapsed time from `ctx.elapsed_millis()`: the application drives the
  clock, so a module never measures time itself.
- Return `Transition::Exit` when the learner is done, or nothing to stay.
- Module ids end up as directory names: lowercase letters, digits, `-` and `_`.

## Tests

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The suite covers the domain rules, the storage format, path safety, the state
machine (including the full delete flow and the `q`/`Ctrl-C` behaviour), and a
few end-to-end runs through the public API. None of it needs a real terminal.

## What is still dummy

The multiplication module asks one question (`7 x 8`), shows feedback, and
records the answer. Its id is `tafel-oefenen`, which is the directory its
results end up in. The table selection, the question generation and the
adaptive weighting are the next steps; the result format is already shaped for
them.
