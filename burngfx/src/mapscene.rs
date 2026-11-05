//! The world map as v0.9.1's `MapScene` has it, as state and arithmetic: the
//! roads it draws and where, the player markers, the status line, the turn's
//! clock, and the right-click menu. The program draws what this says.
//!
//! Sources: `ClassicMapView`, `MainUiOriginalWindow`, `MapView`,
//! `MapViewOverlayPlayer`, `MapViewOverlayFlags`, `MenuWindow`, and the
//! BurnGfx `WayProcessor`, whose road table is copied below entry for entry.

use alloc::vec::Vec;

/// One drawn road: the `syst.raw` frames laid left to right, 32 apart, and
/// the nudge `WayProcessor` adds to where it starts.
pub struct WaySprite {
    pub frames: &'static [u8],
    pub offset: (i32, i32),
}

/// `WayProcessor`'s table, by way id — the number `gam.dat` gives each road.
/// Its position is the smaller of the two doorway centres, each axis on its
/// own, plus `offset` (and `MAT_000-ways.txt`'s `burngfx_offset`, 0x0).
///
/// The remaster's one extra way, 8 ↔ 37 (`maps/ways/newvillage_0.png`), leads
/// to the location it added, which has no `gam.dat` record and is not packed.
pub const WAYS: [WaySprite; 47] = [
    WaySprite { frames: &[28], offset: (7, 5) },
    WaySprite { frames: &[30], offset: (10, 6) },
    WaySprite { frames: &[31, 32], offset: (20, 3) },
    WaySprite { frames: &[33, 34, 35], offset: (12, 7) },
    WaySprite { frames: &[36, 37, 38], offset: (10, -1) },
    WaySprite { frames: &[39], offset: (14, 2) },
    WaySprite { frames: &[40], offset: (6, 0) },
    WaySprite { frames: &[41, 42], offset: (4, 6) },
    WaySprite { frames: &[43, 44, 45], offset: (2, 4) },
    WaySprite { frames: &[46, 47], offset: (7, 3) },
    WaySprite { frames: &[48], offset: (5, 4) },
    WaySprite { frames: &[49, 50], offset: (4, 7) },
    WaySprite { frames: &[51, 52], offset: (3, 7) },
    WaySprite { frames: &[53, 54], offset: (6, 2) },
    WaySprite { frames: &[29], offset: (10, 3) },
    WaySprite { frames: &[55, 56], offset: (3, 7) },
    WaySprite { frames: &[57], offset: (5, 7) },
    WaySprite { frames: &[58], offset: (5, 2) },
    WaySprite { frames: &[59, 60], offset: (5, 0) },
    WaySprite { frames: &[61, 62], offset: (12, 5) },
    WaySprite { frames: &[63], offset: (8, 9) },
    WaySprite { frames: &[64, 65, 66], offset: (8, -1) },
    WaySprite { frames: &[68, 69], offset: (1, 7) },
    WaySprite { frames: &[70], offset: (-4, 6) },
    WaySprite { frames: &[71, 72], offset: (11, 3) },
    WaySprite { frames: &[73, 74, 75, 76], offset: (8, 4) },
    WaySprite { frames: &[77], offset: (-20, 10) },
    WaySprite { frames: &[78, 79, 80], offset: (11, 7) },
    WaySprite { frames: &[81], offset: (2, 7) },
    WaySprite { frames: &[82, 83], offset: (20, 3) },
    WaySprite { frames: &[84], offset: (-3, 8) },
    WaySprite { frames: &[85, 86], offset: (10, 3) },
    WaySprite { frames: &[87], offset: (-2, 8) },
    WaySprite { frames: &[88], offset: (13, 4) },
    WaySprite { frames: &[89, 90], offset: (10, 2) },
    WaySprite { frames: &[91], offset: (5, 7) },
    WaySprite { frames: &[92], offset: (3, 3) },
    WaySprite { frames: &[93, 94], offset: (4, 4) },
    WaySprite { frames: &[95, 96], offset: (13, -1) },
    WaySprite { frames: &[97], offset: (-3, 8) },
    WaySprite { frames: &[98, 99], offset: (19, 5) },
    WaySprite { frames: &[100], offset: (8, 8) },
    WaySprite { frames: &[101, 102], offset: (12, 6) },
    WaySprite { frames: &[103, 104, 105, 106], offset: (12, 4) },
    WaySprite { frames: &[107, 108], offset: (2, 7) },
    WaySprite { frames: &[109], offset: (-2, 7) },
    WaySprite { frames: &[110, 111], offset: (10, 7) },
];

