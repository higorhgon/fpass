use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame, Terminal,
};
use rust_i18n::t;
use std::{io, time::{Duration, Instant}};

use crate::app::{App, ContextAction, TextSelection};
use crate::backend::BackendKind;
use crate::util::{centered_fixed_rect, mask_cursor, scroll_tail};
use crate::AppMode;

/// Distância máxima (em tempo) entre dois cliques no mesmo item para que
/// sejam tratados como duplo clique.
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(450);

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;
        if event::poll(Duration::from_millis(100))? {
            if process_event(app, event::read()?) {
                return Ok(());
            }
            // Esvazia quaisquer eventos já enfileirados (comum com MouseEventKind::Moved,
            // que dispara a cada célula percorrida) antes do próximo redesenho, para
            // que o hover do mouse não fique visivelmente atrasado.
            while event::poll(Duration::from_millis(0))? {
                if process_event(app, event::read()?) {
                    return Ok(());
                }
            }
        }
    }
}

/// Processa um único evento de terminal. Retorna `true` se o app deve encerrar.
fn process_event(app: &mut App, event: Event) -> bool {
    match event {
        Event::Key(key) => {
            let mut is_g_key = false;

            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if app.mode == AppMode::Search || app.mode == AppMode::Normal {
                    handle_ctrl_key(app, key.code);
                    return false;
                } else if key.code == KeyCode::Char('c') {
                    return true;
                } else if app.mode == AppMode::Form && app.form_active_field == 0 {
                    // No campo Grupo (um "insert mode": j/k digitariam letras),
                    // CTRL+N/CTRL+P navegam a dropdown de sugestões.
                    match key.code {
                        KeyCode::Char('n') => { app.form_next_group(); return false; }
                        KeyCode::Char('p') => { app.form_prev_group(); return false; }
                        _ => {}
                    }
                }
            }

            match app.mode {
                AppMode::Search => handle_search_key(app, key.code),
                AppMode::Normal => {
                    if handle_normal_key(app, key.code, &mut is_g_key) {
                        return true;
                    }
                }
                AppMode::ConfirmDelete => handle_confirm_delete_key(app, key.code),
                AppMode::Form => handle_form_key(app, key.code),
                AppMode::Info => handle_info_key(app, key.code),
                AppMode::ContextMenu => handle_context_menu_key(app, key.code),
                AppMode::Help => handle_help_key(app, key.code),
                AppMode::RenameGroup => handle_rename_group_key(app, key.code),
                AppMode::PasswordInput | AppMode::ConfirmCreateDb | AppMode::CreateDb
                | AppMode::ChooseDbType | AppMode::CreatePassStore => {}
            }
            app.last_key_was_g = is_g_key;
            false
        }
        Event::Mouse(mouse) => {
            handle_mouse_event(app, mouse);
            false
        }
        _ => false,
    }
}

fn handle_ctrl_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('d') => app.half_page_down(),
        KeyCode::Char('u') => app.half_page_up(),
        // Alias de navegação (estilo readline/emacs) pra quando o modo de
        // busca está ativo e j/k digitariam letras em vez de navegar.
        KeyCode::Char('n') => app.next(),
        KeyCode::Char('p') => app.previous(),
        KeyCode::Char('a') => app.open_add_form(),
        KeyCode::Char('e') => app.edit_selected(),
        KeyCode::Char('x') => {
            if app.get_selected().is_some() {
                app.hover_index = None;
                app.mode = AppMode::ConfirmDelete;
            }
        }
        // "CTRL+?" chega de formas diferentes conforme o terminal: sem o
        // protocolo estendido de teclado, Ctrl+/ e Ctrl+Shift+/ (?) mandam o
        // mesmo byte 0x1F, que o crossterm decodifica como Char('7')+CONTROL.
        // Aceitamos as variantes plausíveis para funcionar na maioria dos terminais.
        KeyCode::Char('?') | KeyCode::Char('/') | KeyCode::Char('7') => {
            app.help_previous_mode = if app.mode == AppMode::Search { AppMode::Search } else { AppMode::Normal };
            app.hover_index = None;
            app.mode = AppMode::Help;
        }
        _ => {}
    }
}

fn handle_search_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.mode = AppMode::Normal,
        KeyCode::Down => app.next(),
        KeyCode::Up => app.previous(),
        KeyCode::Enter => app.copy_password(),
        KeyCode::Tab => app.open_info_modal(),
        KeyCode::Backspace => { app.search_query.pop(); app.apply_filter(); }
        KeyCode::Char(c) => { app.search_query.push(c); app.apply_filter(); }
        _ => {}
    }
}

