//! The start screen: `MenuScene` and the controls it is made
//! of — `NameWindow`, `FaceWindow`, `Radio`, `Toggle`, `Button` — as state and
//! hit-testing, with no pixels. The program draws what [`StartScreen`] says
//! and hands it clicks, keys and time.
//!
//! Positions are the scene's own, 320 × 200 from its top left; the scene is
//! centred in whatever the game resolution is. Every number here is
//! `MenuScene`'s constructor's.
//!
//! **Behaviour copied, quirks included.** The two name slots are switches
//! whose *down* state is keyboard focus, and whether a slot takes part is a
//! separate flag; closing one hands focus to the other if it is in use; a
//! face can be clicked even while its slot is covered, because the cover is an
//! `Image`, which lets clicks through. Each of these is what the source does,
//! and each is marked where it happens.
//!
//! **v1.2's choices.** The game mode toggle chooses the rules
//! (`GameMode.State`: 0 Extended, 1 Classic) and the AI toggle the opponent
//! (`AiPlayers.State`: 0 Modern, 1 Amiga, 2 None), and START gives a v1.2
//! game. The legacy v0.9.1 game is the command line's, not this screen's.
//! An opponent this build does not offer is passed over by the toggle
//! ([`StartScreen::offer`]), so the screen never starts what cannot play.

use alloc::string::String;
use alloc::vec::Vec;

use crate::rules::{AiProfile, RuleSet};

/// `AiPlayers`' states, in order.
pub const OPPONENTS: [AiProfile; 3] = [AiProfile::Modern, AiProfile::Amiga, AiProfile::None];

/// `MenuScene.MAX_FACE_ID`: faces 0 to 5, `ges_00.ani` … `ges_05.ani`.
pub const MAX_FACE: i32 = 5;
/// `NameWindow.MaxNameLength`.
pub const MAX_NAME: usize = 10;
/// `sta.ani?10-16`: the cover over a face, seven frames, closed at the last.
pub const SLIDE_FRAMES: u32 = 7;
/// `PlayerOneSlide.Speed = 10`: frames per second.
pub const SLIDE_FPS: u32 = 10;
/// `SpriteAnimation.Speed`'s default, which the faces keep.
pub const FACE_FPS: u32 = 5;

/// A rectangle in scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// Everything on the screen that answers to the mouse, with where it is and
/// how big. Sizes are the images' (`Button` takes its size from its image),
/// except the name fields, whose `Size` is set to 104 × 24 over a 112-wide
/// picture.
pub mod at {
    use super::Rect;
    pub const LOAD: Rect = Rect::new(131, 15, 64, 24);
    pub const START: Rect = Rect::new(131, 42, 64, 24);
    pub const EXIT: Rect = Rect::new(276, 163, 31, 18);
    pub const NAME: [Rect; 2] = [Rect::new(15, 92, 104, 24), Rect::new(204, 92, 104, 24)];
    pub const FACE: [Rect; 2] = [Rect::new(33, 28, 68, 55), Rect::new(223, 28, 68, 55)];
    pub const SLIDE: [(i32, i32); 2] = [(30, 26), (220, 26)];
    pub const COLOUR: [Rect; 2] = [Rect::new(45, 121, 44, 23), Rect::new(237, 121, 44, 23)];
    pub const DIFFICULTY: Rect = Rect::new(100, 149, 32, 32);
    pub const MODE: Rect = Rect::new(145, 149, 32, 32);
    pub const AI: Rect = Rect::new(190, 149, 32, 32);
    pub const COPYRIGHT: (i32, i32) = (75, 185);
    /// v1.2's NOTES (`MenuScene.cs:193-203`): `gfx/ui/menu_exit.png`, 36 ×
    /// 18, at EXIT's 13-pixel margin on the left.
    pub const NOTES: Rect = Rect::new(13, 163, 36, 18);
}

