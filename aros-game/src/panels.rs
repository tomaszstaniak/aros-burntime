//! The inventory and the services as v0.9.1 draws them — `InventoryWindow`,
//! `ItemGridWindow`, `InventoryScene`, `DoctorScene`, `PubScene`,
//! `RestaurantScene` — in their own 320 × 200 scene coordinates.
//!
//! Only pictures and text are here. Which slot a click means, and what it
//! does, stays with the rules and the grids in `main`.

use alloc::string::String;
use alloc::vec::Vec;
use burngfx::pack::{Pack, PackedImage};
use burngfx::view::ItemGrid;

use crate::gfx::{Align, Target};
use crate::hud::Hud;
use crate::start_view::frame;

/// `InventoryWindow` at (2, 5) with `basePos` (15, 15).
pub const WINDOW: (i32, i32) = (2, 5);
pub const BASE: (i32, i32) = (WINDOW.0 + 15, WINDOW.1 + 15);
/// The stats' first line, `basePos + (73, 11)`, and its ink.
const STATS: (i32, i32) = (WINDOW.0 + 88, WINDOW.1 + 26);
const STATS_INK: u32 = 0x0080_88C0;
/// "Attack" sits at `basePos.x + 20` on the defence's line.
const ATTACK_X: i32 = WINDOW.0 + 35;
/// `ItemWindow`'s title and the page buttons' names: `(240, 64, 56)`.
const TITLE_INK: u32 = 0x00F0_4038;
/// The EXIT-style text buttons, and their hover.
const BUTTON_INK: u32 = 0x005C_5C94;
const BUTTON_HOVER: u32 = 0x0090_A0D4;
/// The services' three lines, `(212, 212, 212)`, centred on x 237.
const SAY_INK: u32 = 0x00D4_D4D4;
/// EXIT at (25, 183); the service's own button at (116, 183).
pub const EXIT_AT: (i32, i32) = (25, 183);
pub const ACTION_AT: (i32, i32) = (116, 183);

/// The pack's grid: `(19, 83) + basePos`, three across, two down, spacing
/// (4, 5).
pub fn pack_grid() -> ItemGrid {
    ItemGrid { gap_y: 5, ..ItemGrid::plain(WINDOW.0 + 34, WINDOW.1 + 98, 3, 2, 4) }
}

/// A room's floor: (160, 20), four by five and the layer between, or at a
/// water source (160, 128), four by two.
pub fn room_grid(is_water: bool) -> ItemGrid {
    if is_water {
        ItemGrid::plain(160, 128, 4, 2, 4)
    } else {
        ItemGrid { double: true, ..ItemGrid::plain(160, 20, 4, 5, 4) }
    }
}

/// The ground around the character, `(170, 10)`, four by five, spacing 2.
pub fn ground_grid() -> ItemGrid {
    ItemGrid::plain(170, 10, 4, 5, 2)
}

/// The trader's `temporarySpace`: (156, 0), one across and six down, spacing
/// (0, 1).
pub fn parked_grid() -> ItemGrid {
    ItemGrid { gap_y: 1, ..ItemGrid::plain(156, 0, 1, 6, 0) }
}

/// The services' table: (160, 165), four by one.
pub fn table_grid() -> ItemGrid {
    ItemGrid::plain(160, 165, 4, 1, 4)
}

/// One page of the stack: whose, and what the stats say.
pub struct Page {
    /// `PageNumber`: 0 the leader, then each companion.
    pub number: usize,
    pub face: u8,
    /// `burn?(40 + Class)`.
    pub class_line: String,
    pub experience: i32,
    pub health: i32,
    pub food: i32,
    pub water: i32,
    pub attack: i32,
    pub defense: i32,
    /// A v1.2 game's line instead of attack and defence: the damage text at
    /// the attack's place, the defence text (when there is one) at the
    /// defence's, and the hazard protection a line below — v1.2's
    /// `InventoryWindow.OnRender` in the classic look.
    pub v12: Option<(String, Option<String>, String)>,
    pub is_trader: bool,
    /// Item types in the pack, by slot; `None` for a slot shown empty.
    pub items: Vec<Option<usize>>,
    /// Slots marked: the weapon and the protection; in a v1.2 game every
    /// item in use (`Character.IsItemInUse`: those two and any item with a
    /// function, `Character.cs:220-221`).
    pub selected: Vec<usize>,
    pub name: String,
}

