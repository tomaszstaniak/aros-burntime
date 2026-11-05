//! `InfoScene`, `StatisticsScene` and `OptionsScene`: three full 320 × 200
//! scenes, drawn and handled as v0.9.1 lays them out. None of them runs
//! `World.Update`, so while one is open no time passes.
//!
//! What they show is worked out by the game (`main.rs`) from its own model and
//! handed in; this side places it, answers the pointer and says what a click
//! asked for.

use alloc::string::String;
use alloc::vec::Vec;
use burngfx::pack::{Pack, PackedImage};

use crate::gfx::{Align, Face, Target};
use crate::hud::Hud;
use crate::panels::{anim, text_helper, Anim};

// ---------------------------------------------------------------- colours

/// `InfoScene`: `titleFont` (156,156,156) and `font` (184,184,184).
const INFO_TITLE: u32 = 0x009C_9C9C;
const INFO_TEXT: u32 = 0x00B8_B8B8;
/// `ItemWindow`'s title, (240,64,56).
const ITEM_TITLE: u32 = 0x00F0_4038;
/// `StatisticsScene`: (212,212,212).
const STATS_TEXT: u32 = 0x00D4_D4D4;
/// `OptionsScene`'s fonts.
const RED: u32 = 0x0086_2C04;
const BLUE: u32 = 0x006D_75AA;
const DISABLED: u32 = 0x0064_6464;
const ORANGE: u32 = 0x00BE_4D0C;
const GREEN: u32 = 0x0000_6C00;

fn image<'a>(pack: &Pack<'a>, key: &str) -> Option<PackedImage<'a>> {
    crate::start_view::image(pack, key)
}

fn inside(p: (i32, i32), at: (i32, i32), size: (i32, i32)) -> bool {
    p.0 >= at.0 && p.1 >= at.1 && p.0 < at.0 + size.0 && p.1 < at.1 + size.1
}

fn size_of(img: &Option<PackedImage>) -> (i32, i32) {
    img.as_ref().map_or((0, 0), |i| (i.game_w(), i.game_h()))
}

/// The pictures of all three scenes, loaded once.
pub struct Art<'a> {
    info: Option<PackedImage<'a>>,
    blz: Option<PackedImage<'a>>,
    opti: Option<PackedImage<'a>>,
    map_button: Option<PackedImage<'a>>,
    map_button_hover: Option<PackedImage<'a>>,
    up: Option<PackedImage<'a>>,
    down: Option<PackedImage<'a>>,
    bulb: Option<PackedImage<'a>>,
    rad: Option<PackedImage<'a>>,
    gas: Option<PackedImage<'a>>,
    cover: Option<PackedImage<'a>>,
    cover_hover: Option<PackedImage<'a>>,
    /// `inf.ani?0-2` and `?4-6`, and `opt.ani`.
    inf_a: Anim<'a>,
    inf_b: Anim<'a>,
    opt: Anim<'a>,
    /// `syssze.raw?32`, `?48`, `?16`: fighter, technician, doctor.
    people: [Option<PackedImage<'a>>; 3],
    /// NewGfx's statistics (`StatisticsScene.cs:66-75`): `statistics.png`
    /// and its corners, left top, right top, left bottom, right bottom.
    stats_ng: Option<PackedImage<'a>>,
    stats_corners: [Option<PackedImage<'a>>; 4],
}

impl<'a> Art<'a> {
    pub fn load(pack: &Pack<'a>) -> Art<'a> {
        let first = |track: &str| {
            let mut key = String::from("an/");
            key.push_str(track);
            key.push_str("/0");
            image(pack, &key)
        };
        Art {
            info: image(pack, "scene/info"),
            blz: image(pack, "scene/blz"),
            opti: image(pack, "scene/opti"),
            // v1.2's German package has its own (`lang/de/gfx`).
            map_button: crate::localized(pack, "ui/mapbutton").and_then(PackedImage::parse),
            map_button_hover: crate::localized(pack, "ui/mapbuttonh").and_then(PackedImage::parse),
            up: image(pack, "ui/up"),
            down: image(pack, "ui/down"),
            bulb: image(pack, "ui/options_bulb"),
            rad: first("inf_rad"),
            gas: first("inf_gas"),
            cover: first("opta0"),
            cover_hover: first("opta1"),
            // `Animation.Speed = 6` for both of `InfoScene`'s; `opt.ani` is
            // given none and keeps `SpriteAnimation`'s default of 5.
            inf_a: anim(pack, "inf_a", (4, 105), 60, 0),
            inf_b: anim(pack, "inf_b", (234, 150), 60, 0),
            opt: anim(pack, "opt", (0, 4), 50, 0),
            people: [image(pack, "body/32"), image(pack, "body/48"), image(pack, "body/16")],
            stats_ng: crate::gfx::newgfx().then(|| crate::start_view::newgfx_image(pack, "n/statistics")).flatten(),
            stats_corners: core::array::from_fn(|i| {
                crate::gfx::newgfx().then(|| crate::start_view::newgfx_image(pack, ["n/stats_lt", "n/stats_rt", "n/stats_lb", "n/stats_rb"][i]))
                    .flatten()
            }),
        }
    }

    /// What the pack lacks, for the console.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for (have, name) in [
            (self.info.is_some(), "info.pac"),
            (self.blz.is_some(), "blz.pac"),
            (self.opti.is_some(), "opti.pac"),
            (self.map_button.is_some(), "gfx/mapbutton.png"),
            (self.bulb.is_some(), "gfx/ui/options_bulb.png"),
            (!self.inf_a.frames.is_empty(), "inf.ani"),
            (!self.opt.frames.is_empty(), "opt.ani"),
        ] {
            if !have {
                out.push(name);
            }
        }
        out
    }
}

fn draw_anim(t: &mut Target, a: &Anim, ms: u32) {
    if let Some(f) = a.frames.get(a.frame_at(ms)) {
        t.blit(f, a.at.0, a.at.1);
    }
}

/// A `Button` with a `Size` of its own: text placed in it by alignment.
fn draw_in_box(t: &mut Target, f: &Face, text: &str, at: (i32, i32), size: (i32, i32), v_centre: bool, ink: u32) {
    let (y, v) = if v_centre { (at.1 + size.1 / 2, Align::Centre) } else { (at.1, Align::Start) };
    f.draw(t, text, at.0 + size.0 / 2, y, Align::Centre, v, Some(ink));
}

// ------------------------------------------------------------------- INFO

/// One kind of item in the camp's rooms, and how many.
pub struct Stored {
    pub kind: usize,
    pub sprite: Option<u8>,
    pub count: i32,
    pub title: String,
}

/// `InfoScene`, for one location.
pub struct Info {
    pub city: usize,
    pub title: String,
    /// `burn?396`–`398`, `399`, `406`.
    pub labels: [String; 3],
    pub storage: String,
    pub available: String,
    /// Fighters, technicians, doctors camped there (`CampNPC`).
    pub residents: [usize; 3],
    /// v1.3: each of them's experience tier (from 1) and health in sevenths,
    /// in the same rows, when `RadioIntel.CanSeeHealth` lets the player see
    /// them; `None` draws the pictures alone, 18 apart, as v1.2.
    pub details: Option<[Vec<(i32, i32)>; 3]>,
    /// A v1.2-preset game's numbers as its rules give them: food a day, the
    /// technician's part of it, `Source.Water`.
    pub v12_numbers: Option<(i32, i32, i32)>,
    /// v1.3's two lines each for food and water (`newburn?82`/`?81`,
    /// `?80`/`?81`), drawn in place of `food` and `water` when set.
    pub v13_lines: Option<[[String; 2]; 2]>,
    /// Every kind in the rooms, ordered by type id as `SortedList` has them.
    pub items: Vec<Stored>,
    pub offset: usize,
    /// `AvailableProducts` and `productionID`; `None` is `-1`.
    pub products: [Option<usize>; 4],
    pub slot: Option<usize>,
    /// v1.2's `IsProductionAutomatic`, or `None` in the legacy game, which
    /// has no such thing. An automatic camp shows no slot chosen
    /// (`productionID = -1`).
    pub automatic: Option<bool>,
    /// The item the production makes: its sprite and title.
    pub produce: Option<(Option<u8>, String)>,
    /// `burn?421` and `422` with their numbers put in.
    pub food: String,
    pub water: String,
    /// `Danger.InfoIcon` (radiation or not) and `InfoString`.
    pub danger: Option<(bool, String)>,
    /// v1.2's tooltips: the production's (`UpdateProductionTooltip`) and the
    /// stored items' (`ItemGridTooltip`, parallel to `items`). `None` and
    /// empty in the legacy game, which has neither.
    pub production_tip: Option<ProductionTip>,
    pub facts: Vec<Vec<String>>,
    /// `LastInputMode` is not the mouse: the production tooltip shows
    /// without the pointer on it.
    pub keyboard: bool,
    /// `productionTooltipDismissed`: Primary hid it until the production
    /// changes.
    pub tip_dismissed: bool,
}