/// A name slot and the face above it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// `UsePlayerOne`: the slot takes part.
    pub used: bool,
    /// The switch's `IsDown`, which is keyboard focus (`NameWindow` sets
    /// `HasFocus` on down) and picks the pressed picture.
    pub focused: bool,
    pub name: String,
    /// `NameWindow.HasManualName`: the name was typed, not drawn.
    pub manual: bool,
    /// `FaceWindow.FaceID`, −1 for none.
    pub face: i32,
    /// The last face shown. `RefreshFace` changes the picture only for a real
    /// face, so after −1 the old one stays behind the cover.
    pub face_shown: Option<i32>,
    /// The cover's frame position in thousandths of a frame, 0 = open,
    /// `SLIDE_FRAMES * 1000 - 1` = closed.
    slide_pos: i32,
    /// Opening (running backwards) or closing, or still.
    slide_dir: i32,
}

impl Slot {
    fn new() -> Slot {
        Slot {
            used: false,
            focused: false,
            name: String::new(),
            manual: false,
            face: -1,
            face_shown: None,
            slide_pos: SLIDE_FRAMES as i32 * 1000 - 1,
            slide_dir: 0,
        }
    }

    /// The cover's frame, 0..7, of `sta.ani?10-16`.
    pub fn slide_frame(&self) -> u32 {
        (self.slide_pos / 1000).clamp(0, SLIDE_FRAMES as i32 - 1) as u32
    }

    /// `"[ " + name + " ]"`.
    pub fn label(&self) -> String {
        let mut s = String::from("[ ");
        s.push_str(&self.name);
        s.push_str(" ]");
        s
    }
}

/// One of the three toggles along the bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Difficulty,
    Mode,
    Ai,
}

/// What the pointer is over, for the pictures that change under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hover {
    Load,
    Start,
    Exit,
    Notes,
    Toggle(Toggle),
}

/// What a click asked for beyond the screen's own state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// START with at least one face: the game `NewGameInfo` describes.
    Start(NewGame),
    /// LOAD: the original goes to `OptionsScene`.
    Load,
    Exit,
    /// v1.2's NOTES: the setup notes (`ManualWindow(setupNotes: true)`).
    Notes,
}

/// One human seat out of the start screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Human {
    /// The name typed or drawn — `GetRandomName` for a slot with a face and
    /// no name.
    pub name: String,
    pub face: i32,
    /// `ColorOne` for the first slot, `ColorTwo` for the second
    /// (`MenuScene.cs:1028-1029` at v1.3).
    pub color: crate::player_colors::PlayerColor,
}

/// `NewGameInfo`.
///
/// **A seat is human only if its own slot is.** `NameOne` and `NameTwo` are
/// separate: with only the second slot used, player 0 is left to the machine
/// and the human is player 1, in the second colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewGame {
    /// Seats 0 and 1: `Some` for a human, `None` for the machine.
    pub players: [Option<Human>; 2],
    /// `ColorOne`, `ColorTwo`: the two flags, whoever plays the seats.
    pub colors: [crate::player_colors::PlayerColor; 2],
    /// `Difficulty.State`: 0 easy, 1 normal, 2 hard.
    pub difficulty: u16,
    /// `Rules = GameMode.State switch { 1 => Classic, _ => Extended }`.
    pub rules: RuleSet,
    /// `AI = AiPlayers.State switch { 1 => Amiga, 2 => None, _ => Modern }`.
    pub opponent: AiProfile,
}

