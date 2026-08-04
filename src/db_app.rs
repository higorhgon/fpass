use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{io, time::Duration};
use zeroize::Zeroizing;

use crate::backend::{Backend, DbRef};
use crate::config::Theme;
use crate::keepass;
use crate::util::centered_fixed_rect;
use crate::AppMode;

pub struct DbApp {
    pub entries: Vec<DbRef>,
    pub filtered: Vec<DbRef>,
    pub search_query: String,
    pub list_state: ListState,
    pub mode: AppMode,
    pub last_key_was_g: bool,
    pub list_height: usize,
    pub theme: Theme,
    pub password_input: Zeroizing<String>,
    pub selected_db: Option<DbRef>,
    pub error_msg: Option<String>,
    pub new_db_name: String,
    pub new_db_password: Zeroizing<String>,
    pub create_db_active_field: usize,
    pub help_previous_mode: AppMode,
}

impl DbApp {
    fn new(dbs: Vec<DbRef>, theme: Theme) -> Self {
        let mode = if dbs.is_empty() { AppMode::ConfirmCreateDb } else { AppMode::Search };
        let mut app = Self {
            entries: dbs.clone(), filtered: dbs, search_query: String::new(),
            list_state: ListState::default(), mode, last_key_was_g: false,
            list_height: 10, theme,
            password_input: Zeroizing::new(String::new()), selected_db: None, error_msg: None,
            new_db_name: String::new(), new_db_password: Zeroizing::new(String::new()), create_db_active_field: 0,
            help_previous_mode: AppMode::Normal,
        };
        if !app.filtered.is_empty() { app.list_state.select(Some(0)); }

        // Se só existir um banco, já pula direto para o modal de senha dele!
        if app.entries.len() == 1 {
            app.selected_db = Some(app.entries[0].clone());
            app.mode = AppMode::PasswordInput;
        }

        app
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        self.filtered = if terms.is_empty() {
            self.entries.clone()
        } else {
            self.entries.iter().filter(|d| {
                let lower = d.path.to_lowercase();
                terms.iter().all(|t| lower.contains(t))
            }).cloned().collect()
        };
        self.list_state.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    fn next(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i >= self.filtered.len() - 1 { 0 } else { i + 1 }, None => 0 }; self.list_state.select(Some(i)); }
    fn previous(&mut self) { if self.filtered.is_empty() { return; } let i = match self.list_state.selected() { Some(i) => if i == 0 { self.filtered.len() - 1 } else { i - 1 }, None => 0 }; self.list_state.select(Some(i)); }
    fn go_to_top(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(0)); } }
    fn go_to_bottom(&mut self) { if !self.filtered.is_empty() { self.list_state.select(Some(self.filtered.len() - 1)); } }
    fn half_page_down(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some((i + step).min(self.filtered.len() - 1))); }
    fn half_page_up(&mut self) { if self.filtered.is_empty() { return; } let step = (self.list_height.saturating_sub(2) / 2).max(1); let i = self.list_state.selected().unwrap_or(0); self.list_state.select(Some(i.saturating_sub(step))); }
}