/// An animation of a service: frames, where, `Speed` and `IntervalMargin`.
pub struct Anim<'a> {
    pub frames: Vec<PackedImage<'a>>,
    pub at: (i32, i32),
    pub fps: u32,
    /// Tenths of a frame per second, to hold 6.5 and 6.1.
    pub fps_tenths: u32,
    pub pause_ms: u32,
}

impl Anim<'_> {
    /// `SpriteAnimation.Update`, endless: frames at `Speed`, then a pause of
    /// `IntervalMargin` seconds on the wrapped-round frame, then again.
    pub fn frame_at(&self, ms: u32) -> usize {
        let n = self.frames.len() as u32;
        if n == 0 {
            return 0;
        }
        let run = n * 10_000 / self.fps_tenths.max(1);
        let cycle = run + self.pause_ms;
        let t = ms % cycle.max(1);
        if t >= run {
            0
        } else {
            ((t * self.fps_tenths / 10_000) as usize).min(n as usize - 1)
        }
    }
}

pub fn anim<'a>(pack: &Pack<'a>, track: &str, at: (i32, i32), fps_tenths: u32, pause_ms: u32) -> Anim<'a> {
    let frames = (0..80)
        .map_while(|n| {
            let mut key = String::from("an/");
            key.push_str(track);
            key.push('/');
            push_num(&mut key, n);
            crate::start_view::image(pack, &key)
        })
        .collect();
    Anim { frames, at, fps: fps_tenths / 10, fps_tenths, pause_ms }
}

