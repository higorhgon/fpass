//! Desenho da interface (ratatui) — separado da lógica de negócio.

use crate::app::{App, AppMode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use std::time::Duration;

pub fn draw_ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.size());
    app.list_height = chunks[1].height as usize;

    draw_search_bar(f, app, chunks[0]);
    draw_entry_list(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    match app.mode {
        AppMode::ConfirmDelete => draw_confirm_delete(f, app),
        AppMode::Form => draw_form(f, app),
        AppMode::GpgUnlock => draw_gpg_unlock(f, app),
        _ => {}
    }

    draw_message(f, app);
}

fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let (text, color) = if app.mode == AppMode::Search {
        (format!(" {}█ ", app.search_query), app.theme.annotation)
    } else {
        (format!(" {} ", app.search_query), app.theme.guidance)
    };
    let widget = Paragraph::new(text).block(
        Block::default()
            .title(" Pesquisar (/) ")
            .borders(Borders::ALL)
            .style(Style::default().fg(color)),
    );
    f.render_widget(widget, area);
}

fn draw_entry_list(f: &mut Frame, app: &mut App, area: Rect) {
    let title = if app.mode == AppMode::Normal {
        " NORMAL (j/k) "
    } else {
        " PESQUISA "
    };
    let color = if app.mode == AppMode::Normal {
        app.theme.title
    } else {
        app.theme.base
    };
    let items: Vec<ListItem> = app.filtered.iter().map(|e| ListItem::new(e.as_str())).collect();
    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Style::default().fg(color)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol(">> ");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let text = match app.mode {
        AppMode::Search => "CTRL-U/D: Meia Pág | ENTER: Copiar | CTRL-A/E/X: Ações | CTRL+C: Sair",
        AppMode::Normal => {
            "gg/G: Topo/Fim | CTRL-U/D: Meia Pág | ENTER: Copiar | CTRL-A/E/X: Ações | ESC/q: Sair"
        }
        AppMode::ConfirmDelete => "y: Sim | n/N: Não | CTRL+C: Sair",
        AppMode::Form => "TAB/SHIFT-TAB: Navegar | ENTER: Confirmar | ESC: Cancelar",
        AppMode::GpgUnlock => "ENTER: Desbloquear",
    };
    let widget = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(app.theme.guidance)),
        )
        .alignment(Alignment::Center);
    f.render_widget(widget, area);
}

fn draw_confirm_delete(f: &mut Frame, app: &App) {
    let area = centered_fixed_rect(60, 5, f.size());
    f.render_widget(Clear, area);
    let widget = Paragraph::new(format!(
        "\nDeseja EXCLUIR '{}'? [y/N]",
        app.get_selected().unwrap_or_default()
    ))
    .block(
        Block::default()
            .title(" Confirmar ")
            .borders(Borders::ALL)
            .style(Style::default().fg(app.theme.important)),
    )
    .alignment(Alignment::Center);
    f.render_widget(widget, area);
}

fn draw_form(f: &mut Frame, app: &mut App) {
    let show_dropdown = app.form_active_field == 0 && !app.filtered_groups.is_empty();
    let height = if show_dropdown { 24 } else { 19 };
    let area = centered_fixed_rect(70, height, f.size());
    f.render_widget(Clear, area);

    let form_block = Block::default()
        .title(if app.form_is_edit { " Editar Entrada " } else { " Nova Entrada " })
        .borders(Borders::ALL)
        .style(Style::default().fg(app.theme.alert_info));
    f.render_widget(form_block.clone(), area);
    let inner = form_block.inner(area);

    let form_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                                    // Grupo
            Constraint::Length(if show_dropdown { 5 } else { 0 }),    // Dropdown
            Constraint::Length(3),                                    // Título
            Constraint::Length(3),                                    // Usuário
            Constraint::Length(3),                                    // Senha
            Constraint::Length(3),                                    // URL
            Constraint::Length(1),                                    // Espaçador
            Constraint::Length(1),                                    // Ajuda
            Constraint::Min(0),
        ])
        .split(inner);

    let group_rect = form_chunks[0].union(form_chunks[1]);
    let group_focus = app.form_active_field == 0;
    let group_block = Block::default()
        .title(" Grupo ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if group_focus { app.theme.annotation } else { app.theme.base }));
    f.render_widget(group_block, group_rect);
    f.render_widget(
        Paragraph::new(format!(" {}{}", app.form_group, cursor(group_focus))),
        Rect::new(group_rect.x + 1, group_rect.y + 1, group_rect.width - 2, 1),
    );

    if show_dropdown {
        let items: Vec<ListItem> = app
            .filtered_groups
            .iter()
            .map(|g| ListItem::new(g.as_str()))
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(app.theme.annotation)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");
        f.render_stateful_widget(
            list,
            Rect::new(
                group_rect.x + 1,
                group_rect.y + 2,
                group_rect.width - 2,
                group_rect.height - 3,
            ),
            &mut app.form_group_state,
        );
    }

    draw_form_field(f, app, form_chunks[2], " Título ", &app.form_title, 1, false);
    draw_form_field(f, app, form_chunks[3], " Usuário ", &app.form_username, 2, false);
    let hidden: String = app.form_password.chars().map(|_| '*').collect();
    draw_form_field(f, app, form_chunks[4], " Senha ", &hidden, 3, true);
    draw_form_field(f, app, form_chunks[5], " URL ", &app.form_url, 4, false);

    let help = Paragraph::new("TAB/SHIFT-TAB: Navegar | ENTER: Confirmar")
        .alignment(Alignment::Center)
        .style(Style::default().fg(app.theme.guidance));
    f.render_widget(help, form_chunks[7]);
}