/// Where a way's first sprite goes on the map, given the centres of the two
/// doorways it joins.
pub fn way_position(way: usize, a: (i32, i32), b: (i32, i32)) -> Option<(i32, i32)> {
    let w = WAYS.get(way)?;
    Some((a.0.min(b.0) + w.offset.0, a.1.min(b.1) + w.offset.1))
}

/// `syst.raw` frame of a player's marker, by `IconID`: `{17, 16, 19, 18}`.
pub const MARKERS: [u8; 4] = [17, 16, 19, 18];
/// `burngfxani@syst.raw?0-3` for green, `?8-11` for red.
pub const FLAG_GREEN: u8 = 0;
pub const FLAG_RED: u8 = 8;
/// The flags' shared clock: `SpriteAnimation(12)` at speed 15, each flag's
/// four frames taken as `frame * 4 / 12`.
pub fn flag_frame(ms: u32) -> u8 {
    ((ms * 15 / 1000) % 12 * 4 / 12) as u8
}
/// The mode cursor, `syst.raw?24-27` travelling, `?20-23` in info mode. Not
/// progressive, at the default five frames a second.
pub fn mode_cursor_frame(info: bool, ms: u32) -> u8 {
    (if info { 20 } else { 24 }) + ((ms * 5 / 1000) % 4) as u8
}

/// `GameCreation`'s player colours: name ink and the like.
pub const GREEN: u32 = 0x00_00D0_00;
pub const RED: u32 = 0x00_F040_38;

/// Where a player's marker is drawn, before `(-width/2, -15)`: at the
/// doorway's left edge and middle height, 3 px further for each earlier player
/// in the same place — or, on the road, `end + (start − end) · left / total`
/// between the two doorway centres.
pub fn marker_position(
    door: (i32, i32, i32, i32),
    journey: Option<((i32, i32), (i32, i32), u8, u8)>,
    earlier_here: i32,
) -> (i32, i32) {
    match journey {
        Some((start, end, left, total)) if total > 0 => (
            end.0 + (start.0 - end.0) * left as i32 / total as i32,
            end.1 + (start.1 - end.1) * left as i32 / total as i32,
        ),
        _ => (door.0 + 3 * earlier_here, door.1 + door.3 / 2 + 3 * earlier_here),
    }
}

/// `Group.GetLowestAfterDistribution`: what the worst-off member would have if
/// what is carried were shared out one day at a time to whoever has least.
pub fn lowest_after_distribution(reserves: &[i32], mut distribute: i32) -> i32 {
    if reserves.is_empty() {
        return 0;
    }
    let mut r: Vec<i32> = reserves.to_vec();
    if distribute <= 0 {
        return r.iter().copied().min().unwrap_or(0);
    }
    let mut lowest = r.iter().copied().min().unwrap_or(0).min(0);
    while distribute > 0 {
        for v in r.iter_mut() {
            if *v == lowest {
                distribute -= 1;
                *v += 1;
                if distribute == 0 {
                    break;
                }
            }
        }
        lowest += 1;
    }
    r.iter().copied().min().unwrap_or(0)
}

/// What the status line says, `MainUiOriginalWindow.OnRender`'s chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// `@newburn?38`, "too little water", in the warning ink.
    TooLittleWater,
    /// `@newburn?39`, "too little food".
    TooLittleFood,
    /// The location's danger line (`burn?408` radiation, `?413` gas, with
    /// its figure for `|E`), **in the warning ink**: somebody in the group
    /// is not fully protected.
    InDanger,
    /// The same line in the day's quiet ink: there is a danger here and the
    /// whole group is shielded from it.
    DangerHere,
}

/// The location's danger as the status line sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hazard {
    /// No danger here.
    None,
    /// `Group.IsInDanger()`: the members' `GetDangerRate` add up to more than
    /// nothing — at least one of them carries less than full protection.
    Unprotected,
    /// A danger, and every member fully protected against it.
    Protected,
}

impl Status {
    /// The `newburn` line, for the two that have one. The danger's text is
    /// the location's own and the caller supplies it.
    pub fn newburn(self) -> Option<u32> {
        match self {
            Status::TooLittleWater => Some(38),
            Status::TooLittleFood => Some(39),
            Status::InDanger | Status::DangerHere => None,
        }
    }
}