fn push_num(s: &mut String, mut v: u32) {
    let mut d = [0u8; 10];
    let mut n = 0;
    loop {
        d[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for i in (0..n).rev() {
        s.push(d[i] as char);
    }
}

pub struct Art<'a> {
    inventory_left: Option<PackedImage<'a>>,
    inv: Vec<Option<PackedImage<'a>>>,
    /// v1.2's mark on what is in use, `gfx/ui/item_equipped.png`
    /// (`ItemGridWindow.cs:122,146-157`).
    equipped: Option<PackedImage<'a>>,
    pub hint2: Option<PackedImage<'a>>,
    pub arzt: Option<PackedImage<'a>>,
}

impl<'a> Art<'a> {
    pub fn load(pack: &Pack<'a>) -> Art<'a> {
        Art {
            inventory_left: crate::start_view::image(pack, "ui/inventory_left"),
            inv: (0..4).map(|n| frame(pack, "inv", n)).collect(),
            equipped: crate::start_view::image(pack, "ui/item_equipped"),
            hint2: crate::start_view::image(pack, "scene/hint2"),
            arzt: crate::start_view::image(pack, "scene/arzt"),
        }
    }

    /// `InventoryWindow.OnRender` and its children, for the pages given
    /// front first. `pages[0]` is the one whose pack and face are shown.
    pub fn draw_window(&self, t: &mut Target, hud: &Hud, pages: &[Page], item: &dyn Fn(usize) -> Option<PackedImage<'a>>,
                       texts: &dyn Fn(bool, u32) -> String, ms: u32) {
        let n = pages.len().max(1) as i32;
        for i in (0..n).rev() {
            if let Some(back) = &self.inventory_left {
                t.blit(back, BASE.0 - 3 * i, BASE.1 - 3 * i);
            }
        }
        let Some(front) = pages.first() else { return };
        hud.draw_face(t, front.face as usize, BASE.0, BASE.1, ms);
        // The page buttons, the front page's number on the first.
        for (i, page) in pages.iter().enumerate() {
            if let Some(img) = hud.munt(5 + page.number as u8) {
                t.blit(img, BASE.0 - 3 * i as i32, BASE.1 + 142 - 18 * i as i32);
            }
        }
        // The stats, ten pixels a line.
        if let Some(f) = &hud.font {
            let mut y = STATS.1;
            let line = |t: &mut Target, s: &str, y: &mut i32| {
                f.draw_at(t, s, STATS.0, *y, Some(STATS_INK));
                *y += 10;
            };
            line(t, &front.class_line, &mut y);
            line(t, &texts(false, 380).replace("|A", &num(front.experience)), &mut y);
            line(t, &texts(false, 381).replace("|B", &num(front.health)), &mut y);
            if !front.is_trader {
                line(t, &texts(false, 403).replace("|D", &num(front.food)), &mut y);
                line(t, &texts(false, 402).replace("|C", &num(front.water)), &mut y);
                match &front.v12 {
                    Some((damage, defense, hazard)) => {
                        // `statsFont = IsNewGfx ? smallFont : font`
                        // (`InventoryWindow.cs:299`).
                        let sf = if crate::gfx::newgfx() { hud.small.as_ref().unwrap_or(f) } else { f };
                        if let Some(d) = defense {
                            sf.draw_at(t, d, STATS.0, y, Some(STATS_INK));
                        }
                        sf.draw_at(t, damage, ATTACK_X, y, Some(STATS_INK));
                        sf.draw_at(t, hazard, ATTACK_X, y + 10, Some(STATS_INK));
                    }
                    None => {
                        f.draw_at(t, &texts(true, 99).replace("{attack}", &num(front.attack)), ATTACK_X, y, Some(STATS_INK));
                        f.draw_at(t, &texts(true, 100).replace("{defense}", &num(front.defense)), STATS.0, y, Some(STATS_INK));
                    }
                }
            }
        }
        // The pack, and the frames on what is drawn: v1.2's
        // `item_equipped.png` (`ItemGridWindow.OnRender`), v0.9.1's
        // `inv.raw?3` — and that where a pack has not the former.
        let grid = pack_grid();
        self.draw_items(t, &grid, &front.items, item);
        let legacy = self.inv.get(3).and_then(|i| i.as_ref());
        let mark = if front.v12.is_some() { self.equipped.as_ref().or(legacy) } else { legacy };
        for &slot in &front.selected {
            if let (Some(at), Some(sel)) = (grid.slot_at(slot), mark) {
                t.blit(sel, at.x, at.y);
            }
        }
    }

    /// The trader's `InventoryWindow`, `InventorySide.Right`: `inv.raw?2`
    /// pages stacked (3, −2) apart, the face at `basePos + (67, 0)`, the stats
    /// at `basePos.x + 11`, the pack grid given, two page buttons on the right.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_trader_window(&self, t: &mut Target, hud: &Hud, at: (i32, i32), page: usize, face: u8,
                              lines: &[String], grid: &ItemGrid, items: &[Option<usize>],
                              item: &dyn Fn(usize) -> Option<PackedImage<'a>>, ms: u32) {
        let base = (at.0 + 15, at.1 + 15);
        for i in (0..2).rev() {
            if let Some(Some(back)) = self.inv.get(2) {
                t.blit(back, base.0 + 3 * i, base.1 - 2 * i);
            }
        }
        hud.draw_face(t, face as usize, base.0 + 67, base.1, ms);
        for i in 0..2usize {
            // The front page's number on the first button.
            let n = if i == 0 { page } else { 1 - page };
            if let Some(img) = hud.munt(5 + n as u8) {
                t.blit(img, base.0 + 120 + 3 * i as i32, base.1 + 142 - 18 * i as i32);
            }
        }
        if let Some(f) = &hud.font {
            for (i, l) in lines.iter().enumerate() {
                f.draw_at(t, l, base.0 + 11, base.1 + 11 + 10 * i as i32, Some(STATS_INK));
            }
        }
        self.draw_items(t, grid, items, item);
    }

    /// An `ExchangeWindow`: `inv.raw?1`, its title centred at (63, 6), its
    /// light at (10, 7), and what is in it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_exchange(&self, t: &mut Target, hud: &Hud, at: (i32, i32), title: &str, light: u32,
                         grid: &ItemGrid, items: &[Option<usize>], item: &dyn Fn(usize) -> Option<PackedImage<'a>>) {
        if let Some(Some(back)) = self.inv.get(1) {
            t.blit(back, at.0, at.1);
        }
        if let Some(f) = &hud.font {
            f.draw(t, title, at.0 + 63, at.1 + 6, Align::Centre, Align::Start, Some(STATS_INK));
        }
        t.fill(at.0 + 10, at.1 + 7, 12, 5, light, 255);
        self.draw_items(t, grid, items, item);
    }

    /// `grid.png` over the items that are offered.
    pub fn draw_masks(&self, t: &mut Target, grid: &ItemGrid, slots: &[usize], mask: Option<&PackedImage<'a>>) {
        let Some(m) = mask else { return };
        for &s in slots {
            if let Some(at) = grid.slot_at(s) {
                t.blit(m, at.x, at.y);
            }
        }
    }

    /// Where the trader's page buttons are.
    pub fn trader_page_button_at(hud: &Hud, at: (i32, i32), x: i32, y: i32) -> Option<usize> {
        let base = (at.0 + 15, at.1 + 15);
        let (w, h) = hud.munt(5).map_or((16, 16), |m| (m.width as i32, m.height as i32));
        (0..2).find(|&i| {
            let (bx, by) = (base.0 + 120 + 3 * i as i32, base.1 + 142 - 18 * i as i32);
            x >= bx && y >= by && x < bx + w && y < by + h
        })
    }

    /// The pictures of a grid's items.
    pub fn draw_items(&self, t: &mut Target, grid: &ItemGrid, items: &[Option<usize>],
                      item: &dyn Fn(usize) -> Option<PackedImage<'a>>) {
        for (i, kind) in items.iter().enumerate() {
            let (Some(kind), Some(at)) = (kind, grid.slot_at(i)) else { continue };
            if let Some(img) = item(*kind) {
                t.blit(&img, at.x, at.y);
            }
        }
    }

    /// The name of the page button under the pointer, `(6, -9)` from it.
    pub fn draw_page_name(&self, t: &mut Target, hud: &Hud, index: usize, name: &str) {
        if let Some(f) = &hud.font {
            let (x, y) = page_button(index);
            f.draw(t, name, x + 6, y - 9, Align::Centre, Align::Start, Some(TITLE_INK));
        }
    }
}

