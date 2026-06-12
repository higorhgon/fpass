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

#[derive(PartialEq, Clone, Copy)]
enum AppMode {
    Search,
    Normal,
    AddPath,
    AddUser,
    AddPassword,
    EditPassword,
    ConfirmDelete,
}

// ==========================================
// APLICATIVO 1: SELETOR DE BANCO DE DADOS
// ==========================================

struct DbApp {
    entries: Vec<String>,
    filtered: Vec<String>,
    search_query: String,
    list_state: ListState,
    mode: AppMode,
    last_key_was_g: bool,
    list_height: usize,
}

impl DbApp {
    fn new(dbs: Vec<String>) -> Self {
        let mut app = Self {
            entries: dbs.clone(),
            filtered: dbs,
            search_query: String::new(),
            list_state: ListState::default(),
            mode: AppMode::Search,
            last_key_was_g: false,
            list_height: 10,
        };
        if !app.filtered.is_empty() {
            app.list_state.select(Some(0));
        }
        app
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        if q.is_empty() {
            self.filtered = self.entries.clone();
        } else {
            self.filtered = self.entries.iter().filter(|e| e.to_lowercase().contains(&q)).cloned().collect();
        }
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    // Navegação (Compartilhada conceitualmente com o app principal)
    fn next(&mut self) {
        if self.filtered.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i >= self.filtered.len() - 1 { 0 } else { i + 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }
    fn previous(&mut self) {
        if self.filtered.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i == 0 { self.filtered.len() - 1 } else { i - 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }
    fn go_to_top(&mut self) {
        if !self.filtered.is_empty() { self.list_state.select(Some(0)); }
    }
    fn go_to_bottom(&mut self) {
        if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); }
    }
    fn half_page_down(&mut self) {
        if self.filtered.is_empty() { return; }
        let step = (self.list_height.saturating_sub(2) / 2).max(1);
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((i + step).min(self.filtered.len() - 1)));
    }
    fn half_page_up(&mut self) {
        if self.filtered.is_empty() { return; }
        let step = (self.list_height.saturating_sub(2) / 2).max(1);
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(step)));
    }
}

// ==========================================
// APLICATIVO 2: GERENCIADOR DE SENHAS (PRINCIPAL)
// ==========================================

struct App {
    db_path: String,
    password: String,
    entries: Vec<String>,
    filtered: Vec<String>,
    search_query: String,
    list_state: ListState,
    mode: AppMode,
    input_buffer: String,
    add_path: String,
    add_user: String,
    message: Option<(String, Instant, bool)>,
    is_mac: bool,
    
    // Controle de Navegação
    last_key_was_g: bool,
    list_height: usize,
}

impl App {
    fn new(db_path: String, password: String, is_mac: bool) -> Self {
        let mut app = Self {
            db_path, password,
            entries: vec![], filtered: vec![],
            search_query: String::new(), list_state: ListState::default(),
            mode: AppMode::Search, input_buffer: String::new(), add_path: String::new(), add_user: String::new(),
            message: None, is_mac, last_key_was_g: false, list_height: 10,
        };
        app.refresh_entries();
        app
    }