/// v1.2's production tooltip on INFO: `tooltip?66` over a line per valid
/// production (`?67`, its tools in the rooms or `?68`), `?76` while the
/// choice is automatic; the prompts Next and Previous (`?75`, `?74`) at the
/// mouse, Change (`?69`) at the keys.
#[derive(Clone, Debug, Default)]
pub struct ProductionTip {
    pub header: String,
    pub lines: Vec<String>,
    pub automatic: Option<String>,
    /// v1.3: the line of the production in use — 0, "Auto", while the
    /// choice is automatic.
    pub highlight: Option<usize>,
    /// Next (Primary, the left button) and Previous (Secondary, the right).
    pub mouse: Vec<(String, burngfx::prompts::Prompt)>,
    /// Change (`HorizontalNavigation`).
    pub keys: Vec<(String, burngfx::prompts::Prompt)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoClick {
    Back,
    /// A new production slot, already stepped as `OnProductionLeft`/`Right`.
    /// In a v1.2 game it is the player's own choice (`SelectProduction`).
    Production(usize),
    /// v1.2's `SelectAutomaticFoodProduction`: a right click on the first
    /// slot.
    Automatic,
    None,
}

pub(crate) const INFO_BACK: (i32, i32) = (5, 174);
pub(crate) const INFO_UP: (i32, i32) = (107, 122);
pub(crate) const INFO_DOWN: (i32, i32) = (107, 141);
pub(crate) const INFO_PRODUCTION: (i32, i32) = (230, 105);
const INFO_CELLS: [(i32, i32); 2] = [(137, 105), (137, 143)];

impl Info {
    /// The same screen in a v1.2 game: whether the camp's production is the
    /// player's choice decides which slot shows as chosen.
    pub fn v12(mut self, manual: bool) -> Info {
        self.automatic = Some(!manual);
        if !manual {
            self.slot = None;
        }
        self
    }

    fn shown(&self) -> &[Stored] {
        let from = self.offset.min(self.items.len());
        &self.items[from..(from + 2).min(self.items.len())]
    }

    /// v1.3's `InfoScene.OnMouseWheel`: over the storage (`StorageBounds`,
    /// 105, 103, 106 × 72) the list moves one row against the wheel,
    /// within `0..=items - 2` (`MoveItemOffset`). A v1.2-preset game only
    /// (the legacy game has no such scene). True when the list moved.
    pub fn wheel(&mut self, p: (i32, i32), delta: i32) -> bool {
        if self.automatic.is_none() || delta == 0 || !inside(p, (105, 103), (106, 72)) {
            return false;
        }
        let most = self.items.len().saturating_sub(2) as i32;
        let to = (self.offset as i32 - delta.signum()).clamp(0, most) as usize;
        let moved = to != self.offset;
        self.offset = to;
        moved
    }

    pub fn click(&mut self, art: &Art, p: (i32, i32), right: bool) -> InfoClick {
        if !right && inside(p, INFO_BACK, size_of(&art.map_button)) {
            return InfoClick::Back;
        }
        if !right && inside(p, INFO_UP, size_of(&art.up)) {
            // `OnButtonListUp`.
            if self.items.len() > 1 {
                self.offset = self.offset.saturating_sub(1);
            }
            return InfoClick::None;
        }
        if !right && inside(p, INFO_DOWN, size_of(&art.down)) {
            if self.items.len() > 1 {
                self.offset = (self.offset + 1).min(self.items.len() - 2);
            }
            return InfoClick::None;
        }
        // An empty `ItemWindow` takes no click.
        if self.produce.is_some() && inside(p, INFO_PRODUCTION, (32, 32)) {
            // v1.2's `NextProduction`/`PreviousProduction`, where an
            // automatic camp is slot -1: a left click picks the first, a
            // right click on the first gives the choice back.
            if self.automatic == Some(true) {
                if !right && self.products[0].is_some() {
                    return InfoClick::Production(0);
                }
                return InfoClick::None;
            }
            if let Some(at) = self.slot {
                if !right && at < 3 && self.products[at + 1].is_some() {
                    return InfoClick::Production(at + 1);
                }
                if right && at == 0 && self.automatic.is_some() {
                    return InfoClick::Automatic;
                }
                if right && at > 0 {
                    return InfoClick::Production(at - 1);
                }
            }
        }
        InfoClick::None
    }

    pub fn draw<'a>(&self, t: &mut Target, art: &Art, hud: &Hud, item: &dyn Fn(u8) -> Option<PackedImage<'a>>,
                    mouse: (i32, i32), ms: u32) {
        if let Some(bg) = &art.info {
            // `Size = 320 × 200`: the background centred on it
            // (`Container.GetBackgroundPosition`) — NewGfx's 540 × 300.
            t.blit(bg, (320 - bg.game_w()) / 2, (200 - bg.game_h()) / 2);
        }
        let back = if inside(mouse, INFO_BACK, size_of(&art.map_button)) { &art.map_button_hover } else { &art.map_button };
        if let Some(b) = back.as_ref().or(art.map_button.as_ref()) {
            t.blit(b, INFO_BACK.0, INFO_BACK.1);
        }
        // Up and down have only a hover image.
        for (at, img) in [(INFO_UP, &art.up), (INFO_DOWN, &art.down)] {
            if inside(mouse, at, size_of(img)) {
                if let Some(i) = img {
                    t.blit(i, at.0, at.1);
                }
            }
        }
        draw_anim(t, &art.inf_a, ms);
        draw_anim(t, &art.inf_b, ms);
        if let Some((rad, _)) = &self.danger {
            if let Some(i) = if *rad { &art.rad } else { &art.gas } {
                // v1.3 moved the hazard's picture 20 right (`danger.Position`
                // 224 → 244, `InfoScene.cs:96`).
                t.blit(i, if self.v13_lines.is_some() { 249 } else { 229 }, 27);
            }
        }
        let mut hovered: Option<((i32, i32), &str)> = None;
        if let Some((sprite, title)) = &self.produce {
            if let Some(img) = sprite.and_then(item) {
                t.blit(&img, INFO_PRODUCTION.0, INFO_PRODUCTION.1);
            }
            if inside(mouse, INFO_PRODUCTION, (32, 32)) {
                hovered = Some((INFO_PRODUCTION, title));
            }
        }
        for (s, at) in self.shown().iter().zip(INFO_CELLS) {
            if let Some(img) = s.sprite.and_then(item) {
                t.blit(&img, at.0, at.1);
            }
            if inside(mouse, at, (32, 32)) {
                hovered = Some((at, &s.title));
            }
        }
        let Some(f) = &hud.font else { return };
        f.draw(t, &self.title, 193, 7, Align::Centre, Align::Start, Some(INFO_TITLE));
        for (i, s) in self.shown().iter().enumerate() {
            f.draw_at(t, &crate::panels::num(s.count), 188, if i == 0 { 120 } else { 160 }, Some(INFO_TEXT));
        }
        let mut widest = 0;
        for (i, l) in self.labels.iter().enumerate() {
            f.draw_at(t, l, 100, 33 + 18 * i as i32, Some(INFO_TEXT));
            widest = widest.max(f.width(l));
        }
        // `RenderNPCLine`: a picture each, 18 apart; with the details (v1.3)
        // `~d{tier}~r{health}` after each picture, centred on its middle,
        // and 24 apart.
        let step = if self.details.is_some() { 24 } else { 18 };
        for (i, n) in self.residents.iter().enumerate() {
            if let Some(img) = &art.people[i] {
                for k in 0..*n as i32 {
                    let (x, y) = (110 + widest + step * k, 29 + 18 * i as i32);
                    t.blit(img, x, y);
                    if let Some(&(tier, health)) = self.details.as_ref().and_then(|d| d[i].get(k as usize)) {
                        use crate::hud::{Bar, Mark};
                        hud.draw_marks(t, &[Mark { bar: Bar::Dots, value: tier, gap_after: false },
                                            Mark { bar: Bar::Red, value: health, gap_after: false }],
                                       x + img.width as i32, y + 8);
                    }
                }
            }
        }
        f.draw_at(t, &self.storage, 137, 86, Some(INFO_TEXT));
        f.draw_at(t, &self.available, 229, 86, Some(INFO_TEXT));
        match &self.v13_lines {
            // `DrawProductionText`: the reserve half a line above the old
            // line's top, the day's line under it, at x 263 (v1.3).
            Some(lines) => {
                let lh = f.line_height();
                for (pair, y) in lines.iter().zip([117, 152]) {
                    f.draw_at(t, &pair[0], 263, y - lh / 2, Some(INFO_TEXT));
                    f.draw_at(t, &pair[1], 263, y - lh / 2 + lh, Some(INFO_TEXT));
                }
            }
            None => {
                f.draw_at(t, &self.food, 270, 117, Some(INFO_TEXT));
                f.draw_at(t, &self.water, 270, 152, Some(INFO_TEXT));
            }
        }
        if let Some((_, text)) = &self.danger {
            // v1.3 moved the hazard's text 5 left and aligns it left there.
            if self.v13_lines.is_some() {
                f.draw_at(t, text, 246, 68, Some(INFO_TEXT));
            } else {
                f.draw(t, text, 251, 68, Align::Centre, Align::Start, Some(INFO_TEXT));
            }
        }
        // v1.2: the production's tooltip, right edge at 225 and bottom at
        // 137, while the pointer is on it or at the keys; the grid's
        // tooltip on the item under the pointer. Neither window shows the
        // hovered title (`ShowHoverText = false`).
        if let Some(tip) = &self.production_tip {
            if self.produce.is_some() && !self.tip_dismissed
                && (self.keyboard || inside(mouse, INFO_PRODUCTION, (32, 32))) {
                let prompts = if self.keyboard { &tip.keys } else { &tip.mouse };
                let w = crate::hud::TipWindow {
                    header: &tip.header,
                    header_ink: crate::hud::HUD_TEXT_HOVER,
                    lines: &tip.lines,
                    status: tip.automatic.as_deref(),
                    highlight: tip.highlight,
                    prompts,
                    min_width: 100,
                };
                hud.draw_tip_window(t, &w, (225, 137), Align::End, Align::End, (-t.origin.0, -t.origin.1, t.game_size().0, t.game_size().1));
            }
            for (i, at) in INFO_CELLS.iter().enumerate().take(self.shown().len()) {
                if !inside(mouse, *at, (32, 32)) {
                    continue;
                }
                let index = self.offset.min(self.items.len()) + i;
                let (Some(s), Some(facts)) = (self.items.get(index), self.facts.get(index)) else { continue };
                // `ItemTooltipSide.Auto`: the grid (137, one cell wide) is
                // left of the middle, so the window opens to its right.
                let right = t.origin.0 + at.0 + 16 < t.game_size().0 / 2;
                let edge = if right { at.0 + 32 } else { at.0 };
                hud.draw_tooltip(t, &s.title, facts, edge, at.1 - 16, right, (320, 200), None);
            }
            return;
        }
        // `ItemWindow.OnRender`: the hovered item's title at (16, -9).
        if let Some((at, title)) = hovered {
            f.draw(t, title, at.0 + 16, at.1 - 9, Align::Centre, Align::Start, Some(ITEM_TITLE));
        }
    }
}