/// `expected` is the road length to the doorway under the pointer, 0 when
/// there is none.
pub fn status(lowest_water: i32, lowest_food: i32, expected: i32, hazard: Hazard) -> Option<Status> {
    if lowest_water < expected {
        Some(Status::TooLittleWater)
    } else if lowest_food < expected {
        Some(Status::TooLittleFood)
    } else if hazard == Hazard::Unprotected {
        Some(Status::InDanger)
    } else if lowest_water == 0 {
        Some(Status::TooLittleWater)
    } else if lowest_food == 0 {
        Some(Status::TooLittleFood)
    } else if hazard == Hazard::Protected {
        Some(Status::DangerHere)
    } else {
        None
    }
}

/// v1.2's order (`MainUiOriginalWindow.OnRender`): nothing to drink or to
/// eat first, then too little for the journey under the pointer, then the
/// danger. v0.9.1 put the journey first and the danger before an empty
/// reserve.
pub fn status_v12(lowest_water: i32, lowest_food: i32, expected: i32, hazard: Hazard) -> Option<Status> {
    if lowest_water == 0 {
        Some(Status::TooLittleWater)
    } else if lowest_food == 0 {
        Some(Status::TooLittleFood)
    } else if expected > 0 && lowest_water < expected {
        Some(Status::TooLittleWater)
    } else if expected > 0 && lowest_food < expected {
        Some(Status::TooLittleFood)
    } else if hazard == Hazard::Unprotected {
        Some(Status::InDanger)
    } else if hazard == Hazard::Protected {
        Some(Status::DangerHere)
    } else {
        None
    }
}

/// `World.Time`: a day's clock, 1.0 at the start of a turn, run down over
/// `roundtime` seconds (240 unless `settings.txt` says otherwise). At zero the
/// turn ends. Kept in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurnClock {
    pub left_ms: u32,
    pub round_ms: u32,
}

/// The default when `settings.txt` has no usable `roundtime`:
/// `GetInt("roundtime") > 0 ? … : 240`.
pub const ROUND_SECONDS: u32 = 240;

/// `Settings["game"].GetInt("roundtime")` from `settings.txt`, or 240 when it
/// is missing or not above zero. Comments start with `'`.
pub fn round_seconds(settings: &str) -> u32 {
    let mut section = "";
    for line in settings.trim_start_matches('\u{feff}').lines() {
        let line = line.split('\'').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name;
            continue;
        }
        if section != "game" {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "roundtime" {
                return match v.trim().parse::<i64>() {
                    Ok(n) if n > 0 => n as u32,
                    _ => ROUND_SECONDS,
                };
            }
        }
    }
    ROUND_SECONDS
}

impl TurnClock {
    pub fn new(round_seconds: u32) -> TurnClock {
        let round_ms = round_seconds.max(1) * 1000;
        TurnClock { left_ms: round_ms, round_ms }
    }

    /// `World.Turn` puts it back to a full day.
    pub fn reset(&mut self) {
        self.left_ms = self.round_ms;
    }

    /// Run it down. True once it is at zero.
    pub fn run(&mut self, ms: u32) -> bool {
        self.left_ms = self.left_ms.saturating_sub(ms);
        self.left_ms == 0
    }

    /// The red bar at the top: `(int)(Time * 60)` pixels.
    pub fn bar(&self) -> i32 {
        (self.left_ms as u64 * 60 / self.round_ms as u64) as i32
    }
}

/// The map's right-click menu: `MenuWindow` with `MapScene`'s five lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Info,
    Travel,
    Inventory,
    Statistics,
    Options,
    Next,
    // LocationScene's.
    Speak,
    Fight,
    Single,
    All,
    MainMap,
    /// `OnMenuDismiss`, `OnMenuMakeCamp` (STAY THERE), `OnMenuLeaveCamp`
    /// (REJOIN): about the selected character.
    Dismiss,
    StayThere,
    Rejoin,
    /// v1.2's MANUAL (`@manualui?5`): opens the manual.
    Manual,
}

impl Command {
    /// A line of `manualui.txt` rather than `burn.txt`: MANUAL is `?5`.
    pub fn manualui(self) -> Option<usize> {
        matches!(self, Command::Manual).then_some(5)
    }