    fn refresh_entries(&mut self) {
        let mut cmd = Command::new("keepassxc-cli");
        cmd.args(["ls", "-Rfq", &self.db_path]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n", self.password).as_bytes()); }
            if let Ok(output) = child.wait_with_output() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                self.entries = out_str.lines().map(|s| s.trim()).filter(|s| !s.is_empty() && !s.ends_with('/')).map(String::from).collect();
            }
        }
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        if q.is_empty() {
            self.filtered = self.entries.clone();
        } else {
            self.filtered = self.entries.iter().filter(|e| e.to_lowercase().contains(&q)).cloned().collect();
        }
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    fn next(&mut self) {
        if self.filtered.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i >= self.filtered.len() - 1 { 0 } else { i + 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }
    fn previous(&mut self) {
        if self.filtered.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i == 0 { self.filtered.len() - 1 } else { i - 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }
    fn go_to_top(&mut self) {
        if !self.filtered.is_empty() { self.list_state.select(Some(0)); }
    }
    fn go_to_bottom(&mut self) {
        if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); }
    }
    fn half_page_down(&mut self) {
        if self.filtered.is_empty() { return; }
        let step = (self.list_height.saturating_sub(2) / 2).max(1);
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((i + step).min(self.filtered.len() - 1)));
    }
    fn half_page_up(&mut self) {
        if self.filtered.is_empty() { return; }
        let step = (self.list_height.saturating_sub(2) / 2).max(1);
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(step)));
    }

    fn get_selected(&self) -> Option<String> {
        self.list_state.selected().map(|i| self.filtered[i].clone())
    }
    fn set_msg(&mut self, msg: &str, is_error: bool) {
        self.message = Some((msg.to_string(), Instant::now(), is_error));
    }

    // Ações de Backend omitidas para manter curto (mas funcionam idênticas ao código anterior)
    fn copy_password(&mut self) {
        if let Some(entry) = self.get_selected() {
            let mut cmd = Command::new("keepassxc-cli");
            cmd.args(["show", "-q", &self.db_path, &entry, "-a", "Password"]).stdin(Stdio::piped()).stdout(Stdio::piped());
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n", self.password).as_bytes()); }
                if let Ok(output) = child.wait_with_output() {
                    let entry_pass = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let cmd_name = if self.is_mac { "pbcopy" } else { "wl-copy" };
                    if let Ok(mut copy_child) = Command::new(cmd_name).stdin(Stdio::piped()).spawn() {
                        if let Some(mut stdin) = copy_child.stdin.take() { let _ = stdin.write_all(entry_pass.as_bytes()); }
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
        let mut cmd = Command::new("keepassxc-cli");
        cmd.args(["add", "-q", "-p", "-u", &self.add_user, &self.db_path, &self.add_path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n{}\n{}\n", self.password, new_pass, new_pass).as_bytes()); }
            if child.wait().map(|s| s.success()).unwrap_or(false) {
                self.set_msg("Entrada adicionada!", false); self.refresh_entries();
            } else { self.set_msg("Erro ao adicionar.", true); }
        }
    }
    fn execute_edit(&mut self, new_pass: String) {
        if let Some(entry) = self.get_selected() {
            let mut cmd = Command::new("keepassxc-cli");
            cmd.args(["edit", "-q", "-p", &self.db_path, &entry]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n{}\n{}\n", self.password, new_pass, new_pass).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) { self.set_msg("Senha editada!", false); } 
                else { self.set_msg("Erro ao editar.", true); }
            }
        }
    }
    fn delete_selected(&mut self) {
        if let Some(entry) = self.get_selected() {
            let mut cmd = Command::new("keepassxc-cli");
            cmd.args(["rm", "-q", &self.db_path, &entry]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n", self.password).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    self.set_msg("Entrada excluída!", false); self.refresh_entries(); self.previous();
                } else { self.set_msg("Erro ao excluir.", true); }
            }
        }
    }
}

// ==========================================
// FUNÇÕES PRINCIPAIS E LOOPS
// ==========================================

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
    
    // 1. Busca os bancos e inicia interface de seleção (se necessário)
    let dbs = find_databases();
    let db_path = if dbs.is_empty() {
        println!("Nenhum arquivo .kdbx encontrado.");
        std::process::exit(1);
    } else if dbs.len() == 1 {
        dbs[0].clone()
    } else {
        // Inicia o terminal gráfico Apenas para escolher o banco
        run_selection_tui(dbs)?
    };

    // 2. Coleta a senha com prompt limpo no terminal padrão
    print!("[KeePassXC] Senha para '{}': ", db_path);
    io::stdout().flush()?;
    let password = rpassword::read_password()?;

    // Testa senha
    let mut test_cmd = Command::new("keepassxc-cli").args(["ls", "-q", &db_path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    if let Some(mut stdin) = test_cmd.stdin.take() { let _ = stdin.write_all(format!("{}\n", password).as_bytes()); }
    if !test_cmd.wait()?.success() {
        println!("Senha incorreta ou erro ao acessar o banco.");
        std::process::exit(1);
    }

    // 3. Inicia o app principal
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

fn find_databases() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    if let Ok(output) = Command::new("fd").args([".kdbx$", &home]).output() {
        let out_str = String::from_utf8_lossy(&output.stdout);
        return out_str.lines().map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from).collect();
    }
    vec![]
}

// --- LOOP DO SELETOR DE DB ---
fn run_selection_tui(dbs: Vec<String>) -> Result<String, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = DbApp::new(dbs);
    let mut selected = None;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)]).split(f.size());
            app.list_height = chunks[1].height as usize;

            let (search_text, search_color) = if app.mode == AppMode::Search { (format!(" {}█ ", app.search_query), Color::Yellow) } else { (format!(" {} ", app.search_query), Color::DarkGray) };
            f.render_widget(Paragraph::new(search_text).block(Block::default().title(" Filtrar Banco (/) ").borders(Borders::ALL).style(Style::default().fg(search_color))), chunks[0]);

            let list_color = if app.mode == AppMode::Normal { Color::Cyan } else { Color::White };
            let items: Vec<ListItem> = app.filtered.iter().map(|e| ListItem::new(e.as_str())).collect();
            let list = List::new(items).block(Block::default().title(" Bancos Disponíveis ").borders(Borders::ALL).style(Style::default().fg(list_color))).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol(">> ");
            f.render_stateful_widget(list, chunks[1], &mut app.list_state);

            f.render_widget(Paragraph::new("ENTER: Selecionar | ESC/q: Cancelar").block(Block::default().borders(Borders::ALL)).alignment(Alignment::Center), chunks[2]);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut is_g_key = false;

                // Controles universais em ambos os modos
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('d') => app.half_page_down(),
                        KeyCode::Char('u') => app.half_page_up(),
                        KeyCode::Char('c') => std::process::exit(0),
                        _ => {}
                    }
                    continue;
                }

                match app.mode {
                    AppMode::Search => match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Down => app.next(),
                        KeyCode::Up => app.previous(),
                        KeyCode::Enter => { if let Some(i) = app.list_state.selected() { selected = Some(app.filtered[i].clone()); break; } },
                        KeyCode::Backspace => { app.search_query.pop(); app.apply_filter(); }
                        KeyCode::Char(c) => { app.search_query.push(c); app.apply_filter(); }
                        _ => {}
                    },
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => std::process::exit(0),
                        KeyCode::Down | KeyCode::Char('j') => app.next(),
                        KeyCode::Up | KeyCode::Char('k') => app.previous(),
                        KeyCode::Char('/') | KeyCode::Char('f') => app.mode = AppMode::Search,
                        KeyCode::Char('G') => app.go_to_bottom(),
                        KeyCode::Char('g') => {
                            is_g_key = true;
                            if app.last_key_was_g { app.go_to_top(); is_g_key = false; }
                        }
                        KeyCode::Enter => { if let Some(i) = app.list_state.selected() { selected = Some(app.filtered[i].clone()); break; } },
                        _ => {}
                    },
                    _ => {}
                }
                app.last_key_was_g = is_g_key;
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(selected.unwrap())
}

