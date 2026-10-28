use crate::game::Action;
use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use directories::ProjectDirs;
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub const DEFAULT_CONFIG: &str = include_str!("../config.example.toml");

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub controls: Controls,
    pub game: GameConfig,
    pub display: DisplayConfig,
    pub history: HistoryConfig,
}
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub soft_drop: Vec<String>,
    pub rotate_cw: Vec<String>,
    pub rotate_ccw: Vec<String>,
    pub hard_drop: Vec<String>,
    pub hold: Vec<String>,
    pub pause: Vec<String>,
    pub help: Vec<String>,
    pub quit: Vec<String>,
}
impl Default for Controls {
    fn default() -> Self {
        fn keys(values: &[&str]) -> Vec<String> {
            values.iter().map(|s| (*s).into()).collect()
        }
        Self {
            left: keys(&["h", "Left"]),
            right: keys(&["l", "Right"]),
            soft_drop: keys(&["j", "Down"]),
            rotate_cw: keys(&["k", "Up"]),
            rotate_ccw: keys(&["z"]),
            hard_drop: keys(&["Space"]),
            hold: keys(&["c"]),
            pause: keys(&["p", "Esc"]),
            help: keys(&["?"]),
            quit: keys(&["q", "Ctrl+c"]),
        }
    }
}
impl Controls {
    pub fn entries(&self) -> [(Action, &str, &[String]); 10] {
        [
            (Action::Left, "left", &self.left),
            (Action::Right, "right", &self.right),
            (Action::SoftDrop, "soft_drop", &self.soft_drop),
            (Action::RotateCw, "rotate_cw", &self.rotate_cw),
            (Action::RotateCcw, "rotate_ccw", &self.rotate_ccw),
            (Action::HardDrop, "hard_drop", &self.hard_drop),
            (Action::Hold, "hold", &self.hold),
            (Action::Pause, "pause", &self.pause),
            (Action::Help, "help", &self.help),
            (Action::Quit, "quit", &self.quit),
        ]
    }
}
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GameConfig {
    pub start_level: u32,
}
impl Default for GameConfig {
    fn default() -> Self {
        Self { start_level: 1 }
    }
}
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayConfig {
    pub ghost: bool,
    pub color: bool,
}
impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            ghost: true,
            color: true,
        }
    }
}
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HistoryConfig {
    pub path: Option<PathBuf>,
}