impl NewGame {
    /// The seats with a human in them, in seat order.
    pub fn humans(&self) -> impl Iterator<Item = (usize, &Human)> {
        self.players.iter().enumerate().filter_map(|(i, h)| h.as_ref().map(|h| (i, h)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartScreen {
    pub slots: [Slot; 2],
    /// `_playerColors` (v1.3, `MenuScene.cs:45-46`): each player's flag,
    /// green and red to start with.
    pub colors: [crate::player_colors::PlayerColor; 2],
    /// `Difficulty.State`, `GameMode.State`, `AiPlayers.State`.
    pub difficulty: u16,
    pub mode_state: u16,
    pub ai_state: u16,
    pub hover: Option<Hover>,
    /// Milliseconds the faces have been animating, for their frame.
    pub clock_ms: u32,
    /// Which of [`OPPONENTS`] this build plays.
    pub offered: [bool; 3],
    /// v1.2's keyboard selection (`_setupSelection`, `_currentPlayer`), and
    /// whether it shows (the keyboard is in use).
    pub setup: Setup,
    pub current: usize,
    pub setup_shown: bool,
    /// `names.txt` (`_playerNames`): trimmed, blank lines dropped, each name
    /// once whatever its case. Empty, and every automatic name is "Max".
    pub names: Vec<String>,
    /// What `GetRandomName` draws from: the process's one `Random`, as
    /// upstream's, lent to the screen while it is open and handed back, so
    /// the game the screen starts draws on from where the names left it.
    pub rng: crate::spawn::Rng,
}

/// `names.txt` as `MenuScene` reads it: every line trimmed, blank ones
/// dropped, and each name once, compared without case.
pub fn parse_names(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let name = line.trim();
        if name.is_empty() || out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            continue;
        }
        out.push(String::from(name));
    }
    out
}

/// `MenuScene.SetupSelection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setup {
    Player,
    Load,
    Start,
    Difficulty,
    GameMode,
    AiPlayers,
    Notes,
    Exit,
}

/// A key on the start screen, as v1.2 reads it (`OnInputAction`,
/// `OnVKeyPress`): only the window's virtual keys, so letters stay the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupKey {
    Up,
    Down,
    Left,
    Right,
    /// Primary (Enter).
    Primary,
    /// Secondary, or Shift+Up/Down on a player: the colours.
    Colours,
    /// LeftArea/RightArea on a player: the face.
    FaceDown,
    FaceUp,
    Tab,
    ShiftTab,
    /// Back or Options: Load.
    Back,
}

/// `@newburn?N` (v1.2's `newburn.txt`) for a toggle's current tooltip, as
/// `AddState` names them.
pub fn tooltip(toggle: Toggle, s: &StartScreen) -> u32 {
    match toggle {
        Toggle::Difficulty => 14 + s.difficulty as u32,
        Toggle::Mode => if s.mode_state == 0 { 110 } else { 105 },
        Toggle::Ai => [109, 108, 13][(s.ai_state as usize).min(2)],
    }
}

/// v1.2's tooltip window for a toggle, with the hints on: the section of
/// `tooltip.txt` it shows (`UpdateDifficultyTooltip`: `9 + State`;
/// `UpdateGameModeTooltip`: 5 for Classic, 4 for Extended;
/// `UpdateAiTooltip`: 1 Amiga, 2 None, 0 Modern).
pub fn tooltip_section(toggle: Toggle, s: &StartScreen) -> usize {
    match toggle {
        Toggle::Difficulty => 9 + (s.difficulty as usize).min(2),
        Toggle::Mode => if s.mode_state == 1 { 5 } else { 4 },
        Toggle::Ai => match s.ai_state {
            1 => 1,
            2 => 2,
            _ => 0,
        },
    }
}

impl Default for StartScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl StartScreen {
    /// As `OnActivateScene` leaves it: both slots closed and empty, easy,
    /// Extended, the Modern opponent.
    pub fn new() -> StartScreen {
        StartScreen {
            slots: [Slot::new(), Slot::new()],
            colors: [crate::player_colors::PlayerColor::Green, crate::player_colors::PlayerColor::Red],
            difficulty: 0,
            mode_state: 0,
            ai_state: 0,
            hover: None,
            clock_ms: 0,
            offered: [true; 3],
            setup: Setup::Player,
            current: 0,
            setup_shown: false,
            names: Vec::new(),
            rng: crate::spawn::Rng::game(0),
        }
    }

    /// `GetRandomName(excludedNames)`: from a start drawn with
    /// `Next(0, Length)`, the first name, going round, that is none of
    /// `excluded` (without case); the start's own when all are. "Max", with
    /// no draw, when there are no names (`MenuScene.cs:926-940`).
    pub fn random_name(&mut self, excluded: &[&str]) -> String {
        if self.names.is_empty() {
            return String::from("Max");
        }
        let n = self.names.len();
        let start = self.rng.range(0, n as i32) as usize;
        for offset in 0..n {
            let candidate = &self.names[(start + offset) % n];
            if !excluded.iter().any(|e| candidate.eq_ignore_ascii_case(e)) {
                return candidate.clone();
            }
        }
        self.names[start].clone()
    }

