mod app;
mod model;
mod storage;
mod theme;
mod ui;

use std::{
    error::Error,
    io::{self, IsTerminal},
    panic,
    time::Duration,
};

use app::App;
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use storage::Store;
use theme::AppTheme;

const HELP: &str = "who-me — a terminal dashboard for the different parts of who you are

Usage:
  who-me
  who-me --help
  who-me --version

Data is stored in $XDG_DATA_HOME/who-me/data.toml, or
~/.local/share/who-me/data.toml when XDG_DATA_HOME is not set.
";

fn main() {
    if let Err(error) = try_main() {
        eprintln!("who-me: {error}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn Error>> {
    match parse_args(std::env::args().skip(1))? {
        Command::Help => {
            print!("{HELP}");
            return Ok(());
        }
        Command::Version => {
            println!("who-me {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Command::Run => {}
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("the interactive dashboard must be run in a terminal".into());
    }

    let store = Store::discover()?;
    let document = store.load_or_create()?;
    let theme = AppTheme::load();
    run_tui(App::new(document), store, theme)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Run,
    Help,
    Version,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let args: Vec<String> = args.into_iter().collect();
    match args.as_slice() {
        [] => Ok(Command::Run),
        [argument] if argument == "-h" || argument == "--help" => Ok(Command::Help),
        [argument] if argument == "-V" || argument == "--version" => Ok(Command::Version),
        _ => Err(format!(
            "unexpected argument(s): {}\n\nRun `who-me --help` for usage.",
            args.join(" ")
        )),
    }
}

fn run_tui(mut app: App, store: Store, theme: AppTheme) -> Result<(), Box<dyn Error>> {
    install_panic_restoration();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            terminal.draw(|frame| ui::render(frame, &mut app, &theme))?;
            if !event::poll(Duration::from_millis(250))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }

            let previous = app.clone();
            let outcome = app.handle_key(key);
            if outcome.changed
                && let Err(error) = store.save(&app.document)
            {
                app = previous;
                app.set_error(format!("Could not save: {error}"));
            }
            if outcome.quit {
                break;
            }
        }
        Ok(())
    })();

    restore_terminal();
    result
}

fn install_panic_restoration() {
    let original = panic::take_hook();
    panic::set_hook(Box::new(move |information| {
        restore_terminal();
        original(information);
    }));
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_public_cli() {
        assert_eq!(parse_args(Vec::<String>::new()).unwrap(), Command::Run);
        assert_eq!(parse_args(["--help".into()]).unwrap(), Command::Help);
        assert_eq!(parse_args(["-V".into()]).unwrap(), Command::Version);
        assert!(parse_args(["--unknown".into()]).is_err());
        assert!(parse_args(["one".into(), "two".into()]).is_err());
    }
}