/// Where page button `i` is.
pub fn page_button(i: usize) -> (i32, i32) {
    (BASE.0 - 3 * i as i32, BASE.1 + 142 - 18 * i as i32)
}

/// Page buttons are `munt.raw` 5–9, 12 × 16 or so; hit by their picture.
pub fn page_button_at(hud: &Hud, pages: usize, x: i32, y: i32) -> Option<usize> {
    (0..pages).find(|&i| {
        let (bx, by) = page_button(i);
        let (w, h) = hud.munt(5).map_or((16, 16), |m| (m.width as i32, m.height as i32));
        x >= bx && y >= by && x < bx + w && y < by + h
    })
}

/// `ItemWindow`'s title, centred above the cell in red.
pub fn draw_title(t: &mut Target, hud: &Hud, grid: &ItemGrid, slot: usize, title: &str) {
    if let (Some(f), Some(at)) = (&hud.font, grid.slot_at(slot)) {
        f.draw(t, title, at.x + 16, at.y - 9, Align::Centre, Align::Start, Some(TITLE_INK));
    }
}

/// A text-only `Button`: drawn from its top left, red-blue when hovered.
/// Returns its size, for hit-testing.
pub fn draw_text_button(t: &mut Target, hud: &Hud, at: (i32, i32), text: &str, hover: bool) {
    if let Some(f) = &hud.font {
        f.draw_at(t, text, at.0, at.1, Some(if hover { BUTTON_HOVER } else { BUTTON_INK }));
    }
}

