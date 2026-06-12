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
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(PartialEq, Clone, Copy)]
enum AppMode { Search, Normal, ConfirmDelete, Form }

// ==========================================
// SISTEMA DE CONFIGURAÇÃO E TEMAS (TOML)
// ==========================================

#[derive(Deserialize, Default)]
struct RawConfig {
    general: Option<RawGeneral>,
}

#[derive(Deserialize)]
struct RawGeneral {
    path: Option<String>,
    recency: Option<bool>,
    theme: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawThemeFile {
    colors: Option<RawThemeColors>,
}

#[derive(Deserialize)]
struct RawThemeColors {
    #[serde(alias = "AlertInfo")] alert_info: Option<String>,
    #[serde(alias = "AlertWarn")] alert_warn: Option<String>,
    #[serde(alias = "AlertError")] alert_error: Option<String>,
    #[serde(alias = "Annotation")] annotation: Option<String>,
    #[serde(alias = "Base")] base: Option<String>,
    #[serde(alias = "Guidance")] guidance: Option<String>,
    #[serde(alias = "Important")] important: Option<String>,
    #[serde(alias = "Title")] title: Option<String>,
}

#[derive(Clone)]
struct AppConfig {
    search_path: String,
    recency_enabled: bool,
    theme_name: String,
}

#[derive(Clone)]
struct Theme {
    alert_info: Color,
    alert_warn: Color,
    alert_error: Color,
    annotation: Color,
    base: Color,
    guidance: Color,
    important: Color,
    title: Color,
}

impl Theme {
    fn default() -> Self {
        Self {
            alert_info: Color::Green,
            alert_warn: Color::Yellow,
            alert_error: Color::Red,
            annotation: Color::Yellow,
            base: Color::White,
            guidance: Color::DarkGray,
            important: Color::Red,
            title: Color::Cyan,
        }
    }
}

fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

fn setup_and_load_config() -> (AppConfig, Theme) {
    let home = std::env::var("HOME").unwrap_or_default();
    let config_dir = PathBuf::from(format!("{}/.config/fpass", home));
    let themes_dir = config_dir.join("themes");
    fs::create_dir_all(&themes_dir).ok();

    let mut config = AppConfig {
        search_path: home.clone(),
        recency_enabled: true,
        theme_name: "default".to_string(),
    };

    let config_path = config_dir.join("config.toml");
    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(raw) = toml::from_str::<RawConfig>(&content) {
            if let Some(general_config) = raw.general { // Alterado aqui
                if let Some(p) = general_config.path {  // Alterado aqui
                    config.search_path = if p.starts_with("~/") { 
                        p.replacen("~/", &format!("{}/", home), 1) 
                    } else { p };
                }
                if let Some(r) = general_config.recency { config.recency_enabled = r; }
                if let Some(t) = general_config.theme { config.theme_name = t; }
            }
        }
    }

    let mut theme = Theme::default();
    if config.theme_name != "default" {
        let theme_path = themes_dir.join(format!("{}.toml", config.theme_name));
        if let Ok(content) = fs::read_to_string(&theme_path) {
            if let Ok(raw_theme) = toml::from_str::<RawThemeFile>(&content) {
                if let Some(colors) = raw_theme.colors {
                    if let Some(c) = colors.alert_info.and_then(|h| hex_to_color(&h)) { theme.alert_info = c; }
                    if let Some(c) = colors.alert_warn.and_then(|h| hex_to_color(&h)) { theme.alert_warn = c; }
                    if let Some(c) = colors.alert_error.and_then(|h| hex_to_color(&h)) { theme.alert_error = c; }
                    if let Some(c) = colors.annotation.and_then(|h| hex_to_color(&h)) { theme.annotation = c; }
                    if let Some(c) = colors.base.and_then(|h| hex_to_color(&h)) { theme.base = c; }
                    if let Some(c) = colors.guidance.and_then(|h| hex_to_color(&h)) { theme.guidance = c; }
                    if let Some(c) = colors.important.and_then(|h| hex_to_color(&h)) { theme.important = c; }
                    if let Some(c) = colors.title.and_then(|h| hex_to_color(&h)) { theme.title = c; }
                }
            }
        }
    }