fn draw_gpg_unlock(f: &mut Frame, app: &App) {
    let area = centered_fixed_rect(60, 8, f.size());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Desbloquear Cofre GPG ")
        .borders(Borders::ALL)
        .style(Style::default().fg(app.theme.alert_info));
    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    f.render_widget(
        Paragraph::new("Digite a senha GPG para desbloquear o cofre de senhas:")
            .style(Style::default().fg(app.theme.guidance)),
        chunks[0],
    );

    let hidden: String = app.unlock_input.chars().map(|_| '*').collect();
    let input = Paragraph::new(format!(" {}{}", hidden, "█"))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(app.theme.annotation));
    f.render_widget(input, chunks[1]);
}

fn draw_form_field(
    f: &mut Frame,
    app: &App,
    area: Rect,
    title: &str,
    value: &str,
    field_idx: usize,
    _masked: bool,
) {
    let focus = app.form_active_field == field_idx;
    let color = if focus { app.theme.annotation } else { app.theme.base };
    let widget = Paragraph::new(format!(" {}{}", value, cursor(focus))).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(Style::default().fg(color)),
    );
    f.render_widget(widget, area);
}

fn cursor(focused: bool) -> &'static str {
    if focused {
        "█"
    } else {
        ""
    }
}

fn draw_message(f: &mut Frame, app: &mut App) {
    if let Some((msg, time, is_error)) = &app.message {
        if time.elapsed() < Duration::from_secs(3) {
            let terminal_w = f.size().width.saturating_sub(4);
            let max_w = terminal_w.min(70).max(40) as usize;
            let lines = count_wrapped_lines(msg, max_w.saturating_sub(4));
            let h = (lines + 3).min(f.size().height.saturating_sub(2) as usize) as u16;
            let w = max_w.min(terminal_w as usize) as u16;
            let area = centered_fixed_rect(w, h, f.size());
            f.render_widget(Clear, area);
            let title = if *is_error { " Erro " } else { " Sucesso " };
            let color = if *is_error { app.theme.alert_error } else { app.theme.alert_info };
            let widget = Paragraph::new(format!("\n{}", msg))
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .style(Style::default().fg(color)),
                )
                .alignment(Alignment::Center)
                .wrap(ratatui::widgets::Wrap { trim: false });
            f.render_widget(widget, area);
        } else {
            app.message = None;
        }
    }
}

/// Estima quantas linhas o texto ocupará com wrap na largura `width`.
fn count_wrapped_lines(text: &str, width: usize) -> usize {
    if width == 0 {
        return text.lines().count();
    }
    text.lines()
        .map(|line| {
            let len = line.len();
            if len == 0 {
                1
            } else {
                (len + width - 1) / width
            }
        })
        .sum()
}

pub fn centered_fixed_rect(width: u16, height: u16, r: Rect) -> Rect {
    let col = r.width.saturating_sub(width) / 2;
    let row = r.height.saturating_sub(height) / 2;
    Rect::new(col, row, width.min(r.width), height.min(r.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_centers_and_clamps() {
        let outer = Rect::new(0, 0, 100, 40);
        let r = centered_fixed_rect(50, 10, outer);
        assert_eq!((r.x, r.y, r.width, r.height), (25, 15, 50, 10));

        // Não estoura quando a área pedida é maior que a tela
        let small = Rect::new(0, 0, 30, 5);
        let r = centered_fixed_rect(50, 10, small);
        assert_eq!((r.width, r.height), (30, 5));
    }
}