    /// `SetAutomaticName(GetRandomName(other's name))` for slot `i`.
    fn automatic_name(&mut self, i: usize) {
        let other = self.slots[1 - i].name.clone();
        let name = self.random_name(&[other.as_str()]);
        self.slots[i].name = name;
        self.slots[i].manual = false;
    }

    /// v1.2's start screen keys: the selection moves as `MoveSetupSelection*`
    /// move it, and a key that acts says where the click is that acts —
    /// a rectangle and whether it is the right button — so that the click's
    /// own code decides what it does. The first Primary only shows the
    /// selection.
    pub fn key(&mut self, k: SetupKey) -> Option<(Rect, bool)> {
        use Setup::*;
        // `MoveSetupSelectionHorizontal`: NOTES, the three toggles, EXIT.
        let bottom = [Notes, Difficulty, GameMode, AiPlayers, Exit];
        match k {
            SetupKey::Up => {
                self.setup = match self.setup {
                    Player => Start,
                    Start => Load,
                    Difficulty | GameMode | Notes => {
                        self.current = 0;
                        Player
                    }
                    AiPlayers | Exit => {
                        self.current = 1;
                        Player
                    }
                    other => other,
                };
            }
            SetupKey::Down => {
                self.setup = match self.setup {
                    Player => if self.current == 0 { Difficulty } else { AiPlayers },
                    Load => Start,
                    Start => {
                        self.current = 0;
                        Player
                    }
                    other => other,
                };
            }
            SetupKey::Left | SetupKey::Right => {
                let right = k == SetupKey::Right;
                match self.setup {
                    Player => {
                        // Only inward: from the first slot right, the second left.
                        if (self.current == 0) == right {
                            self.current = 1 - self.current;
                        }
                    }
                    Load | Start => {
                        self.current = usize::from(right);
                        self.setup = Player;
                    }
                    other => {
                        let i = bottom.iter().position(|&b| b == other).unwrap_or(0);
                        let j = if right { (i + 1).min(bottom.len() - 1) } else { i.saturating_sub(1) };
                        self.setup = bottom[j];
                    }
                }
            }
            SetupKey::Tab | SetupKey::ShiftTab => {
                self.current = 1 - self.current;
                self.setup = Player;
            }
            SetupKey::Primary => {
                if !self.setup_shown {
                    self.setup_shown = true;
                    return None;
                }
                return Some((
                    match self.setup {
                        Player => at::NAME[self.current],
                        Load => at::LOAD,
                        Start => at::START,
                        Difficulty => at::DIFFICULTY,
                        GameMode => at::MODE,
                        AiPlayers => at::AI,
                        Notes => at::NOTES,
                        Exit => at::EXIT,
                    },
                    false,
                ));
            }
            // `CyclePlayerColor` only for a player in use (`CurrentPlayerEnabled`,
            // `MenuScene.cs:503-505`).
            SetupKey::Colours if self.setup == Player && self.slots[self.current].used => {
                return Some((at::COLOUR[self.current], false))
            }
            SetupKey::FaceUp if self.setup == Player && self.slots[self.current].used => {
                return Some((at::FACE[self.current], false))
            }
            SetupKey::FaceDown if self.setup == Player && self.slots[self.current].used => {
                return Some((at::FACE[self.current], true))
            }
            SetupKey::Back => return Some((at::LOAD, false)),
            _ => {}
        }
        self.setup_shown = true;
        None
    }

    /// Only the opponents `offered` (in [`OPPONENTS`]' order) can be chosen;
    /// the toggle starts on the first of them.
    pub fn offer(&mut self, offered: [bool; 3]) {
        self.offered = offered;
        if !self.offered[self.ai_state as usize % 3] {
            self.ai_state = self.next_opponent(self.ai_state);
        }
    }

    /// The toggle's next state from `state`, skipping what is not offered.
    fn next_opponent(&self, state: u16) -> u16 {
        (1..=3).map(|d| (state + d) % 3).find(|&n| self.offered[n as usize]).unwrap_or(state)
    }

