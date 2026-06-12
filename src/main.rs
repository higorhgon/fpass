use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{
    io::{self, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(PartialEq)]
enum AppMode {
    Search, // Modo padrão FZF-like
    Normal, // Modo Neovim (j/k)
    AddPath,
    AddUser,
    AddPassword,
    EditPassword,
    ConfirmDelete,
}

struct App {
    db_path: String,
    password: String,
    
    // Listas e Filtros
    entries: Vec<String>,
    filtered_entries: Vec<String>,
    search_query: String,
    list_state: ListState,
    
    // Estado do App
    mode: AppMode,
    input_buffer: String,
    
    // Temporários para Adição
    add_path: String,
    add_user: String,

    // Feedback (Mensagem, Tempo, É Erro?)
    message: Option<(String, Instant, bool)>,
    is_mac: bool,
}

impl App {
    fn new(db_path: String, password: String, is_mac: bool) -> Self {
        let mut app = Self {
            db_path,
            password,
            entries: vec![],
            filtered_entries: vec![],
            search_query: String::new(),
            list_state: ListState::default(),
            mode: AppMode::Search, // Começa filtrando, igual ao fzf
            input_buffer: String::new(),
            add_path: String::new(),
            add_user: String::new(),
            message: None,
            is_mac,
        };
        app.refresh_entries();
        app
    }

    fn refresh_entries(&mut self) {
        let mut cmd = Command::new("keepassxc-cli");
        cmd.args(["ls", "-Rfq", &self.db_path])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(format!("{}\n", self.password).as_bytes());
            }
            if let Ok(output) = child.wait_with_output() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                self.entries = out_str
                    .lines()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && !s.ends_with('/'))
                    .map(String::from)
                    .collect();
            }
        }
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            self.filtered_entries = self.entries.clone();
        } else {
            self.filtered_entries = self.entries
                .iter()
                .filter(|e| e.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }

        if !self.filtered_entries.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    fn next(&mut self) {
        if self.filtered_entries.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i >= self.filtered_entries.len() - 1 { 0 } else { i + 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        if self.filtered_entries.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i == 0 { self.filtered_entries.len() - 1 } else { i - 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn get_selected(&self) -> Option<String> {
        self.list_state.selected().map(|i| self.filtered_entries[i].clone())
    }

    fn set_msg(&mut self, msg: &str, is_error: bool) {
        self.message = Some((msg.to_string(), Instant::now(), is_error));
    }

    // --- Métodos de CRUD inalterados (Omitidos aqui por clareza, mas mantidos iguais) ---
    fn copy_password(&mut self) {
        if let Some(entry) = self.get_selected() {
            let mut cmd = Command::new("keepassxc-cli");
            cmd.args(["show", "-q", &self.db_path, &entry, "-a", "Password"])
                .stdin(Stdio::piped()).stdout(Stdio::piped());

            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(format!("{}\n", self.password).as_bytes());
                }
                if let Ok(output) = child.wait_with_output() {
                    let entry_pass = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let cmd_name = if self.is_mac { "pbcopy" } else { "wl-copy" };
                    if let Ok(mut copy_child) = Command::new(cmd_name).stdin(Stdio::piped()).spawn() {
                        if let Some(mut stdin) = copy_child.stdin.take() {
                            let _ = stdin.write_all(entry_pass.as_bytes());
                        }
                        if copy_child.wait().is_ok() {
                            self.set_msg(&format!("Copiado: {}", entry), false);
                            spawn_clipboard_clearer(entry_pass, self.is_mac);
                            return;
                        }
                    }
                }
            }
            self.set_msg("Erro ao copiar senha.", true);
        }
    }

    fn execute_add(&mut self, new_pass: String) {
        let input_str = format!("{}\n{}\n{}\n", self.password, new_pass, new_pass);
        let mut cmd = Command::new("keepassxc-cli");
        cmd.args(["add", "-q", "-p", "-u", &self.add_user, &self.db_path, &self.add_path])
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());

        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(input_str.as_bytes()); }
            if child.wait().map(|s| s.success()).unwrap_or(false) {
                self.set_msg("Entrada adicionada!", false);
                self.refresh_entries();
            } else { self.set_msg("Erro ao adicionar.", true); }
        }
    }

    fn execute_edit(&mut self, new_pass: String) {
        if let Some(entry) = self.get_selected() {
            let input_str = format!("{}\n{}\n{}\n", self.password, new_pass, new_pass);
            let mut cmd = Command::new("keepassxc-cli");
            cmd.args(["edit", "-q", "-p", &self.db_path, &entry])
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());

            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(input_str.as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    self.set_msg("Senha editada!", false);
                } else { self.set_msg("Erro ao editar.", true); }
            }
        }
    }

    fn delete_selected(&mut self) {
        if let Some(entry) = self.get_selected() {
            let mut cmd = Command::new("keepassxc-cli");
            cmd.args(["rm", "-q", &self.db_path, &entry])
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());

            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n", self.password).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    self.set_msg("Entrada excluída!", false);
                    self.refresh_entries();
                    self.previous();
                } else { self.set_msg("Erro ao excluir.", true); }
            }
        }
    }
}

