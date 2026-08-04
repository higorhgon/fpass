mod app;
mod config;
mod db_app;
mod history;
mod keepass;
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
                println!("fpass - Gerenciador de senhas TUI para KeePassXC");
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

    let mut dbs = keepass::find_databases(&config.search_path);
    history.sort_items(&mut dbs);

    let (db_path, password) = match db_app::run_selection_tui(dbs, theme.clone())? {
        Some(res) => res,
        None => std::process::exit(0),
    };

    history.record_use(&db_path);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db_path, password, is_mac, history, theme);
    let res = ui::run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    if let Err(err) = res { println!("{:?}", err) }
    Ok(())
}
