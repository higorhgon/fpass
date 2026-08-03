use ratatui::widgets::ListState;
use std::time::Instant;
use zeroize::Zeroizing;

use crate::config::Theme;
use crate::history::History;
use crate::keepass::{self, run_kpcli};
use crate::util::filter_items;
use crate::AppMode;

pub struct App {
    pub db_path: String,
    pub password: Zeroizing<String>,
    pub entries: Vec<String>,
    pub filtered: Vec<String>,
    pub search_query: String,
    pub list_state: ListState,
    pub mode: AppMode,
    pub message: Option<(String, Instant, bool)>,
    pub is_mac: bool,
    pub history: History,
    pub last_key_was_g: bool,
    pub list_height: usize,
    pub theme: Theme,
    pub all_groups: Vec<String>,
    pub filtered_groups: Vec<String>,
    pub form_group_state: ListState,
    pub form_is_edit: bool,
    pub form_original_path: String,
    pub form_active_field: usize,
    pub form_group: String,
    pub form_title: String,
    pub form_username: String,
    pub form_password: Zeroizing<String>,
    pub form_url: String,
}

impl App {
    pub fn new(db_path: String, password: Zeroizing<String>, is_mac: bool, history: History, theme: Theme) -> Self {
        let mut app = Self {
            db_path, password, entries: vec![], filtered: vec![], search_query: String::new(), list_state: ListState::default(),
            mode: AppMode::Search, message: None, is_mac, history, last_key_was_g: false, list_height: 10, theme,
            all_groups: vec![], filtered_groups: vec![], form_group_state: ListState::default(),
            form_is_edit: false, form_original_path: String::new(), form_active_field: 0,
            form_group: String::new(), form_title: String::new(), form_username: String::new(),
            form_password: Zeroizing::new(String::new()), form_url: String::new(),
        };
        app.refresh_entries();
        app
    }