// ------------------------------------------------------------- statistics

/// `StatisticsScene`: sixteen lines, `burn?569`–`584`, arguments put in.
pub struct Stats {
    pub lines: Vec<String>,
}

impl Stats {
    pub fn draw(&self, t: &mut Target, art: &Art, hud: &Hud) {
        if let (Some(bg), true) = (&art.stats_ng, crate::gfx::newgfx()) {
            // `StatisticsScene.OnRender` in NewGfx: the picture at the
            // game's top left (or centred where the game is wider), the
            // corners on the part of it the game shows. `Max` keeps the
            // smaller axis and `Min(0)` the larger, as upstream names them.
            let (gw, gh) = t.game_size();
            let size = (gw.min(bg.game_w()), gh.min(bg.game_h()));
            let off = (((gw - bg.game_w()) / 2).max(0) - t.origin.0, ((gh - bg.game_h()) / 2).max(0) - t.origin.1);
            t.blit(bg, off.0, off.1);
            let [lt, rt, lb, rb] = &art.stats_corners;
            if let Some(i) = rb {
                t.blit(i, size.0 - i.game_w() + off.0 + 1, size.1 - i.game_h() + off.1 + 1);
            }
            if let Some(i) = rt {
                t.blit(i, size.0 - i.game_w() + 1 + off.0, off.1);
            }
            if let Some(i) = lt {
                t.blit(i, off.0, off.1);
            }
            if let Some(i) = lb {
                t.blit(i, off.0, size.1 - i.game_h() + 1 + off.1);
            }
        } else if let Some(bg) = &art.blz {
            t.blit(bg, 0, 0);
        }
        let Some(f) = &hud.font else { return };
        let mut y = -1;
        for l in &self.lines {
            // `Target.Width / 2 - GetWidth(str) / 2 - 10`.
            f.draw_at(t, l, 320 / 2 - f.width(l) / 2 - 10, y, Some(STATS_TEXT));
            y += 11;
        }
    }
}

// ---------------------------------------------------------------- options

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// v1.2's empty page behind the Back tab (`_emptyPage`).
    Back,
    Saves,
    Jukebox,
    Settings,
    GiveUp,
}

/// A slot on the Saves page.
#[derive(Clone, Debug)]
pub struct Slot {
    /// Lower case, without `.sav`.
    pub name: String,
    /// `SaveInfo.IsValid`: a save of this version for this pack.
    pub valid: bool,
    pub days: u16,
    pub camps: u16,
    /// v1.2's hints: whose game, which difficulty, when.
    pub player: Option<String>,
    pub difficulty: u16,
    pub saved: Option<burngfx::save::Stamp>,
    /// `AutosaveManager.IsAutosave`: shown in brackets, never deleted, and a
    /// save over it makes a new file.
    pub autosave: bool,
}

/// Every line the scene shows, resolved once.
#[derive(Default)]
pub struct OptionTexts {
    /// Back, Saves, Jukebox, Settings, Give up.
    pub menu: [String; 5],
    /// LOAD, SAVE, DELETE.
    pub buttons: [String; 3],
    /// `burn?385`–`387`: what LOAD, SAVE and DELETE will do to `|A`.
    pub modes: [String; 3],
    /// `newburn?37`, `?35`, `?36`.
    pub incompatible: String,
    pub camps: String,
    pub days: String,
    /// `newburn?17`/`18` (new/classic graphics), the music line for the
    /// set playing, `?19`/`?20` (fullscreen), `?26` (language).
    pub graphics: String,
    pub music: String,
    pub window: String,
    pub language: String,
    /// `newburn?28`, `burn?391`.
    pub new_game: String,
    pub exit: String,
    /// v1.2's `newburn?53` ("[NEW SAVE]"), `?54` (the row's details) and
    /// `?35` (its camps, with plurals).
    pub new_save: String,
    pub details: String,
    pub camps12: String,
    /// v1.2's Settings: the map music line for the mode in force
    /// (`newburn?58`–`60`) and its hint (`?61`–`63`).
    pub map_music: String,
    pub map_hint: String,
    /// The music line's hint, `newburn?25` ("toggle in-game with F9").
    pub music_hint: String,
    /// v1.2's Settings beyond the music: the fullscreen line on and off
    /// (`newburn?19`/`20`) and its hint (`?23`, `?57` on macOS); the
    /// graphics' hint (`?24`); `controller?0–4`, `prompt_mode?0/1`,
    /// `language_mode?0–2`.
    pub fullscreen: [String; 2],
    pub fullscreen_hint: String,
    pub graphics_hint: String,
    pub controller: [String; 5],
    pub prompt_mode: [String; 2],
    pub language_mode: [String; 3],
}

/// v1.2's Settings lines (`OptionsSettingsPage`), top to bottom, which is
/// also the keys' order (`_buttons`): (38, 58) and ten down each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Graphics,
    Fullscreen,
    Music,
    MapMusic,
    Controller,
    Prompts,
    Language,
}

pub const SETTINGS12: [Setting; 7] = [
    Setting::Graphics,
    Setting::Fullscreen,
    Setting::Music,
    Setting::MapMusic,
    Setting::Controller,
    Setting::Prompts,
    Setting::Language,
];

/// Where a v1.2 Settings line is: `Position = (38, 58 + 10 i)`.
pub fn setting_at(i: usize) -> (i32, i32) {
    (38, 58 + 10 * i as i32)
}

/// The player's own v1.2 choices that the Settings page shows and the
/// game keeps in `user.txt`: `fullscreen` (`None` where the window cannot
/// be switched, v1.2's `!SupportsFullscreenToggle`, and the line is
/// hidden), `controller_glyphs` (0 Auto, 1 Xbox, 2 PlayStation, 3 Steam,
/// 4 Switch), `prompts` (0 Full, 1 Less, 2 Hide) and `language` (0 Auto,
/// 1 English, 2 German).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Choices {
    pub fullscreen: Option<bool>,
    pub controller: u8,
    pub prompts: u8,
    pub language: u8,
}

impl Choices {
    /// `CycleControllerGlyphMode`: Auto, Xbox, PlayStation, Steam, Switch,
    /// round.
    pub fn next_controller(c: u8) -> u8 {
        (c + 1) % 5
    }

    /// `CycleUIHintVisibilityMode`: Hide and Full, each to the other; Less
    /// counts as shown and goes to Hide.
    pub fn next_prompts(p: u8) -> u8 {
        if p == 2 { 0 } else { 2 }
    }

    /// `CycleLanguageMode`: Auto, English, German, round.
    pub fn next_language(l: u8) -> u8 {
        (l + 1) % 3
    }