/// Retorna `true` se o app deve encerrar.
fn handle_normal_key(app: &mut App, code: KeyCode, is_g_key: &mut bool) -> bool {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Down | KeyCode::Char('j') => app.next(),
        KeyCode::Up | KeyCode::Char('k') => app.previous(),
        KeyCode::Enter => app.copy_password(),
        KeyCode::Tab => app.open_info_modal(),
        KeyCode::Char(' ') => app.open_context_menu(),
        KeyCode::Char('/') | KeyCode::Char('f') | KeyCode::Char('i') => app.mode = AppMode::Search,
        KeyCode::Char('G') => app.go_to_bottom(),
        KeyCode::Char('g') => {
            *is_g_key = true;
            if app.last_key_was_g {
                app.go_to_top();
                *is_g_key = false;
            }
        }
        _ => {}
    }
    false
}

fn handle_confirm_delete_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') => { app.delete_selected(); app.mode = AppMode::Normal; }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Enter | KeyCode::Esc => app.mode = AppMode::Normal,
        _ => {}
    }
}

fn handle_info_key(app: &mut App, code: KeyCode) {
    // O formato do pass não tem URL/Notas como campos próprios (ver
    // draw_info_modal_pass): só o Título é navegável nesse caso.
    let field_count = if app.backend.kind() == BackendKind::Pass { 1 } else { 3 };
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = app.info_previous_mode,
        KeyCode::Tab => app.info_active_field = (app.info_active_field + 1) % field_count,
        KeyCode::BackTab => app.info_active_field = if app.info_active_field == 0 { field_count - 1 } else { app.info_active_field - 1 },
        KeyCode::Down if app.info_active_field == 2 => app.info_notes_scroll = app.info_notes_scroll.saturating_add(1),
        KeyCode::Up if app.info_active_field == 2 => app.info_notes_scroll = app.info_notes_scroll.saturating_sub(1),
        _ => {}
    }
}

fn handle_context_menu_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.mode = app.context_menu_prev_mode,
        KeyCode::Down | KeyCode::Char('j') => app.context_menu_next(),
        KeyCode::Up | KeyCode::Char('k') => app.context_menu_prev(),
        KeyCode::Enter => {
            let action = match app.context_menu_selected {
                0 => ContextAction::AddNew,
                1 => ContextAction::Edit,
                _ => ContextAction::Delete,
            };
            app.context_menu_action(action);
        }
        _ => {}
    }
}

fn handle_help_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => app.mode = app.help_previous_mode,
        _ => {}
    }
}

fn handle_rename_group_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.mode = AppMode::Normal,
        KeyCode::Enter => app.submit_rename_group(),
        KeyCode::Backspace => { app.rename_group_title.pop(); }
        KeyCode::Char(c) => { app.rename_group_title.push(c); }
        _ => {}
    }
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// Converte a linha do terminal clicada em um índice na lista filtrada,
/// levando em conta o deslocamento de rolagem atual da lista.
fn list_index_at(app: &App, column: u16, row: u16) -> Option<usize> {
    if !rect_contains(app.list_inner_rect, column, row) { return None; }
    let rel = (row - app.list_inner_rect.y) as usize;
    let idx = app.list_state.offset() + rel;
    if idx < app.filtered.len() { Some(idx) } else { None }
}

fn context_menu_index_at(app: &App, column: u16, row: u16) -> Option<usize> {
    app.context_menu_item_rects.iter().position(|rect| rect_contains(*rect, column, row))
}

fn context_menu_action_at(app: &App, column: u16, row: u16) -> Option<ContextAction> {
    context_menu_index_at(app, column, row).map(|i| match i { 0 => ContextAction::AddNew, 1 => ContextAction::Edit, _ => ContextAction::Delete })
}

fn info_field_at(app: &App, column: u16, row: u16) -> Option<usize> {
    (0..3).find(|&i| rect_contains(app.info_field_rects[i], column, row))
}

fn handle_mouse_event(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => handle_left_down(app, mouse.column, mouse.row),
        MouseEventKind::Drag(MouseButton::Left) => handle_left_drag(app, mouse.column, mouse.row),
        MouseEventKind::Up(MouseButton::Left) => handle_left_up(app),
        MouseEventKind::Down(MouseButton::Right) => handle_right_down(app, mouse.column, mouse.row),
        MouseEventKind::Moved => handle_mouse_moved(app, mouse.column, mouse.row),
        _ => {}
    }
}

/// Atualiza os destaques de hover (lista de entradas / itens do menu de
/// contexto) conforme o mouse se move, sem alterar nenhuma seleção real.
fn handle_mouse_moved(app: &mut App, column: u16, row: u16) {
    match app.mode {
        AppMode::Normal | AppMode::Search => {
            app.hover_index = list_index_at(app, column, row);
        }
        AppMode::ContextMenu => {
            if let Some(idx) = context_menu_index_at(app, column, row) {
                app.context_menu_selected = idx;
            }
        }
        _ => {}
    }
}

