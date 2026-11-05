//! v1.2's play controls: the actions (`Burntime.Framework/InputAction.cs`)
//! and what is bound to them, from the game's `settings.txt` sections
//! `[keyboard]` and `[gamepad]` (`KeyboardBindings.cs`, `GamepadBindings.cs`).
//!
//! v0.9.1 had none of this: play was the mouse. Which key a player pressed
//! is the game's to translate into a [`Key`]; this module only says what
//! that key does.

use alloc::vec::Vec;

use crate::rules::Section;

/// `InputAction`, less `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PanCameraUp,
    PanCameraDown,
    PanCameraLeft,
    PanCameraRight,
    LeftArea,
    RightArea,
    Back,
    Primary,
    Secondary,
    Action,
    Options,
    Statistics,
    ToggleInteractionMode,
    NextTurn,
    Inventory,
    LocationInfo,
    WorldMap,
    ShowEntrances,
    PreviousTarget,
    NextTarget,
    SetupNotes,
    /// The left stick's diagonal press, in the scenes that take one
    /// (`UseDiagonalGamepadNavigation`). No key is bound to these.
    MoveUpLeft,
    MoveUpRight,
    MoveDownLeft,
    MoveDownRight,
}

impl Action {
    /// A move's direction, `(x, y)` with y down; `None` for anything else.
    pub fn direction(self) -> Option<(i32, i32)> {
        Some(match self {
            Action::MoveUp => (0, -1),
            Action::MoveDown => (0, 1),
            Action::MoveLeft => (-1, 0),
            Action::MoveRight => (1, 0),
            Action::MoveUpLeft => (-1, -1),
            Action::MoveUpRight => (1, -1),
            Action::MoveDownLeft => (-1, 1),
            Action::MoveDownRight => (1, 1),
            _ => return None,
        })
    }

    /// The move in a direction, the inverse of [`Action::direction`].
    pub fn moving(d: (i32, i32)) -> Option<Action> {
        Some(match d {
            (0, -1) => Action::MoveUp,
            (0, 1) => Action::MoveDown,
            (-1, 0) => Action::MoveLeft,
            (1, 0) => Action::MoveRight,
            (-1, -1) => Action::MoveUpLeft,
            (1, -1) => Action::MoveUpRight,
            (-1, 1) => Action::MoveDownLeft,
            (1, 1) => Action::MoveDownRight,
            _ => return None,
        })
    }
}

/// A keyboard control as `KeyboardBindings` names them. Letters and digits
/// are their character, lower case; `space` is `' '`, `backspace` `'\x08'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    F1,
    Ctrl,
    Enter,
    Escape,
    Tab,
    Alt,
    Up,
    Down,
    Left,
    Right,
}

/// `GamepadControl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pad {
    A,
    B,
    X,
    Y,
    Menu,
    View,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    LeftTrigger,
    RightTrigger,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// `KeyboardBindings.definitions`: the setting, its default, the action.
/// The order matters: a key named by two settings does what the later says.
pub const KEYBOARD: [(&str, &str, Action); 21] = [
    ("setup_notes", "ctrl", Action::SetupNotes),
    ("move_up", "up", Action::MoveUp),
    ("move_down", "down", Action::MoveDown),
    ("move_left", "left", Action::MoveLeft),
    ("move_right", "right", Action::MoveRight),
    ("pan_up", "w", Action::PanCameraUp),
    ("pan_down", "s", Action::PanCameraDown),
    ("pan_left", "a", Action::PanCameraLeft),
    ("pan_right", "d", Action::PanCameraRight),
    ("accept", "space enter", Action::Primary),
    ("back", "escape", Action::Back),
    ("secondary", "f", Action::Secondary),
    ("action", "q", Action::Action),
    ("options", "o", Action::Options),
    ("inventory", "e i", Action::Inventory),
    ("world_map", "v m", Action::WorldMap),
    ("statistics", "h", Action::Statistics),
    ("info", "r", Action::LocationInfo),
    ("next_turn", "t", Action::NextTurn),
    ("toggle_interaction", "c", Action::ToggleInteractionMode),
    ("show_entrances", "alt", Action::ShowEntrances),
];

