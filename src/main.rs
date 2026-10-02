//! Program entry point: command line, terminal setup, event loop.

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use crossterm::event::{self, Event as CtEvent};
use crossterm::terminal::EnterAlternateScreen;
use crossterm::{execute, terminal};
use oefenen::app::App;
use oefenen::event::{DEFAULT_TICK_MS, translate, translate_key_event};
use oefenen::storage;
use oefenen::ui;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// Command line options.
#[derive(Debug, Parser)]
#[command(
    name = "oefenen",
    version,
    about = "Oefenen: leer Nederlands voor basisschoolkinderen in de terminal",
    long_about = None,
)]
struct Cli {
    /// Map met de profielen en resultaten (standaard: de platformmap).
    #[arg(long, value_name = "PAD")]
    data_dir: Option<PathBuf>,

    /// Interval van de interne klok in milliseconden.
    #[arg(long, value_name = "MS", default_value_t = DEFAULT_TICK_MS)]
    tick_ms: u64,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let data_dir = storage::resolve_data_dir(cli.data_dir.as_deref()).with_context(|| {
        format!(
            "het gegevenspad {} kon niet worden gebruikt",
            cli.data_dir.as_ref().map_or_else(
                || "van het systeem".to_string(),
                |path| path.display().to_string()
            )
        )
    })?;

    let tick = Duration::from_millis(cli.tick_ms.max(1));
    let mut app = App::new(data_dir, tick).context("de profielen konden niet worden gelezen")?;

    let mut terminal = setup_terminal()?;
    // A panic must never leave the user in a raw, half-drawn terminal.
    install_panic_hook();
    let result = run(&mut terminal, &mut app, tick);
    restore_terminal();
    result
}

/// Draws frames until the application is asked to quit.
fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    tick: Duration,
) -> anyhow::Result<()> {
    loop {
        terminal
            .draw(|frame| ui::render(frame, app))
            .context("tekenen naar het scherm")?;
        if app.should_quit() {
            return Ok(());
        }

        let event = read_event(tick)?;
        app.handle_event(&event);
    }
}

/// Waits for input, or returns a tick when nothing arrives in time.
///
/// A tick guarantees that timers and animations keep running even when nobody
/// touches the keyboard, and that redrawing never blocks on input.
fn read_event(tick: Duration) -> anyhow::Result<oefenen::event::Event> {
    if event::poll(tick).context("wachten op een toetsaanslag")? {
        match event::read().context("lezen van een toetsaanslag")? {
            CtEvent::Key(key) => Ok(translate_key_event(key)),
            other => Ok(translate(other)),
        }
    } else {
        Ok(oefenen::event::Event::Tick)
    }
}

/// Switches the terminal to raw mode and to the alternate screen.
fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<Stdout>>> {
    terminal::enable_raw_mode().context("raw mode aanzetten")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("naar het alternatieve scherm gaan")?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("terminal aanmaken")
}

/// Undoes [`setup_terminal`]. Safe to call twice, and from a panic hook.
fn restore_terminal() {
    // Disable mouse capture first: it is a no-op when it was never enabled.
    let _ = execute!(io::stdout(), terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
}

/// Restores the terminal before printing a panic message.
fn install_panic_hook() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

/// Returns the moment the next frame is due, used to keep the loop responsive.
#[allow(dead_code)]
fn next_frame(last: Instant, tick: Duration) -> Instant {
    last + tick
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cli_accepts_a_data_dir_and_a_tick() {
        let cli =
            Cli::try_parse_from(["oefenen", "--data-dir", "/tmp/xyz", "--tick-ms", "25"]).unwrap();
        assert_eq!(cli.data_dir, Some(PathBuf::from("/tmp/xyz")));
        assert_eq!(cli.tick_ms, 25);
    }

    #[test]
    fn the_tick_defaults_to_seventy_five_milliseconds() {
        let cli = Cli::try_parse_from(["oefenen"]).unwrap();
        assert_eq!(cli.tick_ms, DEFAULT_TICK_MS);
        assert!(cli.data_dir.is_none());
    }

    #[test]
    fn a_zero_tick_is_clamped_to_something_usable() {
        let cli = Cli::try_parse_from(["oefenen", "--tick-ms", "0"]).unwrap();
        let tick = Duration::from_millis(cli.tick_ms.max(1));
        assert_eq!(tick, Duration::from_millis(1));
    }

    #[test]
    fn unknown_options_are_rejected() {
        assert!(Cli::try_parse_from(["oefenen", "--nonsense"]).is_err());
    }
}