fn handle_left_down(app: &mut App, column: u16, row: u16) {
    app.message = None;

    match app.mode {
        AppMode::ContextMenu => {
            match context_menu_action_at(app, column, row) {
                Some(action) => app.context_menu_action(action),
                None => app.mode = app.context_menu_prev_mode,
            }
        }
        AppMode::Form => {
            if !rect_contains(app.form_rect, column, row) { app.mode = AppMode::Normal; }
        }
        AppMode::ConfirmDelete => {
            if !rect_contains(app.confirm_delete_rect, column, row) { app.mode = AppMode::Normal; }
        }
        AppMode::Help => {
            if !rect_contains(app.help_modal_rect, column, row) { app.mode = app.help_previous_mode; }
        }
        AppMode::RenameGroup => {
            if !rect_contains(app.rename_group_rect, column, row) { app.mode = AppMode::Normal; }
        }
        AppMode::Info => {
            if rect_contains(app.info_modal_rect, column, row) {
                if let Some(field) = info_field_at(app, column, row) {
                    let pos = app.info_hit_to_pos(field, column, row);
                    app.info_active_field = field;
                    app.info_selection[field] = TextSelection { anchor: pos, cursor: pos };
                    app.info_dragging = true;
                    app.info_drag_field = Some(field);
                }
            } else {
                app.mode = app.info_previous_mode;
            }
        }
        AppMode::Normal | AppMode::Search => {
            if rect_contains(app.search_rect, column, row) {
                app.mode = AppMode::Search;
            } else if let Some(idx) = list_index_at(app, column, row) {
                let now = Instant::now();
                let is_double = matches!(app.last_click, Some((t, i)) if i == idx && now.duration_since(t) < DOUBLE_CLICK_WINDOW);
                app.list_state.select(Some(idx));
                app.mode = AppMode::Normal;
                if is_double {
                    app.copy_password();
                    app.last_click = None;
                } else {
                    app.last_click = Some((now, idx));
                }
            }
        }
        _ => {}
    }
}

fn handle_left_drag(app: &mut App, column: u16, row: u16) {
    if app.mode == AppMode::Info {
        if let Some(field) = app.info_drag_field {
            app.info_selection[field].cursor = app.info_hit_to_pos(field, column, row);
        }
    }
}

fn handle_left_up(app: &mut App) {
    if app.mode == AppMode::Info {
        app.info_finish_drag();
    }
}

fn handle_right_down(app: &mut App, column: u16, row: u16) {
    app.message = None;
    if !matches!(app.mode, AppMode::Normal | AppMode::Search | AppMode::ContextMenu) {
        return;
    }
    if let Some(idx) = list_index_at(app, column, row) {
        app.list_state.select(Some(idx));
    }
    if app.mode != AppMode::ContextMenu {
        app.context_menu_prev_mode = if app.mode == AppMode::Search { AppMode::Search } else { AppMode::Normal };
        app.context_menu_selected = 0;
    }
    app.context_menu_anchor = (column, row);
    app.mode = AppMode::ContextMenu;
}

fn handle_form_key(app: &mut App, code: KeyCode) {
    // Entradas do tipo `pass` só expõem Grupo/Título/Senha (3 campos); o
    // KeePassXC mantém o formulário completo (6 campos, com Notas).
    let is_pass = app.backend.kind() == BackendKind::Pass;
    let field_count = if is_pass { 3 } else { 6 };
    match code {
        KeyCode::Esc => app.mode = AppMode::Normal,
        KeyCode::BackTab => { app.form_active_field = if app.form_active_field == 0 { field_count - 1 } else { app.form_active_field - 1 }; }
        KeyCode::Tab => { app.form_active_field = (app.form_active_field + 1) % field_count; }
        KeyCode::Down => { if app.form_active_field == 0 { app.form_next_group(); } }
        KeyCode::Up => { if app.form_active_field == 0 { app.form_prev_group(); } }
        KeyCode::Enter => {
            if let Some(idx) = (app.form_active_field == 0).then(|| app.form_group_state.selected()).flatten() {
                app.form_group = app.filtered_groups[idx].clone();
                app.form_active_field = 1;
            } else if !is_pass && app.form_active_field == 5 {
                // Campo de Notas: ENTER quebra linha em vez de confirmar o formulário.
                app.form_notes.push('\n');
            } else {
                app.submit_form();
            }
        }
        KeyCode::Backspace => match app.form_active_field {
            0 => { app.form_group.pop(); app.filter_form_groups(); }
            1 => { app.form_title.pop(); }
            2 if is_pass => { app.form_password.pop(); }
            2 => { app.form_username.pop(); }
            3 => { app.form_password.pop(); }
            4 => { app.form_url.pop(); }
            5 => { app.form_notes.pop(); }
            _ => {}
        },
        KeyCode::Char(c) => match app.form_active_field {
            0 => { app.form_group.push(c); app.filter_form_groups(); }
            1 => { app.form_title.push(c); }
            2 if is_pass => { app.form_password.push(c); }
            2 => { app.form_username.push(c); }
            3 => { app.form_password.push(c); }
            4 => { app.form_url.push(c); }
            5 => { app.form_notes.push(c); }
            _ => {}
        },
        _ => {}
    }
}

