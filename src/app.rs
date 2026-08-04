use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::time::Instant;
use zeroize::Zeroizing;

use crate::config::Theme;
use crate::history::History;
use crate::keepass::{self, run_kpcli};
use crate::util::filter_items;
use crate::AppMode;

/// Uma posição de texto dentro de um campo do modal de informações,
/// endereçada como (linha, coluna) em índices de caracteres (não bytes).
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextPos {
    pub line: usize,
    pub col: usize,
}

/// Seleção de texto em um campo do modal de informações: âncora (onde o
/// clique começou) e cursor (posição atual, seguindo o arrasto do mouse).
/// `anchor == cursor` significa "sem seleção".
#[derive(Clone, Copy, Default)]
pub struct TextSelection {
    pub anchor: TextPos,
    pub cursor: TextPos,
}

impl TextSelection {
    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }

    /// Retorna (início, fim) normalizados (início <= fim).
    pub fn normalized(&self) -> (TextPos, TextPos) {
        if self.anchor <= self.cursor { (self.anchor, self.cursor) } else { (self.cursor, self.anchor) }
    }
}

/// Ações disponíveis no menu de contexto (clique com botão direito).
#[derive(Clone, Copy)]
pub enum ContextAction {
    AddNew,
    Edit,
    Delete,
}

/// Mensagem de status transitória (sucesso ou erro). Mensagens de sucesso
/// são exibidas na área de dicas de atalhos (rodapé); mensagens de erro
/// continuam em um modal centralizado. `clipboard_clear_secs`, quando
/// presente, faz a UI exibir uma contagem regressiva até a senha ser
/// apagada do clipboard.
#[derive(Clone)]
pub struct StatusMessage {
    pub text: String,
    pub time: Instant,
    pub is_error: bool,
    pub clipboard_clear_secs: Option<u64>,
}

pub struct App {
    pub db_path: String,
    pub password: Zeroizing<String>,
    pub entries: Vec<String>,
    pub filtered: Vec<String>,
    pub search_query: String,
    pub list_state: ListState,
    pub mode: AppMode,
    pub message: Option<StatusMessage>,
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
    pub form_notes: String,

    // Retângulos calculados no último desenho da tela, usados para
    // converter cliques/eventos de mouse em posições lógicas da UI.
    pub term_size: Rect,
    pub search_rect: Rect,
    pub list_inner_rect: Rect,
    pub form_rect: Rect,
    pub confirm_delete_rect: Rect,
    pub last_click: Option<(Instant, usize)>,

    // Menu de contexto (clique com botão direito)
    pub context_menu_anchor: (u16, u16),
    pub context_menu_rect: Rect,
    pub context_menu_item_rects: Vec<Rect>,
    pub context_menu_prev_mode: AppMode,
    pub context_menu_selected: usize,

    // Item da lista sob o cursor do mouse (destaque de hover, distinto da
    // seleção real navegada por teclado).
    pub hover_index: Option<usize>,

    // Modal de informações da entrada (TAB)
    pub info_title: String,
    pub info_url: String,
    pub info_notes: String,
    pub info_active_field: usize,
    pub info_field_rects: [Rect; 3],
    pub info_selection: [TextSelection; 3],
    pub info_dragging: bool,
    pub info_drag_field: Option<usize>,
    pub info_notes_scroll: usize,
    pub info_modal_rect: Rect,
    pub info_previous_mode: AppMode,

    // Modal de ajuda com os atalhos de teclado (CTRL+?)
    pub help_modal_rect: Rect,
    pub help_previous_mode: AppMode,
}