    /// `@burn?N`: INFO 351, TRAVEL 360, INVENTORY 367, STATISTIC 359,
    /// OPTIONS 361, NEXT 357.
    pub fn burn(self) -> u32 {
        match self {
            Command::Info => 351,
            Command::Travel => 360,
            Command::Inventory => 367,
            Command::Statistics => 359,
            Command::Options => 361,
            Command::Next => 357,
            Command::Speak => 350,
            Command::Fight => 352,
            Command::Single => 358,
            Command::All => 356,
            Command::MainMap => 362,
            Command::Dismiss => 363,
            Command::StayThere => 364,
            Command::Rejoin => 365,
            // Not a `burn.txt` line: [`Command::manualui`] says which.
            Command::Manual => 0,
        }
    }
}

/// `charOverlay.SelectedCharacter`, as far as the menu asks: the leader,
/// somebody walking with the group, or one of the player's campers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selected {
    Leader,
    InGroup,
    Camper,
}

/// The menu's width, `Size = (68, 10 + 11 * lines)`.
pub const MENU_W: i32 = 68;
/// `munt.raw?24` is 4 high, each line `munt.raw?25` 11.
pub const MENU_TOP: i32 = 4;
pub const MENU_LINE: i32 = 11;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub lines: Vec<Command>,
    /// Top left, in scene coordinates, while shown.
    pub at: Option<(i32, i32)>,
    pub hover: Option<usize>,
}

impl Menu {
    /// `MapScene`'s: the first line is INFO in travel mode and TRAVEL in info
    /// mode (`OnMenuInfo` / `OnMenuTravel` swap it).
    pub fn for_map(info_mode: bool) -> Menu {
        let first = if info_mode { Command::Travel } else { Command::Info };
        Menu {
            lines: alloc::vec![first, Command::Inventory, Command::Statistics, Command::Options, Command::Next],
            at: None,
            hover: None,
        }
    }

    /// v1.2's `MapScene.ConfigureMenu(includeInteractionMode: true)`: as
    /// v0.9.1's, with MANUAL before NEXT (`MapScene.cs:265`).
    pub fn for_map_v12(info_mode: bool) -> Menu {
        let mut menu = Menu::for_map(info_mode);
        menu.lines.insert(menu.lines.len() - 1, Command::Manual);
        menu
    }

    /// `LocationScene.ShowMenu`, built afresh on every right click. With
    /// somebody other than the leader selected, DISMISS, and then STAY THERE
    /// for a member of the group or REJOIN for one of the player's campers.
    pub fn for_location(is_city: bool, fight_mode: bool, group: usize, single_mode: bool,
                        selected: Selected) -> Menu {
        let mut lines = alloc::vec::Vec::new();
        if !is_city {
            lines.push(Command::Info);
            lines.push(if fight_mode { Command::Speak } else { Command::Fight });
        }
        if group > 1 {
            lines.push(if single_mode { Command::All } else { Command::Single });
        }
        match selected {
            Selected::Leader => {}
            Selected::InGroup => {
                lines.push(Command::Dismiss);
                lines.push(Command::StayThere);
            }
            Selected::Camper => {
                lines.push(Command::Dismiss);
                lines.push(Command::Rejoin);
            }
        }
        lines.push(Command::MainMap);
        lines.push(Command::Inventory);
        lines.push(Command::Next);
        Menu { lines, at: None, hover: None }
    }

    /// v1.2's `LocationScene.ShowActionsMenu`, opened with the mouse. Outside
    /// a city: FIGHT or SPEAK first, then INFO, INVENTORY, the map, the
    /// group's lines, OPTIONS and NEXT. In a city: INVENTORY, the group's
    /// lines, the map, OPTIONS, NEXT. The group's lines
    /// (`AddGroupMenuLines`): SINGLE or ALL with more than one in the group;
    /// for a camper REJOIN (while the group has room) then DISMISS, for a
    /// member DISMISS then STAY THERE — outside a city only, as v1.3 has it
    /// (`d9a1221`, `LocationScene.cs:1111-1113` at `v1.3`); MANUAL between
    /// OPTIONS and NEXT (`LocationScene.cs:991`).
    pub fn for_location_v12(is_city: bool, fight_mode: bool, group: usize, single_mode: bool,
                            selected: Selected) -> Menu {
        let mut lines = alloc::vec::Vec::new();
        let group_lines = |lines: &mut alloc::vec::Vec<Command>| {
            if group > 1 {
                lines.push(if single_mode { Command::All } else { Command::Single });
            }
            // v1.3 (`d9a1221`, "fix cities allowing camp creation"): a city
            // has no follower lines — no REJOIN, DISMISS or STAY THERE.
            if is_city {
                return;
            }
            match selected {
                Selected::Leader => {}
                Selected::InGroup => {
                    lines.push(Command::Dismiss);
                    lines.push(Command::StayThere);
                }
                Selected::Camper => {
                    if group < crate::hire::MAX_GROUP {
                        lines.push(Command::Rejoin);
                    }
                    lines.push(Command::Dismiss);
                }
            }
        };
        if is_city {
            lines.push(Command::Inventory);
            group_lines(&mut lines);
            lines.push(Command::MainMap);
        } else {
            lines.push(if fight_mode { Command::Speak } else { Command::Fight });
            lines.push(Command::Info);
            lines.push(Command::Inventory);
            lines.push(Command::MainMap);
            group_lines(&mut lines);
        }
        lines.push(Command::Options);
        lines.push(Command::Manual);
        lines.push(Command::Next);
        Menu { lines, at: None, hover: None }
    }