    pub fn refresh_entries(&mut self) {
        if let Ok(result) = run_kpcli(&["ls", "-Rfq", &self.db_path], &[self.password.as_str()]) {
            self.entries.clear(); self.all_groups.clear();
            let lines: Vec<String> = result.stdout.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

            let mut groups = Vec::new();
            let mut entries = Vec::new();
            for line in &lines {
                if line.ends_with('/') { groups.push(line.trim_end_matches('/').to_string()); }
                else { entries.push(line.clone()); }
            }
            self.all_groups = groups.clone();

            // Identifica grupos vazios
            for g in &groups {
                let g_prefix = format!("{}/", g);
                let is_empty = !lines.iter().any(|l| l.starts_with(&g_prefix) && l != &g_prefix);
                if is_empty { entries.push(format!("{}/[vazio]", g)); }
            }

            self.entries = entries;
            self.history.sort_items(&mut self.entries);
        }
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        self.filtered = filter_items(&self.entries, &self.search_query);
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    pub fn open_add_form(&mut self) {
        self.form_is_edit = false; self.form_group.clear(); self.form_title.clear(); self.form_username.clear();
        self.form_password = Zeroizing::new(String::new()); self.form_url.clear(); self.form_active_field = 0; self.mode = AppMode::Form;
        self.filter_form_groups();
    }

    pub fn open_edit_form(&mut self, entry: String) {
        self.form_is_edit = true; self.form_original_path = entry.clone();
        if let Some(idx) = entry.rfind('/') { self.form_group = entry[..idx].to_string(); self.form_title = entry[idx+1..].to_string(); }
        else { self.form_group = String::new(); self.form_title = entry.clone(); }
        self.form_username = self.fetch_field(&entry, "UserName");
        self.form_password = Zeroizing::new(self.fetch_field(&entry, "Password"));
        self.form_url = self.fetch_field(&entry, "URL");
        self.form_active_field = 3; self.mode = AppMode::Form; self.filter_form_groups();
    }

    fn fetch_field(&self, path: &str, field: &str) -> String {
        run_kpcli(&["show", "-q", &self.db_path, path, "-a", field], &[self.password.as_str()])
            .map(|r| r.stdout.trim().to_string())
            .unwrap_or_default()
    }

    pub fn filter_form_groups(&mut self) {
        self.filtered_groups = filter_items(&self.all_groups, &self.form_group);
        self.form_group_state.select(if self.filtered_groups.is_empty() { None } else { Some(0) });
    }

    pub fn form_next_group(&mut self) { if self.filtered_groups.is_empty() { return; } let i = match self.form_group_state.selected() { Some(i) => if i >= self.filtered_groups.len() - 1 { 0 } else { i + 1 }, None => 0 }; self.form_group_state.select(Some(i)); }
    pub fn form_prev_group(&mut self) { if self.filtered_groups.is_empty() { return; } let i = match self.form_group_state.selected() { Some(i) => if i == 0 { self.filtered_groups.len() - 1 } else { i - 1 }, None => 0 }; self.form_group_state.select(Some(i)); }

    pub fn submit_form(&mut self) {
        let group = self.form_group.trim().trim_end_matches('/').to_string();
        let title = self.form_title.trim().to_string();
        let path = if group.is_empty() { title.clone() } else { format!("{}/{}", group, title) };

        if title.is_empty() { self.set_msg("O Título não pode ser vazio!", true); return; }

        // Tenta criar o grupo primeiro (mkdir no keepassxc-cli não falha se o grupo já existir, ou podemos ignorar o erro)
        if !group.is_empty() {
            let _ = run_kpcli(&["mkdir", "-q", &self.db_path, &group], &[self.password.as_str()]);
        }

        if self.form_is_edit {
            if path != self.form_original_path {
                // O comando 'mv' do keepassxc-cli espera [database] [origem] [grupo_destino]
                // Se o destino for a raiz, o grupo_destino deve ser "/"
                let dest_group = if group.is_empty() { "/" } else { &group };
                let _ = run_kpcli(&["mv", "-q", &self.db_path, &self.form_original_path, dest_group], &[self.password.as_str()]);
            }
            let result = run_kpcli(
                &["edit", "-q", "-p", "-u", &self.form_username, "--url", &self.form_url, &self.db_path, &path],
                &[self.password.as_str(), self.form_password.as_str(), self.form_password.as_str()],
            );
            if result.map(|r| r.success).unwrap_or(false) { self.set_msg("Entrada editada com sucesso!", false); } else { self.set_msg("Erro ao editar.", true); }
        } else {
            let result = run_kpcli(
                &["add", "-q", "-p", "-u", &self.form_username, "--url", &self.form_url, &self.db_path, &path],
                &[self.password.as_str(), self.form_password.as_str(), self.form_password.as_str()],
            );
            if result.map(|r| r.success).unwrap_or(false) { self.history.record_use(&path); self.set_msg("Entrada adicionada!", false); } else { self.set_msg("Erro ao adicionar.", true); }
        }
        self.refresh_entries(); self.mode = AppMode::Normal;
    }

    pub fn next(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i >= self.filtered.len() - 1 { 0 } else { i + 1 }, None => 0 }; self.list_state.select(Some(i)); }
    pub fn previous(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i == 0 { self.filtered.len() - 1 } else { i - 1 }, None => 0 }; self.list_state.select(Some(i)); }
    pub fn go_to_top(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(0)); } }
    pub fn go_to_bottom(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); } }
    pub fn half_page_down(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some((i + step).min(self.filtered.len() - 1))); }
    pub fn half_page_up(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some(i.saturating_sub(step))); }
    pub fn get_selected(&self) -> Option<String> { self.list_state.selected().map(|i| self.filtered[i].clone()) }
    pub fn set_msg(&mut self, msg: &str, is_error: bool) { self.message = Some((msg.to_string(), Instant::now(), is_error)); }

    pub fn copy_password(&mut self) {
        let Some(entry) = self.get_selected() else { return; };
        if entry.ends_with("/[vazio]") {
            self.set_msg("Isso é um grupo vazio!", true);
            return;
        }
        self.history.record_use(&entry);

        let result = run_kpcli(&["show", "-q", &self.db_path, &entry, "-a", "Password"], &[self.password.as_str()]);
        let Ok(result) = result else {
            self.set_msg("Erro ao copiar senha.", true);
            return;
        };
        let entry_pass = Zeroizing::new(result.stdout.trim().to_string());

        match keepass::copy_to_clipboard(&entry_pass, self.is_mac) {
            Ok(()) => {
                self.set_msg(&format!("Copiado: {}\n(Limpo do clipboard em 10s)", entry), false);
                keepass::spawn_clipboard_clearer(entry_pass, self.is_mac);
            }
            Err(_) => self.set_msg("Erro ao copiar senha.", true),
        }
    }

    pub fn delete_selected(&mut self) {
        let Some(entry) = self.get_selected() else { return; };
        let is_empty_group = entry.ends_with("/[vazio]");
        let (cmd_name, path_to_del) = if is_empty_group {
            ("rmdir", entry.trim_end_matches("/[vazio]").to_string())
        } else {
            ("rm", entry)
        };

        match run_kpcli(&[cmd_name, "-q", &self.db_path, &path_to_del], &[self.password.as_str()]) {
            Ok(result) if result.success => {
                self.set_msg(if is_empty_group { "Grupo excluído!" } else { "Entrada excluída!" }, false);
                self.refresh_entries();
                self.previous();
            }
            Ok(result) => self.set_msg(&format!("Erro ao excluir: {}", result.stderr), true),
            Err(e) => self.set_msg(&format!("Erro ao excluir: {}", e), true),
        }
    }
}
