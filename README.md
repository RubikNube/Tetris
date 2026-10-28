# Tetris

A Linux-first Rust terminal game with Vim controls, TOML configuration, and a
persistent personal high-score history. Includes seven-bag randomization, SRS wall
kicks, hold, a ghost piece, and five upcoming pieces.

## Run

Install a current stable Rust toolchain and a C compiler (for bundled SQLite), then:

```sh
cargo run --release
```

Or install the executable:

```sh
cargo install --path . --locked
tetris
```

Use an interactive terminal at least **72 columns × 26 rows**. Linux is the verified
platform; the terminal and storage libraries are portable, but macOS and Windows
have not been verified. Blocks use ASCII characters, so no special font is needed.

## Controls

| Action | Default keys |
| --- | --- |
| Move left / right | `h` / `l`, Left / Right |
| Soft drop | `j`, Down |
| Rotate clockwise | `k`, Up |
| Rotate counterclockwise | `z` |
| Hard drop | Space |
| Hold / exchange piece | `c` |
| Pause / resume | `p`, Esc |
| Show / close help | `?` |
| Quit current game | `q`, Ctrl-C |

In menus, use `j`/`k` or arrows and Enter. The main menu also accepts `n` for a new
game, `s` for scores, and `q` to exit. Help closes with Esc or Enter as well.
Quitting a game asks for `y`/`n` confirmation and records confirmed quits as
**abandoned**. Ctrl-C follows this flow instead of abruptly leaving raw mode.

Resizing below the minimum pauses play. Enlarge the terminal and press your pause
key to resume. Pauses, help, and confirmation screens stop both gravity and the
recorded game clock. Hold is available once per piece and refreshes after locking.

## Configuration

Configuration is optional. On Linux the default location is
`$XDG_CONFIG_HOME/tetris/config.toml`, or `~/.config/tetris/config.toml` when that
variable is unset. The file is read once at startup. Missing settings use defaults.

```sh
mkdir -p ~/.config/tetris
tetris --print-default-config > ~/.config/tetris/config.toml
# Or select a different file:
tetris --config ./my-tetris.toml
```

See [config.example.toml](config.example.toml) for every setting. For example:

```toml
[controls]
left = ["a", "Left"]
right = ["d", "Right"]

[game]
start_level = 3

[display]
ghost = true
color = false

[history]
path = "scores.sqlite3"
```

Bindings are lists of single printable characters or named keys: `Space`, `Left`,
`Right`, `Up`, `Down`, `Esc`/`Escape`, `Enter`, `Tab`, and `Backspace`. Named keys are
case insensitive; characters preserve case. `Ctrl+` and `Alt+` modifiers are
supported, for example `Ctrl+x`. Ctrl-C always remains available for quitting and
cannot be assigned to another action. Shifted printable characters are written as
the resulting character, such as `?` or `K`, rather than `Shift+...`.

Each action requires at least one binding. Unknown settings, malformed keys,
duplicate/conflicting bindings, empty history paths, and starting levels outside
1–20 are rejected before terminal mode is entered. An explicitly selected missing
configuration file is an error. Menu and confirmation keys stay fixed; gameplay
keys are configurable. The compact status line shows the configured help key; the
help overlay lists all configured bindings.

The history path can be absolute or relative to the configuration file's directory.
`~` and environment variables inside TOML paths are not expanded.

## Score history

On Linux, history defaults to `$XDG_DATA_HOME/tetris/scores.sqlite3`, or
`~/.local/share/tetris/scores.sqlite3`. It is independent of the working directory.
SQLite is bundled; no database service or separate SQLite installation is needed.

Open **Score history** from the menu. Tab switches between the top ten completed
games and recent attempts; `h`/`l` or Left/Right page through recent attempts.
Each entry records its UTC completion date, score, cleared lines, starting and final
level, active duration, and completed/abandoned status. High scores sort by score
descending and completion time ascending. Abandoned attempts appear only in recent
history. A completed game means one that reached game over.

Writes are transactional and retries use the same unique attempt ID. If history is
unavailable, play remains available and unsaved attempts stay in memory. Press `r`
on the menu, result, or history screen to retry. Exiting with pending attempts asks
whether to retry, return, or leave without saving. An unreadable database is never
automatically replaced. Pending attempts are not durable until a save succeeds;
unfinished games cannot be resumed after exit.

## Rules and scoring

- The visible board is 10 × 20 cells, with four hidden spawn rows. Pieces spawn
  above the visible board. A blocked spawn or blocks remaining in hidden rows after
  locking and clearing lines ends the game.
- One, two, three, or four simultaneous cleared lines award 100, 300, 500, or 800
  points multiplied by the level before that clear.
- Soft drop awards one point per cell; hard drop awards two and locks immediately.
- Every ten cleared lines adds a level to the configured starting level. Gravity is
  `max(50, floor(1000 × 0.8^(level − 1)))` milliseconds per cell.
- A grounded piece locks after 500 ms. Successful grounded moves/rotations reset
  the timer at most 15 times per piece. O-piece rotation does not reset it.
- No T-spin, combo, back-to-back bonuses, multiplayer, or online accounts in v1.

## Development

The engine takes explicit actions, elapsed durations, and a random seed; it has no
terminal or filesystem dependencies. The application controller owns modal state
and persistence. Ratatui/Crossterm handle rendering and input; Serde/TOML handle
configuration; SQLite stores the history with a versioned schema.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests cover deterministic gameplay and timing, SRS kicks, scoring, configuration
validation, SQLite durability/ranking/retries, UI state transitions, terminal-size
rendering, and command-line errors. To try the UI, run `cargo run`, move/drop with
Vim and arrow keys, pause/help, resize smaller and back, abandon one game and finish
another, then relaunch and check both history views.
