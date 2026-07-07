use crate::clipboard;
use crate::config::Theme;
use crate::history::History;
use crate::store::{EntryData, PassStore};
use ratatui::widgets::ListState;
use std::time::Instant;
use zeroize::Zeroizing;

pub const EMPTY_GROUP_SUFFIX: &str = "/[vazio]";

#[derive(PartialEq)]
pub enum AppMode {
    GpgUnlock,
    Search,
    Normal,
    ConfirmDelete,
    Form,
}

pub fn filter_items(items: &[String], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let terms: Vec<&str> = q.split_whitespace().collect();
    if terms.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .filter(|e| {
            let lower = e.to_lowercase();
            terms.iter().all(|t| lower.contains(t))
        })
        .cloned()
        .collect()
}

pub fn split_entry_path(entry: &str) -> (String, String) {
    match entry.rfind('/') {
        Some(idx) => (entry[..idx].to_string(), entry[idx + 1..].to_string()),
        None => (String::new(), entry.to_string()),
    }
}

pub fn join_entry_path(group: &str, title: &str) -> String {
    let group = group.trim().trim_matches('/');
    let title = title.trim();
    if group.is_empty() {
        title.to_string()
    } else {
        format!("{}/{}", group, title)
    }
}

pub struct App {
    pub store: PassStore,
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
    pub clip_time: u64,

    pub master_passphrase: Zeroizing<String>,
    pub unlock_input: Zeroizing<String>,

    pub all_groups: Vec<String>,
    pub filtered_groups: Vec<String>,
    pub form_group_state: ListState,
    pub form_is_edit: bool,
    pub form_original_path: String,
    pub form_original_extra: Vec<String>,
    pub form_active_field: usize,
    pub form_group: String,
    pub form_title: String,
    pub form_username: String,
    pub form_password: Zeroizing<String>,
    pub form_url: String,
}

impl App {
    pub fn new(
        store: PassStore,
        is_mac: bool,
        history: History,
        theme: Theme,
        clip_time: u64,
    ) -> Self {
        let mut app = Self {
            store,
            entries: vec![],
            filtered: vec![],
            search_query: String::new(),
            list_state: ListState::default(),
            mode: AppMode::GpgUnlock,
            message: None,
            is_mac,
            history,
            last_key_was_g: false,
            list_height: 10,
            theme,
            clip_time,
            master_passphrase: Zeroizing::new(String::new()),
            unlock_input: Zeroizing::new(String::new()),
            all_groups: vec![],
            filtered_groups: vec![],
            form_group_state: ListState::default(),
            form_is_edit: false,
            form_original_path: String::new(),
            form_original_extra: vec![],
            form_active_field: 0,
            form_group: String::new(),
            form_title: String::new(),
            form_username: String::new(),
            form_password: Zeroizing::new(String::new()),
            form_url: String::new(),
        };
        app.refresh_entries();
        app
    }

