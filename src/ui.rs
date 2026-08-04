use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::{io, time::{Duration, Instant}};

use crate::app::{App, ContextAction, TextSelection};
use crate::util::centered_fixed_rect;
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
                AppMode::PasswordInput | AppMode::ConfirmCreateDb | AppMode::CreateDb => {}
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
        KeyCode::Char('a') => app.open_add_form(),
        KeyCode::Char('e') => {
            if let Some(entry) = app.get_selected() {
                app.open_edit_form(entry);
            }
        }
        KeyCode::Char('x') => {
            if app.get_selected().is_some() {
                app.hover_index = None;
                app.mode = AppMode::ConfirmDelete;
            }
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
        KeyCode::Char('/') | KeyCode::Char('f') => app.mode = AppMode::Search,
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
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = app.info_previous_mode,
        KeyCode::Tab => app.info_active_field = (app.info_active_field + 1) % 3,
        KeyCode::BackTab => app.info_active_field = if app.info_active_field == 0 { 2 } else { app.info_active_field - 1 },
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
    match code {
        KeyCode::Esc => app.mode = AppMode::Normal,
        KeyCode::BackTab => { app.form_active_field = if app.form_active_field == 0 { 4 } else { app.form_active_field - 1 }; }
        KeyCode::Tab => { app.form_active_field = (app.form_active_field + 1) % 5; }
        KeyCode::Down => { if app.form_active_field == 0 { app.form_next_group(); } }
        KeyCode::Up => { if app.form_active_field == 0 { app.form_prev_group(); } }
        KeyCode::Enter => {
            if let Some(idx) = (app.form_active_field == 0).then(|| app.form_group_state.selected()).flatten() {
                app.form_group = app.filtered_groups[idx].clone();
                app.form_active_field = 1;
            } else {
                app.submit_form();
            }
        }
        KeyCode::Backspace => match app.form_active_field {
            0 => { app.form_group.pop(); app.filter_form_groups(); }
            1 => { app.form_title.pop(); }
            2 => { app.form_username.pop(); }
            3 => { app.form_password.pop(); }
            4 => { app.form_url.pop(); }
            _ => {}
        },
        KeyCode::Char(c) => match app.form_active_field {
            0 => { app.form_group.push(c); app.filter_form_groups(); }
            1 => { app.form_title.push(c); }
            2 => { app.form_username.push(c); }
            3 => { app.form_password.push(c); }
            4 => { app.form_url.push(c); }
            _ => {}
        },
        _ => {}
    }
}

/// Texto mínimo de dicas de atalhos exibido na área de rodapé, por modo.
fn draw_ui(f: &mut Frame, app: &mut App) {
    app.term_size = f.size();
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)]).split(f.size());
    app.list_height = chunks[1].height as usize;
    app.search_rect = chunks[0];

    let (search_text, search_color) = if app.mode == AppMode::Search { (format!(" {}█ ", app.search_query), app.theme.annotation) } else { (format!(" {} ", app.search_query), app.theme.guidance) };
    f.render_widget(Paragraph::new(search_text).block(Block::default().title(" Pesquisar (/) ").borders(Borders::ALL).style(Style::default().fg(search_color))), chunks[0]);

    let list_title = if app.mode == AppMode::Normal { " NORMAL (j/k) " } else { " PESQUISA " };
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

    let footer_text = match app.mode {
        AppMode::Search => "CTRL-U/D: Meia Pág | ENTER: Copiar | TAB: Info | CTRL-A/E/X: Ações | CTRL+C: Sair",
        AppMode::Normal => "gg/G: Topo/Fim | CTRL-U/D: Meia Pág | ENTER: Copiar | TAB: Info | ESPAÇO/CTRL-A/E/X: Ações | ESC/q: Sair",
        AppMode::ConfirmDelete => "y: Sim | n/N: Não | CTRL+C: Sair",
        AppMode::Form => "TAB/SHIFT-TAB: Navegar | ENTER: Confirmar | ESC: Cancelar",
        AppMode::Info => "Clique e arraste para copiar | TAB: Alternar campo | ESC: Fechar",
        AppMode::ContextMenu => "j/k ou ↑/↓: Navegar | ENTER/Clique: Confirmar | ESC: Fechar",
        _ => "",
    };
    if !footer_text.is_empty() {
        f.render_widget(Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL).style(Style::default().fg(app.theme.guidance))).alignment(Alignment::Center), chunks[2]);
    }

    match app.mode {
        AppMode::ConfirmDelete => draw_confirm_delete_modal(f, app),
        AppMode::Form => draw_form_modal(f, app),
        AppMode::Info => draw_info_modal(f, app),
        AppMode::ContextMenu => draw_context_menu(f, app),
        _ => {}
    }

    draw_message_toast(f, app);
}