pub struct LoadedConfig {
    pub config: Config,
    pub bindings: Bindings,
    pub config_path: PathBuf,
    pub history_path: PathBuf,
}
#[derive(Debug)]
pub struct Bindings(Vec<(KeyEvent, Action)>);
impl Bindings {
    pub fn action(&self, key: KeyEvent) -> Option<Action> {
        let key = normalize_key(key);
        self.0
            .iter()
            .find(|(bound, _)| bound.code == key.code && bound.modifiers == key.modifiers)
            .map(|(_, action)| *action)
    }
}
// Terminals disagree on the Shift flag for printable characters; the character itself carries case.
pub fn normalize_key(mut key: KeyEvent) -> KeyEvent {
    if matches!(key.code, KeyCode::Char(_)) {
        key.modifiers.remove(KeyModifiers::SHIFT);
    }
    key
}
pub fn parse_key(value: &str) -> Result<KeyEvent> {
    let mut rest = value;
    let mut modifiers = KeyModifiers::NONE;
    while rest.chars().count() > 1 {
        let Some((prefix, suffix)) = rest.split_once('+') else {
            break;
        };
        let flag = match prefix.to_ascii_lowercase().as_str() {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            _ => bail!("unknown key modifier in {value:?}; use Ctrl+ or Alt+"),
        };
        if modifiers.contains(flag) {
            bail!("duplicate modifier in {value:?}");
        }
        modifiers.insert(flag);
        rest = suffix;
    }
    let code = if rest.chars().count() == 1 {
        let c = rest.chars().next().unwrap();
        if c.is_control() {
            bail!("control characters must use named keys");
        }
        KeyCode::Char(c)
    } else {
        match rest.to_ascii_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "esc" | "escape" => KeyCode::Esc,
            "enter" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "backspace" => KeyCode::Backspace,
            _ => bail!("unknown key {value:?}"),
        }
    };
    Ok(normalize_key(KeyEvent::new(code, modifiers)))
}
impl Config {
    pub fn parse(source: &str) -> Result<Self> {
        let config: Self = toml::from_str(source).context("invalid TOML configuration")?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<Bindings> {
        if !(1..=20).contains(&self.game.start_level) {
            bail!("game.start_level must be between 1 and 20");
        }
        if self
            .history
            .path
            .as_ref()
            .is_some_and(|p| p.as_os_str().is_empty())
        {
            bail!("history.path cannot be empty");
        }
        let mut seen = HashSet::new();
        let mut bindings = Vec::new();
        for (action, name, keys) in self.controls.entries() {
            if keys.is_empty() {
                bail!("controls.{name} needs at least one key");
            }
            for value in keys {
                let key = parse_key(value).with_context(|| format!("invalid controls.{name}"))?;
                if key.code == KeyCode::Char('c')
                    && key.modifiers == KeyModifiers::CONTROL
                    && action != Action::Quit
                {
                    bail!("Ctrl+c is reserved for quitting");
                }
                if !seen.insert((key.code, key.modifiers)) {
                    bail!("conflicting or repeated key binding: {value}");
                }
                bindings.push((key, action));
            }
        }
        // Keep a reliable interrupt even when quit has been rebound.
        if !seen.contains(&(KeyCode::Char('c'), KeyModifiers::CONTROL)) {
            bindings.push((
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Action::Quit,
            ));
        }
        Ok(Bindings(bindings))
    }
}
impl LoadedConfig {
    #[cfg(test)]
    pub(crate) fn for_test(history_path: &Path) -> Self {
        let config = Config::default();
        let bindings = config.validate().unwrap();
        Self {
            config,
            bindings,
            config_path: history_path.with_file_name("config.toml"),
            history_path: history_path.to_path_buf(),
        }
    }
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "tetris")
            .context("cannot locate user config/data directories")?;
        Self::load_from(
            explicit,
            &dirs.config_dir().join("config.toml"),
            &dirs.data_local_dir().join("scores.sqlite3"),
        )
    }
    fn load_from(
        explicit: Option<&Path>,
        default_config: &Path,
        default_history: &Path,
    ) -> Result<Self> {
        let config_path = explicit.unwrap_or(default_config).to_path_buf();
        let config = match std::fs::read_to_string(&config_path) {
            Ok(source) => Config::parse(&source)
                .with_context(|| format!("configuration {}", config_path.display()))?,
            Err(e) if explicit.is_none() && e.kind() == std::io::ErrorKind::NotFound => {
                Config::default()
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("cannot read configuration {}", config_path.display())
                });
            }
        };
        let bindings = config.validate()?;
        let history_path = match config.history.path.as_ref() {
            Some(path) if path.is_absolute() => path.clone(),
            Some(path) => config_path.parent().unwrap_or(Path::new(".")).join(path),
            None => default_history.to_path_buf(),
        };
        Ok(Self {
            config,
            bindings,
            config_path,
            history_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn example_matches_defaults_and_partial_overrides_work() {
        let a = Config::parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(a.controls.left, Config::default().controls.left);
        let b = Config::parse("[game]\nstart_level = 8\n[display]\ncolor = false").unwrap();
        assert_eq!(b.game.start_level, 8);
        assert!(b.display.ghost);
        assert!(!b.display.color);
    }
    #[test]
    fn rejects_bad_configuration() {
        for source in [
            "[",
            "typo = 1",
            "[display]\nghots = true",
            "[game]\nstart_level = 0",
            "[game]\nstart_level = 21",
            "[controls]\nleft = []",
            "[controls]\nleft = ['j']",
            "[controls]\nleft = ['Bogus']",
            "[controls]\nleft = ['Ctrl+c']",
            "[history]\npath = ''",
        ] {
            assert!(Config::parse(source).is_err(), "accepted {source}");
        }
    }
    #[test]
    fn parses_keys_and_normalizes_shift() {
        let b = Config::default().validate().unwrap();
        assert_eq!(
            b.action(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            Some(Action::Help)
        );
        assert_eq!(
            parse_key("Ctrl+c").unwrap().modifiers,
            KeyModifiers::CONTROL
        );
        assert_eq!(parse_key("Alt+x").unwrap().modifiers, KeyModifiers::ALT);
        assert!(parse_key("Ctrl+Ctrl+c").is_err());
        assert_eq!(parse_key("+").unwrap().code, KeyCode::Char('+'));
        assert_eq!(parse_key("Ctrl++").unwrap().code, KeyCode::Char('+'));
    }
    #[test]
    fn paths_and_missing_file_policy() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let history = dir.path().join("default.sqlite3");
        assert!(LoadedConfig::load_from(None, &config, &history).is_ok());
        assert!(LoadedConfig::load_from(Some(&config), &config, &history).is_err());
        std::fs::write(&config, "[history]\npath = 'custom.sqlite3'").unwrap();
        let loaded = LoadedConfig::load_from(Some(&config), &config, &history).unwrap();
        assert_eq!(loaded.history_path, dir.path().join("custom.sqlite3"));
    }
}