    /// `ParseControllerGlyphMode` and its writer: the `controller_glyphs`
    /// value in `user.txt`.
    pub fn controller_name(c: u8) -> &'static str {
        ["auto", "xbox", "playstation", "steam", "switch"][c.min(4) as usize]
    }

    pub fn controller_of(s: &str) -> u8 {
        match s.trim().to_ascii_lowercase().as_str() {
            "xbox" => 1,
            "playstation" => 2,
            "steam" => 3,
            "switch" => 4,
            _ => 0,
        }
    }

    /// `ParseLanguageMode` and `FormatLanguageMode`: `language` in
    /// `user.txt`.
    pub fn language_name(l: u8) -> &'static str {
        ["auto", "en", "de"][l.min(2) as usize]
    }

    pub fn language_of(s: &str) -> u8 {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => 1,
            "de" | "german" => 2,
            _ => 0,
        }
    }

    /// `GetInt("prompts")` clamped to Full..Hide; anything unreadable is 0.
    pub fn prompts_of(s: &str) -> u8 {
        s.trim().parse::<i32>().map_or(0, |n| n.clamp(0, 2) as u8)
    }

    /// `ConfigSection.GetBool`: 1, yes, true, on.
    pub fn bool_of(s: &str) -> bool {
        matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "yes" | "true" | "on")
    }
}

/// v1.2's keys on the scene (`OptionsScene.OnInputAction`, `OnVKeyPress`):
/// the actions, Tab, and Shift+Up/Down for the next or previous page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsKey {
    Up,
    Down,
    Left,
    Right,
    Primary,
    Back,
    Tab,
    PageUp,
    PageDown,
}

/// v1.2's scene state beyond v0.9.1's: which tab has the keys, and the
/// Saves page's marked row, focused row, scroll and action strip.
#[derive(Clone, Debug, Default)]
pub struct V12Options {
    pub tabs_focused: bool,
    pub tab_focus: usize,
    /// `_markedIndex`: the entry LOAD, SAVE and DELETE act on.
    pub marked: usize,
    /// `_saveFocusIndex`, when the keys hold one.
    pub focus: Option<usize>,
    pub scroll: usize,
    /// `KeyboardArea.Actions`, and which of LOAD, SAVE, DELETE.
    pub actions: bool,
    pub action_focus: usize,
    pub give_up_focus: usize,
    pub song_focus: usize,
    /// The Settings page's focus, an index into [`SETTINGS12`].
    pub settings_focus: usize,
    /// The file to mark when the list is next read (`RefreshSaveGames(changed)`).
    pub changed: Option<String>,
}

pub struct Options {
    pub page: Page,
    /// `SavegameInputWindow.Name`.
    pub name: String,
    pub slots: Vec<Slot>,
    /// Why `saves/` could not be read, when it could not: this port's own
    /// line on the page, so that a failure does not pass for no saves.
    pub list_error: Option<String>,
    /// Opened from `MenuScene`: nothing to save, and no game to give up.
    pub from_menu: bool,
    /// The Jukebox's entries, `(song, title)`: v1.2's `jukebox_dos.txt` or
    /// `jukebox_amiga.txt` by the music set (`CreateSongButtons`), the
    /// legacy game's every song of the set, capitalised.
    pub songs: Vec<(String, String)>,
    /// The Jukebox tab can be chosen: v1.2's `!DisableMusic` (a music set
    /// on disk, playing or not); the legacy game's songs to play.
    pub jukebox: bool,
    pub playing: Option<String>,
    pub t: OptionTexts,
    /// v1.2's scene and Saves page; v0.9.1's when `None` (the legacy game).
    pub v12: Option<V12Options>,
    /// The saving player's name, for a new save's file name.
    pub player: String,
    /// `LastInputMode` is the keyboard (or a pad): the keys' focus shows.
    pub keyboard: bool,
    /// v1.2's Settings choices beyond the music.
    pub choices: Choices,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionsClick {
    Back,
    Load(String),
    Save(String),
    Delete(String),
    Play(String),
    /// v1.2's Settings: the music set, the map music mode, the window, the
    /// controller's glyphs, the prompts, the language.
    CycleMusic,
    CycleMapMusic,
    ToggleFullscreen,
    CycleController,
    TogglePrompts,
    CycleLanguage,
    /// The graphics line: the other look (`IsNewGfx = !IsNewGfx`).
    ToggleGraphics,
    NewGame,
    Exit,
    None,
}

const MENU_X: i32 = 214;
const MENU_Y: [i32; 5] = [64, 84, 105, 127, 148];
const COVER: (i32, i32) = (186, 51);
const INPUT: (i32, i32) = (40, 112);
const BOX: (i32, i32) = (120, 10);
const BUTTONS: (i32, i32) = (40, 124);
const HINT: (i32, i32) = (40, 123);
const SETTINGS_Y: [i32; 4] = [58, 68, 78, 98];
const GIVE_UP: [(i32, i32); 2] = [(40, 82), (40, 102)];
/// v1.2's Saves list (`OptionsSavesPage`, classic graphics): six rows of
/// ten from (38, 58), 122 wide, and a scroll bar two right of them.
/// `VisibleSaveCount`: seven rows in NewGfx, six in the classic look
/// (`OptionsSavesPage.cs:89`).
fn visible_saves() -> usize {
    if crate::gfx::newgfx() { 7 } else { 6 }
}
const LIST: (i32, i32) = (38, 58);
const LIST_W: i32 = 122;
/// `RowHeight`: the row font's `LineHeight` in NewGfx — `font-highres.txt`'s
/// `line_height=11` — and 10 in the classic look (`OptionsSavesPage.cs:95`).
fn row_h() -> i32 {
    if crate::gfx::newgfx() { 11 } else { 10 }
}
const SCROLL_X: i32 = LIST.0 + LIST_W + 2;
fn scroll_h() -> i32 {
    visible_saves() as i32 * row_h() - 3
}
/// `new PixelColor(80, 108, 116, 168)`: the bar's track.
const TRACK: u32 = 0x0050_6C74;

/// `OptionsSavesPage.OnMouseWheel`: over the list (six rows from (38, 58),
/// 122 wide) and with more entries than it shows, a turn moves it a row
/// against the turn's sign (`ScrollList(-Sign(delta))`, clamped). The new
/// first row, or `None` when the wheel is not the list's.
pub fn saves_wheel(scroll: usize, count: usize, p: (i32, i32), delta: i32) -> Option<usize> {
    let over = inside(p, LIST, (LIST_W, visible_saves() as i32 * row_h()));
    if !over || count <= visible_saves() || delta == 0 {
        return None;
    }
    let most = (count - visible_saves()) as i32;
    Some((scroll as i32 - delta.signum()).clamp(0, most) as usize)
}

fn slot_at(i: usize) -> (i32, i32) {
    (38 + (i / 5) as i32 * 67, 58 + (i % 5) as i32 * 10)
}

/// `CreateSongButtons`: eight to a column from (38, 58), 44 across, 10 down.
pub fn song_at(i: usize) -> (i32, i32) {
    (38 + (i / 8) as i32 * 44, 58 + (i % 8) as i32 * 10)
}

/// The legacy Jukebox's titles: the song's name, capitalised.
pub fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

impl Options {
    /// `RefreshSaveGames` with what the directory gave.
    pub fn listed(&mut self, slots: Result<Vec<Slot>, String>) {
        // v1.2 keeps its mark on the same file, or on the one just written.
        let keep = self.v12.as_ref().and_then(|v| v.changed.clone().or_else(|| self.save_at(v.marked).map(|s| s.name.clone())));
        match slots {
            Ok(s) => {
                self.slots = s;
                self.list_error = None;
            }
            Err(why) => {
                self.slots.clear();
                self.list_error = Some(why);
            }
        }
        if self.v12.is_some() {
            // Newest first (`OrderByDescending(timestamp).ThenBy(name)`);
            // a save that does not say when goes last.
            self.slots.sort_by(|a, b| b.saved.cmp(&a.saved).then_with(|| a.name.cmp(&b.name)));
            let offset = self.entry_offset();
            let found = keep.and_then(|n| self.slots.iter().position(|s| s.name == n)).map(|i| i + offset);
            let count = self.entry_count();
            if let Some(v) = self.v12.as_mut() {
                v.marked = found.unwrap_or(0);
                if let Some(f) = v.focus {
                    v.focus = Some(found.unwrap_or(f.min(count.saturating_sub(1))));
                }
                v.changed = None;
                v.actions = false;
            }
            self.ensure_visible();
        }
    }

    // -- v1.2 (`OptionsScene`, `OptionsSavesPage`, `OptionsGiveUpPage`,
    //    `OptionsJukeboxPage` at v1.2) ----------------------------------------

    /// `CanCreateSave`: a game is running.
    fn can_create(&self) -> bool {
        !self.from_menu
    }

    fn entry_offset(&self) -> usize {
        self.can_create() as usize
    }

    fn entry_count(&self) -> usize {
        self.slots.len() + self.entry_offset()
    }

    fn save_at(&self, entry: usize) -> Option<&Slot> {
        entry.checked_sub(self.entry_offset()).and_then(|i| self.slots.get(i))
    }

    fn is_create(&self, entry: usize) -> bool {
        self.can_create() && entry == 0
    }

    fn marked_save(&self) -> Option<&Slot> {
        self.v12.as_ref().and_then(|v| self.save_at(v.marked))
    }

    /// LOAD, SAVE, DELETE: shown, and usable (`SetActionVisibility`,
    /// `OnUpdate`).
    fn actions_v12(&self) -> [(bool, bool); 3] {
        let marked = self.marked_save();
        [
            (marked.is_some(), marked.is_some_and(|s| s.valid)),
            (true, self.can_create()),
            (marked.is_some(), marked.is_some_and(|s| !s.autosave)),
        ]
    }