fn spawn_clipboard_clearer(password: String, is_mac: bool) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        let cmd_name = if is_mac { "pbcopy" } else { "wl-copy" };
        if let Ok(mut child) = Command::new(cmd_name).stdin(Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(b""); }
            let _ = child.wait();
        }
        if !is_mac { let _ = Command::new("cliphist").args(["delete-query", &password]).status(); }
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let is_mac = std::env::consts::OS == "macos";
    
    // Agora o sistema de seleção de DB é um menu seletivo!
    let db_path = select_database();

    print!("[KeePassXC] Senha para '{}': ", db_path);
    io::stdout().flush()?;
    let password = rpassword::read_password()?;

    let test_cmd = Command::new("keepassxc-cli")
        .args(["ls", "-q", &db_path])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn();
        
    if let Ok(mut child) = test_cmd {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(format!("{}\n", password).as_bytes());
        }
        if !child.wait()?.success() {
            println!("Senha incorreta ou erro ao acessar o banco.");
            std::process::exit(1);
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db_path, password, is_mac);
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = res { println!("{:?}", err) }
    Ok(())
}

fn select_database() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let output = Command::new("fd").args([".kdbx$", &home]).output().expect("fd não encontrado.");
    let out_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = out_str.lines().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    
    if lines.is_empty() {
        println!("Nenhum arquivo .kdbx encontrado.");
        std::process::exit(1);
    }
    
    if lines.len() == 1 {
        return lines[0].to_string();
    }

    // Se tiver mais de um, apresenta um menu simples CLI antes de abrir o Ratatui
    println!("Múltiplos bancos de dados encontrados:");
    for (i, db) in lines.iter().enumerate() {
        println!("  [{}] {}", i + 1, db);
    }
    print!("Selecione o número do banco: ");
    io::stdout().flush().unwrap();
    
    let mut choice = String::new();
    io::stdin().read_line(&mut choice).unwrap();
    
    let idx: usize = choice.trim().parse().unwrap_or(1);
    let final_idx = if idx > 0 && idx <= lines.len() { idx - 1 } else { 0 };
    
    lines[final_idx].to_string()
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    AppMode::Search => match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal, // Vai para modo Neovim
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
                        KeyCode::Char('q') => return Ok(()), // Sai do app
                        KeyCode::Down | KeyCode::Char('j') => app.next(),
                        KeyCode::Up | KeyCode::Char('k') => app.previous(),
                        KeyCode::Enter => app.copy_password(),
                        KeyCode::Char('/') | KeyCode::Char('f') => app.mode = AppMode::Search, // Volta pro filtro
                        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.mode = AppMode::AddPath; app.input_buffer.clear();
                        }
                        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if app.get_selected().is_some() { app.mode = AppMode::EditPassword; app.input_buffer.clear(); }
                        }
                        KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if app.get_selected().is_some() { app.mode = AppMode::ConfirmDelete; }
                        }
                        _ => {}
                    },
                    AppMode::ConfirmDelete => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                            app.delete_selected(); app.mode = AppMode::Normal;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.mode = AppMode::Normal,
                        _ => {}
                    },
                    _ => match key.code {
                        KeyCode::Esc => { app.mode = AppMode::Normal; app.input_buffer.clear(); }
                        KeyCode::Char(c) => app.input_buffer.push(c),
                        KeyCode::Backspace => { app.input_buffer.pop(); }
                        KeyCode::Enter => {
                            match app.mode {
                                AppMode::AddPath => {
                                    app.add_path = app.input_buffer.clone(); app.input_buffer.clear(); app.mode = AppMode::AddUser;
                                }
                                AppMode::AddUser => {
                                    app.add_user = app.input_buffer.clone(); app.input_buffer.clear(); app.mode = AppMode::AddPassword;
                                }
                                AppMode::AddPassword => {
                                    app.execute_add(app.input_buffer.clone()); app.input_buffer.clear(); app.mode = AppMode::Normal;
                                }
                                AppMode::EditPassword => {
                                    app.execute_edit(app.input_buffer.clone()); app.input_buffer.clear(); app.mode = AppMode::Normal;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn draw_ui(f: &mut Frame, app: &mut App) {
    // Agora o layout possui 3 áreas: Barra de Pesquisa, Lista, e Rodapé
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
        .split(f.size());

    // 1. Desenhando a Barra de Pesquisa no topo
    let (search_text, search_color) = if app.mode == AppMode::Search {
        (format!(" {}█ ", app.search_query), Color::Yellow) // Cursor em bloco simulado
    } else {
        (format!(" {} ", app.search_query), Color::DarkGray)
    };
    
    let search_p = Paragraph::new(search_text)
        .block(Block::default().title(" Pesquisar (/) ").borders(Borders::ALL).style(Style::default().fg(search_color)));
    f.render_widget(search_p, chunks[0]);

    // 2. Desenhando a Lista filtrada
    let items: Vec<ListItem> = app.filtered_entries.iter().map(|e| ListItem::new(e.as_str())).collect();
    
    let list_title = if app.mode == AppMode::Normal { " MODO NORMAL " } else { " MODO PESQUISA " };
    let list_color = if app.mode == AppMode::Normal { Color::Cyan } else { Color::White };

    let list = List::new(items)
        .block(Block::default().title(list_title).borders(Borders::ALL).style(Style::default().fg(list_color)))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol(">> ");
    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    // 3. Rodapé
    let footer_text = if app.mode == AppMode::Normal {
        "j/k: Navegar | / ou f: Pesquisar | ENTER: Copiar | CTRL-A/E/X: Ações | q: Sair"
    } else {
        "ESC: Modo Normal | ENTER: Copiar"
    };
    
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL)).alignment(Alignment::Center);
    f.render_widget(footer, chunks[2]);

    // Modais flutuantes por cima de tudo
    if app.mode != AppMode::Normal && app.mode != AppMode::Search {
        let area = centered_rect(50, 15, f.size());
        f.render_widget(Clear, area);
        
        match app.mode {
            AppMode::AddPath => draw_modal(f, area, "Nova Entrada: Caminho (ex: Pessoal/Twitter)", &app.input_buffer),
            AppMode::AddUser => draw_modal(f, area, "Nova Entrada: Usuário", &app.input_buffer),
            AppMode::AddPassword | AppMode::EditPassword => {
                let title = if app.mode == AppMode::AddPassword { "Nova Entrada: Senha" } else { "Editar Senha" };
                let hidden: String = app.input_buffer.chars().map(|_| '*').collect();
                draw_modal(f, area, title, &hidden)
            },
            AppMode::ConfirmDelete => {
                let text = format!("Deseja EXCLUIR '{}'? [y/N]", app.get_selected().unwrap_or_default());
                let p = Paragraph::new(text).block(Block::default().title(" Confirmação ").borders(Borders::ALL).style(Style::default().fg(Color::Red))).alignment(Alignment::Center);
                f.render_widget(p, area);
            }
            _ => {}
        }
    }

    if let Some((msg, time, is_error)) = &app.message {
        if time.elapsed() < Duration::from_secs(2) {
            let area = centered_rect(40, 10, f.size());
            f.render_widget(Clear, area);
            let color = if *is_error { Color::Red } else { Color::Green };
            let p = Paragraph::new(msg.as_str()).block(Block::default().borders(Borders::ALL).style(Style::default().fg(color))).alignment(Alignment::Center);
            f.render_widget(p, area);
        } else {
            app.message = None;
        }
    }
}

fn draw_modal(f: &mut Frame, area: Rect, title: &str, input: &str) {
    let text = format!("> {}█", input); // Adicionado cursor em bloco no modal também
    let p = Paragraph::new(text).block(Block::default().title(title).borders(Borders::ALL));
    f.render_widget(p, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([ Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2) ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([ Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2) ])
        .split(popup_layout[1])[1]
}