/// Texto mínimo de dicas de atalhos exibido na área de rodapé, por modo.
/// Mantido enxuto de propósito — a lista completa fica no modal de ajuda
/// (CTRL+?), que é sempre o atalho mais essencial a lembrar.
fn footer_hint_text(mode: AppMode) -> String {
    match mode {
        AppMode::Search => t!("ui.footer_search").to_string(),
        AppMode::Normal => t!("ui.footer_normal").to_string(),
        AppMode::ConfirmDelete => t!("common.yes_no_hint").to_string(),
        AppMode::Form => t!("ui.footer_form").to_string(),
        AppMode::Info => t!("ui.footer_info").to_string(),
        AppMode::ContextMenu => t!("ui.footer_context_menu").to_string(),
        AppMode::Help => t!("common.footer_close").to_string(),
        AppMode::RenameGroup => t!("ui.footer_rename").to_string(),
        _ => String::new(),
    }
}

/// Quebra `text` em linhas de até `max_width` colunas, respeitando quebras
/// de linha explícitas e quebrando por palavra quando uma linha não cabe.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split(' ') {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= max_width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        lines.push(current);
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

struct FooterContent {
    lines: Vec<String>,
    color: Color,
}

/// Decide o que mostrar na área de dicas: uma mensagem ativa (sucesso ou
/// erro, essa em vermelho — com contagem regressiva de limpeza do
/// clipboard quando aplicável) ou, na ausência dela, as dicas de atalhos
/// do modo atual.
fn build_footer_content(app: &mut App, hint_text: &str, width: usize) -> FooterContent {
    let mut active: Option<(String, Color)> = None;

    if let Some(msg) = app.message.clone() {
        let elapsed = msg.time.elapsed();
        let expired = match msg.clipboard_clear_secs {
            Some(secs) => elapsed.as_secs() >= secs,
            None => elapsed >= Duration::from_secs(3),
        };
        if expired {
            app.message = None;
        } else {
            let text = match msg.clipboard_clear_secs {
                Some(secs) => t!("ui.clipboard_clear_countdown", msg = msg.text, secs = secs.saturating_sub(elapsed.as_secs())).to_string(),
                None => msg.text,
            };
            let color = if msg.is_error { app.theme.alert_error } else { app.theme.alert_info };
            active = Some((text, color));
        }
    }

    match active {
        Some((text, color)) => FooterContent { lines: wrap_text(&text, width), color },
        None => FooterContent { lines: wrap_text(hint_text, width), color: app.theme.guidance },
    }
}

fn draw_ui(f: &mut Frame, app: &mut App) {
    app.term_size = f.size();
    let full = f.size();

    let hint_text = footer_hint_text(app.mode);
    let footer_width = full.width.saturating_sub(2) as usize;
    let footer = build_footer_content(app, &hint_text, footer_width);
    let footer_height = (footer.lines.len() as u16 + 2).max(3).min(full.height);

    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(footer_height)]).split(full);
    app.list_height = chunks[1].height as usize;
    app.search_rect = chunks[0];

    let (search_text, search_color) = if app.mode == AppMode::Search { (format!(" {}█ ", app.search_query), app.theme.annotation) } else { (format!(" {} ", app.search_query), app.theme.guidance) };
    let search_text = scroll_tail(&search_text, chunks[0].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(search_text).block(Block::default().title(t!("ui.search_title").to_string()).borders(Borders::ALL).style(Style::default().fg(search_color))), chunks[0]);

    let list_title = if app.mode == AppMode::Normal { t!("ui.mode_normal_title").to_string() } else { t!("ui.mode_search_title").to_string() };
    let list_color = if app.mode == AppMode::Normal { app.theme.title } else { app.theme.base };
    let items: Vec<ListItem> = app.filtered.iter().enumerate().map(|(i, e)| {
        let item = ListItem::new(e.as_str());
        if app.hover_index == Some(i) && app.list_state.selected() != Some(i) {
            item.style(Style::default().fg(app.theme.annotation).add_modifier(Modifier::UNDERLINED))
        } else {
            item
        }
    }).collect();
    let list_block = Block::default().title(list_title).borders(Borders::ALL).style(Style::default().fg(list_color));
    app.list_inner_rect = list_block.inner(chunks[1]);
    let list = List::new(items).block(list_block).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol(">> ");
    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    let footer_paragraph = Paragraph::new(footer.lines.join("\n"))
        .style(Style::default().fg(footer.color))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer_paragraph, chunks[2]);

    match app.mode {
        AppMode::ConfirmDelete => draw_confirm_delete_modal(f, app),
        AppMode::Form => draw_form_modal(f, app),
        AppMode::Info => draw_info_modal(f, app),
        AppMode::ContextMenu => draw_context_menu(f, app),
        AppMode::Help => draw_help_modal(f, app),
        AppMode::RenameGroup => draw_rename_group_modal(f, app),
        _ => {}
    }
}