    fn ensure_visible(&mut self) {
        let count = self.entry_count();
        let Some(v) = self.v12.as_mut() else { return };
        if let Some(f) = v.focus {
            if f < v.scroll {
                v.scroll = f;
            } else if f >= v.scroll + visible_saves() {
                v.scroll = f + 1 - visible_saves();
            }
        }
        v.scroll = v.scroll.min(count.saturating_sub(visible_saves()));
    }

    /// What SAVE writes: over the marked save, or — on `[NEW SAVE]` or an
    /// autosave — the next `<player>-saveNNN` (`OnSave`).
    fn save_target(&self) -> String {
        match self.marked_save() {
            Some(s) if !s.autosave && !self.v12.as_ref().is_some_and(|v| self.is_create(v.marked)) => s.name.clone(),
            _ => {
                let names: Vec<String> = self.slots.iter().map(|s| s.name.clone()).collect();
                burngfx::slots::next_save_name(&self.player, &names)
            }
        }
    }

    fn run_action(&mut self, i: usize) -> OptionsClick {
        let (shown, usable) = self.actions_v12()[i];
        if !(shown && usable) {
            return OptionsClick::None;
        }
        match i {
            0 => self.marked_save().map_or(OptionsClick::None, |s| OptionsClick::Load(s.name.clone())),
            1 => {
                let name = self.save_target();
                if let Some(v) = self.v12.as_mut() {
                    v.changed = Some(name.clone());
                }
                OptionsClick::Save(name)
            }
            _ => self.marked_save().map_or(OptionsClick::None, |s| OptionsClick::Delete(s.name.clone())),
        }
    }

    /// `OnSelectRow`/`MarkEntry`: the entry marked; `[NEW SAVE]` saves.
    fn mark(&mut self, entry: usize) -> OptionsClick {
        if let Some(v) = self.v12.as_mut() {
            v.marked = entry;
            v.focus = Some(entry);
        }
        if self.is_create(entry) {
            self.run_action(1)
        } else {
            OptionsClick::None
        }
    }

    /// The tabs: Back, Saves, Jukebox, Settings, Give up; Jukebox only with
    /// songs to play.
    fn tab_enabled(&self, i: usize) -> bool {
        i != 2 || self.jukebox
    }

    fn page_of(i: usize) -> Page {
        match i {
            0 => Page::Back,
            2 => Page::Jukebox,
            3 => Page::Settings,
            4 => Page::GiveUp,
            _ => Page::Saves,
        }
    }

    fn tab_of(&self) -> usize {
        match self.page {
            Page::Back => 0,
            Page::Saves => 1,
            Page::Jukebox => 2,
            Page::Settings => 3,
            Page::GiveUp => 4,
        }
    }

    /// `SelectPage`: a new page starts with its first control focused.
    fn select_page(&mut self, i: usize) {
        let page = Self::page_of(i);
        let changed = page != self.page;
        self.page = page;
        if let Some(v) = self.v12.as_mut() {
            if i == 0 {
                v.tabs_focused = true;
            }
            if changed {
                v.focus = Some(0);
                v.actions = false;
                v.give_up_focus = if self.from_menu { 1 } else { 0 };
                v.song_focus = 0;
                // `SetKeyboardActive(…, resetFocus)`: the first enabled line.
                v.settings_focus = usize::MAX;
            }
        }
        if changed && self.v12.is_some() {
            let first = self.settings_focused();
            self.v().settings_focus = first;
        }
        if changed {
            self.ensure_visible();
        }
    }

    /// `MoveIndex`: the next enabled tab that way, round or not.
    fn move_tab(&self, from: usize, dir: i32, wrap: bool) -> usize {
        let mut i = from as i32;
        loop {
            let next = i + dir;
            let next = if wrap {
                next.rem_euclid(5)
            } else if !(0..5).contains(&next) {
                return i as usize;
            } else {
                next
            };
            i = next;
            if self.tab_enabled(i as usize) {
                return i as usize;
            }
        }
    }

    fn v(&mut self) -> &mut V12Options {
        self.v12.get_or_insert_with(V12Options::default)
    }

    /// v1.2's keys: the scene's, then the page's (`OnInputAction` of the
    /// focused page first, the scene's when it declines).
    pub fn key(&mut self, k: OptionsKey) -> OptionsClick {
        use OptionsKey as K;
        if self.v12.is_none() {
            return OptionsClick::None;
        }
        let click = match k {
            K::PageUp | K::PageDown => {
                let i = self.move_tab(self.tab_of(), if k == K::PageUp { -1 } else { 1 }, true);
                self.v().tab_focus = i;
                self.select_page(i);
                OptionsClick::None
            }
            K::Tab => {
                let i = self.tab_of();
                let v = self.v();
                v.tab_focus = i;
                v.tabs_focused = true;
                OptionsClick::None
            }
            K::Back => OptionsClick::Back,
            _ if self.v().tabs_focused => self.tabs_key(k),
            _ => {
                // The page first; a Right it does not take goes to the tabs.
                let (taken, click) = match self.page {
                    Page::Saves => self.saves_key(k),
                    Page::GiveUp => self.give_up_key(k),
                    Page::Jukebox => self.jukebox_key(k),
                    Page::Settings => self.settings_key(k),
                    Page::Back => (false, OptionsClick::None),
                };
                if !taken && k == K::Right {
                    let i = self.tab_of();
                    let v = self.v();
                    v.tab_focus = i;
                    v.tabs_focused = true;
                }
                click
            }
        };
        self.ensure_visible();
        click
    }

    fn tabs_key(&mut self, k: OptionsKey) -> OptionsClick {
        use OptionsKey as K;
        let focus = self.v().tab_focus;
        match k {
            K::Up | K::Down => {
                let i = self.move_tab(focus, if k == K::Up { -1 } else { 1 }, false);
                self.v().tab_focus = i;
                self.select_page(i);
            }
            K::Primary if focus == 0 => return OptionsClick::Back,
            K::Left | K::Primary => {
                self.select_page(focus);
                if focus != 0 {
                    self.v().tabs_focused = false;
                }
            }
            _ => {}
        }
        OptionsClick::None
    }

    fn saves_key(&mut self, k: OptionsKey) -> (bool, OptionsClick) {
        use OptionsKey as K;
        let count = self.entry_count();
        if !self.v().actions {
            // `EnsureFocus`: a key with nothing focused focuses the first.
            let focus = self.v().focus.filter(|&f| f < count);
            match k {
                K::Up | K::Down => {
                    let Some(f) = focus.or((count > 0).then_some(0)) else { return (true, OptionsClick::None) };
                    let next = f as i32 + if k == K::Up { -1 } else { 1 };
                    if next >= 0 && (next as usize) < count {
                        self.v().focus = Some(next as usize);
                    } else {
                        self.v().focus = Some(f);
                        if k == K::Down {
                            self.first_action();
                        }
                    }
                    (true, OptionsClick::None)
                }
                K::Primary => match focus {
                    None => {
                        if count > 0 {
                            self.v().focus = Some(0);
                        }
                        (true, OptionsClick::None)
                    }
                    Some(f) => {
                        self.v().marked = f;
                        if self.is_create(f) {
                            (true, self.run_action(1))
                        } else {
                            self.first_action();
                            (true, OptionsClick::None)
                        }
                    }
                },
                _ => (false, OptionsClick::None),
            }
        } else {
            match k {
                K::Left | K::Right => {
                    let dir: i32 = if k == K::Left { -1 } else { 1 };
                    let actions = self.actions_v12();
                    let mut i = self.v().action_focus as i32 + dir;
                    while (0..3).contains(&i) {
                        let (shown, usable) = actions[i as usize];
                        if shown && usable {
                            self.v().action_focus = i as usize;
                            return (true, OptionsClick::None);
                        }
                        i += dir;
                    }
                    (dir < 0, OptionsClick::None)
                }
                K::Up => {
                    self.v().actions = false;
                    (true, OptionsClick::None)
                }
                K::Primary => {
                    let i = self.v().action_focus;
                    (true, self.run_action(i))
                }
                _ => (false, OptionsClick::None),
            }
        }
    }

    /// `SelectFirstEnabledAction`: the strip takes the keys, on the action
    /// that had them if it still can, else the first that can.
    fn first_action(&mut self) {
        let actions = self.actions_v12();
        let usable = |i: usize| actions[i].0 && actions[i].1;
        let had = self.v().action_focus;
        if let Some(i) = if usable(had) { Some(had) } else { (0..3).find(|&i| usable(i)) } {
            let v = self.v();
            v.action_focus = i;
            v.actions = true;
        }
    }

    /// `OptionsSettingsPage`: Up and Down between the two that work here,
    /// Space cycles the one focused.
    /// A v1.2 Settings line can be chosen (`IsEnabled`): the graphics are
    /// the port's one look (UI-19), the fullscreen line is hidden where the
    /// window cannot be switched.
    pub fn setting_enabled(&self, s: Setting) -> bool {
        match s {
            Setting::Graphics => true,
            Setting::Fullscreen => self.choices.fullscreen.is_some(),
            _ => true,
        }
    }

