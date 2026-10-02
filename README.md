# Oefenen

A Dutch terminal application for primary-school practice (basisschool). Pick a
learner, pick an exercise, practise with a timer, and keep the results per
learner.

The shell around the app (profiles, storage, module list) is finished and tested,
and "Tafels oefenen" is the first real module: pick your tables, pick a clock, and
practise multiplication tables one big-font question at a time.

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

Inside "Tafels oefenen":

| Screen | Keys |
| --- | --- |
| Tables | `1`-`9` `0` switch that table (on a number row, `0` is table 10), `↑` `↓` `Home` `End` select, `Space` / `Enter` tick the row under the cursor, `a` all on, `u` all off, `Esc` back |
| Time | `↑` `↓` `Home` `End` select, digits type a custom time, `Backspace` deletes, `Space` / `Enter` start, `Esc` back |
| Round | digits type the answer, `Backspace` deletes, `Enter` submits, `Esc` asks whether to stop (`←` `→` choose, `Enter` confirms, `Esc` carries on), `Ctrl-C` stops at once |
| Results | `↑` `↓` select, `Space` / `Enter` confirms, `Esc` back to the module list |

A custom time of one or two digits is seconds; three or four digits are minutes
and seconds (`0130` is 1 minute 30 seconds).

## Screens

1. **Profile list** - the learners on this machine, or an invitation to create
   the first one. From here: new, rename, delete (with confirmation), open.
2. **Profile form** - the same form for creating and renaming: a name and an
   optional group. The group stepper wraps around both ends, so `geen groep` is
   always one step away.
3. **Module list** - the exercises, grouped by subject, with a one-line
   description each.
4. **Module** - the exercise itself, with a running timer and a hint bar. For
   "Tafels oefenen" that is: choose tables, choose a time, answer the round, and
   read the score, the mistakes worth practising again, and the total time.

A window smaller than 40x14 shows a short message instead of a broken layout. The
question is drawn in a block font at the largest size that fits the longest
possible question, so the digits never jump size mid-round; a window too narrow
for that falls back to plain text.

## Where things live on disk

```
<data-dir>/
└── profiles/
    └── <profile-id>/            e.g. sem-3f9a2c71
        ├── profile.toml         the learner
        └── modules/
            └── <module-id>/
                ├── settings.toml       the module's own settings
                ├── answers.jsonl       one line per answered question
                └── sessions.jsonl      one line per finished session
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

A module keeps its own files, and their names are up to the module. "Tafels
oefenen" (id `tafels`) writes three:

`settings.toml`, so the choices survive between visits:

```toml
version = 1
tables = [3, 7]
reverse = true
timer_mode = "per_question"    # or "total", or "none"
seconds = 15
```

`reverse` adds the flipped forms (`8 × 7` next to `7 × 8`). A file that is damaged,
from another version, or full of impossible values is ignored rather than
crashing the module; the learner then starts from the defaults.

`answers.jsonl`, one line the moment an answer is resolved:

```json
{"schema":1,"session_id":"...","timestamp":"...","question_index":0,"a":7,"b":8,"expected":56,"given":56,"outcome":"Correct","response_ms":760}
```

`outcome` is `Correct`, `Incorrect`, `TimedOut` or `NotReached`. `given` and
`response_ms` are `null` for a question that was never answered.

`sessions.jsonl`, one line when the round ends, whether it was finished, timed
out or stopped:

```json
{"schema":1,"session_id":"...","started_at":"...","ended_at":"...","status":"Abandoned","settings":{"tables":[7],"reverse":false,"timer_mode":"per_question","seconds":5},"question_count":10,"correct_count":7,"total_ms":41230}
```

`status` is `Finished`, `TimedOut` or `Abandoned`. `total_ms` counts real play
only: the pause on the stop question does not count against the learner.

The profile id is not repeated in the line: it is already the directory the line
lives in.

Settings files are written without appending: to a temporary file in the same
directory, flushed with `sync_all`, and then renamed over the old one, so an
interrupted write cannot leave half a file behind. Records are appended as JSON
lines, which keeps writing cheap and reading trivial. The records carry the
schema version they were written with, so an old file is recognised rather than
guessed at.

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

1. Create `src/modules/<name>.rs` with a type that implements `Module`:

   ```rust
   impl Module for MyModule {
       fn id(&self) -> &'static str { "mijn-module" }        // stable, becomes a directory
       fn title(&self) -> String { i18n::my_module::TITLE.to_string() }
       fn description(&self) -> String { i18n::my_module::DESCRIPTION.to_string() }
       fn subject(&self) -> Subject { Subject::Math }
       fn on_enter(&mut self, ctx: &mut ModuleContext) { /* load settings */ }
       fn handle_event(&mut self, event: &Event, ctx: &mut ModuleContext) -> Transition { ... }
       fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &ModuleContext) { ... }
       fn hints(&self) -> Vec<KeyHint> { vec![KeyHint::back()] }
       fn on_exit(&mut self, ctx: &mut ModuleContext) { /* flush and clean up */ }
   }
   ```

2. Add its Dutch strings to `src/i18n.rs`.
3. Register it in `registry()` in `src/modules/mod.rs`. The registry is
   exhaustive, so a module that is not registered cannot compile unnoticed.
4. Let the module render itself; the application only provides the profile, a
   `ModuleStore` for results, and a timer that ticks every `--tick-ms`.

Notes for module authors:

- Store settings and records through `ctx.store()`; it is already scoped to this
  profile and this module, and it refuses anything that is not a plain path
  segment.
- Read the elapsed time from `ctx.elapsed_millis()`: the application drives the
  clock, so a module never measures time itself. Subtract the time a learner spent
  on a dialog instead of letting it count against the clock.
- Return `Transition::Exit` to leave, or `Transition::Stay` to carry on.
- Report a failed write with `ctx.report_error(...)`; the trait has no error
  return value, and silently dropping it would lose a learner's work.
- Module ids end up as directory names: lowercase letters, digits, `-` and `_`.

## Tests

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The suite covers the domain rules, the storage format, path safety, the state
machine (including the full delete flow and the `q`/`Ctrl-C` behaviour), every
tafels screen drawn into a test buffer, and a few end-to-end runs through the
public API. None of it needs a real terminal.

## What is next

"Tafels oefenen" practises multiplication tables. The obvious next steps are the
other subjects the module list already groups for (reading, geography), and
using the mistakes on the results screen to weight the next round.