fn draw_confirm_delete_modal(f: &mut Frame, app: &mut App) {
    let area = centered_fixed_rect(60, 5, f.size());
    app.confirm_delete_rect = area;
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(t!("ui.confirm_delete", entry = app.get_selected().unwrap_or_default()).to_string()).block(Block::default().title(t!("common.confirm_title").to_string()).borders(Borders::ALL).style(Style::default().fg(app.theme.important))).alignment(Alignment::Center), area);
}

fn draw_rename_group_modal(f: &mut Frame, app: &mut App) {
    let area = centered_fixed_rect(50, 6, f.size());
    app.rename_group_rect = area;
    f.render_widget(Clear, area);

    let block = Block::default().title(t!("ui.rename_group_title").to_string()).borders(Borders::ALL).style(Style::default().fg(app.theme.alert_info));
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let chunks = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(3), // Título
        Constraint::Length(1), // Footer ajuda
    ]).split(inner);

    let title_text = scroll_tail(&format!(" {}█", app.rename_group_title), chunks[0].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(title_text).block(Block::default().title(t!("common.title_label").to_string()).borders(Borders::ALL).style(Style::default().fg(app.theme.annotation))), chunks[0]);
    f.render_widget(Paragraph::new(t!("ui.footer_rename").to_string()).alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), chunks[1]);
}

fn draw_form_modal(f: &mut Frame, app: &mut App) {
    let is_pass = app.backend.kind() == BackendKind::Pass;
    let show_dropdown = app.form_active_field == 0 && !app.filtered_groups.is_empty();

    if is_pass {
        draw_form_modal_pass(f, app, show_dropdown);
        return;
    }

    let height = if show_dropdown { 29 } else { 24 };
    let area = centered_fixed_rect(70, height, f.size());
    app.form_rect = area;
    f.render_widget(Clear, area);

    let form_title = if app.form_is_edit { t!("common.edit_entry_title") } else { t!("common.new_entry_title") };
    let form_block = Block::default().title(form_title.to_string()).borders(Borders::ALL).style(Style::default().fg(app.theme.alert_info));
    f.render_widget(form_block.clone(), area);

    let inner_area = form_block.inner(area);

    let form_chunks = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(3), // Grupo
        Constraint::Length(if show_dropdown { 5 } else { 0 }), // Dropdown
        Constraint::Length(3), // Título
        Constraint::Length(3), // Usuário
        Constraint::Length(3), // Senha
        Constraint::Length(3), // URL
        Constraint::Length(5), // Notas
        Constraint::Length(1), // Espaçador pequeno
        Constraint::Length(1), // Footer ajuda
        Constraint::Min(0)     // Resto (vazio)
    ]).split(inner_area);

    let group_rect = form_chunks[0].union(form_chunks[1]);
    draw_form_group_field(f, app, group_rect, show_dropdown);

    let title_color = if app.form_active_field == 1 { app.theme.annotation } else { app.theme.base };
    let title_text = scroll_tail(&format!(" {}{}", app.form_title, if app.form_active_field == 1 { "█" } else { "" }), form_chunks[2].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(title_text).block(Block::default().title(t!("common.title_label").to_string()).borders(Borders::ALL).style(Style::default().fg(title_color))), form_chunks[2]);
    let user_color = if app.form_active_field == 2 { app.theme.annotation } else { app.theme.base };
    let user_text = scroll_tail(&format!(" {}{}", app.form_username, if app.form_active_field == 2 { "█" } else { "" }), form_chunks[3].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(user_text).block(Block::default().title(t!("common.username_label").to_string()).borders(Borders::ALL).style(Style::default().fg(user_color))), form_chunks[3]);
    let pass_color = if app.form_active_field == 3 { app.theme.annotation } else { app.theme.base };
    let hidden: String = app.form_password.chars().map(|_| '*').collect();
    let pass_cursor = if app.form_active_field == 3 { mask_cursor(app.form_password.chars().count()).to_string() } else { String::new() };
    let pass_text = scroll_tail(&format!(" {}{}", hidden, pass_cursor), form_chunks[4].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(pass_text).block(Block::default().title(t!("common.password_label").to_string()).borders(Borders::ALL).style(Style::default().fg(pass_color))), form_chunks[4]);
    let url_color = if app.form_active_field == 4 { app.theme.annotation } else { app.theme.base };
    let url_text = scroll_tail(&format!(" {}{}", app.form_url, if app.form_active_field == 4 { "█" } else { "" }), form_chunks[5].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(url_text).block(Block::default().title(t!("common.url_label").to_string()).borders(Borders::ALL).style(Style::default().fg(url_color))), form_chunks[5]);

    let notes_color = if app.form_active_field == 5 { app.theme.annotation } else { app.theme.base };
    let notes_block = Block::default().title(t!("common.notes_label").to_string()).borders(Borders::ALL).border_style(Style::default().fg(notes_color));
    let notes_inner = notes_block.inner(form_chunks[6]);
    f.render_widget(notes_block, form_chunks[6]);

    let cursor = if app.form_active_field == 5 { "█" } else { "" };
    let notes_display = format!("{}{}", app.form_notes, cursor);
    let wrapped = wrap_text(&notes_display, notes_inner.width as usize);
    let visible = notes_inner.height as usize;
    let start = wrapped.len().saturating_sub(visible.max(1));
    f.render_widget(Paragraph::new(wrapped[start..].join("\n")), notes_inner);

    f.render_widget(Paragraph::new(t!("ui.footer_form_full").to_string()).alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), form_chunks[8]);
}

