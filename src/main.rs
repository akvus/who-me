mod app;
mod model;
mod storage;
mod sync;
mod theme;
mod ui;

use std::{
    error::Error,
    io::{self, IsTerminal},
    panic, thread,
    time::{Duration, Instant},
};

use app::{App, Mode, SyncAction};
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use storage::Store;
use sync::{ConfigStore, SyncConfig, SyncEvent, SyncPaths, SyncService, SyncStatus};
use theme::AppTheme;

const HELP: &str = "who-me — a terminal dashboard for identities and daily plans

Usage:
  who-me
  who-me --help
  who-me --version

Data is stored in $XDG_DATA_HOME/who-me/data.toml, or
~/.local/share/who-me/data.toml when XDG_DATA_HOME is not set.

Press 1 for Identities, 2 for Calendar, or g to configure private GitHub sync.
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
    let sync_paths = SyncPaths::discover()?;
    let config_store = ConfigStore::new(sync_paths.config.clone());
    let config = config_store.load();
    let service = SyncService::start(sync_paths);
    let mut app = App::new(document);
    match config {
        Ok(Some(config)) => {
            app.set_sync_repository(Some(config.repository.clone()));
            app.set_sync_branch(Some(config.branch.clone()));
            app.set_sync_status(SyncStatus::Pending);
            service.configure(config, app.document.clone(), 0);
        }
        Ok(None) => {}
        Err(error) => app.set_sync_status(SyncStatus::Error(error.to_string())),
    }
    run_tui(app, store, theme, config_store, service)
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

fn run_tui(
    mut app: App,
    store: Store,
    theme: AppTheme,
    config_store: ConfigStore,
    service: SyncService,
) -> Result<(), Box<dyn Error>> {
    install_panic_restoration();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut revision = 0u64;
        let mut pending_document = None;
        let mut sync_after = None;
        let mut last_periodic = Instant::now();
        let mut last_retry = Instant::now();
        loop {
            process_sync_events(
                &mut app,
                &store,
                &config_store,
                &service,
                revision,
                &mut pending_document,
            );
            if let Some((document, prepared_revision)) = pending_document.take() {
                if document_can_be_replaced(&app.mode) {
                    if prepared_revision == revision {
                        if document == app.document {
                            service.accept_prepared();
                        } else {
                            match store.save(&document) {
                                Ok(()) => {
                                    app.apply_document(document);
                                    service.accept_prepared();
                                }
                                Err(error) => {
                                    app.set_error(format!("Could not apply GitHub data: {error}"));
                                    service.retry_prepared(app.document.clone(), revision);
                                }
                            }
                        }
                    } else {
                        service.retry_prepared(app.document.clone(), revision);
                    }
                } else {
                    pending_document = Some((document, prepared_revision));
                }
            }

            if sync_after.is_some_and(|deadline| Instant::now() >= deadline) {
                service.synchronize(app.document.clone(), revision);
                sync_after = None;
            }
            if last_periodic.elapsed() >= Duration::from_secs(60)
                && app.sync_repository.is_some()
                && !matches!(app.sync_status, SyncStatus::Conflict(_))
            {
                service.synchronize(app.document.clone(), revision);
                last_periodic = Instant::now();
            }
            if last_retry.elapsed() >= Duration::from_secs(30)
                && app.sync_repository.is_some()
                && matches!(
                    app.sync_status,
                    SyncStatus::Offline(_) | SyncStatus::Pending
                )
            {
                service.synchronize(app.document.clone(), revision);
                last_retry = Instant::now();
            }

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
            } else if outcome.changed {
                revision = revision.saturating_add(1);
                if app.sync_repository.is_some() {
                    app.set_sync_status(SyncStatus::Pending);
                    sync_after = Some(Instant::now() + Duration::from_secs(1));
                }
            }
            if let Some(action) = outcome.sync_action {
                handle_sync_action(action, &mut app, &config_store, &service, revision);
            }
            if outcome.quit {
                if app.sync_repository.is_some() {
                    app.set_sync_status(SyncStatus::Syncing);
                    service.synchronize(app.document.clone(), revision);
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < deadline {
                        process_sync_events(
                            &mut app,
                            &store,
                            &config_store,
                            &service,
                            revision,
                            &mut pending_document,
                        );
                        if let Some((document, prepared_revision)) = pending_document.take() {
                            if prepared_revision == revision {
                                if document == app.document {
                                    service.accept_prepared();
                                } else {
                                    match store.save(&document) {
                                        Ok(()) => {
                                            app.apply_document(document);
                                            service.accept_prepared();
                                        }
                                        Err(error) => app.set_error(format!(
                                            "Could not apply GitHub data: {error}"
                                        )),
                                    }
                                }
                            } else {
                                service.retry_prepared(app.document.clone(), revision);
                            }
                        }
                        if matches!(
                            app.sync_status,
                            SyncStatus::Synced
                                | SyncStatus::Offline(_)
                                | SyncStatus::Error(_)
                                | SyncStatus::Conflict(_)
                        ) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                }
                break;
            }
        }
        Ok(())
    })();

    restore_terminal();
    result
}

fn document_can_be_replaced(mode: &Mode) -> bool {
    matches!(mode, Mode::Normal | Mode::Settings(_) | Mode::Help)
}

fn process_sync_events(
    app: &mut App,
    _store: &Store,
    config_store: &ConfigStore,
    service: &SyncService,
    _revision: u64,
    pending_document: &mut Option<(model::Document, u64)>,
) {
    while let Some(event) = service.try_recv() {
        match event {
            SyncEvent::Status(status) => app.set_sync_status(status),
            SyncEvent::Configured(config) => {
                if let Err(error) = config_store.save(&config) {
                    app.set_sync_status(SyncStatus::Error(error.to_string()));
                } else {
                    app.set_sync_repository(Some(config.repository));
                    app.set_sync_branch(Some(config.branch));
                }
            }
            SyncEvent::Prepared { document, revision } => {
                *pending_document = Some((document, revision));
            }
            SyncEvent::Disconnected => match config_store.clear() {
                Ok(()) => {
                    app.set_sync_repository(None);
                    app.set_sync_branch(None);
                    app.set_sync_status(SyncStatus::LocalOnly);
                    if let Mode::Settings(settings) = &mut app.mode {
                        settings.repository.clear();
                        settings.cursor = 0;
                    }
                }
                Err(error) => app.set_sync_status(SyncStatus::Error(error.to_string())),
            },
        }
    }
}

fn handle_sync_action(
    action: SyncAction,
    app: &mut App,
    config_store: &ConfigStore,
    service: &SyncService,
    revision: u64,
) {
    match action {
        SyncAction::Configure(repository) => {
            let config = SyncConfig::new(repository.clone());
            match config_store.save(&config) {
                Ok(()) => {
                    app.set_sync_repository(Some(repository));
                    app.set_sync_status(SyncStatus::Connecting);
                    service.configure(config, app.document.clone(), revision);
                }
                Err(error) => app.set_sync_status(SyncStatus::Error(error.to_string())),
            }
        }
        SyncAction::Synchronize => {
            app.set_sync_status(SyncStatus::Syncing);
            service.synchronize(app.document.clone(), revision);
        }
        SyncAction::Resolve(choice) => {
            app.set_sync_status(SyncStatus::Syncing);
            service.resolve(choice, app.document.clone(), revision);
        }
        SyncAction::Disconnect => service.disconnect(),
    }
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