// --- LOOP PRINCIPAL DO APP ---
fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut is_g_key = false;

                // CORREÇÃO: Verifica explicitamente o CTRL isolado primeiro
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if app.mode == AppMode::Search || app.mode == AppMode::Normal {
                        match key.code {
                            KeyCode::Char('d') => app.half_page_down(),
                            KeyCode::Char('u') => app.half_page_up(),
                            KeyCode::Char('a') => { app.mode = AppMode::AddPath; app.input_buffer.clear(); }
                            KeyCode::Char('e') => { if app.get_selected().is_some() { app.mode = AppMode::EditPassword; app.input_buffer.clear(); } }
                            KeyCode::Char('x') => { if app.get_selected().is_some() { app.mode = AppMode::ConfirmDelete; } }
                            KeyCode::Char('c') => return Ok(()),
                            _ => {}
                        }
                        continue; // Processou o Ctrl, recomeça o loop
                    } else if key.code == KeyCode::Char('c') {
                        // Permite usar Ctrl+C para forçar a saída mesmo dentro dos modais de adição/edição
                        return Ok(());
                    }
                }

                match app.mode {
                    AppMode::Search => match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Down => app.next(),
                        KeyCode::Up => app.previous(),
                        KeyCode::Enter => app.copy_password(),
                        KeyCode::Backspace => { app.search_query.pop(); app.apply_filter(); }
                        KeyCode::Char(c) => { app.search_query.push(c); app.apply_filter(); }
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
                            if app.last_key_was_g { app.go_to_top(); is_g_key = false; }
                        }
                        _ => {}
                    },
                    AppMode::ConfirmDelete => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => { app.delete_selected(); app.mode = AppMode::Normal; }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.mode = AppMode::Normal,
                        _ => {}
                    },
                    _ => match key.code {
                        KeyCode::Esc => { app.mode = AppMode::Normal; app.input_buffer.clear(); }
                        KeyCode::Char(c) => app.input_buffer.push(c),
                        KeyCode::Backspace => { app.input_buffer.pop(); }
                        KeyCode::Enter => {
                            match app.mode {
                                AppMode::AddPath => { app.add_path = app.input_buffer.clone(); app.input_buffer.clear(); app.mode = AppMode::AddUser; }
                                AppMode::AddUser => { app.add_user = app.input_buffer.clone(); app.input_buffer.clear(); app.mode = AppMode::AddPassword; }
                                AppMode::AddPassword => { app.execute_add(app.input_buffer.clone()); app.input_buffer.clear(); app.mode = AppMode::Normal; }
                                AppMode::EditPassword => { app.execute_edit(app.input_buffer.clone()); app.input_buffer.clear(); app.mode = AppMode::Normal; }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                app.last_key_was_g = is_g_key;
            }
        }
    }
}