/// `GamepadBindings.definitions`.
pub const GAMEPAD: [(&str, &str, Action); 14] = [
    ("accept", "a", Action::Primary),
    ("back", "b", Action::Back),
    ("secondary", "x", Action::Secondary),
    ("action", "y", Action::Action),
    ("options", "menu", Action::Options),
    ("world_map", "view", Action::WorldMap),
    ("left_area", "left_shoulder", Action::LeftArea),
    ("right_area", "right_shoulder", Action::RightArea),
    ("cycle_target", "right_trigger", Action::NextTarget),
    ("inventory", "dpad_up", Action::Inventory),
    ("statistics", "dpad_left", Action::Statistics),
    ("info", "dpad_right", Action::LocationInfo),
    ("next_turn", "dpad_down", Action::NextTurn),
    ("show_entrances", "left_trigger", Action::ShowEntrances),
];

/// `KeyboardBindings.controls`, case-insensitive.
pub fn key_named(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    Some(match lower.as_str() {
        "f1" => Key::F1,
        "ctrl" => Key::Ctrl,
        "space" => Key::Char(' '),
        "backspace" => Key::Char('\x08'),
        "enter" => Key::Enter,
        "escape" => Key::Escape,
        "tab" => Key::Tab,
        "alt" => Key::Alt,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        _ => {
            let mut chars = lower.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_lowercase() || c.is_ascii_digit() => Key::Char(c),
                _ => return None,
            }
        }
    })
}

/// `GamepadBindings.controls`, case-insensitive.
pub fn pad_named(name: &str) -> Option<Pad> {
    Some(match name.to_ascii_lowercase().as_str() {
        "a" => Pad::A,
        "b" => Pad::B,
        "x" => Pad::X,
        "y" => Pad::Y,
        "menu" => Pad::Menu,
        "view" => Pad::View,
        "left_shoulder" => Pad::LeftShoulder,
        "right_shoulder" => Pad::RightShoulder,
        "left_stick" => Pad::LeftStick,
        "right_stick" => Pad::RightStick,
        "left_trigger" => Pad::LeftTrigger,
        "right_trigger" => Pad::RightTrigger,
        "dpad_up" => Pad::DPadUp,
        "dpad_down" => Pad::DPadDown,
        "dpad_left" => Pad::DPadLeft,
        "dpad_right" => Pad::DPadRight,
        _ => return None,
    })
}

/// What each key and pad control does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bindings {
    keys: Vec<(Key, Action)>,
    pad: Vec<(Pad, Action)>,
}

/// The value a setting has: the section's when it names it (even empty,
/// which unbinds), `global_action` for `action` when only that is there,
/// else the default.
fn value<'a>(section: Option<&'a Section>, setting: &str, default: &'a str) -> &'a str {
    match section {
        Some(s) if s.has(setting) => s.string(setting).trim(),
        Some(s) if setting == "action" && s.has("global_action") => s.string("global_action").trim(),
        _ => default,
    }
}

impl Bindings {
    /// The defaults, as if `settings.txt` named nothing.
    pub fn defaults() -> Bindings {
        Bindings::load(None, None)
    }

    /// `Load` of both, from the game's settings file.
    pub fn from_settings(text: &str) -> Bindings {
        let sections = crate::rules::sections(text);
        let find = |name: &str| sections.iter().find(|s| s.name.eq_ignore_ascii_case(name));
        Bindings::load(find("keyboard"), find("gamepad"))
    }

    fn load(keyboard: Option<&Section>, gamepad: Option<&Section>) -> Bindings {
        let mut b = Bindings::default();
        for (setting, default, action) in KEYBOARD {
            // Several keys, separated by whitespace or commas.
            for key in value(keyboard, setting, default).split([' ', '\t', ',']).filter_map(key_named) {
                b.keys.retain(|(k, _)| *k != key);
                b.keys.push((key, action));
            }
        }
        for (setting, default, action) in GAMEPAD {
            // One control, the whole value.
            if let Some(pad) = pad_named(value(gamepad, setting, default)) {
                b.pad.retain(|(p, _)| *p != pad);
                b.pad.push((pad, action));
            }
        }
        b
    }

    /// `KeyboardBindings.GetAction`. Tab is the next target and Shift+Tab
    /// the previous, Shift+Left and Shift+Right the areas, whatever the
    /// settings say.
    pub fn key_action(&self, key: Key, shift: bool) -> Option<Action> {
        match (key, shift) {
            (Key::Tab, true) => return Some(Action::PreviousTarget),
            (Key::Tab, false) => return Some(Action::NextTarget),
            (Key::Left, true) => return Some(Action::LeftArea),
            (Key::Right, true) => return Some(Action::RightArea),
            _ => {}
        }
        let key = match key {
            Key::Char(c) => Key::Char(c.to_ascii_lowercase()),
            k => k,
        };
        self.keys.iter().find(|(k, _)| *k == key).map(|(_, a)| *a)
    }