/// Formulário reduzido para entradas do tipo `pass`: apenas Grupo, Título e
/// Senha — o formato do pass não tem Usuário/URL/Notas como campos próprios.
fn draw_form_modal_pass(f: &mut Frame, app: &mut App, show_dropdown: bool) {
    // Soma dos Constraint::Length abaixo (Grupo 3 + Dropdown 0/5 + Título 3 +
    // Senha 3 + espaçador 1 + footer 1) + 2 linhas de borda do modal. Um valor
    // menor faz o solver de layout do ratatui encolher o primeiro bloco
    // (Grupo) para caber, quebrando a borda dele.
    let height = if show_dropdown { 18 } else { 13 };
    let area = centered_fixed_rect(60, height, f.size());
    app.form_rect = area;
    f.render_widget(Clear, area);

    let form_title = if app.form_is_edit { t!("common.edit_entry_title") } else { t!("common.new_entry_title") };
    let form_block = Block::default().title(form_title.to_string()).borders(Borders::ALL).style(Style::default().fg(app.theme.alert_info));
    f.render_widget(form_block.clone(), area);

    let inner_area = form_block.inner(area);

    let form_chunks = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(3), // Grupo
        Constraint::Length(if show_dropdown { 5 } else { 0 }), // Dropdown
        Constraint::Length(3), // Título
        Constraint::Length(3), // Senha
        Constraint::Length(1), // Espaçador pequeno
        Constraint::Length(1), // Footer ajuda
        Constraint::Min(0)     // Resto (vazio)
    ]).split(inner_area);

    let group_rect = form_chunks[0].union(form_chunks[1]);
    draw_form_group_field(f, app, group_rect, show_dropdown);

    let title_color = if app.form_active_field == 1 { app.theme.annotation } else { app.theme.base };
    let title_text = scroll_tail(&format!(" {}{}", app.form_title, if app.form_active_field == 1 { "█" } else { "" }), form_chunks[2].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(title_text).block(Block::default().title(t!("common.title_label").to_string()).borders(Borders::ALL).style(Style::default().fg(title_color))), form_chunks[2]);

    let pass_color = if app.form_active_field == 2 { app.theme.annotation } else { app.theme.base };
    let hidden: String = app.form_password.chars().map(|_| '*').collect();
    let pass_cursor = if app.form_active_field == 2 { mask_cursor(app.form_password.chars().count()).to_string() } else { String::new() };
    let pass_text = scroll_tail(&format!(" {}{}", hidden, pass_cursor), form_chunks[3].width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(pass_text).block(Block::default().title(t!("common.password_label").to_string()).borders(Borders::ALL).style(Style::default().fg(pass_color))), form_chunks[3]);

    f.render_widget(Paragraph::new(t!("ui.footer_form_pass").to_string()).alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), form_chunks[5]);
}

fn draw_form_group_field(f: &mut Frame, app: &mut App, group_rect: Rect, show_dropdown: bool) {
    let group_block = Block::default().title(t!("common.group_label").to_string()).borders(Borders::ALL).border_style(Style::default().fg(if app.form_active_field == 0 { app.theme.annotation } else { app.theme.base }));
    f.render_widget(group_block, group_rect);
    let group_text = scroll_tail(&format!(" {}{}", app.form_group, if app.form_active_field == 0 { "█" } else { "" }), group_rect.width.saturating_sub(2) as usize);
    f.render_widget(Paragraph::new(group_text), Rect::new(group_rect.x + 1, group_rect.y + 1, group_rect.width - 2, 1));

    if show_dropdown {
        let items: Vec<ListItem> = app.filtered_groups.iter().map(|g| ListItem::new(g.as_str())).collect();
        let divider_color = if app.form_active_field == 0 { app.theme.annotation } else { app.theme.base };
        let list = List::new(items).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(divider_color))).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol("> ");
        f.render_stateful_widget(list, Rect::new(group_rect.x + 1, group_rect.y + 2, group_rect.width - 2, group_rect.height - 3), &mut app.form_group_state);
    }
}