pub fn text_button_hit(hud: &Hud, at: (i32, i32), text: &str, x: i32, y: i32) -> bool {
    let Some(f) = &hud.font else { return false };
    x >= at.0 && y >= at.1 - 2 && x < at.0 + f.width(text) && y < at.1 + 10
}

/// A service's three lines, centred on x 237, nine apart from y 4.
pub fn draw_saying(t: &mut Target, hud: &Hud, lines: &[String]) {
    if let Some(f) = &hud.font {
        for (i, l) in lines.iter().enumerate() {
            f.draw(t, l, 237, 4 + 9 * i as i32, Align::Centre, Align::Start, Some(SAY_INK));
        }
    }
}

/// The water source's "N days" and its red bar, bottom right.
pub fn draw_reserve(t: &mut Target, hud: &Hud, text: &str, reserve: i32) {
    t.fill(320 - 2 - 14, 200 - 10 - reserve * 2, 14, reserve * 2, 0x00F0_4038, 255);
    if let Some(f) = &hud.font {
        // `DrawText(target.Size, …)` clamped into the window: the text's own
        // width and height from the bottom-right corner.
        let x = 320 - f.width(text);
        let y = 200 - f.metric_height();
        f.draw_at(t, text, x, y, Some(0x0048_484C));
    }
}

/// v1.3's spring bar (`InventoryScene.DrawWaterBarSegment`): 14 wide and 2
/// a unit, from 10 above the bottom-right corner and 2 in; the capacity in
/// translucent blue (112, 136, 208 at alpha 128), the reserve (at most the
/// capacity) up to `daily` in that blue, the rest in light blue
/// (84, 179, 194); the text as v1.2's.
pub fn draw_reserve_v13(t: &mut Target, hud: &Hud, text: &str, reserve: i32, capacity: i32, daily: i32) {
    let stored = reserve.min(capacity);
    let at_daily = stored.min(daily).max(0);
    let above = (stored - daily).max(0);
    let segment = |t: &mut Target, below: i32, units: i32, rgb: u32, alpha: u32| {
        if units > 0 {
            t.fill(320 - 2 - 14, 200 - 10 - (below + units) * 2, 14, units * 2, rgb, alpha);
        }
    };
    segment(t, 0, capacity, 0x0070_88D0, 128);
    segment(t, 0, at_daily, 0x0070_88D0, 255);
    segment(t, at_daily, above, 0x0054_B3C2, 255);
    if let Some(f) = &hud.font {
        let x = 320 - f.width(text);
        let y = 200 - f.metric_height();
        f.draw_at(t, text, x, y, Some(0x0048_484C));
    }
}

/// `TextHelper`'s plural: `plural|#singular|#zero`, by the number.
pub fn plural(text: &str, n: i32) -> String {
    let parts: Vec<&str> = text.split("|#").collect();
    let pick = match (n, parts.len()) {
        (1, l) if l > 1 => parts[1],
        (0, l) if l > 2 => parts[2],
        _ => parts[0],
    };
    String::from(pick)
}

/// `TextHelper.Get`: the arguments in the order they were added; the first
/// one the plural form contains picks the form — `|#` separates plural,
/// singular and none — by being `1` or `0`, and every one after it is put
/// into whatever form was picked.
pub fn text_helper(raw: &str, args: &[(&str, i32)]) -> String {
    let forms: Vec<&str> = raw.split("|#").collect();
    let mut result: Option<String> = None;
    for (k, v) in args {
        let current = result.clone().unwrap_or_else(|| String::from(forms[0]));
        if !current.contains(k) {
            continue;
        }
        let value = num(*v);
        let base = match result.take() {
            Some(r) => r,
            None if forms.len() >= 3 && *v == 0 => String::from(forms[2]),
            None if forms.len() >= 2 && *v == 1 => String::from(forms[1]),
            None => String::from(forms[0]),
        };
        result = Some(base.replace(k, &value));
    }
    result.unwrap_or_else(|| String::from(forms[0]))
}

pub fn num(n: i32) -> String {
    let mut s = String::new();
    if n < 0 {
        s.push('-');
    }
    push_num(&mut s, n.unsigned_abs());
    s
}