    pub fn height(&self) -> i32 {
        10 + MENU_LINE * self.lines.len() as i32
    }

    /// `Show(position, boundings)`: centred on the click, then moved inside
    /// the map view (`MoveInside`).
    pub fn show(&mut self, x: i32, y: i32, bounds: (i32, i32, i32, i32)) {
        let (w, h) = (MENU_W, self.height());
        let mut left = x - w / 2;
        let mut top = y - h / 2;
        let (bx, by, bw, bh) = bounds;
        left = left.min(bx + bw - w).max(bx);
        top = top.min(by + bh - h).max(by);
        self.at = Some((left, top));
        self.hover = None;
    }

    /// `OnMouseMove`: a line is hovered only over its text, whose width the
    /// caller measures.
    pub fn point(&mut self, x: i32, y: i32, text_width: impl Fn(Command) -> i32) {
        self.hover = None;
        let Some((left, top)) = self.at else { return };
        let (px, py) = (x - left, y - top - MENU_TOP);
        if py < 0 || x < left || x >= left + MENU_W || y >= top + self.height() {
            return;
        }
        let item = (py / MENU_LINE) as usize;
        if let Some(&c) = self.lines.get(item) {
            let w = text_width(c);
            if px >= MENU_W / 2 - w / 2 && px < MENU_W / 2 + w / 2 {
                self.hover = Some(item);
            }
        }
    }

    /// `OnMouseClick`. The menu is modal: any click closes it, and a left
    /// click on a hovered line returns its command.
    pub fn click(&mut self, left_button: bool) -> Option<Command> {
        let picked = if left_button { self.hover.and_then(|i| self.lines.get(i).copied()) } else { None };
        self.at = None;
        self.hover = None;
        picked
    }

    pub fn is_open(&self) -> bool {
        self.at.is_some()
    }

    /// v1.2's `ShowActionsMenu` opened by a key (Back) in a place: the map,
    /// the inventory, INFO outside a city, OPTIONS, MANUAL, NEXT — no
    /// interaction mode and no group lines.
    pub fn for_location_v12_keys(is_city: bool) -> Menu {
        let mut lines = alloc::vec![Command::MainMap, Command::Inventory];
        if !is_city {
            lines.push(Command::Info);
        }
        lines.push(Command::Options);
        lines.push(Command::Manual);
        lines.push(Command::Next);
        Menu { lines, at: None, hover: None }
    }

    /// `MapScene.ConfigureMenu(includeInteractionMode: false)`: the world
    /// map's menu opened by a key.
    pub fn for_map_keys() -> Menu {
        Menu {
            lines: alloc::vec![Command::Inventory, Command::Statistics, Command::Options, Command::Manual, Command::Next],
            at: None,
            hover: None,
        }
    }

    /// `MenuWindow.OnInputAction` Up and Down: the focus round the lines,
    /// from the first. The focus is the line drawn lit.
    pub fn focus_step(&mut self, down: bool) {
        let n = self.lines.len();
        if n == 0 {
            return;
        }
        let at = self.hover.unwrap_or(0);
        self.hover = Some(if down { (at + 1) % n } else { (at + n - 1) % n });
    }

    /// `MenuWindow.OnInputAction` Primary: the first press only focuses the
    /// first line; after that, the focused line runs and the menu closes.
    pub fn primary(&mut self) -> Option<Command> {
        match self.hover {
            None => {
                self.hover = (!self.lines.is_empty()).then_some(0);
                None
            }
            Some(_) => self.click(true),
        }
    }

    /// Back: closed, nothing run.
    pub fn close(&mut self) {
        self.at = None;
        self.hover = None;
    }