/// Divide `line` em spans estilizados, invertendo a cor da parte que cai
/// dentro da seleção (se houver) na linha `line_idx` do campo.
fn spans_with_selection(line: &str, sel: TextSelection, line_idx: usize, base: Color, hl: Color) -> Vec<Span<'static>> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![Span::raw(String::new())];
    }

    let (start, end) = sel.normalized();
    let on_selected_line = !sel.is_empty() && start.line <= line_idx && line_idx <= end.line;
    if !on_selected_line {
        return vec![Span::styled(chars.iter().collect::<String>(), Style::default().fg(base))];
    }

    let sel_start = if line_idx == start.line { start.col.min(chars.len()) } else { 0 };
    let sel_end = if line_idx == end.line { end.col.min(chars.len()) } else { chars.len() };

    let mut spans = Vec::new();
    if sel_start > 0 {
        spans.push(Span::styled(chars[..sel_start].iter().collect::<String>(), Style::default().fg(base)));
    }
    if sel_end > sel_start {
        spans.push(Span::styled(chars[sel_start..sel_end].iter().collect::<String>(), Style::default().fg(base).bg(hl).add_modifier(Modifier::BOLD)));
    }
    if sel_end < chars.len() {
        spans.push(Span::styled(chars[sel_end..].iter().collect::<String>(), Style::default().fg(base)));
    }
    spans
}

fn draw_info_field(f: &mut Frame, app: &mut App, field: usize, label: &str, area: Rect) {
    let color = if app.info_active_field == field { app.theme.annotation } else { app.theme.base };
    let block = Block::default().title(format!(" {} ", label)).borders(Borders::ALL).border_style(Style::default().fg(color));
    let inner = block.inner(area);
    app.info_field_rects[field] = inner;
    f.render_widget(block, area);

    let text = if field == 0 { app.info_title.clone() } else { app.info_url.clone() };
    let spans = spans_with_selection(&text, app.info_selection[field], 0, app.theme.base, app.theme.annotation);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn draw_info_modal(f: &mut Frame, app: &mut App) {
    if app.backend.kind() == BackendKind::Pass {
        draw_info_modal_pass(f, app);
        return;
    }

    let area = centered_fixed_rect(74, 20, f.size());
    app.info_modal_rect = area;
    f.render_widget(Clear, area);

    let block = Block::default().title(t!("common.entry_info_title").to_string()).borders(Borders::ALL).border_style(Style::default().fg(app.theme.alert_info));
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let chunks = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(3), // Título
        Constraint::Length(3), // URL
        Constraint::Min(3),    // Notas
        Constraint::Length(1), // Rodapé
    ]).split(inner);

    let title_word = t!("common.title_word").to_string();
    draw_info_field(f, app, 0, &title_word, chunks[0]);
    draw_info_field(f, app, 1, "URL", chunks[1]);

    let notes_color = if app.info_active_field == 2 { app.theme.annotation } else { app.theme.base };
    let notes_block = Block::default().title(t!("common.notes_label").to_string()).borders(Borders::ALL).border_style(Style::default().fg(notes_color));
    let notes_inner = notes_block.inner(chunks[2]);
    app.info_field_rects[2] = notes_inner;
    f.render_widget(notes_block, chunks[2]);

    let notes_snapshot = app.info_notes.clone();
    let lines: Vec<&str> = if notes_snapshot.is_empty() { vec![""] } else { notes_snapshot.split('\n').collect() };
    let visible_height = notes_inner.height as usize;
    app.info_notes_scroll = app.info_notes_scroll.min(lines.len().saturating_sub(1));
    let start = app.info_notes_scroll;
    let end = (start + visible_height).min(lines.len());
    let out_lines: Vec<Line> = (start..end)
        .map(|i| Line::from(spans_with_selection(lines[i], app.info_selection[2], i, app.theme.base, app.theme.annotation)))
        .collect();
    f.render_widget(Paragraph::new(out_lines), notes_inner);

    f.render_widget(Paragraph::new(t!("ui.info_footer_full").to_string()).alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), chunks[3]);
}

/// Modal de informações reduzido para entradas `pass`: o formato não tem
/// URL/Notas como campos próprios, então só o Título é exibido.
fn draw_info_modal_pass(f: &mut Frame, app: &mut App) {
    let area = centered_fixed_rect(74, 6, f.size());
    app.info_modal_rect = area;
    f.render_widget(Clear, area);

    let block = Block::default().title(t!("common.entry_info_title").to_string()).borders(Borders::ALL).border_style(Style::default().fg(app.theme.alert_info));
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let chunks = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(3), // Título
        Constraint::Length(1), // Rodapé
    ]).split(inner);

    let title_word = t!("common.title_word").to_string();
    draw_info_field(f, app, 0, &title_word, chunks[0]);

    f.render_widget(Paragraph::new(t!("ui.info_footer_pass").to_string()).alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), chunks[1]);
}

