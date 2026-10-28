use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::event::{self, Event};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::{Duration, Instant},
};
use tetris::{
    app::App,
    config::{DEFAULT_CONFIG, LoadedConfig},
    ui,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Use this TOML file instead of the user configuration file.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Print the default TOML configuration and exit.
    #[arg(long)]
    print_default_config: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.print_default_config {
        print!("{DEFAULT_CONFIG}");
        return Ok(());
    }
    let settings = LoadedConfig::load(cli.config.as_deref())?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Tetris requires an interactive terminal. Use --help for options.");
    }
    let mut app = App::new(settings);
    // Install cleanup before initialization, including its partial-failure path.
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        old_hook(info);
    }));
    let result = run(&mut app);
    ratatui::restore();
    result
}
fn run(app: &mut App) -> Result<()> {
    let mut terminal = ratatui::try_init().context("cannot initialize terminal")?;
    let mut last_tick = Instant::now();
    while app.running {
        let size = terminal.size()?;
        app.set_size(size.width, size.height);
        let now = Instant::now();
        app.advance(now.duration_since(last_tick));
        last_tick = now;
        terminal.draw(|frame| ui::render(frame, app))?;
        let timeout = if app.screen == tetris::app::Screen::Playing {
            app.game
                .as_ref()
                .unwrap()
                .next_deadline()
                .min(Duration::from_millis(16))
        } else {
            Duration::from_millis(100)
        };
        if event::poll(timeout.saturating_sub(last_tick.elapsed()))? {
            let event = event::read()?;
            // Account for time spent polling before a key changes the current screen.
            let now = Instant::now();
            if let Event::Resize(width, height) = event {
                app.set_size(width, height);
            }
            app.advance(now.duration_since(last_tick));
            if let Event::Key(key) = event {
                app.handle_key(key);
            }
            // Disk I/O and menus must not become gravity catch-up on a new/resumed game.
            last_tick = Instant::now();
        }
    }
    Ok(())
}