    /// `GamepadBindings.GetAction`.
    pub fn pad_action(&self, pad: Pad) -> Option<Action> {
        self.pad.iter().find(|(p, _)| *p == pad).map(|(_, a)| *a)
    }

    /// The keys bound to an action, in the order the settings name them.
    pub fn keys_for(&self, action: Action) -> Vec<Key> {
        self.keys.iter().filter(|(_, a)| *a == action).map(|(k, _)| *k).collect()
    }

    /// `GamepadBindings.GetControls`: the pad controls bound to an action.
    /// Each action has one setting naming one control, so there is at most
    /// one.
    pub fn pads_for(&self, action: Action) -> Vec<Pad> {
        self.pad.iter().filter(|(_, a)| *a == action).map(|(p, _)| *p).collect()
    }
}

/// `SelectAdjacentGroupCharacter` (LeftArea and RightArea): who is selected
/// next, and whether single mode is then on, from the selection now. The
/// group is the leader then `companions` (by identity), as `Party` orders
/// them. A camper selected goes back to the leader either way; a group of
/// one does nothing; in group mode right takes the first follower and left
/// the leader; in single mode it goes round. Whoever it lands on is
/// selected alone (`SelectGroup(boss)` with a group of one, or
/// `SelectCharacter`), so single mode is on after any change. `None` when
/// nothing changes.
pub fn cycle_party(selected: crate::save::Selection, single_mode: bool, companions: &[usize], right: bool)
    -> Option<(crate::save::Selection, bool)> {
    use crate::save::Selection;
    let members = 1 + companions.len();
    let index = |s: Selection| match s {
        Selection::Leader => Some(0),
        Selection::Companion(id) => companions.iter().position(|&c| c == id).map(|i| i + 1),
        Selection::Camper(_) => None,
    };
    let target = match index(selected) {
        None => 0,
        Some(_) if members == 1 => return None,
        Some(_) if !single_mode => usize::from(right),
        Some(now) => (now + if right { 1 } else { members - 1 }) % members,
    };
    let next = if target == 0 { Selection::Leader } else { Selection::Companion(companions[target - 1]) };
    Some((next, true))
}

/// The world map's `CyclePartyCharacter`: as [`cycle_party`] from the
/// selection now, but a group of one does nothing, and a selection outside
/// the group (a camper) counts from the leader's place instead of going back
/// to him. Single mode is on after it either way.
pub fn cycle_party_on_map(selected: crate::save::Selection, single_mode: bool, companions: &[usize], right: bool)
    -> Option<(crate::save::Selection, bool)> {
    use crate::save::Selection;
    let members = 1 + companions.len();
    if members <= 1 {
        return None;
    }
    let target = if !single_mode {
        usize::from(right)
    } else {
        let now = match selected {
            Selection::Companion(id) => companions.iter().position(|&c| c == id).map_or(0, |i| i + 1),
            _ => 0,
        };
        (now + if right { 1 } else { members - 1 }) % members
    };
    let next = if target == 0 { Selection::Leader } else { Selection::Companion(companions[target - 1]) };
    Some((next, true))
}

/// Square root by Newton's method, for [`acos`]: `core` has none.
fn sqrt64(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut r = if x > 1.0 { x } else { 1.0 };
    for _ in 0..64 {
        let next = 0.5 * (r + x / r);
        if next >= r {
            break;
        }
        r = next;
    }
    r
}

/// Arc tangent of `t ≥ 0`: reduced below tan(π/12), then its series.
fn atan_positive(t: f64) -> f64 {
    const PI: f64 = core::f64::consts::PI;
    if t > 1.0 {
        return PI / 2.0 - atan_positive(1.0 / t);
    }
    // tan(π/12)
    if t > 0.267_949_192_431_122_7 {
        let s3 = 1.732_050_807_568_877_2;
        return PI / 6.0 + atan_positive((t * s3 - 1.0) / (s3 + t));
    }
    let (t2, mut term, mut sum) = (t * t, t, 0.0);
    let mut k = 0;
    while k < 40 {
        let add = term / (2 * k + 1) as f64;
        sum += if k % 2 == 0 { add } else { -add };
        term *= t2;
        k += 1;
    }
    sum
}

/// `Math.Acos`, the same on every build: `atan2(√(1−x²), x)`.
pub fn acos(x: f64) -> f64 {
    const PI: f64 = core::f64::consts::PI;
    let x = x.clamp(-1.0, 1.0);
    let y = sqrt64(1.0 - x * x);
    if x > 0.0 {
        atan_positive(y / x)
    } else if x < 0.0 {
        PI - atan_positive(y / -x)
    } else {
        PI / 2.0
    }
}

