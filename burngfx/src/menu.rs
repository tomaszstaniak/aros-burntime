//! The start screen: begin a game, or take up the one on disk.
//!
//! v0.9.1's `MenuScene` is the model. Its buttons are Start and Load, and
//! along the bottom sit three toggles: difficulty (`@newburn?14..16`), game
//! mode — `GameMode.State == 0` means **extended**, state 1 the original
//! rules — and whether the other players are played by the machine. The
//! chosen difficulty and mode go into `NewGameInfo` and from there into
//! `GameCreation.CreateNewGame`; nothing else on that screen decides a rule.
//!
//! What this carries over: **start, load, quit, the two modes and the three
//! difficulties**. What it does not:
//!
//! * **Players, faces, names and colours.** There is one player here and no
//!   machine players at all, so there is nothing to choose.
//! * **The engine axis and the contested combat rules.** They are ours, not
//!   the game's — a way of asking which release's behaviour to reproduce and
//!   which reading of an undecided rule to play by. Putting them on the same
//!   screen as Original and Extended would present four rulesets where the
//!   game has two. They stay on the command line, and a game started from
//!   this screen is v0.9.1 with the default answers.
//!
//! This is the older text menu. The start screen a player sees is
//! [`crate::start`], v0.9.1's `MenuScene`, which opens on **Extended** as
//! upstream's does (`OnActivateScene` sets `GameMode.State = 0`); this module
//! only supplies [`Choice`] and [`Start`] to it and to scripted runs.
//!
//! **A load takes its rules from the file, not from this screen.** The mode,
//! the engine, the difficulty and the contested answers are all in the save's
//! header; choosing Load therefore ignores whatever the toggles say, which is
//! why [`Choice::Load`] carries nothing.

use crate::game::{Engine, Mode};

/// The three sections of `gamesettings.txt`.
pub const DIFFICULTIES: u16 = 3;

/// What the player picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Begin, with these rules.
    Start(Start),
    /// Take up the save. Its own header says under what rules.
    Load,
    Quit,
}

/// Everything the start screen decides about a new game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    pub mode: Mode,
    /// Always the default from this screen; see the module docs.
    pub engine: Engine,
    pub difficulty: u16,
    /// **Whether somebody plays against you.** Upstream's screen has a
    /// toggle for whether the other players are played by the machine; this
    /// is what is left of it with one opponent.
    ///
    /// It is *our* opponent and the screen says so rather than implying the
    /// 1993 game's: v0.9.1's own `ClassicAiState` conjures food, water and
    /// hire payment for itself, and that behaviour was excluded as a model —
    /// see [`crate::rival`]. Calling it "the machine plays too" without
    /// naming whose machine it is would be claiming a provenance it does not
    /// have.
    pub opponent: bool,
    /// The v1.2 game the screen chose — rules and opponent. When set, the
    /// session is that v1.2 game and `mode`, `engine` and `opponent` above
    /// are not used.
    pub v12: Option<(crate::rules::RuleSet, crate::rules::AiProfile)>,
}

/// What the file on disk turned out to be, when there is one and it can be
/// read far enough to say. Shown so that Load is not a blind choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub day: u16,
    pub mode: Mode,
    pub difficulty: u16,
    /// **False when the file is there but this program cannot use it** — a
    /// save from another version, or made against another pack. Load still
    /// offers it and still refuses it, with the reason, rather than the row
    /// vanishing and leaving the player to guess.
    pub usable: bool,
}

/// The rows, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    NewGame,
    Mode,
    Difficulty,
    Opponent,
    Load,
    Quit,
}

pub const ROWS: [Row; 6] =
    [Row::NewGame, Row::Mode, Row::Difficulty, Row::Opponent, Row::Load, Row::Quit];

/// The screen's state: which row the player is on, and what the two toggles
/// say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Menu {
    pub row: usize,
    pub mode: Mode,
    pub difficulty: u16,
    /// Whether the new game has somebody on the other side.
    pub opponent: bool,
    /// The save the Load row would take up, if there is one.
    pub waiting: Option<Waiting>,
}

impl Menu {
    /// `difficulty` is what a game started without this screen would have
    /// used — `gam.dat`'s own — so that pressing Enter at once changes
    /// nothing about the game that used to start.
    pub fn new(mode: Mode, difficulty: u16, waiting: Option<Waiting>) -> Menu {
        Menu {
            row: 0,
            mode,
            difficulty: difficulty.min(DIFFICULTIES - 1),
            // **On by default.** A game with nobody trying to take the world
            // from you has no victory to race for, and the screen that opens
            // on "alone" would make the ordinary game the hidden one.
            opponent: true,
            waiting,
        }
    }

    pub fn at(&self) -> Row {
        ROWS[self.row.min(ROWS.len() - 1)]
    }

    pub fn down(&mut self) {
        self.row = (self.row + 1) % ROWS.len();
    }

    pub fn up(&mut self) {
        self.row = (self.row + ROWS.len() - 1) % ROWS.len();
    }

    /// Change the setting the cursor is on. Rows that are not settings do
    /// nothing, rather than being a second way to press them.
    pub fn turn(&mut self, forwards: bool) {
        match self.at() {
            Row::Mode => {
                self.mode = match self.mode {
                    Mode::Original => Mode::Extended,
                    Mode::Extended => Mode::Original,
                }
            }
            Row::Difficulty => {
                self.difficulty = if forwards {
                    (self.difficulty + 1) % DIFFICULTIES
                } else {
                    (self.difficulty + DIFFICULTIES - 1) % DIFFICULTIES
                }
            }
            Row::Opponent => self.opponent = !self.opponent,
            _ => {}
        }
    }

    /// Take the row the cursor is on. A setting row turns instead, so that
    /// one key does something everywhere.
    pub fn take(&mut self) -> Option<Choice> {
        match self.at() {
            Row::NewGame => Some(Choice::Start(Start {
                mode: self.mode,
                engine: Engine::default(),
                difficulty: self.difficulty,
                opponent: self.opponent,
                v12: None,
            })),
            // **Offered even when the file cannot be used.** The refusal, with
            // its reason, belongs to the load itself: a row that disappears
            // says nothing about why.
            Row::Load => self.waiting.is_some().then_some(Choice::Load),
            Row::Quit => Some(Choice::Quit),
            Row::Mode | Row::Difficulty | Row::Opponent => {
                self.turn(true);
                None
            }
        }
    }
}