pub fn run_selection_tui(dbs: Vec<DbRef>, theme: Theme) -> Result<Option<Backend>, Box<dyn std::error::Error>> {
    enable_raw_mode()?; let mut stdout = io::stdout(); execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout); let mut terminal = Terminal::new(backend)?;
    let mut app = DbApp::new(dbs, theme);

    let selected = loop {
        terminal.draw(|f| draw_selection_ui(f, &mut app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut is_g_key = false;

                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('d') => app.half_page_down(), KeyCode::Char('u') => app.half_page_up(), KeyCode::Char('c') => break None,
                        KeyCode::Char('a') => {
                            if app.mode == AppMode::Normal || app.mode == AppMode::Search {
                                app.new_db_name.clear();
                                app.new_db_password = Zeroizing::new(String::new());
                                app.create_db_active_field = 0;
                                app.mode = AppMode::CreateDb;
                            }
                        }
                        // "CTRL+?" chega de formas diferentes conforme o terminal: sem o
                        // protocolo estendido de teclado, Ctrl+/ e Ctrl+Shift+/ (?) mandam o
                        // mesmo byte 0x1F, que o crossterm decodifica como Char('7')+CONTROL.
                        KeyCode::Char('?') | KeyCode::Char('/') | KeyCode::Char('7') => {
                            if app.mode == AppMode::Normal || app.mode == AppMode::Search {
                                app.help_previous_mode = app.mode;
                                app.mode = AppMode::Help;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                match app.mode {
                    AppMode::ConfirmCreateDb => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => { app.mode = AppMode::CreateDb; app.create_db_active_field = 0; }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => break None,
                        _ => {}
                    },
                    AppMode::CreateDb => match key.code {
                        KeyCode::Esc => app.mode = if app.entries.is_empty() { AppMode::ConfirmCreateDb } else { AppMode::Normal },
                        KeyCode::Tab | KeyCode::BackTab => { app.create_db_active_field = (app.create_db_active_field + 1) % 2; }
                        KeyCode::Enter => {
                            if app.create_db_active_field == 0 && !app.new_db_name.is_empty() {
                                app.create_db_active_field = 1;
                            } else if !app.new_db_name.is_empty() {
                                match keepass::create_database(&app.new_db_name, app.new_db_password.as_str()) {
                                    Ok(path) => {
                                        let db_path = path.to_string_lossy().to_string();
                                        break Some(Backend::Keepass { db_path, password: app.new_db_password.clone() });
                                    }
                                    Err(e) => {
                                        app.error_msg = Some(e);
                                        app.create_db_active_field = 0;
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            app.error_msg = None;
                            match app.create_db_active_field {
                                0 => { app.new_db_name.pop(); }
                                1 => { app.new_db_password.pop(); }
                                _ => {}
                            }
                        }
                        KeyCode::Char(c) => {
                            app.error_msg = None;
                            match app.create_db_active_field {
                                0 => { app.new_db_name.push(c); }
                                1 => { app.new_db_password.push(c); }
                                _ => {}
                            }
                        }
                        _ => {}
                    },
                    AppMode::PasswordInput => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal; app.password_input = Zeroizing::new(String::new()); app.error_msg = None; app.selected_db = None;
                        }
                        KeyCode::Enter => {
                            if let Some(db) = app.selected_db.clone() {
                                // Autentica em segundo plano sem fechar o TUI!
                                match Backend::open(&db, app.password_input.clone()) {
                                    Ok(backend) => break Some(backend),
                                    Err(e) => {
                                        app.error_msg = Some(e);
                                        app.password_input = Zeroizing::new(String::new());
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => { app.password_input.pop(); app.error_msg = None; }
                        KeyCode::Char(c) => { app.password_input.push(c); app.error_msg = None; }
                        _ => {}
                    },
                    AppMode::Search => match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal, KeyCode::Down => app.next(), KeyCode::Up => app.previous(),
                        KeyCode::Enter => { if let Some(i) = app.list_state.selected() { app.selected_db = Some(app.filtered[i].clone()); app.mode = AppMode::PasswordInput; } },
                        KeyCode::Backspace => { app.search_query.pop(); app.apply_filter(); } KeyCode::Char(c) => { app.search_query.push(c); app.apply_filter(); } _ => {}
                    },
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break None, KeyCode::Down | KeyCode::Char('j') => app.next(), KeyCode::Up | KeyCode::Char('k') => app.previous(), KeyCode::Char('/') | KeyCode::Char('f') => app.mode = AppMode::Search, KeyCode::Char('G') => app.go_to_bottom(),
                        KeyCode::Char('g') => { is_g_key = true; if app.last_key_was_g { app.go_to_top(); is_g_key = false; } }
                        KeyCode::Enter => { if let Some(i) = app.list_state.selected() { app.selected_db = Some(app.filtered[i].clone()); app.mode = AppMode::PasswordInput; } }, _ => {}
                    },
                    AppMode::Help => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => app.mode = app.help_previous_mode,
                        _ => {}
                    },
                    _ => {}
                }
                app.last_key_was_g = is_g_key;
            }
        }
    };
    disable_raw_mode()?; execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?; terminal.show_cursor()?; Ok(selected)
}

fn draw_selection_ui(f: &mut Frame, app: &mut DbApp) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)]).split(f.size());
    app.list_height = chunks[1].height as usize;

    let (search_text, search_color) = if app.mode == AppMode::Search { (format!(" {}█ ", app.search_query), app.theme.annotation) } else { (format!(" {} ", app.search_query), app.theme.guidance) };
    f.render_widget(Paragraph::new(search_text).block(Block::default().title(" Filtrar Banco (/) ").borders(Borders::ALL).style(Style::default().fg(search_color))), chunks[0]);

    let list_color = if app.mode == AppMode::Normal { app.theme.title } else { app.theme.base };
    let items: Vec<ListItem> = app.filtered.iter().map(|d| {
        ListItem::new(Line::from(vec![
            Span::styled(format!("[{}] ", d.kind.label()), Style::default().fg(app.theme.annotation)),
            Span::raw(d.path.as_str()),
        ]))
    }).collect();
    let list = List::new(items).block(Block::default().title(" Bancos Disponíveis ").borders(Borders::ALL).style(Style::default().fg(list_color))).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).highlight_symbol(">> ");
    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    // Apenas as dicas essenciais ficam visíveis aqui; a lista completa de
    // atalhos está no modal de ajuda (CTRL+?).
    let footer_text = match app.mode {
        AppMode::Search => "ENTER: Selecionar | CTRL+?: Ajuda | ESC: Normal",
        AppMode::ConfirmCreateDb => "y: Sim | n/N: Não",
        AppMode::CreateDb => "TAB: Navegar | ENTER: Criar | ESC: Cancelar",
        AppMode::PasswordInput => "ENTER: Confirmar | ESC: Cancelar",
        AppMode::Help => "ESC: Fechar",
        _ => "ENTER: Selecionar | CTRL+A: Novo | CTRL+?: Ajuda | ESC/q: Sair",
    };
    f.render_widget(Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL).style(Style::default().fg(app.theme.guidance))).alignment(Alignment::Center), chunks[2]);

    if app.mode == AppMode::PasswordInput {
        let modal_area = centered_fixed_rect(50, 7, f.size());
        f.render_widget(Clear, modal_area);

        let db_name = app.selected_db.as_ref().map(|d| d.path.as_str()).unwrap_or("");
        let db_short = std::path::Path::new(db_name).file_name().unwrap_or_default().to_string_lossy();

        let modal_color = if app.error_msg.is_some() { app.theme.alert_error } else { app.theme.alert_info };
        let modal_block = Block::default()
            .title(format!(" Desbloquear: {} ", db_short))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(modal_color));

        f.render_widget(modal_block.clone(), modal_area);

        let inner_area = modal_block.inner(modal_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(1)])
            .split(inner_area);

        let input_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(10), Constraint::Percentage(80), Constraint::Percentage(10)])
            .split(chunks[1]);

        let hidden_pw: String = app.password_input.chars().map(|_| '*').collect();
        let input_border_color = if app.error_msg.is_some() { app.theme.alert_error } else { app.theme.title };

        let input_field = Paragraph::new(format!(" {}{}", hidden_pw, "█"))
            .block(Block::default().borders(Borders::ALL).style(Style::default().fg(input_border_color)));

        f.render_widget(input_field, input_chunks[1]);

        let msg_text = if let Some(err) = &app.error_msg { err.as_str() } else { "ENTER: Confirmar | ESC: Cancelar" };
        let msg_color = if app.error_msg.is_some() { app.theme.alert_error } else { app.theme.guidance };

        let msg_paragraph = Paragraph::new(msg_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(msg_color));

        f.render_widget(msg_paragraph, chunks[2]);
    }

    if app.mode == AppMode::ConfirmCreateDb {
        let area = centered_fixed_rect(60, 5, f.size());
        f.render_widget(Clear, area);
        f.render_widget(Paragraph::new("\nNenhum banco de dados encontrado.\nDeseja criar um banco de dados KeePassXC? [y/N]").block(Block::default().title(" Confirmar ").borders(Borders::ALL).style(Style::default().fg(app.theme.alert_warn))).alignment(Alignment::Center), area);
    }

    if app.mode == AppMode::CreateDb {
        let modal_area = centered_fixed_rect(50, 10, f.size());
        f.render_widget(Clear, modal_area);

        let modal_block = Block::default().title(" Criar Novo Banco de Dados ").borders(Borders::ALL).border_style(Style::default().fg(app.theme.alert_info));
        f.render_widget(modal_block.clone(), modal_area);

        let inner_area = modal_block.inner(modal_area);
        let chunks = Layout::default().direction(Direction::Vertical).constraints([
            Constraint::Length(3), // Nome
            Constraint::Length(3), // Senha
            Constraint::Min(1),    // Footer/Erro
        ]).split(inner_area);

        let name_color = if app.create_db_active_field == 0 { app.theme.annotation } else { app.theme.base };
        let name_field = Paragraph::new(format!(" {}{}", app.new_db_name, if app.create_db_active_field == 0 { "█" } else { "" }))
            .block(Block::default().title(" Nome do Arquivo ").borders(Borders::ALL).style(Style::default().fg(name_color)));
        f.render_widget(name_field, chunks[0]);

        let pass_color = if app.create_db_active_field == 1 { app.theme.annotation } else { app.theme.base };
        let hidden_pw: String = app.new_db_password.chars().map(|_| '*').collect();
        let pass_field = Paragraph::new(format!(" {}{}", hidden_pw, if app.create_db_active_field == 1 { "█" } else { "" }))
            .block(Block::default().title(" Senha do Banco ").borders(Borders::ALL).style(Style::default().fg(pass_color)));
        f.render_widget(pass_field, chunks[1]);

        let (footer_text, footer_color) = if let Some(err) = &app.error_msg {
            (err.as_str(), app.theme.alert_error)
        } else {
            ("TAB: Navegar | ENTER: Criar | ESC: Cancelar", app.theme.guidance)
        };
        f.render_widget(Paragraph::new(footer_text).alignment(Alignment::Center).style(Style::default().fg(footer_color)), chunks[2]);
    }

    if app.mode == AppMode::Help {
        draw_help_modal(f, app);
    }
}