fn draw_confirm_delete_modal(f: &mut Frame, app: &mut App) {
    let area = centered_fixed_rect(60, 5, f.size());
    app.confirm_delete_rect = area;
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("\nDeseja EXCLUIR '{}'? [y/N]", app.get_selected().unwrap_or_default())).block(Block::default().title(" Confirmar ").borders(Borders::ALL).style(Style::default().fg(app.theme.important))).alignment(Alignment::Center), area);
}

fn draw_form_modal(f: &mut Frame, app: &mut App) {
    let show_dropdown = app.form_active_field == 0 && !app.filtered_groups.is_empty();
    let height = if show_dropdown { 24 } else { 19 };
    let area = centered_fixed_rect(70, height, f.size());
    app.form_rect = area;
    f.render_widget(Clear, area);

    let form_block = Block::default().title(if app.form_is_edit { " Editar Entrada " } else { " Nova Entrada " }).borders(Borders::ALL).style(Style::default().fg(app.theme.alert_info));
    f.render_widget(form_block.clone(), area);

    let inner_area = form_block.inner(area);

    let form_chunks = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(3), // Grupo
        Constraint::Length(if show_dropdown { 5 } else { 0 }), // Dropdown
        Constraint::Length(3), // Título
        Constraint::Length(3), // Usuário
        Constraint::Length(3), // Senha
        Constraint::Length(3), // URL
        Constraint::Length(1), // Espaçador pequeno
        Constraint::Length(1), // Footer ajuda
        Constraint::Min(0)     // Resto (vazio)
    ]).split(inner_area);

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

    f.render_widget(Paragraph::new("TAB/SHIFT-TAB: Navegar | ENTER: Confirmar").alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), form_chunks[7]);
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
    let area = centered_fixed_rect(74, 20, f.size());
    app.info_modal_rect = area;
    f.render_widget(Clear, area);

    let block = Block::default().title(" Informações da Entrada ").borders(Borders::ALL).border_style(Style::default().fg(app.theme.alert_info));
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let chunks = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(3), // Título
        Constraint::Length(3), // URL
        Constraint::Min(3),    // Notas
        Constraint::Length(1), // Rodapé
    ]).split(inner);

    draw_info_field(f, app, 0, "Título", chunks[0]);
    draw_info_field(f, app, 1, "URL", chunks[1]);

    let notes_color = if app.info_active_field == 2 { app.theme.annotation } else { app.theme.base };
    let notes_block = Block::default().title(" Notas ").borders(Borders::ALL).border_style(Style::default().fg(notes_color));
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

    f.render_widget(Paragraph::new("Clique e arraste em um campo para selecionar e copiar | TAB: Alternar | ESC: Fechar").alignment(Alignment::Center).style(Style::default().fg(app.theme.guidance)), chunks[3]);
}

fn draw_context_menu(f: &mut Frame, app: &mut App) {
    let width = 22u16;
    let height = 5u16;
    let x = app.context_menu_anchor.0.min(app.term_size.width.saturating_sub(width));
    let y = app.context_menu_anchor.1.min(app.term_size.height.saturating_sub(height));
    let area = Rect::new(x, y, width, height);
    app.context_menu_rect = area;
    f.render_widget(Clear, area);

    let block = Block::default().title(" Ações ").borders(Borders::ALL).border_style(Style::default().fg(app.theme.title));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let labels = ["Adicionar nova", "Editar", "Excluir"];
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

fn draw_message_toast(f: &mut Frame, app: &mut App) {
    if let Some((msg, time, is_error)) = &app.message {
        if time.elapsed() < Duration::from_secs(3) {
            let area = centered_fixed_rect(50, 5, f.size());
            f.render_widget(Clear, area);
            let title = if *is_error { " Erro " } else { " Sucesso " };
            f.render_widget(Paragraph::new(format!("\n{}", msg)).block(Block::default().title(title).borders(Borders::ALL).style(Style::default().fg(if *is_error { app.theme.alert_error } else { app.theme.alert_info }))).alignment(Alignment::Center), area);
        } else {
            app.message = None;
        }
    }
}