/// The eight directions `SelectLocationInDirection` ranks in, in its order.
pub const DIRECTIONS: [(i32, i32); 8] = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];

fn alignment(d: (f32, f32), dir: (i32, i32), distance: f32) -> f32 {
    let (dx, dy) = (dir.0 as f32, dir.1 as f32);
    let len = if dir.0 != 0 && dir.1 != 0 { core::f32::consts::SQRT_2 } else { 1.0 };
    (d.0 * dx + d.1 * dy) / (distance * len)
}

fn directional_score(d: (f32, f32), dir: (i32, i32), distance: f32) -> f32 {
    const ANGULAR_WEIGHT: f32 = 0.75;
    let a = alignment(d, dir, distance).clamp(-1.0, 1.0) as f64;
    let normalized = acos(a) / (core::f64::consts::PI / 4.0);
    distance * (1.0 + ANGULAR_WEIGHT * (normalized * normalized) as f32)
}

/// `MapScene.SelectLocationInDirection`: from `current` (a point), the place
/// the arrow `dir` selects among `places` — each a location number and its
/// entrance's centre, already the ones the player may select (where he is,
/// or a place he could see INFO of or travel to; not the one selected).
/// Every direction is given the place best for it, a place going to the
/// direction it lines up with best; the asked direction's own is kept if it
/// is within 5 % of its best score. `None` when nothing lies that way.
pub fn select_in_direction(current: (f32, f32), places: &[(usize, (f32, f32))], dir: (i32, i32)) -> Option<usize> {
    let cands: Vec<(usize, (f32, f32), f32)> = places
        .iter()
        .map(|&(l, c)| {
            let d = (c.0 - current.0, c.1 - current.1);
            let (dx, dy) = (d.0 as f64, d.1 as f64);
            (l, d, sqrt64(dx * dx + dy * dy) as f32)
        })
        .filter(|c| c.1 != (0.0, 0.0))
        .collect();
    let rankings: Vec<Vec<usize>> = DIRECTIONS
        .iter()
        .map(|&dd| {
            let mut r: Vec<usize> = (0..cands.len())
                .filter(|&i| cands[i].1 .0 * dd.0 as f32 + cands[i].1 .1 * dd.1 as f32 > 0.0)
                .collect();
            // `List.Sort` is not stable; ties are left in index order here.
            r.sort_by(|&a, &b| {
                directional_score(cands[a].1, dd, cands[a].2)
                    .partial_cmp(&directional_score(cands[b].1, dd, cands[b].2))
                    .unwrap_or(core::cmp::Ordering::Equal)
            });
            r
        })
        .collect();
    let n = DIRECTIONS.len();
    let mut assigned: Vec<Option<usize>> = alloc::vec![None; n];
    let mut index = alloc::vec![0usize; n];
    let mut owner: Vec<(usize, usize)> = Vec::new();
    let mut pending: alloc::collections::VecDeque<usize> = (0..n).collect();
    while let Some(d) = pending.pop_front() {
        while index[d] < rankings[d].len() {
            let c = rankings[d][index[d]];
            match owner.iter().find(|(cand, _)| *cand == c).map(|&(_, o)| o) {
                None => {
                    assigned[d] = Some(c);
                    owner.push((c, d));
                    break;
                }
                Some(o) => {
                    let challenger = alignment(cands[c].1, DIRECTIONS[d], cands[c].2);
                    let holder = alignment(cands[c].1, DIRECTIONS[o], cands[c].2);
                    if challenger > holder + 0.001 {
                        assigned[o] = None;
                        index[o] += 1;
                        pending.push_back(o);
                        assigned[d] = Some(c);
                        owner.retain(|(cand, _)| *cand != c);
                        owner.push((c, d));
                        break;
                    }
                    index[d] += 1;
                }
            }
        }
    }
    let r = DIRECTIONS.iter().position(|&dd| dd == dir)?;
    let best = *rankings[r].first()?;
    let mut result = assigned[r].unwrap_or(best);
    if result != best {
        let best_score = directional_score(cands[best].1, dir, cands[best].2);
        if directional_score(cands[result].1, dir, cands[result].2) > best_score * 1.05 {
            result = best;
        }
    }
    Some(cands[result].0)
}

/// Square root of an `f32`, for code that has no `std`.
pub fn sqrt32(x: f32) -> f32 {
    sqrt64(x as f64) as f32
}

