//! fpass 2.x — TUI rápida para o `pass` (the standard unix password manager).
//!
//! Mudanças em relação ao 1.x (backend keepassxc-cli):
//! - Autenticação delegada ao gpg-agent: o fpass nunca vê a senha mestra.
//! - Sem tela de seleção de banco: um password-store único (configurável).
//! - Segredos em memória usam Zeroizing e são zerados após uso.

mod app;
mod clipboard;
mod config;
mod history;
mod store;
mod ui;

use app::{App, AppMode};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use history::History;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;
use std::io;
use std::time::Duration;
use store::PassStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-v" | "--version" => {
                println!("fpass versão {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            _ => {}
        }
    }

    let is_mac = std::env::consts::OS == "macos";

    let config_dir = config::config_dir();
    std::fs::create_dir_all(&config_dir).ok();
    config::ensure_config_exists(&config_dir);

    let (cfg, theme) = config::setup_and_load_config();
    let history = History::new(cfg.recency_enabled, &config_dir);

    // Abre o password-store. Erro aqui (ex: store não inicializado)
    // sai com mensagem amigável, sem panic.
    let store = match PassStore::open(cfg.store_path.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fpass: {}", e);
            std::process::exit(1);
        }
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(store, is_mac, history, theme, cfg.clip_time);
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    if let Err(err) = res {
        eprintln!("{:?}", err);
    }
    Ok(())
}

fn print_help() {
    println!("fpass - TUI para o pass (the standard unix password manager)");
    println!("\nUso:");
    println!("  fpass [opções]");
    println!("\nOpções:");
    println!("  -v, --version    Exibe a versão do programa");
    println!("  -h, --help       Exibe esta mensagem de ajuda");
    println!("\nRequisitos:");
    println!("  - pass instalado e store inicializado (pass init <GPG-KEY-ID>)");
    println!("  - gpg-agent com um pinentry gráfico (pinentry-gnome3/qt) —");
    println!("    pinentry-curses conflita com a TUI no mesmo terminal.");
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw_ui(f, app))?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };

        let mut is_g_key = false;

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if app.mode == AppMode::Search || app.mode == AppMode::Normal {
                match key.code {
                    KeyCode::Char('d') => app.half_page_down(),
                    KeyCode::Char('u') => app.half_page_up(),
                    KeyCode::Char('a') => app.open_add_form(),
                    KeyCode::Char('e') => {
                        if let Some(entry) = app.get_selected() {
                            app.open_edit_form(entry);
                        }
                    }
                    KeyCode::Char('x') => {
                        if app.get_selected().is_some() {
                            app.mode = AppMode::ConfirmDelete;
                        }
                    }
                    KeyCode::Char('c') => return Ok(()),
                    _ => {}
                }
                continue;
            } else if key.code == KeyCode::Char('c') {
                return Ok(());
            }
        }

        match app.mode {
                AppMode::Search => match key.code {
                KeyCode::Esc => app.mode = AppMode::Normal,
                KeyCode::Down => app.next(),
                KeyCode::Up => app.previous(),
                KeyCode::Enter => app.copy_password(),
                KeyCode::Backspace => {
                    app.search_query.pop();
                    app.apply_filter();
                }
                KeyCode::Char(c) => {
                    app.search_query.push(c);
                    app.apply_filter();
                }
                _ => {}
            },
                AppMode::Normal => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.previous(),
                KeyCode::Enter => app.copy_password(),
                KeyCode::Char('/') | KeyCode::Char('f') => app.mode = AppMode::Search,
                KeyCode::Char('G') => app.go_to_bottom(),
                KeyCode::Char('g') => {
                    is_g_key = true;
                    if app.last_key_was_g {
                        app.go_to_top();
                        is_g_key = false;
                    }
                }
                _ => {}
            },
            AppMode::ConfirmDelete => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    app.delete_selected();
                    app.mode = AppMode::Normal;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Enter | KeyCode::Esc => {
                    app.mode = AppMode::Normal;
                }
                _ => {}
            },
            AppMode::Form => match key.code {
                KeyCode::Esc => {
                    // Zera a senha do form ao cancelar
                    app.form_password = zeroize::Zeroizing::new(String::new());
                    app.mode = AppMode::Normal;
                }
                KeyCode::BackTab => {
                    app.form_active_field = if app.form_active_field == 0 {
                        4
                    } else {
                        app.form_active_field - 1
                    };
                }
                KeyCode::Tab => {
                    app.form_active_field = (app.form_active_field + 1) % 5;
                }
                KeyCode::Down => {
                    if app.form_active_field == 0 {
                        app.form_next_group();
                    }
                }
                KeyCode::Up => {
                    if app.form_active_field == 0 {
                        app.form_prev_group();
                    }
                }
                KeyCode::Enter => {
                    if app.form_active_field == 0 {
                        if let Some(i) = app.form_group_state.selected() {
                            app.form_group = app.filtered_groups[i].clone();
                            app.form_active_field = 1;
                        } else {
                            app.form_active_field = 1;
                        }
                    } else {
                        app.submit_form();
                    }
                }
                KeyCode::Backspace => match app.form_active_field {
                    0 => {
                        app.form_group.pop();
                        app.filter_form_groups();
                    }
                    1 => {
                        app.form_title.pop();
                    }
                    2 => {
                        app.form_username.pop();
                    }
                    3 => {
                        app.form_password.pop();
                    }
                    4 => {
                        app.form_url.pop();
                    }
                    _ => {}
                },
                KeyCode::Char(c) => match app.form_active_field {
                    0 => {
                        app.form_group.push(c);
                        app.filter_form_groups();
                    }
                    1 => {
                        app.form_title.push(c);
                    }
                    2 => {
                        app.form_username.push(c);
                    }
                    3 => {
                        app.form_password.push(c);
                    }
                    4 => {
                        app.form_url.push(c);
                    }
                    _ => {}
                },
                _ => {}
            },
            AppMode::GpgUnlock => match key.code {
                KeyCode::Enter => {
                    if !app.unlock_input.is_empty() {
                        app.confirm_unlock();
                    }
                }
                KeyCode::Backspace => {
                    app.unlock_input.pop();
                }
                KeyCode::Char(c) => {
                    app.unlock_input.push(c);
                }
                _ => {}
            },
        }
        app.last_key_was_g = is_g_key;
    }
}