    /// The focused line, on an enabled one (`PrepareFocusForInput`: the
    /// first enabled when the focus is on none).
    fn settings_focused(&self) -> usize {
        let f = self.v12.as_ref().map_or(0, |v| v.settings_focus);
        if SETTINGS12.get(f).is_some_and(|&s| self.setting_enabled(s)) {
            f
        } else {
            SETTINGS12.iter().position(|&s| self.setting_enabled(s)).unwrap_or(0)
        }
    }

    /// What choosing a line does.
    fn setting_click(s: Setting) -> OptionsClick {
        match s {
            Setting::Graphics => OptionsClick::ToggleGraphics,
            Setting::Fullscreen => OptionsClick::ToggleFullscreen,
            Setting::Music => OptionsClick::CycleMusic,
            Setting::MapMusic => OptionsClick::CycleMapMusic,
            Setting::Controller => OptionsClick::CycleController,
            Setting::Prompts => OptionsClick::TogglePrompts,
            Setting::Language => OptionsClick::CycleLanguage,
        }
    }

    /// `OptionsSettingsPage.OnInputAction`: Up and Down move to the
    /// enabled line before or after, round (`MoveFocus`); Primary chooses
    /// the focused one.
    fn settings_key(&mut self, k: OptionsKey) -> (bool, OptionsClick) {
        use OptionsKey as K;
        let mut f = self.settings_focused();
        match k {
            K::Up | K::Down => {
                let n = SETTINGS12.len();
                loop {
                    f = if k == K::Up { (f + n - 1) % n } else { (f + 1) % n };
                    if self.setting_enabled(SETTINGS12[f]) {
                        break;
                    }
                }
                self.v().settings_focus = f;
                (true, OptionsClick::None)
            }
            K::Primary => {
                self.v().settings_focus = f;
                (true, Self::setting_click(SETTINGS12[f]))
            }
            _ => (false, OptionsClick::None),
        }
    }

    /// The text a v1.2 Settings line shows now (`OnUpdate`).
    pub fn setting_text(&self, s: Setting) -> &str {
        let c = &self.choices;
        match s {
            Setting::Graphics => &self.t.graphics,
            Setting::Fullscreen => &self.t.fullscreen[if c.fullscreen == Some(true) { 0 } else { 1 }],
            Setting::Music => &self.t.music,
            Setting::MapMusic => &self.t.map_music,
            Setting::Controller => &self.t.controller[c.controller.min(4) as usize],
            Setting::Prompts => &self.t.prompt_mode[if c.prompts == 2 { 1 } else { 0 }],
            Setting::Language => &self.t.language_mode[c.language.min(2) as usize],
        }
    }

    /// `_hintText` for the line lit: the fullscreen's, the graphics', the
    /// music's or the map music's; none for the others.
    pub fn setting_hint(&self, s: Setting) -> Option<&str> {
        match s {
            Setting::Fullscreen => Some(&self.t.fullscreen_hint),
            Setting::Graphics => Some(&self.t.graphics_hint),
            Setting::Music => Some(&self.t.music_hint),
            Setting::MapMusic => Some(&self.t.map_hint),
            _ => None,
        }
    }

    fn give_up_key(&mut self, k: OptionsKey) -> (bool, OptionsClick) {
        use OptionsKey as K;
        // New game is not offered from the start screen.
        let from_menu = self.from_menu;
        let enabled = |i: usize| i == 1 || !from_menu;
        let v = self.v();
        match k {
            K::Up | K::Down => {
                let mut i = v.give_up_focus;
                loop {
                    i = (i + 1) % 2;
                    if enabled(i) {
                        break;
                    }
                }
                v.give_up_focus = i;
                (true, OptionsClick::None)
            }
            K::Primary => {
                let i = if enabled(v.give_up_focus) { v.give_up_focus } else { 1 };
                (true, if i == 0 { OptionsClick::NewGame } else { OptionsClick::Exit })
            }
            _ => (false, OptionsClick::None),
        }
    }

    fn jukebox_key(&mut self, k: OptionsKey) -> (bool, OptionsClick) {
        use OptionsKey as K;
        let n = self.songs.len();
        if n == 0 {
            return (false, OptionsClick::None);
        }
        let f = self.v().song_focus.min(n - 1);
        let song = self.songs[f].0.clone();
        let v = self.v();
        match k {
            K::Up | K::Down => {
                // `MoveVertical`: round within the column of eight.
                let start = f / 8 * 8;
                let in_column = (n - start).min(8);
                let row = (f % 8 + if k == K::Up { in_column - 1 } else { 1 }) % in_column;
                v.song_focus = start + row;
                (true, OptionsClick::None)
            }
            K::Left | K::Right => {
                // `MoveHorizontal`: the same row of the next column, or its last.
                let column = f / 8;
                let last = (n - 1) / 8;
                let to = column as i32 + if k == K::Left { -1 } else { 1 };
                if to < 0 || to as usize > last {
                    return (k == K::Left, OptionsClick::None);
                }
                let start = to as usize * 8;
                v.song_focus = start + (f % 8).min((n - start).min(8) - 1);
                (true, OptionsClick::None)
            }
            K::Primary => (true, OptionsClick::Play(song)),
            _ => (false, OptionsClick::None),
        }
    }

    fn text_size(f: &Face, s: &str) -> (i32, i32) {
        (f.width(s), f.metric_height())
    }

    fn slot_text(s: &Slot) -> String {
        let mut t = s.name.to_uppercase();
        t.push_str(".SAV");
        t
    }

    /// `AutoAlignContainer`: LOAD at the left, DELETE at the right, SAVE
    /// between with the remainder on its right.
    fn button_places(&self, f: &Face) -> [(i32, i32); 3] {
        let w: Vec<i32> = self.t.buttons.iter().map(|b| f.width(b)).collect();
        let spaces = BOX.0 - w.iter().sum::<i32>();
        let margin = spaces / 2;
        let last_margin = spaces - margin;
        [
            (BUTTONS.0, BUTTONS.1),
            (BUTTONS.0 + w[0] + margin, BUTTONS.1),
            (BUTTONS.0 + w[0] + margin + w[1] + last_margin, BUTTONS.1),
        ]
    }

    fn highlighted(&self) -> Option<&Slot> {
        let n = self.name.to_lowercase();
        self.slots.iter().find(|s| s.name == n)
    }

    pub fn can_save(&self) -> bool {
        !self.name.is_empty() && !self.from_menu
    }

    pub fn can_load(&self) -> bool {
        self.highlighted().is_some_and(|s| s.valid)
    }

    pub fn can_delete(&self) -> bool {
        self.highlighted().is_some()
    }

    fn slot_hovered(&self, f: &Face, mouse: (i32, i32)) -> Option<usize> {
        (0..self.slots.len().min(10)).find(|&i| {
            inside(mouse, slot_at(i), Self::text_size(f, &Self::slot_text(&self.slots[i])))
        })
    }

    /// `SavegameInputWindow.OnKeyPress`: a character the font has, up to
    /// eight, or backspace.
    pub fn type_char(&mut self, c: char, f: &Face) {
        if self.page != Page::Saves {
            return;
        }
        if c == '\u{8}' {
            self.name.pop();
        } else if f.supports(c) && self.name.chars().count() < burngfx::slots::MAX_NAME {
            self.name.push(c);
        }
    }

    /// The scene's font. v1.2's `OptionsScene` and its pages draw in
    /// `font.txt` and `font-small.txt`, which are one file at v1.2:
    /// `font-small` with its umlauts and 11-pixel lines. The legacy game
    /// keeps its own.
    fn face<'h, 'a>(&self, hud: &'h Hud<'a>) -> Option<&'h Face<'a>> {
        if self.v12.is_some() { hud.small.as_ref().or(hud.font.as_ref()) } else { hud.font.as_ref() }
    }

    /// The wheel at `p`: the v1.2 Saves list scrolls under it. True when
    /// it did anything.
    pub fn wheel(&mut self, p: (i32, i32), delta: i32) -> bool {
        if self.page != Page::Saves || self.v12.is_none() {
            return false;
        }
        let count = self.entry_count();
        let v = self.v();
        match saves_wheel(v.scroll, count, p, delta) {
            Some(to) => {
                v.scroll = to;
                true
            }
            None => false,
        }
    }

