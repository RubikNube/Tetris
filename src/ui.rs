use crate::{
    app::{App, MIN_HEIGHT, MIN_WIDTH, Screen},
    game::{HEIGHT, HIDDEN, Kind, WIDTH},
    history::Attempt,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, Wrap},
};

fn color(kind: Kind) -> Color {
    match kind {
        Kind::I => Color::Cyan,
        Kind::O => Color::Yellow,
        Kind::T => Color::Magenta,
        Kind::S => Color::Green,
        Kind::Z => Color::Red,
        Kind::J => Color::Blue,
        Kind::L => Color::LightYellow,
    }
}
fn piece_style(kind: Kind, enabled: bool) -> Style {
    if enabled {
        Style::default().fg(color(kind))
    } else {
        Style::default()
    }
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
fn duration(ms: u64) -> String {
    let secs = ms / 1000;
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(Paragraph::new(format!("Terminal too small: {}x{}.\nResize to at least {MIN_WIDTH}x{MIN_HEIGHT}.\nGame paused; use your pause key to resume.\nQuit confirmation: y / n.", area.width, area.height)).wrap(Wrap { trim: false }), area);
        return;
    }
    match app.screen {
        Screen::Menu | Screen::ExitConfirm => menu(frame, app),
        Screen::History => history(frame, app),
        Screen::Help if app.game.is_none() => menu(frame, app),
        _ => game(frame, app),
    }
    match app.screen {
        Screen::Paused => popup(frame, "Paused", format!("Press {} to resume.\nPress {} to quit this game.", app.settings.config.controls.pause.join(" / "), app.settings.config.controls.quit.join(" / ")), 56, 7),
        Screen::QuitConfirm => popup(frame, "Abandon this game?", "Record this attempt as abandoned?\nIt will not appear in high scores.\ny: confirm    n / Esc: cancel".into(), 56, 7),
        Screen::ExitConfirm => popup(frame, "Unsaved scores", format!("{} attempt(s) have not been saved.\nr: retry saving and exit\ny: exit and lose unsaved results\nn / Esc: return to menu", app.pending.len()), 58, 8),
        Screen::GameOver => {
            if let Some(result) = &app.result {
                let saved = if app.pending.iter().any(|a| a.id == result.id) { "UNSAVED - press r to retry" } else { "Saved to personal history" };
                popup(frame, if result.completed { "Game over" } else { "Game abandoned" }, format!("Score: {}   Lines: {}   Level: {}\nTime: {}\n{saved}\n\nn: new game   s: scores   Enter: menu", result.score, result.lines, result.final_level, duration(result.duration_ms)), 62, 10);
            }
        }
        Screen::Help => {
            let mut text = String::from("Clear lines to score; every 10 lines increases the level.\nHold once per piece. Ghost shows the hard-drop position.\n\n");
            for (_, name, keys) in app.settings.config.controls.entries() { text.push_str(&format!("{name:12} {}\n", keys.join(" / "))); }
            text.push_str("\nMenus: j/k or arrows, Enter to choose.\nHistory: Tab switches views; h/l changes recent page.\nEsc / Enter / help key: close help.");
            popup(frame, "How to play", text, 66, 23);
        }
        _ => {}
    }
}
fn popup(frame: &mut Frame, title: &str, text: String, width: u16, height: u16) {
    let rect = centered(frame.area(), width, height);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        rect,
    );
}
fn status(app: &App) -> String {
    let mut message = app.error.clone().unwrap_or_default();
    if !app.pending.is_empty() {
        message = format!(
            "UNSAVED: {} attempt(s). r: retry. {message}",
            app.pending.len()
        );
    }
    message
}
fn menu(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let mut lines = vec![
        Line::from("T E T R I S").style(Style::default().add_modifier(Modifier::BOLD)),
        Line::from("Rust terminal edition"),
        Line::from(""),
    ];
    for (i, label) in ["New game", "Score history", "Quit"].iter().enumerate() {
        lines.push(Line::from(format!(
            "{} {label}",
            if i == app.menu_index { ">" } else { " " }
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from("j/k or arrows: choose   Enter: select"),
        Line::from("n: play   s: scores   ?: help   q: quit"),
        Line::from("r: retry history / unsaved scores"),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Tetris")),
        centered(area, 62, 15),
    );
    let footer = Rect::new(area.x + 1, area.bottom() - 4, area.width - 2, 4);
    frame.render_widget(
        Paragraph::new(format!(
            "Config: {}\nHistory: {}\n{}",
            app.settings.config_path.display(),
            app.settings.history_path.display(),
            status(app)
        ))
        .wrap(Wrap { trim: false }),
        footer,
    );
}
fn game(frame: &mut Frame, app: &App) {
    let Some(game) = app.game.as_ref() else {
        menu(frame, app);
        return;
    };
    let area = frame.area();
    let rows = Layout::vertical([
        Constraint::Length(22),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .margin(1)
    .split(area);
    let board = centered(rows[0], 22, 22);
    let hold = Rect::new(board.x - 16, board.y, 14, 4);
    let next = Rect::new(board.right() + 2, board.y, 14, 16);
    let block = Block::default().title("Tetris").borders(Borders::ALL);
    let board_area = block.inner(board);
    frame.render_widget(block, board);
    let active = game.active.cells();
    let ghost = game.ghost().cells();
    let mut lines = Vec::new();
    for y in HIDDEN..HEIGHT {
        let mut spans = Vec::new();
        for x in 0..WIDTH {
            let position = (x as i32, y as i32);
            let cell = game.board[y][x];
            let (text, kind) = if cell.is_some() {
                ("[]", cell)
            } else if !game.over && active.contains(&position) {
                ("[]", Some(game.active.kind))
            } else if !game.over && app.settings.config.display.ghost && ghost.contains(&position) {
                ("::", Some(game.active.kind))
            } else {
                (" .", None)
            };
            let style = kind
                .map(|k| piece_style(k, app.settings.config.display.color))
                .unwrap_or_default();
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), board_area);
    let mut held = Vec::new();
    if let Some(kind) = game.held {
        preview(&mut held, kind, app.settings.config.display.color);
    } else {
        held.push(Line::from("--"));
    }
    frame.render_widget(
        Paragraph::new(held).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL)
                .title(if game.hold_used {
                    "Hold (used)"
                } else {
                    "Hold"
                }),
        ),
        hold,
    );
    let mut previews = Vec::new();
    for (index, &kind) in game.next.iter().take(5).enumerate() {
        if index > 0 {
            previews.push(Line::from(""));
        }
        preview(&mut previews, kind, app.settings.config.display.color);
    }
    frame.render_widget(
        Paragraph::new(previews)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Next")),
        next,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Score: {}  Lines: {}  Level: {}  Time: {}  {}: help",
            game.score,
            game.lines,
            game.level(),
            duration(game.elapsed.as_millis() as u64),
            app.settings.config.controls.help[0],
        ))
        .alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(status(app)).wrap(Wrap { trim: false }),
        rows[2],
    );
}
fn preview(lines: &mut Vec<Line<'static>>, kind: Kind, colors: bool) {
    let cells = kind.cells(0);
    for y in 0..2 {
        let text: String = (0..4)
            .map(|x| if cells.contains(&(x, y)) { "[]" } else { "  " })
            .collect();
        lines.push(Line::from(Span::styled(text, piece_style(kind, colors))));
    }
}
fn history(frame: &mut Frame, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(14),
        Constraint::Length(3),
        Constraint::Length(4),
    ])
    .margin(1)
    .split(frame.area());
    let title = if app.history_recent {
        format!("Recent attempts - page {}", app.page + 1)
    } else {
        "High scores - top 10 completed games".into()
    };
    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Personal history"),
        ),
        rows[0],
    );
    let entries = if app.history_recent {
        &app.recent
    } else {
        &app.top
    };
    let table_rows = entries.iter().enumerate().map(|(i, a)| {
        history_row(
            i + 1 + if app.history_recent { app.page * 10 } else { 0 },
            a,
        )
    });
    let widths = [
        Constraint::Length(3),
        Constraint::Min(19),
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Length(7),
        Constraint::Length(9),
    ];
    let table = Table::new(table_rows, widths)
        .header(
            Row::new([
                "#",
                "Finished (UTC)",
                "Score",
                "Lines",
                "Levels",
                "Time",
                "Status",
            ])
            .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(table, rows[1]);
    let message = if entries.is_empty() {
        "No games recorded in this view yet.\n"
    } else {
        ""
    };
    frame.render_widget(Paragraph::new(format!("{message}Tab: high scores / recent   h/l: recent page   r: retry/refresh\nEnter / Esc / q: menu")), rows[2]);
    frame.render_widget(
        Paragraph::new(status(app)).wrap(Wrap { trim: false }),
        rows[3],
    );
}
fn history_row(index: usize, a: &Attempt) -> Row<'static> {
    Row::new(vec![
        index.to_string(),
        a.finished_at.replace('T', " ").chars().take(19).collect(),
        a.score.to_string(),
        a.lines.to_string(),
        format!("{}-{}", a.start_level, a.final_level),
        duration(a.duration_ms),
        if a.completed {
            "completed"
        } else {
            "abandoned"
        }
        .into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LoadedConfig;
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn board_is_centered_with_compact_status_below_at_supported_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = LoadedConfig::for_test(&dir.path().join("history.db"));
        settings.config.controls.help = vec!["F".into()];
        let mut app = App::new(settings);
        app.start_game();
        app.game.as_mut().unwrap().board[HEIGHT - 1] = [Some(Kind::I); WIDTH];
        app.error = Some("History unavailable: test error".into());
        for (width, height) in [(MIN_WIDTH, MIN_HEIGHT), (100, 35), (101, 35)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| render(f, &app)).unwrap();
            let buffer = terminal.backend().buffer();
            let text_row = |y| -> String {
                (0..width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect()
            };
            let board_left = (width - 22) / 2;
            let board_right_margin = width - board_left - 22;
            assert!(board_left.abs_diff(board_right_margin) <= 1);
            assert_eq!(buffer.cell((board_left + 1, 21)).unwrap().symbol(), "[");
            assert_eq!(buffer.cell((board_left + 20, 21)).unwrap().symbol(), "]");
            let tiles = text_row(1);
            assert_eq!(buffer.cell((board_left + 1, 1)).unwrap().symbol(), "T");
            assert!(tiles.find("Hold").unwrap() < tiles.find("Tetris").unwrap());
            assert!(tiles.find("Next").unwrap() > tiles.find("Tetris").unwrap());
            let stats = text_row(23);
            assert_eq!(
                stats.trim(),
                "Score: 0  Lines: 0  Level: 1  Time: 0:00  F: help"
            );
            let leading = stats.len() - stats.trim_start().len();
            let trailing = stats.len() - stats.trim_end().len();
            assert!(leading.abs_diff(trailing) <= 1);
            assert!(text_row(24).contains("History unavailable: test error"));
            for y in 0..23 {
                assert!(!text_row(y).contains("Status"));
                assert!(!text_row(y).contains("Controls"));
            }
        }
    }
    #[test]
    fn renders_all_screens_and_tiny_terminals() {
        let dir = tempfile::tempdir().unwrap();
        let settings = LoadedConfig::for_test(&dir.path().join("history.db"));
        let mut app = App::new(settings);
        app.start_game();
        for (width, height) in [(72, 26), (100, 35), (10, 3), (1, 1)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for screen in [
                Screen::Menu,
                Screen::Playing,
                Screen::Paused,
                Screen::Help,
                Screen::QuitConfirm,
                Screen::GameOver,
                Screen::History,
                Screen::ExitConfirm,
            ] {
                app.screen = screen;
                terminal.draw(|f| render(f, &app)).unwrap();
            }
        }
    }
}