fn draw_ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)]).split(f.size());
    
    // Atualiza a altura disponível da lista para calcular meia página
    app.list_height = chunks[1].height as usize;

    let (search_text, search_color) = if app.mode == AppMode::Search { (format!(" {}█ ", app.search_query), Color::Yellow) } else { (format!(" {} ", app.search_query), Color::DarkGray) };
    f.render_widget(Paragraph::new(search_text).block(Block::default().title(" Pesquisar (/) ").borders(Borders::ALL).style(Style::default().fg(search_color))), chunks[0]);

    let list_title = if app.mode == AppMode::Normal { " NORMAL (j/k) " } else { " PESQUISA " };
    let list_color = if app.mode == AppMode::Normal { Color::Cyan } else { Color::White };
    let items: Vec<ListItem> = app.filtered.iter().map(|e| ListItem::new(e.as_str())).collect();
    let list = List::new(items).block(Block::default().title(list_title).borders(Borders::ALL).style(Style::default().fg(list_color))).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol(">> ");
    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    f.render_widget(Paragraph::new("gg/G: Topo/Fim | CTRL-U/D: Page Up/Down | ENTER: Copiar | CTRL-A/E/X: Ações").block(Block::default().borders(Borders::ALL)).alignment(Alignment::Center), chunks[2]);

    if app.mode != AppMode::Normal && app.mode != AppMode::Search {
        let area = centered_rect(50, 15, f.size());
        f.render_widget(Clear, area);
        match app.mode {
            AppMode::AddPath => draw_modal(f, area, "Nova Entrada: Caminho", &app.input_buffer),
            AppMode::AddUser => draw_modal(f, area, "Nova Entrada: Usuário", &app.input_buffer),
            AppMode::AddPassword | AppMode::EditPassword => draw_modal(f, area, if app.mode == AppMode::AddPassword { "Nova Senha" } else { "Editar Senha" }, &app.input_buffer.chars().map(|_| '*').collect::<String>()),
            AppMode::ConfirmDelete => f.render_widget(Paragraph::new(format!("Deseja EXCLUIR '{}'? [y/N]", app.get_selected().unwrap_or_default())).block(Block::default().title(" Confirmar ").borders(Borders::ALL).style(Style::default().fg(Color::Red))).alignment(Alignment::Center), area),
            _ => {}
        }
    }

    if let Some((msg, time, is_error)) = &app.message {
        if time.elapsed() < Duration::from_secs(2) {
            let area = centered_rect(40, 10, f.size());
            f.render_widget(Clear, area);
            f.render_widget(Paragraph::new(msg.as_str()).block(Block::default().borders(Borders::ALL).style(Style::default().fg(if *is_error { Color::Red } else { Color::Green }))).alignment(Alignment::Center), area);
        } else { app.message = None; }
    }
}

fn draw_modal(f: &mut Frame, area: Rect, title: &str, input: &str) {
    f.render_widget(Paragraph::new(format!("> {}█", input)).block(Block::default().title(title).borders(Borders::ALL)), area);
}
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)]).split(popup_layout[1])[1]
}
