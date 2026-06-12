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
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sha2::{Sha256, Digest};

#[derive(PartialEq, Clone, Copy)]
enum AppMode {
    Search,
    Normal,
    ConfirmDelete,
    Form, // Novo Modo Único para Adição e Edição
}

// ==========================================
// MOTOR DE FRECENCY (Seguro com SHA-256)
// ==========================================

struct History {
    records: HashMap<String, (u32, u64)>, 
    file_path: PathBuf,
}

impl History {
    fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let file_path = PathBuf::from(format!("{}/.fpass_history", home));
        let mut records = HashMap::new();

        if let Ok(file) = fs::File::open(&file_path) {
            for line in BufReader::new(file).lines().flatten() {
                let parts: Vec<&str> = line.splitn(3, '|').collect();
                if parts.len() == 3 {
                    if let (Ok(count), Ok(ts)) = (parts[1].parse(), parts[2].parse()) {
                        records.insert(parts[0].to_string(), (count, ts));
                    }
                }
            }
        }
        Self { records, file_path }
    }

    fn hash_item(item: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(item.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn record_use(&mut self, item: &str) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let hashed_item = Self::hash_item(item);
        
        let entry = self.records.entry(hashed_item).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = now;
        self.save();
    }

    fn save(&self) {
        if let Ok(mut file) = OpenOptions::new().write(true).create(true).truncate(true).open(&self.file_path) {
            for (hash, (count, ts)) in &self.records {
                let _ = writeln!(file, "{}|{}|{}", hash, count, ts);
            }
        }
    }

    fn get_score(&self, item: &str) -> u64 {
        let hashed_item = Self::hash_item(item);
        if let Some(&(count, ts)) = self.records.get(&hashed_item) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            let age = now.saturating_sub(ts);
            
            let weight = if age < 86400 { 100 }
            else if age < 604800 { 50 }
            else if age < 2592000 { 20 }
            else { 5 };
            
            (count as u64) * weight
        } else {
            0
        }
    }

    fn sort_items(&self, items: &mut Vec<String>) {
        items.sort_by(|a, b| {
            let score_a = self.get_score(a);
            let score_b = self.get_score(b);
            score_b.cmp(&score_a).then_with(|| a.cmp(b))
        });
    }
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
            entries: dbs.clone(), filtered: dbs, search_query: String::new(),
            list_state: ListState::default(), mode: AppMode::Search, last_key_was_g: false, list_height: 10,
        };
        if !app.filtered.is_empty() { app.list_state.select(Some(0)); }
        app
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        if q.is_empty() { self.filtered = self.entries.clone(); } 
        else { self.filtered = self.entries.iter().filter(|e| e.to_lowercase().contains(&q)).cloned().collect(); }
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
    fn go_to_top(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(0)); } }
    fn go_to_bottom(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); } }
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
// APLICATIVO 2: GERENCIADOR DE SENHAS
// ==========================================

struct App {
    db_path: String,
    password: String,
    entries: Vec<String>,
    filtered: Vec<String>,
    search_query: String,
    list_state: ListState,
    mode: AppMode,
    message: Option<(String, Instant, bool)>,
    is_mac: bool,
    history: History,
    last_key_was_g: bool,
    list_height: usize,

    // --- ESTADO DO NOVO FORMULÁRIO ---
    all_groups: Vec<String>,
    filtered_groups: Vec<String>,
    form_group_state: ListState,
    
    form_is_edit: bool,
    form_original_path: String,
    form_active_field: usize, // 0: Grupo, 1: Titulo, 2: User, 3: Pass
    
    form_group: String,
    form_title: String,
    form_username: String,
    form_password: String,
}

