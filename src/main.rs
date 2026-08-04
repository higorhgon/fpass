mod app;
mod backend;
mod clipboard;
mod config;
mod db_app;
mod history;
mod keepass;
mod pass;
mod ui;
mod util;

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
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-v" | "--version" => {
                println!("fpass versão {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("fpass - Gerenciador de senhas TUI para KeePassXC e pass");
                println!("\nUso:");
                println!("  fpass [opções]");
                println!("\nOpções:");
                println!("  -v, --version    Exibe a versão do programa");
                println!("  -h, --help       Exibe esta mensagem de ajuda");
                return Ok(());
            }
            _ => {}
        }
    }

    let is_mac = std::env::consts::OS == "macos";

    let home = std::env::var("HOME").unwrap_or_default();
    let config_dir = PathBuf::from(format!("{}/.config/fpass", home));
    fs::create_dir_all(&config_dir).ok();
    config::ensure_config_exists(&config_dir);

    let (config, theme) = config::setup_and_load_config();
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
