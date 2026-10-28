use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use std::{collections::VecDeque, time::Duration};

pub const WIDTH: usize = 10;
pub const HEIGHT: usize = 24;
pub const HIDDEN: usize = 4;
const LOCK_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Left,
    Right,
    SoftDrop,
    RotateCw,
    RotateCcw,
    HardDrop,
    Hold,
    Pause,
    Help,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}
impl Kind {
    pub const ALL: [Self; 7] = [
        Self::I,
        Self::O,
        Self::T,
        Self::S,
        Self::Z,
        Self::J,
        Self::L,
    ];
    pub fn letter(self) -> char {
        match self {
            Self::I => 'I',
            Self::O => 'O',
            Self::T => 'T',
            Self::S => 'S',
            Self::Z => 'Z',
            Self::J => 'J',
            Self::L => 'L',
        }
    }
    pub fn cells(self, rotation: u8) -> [(i32, i32); 4] {
        let mut cells = match self {
            Self::I => [(0, 1), (1, 1), (2, 1), (3, 1)],
            Self::O => [(1, 0), (2, 0), (1, 1), (2, 1)],
            Self::T => [(1, 0), (0, 1), (1, 1), (2, 1)],
            Self::S => [(1, 0), (2, 0), (0, 1), (1, 1)],
            Self::Z => [(0, 0), (1, 0), (1, 1), (2, 1)],
            Self::J => [(0, 0), (0, 1), (1, 1), (2, 1)],
            Self::L => [(2, 0), (0, 1), (1, 1), (2, 1)],
        };
        if self != Self::O {
            let edge = if self == Self::I { 3 } else { 2 };
            for _ in 0..rotation % 4 {
                for (x, y) in &mut cells {
                    (*x, *y) = (edge - *y, *x);
                }
            }
        }
        cells
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Piece {
    pub kind: Kind,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
}
impl Piece {
    fn spawn(kind: Kind) -> Self {
        Self {
            kind,
            rotation: 0,
            x: 3,
            y: 2,
        }
    }
    pub fn cells(self) -> [(i32, i32); 4] {
        self.kind
            .cells(self.rotation)
            .map(|(x, y)| (x + self.x, y + self.y))
    }
}

pub struct Game {
    pub board: [[Option<Kind>; WIDTH]; HEIGHT],
    pub active: Piece,
    pub held: Option<Kind>,
    pub hold_used: bool,
    pub next: VecDeque<Kind>,
    pub score: u64,
    pub lines: u32,
    pub start_level: u32,
    pub elapsed: Duration,
    pub over: bool,
    rng: StdRng,
    gravity_elapsed: Duration,
    lock_elapsed: Duration,
    lock_resets: u8,
}

impl Game {
    pub fn new(start_level: u32, seed: u64) -> Self {
        let mut game = Self {
            board: [[None; WIDTH]; HEIGHT],
            active: Piece::spawn(Kind::I),
            held: None,
            hold_used: false,
            next: VecDeque::new(),
            score: 0,
            lines: 0,
            start_level,
            elapsed: Duration::ZERO,
            over: false,
            rng: StdRng::seed_from_u64(seed),
            gravity_elapsed: Duration::ZERO,
            lock_elapsed: Duration::ZERO,
            lock_resets: 0,
        };
        let kind = game.take_next();
        game.active = Piece::spawn(kind);
        game
    }
    pub fn level(&self) -> u32 {
        self.start_level + self.lines / 10
    }
    pub fn gravity_interval(&self) -> Duration {
        Duration::from_millis((1000.0 * 0.8_f64.powf(f64::from(self.level() - 1))).max(50.0) as u64)
    }
    fn take_next(&mut self) -> Kind {
        while self.next.len() < 8 {
            let mut bag = Kind::ALL;
            bag.shuffle(&mut self.rng);
            self.next.extend(bag);
        }
        self.next.pop_front().expect("bag is filled")
    }
    pub fn fits(&self, piece: Piece) -> bool {
        piece.cells().iter().all(|&(x, y)| {
            x >= 0
                && x < WIDTH as i32
                && y >= 0
                && y < HEIGHT as i32
                && self.board[y as usize][x as usize].is_none()
        })
    }
    fn grounded(&self) -> bool {
        !self.fits(Piece {
            y: self.active.y + 1,
            ..self.active
        })
    }
    pub fn ghost(&self) -> Piece {
        let mut ghost = self.active;
        while self.fits(Piece {
            y: ghost.y + 1,
            ..ghost
        }) {
            ghost.y += 1;
        }
        ghost
    }
    fn reset_piece_timers(&mut self) {
        self.gravity_elapsed = Duration::ZERO;
        self.lock_elapsed = Duration::ZERO;
        self.lock_resets = 0;
    }
    fn spawn(&mut self, kind: Kind) {
        self.active = Piece::spawn(kind);
        self.reset_piece_timers();
        self.over = !self.fits(self.active);
    }
    fn reset_lock(&mut self, was_grounded: bool) {
        if was_grounded && self.lock_resets < 15 {
            self.lock_elapsed = Duration::ZERO;
            self.lock_resets += 1;
        }
    }
    fn translate(&mut self, dx: i32, dy: i32) -> bool {
        let candidate = Piece {
            x: self.active.x + dx,
            y: self.active.y + dy,
            ..self.active
        };
        if !self.fits(candidate) {
            return false;
        }
        let grounded = self.grounded();
        self.active = candidate;
        if dx != 0 {
            self.reset_lock(grounded);
        }
        true
    }
    fn rotate(&mut self, clockwise: bool) {
        if self.active.kind == Kind::O {
            return;
        }
        let old = self.active.rotation;
        let new = (old + if clockwise { 1 } else { 3 }) % 4;
        let grounded = self.grounded();
        for &(dx, dy) in kicks(self.active.kind, old, new) {
            let candidate = Piece {
                rotation: new,
                x: self.active.x + dx,
                y: self.active.y - dy,
                ..self.active
            };
            if self.fits(candidate) {
                self.active = candidate;
                self.reset_lock(grounded);
                return;
            }
        }
    }
    pub fn act(&mut self, action: Action) {
        if self.over {
            return;
        }
        match action {
            Action::Left => {
                self.translate(-1, 0);
            }
            Action::Right => {
                self.translate(1, 0);
            }
            Action::SoftDrop => {
                if self.translate(0, 1) {
                    self.score += 1;
                }
            }
            Action::RotateCw => self.rotate(true),
            Action::RotateCcw => self.rotate(false),
            Action::HardDrop => {
                let ghost = self.ghost();
                self.score += (ghost.y - self.active.y) as u64 * 2;
                self.active = ghost;
                self.lock();
            }
            Action::Hold if !self.hold_used => {
                let current = self.active.kind;
                let replacement = self
                    .held
                    .replace(current)
                    .unwrap_or_else(|| self.take_next());
                self.spawn(replacement);
                self.hold_used = true;
            }
            _ => {}
        }
    }
    fn lock(&mut self) {
        for (x, y) in self.active.cells() {
            self.board[y as usize][x as usize] = Some(self.active.kind);
        }
        let level = self.level();
        let mut dest = HEIGHT;
        for src in (0..HEIGHT).rev() {
            if self.board[src].iter().any(Option::is_none) {
                dest -= 1;
                self.board[dest] = self.board[src];
            }
        }
        let cleared = dest;
        for row in &mut self.board[..dest] {
            *row = [None; WIDTH];
        }
        self.score += [0, 100, 300, 500, 800][cleared] * u64::from(level);
        self.lines += cleared as u32;
        if self.board[..HIDDEN].iter().flatten().any(Option::is_some) {
            self.over = true;
            return;
        }
        let kind = self.take_next();
        self.spawn(kind);
        self.hold_used = false;
    }
    pub fn next_deadline(&self) -> Duration {
        let gravity = self.gravity_interval().saturating_sub(self.gravity_elapsed);
        if self.grounded() {
            gravity.min(LOCK_DELAY.saturating_sub(self.lock_elapsed))
        } else {
            gravity
        }
    }
    /// Advance only active game time. The controller excludes pauses and modal screens.
    pub fn advance(&mut self, mut remaining: Duration) {
        if self.over {
            return;
        }
        while !remaining.is_zero() && !self.over {
            let grounded = self.grounded();
            let to_gravity = self.gravity_interval().saturating_sub(self.gravity_elapsed);
            let to_lock = if grounded {
                LOCK_DELAY.saturating_sub(self.lock_elapsed)
            } else {
                Duration::MAX
            };
            let step = remaining.min(to_gravity).min(to_lock);
            self.elapsed += step;
            self.gravity_elapsed += step;
            if grounded {
                self.lock_elapsed += step;
            }
            remaining -= step;
            if grounded && self.lock_elapsed >= LOCK_DELAY {
                self.lock();
            } else if self.gravity_elapsed >= self.gravity_interval() {
                self.gravity_elapsed = Duration::ZERO;
                self.translate(0, 1);
            }
        }
    }
}

// SRS offsets use an upward-positive Y axis; board coordinates are downward-positive.
fn kicks(kind: Kind, from: u8, to: u8) -> &'static [(i32, i32)] {
    if kind == Kind::I {
        match (from, to) {
            (0, 1) | (3, 2) => &[(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
            (1, 0) | (2, 3) => &[(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
            (1, 2) | (0, 3) => &[(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
            (2, 1) | (3, 0) => &[(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
            _ => &[(0, 0)],
        }
    } else {
        match (from, to) {
            (0, 1) | (2, 1) => &[(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
            (1, 0) | (1, 2) => &[(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
            (2, 3) | (0, 3) => &[(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
            (3, 2) | (3, 0) => &[(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
            _ => &[(0, 0)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game() -> Game {
        Game::new(1, 42)
    }
    #[test]
    fn bags_and_seeds_are_deterministic() {
        let mut a = game();
        let mut b = game();
        let mut pieces = vec![a.active.kind];
        assert_eq!(a.active.kind, b.active.kind);
        for _ in 0..20 {
            let p = a.take_next();
            assert_eq!(p, b.take_next());
            pieces.push(p);
        }
        for bag in pieces.as_chunks::<7>().0 {
            for kind in Kind::ALL {
                assert_eq!(bag.iter().filter(|&&k| k == kind).count(), 1);
            }
        }
        assert!(a.next.len() >= 5);
    }
    #[test]
    fn collision_ghost_drop_and_hold() {
        let mut g = game();
        assert!(!g.fits(Piece { x: -5, ..g.active }));
        let first = g.active.kind;
        g.act(Action::Hold);
        let second = g.active.kind;
        g.act(Action::Hold);
        assert_eq!(g.active.kind, second);
        assert_eq!(g.held, Some(first));
        let ghost = g.ghost();
        assert!(g.fits(ghost));
        assert!(!g.fits(Piece {
            y: ghost.y + 1,
            ..ghost
        }));
        let distance = ghost.y - g.active.y;
        g.act(Action::HardDrop);
        assert_eq!(g.score, distance as u64 * 2);
        assert!(!g.hold_used);
        assert_eq!(g.board.iter().flatten().filter(|v| v.is_some()).count(), 4);
        g.act(Action::Hold);
        assert_eq!(g.active.kind, first);
    }
    #[test]
    fn line_clear_scores_at_previous_level_and_compacts() {
        let mut g = game();
        g.lines = 9;
        g.board[23] = [Some(Kind::J); WIDTH];
        for x in 3..7 {
            g.board[23][x] = None;
        }
        g.board[22][0] = Some(Kind::S);
        g.active = Piece {
            kind: Kind::I,
            rotation: 0,
            x: 3,
            y: 22,
        };
        g.lock();
        assert_eq!((g.score, g.lines, g.level()), (100, 10, 2));
        assert_eq!(g.board[23][0], Some(Kind::S));
        assert_eq!(g.gravity_interval(), Duration::from_millis(800));
    }
    #[test]
    fn tetris_and_soft_drop_score() {
        let mut g = game();
        g.act(Action::SoftDrop);
        assert_eq!(g.score, 1);
        for y in 20..24 {
            g.board[y] = [Some(Kind::Z); WIDTH];
            g.board[y][5] = None;
        }
        g.active = Piece {
            kind: Kind::I,
            rotation: 1,
            x: 3,
            y: 20,
        };
        g.lock();
        assert_eq!(g.score, 801);
        assert_eq!(g.lines, 4);
    }
    #[test]
    fn srs_wall_and_floor_kicks() {
        let mut g = game();
        g.active = Piece {
            kind: Kind::T,
            rotation: 1,
            x: -1,
            y: 10,
        };
        assert!(g.fits(g.active));
        g.act(Action::RotateCcw);
        assert_eq!(g.active.rotation, 0);
        assert_eq!(g.active.x, 0);
        g.active = Piece {
            kind: Kind::I,
            rotation: 0,
            x: 3,
            y: 22,
        };
        g.act(Action::RotateCw);
        assert_eq!(g.active.rotation, 1);
        assert!(g.fits(g.active));
        assert_eq!(g.active.y, 20);
    }
    #[test]
    fn lock_delay_and_reset_limit() {
        let mut g = game();
        g.active = Piece {
            kind: Kind::O,
            rotation: 0,
            x: 3,
            y: 22,
        };
        g.advance(Duration::from_millis(499));
        assert!(g.board[23][4].is_none());
        for i in 0..15 {
            g.act(if i % 2 == 0 {
                Action::Left
            } else {
                Action::Right
            });
            g.advance(Duration::from_millis(10));
        }
        assert_eq!(g.lock_resets, 15);
        g.advance(Duration::from_millis(480));
        g.act(Action::Right);
        g.advance(Duration::from_millis(10));
        assert!(g.board[23].iter().any(Option::is_some));
    }
    #[test]
    fn gravity_and_game_over_stop_time() {
        let mut g = game();
        let y = g.active.y;
        g.advance(Duration::from_millis(999));
        assert_eq!(g.active.y, y);
        g.advance(Duration::from_millis(1));
        assert_eq!(g.active.y, y + 1);
        let cells = Piece::spawn(Kind::T).cells();
        for (x, y) in cells {
            g.board[y as usize][x as usize] = Some(Kind::Z);
        }
        g.spawn(Kind::T);
        assert!(g.over);
        let elapsed = g.elapsed;
        g.advance(Duration::from_secs(20));
        assert_eq!(g.elapsed, elapsed);
    }
    #[test]
    fn locking_above_visible_board_ends_game() {
        let mut g = game();
        g.active = Piece {
            kind: Kind::O,
            rotation: 0,
            x: 3,
            y: 2,
        };
        g.lock();
        assert!(g.over);
    }
    #[test]
    fn split_time_matches_single_update() {
        let mut a = game();
        let mut b = game();
        a.advance(Duration::from_secs(25));
        for _ in 0..2500 {
            b.advance(Duration::from_millis(10));
        }
        assert_eq!(a.board, b.board);
        assert_eq!(a.active.cells(), b.active.cells());
        assert_eq!(a.elapsed, b.elapsed);
    }
}