    /// The face frame to show, of `ges_NN.ani`'s 40.
    pub fn face_frame(&self, frames: u32) -> u32 {
        if frames == 0 {
            0
        } else {
            (self.clock_ms * FACE_FPS / 1000) % frames
        }
    }

    /// Advance the covers and the faces.
    pub fn tick(&mut self, ms: u32) {
        self.clock_ms = self.clock_ms.wrapping_add(ms);
        let step = (ms * SLIDE_FPS) as i32;
        let last = SLIDE_FRAMES as i32 * 1000 - 1;
        for slot in &mut self.slots {
            if slot.slide_dir != 0 {
                slot.slide_pos += slot.slide_dir * step;
                if slot.slide_pos <= 0 {
                    slot.slide_pos = 0;
                    slot.slide_dir = 0;
                } else if slot.slide_pos >= last {
                    slot.slide_pos = last;
                    slot.slide_dir = 0;
                }
            }
        }
    }

    /// The pointer moved. Only buttons hover.
    pub fn point(&mut self, x: i32, y: i32) {
        self.hover = if at::LOAD.contains(x, y) {
            Some(Hover::Load)
        } else if at::START.contains(x, y) {
            Some(Hover::Start)
        } else if at::EXIT.contains(x, y) {
            Some(Hover::Exit)
        } else if at::NOTES.contains(x, y) {
            Some(Hover::Notes)
        } else if at::DIFFICULTY.contains(x, y) {
            Some(Hover::Toggle(Toggle::Difficulty))
        } else if at::MODE.contains(x, y) {
            Some(Hover::Toggle(Toggle::Mode))
        } else if at::AI.contains(x, y) {
            Some(Hover::Toggle(Toggle::Ai))
        } else {
            None
        };
    }

    /// A mouse click at scene coordinates. `right` is the right button; only
    /// the faces tell the two apart.
    pub fn click(&mut self, x: i32, y: i32, right: bool) -> Option<Outcome> {
        if at::LOAD.contains(x, y) {
            return Some(Outcome::Load);
        }
        if at::START.contains(x, y) {
            return self.start();
        }
        if at::EXIT.contains(x, y) {
            return Some(Outcome::Exit);
        }
        if at::NOTES.contains(x, y) {
            return Some(Outcome::Notes);
        }
        for i in 0..2 {
            if at::NAME[i].contains(x, y) {
                self.name_clicked(i);
                return None;
            }
        }
        for i in 0..2 {
            // **Through the cover.** The slide is an `Image`, and an `Image`
            // does not take clicks, so the face under a closed cover still
            // changes. `OnButtonStart` then counts it.
            if at::FACE[i].contains(x, y) {
                let face = self.slots[i].face;
                let wanted = if right { (face - 1).max(0) } else { (face + 1).min(MAX_FACE) };
                self.set_face(i, wanted);
                return None;
            }
        }
        for i in 0..2 {
            if at::COLOUR[i].contains(x, y) {
                // A left click on the flag's `Toggle`: its next state.
                if !right {
                    self.cycle_color(i);
                }
                return None;
            }
        }
        if at::DIFFICULTY.contains(x, y) {
            self.difficulty = (self.difficulty + 1) % 3;
        } else if at::MODE.contains(x, y) {
            self.mode_state = (self.mode_state + 1) % 2;
        } else if at::AI.contains(x, y) {
            self.ai_state = self.next_opponent(self.ai_state);
        }
        None
    }

    /// A typed character for whichever slot has focus. `supported` is the
    /// name font's `IsSupportetCharacter`; `'\u{8}'` is backspace, which
    /// `NameWindow` accepts whatever the font.
    pub fn type_char(&mut self, c: char, supported: bool, table: &[(String, String)]) {
        let Some(slot) = self.slots.iter_mut().find(|s| s.focused) else { return };
        if c == '\u{8}' {
            // `HasManualName = name.Length > 0`, after the character goes.
            if slot.name.pop().is_some() {
                slot.manual = !slot.name.is_empty();
            }
            return;
        }
        if !supported {
            return;
        }
        if slot.name.chars().count() >= MAX_NAME {
            return;
        }
        slot.name.push(c);
        slot.manual = true;
        // "convert last characters (3, 2, 1)" through `conversion_table.txt`.
        for n in (1..=3).rev() {
            let chars: Vec<char> = slot.name.chars().collect();
            if chars.len() >= n {
                let tail: String = chars[chars.len() - n..].iter().collect();
                if let Some((_, to)) = table.iter().find(|(from, _)| *from == tail) {
                    let mut name: String = chars[..chars.len() - n].iter().collect();
                    name.push_str(to);
                    slot.name = name;
                }
            }
        }
    }

