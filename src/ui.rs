use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::{io, time::Duration};

use crate::app::App;
use crate::util::centered_fixed_rect;
use crate::AppMode;

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut is_g_key = false;

                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if app.mode == AppMode::Search || app.mode == AppMode::Normal {
                        handle_ctrl_key(app, key.code);
                        continue;
                    } else if key.code == KeyCode::Char('c') {
                        return Ok(());
                    }
                }

                match app.mode {
                    AppMode::Search => handle_search_key(app, key.code),
                    AppMode::Normal => {
                        if handle_normal_key(app, key.code, &mut is_g_key) {
                            return Ok(());
                        }
                    }
                    AppMode::ConfirmDelete => handle_confirm_delete_key(app, key.code),
                    AppMode::Form => handle_form_key(app, key.code),
                    AppMode::PasswordInput | AppMode::ConfirmCreateDb | AppMode::CreateDb => {}
                }
                app.last_key_was_g = is_g_key;
            }
        }
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

    let footer_text = match app.mode {
        AppMode::Search => "CTRL-U/D: Meia Pág | ENTER: Copiar | CTRL-A/E/X: Ações | CTRL+C: Sair",
        AppMode::Normal => "gg/G: Topo/Fim | CTRL-U/D: Meia Pág | ENTER: Copiar | CTRL-A/E/X: Ações | ESC/q: Sair",
        AppMode::ConfirmDelete => "y: Sim | n/N: Não | CTRL+C: Sair",
        AppMode::Form => "TAB/SHIFT-TAB: Navegar | ENTER: Confirmar | ESC: Cancelar",
        _ => "",
    };
    if !footer_text.is_empty() {
        f.render_widget(Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL).style(Style::default().fg(app.theme.guidance))).alignment(Alignment::Center), chunks[2]);
    }

    if app.mode == AppMode::ConfirmDelete {
        draw_confirm_delete_modal(f, app);
    } else if app.mode == AppMode::Form {
        draw_form_modal(f, app);
    }

    draw_message_toast(f, app);
}

fn draw_confirm_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_fixed_rect(60, 5, f.size());
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("\nDeseja EXCLUIR '{}'? [y/N]", app.get_selected().unwrap_or_default())).block(Block::default().title(" Confirmar ").borders(Borders::ALL).style(Style::default().fg(app.theme.important))).alignment(Alignment::Center), area);
}

fn draw_form_modal(f: &mut Frame, app: &mut App) {
    let show_dropdown = app.form_active_field == 0 && !app.filtered_groups.is_empty();
    let height = if show_dropdown { 24 } else { 19 };
    let area = centered_fixed_rect(70, height, f.size());
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