// -- the sticks (`BurntimeGame.HandleGamePadInput`) -------------------------

/// MonoGame's circular dead zone for a thumbstick (`GamePadDeadZone.Circular`,
/// 7849 of 32767), and v1.2's threshold over it.
const DEAD_ZONE: f32 = 7849.0 / 32767.0;
const STICK_THRESHOLD: f32 = 0.25;

/// A stick's axes, -1..1 with up positive (MonoGame's sense), after the
/// circular dead zone and the rescale MonoGame applies.
fn filtered(x: i16, y: i16) -> (f32, f32) {
    let (x, y) = (x as f32 / 32767.0, -(y as f32) / 32767.0);
    let m = sqrt32(x * x + y * y);
    if m <= DEAD_ZONE {
        return (0.0, 0.0);
    }
    let scaled = ((m - DEAD_ZONE) / (1.0 - DEAD_ZONE)).min(1.0);
    (x / m * scaled, y / m * scaled)
}

/// The held directions of one stick: `(x, y)` in screen sense (y down),
/// each -1, 0 or 1 — two at once for a diagonal.
fn held((x, y): (f32, f32)) -> (i32, i32) {
    let h = if x >= STICK_THRESHOLD { 1 } else if x <= -STICK_THRESHOLD { -1 } else { 0 };
    let v = if y >= STICK_THRESHOLD { -1 } else if y <= -STICK_THRESHOLD { 1 } else { 0 };
    (h, v)
}

/// `GetCardinalStickDirection` with no direction before: the stronger axis.
fn cardinal((x, y): (f32, f32)) -> (i32, i32) {
    let (ax, ay) = (x.abs(), y.abs());
    match (ax >= STICK_THRESHOLD, ay >= STICK_THRESHOLD) {
        (false, false) => (0, 0),
        (true, false) => (if x < 0.0 { -1 } else { 1 }, 0),
        (false, true) => (0, if y < 0.0 { 1 } else { -1 }),
        _ if ax > ay => (if x < 0.0 { -1 } else { 1 }, 0),
        _ => (0, if y < 0.0 { 1 } else { -1 }),
    }
}

/// What a frame of the sticks gives: one navigation press when the left
/// stick leaves the middle (latched until it comes back), diagonal where
/// the scene takes diagonals (`UseDiagonalGamepadNavigation`), and a press
/// for each right-stick direction as it starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StickPresses {
    pub navigate: Option<(i32, i32)>,
    pub pan: alloc::vec::Vec<(i32, i32)>,
}

/// Both sticks as the last axis events left them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sticks {
    /// Raw axes: left x, left y, right x, right y (SDL's, y down).
    pub axes: [i16; 4],
    latched: bool,
    pan_before: (i32, i32),
}

impl Sticks {
    /// The left stick held: `ManualPath`'s direction, the selection's.
    pub fn moving(&self) -> (i32, i32) {
        held(filtered(self.axes[0], self.axes[1]))
    }

    /// The right stick held: the camera's pan.
    pub fn panning(&self) -> (i32, i32) {
        held(filtered(self.axes[2], self.axes[3]))
    }

    /// An axis moved (`SDL_CONTROLLER_AXIS_LEFTX` … `RIGHTY`, 0–3). What
    /// it presses is read once a frame, from both axes together, as v1.2
    /// reads the pad's state ([`Sticks::frame`]).
    pub fn axis(&mut self, axis: usize, value: i16) {
        if let Some(a) = self.axes.get_mut(axis) {
            *a = value;
        }
    }

    /// A frame's presses from the sticks as they stand.
    pub fn frame(&mut self, diagonal: bool) -> StickPresses {
        let mut out = StickPresses::default();
        let m = self.moving();
        if m == (0, 0) {
            self.latched = false;
        } else if !self.latched {
            self.latched = true;
            out.navigate = Some(if diagonal && m.0 != 0 && m.1 != 0 { m } else { cardinal(filtered(self.axes[0], self.axes[1])) });
        }
        let p = self.panning();
        for (now, before, dir) in [(p.1 < 0, self.pan_before.1 < 0, (0, -1)), (p.1 > 0, self.pan_before.1 > 0, (0, 1)),
                                   (p.0 < 0, self.pan_before.0 < 0, (-1, 0)), (p.0 > 0, self.pan_before.0 > 0, (1, 0))] {
            if now && !before {
                out.pan.push(dir);
            }
        }
        self.pan_before = p;
        out
    }
}