    /// `OnButtonStart`.
    pub fn start(&mut self) -> Option<Outcome> {
        if self.slots[0].face < 0 && self.slots[1].face < 0 {
            return None;
        }
        // `NameOne`: the slot's name, or `GetRandomName(PlayerTwoSwitch.Name)`
        // when it has a face and none — and then `NameTwo` the same way.
        let mut names = [String::new(), String::new()];
        for (i, name) in names.iter_mut().enumerate() {
            *name = if self.slots[i].name.is_empty() && self.slots[i].face >= 0 {
                let other = self.slots[1 - i].name.clone();
                self.random_name(&[other.as_str()])
            } else {
                self.slots[i].name.clone()
            };
        }
        let seat = |i: usize, s: &Self, name: &String| -> Option<Human> {
            let slot = &s.slots[i];
            if slot.face < 0 {
                return None;
            }
            Some(Human { name: name.clone(), face: slot.face, color: s.colors[i] })
        };
        Some(Outcome::Start(NewGame {
            players: [seat(0, self, &names[0]), seat(1, self, &names[1])],
            colors: self.colors,
            difficulty: self.difficulty,
            rules: if self.mode_state == 1 { RuleSet::Classic } else { RuleSet::Extended },
            opponent: OPPONENTS[(self.ai_state as usize).min(2)],
        }))
    }

    /// `Toggle.NextState` on player `i`'s flag and `SelectPlayerColor(i)`
    /// (v1.3, `MenuScene.cs:917-929`): his next colour; taking the other
    /// player's hands him this one's.
    pub fn cycle_color(&mut self, i: usize) {
        let other = 1 - i;
        let selected = self.colors[i].next();
        self.colors[other] = crate::player_colors::other_selection(self.colors[i], selected, self.colors[other]);
        self.colors[i] = selected;
    }

    /// `Switch.OnMouseClick` on a name slot, and the callbacks it fires.
    fn name_clicked(&mut self, i: usize) {
        let down = !self.slots[i].focused;
        self.set_switch(i, down);
        // `OnPlayerOneClick`.
        let slot = &mut self.slots[i];
        slot.slide_dir = if slot.focused { -1 } else { 1 };
        slot.used = slot.focused;
        let face = if slot.used { 0 } else { -1 };
        // `if (!HasManualName) SetAutomaticName(Use ? GetRandomName(other)
        // : "")`.
        if !slot.used {
            slot.name.clear();
            slot.manual = false;
        } else if !slot.manual {
            self.automatic_name(i);
        }
        self.set_face(i, face);
    }

    /// `Switch.IsDown = value`: down fires `DownCommand`, up `UpCommand`.
    fn set_switch(&mut self, i: usize, down: bool) {
        self.slots[i].focused = down;
        let other = 1 - i;
        if down {
            // `OnPlayerOneDown`: the other slot loses focus — and, its text
            // input closing, gets a name if it is in use and has none
            // (`FillEmptyName`).
            if self.slots[other].focused {
                self.set_switch(other, false);
                if self.slots[other].used && self.slots[other].name.is_empty() {
                    self.automatic_name(other);
                }
            }
        } else if self.slots[other].used && !self.slots[other].focused {
            // `OnPlayerOneUp`: focus goes to the other slot if it is in use.
            self.set_switch(other, true);
        }
    }

    /// `FaceWindow.FaceID = value`, with `CheckSameFace`: a face the other
    /// slot has is stepped past in the direction of travel, and a step off
    /// either end keeps the face there was.
    fn set_face(&mut self, i: usize, wanted: i32) {
        let current = self.slots[i].face;
        if wanted == current {
            return;
        }
        let face = self.check_same_face(i, current, wanted);
        self.slots[i].face = face;
        if face >= 0 {
            self.slots[i].face_shown = Some(face);
        }
    }