    pub fn click(&mut self, hud: &Hud, p: (i32, i32)) -> OptionsClick {
        let Some(f) = self.face(hud) else { return OptionsClick::None };
        if self.v12.is_some() {
            return self.click_v12(f, p);
        }
        let menu_hit = |i: usize| {
            let enabled = i != 2 || self.jukebox;
            enabled && inside(p, (MENU_X, MENU_Y[i]), Self::text_size(f, &self.t.menu[i]))
        };
        if menu_hit(0) {
            return OptionsClick::Back;
        }
        for (i, page) in [(1, Page::Saves), (2, Page::Jukebox), (3, Page::Settings), (4, Page::GiveUp)] {
            if menu_hit(i) {
                self.page = page;
                return OptionsClick::None;
            }
        }
        match self.page {
            Page::Saves => {
                if let Some(i) = self.slot_hovered(f, p) {
                    // `OnSelect`: the slot's text less ".SAV".
                    self.name = self.slots[i].name.to_uppercase();
                    return OptionsClick::None;
                }
                let places = self.button_places(f);
                let enabled = [self.can_load(), self.can_save(), self.can_delete()];
                for i in 0..3 {
                    if enabled[i] && inside(p, places[i], Self::text_size(f, &self.t.buttons[i])) {
                        let n = self.name.to_lowercase();
                        return match i {
                            0 => OptionsClick::Load(n),
                            1 => OptionsClick::Save(n),
                            _ => OptionsClick::Delete(n),
                        };
                    }
                }
            }
            Page::Jukebox => {
                for (i, (song, title)) in self.songs.iter().enumerate() {
                    if inside(p, song_at(i), Self::text_size(f, title)) {
                        return OptionsClick::Play(song.clone());
                    }
                }
            }
            // Nothing on it is built; see `draw`.
            Page::Settings => {}
            Page::Back => {}
            Page::GiveUp => {
                if !self.from_menu && inside(p, GIVE_UP[0], BOX) {
                    return OptionsClick::NewGame;
                }
                if inside(p, GIVE_UP[1], BOX) {
                    return OptionsClick::Exit;
                }
            }
        }
        OptionsClick::None
    }

    /// The row under the pointer, as an entry.
    fn row_hovered(&self, mouse: (i32, i32)) -> Option<usize> {
        let scroll = self.v12.as_ref().map_or(0, |v| v.scroll);
        (0..visible_saves())
            .map(|r| (r, scroll + r))
            .find(|&(r, e)| e < self.entry_count() && inside(mouse, (LIST.0, LIST.1 + r as i32 * row_h()), (LIST_W, row_h())))
            .map(|(_, e)| e)
    }

    /// `GetHintText`: a valid save's difficulty, days and camps
    /// (`newburn?54`), "incompatible" for one this game cannot load.
    fn hint_of(&self, s: &Slot) -> String {
        if !s.valid {
            return self.t.incompatible.clone();
        }
        let difficulty = ["I", "II", "III"].get(s.difficulty as usize).copied().unwrap_or("?");
        let camps = text_helper(&self.t.camps12, &[("|A", s.camps as i32)]);
        self.t.details.replace("|D", difficulty).replace("|J", &crate::panels::num(s.days as i32)).replace("|C", &camps)
    }

    fn row_text(&self, f: &Face, s: &Slot, stamp: &str) -> String {
        let mut name = s.player.clone().unwrap_or_else(|| String::from(burngfx::slots::display_name(&s.name))).to_uppercase();
        let room = LIST_W - f.width(stamp) - 4;
        let shown = |n: &str| if s.autosave { let mut b = String::from("["); b.push_str(n); b.push(']'); b } else { String::from(n) };
        while name.chars().count() > 1 && f.width(&shown(&name)) > room {
            name.pop();
        }
        shown(&name)
    }

    fn draw_saves_v12(&self, t: &mut Target, f: &Face, mouse: (i32, i32), keyed: bool) {
        let Some(v) = &self.v12 else { return };
        if let Some(why) = &self.list_error {
            f.draw_at(t, "cannot read saves/", 38, 58, Some(ORANGE));
            f.draw_at(t, why, 38, 68, Some(ORANGE));
        }
        let hovered = if keyed { None } else { self.row_hovered(mouse) };
        let focused = (keyed && !v.actions).then_some(v.focus).flatten();
        for r in 0..visible_saves() {
            let e = v.scroll + r;
            if e >= self.entry_count() {
                break;
            }
            let y = LIST.1 + r as i32 * row_h();
            let lit = hovered == Some(e) || focused == Some(e);
            let save = self.save_at(e);
            let base = if e == v.marked { BLUE } else if save.is_some_and(|s| !s.valid) { DISABLED } else { GREEN };
            let ink = if lit { ORANGE } else { base };
            match save {
                None => f.draw_at(t, &self.t.new_save, LIST.0, y, Some(ink)),
                Some(s) => {
                    let stamp = s.saved.map(burngfx::slots::stamp_text).unwrap_or_default();
                    f.draw_at(t, &self.row_text(f, s, &stamp), LIST.0, y, Some(ink));
                    f.draw(t, &stamp, LIST.0 + LIST_W, y, Align::End, Align::Start, Some(ink));
                }
            }
        }
        // The scroll bar, when the list is longer than the page.
        let count = self.entry_count();
        if count > visible_saves() {
            let most = (count - visible_saves()) as i32;
            let thumb = (scroll_h() * visible_saves() as i32 / count as i32).max(10);
            let top = LIST.1 + (scroll_h() - thumb) * v.scroll as i32 / most;
            t.fill(SCROLL_X, LIST.1, 2, scroll_h(), TRACK, 168);
            t.fill(SCROLL_X, top, 2, thumb, GREEN, 255);
        }
        // Under the list: the row's details while one is pointed at or
        // focused, else LOAD, SAVE, DELETE.
        let preview = hovered.or(focused);
        if let Some(e) = preview {
            if let Some(s) = self.save_at(e) {
                draw_in_box(t, f, &self.hint_of(s), HINT, BOX, true, BLUE);
            }
            return;
        }
        let places = self.button_places(f);
        for (i, (shown, usable)) in self.actions_v12().into_iter().enumerate() {
            if !shown {
                continue;
            }
            let lit = usable
                && if keyed { v.actions && v.action_focus == i } else { inside(mouse, places[i], Self::text_size(f, &self.t.buttons[i])) };
            let ink = if !usable { DISABLED } else if lit { ORANGE } else { BLUE };
            f.draw_at(t, &self.t.buttons[i], places[i].0, places[i].1, Some(ink));
        }
    }

    fn click_v12(&mut self, f: &Face, p: (i32, i32)) -> OptionsClick {
        if let Some(v) = self.v12.as_mut() {
            v.tabs_focused = false;
        }
        for i in 0..5 {
            if self.tab_enabled(i) && inside(p, (MENU_X, MENU_Y[i]), Self::text_size(f, &self.t.menu[i])) {
                if i == 0 {
                    return OptionsClick::Back;
                }
                self.select_page(i);
                return OptionsClick::None;
            }
        }
        match self.page {
            Page::Saves => {
                if let Some(e) = self.row_hovered(p) {
                    return self.mark(e);
                }
                // The bar: a page of one row up or down.
                let count = self.entry_count();
                if count > visible_saves() && inside(p, (SCROLL_X - 2, LIST.1), (6, scroll_h())) {
                    let v = self.v();
                    let most = count - visible_saves();
                    let thumb = (scroll_h() * visible_saves() as i32 / count as i32).max(10);
                    let top = LIST.1 + (scroll_h() - thumb) * v.scroll as i32 / most as i32;
                    if p.1 < top {
                        v.scroll = v.scroll.saturating_sub(1);
                    } else if p.1 >= top + thumb {
                        v.scroll = (v.scroll + 1).min(most);
                    }
                    return OptionsClick::None;
                }
                let places = self.button_places(f);
                for i in 0..3 {
                    if inside(p, places[i], Self::text_size(f, &self.t.buttons[i])) {
                        return self.run_action(i);
                    }
                }
                OptionsClick::None
            }
            Page::Jukebox => {
                for (i, (song, title)) in self.songs.iter().enumerate() {
                    if inside(p, song_at(i), Self::text_size(f, title)) {
                        return OptionsClick::Play(song.clone());
                    }
                }
                OptionsClick::None
            }
            Page::GiveUp => {
                if !self.from_menu && inside(p, GIVE_UP[0], BOX) {
                    return OptionsClick::NewGame;
                }
                if inside(p, GIVE_UP[1], BOX) {
                    return OptionsClick::Exit;
                }
                OptionsClick::None
            }
            Page::Settings => {
                for (i, &s) in SETTINGS12.iter().enumerate() {
                    if self.setting_enabled(s) && inside(p, setting_at(i), Self::text_size(f, self.setting_text(s))) {
                        self.v().settings_focus = i;
                        return Self::setting_click(s);
                    }
                }
                OptionsClick::None
            }
            Page::Back => OptionsClick::None,
        }
    }

    pub fn draw(&self, t: &mut Target, art: &Art, hud: &Hud, mouse: (i32, i32), ms: u32) {
        if let Some(bg) = &art.opti {
            t.blit(bg, (320 - bg.game_w()) / 2, (200 - bg.game_h()) / 2);
        }
        // `_backgroundAni.IsVisible = !app.IsNewGfx` (`OptionsScene.cs:69`).
        if !crate::gfx::newgfx() {
            draw_anim(t, &art.opt, ms);
        }
        self.draw_controls(t, hud, mouse);
        // **Over everything else**: the bulb one layer up (`target.Layer++`
        // in `OnRender`), the radio's cover — a button with a hover picture
        // and no command — two (`Layer += 2`). The menu's labels run under
        // the cover's edge, and drawn after it they showed through.
        let bulb = match self.page {
            Page::Back => (192, 59),
            Page::Saves => (191, 79),
            Page::Jukebox => (192, 100),
            Page::Settings => (192, 122),
            Page::GiveUp => (192, 143),
        };
        if let Some(b) = &art.bulb {
            t.blit(b, bulb.0, bulb.1);
        }
        // v1.2's rail is `opta.raw?1` whatever the pointer does (both its
        // pictures are 1); v0.9.1 showed 0 until the pointer was over it.
        let cover = if self.v12.is_some() || inside(mouse, COVER, size_of(&art.cover)) { &art.cover_hover } else { &art.cover };
        if let Some(c) = cover.as_ref().or(art.cover.as_ref()) {
            t.blit(c, COVER.0, COVER.1);
        }
    }