impl App {
    fn new(db_path: String, password: String, is_mac: bool, history: History) -> Self {
        let mut app = Self {
            db_path, password,
            entries: vec![], filtered: vec![], search_query: String::new(), list_state: ListState::default(),
            mode: AppMode::Search, message: None, is_mac, history, last_key_was_g: false, list_height: 10,
            
            all_groups: vec![], filtered_groups: vec![], form_group_state: ListState::default(),
            form_is_edit: false, form_original_path: String::new(), form_active_field: 0,
            form_group: String::new(), form_title: String::new(), form_username: String::new(), form_password: String::new(),
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
                
                self.entries.clear();
                self.all_groups.clear();

                for line in out_str.lines().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if line.ends_with('/') {
                        self.all_groups.push(line.trim_end_matches('/').to_string());
                    } else {
                        self.entries.push(line.to_string());
                    }
                }
                
                self.history.sort_items(&mut self.entries);
            }
        }
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        if q.is_empty() { self.filtered = self.entries.clone(); } 
        else { self.filtered = self.entries.iter().filter(|e| e.to_lowercase().contains(&q)).cloned().collect(); }
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    // --- MÉTODOS DO FORMULÁRIO ---
    fn open_add_form(&mut self) {
        self.form_is_edit = false;
        self.form_group.clear(); self.form_title.clear(); self.form_username.clear(); self.form_password.clear();
        self.form_active_field = 0;
        self.mode = AppMode::Form;
        self.filter_form_groups();
    }

    fn open_edit_form(&mut self, entry: String) {
        self.form_is_edit = true;
        self.form_original_path = entry.clone();
        
        // Separa Grupo e Titulo visualmente para o modal
        if let Some(idx) = entry.rfind('/') {
            self.form_group = entry[..idx].to_string();
            self.form_title = entry[idx+1..].to_string();
        } else {
            self.form_group = String::new();
            self.form_title = entry.clone();
        }

        self.form_username = self.fetch_field(&entry, "UserName");
        self.form_password = self.fetch_field(&entry, "Password");

        self.form_active_field = 3; // Foca na senha, que é o que geralmente se edita
        self.mode = AppMode::Form;
        self.filter_form_groups();
    }

    fn fetch_field(&self, path: &str, field: &str) -> String {
        let mut cmd = Command::new("keepassxc-cli");
        cmd.args(["show", "-q", &self.db_path, path, "-a", field]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n", self.password).as_bytes()); }
            if let Ok(out) = child.wait_with_output() { return String::from_utf8_lossy(&out.stdout).trim().to_string(); }
        }
        String::new()
    }

    fn filter_form_groups(&mut self) {
        let q = self.form_group.to_lowercase();
        self.filtered_groups = self.all_groups.iter().filter(|g| g.to_lowercase().contains(&q)).cloned().collect();
        self.form_group_state.select(if self.filtered_groups.is_empty() { None } else { Some(0) });
    }

    fn form_next_group(&mut self) {
        if self.filtered_groups.is_empty() { return; }
        let i = match self.form_group_state.selected() { Some(i) => if i >= self.filtered_groups.len() - 1 { 0 } else { i + 1 }, None => 0 };
        self.form_group_state.select(Some(i));
    }
    
    fn form_prev_group(&mut self) {
        if self.filtered_groups.is_empty() { return; }
        let i = match self.form_group_state.selected() { Some(i) => if i == 0 { self.filtered_groups.len() - 1 } else { i - 1 }, None => 0 };
        self.form_group_state.select(Some(i));
    }

    fn submit_form(&mut self) {
        let path = if self.form_group.trim().is_empty() {
            self.form_title.trim().to_string()
        } else {
            format!("{}/{}", self.form_group.trim().trim_end_matches('/'), self.form_title.trim())
        };

        if path.is_empty() {
            self.set_msg("O Título não pode ser vazio!", true);
            return;
        }

        if self.form_is_edit {
            // Se o caminho (Grupo/Titulo) mudou, precisamos usar o comando 'mv'
            if path != self.form_original_path {
                let mut cmd_mv = Command::new("keepassxc-cli");
                cmd_mv.args(["mv", "-q", &self.db_path, &self.form_original_path, &path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
                if let Ok(mut child) = cmd_mv.spawn() {
                    if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n", self.password).as_bytes()); }
                    let _ = child.wait();
                }
            }

            // Atualiza Usuario e Senha
            let mut cmd_edit = Command::new("keepassxc-cli");
            cmd_edit.args(["edit", "-q", "-p", "-u", &self.form_username, &self.db_path, &path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = cmd_edit.spawn() {
                if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n{}\n{}\n", self.password, self.form_password, self.form_password).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) { self.set_msg("Entrada editada com sucesso!", false); } 
                else { self.set_msg("Erro ao editar.", true); }
            }
        } else {
            // Adição Padrão
            let mut cmd_add = Command::new("keepassxc-cli");
            cmd_add.args(["add", "-q", "-p", "-u", &self.form_username, &self.db_path, &path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = cmd_add.spawn() {
                if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(format!("{}\n{}\n{}\n", self.password, self.form_password, self.form_password).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    self.history.record_use(&path);
                    self.set_msg("Entrada adicionada!", false);
                } else { self.set_msg("Erro ao adicionar.", true); }
            }
        }
        
        self.refresh_entries();
        self.mode = AppMode::Normal;
    }

    // --- NAVEGAÇÃO DA LISTA PRINCIPAL ---
    fn next(&mut self) {
        if self.filtered.is_empty() { return; }
        let i = match self.list_state.selected() { Some(i) => if i >= self.filtered.len() - 1 { 0 } else { i + 1 }, None => 0 };
        self.list_state.select(Some(i));
    }
    fn previous(&mut self) {
        if self.filtered.is_empty() { return; }
        let i = match self.list_state.selected() { Some(i) => if i == 0 { self.filtered.len() - 1 } else { i - 1 }, None => 0 };
        self.list_state.select(Some(i));
    }
    fn go_to_top(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(0)); } }
    fn go_to_bottom(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); } }
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

    fn get_selected(&self) -> Option<String> { self.list_state.selected().map(|i| self.filtered[i].clone()) }
    fn set_msg(&mut self, msg: &str, is_error: bool) { self.message = Some((msg.to_string(), Instant::now(), is_error)); }

    fn copy_password(&mut self) {
        if let Some(entry) = self.get_selected() {
            self.history.record_use(&entry);
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
// BOOTSTRAP E LOOPS
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
    let mut history = History::new();
    let mut dbs = find_databases();
    history.sort_items(&mut dbs);

    let db_path = if dbs.is_empty() {
        println!("Nenhum arquivo .kdbx encontrado.");
        std::process::exit(1);
    } else if dbs.len() == 1 {
        dbs[0].clone()
    } else {
        run_selection_tui(dbs)?
    };

    history.record_use(&db_path);

    print!("[KeePassXC] Senha para '{}': ", db_path);
    io::stdout().flush()?;
    let password = rpassword::read_password()?;

    let mut test_cmd = Command::new("keepassxc-cli").args(["ls", "-q", &db_path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    if let Some(mut stdin) = test_cmd.stdin.take() { let _ = stdin.write_all(format!("{}\n", password).as_bytes()); }
    if !test_cmd.wait()?.success() {
        println!("Senha incorreta ou erro ao acessar o banco.");
        std::process::exit(1);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db_path, password, is_mac, history);
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

fn run_selection_tui(dbs: Vec<String>) -> Result<String, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = DbApp::new(dbs);

    let selected = loop {
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
                        KeyCode::Enter => { if let Some(i) = app.list_state.selected() { break app.filtered[i].clone(); } },
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
                        KeyCode::Char('g') => { is_g_key = true; if app.last_key_was_g { app.go_to_top(); is_g_key = false; } }
                        KeyCode::Enter => { if let Some(i) = app.list_state.selected() { break app.filtered[i].clone(); } },
                        _ => {}
                    },
                    _ => {}
                }
                app.last_key_was_g = is_g_key;
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(selected)
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut is_g_key = false;

                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if app.mode == AppMode::Search || app.mode == AppMode::Normal {
                        match key.code {
                            KeyCode::Char('d') => app.half_page_down(),
                            KeyCode::Char('u') => app.half_page_up(),
                            KeyCode::Char('a') => { app.open_add_form(); }
                            KeyCode::Char('e') => { if let Some(entry) = app.get_selected() { app.open_edit_form(entry); } }
                            KeyCode::Char('x') => { if app.get_selected().is_some() { app.mode = AppMode::ConfirmDelete; } }
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
                        KeyCode::Char('g') => { is_g_key = true; if app.last_key_was_g { app.go_to_top(); is_g_key = false; } }
                        _ => {}
                    },
                    AppMode::ConfirmDelete => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => { app.delete_selected(); app.mode = AppMode::Normal; }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.mode = AppMode::Normal,
                        _ => {}
                    },
                    AppMode::Form => match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::BackTab => { app.form_active_field = if app.form_active_field == 0 { 3 } else { app.form_active_field - 1 }; }
                        KeyCode::Tab => { app.form_active_field = (app.form_active_field + 1) % 4; }
                        KeyCode::Down => { if app.form_active_field == 0 { app.form_next_group(); } }
                        KeyCode::Up => { if app.form_active_field == 0 { app.form_prev_group(); } }
                        KeyCode::Enter => {
                            if app.form_active_field == 0 && app.form_group_state.selected().is_some() {
                                app.form_group = app.filtered_groups[app.form_group_state.selected().unwrap()].clone();
                                app.form_active_field = 1; // Pula para Titulo apos preencher grupo
                            } else {
                                app.submit_form(); // Enter em qualquer outro campo salva tudo
                            }
                        },
                        KeyCode::Backspace => {
                            match app.form_active_field {
                                0 => { app.form_group.pop(); app.filter_form_groups(); }
                                1 => { app.form_title.pop(); }
                                2 => { app.form_username.pop(); }
                                3 => { app.form_password.pop(); }
                                _ => {}
                            }
                        },
                        KeyCode::Char(c) => {
                            match app.form_active_field {
                                0 => { app.form_group.push(c); app.filter_form_groups(); }
                                1 => { app.form_title.push(c); }
                                2 => { app.form_username.push(c); }
                                3 => { app.form_password.push(c); }
                                _ => {}
                            }
                        },
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
    app.list_height = chunks[1].height as usize;

    let (search_text, search_color) = if app.mode == AppMode::Search { (format!(" {}█ ", app.search_query), Color::Yellow) } else { (format!(" {} ", app.search_query), Color::DarkGray) };
    f.render_widget(Paragraph::new(search_text).block(Block::default().title(" Pesquisar (/) ").borders(Borders::ALL).style(Style::default().fg(search_color))), chunks[0]);

    let list_title = if app.mode == AppMode::Normal { " NORMAL (j/k) " } else { " PESQUISA " };
    let list_color = if app.mode == AppMode::Normal { Color::Cyan } else { Color::White };
    let items: Vec<ListItem> = app.filtered.iter().map(|e| ListItem::new(e.as_str())).collect();
    let list = List::new(items).block(Block::default().title(list_title).borders(Borders::ALL).style(Style::default().fg(list_color))).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol(">> ");
    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    f.render_widget(Paragraph::new("gg/G: Topo/Fim | CTRL-U/D: Meia Pág | ENTER: Copiar | CTRL-A/E/X: Ações").block(Block::default().borders(Borders::ALL)).alignment(Alignment::Center), chunks[2]);

    if app.mode == AppMode::ConfirmDelete {
        let area = centered_rect(50, 15, f.size());
        f.render_widget(Clear, area);
        f.render_widget(Paragraph::new(format!("Deseja EXCLUIR '{}'? [y/N]", app.get_selected().unwrap_or_default())).block(Block::default().title(" Confirmar ").borders(Borders::ALL).style(Style::default().fg(Color::Red))).alignment(Alignment::Center), area);
    } 
    else if app.mode == AppMode::Form {
        let area = centered_rect(70, 75, f.size());
        f.render_widget(Clear, area);
        
        let form_block = Block::default()
            .title(if app.form_is_edit { " Editar Entrada " } else { " Nova Entrada " })
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Green));
        f.render_widget(form_block.clone(), area);
        
        let inner_area = form_block.inner(area);
        
        // Layout: Grupo + Lista(Dropdown) (0 e 1), Título(2), Usuario(3), Senha(4)
        let form_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Input Grupo
                Constraint::Length(if app.form_active_field == 0 && !app.filtered_groups.is_empty() { 6 } else { 0 }),
                Constraint::Length(3), // Titulo
                Constraint::Length(3), // Usuario
                Constraint::Length(3), // Senha
                Constraint::Min(1),    // Help
            ]).split(inner_area);

        // --- BLOCO DO GRUPO (CONTÊINER ÚNICO) ---
        let group_rect = form_chunks[0].union(form_chunks[1]); 
        let group_block = Block::default()
            .title(" Grupo ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if app.form_active_field == 0 { Color::Yellow } else { Color::White }));
        
        f.render_widget(group_block, group_rect);

        // Renderiza o input do grupo logo abaixo do título
        let input_rect = Rect::new(group_rect.x + 1, group_rect.y + 1, group_rect.width - 2, 1);
        f.render_widget(Paragraph::new(format!(" {}{}", app.form_group, if app.form_active_field == 0 { "█" } else { "" })), input_rect);

        // Renderiza a lista COM UMA LINHA DIVISÓRIA
        if app.form_active_field == 0 && !app.filtered_groups.is_empty() {
            let items: Vec<ListItem> = app.filtered_groups.iter().map(|g| ListItem::new(g.as_str())).collect();
            
            // Sincroniza a cor da divisória com a cor da borda do painel
            let divider_color = if app.form_active_field == 0 { Color::Yellow } else { Color::White };
            
            let list = List::new(items)
                .block(Block::default()
                    .borders(Borders::TOP) 
                    .border_style(Style::default().fg(divider_color))) // <-- Corrigido aqui
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            
            let list_rect = Rect::new(group_rect.x + 1, group_rect.y + 2, group_rect.width - 2, group_rect.height - 3);
            f.render_stateful_widget(list, list_rect, &mut app.form_group_state);
        }

        // Título
        let title_color = if app.form_active_field == 1 { Color::Yellow } else { Color::White };
        f.render_widget(Paragraph::new(format!(" {}{}", app.form_title, if app.form_active_field == 1 { "█" } else { "" }))
            .block(Block::default().title(" Título ").borders(Borders::ALL).style(Style::default().fg(title_color))), form_chunks[2]);

        // Usuário
        let user_color = if app.form_active_field == 2 { Color::Yellow } else { Color::White };
        f.render_widget(Paragraph::new(format!(" {}{}", app.form_username, if app.form_active_field == 2 { "█" } else { "" }))
            .block(Block::default().title(" Usuário ").borders(Borders::ALL).style(Style::default().fg(user_color))), form_chunks[3]);

        // Senha
        let pass_color = if app.form_active_field == 3 { Color::Yellow } else { Color::White };
        let hidden: String = app.form_password.chars().map(|_| '*').collect();
        f.render_widget(Paragraph::new(format!(" {}{}", hidden, if app.form_active_field == 3 { "█" } else { "" }))
            .block(Block::default().title(" Senha ").borders(Borders::ALL).style(Style::default().fg(pass_color))), form_chunks[4]);

        f.render_widget(Paragraph::new("TAB: Navegar | ENTER: Confirmar | SETAS: Lista").alignment(Alignment::Center).style(Style::default().fg(Color::DarkGray)), form_chunks[5]);
    }

    if let Some((msg, time, is_error)) = &app.message {
        if time.elapsed() < Duration::from_secs(2) {
            let area = centered_rect(40, 10, f.size());
            f.render_widget(Clear, area);
            f.render_widget(Paragraph::new(msg.as_str()).block(Block::default().borders(Borders::ALL).style(Style::default().fg(if *is_error { Color::Red } else { Color::Green }))).alignment(Alignment::Center), area);
        } else { app.message = None; }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)]).split(popup_layout[1])[1]
}