    fn check_same_face(&self, i: usize, current: i32, wanted: i32) -> i32 {
        if wanted == -1 || wanted > MAX_FACE {
            return wanted;
        }
        let direction = if wanted - current > 0 { 1 } else { -1 };
        let mut face = wanted;
        if self.slots[1 - i].face == face {
            face = self.check_same_face(i, current, face + direction);
        }
        if face == -1 || face > MAX_FACE {
            return current;
        }
        face
    }
}

/// `conversion_table.txt`: `from=to` lines, for the name fields.
pub fn parse_conversion_table(text: &str) -> Vec<(String, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|l| {
            let l = l.trim_end_matches('\r');
            let (a, b) = l.split_once('=')?;
            let a = a.trim();
            (!a.is_empty() && !a.starts_with('[')).then(|| (String::from(a), String::from(b.trim())))
        })
        .collect()
}

/// v1.3's `NewGamePreferences` (`72bed91`, "polish game setup";
/// `GameProfiles.cs` at `v1.3`): the start screen opens on the difficulty,
/// rules and opponent of the last game started, kept in `user.txt` as
/// `mode_difficulty` (1–3, else 1), `mode_rules` (`classic` or `original` is
/// Classic, anything else Extended) and `mode_opponent` (`amiga`, `none` or
/// `off`, anything else Modern). These are the screen's three states.
pub fn preferences(difficulty: Option<&str>, rules: Option<&str>, opponent: Option<&str>) -> (u16, u16, u16) {
    let d = difficulty.and_then(|d| d.trim().parse::<i32>().ok()).filter(|d| (1..=3).contains(d)).unwrap_or(1);
    let lower = |v: Option<&str>| v.map(|v| v.trim().to_ascii_lowercase()).unwrap_or_default();
    let mode = match lower(rules).as_str() {
        "classic" | "original" => 1,
        _ => 0,
    };
    let ai = match lower(opponent).as_str() {
        "amiga" => 1,
        "none" | "off" => 2,
        _ => 0,
    };
    ((d - 1) as u16, mode, ai)
}

/// `NewGamePreferences.Store`: what `user.txt` keeps for these states.
pub fn preference_values(difficulty: u16, mode_state: u16, ai_state: u16) -> [(&'static str, alloc::string::String); 3] {
    use alloc::string::ToString;
    [
        ("mode_difficulty", (difficulty.min(2) + 1).to_string()),
        ("mode_rules", (if mode_state == 1 { "classic" } else { "extended" }).to_string()),
        ("mode_opponent", (match ai_state { 1 => "amiga", 2 => "none", _ => "remaster" }).to_string()),
    ]
}

#[cfg(test)]
mod v13_preferences {
    use super::*;

    #[test]
    fn the_start_screen_reads_and_writes_its_choices_as_v13() {
        assert_eq!(preferences(None, None, None), (0, 0, 0), "1, Extended, Modern");
        assert_eq!(preferences(Some("3"), Some(" Original "), Some("OFF")), (2, 1, 2));
        assert_eq!(preferences(Some("4"), Some("classic"), Some("amiga")), (0, 1, 1), "out of range: 1");
        assert_eq!(preferences(Some("x"), Some("dos"), Some("modern")), (0, 0, 0));
        let v = preference_values(1, 1, 2);
        assert_eq!(v.map(|(k, v)| (k, v)), [("mode_difficulty", "2".into()), ("mode_rules", "classic".into()),
                                          ("mode_opponent", "none".into())]);
        assert_eq!(preference_values(0, 0, 0)[2].1, "remaster");
        // What is written reads back as the same states.
        for (d, m, a) in [(0, 0, 0), (2, 1, 1), (1, 0, 2)] {
            let v = preference_values(d, m, a);
            assert_eq!(preferences(Some(&v[0].1), Some(&v[1].1), Some(&v[2].1)), (d, m, a));
        }
    }
}
