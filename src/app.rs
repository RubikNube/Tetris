use crate::{
    config::LoadedConfig,
    game::{Action, Game},
    history::{Attempt, History},
};
use chrono::{SecondsFormat, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;
use uuid::Uuid;

pub const MIN_WIDTH: u16 = 72;
pub const MIN_HEIGHT: u16 = 26;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Playing,
    Paused,
    Help,
    QuitConfirm,
    GameOver,
    History,
    ExitConfirm,
}

pub struct App {
    pub settings: LoadedConfig,
    pub game: Option<Game>,
    pub screen: Screen,
    pub running: bool,
    pub menu_index: usize,
    pub size_ok: bool,
    pub result: Option<Attempt>,
    pub pending: Vec<Attempt>,
    pub error: Option<String>,
    pub top: Vec<Attempt>,
    pub recent: Vec<Attempt>,
    pub page: usize,
    pub history_recent: bool,
    pub has_next_page: bool,
    history: Option<History>,
    session_id: String,
    help_return: Screen,
    quit_return: Screen,
}
impl App {
    pub fn new(settings: LoadedConfig) -> Self {
        let mut app = Self {
            settings,
            game: None,
            screen: Screen::Menu,
            running: true,
            menu_index: 0,
            size_ok: true,
            result: None,
            pending: Vec::new(),
            error: None,
            top: Vec::new(),
            recent: Vec::new(),
            page: 0,
            history_recent: false,
            has_next_page: false,
            history: None,
            session_id: String::new(),
            help_return: Screen::Menu,
            quit_return: Screen::Playing,
        };
        app.open_history();
        app
    }
    fn open_history(&mut self) {
        if self.history.is_none() {
            match History::open(&self.settings.history_path) {
                Ok(h) => {
                    self.history = Some(h);
                    self.error = None;
                }
                Err(e) => self.error = Some(format!("History unavailable: {e:#}")),
            }
        }
    }
    pub fn retry_save(&mut self) {
        self.open_history();
        if let Some(history) = self.history.as_mut() {
            while let Some(attempt) = self.pending.first() {
                if let Err(e) = history.save(attempt) {
                    self.error = Some(format!("Score UNSAVED: {e:#}. Press r to retry."));
                    return;
                }
                self.pending.remove(0);
            }
            self.error = None;
        }
    }
    pub fn start_game(&mut self) {
        self.game = Some(Game::new(
            self.settings.config.game.start_level,
            rand::random(),
        ));
        self.session_id = Uuid::new_v4().to_string();
        self.result = None;
        self.screen = if self.size_ok {
            Screen::Playing
        } else {
            Screen::Paused
        };
    }
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.size_ok = width >= MIN_WIDTH && height >= MIN_HEIGHT;
        if !self.size_ok && self.screen == Screen::Playing {
            self.screen = Screen::Paused;
        }
    }
    pub fn advance(&mut self, elapsed: Duration) {
        if self.screen == Screen::Playing && self.size_ok {
            let game = self.game.as_mut().expect("playing has a game");
            game.advance(elapsed);
            if game.over {
                self.finish(true);
            }
        }
    }
    fn finish(&mut self, completed: bool) {
        let game = self.game.as_ref().expect("finishing has a game");
        let attempt = Attempt {
            id: self.session_id.clone(),
            finished_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            score: game.score,
            lines: game.lines,
            start_level: game.start_level,
            final_level: game.level(),
            duration_ms: game.elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            completed,
        };
        self.result = Some(attempt.clone());
        self.pending.push(attempt);
        self.retry_save();
        self.screen = Screen::GameOver;
    }
    fn request_exit(&mut self) {
        if self.pending.is_empty() {
            self.running = false;
        } else {
            self.screen = Screen::ExitConfirm;
        }
    }
    fn request_quit_game(&mut self, return_to: Screen) {
        self.quit_return = return_to;
        self.screen = Screen::QuitConfirm;
    }
    pub fn refresh_history(&mut self) {
        self.top.clear();
        self.recent.clear();
        self.has_next_page = false;
        self.open_history();
        let Some(history) = self.history.as_ref() else {
            return;
        };
        match (
            history.top(),
            history.recent(self.page),
            history.recent(self.page + 1),
        ) {
            (Ok(top), Ok(recent), Ok(next)) => {
                self.top = top;
                self.recent = recent;
                self.has_next_page = !next.is_empty();
            }
            (a, b, c) => {
                let e = a.err().or(b.err()).or(c.err()).expect("one query failed");
                self.error = Some(format!("Cannot read history: {e:#}"));
            }
        }
    }
    fn show_history(&mut self) {
        self.page = 0;
        self.history_recent = false;
        self.refresh_history();
        self.screen = Screen::History;
    }
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let action = self.settings.bindings.action(key);
        let repeat = key.kind == KeyEventKind::Repeat;
        if repeat
            && !(self.screen == Screen::Playing
                && matches!(
                    action,
                    Some(
                        Action::Left
                            | Action::Right
                            | Action::SoftDrop
                            | Action::RotateCw
                            | Action::RotateCcw
                    )
                ))
        {
            return;
        }
        let interrupt =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        match self.screen {
            Screen::Playing => match action {
                Some(Action::Quit) => self.request_quit_game(Screen::Playing),
                Some(Action::Pause) => self.screen = Screen::Paused,
                Some(Action::Help) => {
                    self.help_return = Screen::Playing;
                    self.screen = Screen::Help;
                }
                Some(action) => {
                    let game = self.game.as_mut().unwrap();
                    game.act(action);
                    if game.over {
                        self.finish(true);
                    }
                }
                None => {}
            },
            Screen::Paused => match action {
                Some(Action::Quit) => self.request_quit_game(Screen::Paused),
                Some(Action::Pause) if self.size_ok => self.screen = Screen::Playing,
                Some(Action::Help) => {
                    self.help_return = Screen::Paused;
                    self.screen = Screen::Help;
                }
                _ => {}
            },
            Screen::QuitConfirm => match key.code {
                KeyCode::Char('y' | 'Y') if !interrupt => self.finish(false),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                    self.screen = if self.quit_return == Screen::Playing && !self.size_ok {
                        Screen::Paused
                    } else {
                        self.quit_return
                    }
                }
                _ => {}
            },
            Screen::Help => {
                if interrupt || action == Some(Action::Quit) {
                    if matches!(self.help_return, Screen::Playing | Screen::Paused) {
                        self.request_quit_game(self.help_return);
                    } else {
                        self.screen = Screen::Menu;
                    }
                } else if matches!(key.code, KeyCode::Esc | KeyCode::Enter)
                    || action == Some(Action::Help)
                {
                    self.screen = if self.help_return == Screen::Playing && !self.size_ok {
                        Screen::Paused
                    } else {
                        self.help_return
                    };
                }
            }
            Screen::Menu => {
                if interrupt {
                    self.request_exit();
                    return;
                }
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.menu_index = (self.menu_index + 1) % 3
                    }
                    KeyCode::Char('k') | KeyCode::Up => self.menu_index = (self.menu_index + 2) % 3,
                    KeyCode::Enter => match self.menu_index {
                        0 => self.start_game(),
                        1 => self.show_history(),
                        _ => self.request_exit(),
                    },
                    KeyCode::Char('n') => self.start_game(),
                    KeyCode::Char('s') => self.show_history(),
                    KeyCode::Char('q') | KeyCode::Esc => self.request_exit(),
                    KeyCode::Char('?') => {
                        self.help_return = Screen::Menu;
                        self.screen = Screen::Help;
                    }
                    KeyCode::Char('r') => self.retry_save(),
                    _ => {}
                }
            }
            Screen::GameOver => {
                if interrupt {
                    self.screen = Screen::Menu;
                    return;
                }
                match key.code {
                    KeyCode::Char('n') => self.start_game(),
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                        self.screen = Screen::Menu
                    }
                    KeyCode::Char('s') => self.show_history(),
                    KeyCode::Char('r') => self.retry_save(),
                    _ => {}
                }
            }
            Screen::History => {
                if interrupt {
                    self.screen = Screen::Menu;
                    return;
                }
                match key.code {
                    KeyCode::Tab => self.history_recent = !self.history_recent,
                    KeyCode::Char('l') | KeyCode::Right if self.has_next_page => {
                        self.history_recent = true;
                        self.page += 1;
                        self.refresh_history();
                    }
                    KeyCode::Char('h') | KeyCode::Left if self.page > 0 => {
                        self.history_recent = true;
                        self.page -= 1;
                        self.refresh_history();
                    }
                    KeyCode::Char('r') => {
                        self.retry_save();
                        self.refresh_history();
                    }
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                        self.screen = Screen::Menu
                    }
                    _ => {}
                }
            }
            Screen::ExitConfirm => match key.code {
                KeyCode::Char('y' | 'Y') if !interrupt => self.running = false,
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.screen = Screen::Menu,
                KeyCode::Char('r') => {
                    self.retry_save();
                    if self.pending.is_empty() {
                        self.running = false;
                    }
                }
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn app(path: &Path) -> App {
        App::new(LoadedConfig::for_test(path))
    }
    fn press(app: &mut App, c: char) {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    #[test]
    fn pause_help_resize_and_quit_freeze_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = app(&dir.path().join("scores.db"));
        a.start_game();
        a.advance(Duration::from_millis(100));
        press(&mut a, 'p');
        a.advance(Duration::from_secs(5));
        assert_eq!(a.game.as_ref().unwrap().elapsed, Duration::from_millis(100));
        press(&mut a, 'p');
        press(&mut a, '?');
        a.advance(Duration::from_secs(10));
        press(&mut a, '?');
        assert_eq!(a.screen, Screen::Playing);
        a.set_size(20, 10);
        a.advance(Duration::from_secs(5));
        a.set_size(80, 30);
        assert_eq!(a.screen, Screen::Paused);
        press(&mut a, 'p');
        press(&mut a, 'q');
        a.advance(Duration::from_secs(20));
        press(&mut a, 'n');
        assert_eq!(a.screen, Screen::Playing);
        assert_eq!(a.game.as_ref().unwrap().elapsed, Duration::from_millis(100));
        press(&mut a, 'q');
        press(&mut a, 'y');
        assert_eq!(a.screen, Screen::GameOver);
        assert!(!a.result.as_ref().unwrap().completed);
        assert_eq!(a.history.as_ref().unwrap().recent(0).unwrap().len(), 1);
        assert!(a.history.as_ref().unwrap().top().unwrap().is_empty());
        a.retry_save();
        assert_eq!(a.history.as_ref().unwrap().recent(0).unwrap().len(), 1);
    }
    #[test]
    fn failed_scores_remain_queued_until_retry_and_exit_warns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scores.db");
        std::fs::write(&path, "bad").unwrap();
        let mut a = app(&path);
        a.start_game();
        press(&mut a, 'q');
        press(&mut a, 'y');
        assert_eq!(a.pending.len(), 1);
        assert!(a.error.is_some());
        a.screen = Screen::Menu;
        press(&mut a, 'q');
        assert_eq!(a.screen, Screen::ExitConfirm);
        assert!(a.running);
        std::fs::remove_file(&path).unwrap();
        a.retry_save();
        a.retry_save();
        assert!(a.pending.is_empty());
        assert_eq!(a.history.as_ref().unwrap().recent(0).unwrap().len(), 1);
    }
    #[test]
    fn repeated_discrete_actions_and_releases_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = app(&dir.path().join("scores.db"));
        a.start_game();
        let mut key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        key.kind = KeyEventKind::Repeat;
        a.handle_key(key);
        assert!(
            a.game
                .as_ref()
                .unwrap()
                .board
                .iter()
                .flatten()
                .all(Option::is_none)
        );
        key.code = KeyCode::Char('q');
        key.kind = KeyEventKind::Release;
        a.handle_key(key);
        assert_eq!(a.screen, Screen::Playing);
    }
}