    (config, theme)
}

// ==========================================
// MOTOR DE FRECENCY
// ==========================================

struct History {
    records: HashMap<String, (u32, u64)>, 
    file_path: PathBuf,
    enabled: bool,
}

impl History {
    fn new(enabled: bool) -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let file_path = PathBuf::from(format!("{}/.config/fpass/history", home));
        let mut records = HashMap::new();

        if enabled {
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
        }
        Self { records, file_path, enabled }
    }

    fn hash_item(item: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(item.as_bytes());
        hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn record_use(&mut self, item: &str) {
        if !self.enabled { return; }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let hashed_item = Self::hash_item(item);
        let entry = self.records.entry(hashed_item).or_insert((0, 0));
        entry.0 += 1; entry.1 = now;
        self.save();
    }

    fn save(&self) {
        if !self.enabled { return; }
        if let Ok(mut file) = OpenOptions::new().write(true).create(true).truncate(true).open(&self.file_path) {
            for (hash, (count, ts)) in &self.records { let _ = writeln!(file, "{}|{}|{}", hash, count, ts); }
        }
    }

    fn get_score(&self, item: &str) -> u64 {
        if !self.enabled { return 0; }
        let hashed_item = Self::hash_item(item);
        if let Some(&(count, ts)) = self.records.get(&hashed_item) {
            let age = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs().saturating_sub(ts);
            let weight = if age < 86400 { 100 } else if age < 604800 { 50 } else if age < 2592000 { 20 } else { 5 };
            (count as u64) * weight
        } else { 0 }
    }

    fn sort_items(&self, items: &mut Vec<String>) {
        if self.enabled {
            items.sort_by(|a, b| {
                let score_a = self.get_score(a);
                let score_b = self.get_score(b);
                score_b.cmp(&score_a).then_with(|| a.cmp(b))
            });
        }
    }
}

// ==========================================
// SELETOR DE BANCO DE DADOS
// ==========================================

struct DbApp {
    entries: Vec<String>, filtered: Vec<String>, search_query: String,
    list_state: ListState, mode: AppMode, last_key_was_g: bool, list_height: usize,
    theme: Theme,
}

impl DbApp {
    fn new(dbs: Vec<String>, theme: Theme) -> Self {
        let mut app = Self { entries: dbs.clone(), filtered: dbs, search_query: String::new(), list_state: ListState::default(), mode: AppMode::Search, last_key_was_g: false, list_height: 10, theme };
        if !app.filtered.is_empty() { app.list_state.select(Some(0)); }
        app
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        // Convertemos para String aqui, tornando os termos independentes
        let terms: Vec<String> = q.split_whitespace().map(|s| s.to_string()).collect();
        
        if terms.is_empty() { 
            self.filtered = self.entries.clone(); 
        } else { 
            self.filtered = self.entries.iter().filter(|e| {
                let lower = e.to_lowercase();
                // Agora 't' é uma String, que vive o tempo necessário
                terms.iter().all(|t| lower.contains(t)) 
            }).cloned().collect(); 
        }
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    fn next(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i >= self.filtered.len() - 1 { 0 } else { i + 1 }, None => 0 }; self.list_state.select(Some(i)); }
    fn previous(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i == 0 { self.filtered.len() - 1 } else { i - 1 }, None => 0 }; self.list_state.select(Some(i)); }
    fn go_to_top(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(0)); } }
    fn go_to_bottom(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); } }
    fn half_page_down(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some((i + step).min(self.filtered.len() - 1))); }
    fn half_page_up(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some(i.saturating_sub(step))); }
}

// ==========================================
// GERENCIADOR DE SENHAS
// ==========================================

struct App {
    db_path: String, password: String, entries: Vec<String>, filtered: Vec<String>, search_query: String,
    list_state: ListState, mode: AppMode, message: Option<(String, Instant, bool)>, is_mac: bool,
    history: History, last_key_was_g: bool, list_height: usize, theme: Theme,
    all_groups: Vec<String>, filtered_groups: Vec<String>, form_group_state: ListState,
    form_is_edit: bool, form_original_path: String, form_active_field: usize,
    form_group: String, form_title: String, form_username: String, form_password: String, form_url: String,
}