    /// The menu's labels and the page: everything at the buttons' layer.
    fn draw_controls(&self, t: &mut Target, hud: &Hud, mouse: (i32, i32)) {
        let Some(f) = self.face(hud) else { return };
        let button = |t: &mut Target, text: &str, at: (i32, i32), ink: u32, hover_ink: u32, enabled: bool| {
            let ink = if !enabled {
                DISABLED
            } else if inside(mouse, at, Self::text_size(f, text)) {
                hover_ink
            } else {
                ink
            };
            f.draw_at(t, text, at.0, at.1, Some(ink));
        };
        let tab_keyed = |i: usize| self.keyboard && self.v12.as_ref().is_some_and(|v| v.tabs_focused && v.tab_focus == i);
        for i in 0..5 {
            let enabled = i != 2 || self.jukebox;
            if tab_keyed(i) && enabled {
                f.draw_at(t, &self.t.menu[i], MENU_X, MENU_Y[i], Some(BLUE));
            } else {
                button(t, &self.t.menu[i], (MENU_X, MENU_Y[i]), RED, BLUE, enabled);
            }
        }
        // The page's own focus at the keys (`IsKeyboardSelected`).
        let page_keyed = self.keyboard && self.v12.as_ref().is_some_and(|v| !v.tabs_focused);
        match self.page {
            Page::Back => {}
            Page::Saves if self.v12.is_some() => self.draw_saves_v12(t, f, mouse, page_keyed),
            Page::Saves => {
                if let Some(why) = &self.list_error {
                    f.draw_at(t, "cannot read saves/", 38, 58, Some(ORANGE));
                    f.draw_at(t, why, 38, 68, Some(ORANGE));
                }
                let hovered = self.slot_hovered(f, mouse);
                let highlighted = self.name.to_lowercase();
                for (i, s) in self.slots.iter().take(10).enumerate() {
                    let ink = if s.name == highlighted {
                        BLUE
                    } else if s.valid {
                        GREEN
                    } else {
                        DISABLED
                    };
                    button(t, &Self::slot_text(s), slot_at(i), ink, ORANGE, true);
                }
                let places = self.button_places(f);
                let enabled = [self.can_load(), self.can_save(), self.can_delete()];
                let mut mode = None;
                if let Some(i) = hovered {
                    // Over a slot: its hint instead of the buttons.
                    let s = &self.slots[i];
                    let hint = if !s.valid {
                        self.t.incompatible.clone()
                    } else {
                        let args = [("|J", s.days as i32), ("|A", s.camps as i32)];
                        let mut h = text_helper(&self.t.camps, &args);
                        h.push_str(&text_helper(&self.t.days, &args));
                        h
                    };
                    draw_in_box(t, f, &hint, HINT, BOX, true, BLUE);
                } else {
                    for i in 0..3 {
                        button(t, &self.t.buttons[i], places[i], BLUE, ORANGE, enabled[i]);
                        if enabled[i] && inside(mouse, places[i], Self::text_size(f, &self.t.buttons[i])) {
                            mode = Some(i);
                        }
                    }
                }
                let mut file = self.name.clone();
                file.push_str(".SAV");
                let line = match mode {
                    Some(i) => self.t.modes[i].replace("|A", &file),
                    None => file,
                };
                draw_in_box(t, f, &line, INPUT, BOX, false, BLUE);
            }
            Page::Jukebox => {
                let focus = self.v12.as_ref().filter(|_| page_keyed).map(|v| v.song_focus);
                for (i, (song, title)) in self.songs.iter().enumerate() {
                    let ink = if self.playing.as_deref() == Some(song.as_str()) { BLUE } else { GREEN };
                    if focus == Some(i) {
                        f.draw_at(t, title, song_at(i).0, song_at(i).1, Some(ORANGE));
                    } else {
                        button(t, title, song_at(i), ink, ORANGE, true);
                    }
                }
            }
            Page::Settings if self.v12.is_some() => {
                let focus = page_keyed.then(|| self.settings_focused());
                let mut lit = None;
                for (i, &s) in SETTINGS12.iter().enumerate() {
                    // `_fullscreenToggle.Hide()` where the window cannot
                    // be switched.
                    if s == Setting::Fullscreen && self.choices.fullscreen.is_none() {
                        continue;
                    }
                    let (text, at, enabled) = (self.setting_text(s), setting_at(i), self.setting_enabled(s));
                    let hovered = enabled && !page_keyed && inside(mouse, at, Self::text_size(f, text));
                    if focus == Some(i) || hovered {
                        f.draw_at(t, text, at.0, at.1, Some(ORANGE));
                        lit = Some(s);
                    } else {
                        button(t, text, at, GREEN, ORANGE, enabled);
                    }
                }
                // `_hintText` (40, 122), 120 × 10, centred, over whatever
                // is under it — the language's line runs into it there too.
                if let Some(hint) = lit.and_then(|s| self.setting_hint(s)) {
                    draw_in_box(t, f, hint, (40, 122), BOX, false, BLUE);
                }
            }
            Page::Settings => {
                // **Not built here**: the graphics set, the music set, the
                // window and the language cannot be switched in this port
                // yet, so their lines are there and greyed — and a greyed
                // button is never hovered, so `newburn?23`–`25` never show.
                for (text, y) in [&self.t.graphics, &self.t.music, &self.t.window, &self.t.language].into_iter().zip(SETTINGS_Y) {
                    button(t, text, (38, y), GREEN, ORANGE, false);
                }
            }
            Page::GiveUp => {
                for (i, (text, at)) in [(&self.t.new_game, GIVE_UP[0]), (&self.t.exit, GIVE_UP[1])].into_iter().enumerate() {
                    let enabled = i == 1 || !self.from_menu;
                    let focused = page_keyed && self.v12.as_ref().is_some_and(|v| v.give_up_focus == i);
                    let ink = if !enabled {
                        DISABLED
                    } else if focused || (!page_keyed && inside(mouse, at, BOX)) {
                        ORANGE
                    } else {
                        GREEN
                    };
                    draw_in_box(t, f, text, at, BOX, false, ink);
                }
            }
        }
    }
}

/// Which of the three is open.
pub enum Screen {
    Info(Info),
    Stats(Stats),
    Options(Options),
}

impl Screen {
    /// `StatisticsScene` hides the pointer (`RenderMouse = false`).
    pub fn shows_pointer(&self) -> bool {
        !matches!(self, Screen::Stats(_))
    }

    pub fn draw<'a>(&self, t: &mut Target, art: &Art, hud: &Hud, item: &dyn Fn(u8) -> Option<PackedImage<'a>>,
                    mouse: (i32, i32), ms: u32) {
        match self {
            Screen::Info(i) => i.draw(t, art, hud, item, mouse, ms),
            Screen::Stats(s) => s.draw(t, art, hud),
            Screen::Options(o) => o.draw(t, art, hud, mouse, ms),
        }
    }
}

#[cfg(test)]
mod v13_info_wheel {
    use super::*;

    fn info(items: usize) -> Info {
        Info {
            city: 0, title: String::new(), labels: Default::default(), storage: String::new(), available: String::new(),
            residents: [0; 3], details: None, v12_numbers: None, v13_lines: None,
            items: (0..items).map(|k| Stored { kind: k, sprite: None, count: 1, title: String::new() }).collect(),
            offset: 0, products: [None; 4], slot: None, automatic: Some(true), produce: None,
            food: String::new(), water: String::new(), danger: None, production_tip: None, facts: Vec::new(),
            keyboard: false, tip_dismissed: false,
        }
    }

    /// Down a row per notch over the storage, never past `items - 2`, up
    /// again; nothing outside it, nothing with two items or fewer, nothing
    /// in a legacy game.
    #[test]
    fn the_wheel_moves_the_storage_list() {
        let mut i = info(4);
        let over = (150, 140);
        assert!(i.wheel(over, -1));
        assert!(i.wheel(over, -3));
        assert_eq!(i.offset, 2);
        assert!(!i.wheel(over, -1), "at items - 2");
        assert!(i.wheel(over, 1));
        assert_eq!(i.offset, 1);
        assert!(!i.wheel((300, 20), -1), "outside StorageBounds");
        let mut two = info(2);
        assert!(!two.wheel(over, -1));
        let mut legacy = info(4);
        legacy.automatic = None;
        assert!(!legacy.wheel(over, -1));
    }
}