    /// `ShowGroupMenu` (Secondary in a place): `AddGroupMenuLines` alone —
    /// SINGLE or ALL for a group of more than one; for anybody but the
    /// leader REJOIN (a camper, while the group has room), DISMISS, STAY
    /// THERE (one of the group), **outside a city only** (v1.3, `d9a1221`:
    /// `HasGroupMenuCommands` and `AddGroupMenuLines` test `IsCity`, so in a
    /// city the menu is the SINGLE/ALL line or nothing). `None` when there
    /// would be no line (`HasGroupMenuCommands`).
    pub fn for_group(is_city: bool, group: usize, single_mode: bool, selected: Selected) -> Option<Menu> {
        let mut lines = alloc::vec::Vec::new();
        if group > 1 {
            lines.push(if single_mode { Command::All } else { Command::Single });
        }
        let selected = if is_city { Selected::Leader } else { selected };
        match selected {
            Selected::Leader => {}
            Selected::InGroup => {
                lines.push(Command::Dismiss);
                lines.push(Command::StayThere);
            }
            Selected::Camper => {
                if group < crate::hire::MAX_GROUP {
                    lines.push(Command::Rejoin);
                }
                lines.push(Command::Dismiss);
            }
        }
        (!lines.is_empty()).then_some(Menu { lines, at: None, hover: None })
    }
}

/// `MapView`'s scrolling: the offset is ≤ 0 and ≥ view − map on each axis,
/// and a map smaller than the view is centred.
pub fn clamp_scroll(offset: (i32, i32), view: (i32, i32), map: (i32, i32)) -> (i32, i32) {
    let axis = |o: i32, v: i32, m: i32| if m < v { (v - m) / 2 } else { o.min(0).max(v - m) };
    (axis(offset.0, view.0, map.0), axis(offset.1, view.1, map.1))
}

/// `MapView.OnMouseMove`'s edge scroll: 1 inside 10 px of an edge, 2 inside 4,
/// pushing the map the other way. Speed is 70 px a second per unit.
pub fn edge_push(x: i32, y: i32, view: (i32, i32)) -> (i32, i32) {
    let axis = |p: i32, size: i32| {
        let mut b = 0;
        if p < 4 { b += 1 }
        if p > size - 4 { b -= 1 }
        if p < 10 { b += 1 }
        if p > size - 10 { b -= 1 }
        b
    };
    (axis(x, view.0), axis(y, view.1))
}

pub const SCROLL_SPEED: i32 = 70;

#[cfg(test)]
mod tests {
    use super::*;

    /// v1.3's `d9a1221`: in a city a selected companion or camper gets no
    /// REJOIN, DISMISS or STAY THERE, in either menu; SINGLE/ALL stays.
    #[test]
    fn a_city_has_no_follower_lines() {
        for selected in [Selected::InGroup, Selected::Camper] {
            let city = Menu::for_location_v12(true, false, 3, false, selected);
            assert_eq!(city.lines, [Command::Inventory, Command::Single, Command::MainMap,
                                    Command::Options, Command::Manual, Command::Next]);
            let group = Menu::for_group(true, 3, true, selected).expect("ALL for a group of three");
            assert_eq!(group.lines, [Command::All]);
            // A leader alone in a city: no line at all, so no group menu.
            assert!(Menu::for_group(true, 1, false, selected).is_none());
        }
    }

    /// Outside a city nothing changed: the follower lines are v1.2's.
    #[test]
    fn outside_a_city_the_follower_lines_stay() {
        let member = Menu::for_group(false, 2, false, Selected::InGroup).unwrap();
        assert_eq!(member.lines, [Command::Single, Command::Dismiss, Command::StayThere]);
        let camper = Menu::for_group(false, 1, false, Selected::Camper).unwrap();
        assert_eq!(camper.lines, [Command::Rejoin, Command::Dismiss]);
        let full = Menu::for_group(false, crate::hire::MAX_GROUP, false, Selected::Camper).unwrap();
        assert_eq!(full.lines, [Command::Single, Command::Dismiss], "no REJOIN into a full group");
        let place = Menu::for_location_v12(false, false, 2, false, Selected::InGroup);
        assert_eq!(place.lines, [Command::Fight, Command::Info, Command::Inventory, Command::MainMap,
                                 Command::Single, Command::Dismiss, Command::StayThere,
                                 Command::Options, Command::Manual, Command::Next]);
        assert!(Menu::for_group(false, 1, false, Selected::Leader).is_none());
    }
}