impl App {
    fn new(db_path: String, password: String, is_mac: bool, history: History, theme: Theme) -> Self {
        let mut app = Self {
            db_path, password, entries: vec![], filtered: vec![], search_query: String::new(), list_state: ListState::default(),
            mode: AppMode::Search, message: None, is_mac, history, last_key_was_g: false, list_height: 10, theme,
            all_groups: vec![], filtered_groups: vec![], form_group_state: ListState::default(),
            form_is_edit: false, form_original_path: String::new(), form_active_field: 0,
            form_group: String::new(), form_title: String::new(), form_username: String::new(), form_password: String::new(), form_url: String::new(),
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
                self.entries.clear(); self.all_groups.clear();
                for line in String::from_utf8_lossy(&output.stdout).lines().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if line.ends_with('/') { self.all_groups.push(line.trim_end_matches('/').to_string()); } 
                    else { self.entries.push(line.to_string()); }
                }
                self.history.sort_items(&mut self.entries);
            }
        }
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        // Convertemos para String aqui, tornando os termos independentes
        let terms: Vec<String> = q.split_whitespace().map(|s| s.to_string()).collect();
        
        if terms.is_empty() { 
            self.filtered = self.entries.clone(); 
        } else { 
            self.filtered = self.entries.iter().filter(|e| {
                let lower = e.to_lowercase();
                // Agora 't' é uma String, que vive o tempo necessário
                terms.iter().all(|t| lower.contains(t)) 
            }).cloned().collect(); 
        }
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    fn open_add_form(&mut self) {
        self.form_is_edit = false; self.form_group.clear(); self.form_title.clear(); self.form_username.clear(); 
        self.form_password.clear(); self.form_url.clear(); self.form_active_field = 0; self.mode = AppMode::Form;
        self.filter_form_groups();
    }

    fn open_edit_form(&mut self, entry: String) {
        self.form_is_edit = true; self.form_original_path = entry.clone();
        if let Some(idx) = entry.rfind('/') { self.form_group = entry[..idx].to_string(); self.form_title = entry[idx+1..].to_string(); } 
        else { self.form_group = String::new(); self.form_title = entry.clone(); }
        self.form_username = self.fetch_field(&entry, "UserName"); self.form_password = self.fetch_field(&entry, "Password"); self.form_url = self.fetch_field(&entry, "URL");
        self.form_active_field = 3; self.mode = AppMode::Form; self.filter_form_groups();
    }

    fn fetch_field(&self, path: &str, field: &str) -> String {
        let mut cmd = Command::new("keepassxc-cli"); cmd.args(["show", "-q", &self.db_path, path, "-a", field]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        if let Ok(mut child) = cmd.spawn() { if let Some(mut s) = child.stdin.take() { let _ = s.write_all(format!("{}\n", self.password).as_bytes()); }
            if let Ok(out) = child.wait_with_output() { return String::from_utf8_lossy(&out.stdout).trim().to_string(); } }
        String::new()
    }

    fn filter_form_groups(&mut self) {
        let q = self.form_group.to_lowercase();
        self.filtered_groups = self.all_groups.iter().filter(|g| g.to_lowercase().contains(&q)).cloned().collect();
        self.form_group_state.select(if self.filtered_groups.is_empty() { None } else { Some(0) });
    }

    fn form_next_group(&mut self) { if self.filtered_groups.is_empty() { return; } let i = match self.form_group_state.selected() { Some(i) => if i >= self.filtered_groups.len() - 1 { 0 } else { i + 1 }, None => 0 }; self.form_group_state.select(Some(i)); }
    fn form_prev_group(&mut self) { if self.filtered_groups.is_empty() { return; } let i = match self.form_group_state.selected() { Some(i) => if i == 0 { self.filtered_groups.len() - 1 } else { i - 1 }, None => 0 }; self.form_group_state.select(Some(i)); }

    fn submit_form(&mut self) {
        let path = if self.form_group.trim().is_empty() { self.form_title.trim().to_string() } else { format!("{}/{}", self.form_group.trim().trim_end_matches('/'), self.form_title.trim()) };
        if path.is_empty() { self.set_msg("O Título não pode ser vazio!", true); return; }

        if self.form_is_edit {
            if path != self.form_original_path {
                let mut cmd_mv = Command::new("keepassxc-cli"); cmd_mv.args(["mv", "-q", &self.db_path, &self.form_original_path, &path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
                if let Ok(mut child) = cmd_mv.spawn() { if let Some(mut s) = child.stdin.take() { let _ = s.write_all(format!("{}\n", self.password).as_bytes()); } let _ = child.wait(); }
            }
            let mut cmd_edit = Command::new("keepassxc-cli"); cmd_edit.args(["edit", "-q", "-p", "-u", &self.form_username, "--url", &self.form_url, &self.db_path, &path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = cmd_edit.spawn() {
                if let Some(mut s) = child.stdin.take() { let _ = s.write_all(format!("{}\n{}\n{}\n", self.password, self.form_password, self.form_password).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) { self.set_msg("Entrada editada com sucesso!", false); } else { self.set_msg("Erro ao editar.", true); }
            }
        } else {
            let mut cmd_add = Command::new("keepassxc-cli"); cmd_add.args(["add", "-q", "-p", "-u", &self.form_username, "--url", &self.form_url, &self.db_path, &path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = cmd_add.spawn() {
                if let Some(mut s) = child.stdin.take() { let _ = s.write_all(format!("{}\n{}\n{}\n", self.password, self.form_password, self.form_password).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) { self.history.record_use(&path); self.set_msg("Entrada adicionada!", false); } else { self.set_msg("Erro ao adicionar.", true); }
            }
        }
        self.refresh_entries(); self.mode = AppMode::Normal;
    }

    fn next(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i >= self.filtered.len() - 1 { 0 } else { i + 1 }, None => 0 }; self.list_state.select(Some(i)); }
    fn previous(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i == 0 { self.filtered.len() - 1 } else { i - 1 }, None => 0 }; self.list_state.select(Some(i)); }
    fn go_to_top(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(0)); } }
    fn go_to_bottom(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); } }
    fn half_page_down(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some((i + step).min(self.filtered.len() - 1))); }
    fn half_page_up(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some(i.saturating_sub(step))); }
    fn get_selected(&self) -> Option<String> { self.list_state.selected().map(|i| self.filtered[i].clone()) }
    fn set_msg(&mut self, msg: &str, is_error: bool) { self.message = Some((msg.to_string(), Instant::now(), is_error)); }

    fn copy_password(&mut self) {
        if let Some(entry) = self.get_selected() {
            self.history.record_use(&entry);
            let mut cmd = Command::new("keepassxc-cli"); cmd.args(["show", "-q", &self.db_path, &entry, "-a", "Password"]).stdin(Stdio::piped()).stdout(Stdio::piped());
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut s) = child.stdin.take() { let _ = s.write_all(format!("{}\n", self.password).as_bytes()); }
                if let Ok(output) = child.wait_with_output() {
                    let entry_pass = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let cmd_name = if self.is_mac { "pbcopy" } else { "wl-copy" };
                    if let Ok(mut copy_child) = Command::new(cmd_name).stdin(Stdio::piped()).spawn() {
                        if let Some(mut s) = copy_child.stdin.take() { let _ = s.write_all(entry_pass.as_bytes()); }
                        if copy_child.wait().is_ok() {
                            self.set_msg(&format!("Copiado: {}\n(Limpo do clipboard em 10s)", entry), false);
                            spawn_clipboard_clearer(entry_pass, self.is_mac); return;
                        }
                    }
                }
            }
            self.set_msg("Erro ao copiar senha.", true);
        }
    }
    
    fn delete_selected(&mut self) {
        if let Some(entry) = self.get_selected() {
            let mut cmd = Command::new("keepassxc-cli"); cmd.args(["rm", "-q", &self.db_path, &entry]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut s) = child.stdin.take() { let _ = s.write_all(format!("{}\n", self.password).as_bytes()); }
                if child.wait().map(|s| s.success()).unwrap_or(false) { self.set_msg("Entrada excluída!", false); self.refresh_entries(); self.previous(); } 
                else { self.set_msg("Erro ao excluir.", true); }
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
        if let Ok(mut child) = Command::new(cmd_name).stdin(Stdio::piped()).spawn() { if let Some(mut s) = child.stdin.take() { let _ = s.write_all(b""); } let _ = child.wait(); }
        if !is_mac { let _ = Command::new("cliphist").args(["delete-query", &password]).status(); }
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Verifica se o sistema é macOS
    let is_mac = std::env::consts::OS == "macos";

    // Cria config default
    let home = std::env::var("HOME").unwrap_or_default();
    let config_dir = PathBuf::from(format!("{}/.config/fpass", home));
    fs::create_dir_all(&config_dir).ok();
    ensure_config_exists(&config_dir);
    
    // Carrega as Configurações e Temas de ~/.config/fpass/
    let (config, theme) = setup_and_load_config();
    let mut history = History::new(config.recency_enabled);
    
    let mut dbs = find_databases(&config.search_path);
    history.sort_items(&mut dbs);

    let db_path = if dbs.is_empty() {
        println!("Nenhum arquivo .kdbx encontrado.");
        std::process::exit(1);
    } else if dbs.len() == 1 {
        dbs[0].clone()
    } else {
        match run_selection_tui(dbs, theme.clone())? {
            Some(path) => path,
            None => std::process::exit(0), // Sai do programa tranquilamente se der ESC/q
        }
    };

    history.record_use(&db_path);

    print!("[KeePassXC] Senha para '{}': ", db_path); io::stdout().flush()?;
    let password = rpassword::read_password()?;

    let mut test_cmd = Command::new("keepassxc-cli").args(["ls", "-q", &db_path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    if let Some(mut s) = test_cmd.stdin.take() { let _ = s.write_all(format!("{}\n", password).as_bytes()); }
    if !test_cmd.wait()?.success() { println!("Senha incorreta ou erro."); std::process::exit(1); }

    enable_raw_mode()?; let mut stdout = io::stdout(); execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout); let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db_path, password, is_mac, history, theme);
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?; execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?; terminal.show_cursor()?;
    if let Err(err) = res { println!("{:?}", err) } Ok(())
}

fn find_databases(path: &str) -> Vec<String> {
    if let Ok(output) = Command::new("fd").args([".kdbx$", path]).output() {
        return String::from_utf8_lossy(&output.stdout).lines().map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from).collect();
    } vec![]
}