impl App {
    pub fn new(db_path: String, password: Zeroizing<String>, is_mac: bool, history: History, theme: Theme) -> Self {
        let mut app = Self {
            db_path, password, entries: vec![], filtered: vec![], search_query: String::new(), list_state: ListState::default(),
            mode: AppMode::Search, message: None, is_mac, history, last_key_was_g: false, list_height: 10, theme,
            all_groups: vec![], filtered_groups: vec![], form_group_state: ListState::default(),
            form_is_edit: false, form_original_path: String::new(), form_active_field: 0,
            form_group: String::new(), form_title: String::new(), form_username: String::new(),
            form_password: Zeroizing::new(String::new()), form_url: String::new(), form_notes: String::new(),
            term_size: Rect::default(), search_rect: Rect::default(), list_inner_rect: Rect::default(),
            form_rect: Rect::default(), confirm_delete_rect: Rect::default(), last_click: None,
            context_menu_anchor: (0, 0), context_menu_rect: Rect::default(), context_menu_item_rects: vec![],
            context_menu_prev_mode: AppMode::Normal, context_menu_selected: 0, hover_index: None,
            info_title: String::new(), info_url: String::new(), info_notes: String::new(),
            info_active_field: 0, info_field_rects: [Rect::default(); 3], info_selection: [TextSelection::default(); 3],
            info_dragging: false, info_drag_field: None, info_notes_scroll: 0, info_modal_rect: Rect::default(),
            info_previous_mode: AppMode::Normal,
            help_modal_rect: Rect::default(), help_previous_mode: AppMode::Normal,
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

    pub fn open_info_modal(&mut self) {
        let Some(entry) = self.get_selected() else { return; };
        if entry.ends_with("/[vazio]") { self.set_msg("Isso é um grupo vazio!", true); return; }

        let mut title = self.fetch_field(&entry, "Title");
        if title.trim().is_empty() {
            title = entry.rsplit('/').next().unwrap_or(&entry).to_string();
        }
        self.info_title = title;
        self.info_url = self.fetch_field(&entry, "URL");
        self.info_notes = self.fetch_field(&entry, "Notes");
        self.info_active_field = 0;
        self.info_selection = [TextSelection::default(); 3];
        self.info_notes_scroll = 0;
        self.info_dragging = false;
        self.info_drag_field = None;
        self.info_previous_mode = self.mode;
        self.hover_index = None;
        self.mode = AppMode::Info;
    }

    /// Linhas do campo "Notas", divididas por quebra de linha explícita.
    pub fn info_notes_lines(&self) -> Vec<&str> {
        if self.info_notes.is_empty() { vec![""] } else { self.info_notes.split('\n').collect() }
    }

    fn info_field_line(&self, field: usize, line_idx: usize) -> &str {
        match field {
            0 => self.info_title.as_str(),
            1 => self.info_url.as_str(),
            _ => self.info_notes_lines().get(line_idx).copied().unwrap_or(""),
        }
    }

    fn info_field_line_count(&self, field: usize) -> usize {
        if field == 2 { self.info_notes_lines().len() } else { 1 }
    }

    /// Converte uma posição de mouse (coluna/linha do terminal) dentro do
    /// retângulo de um campo em uma posição lógica (linha, coluna) no texto.
    pub fn info_hit_to_pos(&self, field: usize, column: u16, row: u16) -> TextPos {
        let rect = self.info_field_rects[field];
        let rel_row = row.saturating_sub(rect.y) as usize;
        let rel_col = column.saturating_sub(rect.x) as usize;
        let scroll = if field == 2 { self.info_notes_scroll } else { 0 };
        let max_line = self.info_field_line_count(field).saturating_sub(1);
        let line = (scroll + rel_row).min(max_line);
        let col = rel_col.min(self.info_field_line(field, line).chars().count());
        TextPos { line, col }
    }

    /// Extrai o texto atualmente selecionado em um campo do modal de informações.
    pub fn info_selected_text(&self, field: usize) -> String {
        let sel = self.info_selection[field];
        if sel.is_empty() { return String::new(); }
        let (start, end) = sel.normalized();
        let mut out = String::new();
        for line_idx in start.line..=end.line {
            let chars: Vec<char> = self.info_field_line(field, line_idx).chars().collect();
            let s = if line_idx == start.line { start.col.min(chars.len()) } else { 0 };
            let e = if line_idx == end.line { end.col.min(chars.len()) } else { chars.len() };
            out.push_str(&chars[s..e].iter().collect::<String>());
            if line_idx != end.line { out.push('\n'); }
        }
        out
    }

    /// Finaliza um arrasto de seleção no modal de informações: se algo foi
    /// selecionado, copia o texto para a área de transferência. Selecionar é
    /// feito inteiramente pela própria aplicação (não pelo terminal) para
    /// evitar que bordas dos campos sejam copiadas junto com o conteúdo.
    pub fn info_finish_drag(&mut self) {
        let Some(field) = self.info_drag_field.take() else { return; };
        self.info_dragging = false;
        let text = self.info_selected_text(field);
        if text.is_empty() { return; }
        match keepass::copy_to_clipboard(&text, self.is_mac) {
            Ok(()) => self.set_msg("Copiado para a área de transferência!", false),
            Err(_) => self.set_msg("Erro ao copiar.", true),
        }
    }

    pub fn open_add_form(&mut self) {
        self.form_is_edit = false; self.form_group.clear(); self.form_title.clear(); self.form_username.clear();
        self.form_password = Zeroizing::new(String::new()); self.form_url.clear(); self.form_notes.clear(); self.form_active_field = 0; self.mode = AppMode::Form;
        self.hover_index = None;
        self.filter_form_groups();
    }

    pub fn open_edit_form(&mut self, entry: String) {
        self.form_is_edit = true; self.form_original_path = entry.clone();
        if let Some(idx) = entry.rfind('/') { self.form_group = entry[..idx].to_string(); self.form_title = entry[idx+1..].to_string(); }
        else { self.form_group = String::new(); self.form_title = entry.clone(); }
        self.form_username = self.fetch_field(&entry, "UserName");
        self.form_password = Zeroizing::new(self.fetch_field(&entry, "Password"));
        self.form_url = self.fetch_field(&entry, "URL");
        self.form_notes = self.fetch_field(&entry, "Notes");
        self.form_active_field = 3; self.mode = AppMode::Form; self.hover_index = None; self.filter_form_groups();
    }

    /// Abre o menu de contexto pela tecla de espaço, ancorado próximo à
    /// entrada atualmente selecionada na lista (em vez da posição do mouse).
    pub fn open_context_menu(&mut self) {
        self.context_menu_prev_mode = if self.mode == AppMode::Search { AppMode::Search } else { AppMode::Normal };
        self.context_menu_selected = 0;
        self.hover_index = None;
        let rect = self.list_inner_rect;
        let row = match self.list_state.selected() {
            Some(idx) => rect.y + idx.saturating_sub(self.list_state.offset()) as u16,
            None => rect.y,
        };
        self.context_menu_anchor = (rect.x + 2, row.min(rect.y + rect.height.saturating_sub(1)));
        self.mode = AppMode::ContextMenu;
    }

    pub fn context_menu_next(&mut self) { self.context_menu_selected = (self.context_menu_selected + 1) % 3; }
    pub fn context_menu_prev(&mut self) { self.context_menu_selected = if self.context_menu_selected == 0 { 2 } else { self.context_menu_selected - 1 }; }

    /// Executa a ação escolhida no menu de contexto (clique com botão direito
    /// ou ENTER com o teclado). Volta ao modo anterior à abertura do menu
    /// (Normal ou Busca).
    pub fn context_menu_action(&mut self, action: ContextAction) {
        self.mode = self.context_menu_prev_mode;
        match action {
            ContextAction::AddNew => self.open_add_form(),
            ContextAction::Edit => {
                if let Some(entry) = self.get_selected() { self.open_edit_form(entry); }
                else { self.set_msg("Nenhuma entrada selecionada.", true); }
            }
            ContextAction::Delete => {
                if self.get_selected().is_some() { self.hover_index = None; self.mode = AppMode::ConfirmDelete; }
                else { self.set_msg("Nenhuma entrada selecionada.", true); }
            }
        }
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
                &["edit", "-q", "-p", "-u", &self.form_username, "--url", &self.form_url, "--notes", &self.form_notes, &self.db_path, &path],
                &[self.password.as_str(), self.form_password.as_str(), self.form_password.as_str()],
            );
            if result.map(|r| r.success).unwrap_or(false) { self.set_msg("Entrada editada com sucesso!", false); } else { self.set_msg("Erro ao editar.", true); }
        } else {
            let result = run_kpcli(
                &["add", "-q", "-p", "-u", &self.form_username, "--url", &self.form_url, "--notes", &self.form_notes, &self.db_path, &path],
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
    pub fn set_msg(&mut self, msg: &str, is_error: bool) {
        self.message = Some(StatusMessage { text: msg.to_string(), time: Instant::now(), is_error, clipboard_clear_secs: None });
    }

    /// Mensagem de sucesso de cópia de senha: a área de dicas exibe uma
    /// contagem regressiva até o clipboard ser limpo automaticamente.
    fn set_clipboard_msg(&mut self, msg: &str) {
        self.message = Some(StatusMessage {
            text: msg.to_string(),
            time: Instant::now(),
            is_error: false,
            clipboard_clear_secs: Some(keepass::CLIPBOARD_CLEAR_SECS),
        });
    }

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
                self.set_clipboard_msg(&format!("Copiado: {}", entry));
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