fn draw_help_modal(f: &mut Frame, app: &mut DbApp) {
    let sections: [(&str, &[(&str, &str)]); 4] = [
        ("NAVEGAÇÃO", &[
            ("j/k, ↑/↓", "Mover seleção"),
            ("gg / G", "Ir para o topo / fim"),
            ("CTRL-U/D", "Meia página"),
        ]),
        ("AÇÕES", &[
            ("ENTER", "Selecionar banco"),
            ("CTRL-A", "Criar novo banco"),
        ]),
        ("BUSCA", &[
            ("/ ou f", "Entrar em modo de busca"),
            ("ESC", "Sair da busca / Cancelar"),
        ]),
        ("GERAL", &[
            ("CTRL+?", "Esta ajuda"),
            ("CTRL-C", "Sair do fpass"),
        ]),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (title, items) in sections.iter() {
        if !lines.is_empty() { lines.push(Line::from("")); }
        lines.push(Line::from(Span::styled(*title, Style::default().fg(app.theme.title).add_modifier(Modifier::BOLD))));
        for (key, desc) in items.iter() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<16}", key), Style::default().fg(app.theme.annotation)),
                Span::styled(*desc, Style::default().fg(app.theme.base)),
            ]));
        }
    }

    let height = (lines.len() as u16 + 2).min(f.size().height);
    let area = centered_fixed_rect(50, height, f.size());
    f.render_widget(Clear, area);

    let block = Block::default().title(" Atalhos de Teclado ").borders(Borders::ALL).border_style(Style::default().fg(app.theme.alert_info));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}