fn run_selection_tui(dbs: Vec<String>, theme: Theme) -> Result<Option<String>, Box<dyn std::error::Error>> {
    enable_raw_mode()?; 
    let mut stdout = io::stdout(); 
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout); 
    let mut terminal = Terminal::new(backend)?;
    let mut app = DbApp::new(dbs, theme);

    let selected = loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
                .split(f.size());
                
            app.list_height = chunks[1].height as usize;

            let (search_text, search_color) = if app.mode == AppMode::Search { 
                (format!(" {}█ ", app.search_query), app.theme.annotation) 
            } else { 
                (format!(" {} ", app.search_query), app.theme.guidance) 
            };
            
            f.render_widget(
                Paragraph::new(search_text)
                    .block(Block::default().title(" Filtrar Banco (/) ").borders(Borders::ALL).style(Style::default().fg(search_color))), 
                chunks[0]
            );

            let list_color = if app.mode == AppMode::Normal { app.theme.title } else { app.theme.base };
            let items: Vec<ListItem> = app.filtered.iter().map(|e| ListItem::new(e.as_str())).collect();
            let list = List::new(items)
                .block(Block::default().title(" Bancos Disponíveis ").borders(Borders::ALL).style(Style::default().fg(list_color)))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol(">> ");
                
            f.render_stateful_widget(list, chunks[1], &mut app.list_state);

            let footer_text = if app.mode == AppMode::Search { 
                "CTRL-U/D: Meia Pág | ENTER: Selecionar | ESC: Modo Normal | CTRL+C: Sair" 
            } else { 
                "gg/G: Topo/Fim | CTRL-U/D: Meia Pág | / ou f: Pesquisar | ENTER: Selecionar | ESC/q: Sair" 
            };
            
            f.render_widget(
                Paragraph::new(footer_text)
                    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(app.theme.guidance)))
                    .alignment(Alignment::Center), 
                chunks[2]
            );
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut is_g_key = false;
                
                // Tratamento de atalhos com CONTROL
                if key.modifiers.contains(KeyModifiers::CONTROL) { 
                    match key.code { 
                        KeyCode::Char('d') => app.half_page_down(), 
                        KeyCode::Char('u') => app.half_page_up(), 
                        KeyCode::Char('c') => break None, // Retorna None e sai do loop pacificamente
                        _ => {} 
                    } 
                    continue; 
                }
                
                // Tratamento de atalhos por modo
                match app.mode {
                    AppMode::Search => match key.code { 
                        KeyCode::Esc => app.mode = AppMode::Normal, 
                        KeyCode::Down => app.next(), 
                        KeyCode::Up => app.previous(), 
                        KeyCode::Enter => { 
                            if let Some(i) = app.list_state.selected() { 
                                break Some(app.filtered[i].clone()); 
                            } 
                        }, 
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
                        KeyCode::Char('q') | KeyCode::Esc => break None, // Retorna None e sai do loop pacificamente
                        KeyCode::Down | KeyCode::Char('j') => app.next(), 
                        KeyCode::Up | KeyCode::Char('k') => app.previous(), 
                        KeyCode::Char('/') | KeyCode::Char('f') => app.mode = AppMode::Search, 
                        KeyCode::Char('G') => app.go_to_bottom(), 
                        KeyCode::Char('g') => { 
                            is_g_key = true; 
                            if app.last_key_was_g { 
                                app.go_to_top(); 
                                is_g_key = false; 
                            } 
                        } 
                        KeyCode::Enter => { 
                            if let Some(i) = app.list_state.selected() { 
                                break Some(app.filtered[i].clone()); 
                            } 
                        }, 
                        _ => {} 
                    },
                    _ => {}
                }
                app.last_key_was_g = is_g_key;
            }
        }
    };
    
    // Limpeza da tela (agora será executada ao sair com q, esc ou ctrl+c)
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
                    if app.mode == AppMode::Search || app.mode == AppMode::Normal { match key.code { KeyCode::Char('d') => app.half_page_down(), KeyCode::Char('u') => app.half_page_up(), KeyCode::Char('a') => { app.open_add_form(); } KeyCode::Char('e') => { if let Some(entry) = app.get_selected() { app.open_edit_form(entry); } } KeyCode::Char('x') => { if app.get_selected().is_some() { app.mode = AppMode::ConfirmDelete; } } KeyCode::Char('c') => return Ok(()), _ => {} } continue; } else if key.code == KeyCode::Char('c') { return Ok(()); }
                }
                match app.mode {
                    AppMode::Search => match key.code { KeyCode::Esc => app.mode = AppMode::Normal, KeyCode::Down => app.next(), KeyCode::Up => app.previous(), KeyCode::Enter => app.copy_password(), KeyCode::Backspace => { app.search_query.pop(); app.apply_filter(); } KeyCode::Char(c) => { app.search_query.push(c); app.apply_filter(); } _ => {} },
                    AppMode::Normal => match key.code { KeyCode::Char('q') | KeyCode::Esc => return Ok(()), KeyCode::Down | KeyCode::Char('j') => app.next(), KeyCode::Up | KeyCode::Char('k') => app.previous(), KeyCode::Enter => app.copy_password(), KeyCode::Char('/') | KeyCode::Char('f') => app.mode = AppMode::Search, KeyCode::Char('G') => app.go_to_bottom(), KeyCode::Char('g') => { is_g_key = true; if app.last_key_was_g { app.go_to_top(); is_g_key = false; } } _ => {} },
                    AppMode::ConfirmDelete => match key.code { KeyCode::Char('y') | KeyCode::Char('Y') => { app.delete_selected(); app.mode = AppMode::Normal; } KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Enter | KeyCode::Esc => app.mode = AppMode::Normal, _ => {} },
                    AppMode::Form => match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal, KeyCode::BackTab => { app.form_active_field = if app.form_active_field == 0 { 4 } else { app.form_active_field - 1 }; } KeyCode::Tab => { app.form_active_field = (app.form_active_field + 1) % 5; } KeyCode::Down => { if app.form_active_field == 0 { app.form_next_group(); } } KeyCode::Up => { if app.form_active_field == 0 { app.form_prev_group(); } }
                        KeyCode::Enter => { if app.form_active_field == 0 && app.form_group_state.selected().is_some() { app.form_group = app.filtered_groups[app.form_group_state.selected().unwrap()].clone(); app.form_active_field = 1; } else { app.submit_form(); } },
                        KeyCode::Backspace => { match app.form_active_field { 0 => { app.form_group.pop(); app.filter_form_groups(); } 1 => { app.form_title.pop(); } 2 => { app.form_username.pop(); } 3 => { app.form_password.pop(); } 4 => { app.form_url.pop(); } _ => {} } },
                        KeyCode::Char(c) => { match app.form_active_field { 0 => { app.form_group.push(c); app.filter_form_groups(); } 1 => { app.form_title.push(c); } 2 => { app.form_username.push(c); } 3 => { app.form_password.push(c); } 4 => { app.form_url.push(c); } _ => {} } }, _ => {}
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

    let (search_text, search_color) = if app.mode == AppMode::Search { (format!(" {}█ ", app.search_query), app.theme.annotation) } else { (format!(" {} ", app.search_query), app.theme.guidance) };
    f.render_widget(Paragraph::new(search_text).block(Block::default().title(" Pesquisar (/) ").borders(Borders::ALL).style(Style::default().fg(search_color))), chunks[0]);

    let list_title = if app.mode == AppMode::Normal { " NORMAL (j/k) " } else { " PESQUISA " };
    let list_color = if app.mode == AppMode::Normal { app.theme.title } else { app.theme.base };
    let items: Vec<ListItem> = app.filtered.iter().map(|e| ListItem::new(e.as_str())).collect();
    let list = List::new(items).block(Block::default().title(list_title).borders(Borders::ALL).style(Style::default().fg(list_color))).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol(">> ");
    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    let footer_text = if app.mode == AppMode::Search { "CTRL-U/D: Meia Pág | ENTER: Copiar | CTRL-A/E/X: Ações | CTRL+C: Sair" } else if app.mode == AppMode::Normal { "gg/G: Topo/Fim | CTRL-U/D: Meia Pág | ENTER: Copiar | CTRL-A/E/X: Ações | ESC/q: Sair" } else { "" };
    if !footer_text.is_empty() { f.render_widget(Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL).style(Style::default().fg(app.theme.guidance))).alignment(Alignment::Center), chunks[2]); }

    if app.mode == AppMode::ConfirmDelete {
        let area = centered_fixed_rect(60, 5, f.size());
        f.render_widget(Clear, area);
        f.render_widget(Paragraph::new(format!("\nDeseja EXCLUIR '{}'? [y/N]", app.get_selected().unwrap_or_default())).block(Block::default().title(" Confirmar ").borders(Borders::ALL).style(Style::default().fg(app.theme.important))).alignment(Alignment::Center), area);
    } 
    else if app.mode == AppMode::Form {
        let area = centered_rect(70, 80, f.size());
        f.render_widget(Clear, area);
        
        let form_block = Block::default().title(if app.form_is_edit { " Editar Entrada " } else { " Nova Entrada " }).borders(Borders::ALL).style(Style::default().fg(app.theme.alert_info));
        f.render_widget(form_block.clone(), area);
        
        let inner_area = form_block.inner(area);
        let show_dropdown = app.form_active_field == 0 && !app.filtered_groups.is_empty();
        
        let form_chunks = Layout::default().direction(Direction::Vertical).constraints([ Constraint::Length(3), Constraint::Length(if show_dropdown { 5 } else { 0 }), Constraint::Length(3), Constraint::Length(3), Constraint::Length(3), Constraint::Length(3), Constraint::Min(1) ]).split(inner_area);

        let group_rect = form_chunks[0].union(form_chunks[1]); 
        let group_block = Block::default().title(" Grupo ").borders(Borders::ALL).border_style(Style::default().fg(if app.form_active_field == 0 { app.theme.annotation } else { app.theme.base }));
        f.render_widget(group_block, group_rect);
        f.render_widget(Paragraph::new(format!(" {}{}", app.form_group, if app.form_active_field == 0 { "█" } else { "" })), Rect::new(group_rect.x + 1, group_rect.y + 1, group_rect.width - 2, 1));

        if show_dropdown {
            let items: Vec<ListItem> = app.filtered_groups.iter().map(|g| ListItem::new(g.as_str())).collect();
            let divider_color = if app.form_active_field == 0 { app.theme.annotation } else { app.theme.base };
            let list = List::new(items).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(divider_color))).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol("> ");
            f.render_stateful_widget(list, Rect::new(group_rect.x + 1, group_rect.y + 2, group_rect.width - 2, group_rect.height - 3), &mut app.form_group_state);
        }

        let title_color = if app.form_active_field == 1 { app.theme.annotation } else { app.theme.base };
        f.render_widget(Paragraph::new(format!(" {}{}", app.form_title, if app.form_active_field == 1 { "█" } else { "" })).block(Block::default().title(" Título ").borders(Borders::ALL).style(Style::default().fg(title_color))), form_chunks[2]);
        let user_color = if app.form_active_field == 2 { app.theme.annotation } else { app.theme.base };
        f.render_widget(Paragraph::new(format!(" {}{}", app.form_username, if app.form_active_field == 2 { "█" } else { "" })).block(Block::default().title(" Usuário ").borders(Borders::ALL).style(Style::default().fg(user_color))), form_chunks[3]);
        let pass_color = if app.form_active_field == 3 { app.theme.annotation } else { app.theme.base };
        let hidden: String = app.form_password.chars().map(|_| '*').collect();
        f.render_widget(Paragraph::new(format!(" {}{}", hidden, if app.form_active_field == 3 { "█" } else { "" })).block(Block::default().title(" Senha ").borders(Borders::ALL).style(Style::default().fg(pass_color))), form_chunks[4]);
        let url_color = if app.form_active_field == 4 { app.theme.annotation } else { app.theme.base };
        f.render_widget(Paragraph::new(format!(" {}{}", app.form_url, if app.form_active_field == 4 { "█" } else { "" })).block(Block::default().title(" URL ").borders(Borders::ALL).style(Style::default().fg(url_color))), form_chunks[5]);

        f.render_widget(Paragraph::new("TAB/SHIFT-TAB: Navegar | ENTER: Confirmar").alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), form_chunks[6]);
    }

    if let Some((msg, time, is_error)) = &app.message {
        if time.elapsed() < Duration::from_secs(3) {
            let area = centered_fixed_rect(50, 5, f.size());
            f.render_widget(Clear, area);
            let title = if *is_error { " Erro " } else { " Sucesso " };
            f.render_widget(Paragraph::new(format!("\n{}", msg)).block(Block::default().title(title).borders(Borders::ALL).style(Style::default().fg(if *is_error { app.theme.alert_error } else { app.theme.alert_info }))).alignment(Alignment::Center), area);
        } else { app.message = None; }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)]).split(popup_layout[1])[1]
}

fn centered_fixed_rect(width: u16, height: u16, r: Rect) -> Rect {
    let col = r.width.saturating_sub(width) / 2;
    let row = r.height.saturating_sub(height) / 2;
    Rect::new(col, row, width.min(r.width), height.min(r.height))
}

fn ensure_config_exists(config_dir: &PathBuf) {
    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = r#"[general]
path = "~/"
recency = true
theme = "default"
"#;
        let _ = fs::write(config_path, default_config);
    }
}
