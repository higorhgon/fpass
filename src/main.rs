mod app;
mod backend;
mod clipboard;
mod config;
mod db_app;
mod history;
mod kdbx2pass;
mod keepass;
mod pass;
mod ui;
mod util;

#[macro_use]
extern crate rust_i18n;

i18n!("locales", fallback = "pt-BR");

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{fs, io, path::PathBuf};

use app::App;
use history::History;

#[derive(PartialEq, Clone, Copy)]
pub enum AppMode {
    Search,
    Normal,
    ConfirmDelete,
    Form,
    PasswordInput,
    ConfirmCreateDb,
    CreateDb,
    Info,
    ContextMenu,
    Help,
    RenameGroup,
    ChooseDbType,
    CreatePassStore,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").unwrap_or_default();
    let config_dir = PathBuf::from(format!("{}/.config/fpass", home));
    fs::create_dir_all(&config_dir).ok();
    config::ensure_config_exists(&config_dir);

    let (config, theme) = config::setup_and_load_config();
    rust_i18n::set_locale(&config.language);

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-v" | "--version" => {
                println!("{}", t!("cli.version", version = env!("CARGO_PKG_VERSION")));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{}", t!("cli.help_title"));
                println!("\n{}", t!("cli.help_usage_header"));
                println!("{}", t!("cli.help_usage_line"));
                println!("  fpass --kdbx2pass <banco.kdbx> <KEYID1> [KEYID2...]");
                println!("\n{}", t!("cli.help_options_header"));
                println!("  -v, --version      {}", t!("cli.help_version"));
                println!("  -h, --help         {}", t!("cli.help_help"));
                println!("  --kdbx2pass        {}", t!("cli.help_kdbx2pass_1"));
                println!("                     {}", t!("cli.help_kdbx2pass_2"));
                return Ok(());
            }
            "--kdbx2pass" => {
                if args.len() < 4 {
                    eprintln!("{}", t!("cli.kdbx2pass_usage"));
                    std::process::exit(1);
                }
                if let Err(e) = kdbx2pass::run(&args[2], &args[3..]) {
                    eprintln!("fpass --kdbx2pass: {}", e);
                    std::process::exit(1);
                }
                return Ok(());
            }
            _ => {}
        }
    }

    let is_mac = std::env::consts::OS == "macos";

    let mut history = History::new(config.recency_enabled);

    let mut dbs = backend::find_databases(&config.search_path);
    history.sort_by_score(&mut dbs, |d| d.path.as_str());

    let db_backend = match db_app::run_selection_tui(dbs, theme.clone())? {
        Some(b) => b,
        None => std::process::exit(0),
    };

    if let backend::Backend::Keepass { db_path, .. } = &db_backend {
        history.record_use(db_path);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let term_backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(term_backend)?;

    let mut app = App::new(db_backend, is_mac, history, theme);
    let res = ui::run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    if let Err(err) = res { println!("{:?}", err) }
    Ok(())
}