fn draw_context_menu(f: &mut Frame, app: &mut App) {
    let width = 22u16;
    let height = 5u16;
    let x = app.context_menu_anchor.0.min(app.term_size.width.saturating_sub(width));
    let y = app.context_menu_anchor.1.min(app.term_size.height.saturating_sub(height));
    let area = Rect::new(x, y, width, height);
    app.context_menu_rect = area;
    f.render_widget(Clear, area);

    let block = Block::default().title(t!("common.actions_title").to_string()).borders(Borders::ALL).border_style(Style::default().fg(app.theme.title));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let add_new = t!("ui.context_add_new").to_string();
    let edit = t!("ui.context_edit").to_string();
    let delete = t!("ui.context_delete").to_string();
    let labels = [add_new.as_str(), edit.as_str(), delete.as_str()];
    app.context_menu_item_rects.clear();
    for (i, label) in labels.iter().enumerate() {
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        app.context_menu_item_rects.push(row);
        let style = if i == app.context_menu_selected {
            Style::default().fg(app.theme.base).add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(app.theme.base)
        };
        f.render_widget(Paragraph::new(format!(" {}", label)).style(style), row);
    }
}

fn draw_help_modal(f: &mut Frame, app: &mut App) {
    let nav_section = t!("help.nav_section").to_string();
    let actions_section = t!("help.actions_section").to_string();
    let search_section = t!("help.search_section").to_string();
    let mouse_section = t!("ui.help_mouse_section").to_string();
    let general_section = t!("help.general_section").to_string();
    let move_selection = t!("help.move_selection").to_string();
    let go_top_bottom = t!("help.go_top_bottom").to_string();
    let half_page = t!("help.half_page").to_string();
    let next_prev = t!("help.next_prev").to_string();
    let copy_password = t!("ui.help_copy_password").to_string();
    let view_details = t!("ui.help_view_details").to_string();
    let open_menu = t!("ui.help_open_menu").to_string();
    let add_entry = t!("ui.help_add_entry").to_string();
    let edit_entry = t!("ui.help_edit_entry").to_string();
    let delete_entry = t!("ui.help_delete_entry").to_string();
    let enter_search = t!("help.enter_search").to_string();
    let exit_search = t!("ui.help_exit_search").to_string();
    let click_label = t!("ui.help_click_label").to_string();
    let select_entry = t!("ui.help_select_entry").to_string();
    let double_click_label = t!("ui.help_double_click_label").to_string();
    let right_click_label = t!("ui.help_right_click_label").to_string();
    let context_menu_desc = t!("ui.help_context_menu_desc").to_string();
    let this_help = t!("help.this_help").to_string();
    let quit_fpass = t!("help.quit_fpass").to_string();
    let space_key = t!("common.space_key").to_string();

    let sections: [(&str, &[(&str, &str)]); 5] = [
        (&nav_section, &[
            ("j/k, ↑/↓", move_selection.as_str()),
            ("gg / G", go_top_bottom.as_str()),
            ("CTRL-U/D", half_page.as_str()),
            ("CTRL-N/P", next_prev.as_str()),
        ]),
        (&actions_section, &[
            ("ENTER", copy_password.as_str()),
            ("TAB", view_details.as_str()),
            (space_key.as_str(), open_menu.as_str()),
            ("CTRL-A", add_entry.as_str()),
            ("CTRL-E", edit_entry.as_str()),
            ("CTRL-X", delete_entry.as_str()),
        ]),
        (&search_section, &[
            ("/, f ou i", enter_search.as_str()),
            ("ESC", exit_search.as_str()),
        ]),
        (&mouse_section, &[
            (click_label.as_str(), select_entry.as_str()),
            (double_click_label.as_str(), copy_password.as_str()),
            (right_click_label.as_str(), context_menu_desc.as_str()),
        ]),
        (&general_section, &[
            ("CTRL+?", this_help.as_str()),
            ("CTRL-C", quit_fpass.as_str()),
        ]),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (title, items) in sections.iter() {
        if !lines.is_empty() { lines.push(Line::from("")); }
        lines.push(Line::from(Span::styled(*title, Style::default().fg(app.theme.title).add_modifier(Modifier::BOLD))));
        for (key, desc) in items.iter() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<16}", key), Style::default().fg(app.theme.annotation)),
                Span::styled(desc.to_string(), Style::default().fg(app.theme.base)),
            ]));
        }
    }

    let height = (lines.len() as u16 + 2).min(f.size().height);
    let area = centered_fixed_rect(56, height, f.size());
    app.help_modal_rect = area;
    f.render_widget(Clear, area);

    let block = Block::default().title(t!("common.shortcuts_title").to_string()).borders(Borders::ALL).border_style(Style::default().fg(app.theme.alert_info));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}