    pub fn refresh_entries(&mut self) {
        let (mut entries, groups) = self.store.list();
        self.all_groups = groups.clone();

        for g in &groups {
            let prefix = format!("{}/", g);
            let has_children = entries.iter().any(|e| e.starts_with(&prefix))
                || groups.iter().any(|other| other.starts_with(&prefix));
            if !has_children {
                entries.push(format!("{}{}", g, EMPTY_GROUP_SUFFIX));
            }
        }

        self.history.sort_items(&mut entries);
        self.entries = entries;
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        self.filtered = filter_items(&self.entries, &self.search_query);
        self.list_state
            .select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    // ---------- Navegação ----------
    pub fn next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) if i >= self.filtered.len() - 1 => 0,
            Some(i) => i + 1,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(0) | None => self.filtered.len() - 1,
            Some(i) => i - 1,
        };
        self.list_state.select(Some(i));
    }

    pub fn go_to_top(&mut self) {
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn go_to_bottom(&mut self) {
        if !self.filtered.is_empty() {
            self.list_state.select(Some(self.filtered.len() - 1));
        }
    }

    pub fn half_page_down(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let step = (self.list_height.saturating_sub(2) / 2).max(1);
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state
            .select(Some((i + step).min(self.filtered.len() - 1)));
    }

    pub fn half_page_up(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let step = (self.list_height.saturating_sub(2) / 2).max(1);
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(step)));
    }

    pub fn get_selected(&self) -> Option<String> {
        self.list_state.selected().map(|i| self.filtered[i].clone())
    }

    pub fn set_msg(&mut self, msg: &str, is_error: bool) {
        self.message = Some((msg.to_string(), Instant::now(), is_error));
    }

    // ---------- Unlock ----------
    pub fn confirm_unlock(&mut self) {
        let passphrase = std::mem::take(&mut self.unlock_input);

        match self.store.verify_passphrase(&passphrase) {
            Ok(()) => {
                self.master_passphrase = passphrase;
                self.mode = AppMode::Search;
                self.set_msg("Desbloqueado com sucesso!", false);
            }
            Err(e) => {
                self.set_msg(&format!("Erro: {}", e), true);
            }
        }
    }

    pub fn start_unlock(&mut self) {
        self.unlock_input = Zeroizing::new(String::new());
        self.mode = AppMode::GpgUnlock;
    }

    // ---------- Ações ----------
    pub fn copy_password(&mut self) {
        let Some(entry) = self.get_selected() else {
            return;
        };
        if entry.ends_with(EMPTY_GROUP_SUFFIX) {
            self.set_msg("Isso é um grupo vazio!", true);
            return;
        }
        if self.master_passphrase.is_empty() {
            self.start_unlock();
            return;
        }

        match self.store.show_with_passphrase(&entry, &self.master_passphrase) {
            Ok(data) => {
                match clipboard::copy_to_clipboard(&data.password, self.is_mac) {
                    Ok(()) => {
                        self.history.record_use(&entry);
                        self.set_msg(
                            &format!("Copiado: {}\n(Limpo do clipboard em {}s)", entry, self.clip_time),
                            false,
                        );
                        clipboard::spawn_clipboard_clearer(
                            data.password,
                            self.is_mac,
                            self.clip_time,
                        );
                    }
                    Err(e) => self.set_msg(&format!("Erro ao copiar: {}", e), true),
                }
            }
            Err(e) => {
                if e.contains("incorreta") || e.contains("Senha GPG") {
                    self.master_passphrase = Zeroizing::new(String::new());
                    self.start_unlock();
                    self.set_msg("Senha GPG incorreta ou cache expirado. Digite novamente.", true);
                } else {
                    self.set_msg(&format!("Erro: {}", e), true);
                }
            }
        }
    }

    pub fn open_add_form(&mut self) {
        self.form_is_edit = false;
        self.form_group.clear();
        self.form_title.clear();
        self.form_username.clear();
        self.form_password = Zeroizing::new(String::new());
        self.form_url.clear();
        self.form_original_extra.clear();
        self.form_active_field = 0;
        self.mode = AppMode::Form;
        self.filter_form_groups();
    }

    pub fn open_edit_form(&mut self, entry: String) {
        if entry.ends_with(EMPTY_GROUP_SUFFIX) {
            self.set_msg("Grupos vazios não são editáveis.", true);
            return;
        }
        self.form_is_edit = true;
        self.form_original_path = entry.clone();
        let (group, title) = split_entry_path(&entry);
        self.form_group = group;
        self.form_title = title;
        match self.store.show_with_passphrase(&entry, &self.master_passphrase) {
            Ok(data) => {
                self.form_username = data.username;
                self.form_password = data.password;
                self.form_url = data.url;
                self.form_original_extra = data.extra;
            }
            Err(e) => {
                self.set_msg(&format!("Erro lendo entrada: {}", e), true);
                return;
            }
        }
        self.form_active_field = 3;
        self.mode = AppMode::Form;
        self.filter_form_groups();
    }

    pub fn filter_form_groups(&mut self) {
        self.filtered_groups = filter_items(&self.all_groups, &self.form_group);
        self.form_group_state
            .select(if self.filtered_groups.is_empty() { None } else { Some(0) });
    }

    pub fn form_next_group(&mut self) {
        if self.filtered_groups.is_empty() {
            return;
        }
        let i = match self.form_group_state.selected() {
            Some(i) if i >= self.filtered_groups.len() - 1 => 0,
            Some(i) => i + 1,
            None => 0,
        };
        self.form_group_state.select(Some(i));
    }

    pub fn form_prev_group(&mut self) {
        if self.filtered_groups.is_empty() {
            return;
        }
        let i = match self.form_group_state.selected() {
            Some(0) | None => self.filtered_groups.len() - 1,
            Some(i) => i - 1,
        };
        self.form_group_state.select(Some(i));
    }

    pub fn submit_form(&mut self) {
        let title = self.form_title.trim().to_string();
        if title.is_empty() {
            self.set_msg("O Título não pode ser vazio!", true);
            return;
        }
        let path = join_entry_path(&self.form_group, &title);

        let data = EntryData {
            password: self.form_password.clone(),
            username: self.form_username.trim().to_string(),
            url: self.form_url.trim().to_string(),
            extra: self.form_original_extra.clone(),
        };

        let result = if self.form_is_edit {
            self.store.insert(&self.form_original_path, &data).and_then(|_| {
                if path != self.form_original_path {
                    self.store.mv(&self.form_original_path, &path)
                } else {
                    Ok(())
                }
            })
        } else {
            self.store.insert(&path, &data)
        };

        match result {
            Ok(()) => {
                self.history.record_use(&path);
                self.set_msg(
                    if self.form_is_edit { "Entrada editada com sucesso!" } else { "Entrada adicionada!" },
                    false,
                );
            }
            Err(e) => self.set_msg(&format!("Erro: {}", e), true),
        }

        self.form_password = Zeroizing::new(String::new());
        self.refresh_entries();
        self.mode = AppMode::Normal;
    }

    pub fn delete_selected(&mut self) {
        let Some(entry) = self.get_selected() else {
            return;
        };
        let result = if let Some(group) = entry.strip_suffix(EMPTY_GROUP_SUFFIX) {
            self.store.rmdir_empty(group).map(|_| "Grupo excluído!")
        } else {
            self.store.rm(&entry).map(|_| "Entrada excluída!")
        };
        match result {
            Ok(msg) => {
                self.set_msg(msg, false);
                self.refresh_entries();
                self.previous();
            }
            Err(e) => self.set_msg(&format!("Erro ao excluir: {}", e), true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_and_terms() {
        let items: Vec<String> = vec![
            "Trabalho/servidor-admin".into(),
            "Trabalho/intranet".into(),
            "Pessoal/github".into(),
        ];
        assert_eq!(filter_items(&items, ""), items);
        assert_eq!(filter_items(&items, "trab serv"), vec!["Trabalho/servidor-admin".to_string()]);
        assert_eq!(filter_items(&items, "GITHUB"), vec!["Pessoal/github".to_string()]);
        assert!(filter_items(&items, "naoexiste").is_empty());
    }

    #[test]
    fn split_and_join_paths() {
        assert_eq!(split_entry_path("a/b/c"), ("a/b".to_string(), "c".to_string()));
        assert_eq!(split_entry_path("solo"), (String::new(), "solo".to_string()));
        assert_eq!(join_entry_path("g", "t"), "g/t");
        assert_eq!(join_entry_path("", "t"), "t");
        assert_eq!(join_entry_path("/g/", " t "), "g/t");
    }
}
