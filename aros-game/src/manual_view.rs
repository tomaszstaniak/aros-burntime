//! Drawing v1.2's manual and the start screen's notes
//! (`GUI/ManualWindow.cs`: `OnRender`, `RenderTextPage`, `RenderEntry` and
//! the entries' own renderers) from [`burngfx::manual_window`]'s state.
//!
//! The texts are v1.2's own (`manual.txt`, `manualui.txt`,
//! `setupnotes.txt`), packed from the player's v1.2 copy. Every font here is
//! `font-small.txt`: v1.2's title font, `font.txt`, is the same file, glyph
//! for glyph and line for line.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use burngfx::construction::Constructions;
use burngfx::items::ItemTypes;
use burngfx::manual_window::{self as mw, Entry, Kind, Metrics, Window};
use burngfx::pack::{Pack, PackedImage};
use burngfx::production::Productions;
use burngfx::rules::RuleBook;

use crate::gfx::{Face, Target};
use crate::hud::{Bar, Hud};

/// `ClassicColors.DialogText`, (240, 164, 56).
const DIALOG_TEXT: u32 = 0x00F0_A438;
/// `ClassicColors.LightGray`, (212, 212, 212).
const LIGHT_GRAY: u32 = 0x00D4_D4D4;
/// `ClassicColors.MenuText`, (108, 116, 168).
const MENU_TEXT: u32 = 0x006C_74A8;
/// `ClassicColors.MenuTextHover`, (240, 64, 56).
const MENU_TEXT_HOVER: u32 = 0x00F0_4038;

/// The fonts, pictures and texts, loaded once.
pub struct ManualArt<'a> {
    pub font: Option<Face<'a>>,
    city: Option<PackedImage<'a>>,
    manual: Vec<Vec<String>>,
    ui: Vec<String>,
    notes: Vec<Vec<String>>,
    /// The notes' first page: the port's own release notes, as v1.2 shows
    /// its changelog there (`SetupPatchNotes`).
    patch_notes: Vec<String>,
}

fn text<'a>(pack: &Pack<'a>, key: &str) -> &'a str {
    crate::localized(pack, key).and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("")
}

impl<'a> ManualArt<'a> {
    pub fn load(pack: &Pack<'a>, fallback: Option<Face<'a>>, release_notes: &str) -> ManualArt<'a> {
        let ui = mw::sections(text(pack, "v12/manualui")).into_iter().next().unwrap_or_default();
        ManualArt {
            font: crate::start_view::face(pack, "font/small").or(fallback),
            city: crate::start_view::image(pack, "ui/manual_city"),
            manual: mw::sections(text(pack, "v12/manual")),
            ui,
            notes: mw::sections(text(pack, "v12/setupnotes")),
            patch_notes: mw::patch_notes(&unwrap_markdown(release_notes)),
        }
    }

    /// Whether the pack has the manual at all.
    pub fn has_manual(&self) -> bool {
        self.manual.len() > mw::LABELS
    }

    /// `manualui?N`: MANUAL is 5, Close 6.
    pub fn ui(&self, n: usize) -> String {
        self.ui.get(n).cloned().unwrap_or_default()
    }

    /// `setupnotes?N`: the notes' tab names, and NOTES' own prompt (3).
    pub fn notes_label(&self, n: usize) -> String {
        self.notes.first().and_then(|s| s.get(n)).cloned().unwrap_or_default()
    }

    fn labels(&self) -> &[String] {
        self.manual.get(mw::LABELS).map_or(&[], |v| v.as_slice())
    }

    fn label(&self, n: usize) -> String {
        self.labels().get(n).cloned().unwrap_or_default()
    }

    /// `manual?sN`, or the notes' page N.
    fn page_lines(&self, notes: bool, page: usize) -> Vec<String> {
        if !notes {
            return self.manual.get(page).cloned().unwrap_or_default();
        }
        if page == 0 {
            return self.patch_notes.clone();
        }
        self.notes.get(page).cloned().unwrap_or_default()
    }

    fn metrics(&self) -> Metrics {
        let line = self.font.as_ref().map_or(11, |f| f.line_height());
        Metrics { line, title_line: line }
    }

    fn font_height(&self) -> i32 {
        self.font.as_ref().map_or(8, |f| f.metric_height())
    }

    fn width(&self, s: &str) -> i32 {
        self.font.as_ref().map_or(0, |f| coded_width(f, s))
    }

    /// The Close button: `manualui?6` centred, `Size.y - 17`, in the
    /// window's own coordinates, with the Back key's glyph beside it when
    /// one is drawn (`PositionFooter`, `ManualWindow.cs:127-137`).
    pub fn close_rect(&self, height: i32, back: &[burngfx::prompts::Part]) -> (i32, i32, i32, i32) {
        let label = self.ui(6);
        let w = self.width(&label);
        let prompt_w = match (&self.font, back.is_empty()) {
            (Some(f), false) => burngfx::prompts::measure(f, EXIT_BRACKETS, back, "", false) + 2,
            _ => 0,
        };
        ((mw::WIDTH - w - prompt_w) / 2, height - 17, w, self.metrics().line)
    }
}

/// `_exitControlRenderer`'s brackets: `brackets: false`.
const EXIT_BRACKETS: burngfx::prompts::Brackets = burngfx::prompts::Brackets { always: false, text_controls: true };

/// The Back key's control beside Close: at the keys or the pad, while hints
/// are shown (`ManualWindow.cs:131-135,185-193`); empty otherwise.
pub fn back_control(p: &crate::prompt_view::Prompts, mode: Option<burngfx::prompts::Mode>) -> burngfx::prompts::Control {
    use burngfx::prompts::{Label, Mode, Prompt, PREFER_SPACE};
    match mode.filter(|_| p.hints.get().shows()) {
        Some(m @ (Mode::Keyboard | Mode::Gamepad)) => {
            p.control(m, &Prompt::action(burngfx::input::Action::Back, Label::Text("")), PREFER_SPACE)
        }
        _ => burngfx::prompts::Control::new(),
    }
}

/// The port's release notes as `SetupPatchNotes.Format` expects a
/// changelog: one line a paragraph or item. Ours are wrapped at 72 columns
/// and use Markdown's emphasis and code marks, which the font would print;
/// so continuation lines are joined, `**` and backticks dropped, and the
/// dashes the font has not got written as `-`.
pub fn unwrap_markdown(md: &str) -> String {
    let mut out = String::new();
    let mut open = false;
    for raw in md.lines() {
        let line = raw.trim();
        let starts_block = line.is_empty() || line.starts_with('#') || line.starts_with("- ") || line.starts_with("* ")
            || line.starts_with('|') || raw.starts_with("    ");
        if open && !starts_block {
            out.push(' ');
            out.push_str(line);
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        open = !line.is_empty() && !line.starts_with('#') && !raw.starts_with("    ");
    }
    out.replace("**", "").replace('`', "").replace(['\u{2014}', '\u{2013}'], "-").replace('\u{2192}', "->")
}

/// Where the window's top left is, over a scene this size.
pub fn origin(scene: (i32, i32)) -> (i32, i32) {
    ((scene.0 - mw::WIDTH) / 2, 0)
}

/// What the entries read from the game being played.
pub struct Game<'x, 'a> {
    pub types: &'x ItemTypes,
    pub productions: &'x Productions,
    pub constructions: &'x Constructions,
    pub book: Option<&'x RuleBook>,
    pub extended: bool,
    /// An item's title.
    pub title: &'x dyn Fn(usize) -> String,
    /// `gst.raw?N`.
    pub image: &'x dyn Fn(u8) -> Option<PackedImage<'a>>,
    /// `syssze.raw?N`.
    pub body: &'x dyn Fn(u8) -> Option<PackedImage<'a>>,
    /// The active player's flag, this frame.
    pub flag: Option<&'x PackedImage<'a>>,
}

impl<'a> Game<'_, 'a> {
    fn kind(&self, id: &str) -> Option<usize> {
        self.types.index_of(id)
    }

    fn picture(&self, id: &str) -> Option<PackedImage<'a>> {
        self.kind(id).and_then(|k| self.types.sprite_of(k)).and_then(|s| (self.image)(s))
    }

    /// `GetRecipes(game).FirstOrDefault(r => r.Result == id)`.
    fn recipe(&self, id: &str) -> Option<&burngfx::construction::Recipe> {
        self.constructions.iter().find(|r| r.available && r.result == id)
    }

    fn production(&self, id: &str) -> Option<&burngfx::production::Production> {
        self.productions.worked_by(id).and_then(|i| self.productions.get(i))
    }

    fn tier_width(&self) -> i32 {
        self.book.map_or(1, |b| b.rules.experience_tier_width)
    }

    /// `RuleBook.CalculateWaterOutput`.
    fn water_output(&self, base: i32, id: &str) -> i32 {
        let rule = self.book.map_or(burngfx::rules::WaterOutput::RemasterFixed, |b| b.rules.water_output);
        burngfx::v12_day::water_output(rule, base, id == "item_hand_pump", id == "item_industrial_pump")
    }
}

struct Measure<'g, 'x, 'a> {
    game: &'g Game<'x, 'a>,
    city: i32,
}

impl mw::Content for Measure<'_, '_, '_> {
    fn has_item(&self, id: &str) -> bool {
        self.game.kind(id).is_some()
    }
    fn item_height(&self, id: &str) -> Option<i32> {
        self.game.picture(id).map(|p| p.height as i32)
    }
    fn class_height(&self, sprite: u8) -> i32 {
        (self.game.body)(sprite).map_or(0, |p| p.height as i32)
    }
    fn city_height(&self) -> i32 {
        self.city
    }
    fn flag_height(&self) -> i32 {
        self.game.flag.map_or(0, |p| p.height as i32)
    }
    fn recipe_items(&self, result: &str) -> Option<Vec<String>> {
        self.game.recipe(result).map(|r| r.items.clone())
    }
}

/// Text with v1.2's inline marks: `~d1` is the dots' frame 1, and so on
/// for `brdn` (`Font.ParseText`).
fn segments(s: &str) -> Vec<Result<&str, (Bar, i32)>> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'~' && i + 2 < bytes.len() {
            if let Some(bar) = bytes.get(i + 1).and_then(|&c| Bar::from_code(c as char)) {
                let mut j = i + 2;
                let mut value: i32 = 0;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    value = value.saturating_mul(10).saturating_add((bytes[j] - b'0') as i32);
                    j += 1;
                }
                if j > i + 2 {
                    if start < i {
                        out.push(Ok(&s[start..i]));
                    }
                    out.push(Err((bar, value)));
                    i = j;
                    start = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    if start < s.len() {
        out.push(Ok(&s[start..]));
    }
    out
}

fn coded_width(f: &Face, s: &str) -> i32 {
    segments(s).iter().map(|seg| match seg {
        Ok(t) => f.width(t),
        Err((bar, _)) => bar.frames().2,
    }).sum()
}

fn draw_coded(t: &mut Target, f: &Face, hud: &Hud, s: &str, x: i32, y: i32, ink: u32) {
    let mut cursor = x;
    for seg in segments(s) {
        match seg {
            Ok(text) => {
                f.draw_at(t, text, cursor, y, Some(ink));
                cursor += f.width(text);
            }
            Err((bar, value)) => {
                let (first, count, advance) = bar.frames();
                if let Some(sheet) = hud.indicators() {
                    t.blit_sheet(sheet, first + value.clamp(0, count - 1) as u32, 8, 12, cursor, y + f.draw_offset());
                }
                cursor += advance;
            }
        }
    }
}

/// `FitText`: cut with "..." to the width.
fn fit(f: &Face, s: &str, width: i32) -> String {
    if coded_width(f, s) <= width {
        return s.to_string();
    }
    let mut cut: String = s.to_string();
    while !cut.is_empty() && {
        let mut probe = cut.clone();
        probe.push_str("...");
        coded_width(f, &probe) > width
    } {
        cut.pop();
    }
    let mut out = cut.trim_end().to_string();
    out.push_str("...");
    out
}

/// Everything one frame of the window draws, and the page's layout kept
/// in step (`_textScroll` clamped as `RenderTextPage` does). `scene` is the
/// size of the scene it is opened over, whose top left is `t.origin`;
/// `mouse` is in that scene's coordinates.
#[allow(clippy::too_many_arguments)]
pub fn draw(t: &mut Target, w: &mut Window, art: &ManualArt, hud: &Hud, game: &Game, notes: bool, scene: (i32, i32),
            mouse: (i32, i32)) {
    let Some(f) = art.font.as_ref() else { return };
    let (ox, oy) = origin(scene);
    let height = scene.1;
    t.fill(ox, oy, mw::WIDTH, height, 0, 176);
    let local = (mouse.0 - ox, mouse.1 - oy);
    w.hovered = w.tab_at(local.0, local.1);
    let tab_w = w.tab_width();
    for i in 0..w.pages() {
        let ink = if i == w.page {
            MENU_TEXT_HOVER
        } else if w.hovered == Some(i) {
            DIALOG_TEXT
        } else {
            MENU_TEXT
        };
        let label = if notes { art.notes_label(i) } else { art.ui(i) };
        let cx = ox + mw::TABS_LEFT + tab_w * i as i32 + tab_w / 2;
        f.draw_at(t, &label, cx - f.width(&label) / 2, oy + 8, Some(ink));
    }

    // `RenderTextPage`.
    let m = art.metrics();
    let measure = |s: &str| coded_width(f, s);
    let lines = mw::wrap(&art.page_lines(notes, w.page), mw::WIDTH - 30, &measure, &measure);
    let content = Measure { game, city: art.city.as_ref().map_or(0, |c| c.height as i32) };
    let entries = mw::entries(&lines, game.extended, notes, m, &content);
    let total: i32 = entries.iter().map(|e| e.lines).sum();
    let visible = mw::visible_lines(height, m.line, art.font_height());
    let maximum = (total - visible).max(0);
    w.clamp(maximum);
    let scroll = w.scroll();
    let at = (ox + mw::CONTENT_LEFT, oy + mw::CONTENT_TOP);
    // `target.GetSubBuffer(new Rect(10, 24, Size.x - 20, ContentHeight))`
    // (ManualWindow.cs:209-210): nothing of the text is drawn outside it —
    // an entry taller than its lines is cut at the footer.
    let saved = t.px.clone();
    let mut line = 0;
    for e in &entries {
        let start = line;
        line += e.lines;
        if start < scroll || line > scroll + visible {
            continue;
        }
        entry(t, f, hud, art, game, e, at, 2 + (start - scroll) * m.line, m);
    }
    t.keep_inside(&saved, at.0, at.1, mw::WIDTH - 20, mw::content_height(height, m.line));
    // `RenderScrollBar`.
    if maximum > 0 {
        let track = height - mw::FOOTER - 28;
        let thumb = (track * visible / total.max(1)).max(10);
        let thumb_y = 26 + (track - thumb) * scroll / maximum;
        t.fill(ox + 290, oy + 26, 2, track, 0x006C_74A8, 80);
        t.fill(ox + 290, oy + thumb_y, 2, thumb, 0x00F0_A438, 220);
    }
    // The Close button: `_mutedFont`, `_titleFont` while hovered.
    let back = back_control(&hud.prompts, hud.prompts.mode.get());
    let (cx, cy, cw, ch) = art.close_rect(height, &back);
    let over = local.0 >= cx && local.0 < cx + cw && local.1 >= cy && local.1 < cy + ch;
    f.draw_at(t, &art.ui(6), ox + cx, oy + cy, Some(if over { DIALOG_TEXT } else { MENU_TEXT }));
    if !back.is_empty() {
        let x = ox + cx + cw + 2;
        let right = x + burngfx::prompts::measure(f, EXIT_BRACKETS, &back, "", false);
        hud.prompts.draw_label_first(t, f, EXIT_BRACKETS, &back, "", right, oy + cy, MENU_TEXT, MENU_TEXT, true);
    }
}

#[allow(clippy::too_many_arguments)]
fn entry(t: &mut Target, f: &Face, hud: &Hud, art: &ManualArt, g: &Game, e: &Entry, at: (i32, i32), y: i32, m: Metrics) {
    let (ax, ay) = at;
    let inner_w = mw::WIDTH - 20;
    match e.kind {
        Kind::Text => draw_coded(t, f, hud, &e.payload, ax + 6, ay + y, LIGHT_GRAY),
        Kind::Heading => {
            let offset = (e.lines * m.line - f.metric_height()) / 2;
            draw_coded(t, f, hud, &e.payload, ax + 6, ay + y + offset, DIALOG_TEXT);
        }
        Kind::Subheading => draw_coded(t, f, hud, &e.payload, ax + 6, ay + y, DIALOG_TEXT),
        Kind::Tiers => {
            let width = g.tier_width();
            for tier in 0..4 {
                let mut s = String::from("~d");
                s.push_str(&crate::panels::num(tier + 1));
                s.push(' ');
                s.push_str(&crate::panels::num(tier * width));
                if tier == 3 {
                    s.push('+');
                } else {
                    s.push('-');
                    s.push_str(&crate::panels::num((tier + 1) * width - 1));
                }
                draw_coded(t, f, hud, &s, ax + 6 + tier * 68, ay + y, LIGHT_GRAY);
            }
        }
        Kind::ClassImage => {
            if let Some(img) = (g.body)(mw::class_sprite(&e.payload)) {
                t.blit(&img, ax + (inner_w - img.width as i32) / 2, ay + y);
            }
        }
        Kind::Goal if e.payload == "@flag" => {
            let Some(flag) = g.flag else { return };
            let (fw, fh) = (flag.width as i32, flag.height as i32);
            let (left, top, right, bottom) = mw::flag_crop(fw, fh);
            let (w, h) = (fw - left - right, fh - top - bottom);
            if w > 0 && h > 0 {
                let x = (inner_w - w) / 2;
                let image_y = y + (e.lines * m.line - h) / 2;
                t.blit_part(flag, (left as u32, top as u32, w as u32, h as u32), 1, ax + x, ay + image_y, None);
            }
        }
        Kind::Goal => {
            if let Some(city) = &art.city {
                let x = (inner_w - city.width as i32) / 2;
                let city_y = y + (e.lines * m.line - city.height as i32) / 2;
                t.blit(city, ax + x, ay + city_y);
            }
        }
        Kind::SupplyExample => supply_example(t, f, hud, art, g, &e.payload, (ax, ay + y), inner_w),
        Kind::ContextItem => {
            let ids: Vec<&str> = e.payload.split('|').collect();
            if ids.len() > 1 {
                for (column, id) in ids.iter().take(2).enumerate() {
                    if g.kind(id).is_some() {
                        context_item(t, f, hud, art, g, id, (ax + column as i32 * 139, ay + y), 139);
                    }
                }
            } else if g.kind(ids[0]).is_some() {
                context_item(t, f, hud, art, g, ids[0], (ax, ay + y), inner_w);
            }
        }
        Kind::Production => production(t, f, hud, art, g, &e.payload, (ax, ay + y), inner_w),
        Kind::Construction => construction(t, f, hud, g, &e.payload, (ax, ay + y), inner_w),
    }
}

/// `RenderSupplyExample`.
#[allow(clippy::too_many_arguments)]
fn supply_example(t: &mut Target, f: &Face, hud: &Hud, art: &ManualArt, g: &Game, id: &str, at: (i32, i32), width: i32) {
    let Some(k) = g.kind(id) else { return };
    let description = if id == "item_gas_mask" {
        art.label(10)
    } else {
        let water = g.types.water_of(k);
        let value = if water > 0 { water } else { g.types.food_of(k) };
        art.label(if water > 0 { 11 } else { 12 }).replace("|A", &crate::panels::num(value))
    };
    let title = (g.title)(k);
    let Some(sprite) = g.picture(id) else { return };
    let text_w = coded_width(f, &title).max(coded_width(f, &description));
    let block = sprite.width as i32 + 3 + text_w;
    let x = at.0 + (width - block) / 2;
    t.blit(&sprite, x, at.1);
    draw_coded(t, f, hud, &title, x + sprite.width as i32 + 3, at.1 + 4, DIALOG_TEXT);
    draw_coded(t, f, hud, &description, x + sprite.width as i32 + 3, at.1 + 15, LIGHT_GRAY);
}

/// `RenderProductionLine`.
#[allow(clippy::too_many_arguments)]
fn production(t: &mut Target, f: &Face, hud: &Hud, art: &ManualArt, g: &Game, payload: &str, at: (i32, i32), width: i32) {
    let ids: Vec<&str> = payload.split('|').collect();
    if ids.len() < 2 || g.kind(ids[0]).is_none() || g.kind(ids[1]).is_none() {
        return;
    }
    let Some(p) = g.production(ids[0]) else { return };
    let Some(k) = g.kind(ids[0]) else { return };
    let one = p.rate(1, 1, 0).food_per_day;
    let most = p.rate(p.max_combination, 1, 0).food_per_day;
    let first = art.label(13).replace("|A", &crate::panels::num(one));
    let second = art.label(14).replace("|A", &crate::panels::num(most)).replace("|B", &crate::panels::num(p.max_combination as i32));
    let title = (g.title)(k);
    let Some(sprite) = g.picture(ids[0]) else { return };
    let text_w = coded_width(f, &title).max(coded_width(f, &first)).max(coded_width(f, &second));
    let x = at.0 + (width - (sprite.width as i32 + 3 + text_w)) / 2;
    t.blit(&sprite, x, at.1);
    let tx = x + sprite.width as i32 + 3;
    draw_coded(t, f, hud, &title, tx, at.1 + 1, DIALOG_TEXT);
    draw_coded(t, f, hud, &first, tx, at.1 + 12, LIGHT_GRAY);
    draw_coded(t, f, hud, &second, tx, at.1 + 23, LIGHT_GRAY);
}

/// `RenderConstructionLine`: the result, a colon, and the parts with `+`
/// between them.
fn construction(t: &mut Target, f: &Face, hud: &Hud, g: &Game, result: &str, at: (i32, i32), width: i32) {
    if g.kind(result).is_none() {
        return;
    }
    let Some(recipe) = g.recipe(result) else { return };
    let parts: Vec<PackedImage> = recipe.items.iter().filter(|id| g.kind(id).is_some()).filter_map(|id| g.picture(id)).collect();
    let Some(made) = g.picture(result) else { return };
    let (colon, plus) = (f.width(":"), f.width("+"));
    let total = made.width as i32 + 4 + colon + 4 + parts.iter().map(|p| p.width as i32).sum::<i32>()
        + (parts.len() as i32 - 1).max(0) * (plus + 8);
    let mut x = at.0 + (width - total) / 2;
    t.blit(&made, x, at.1);
    x += made.width as i32 + 4;
    draw_coded(t, f, hud, ":", x, at.1 + 11, LIGHT_GRAY);
    x += colon + 4;
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            draw_coded(t, f, hud, "+", x + 4, at.1 + 11, LIGHT_GRAY);
            x += plus + 8;
        }
        t.blit(p, x, at.1);
        x += p.width as i32;
    }
}

/// `GetContextItemText`: the statistic and the detail line.
fn context_text(art: &ManualArt, g: &Game, id: &str, k: usize) -> (String, String) {
    let defense = g.types.defense_of(k);
    if defense > 0 {
        return (art.label(15).replace("|A", &crate::panels::num(defense)), String::new());
    }
    if id == "item_hand_pump" || id == "item_industrial_pump" {
        let output = g.water_output(2, id);
        return (art.label(16).replace("|A", &crate::panels::num(output)), art.label(17).replace("|B", "2"));
    }
    let Some(t) = g.types.get(k) else { return (String::new(), String::new()) };
    if t.damage_table.is_empty() {
        return (equipment_statistic(art, g, id, k), material_line(art, g, id));
    }
    let width = g.tier_width();
    let first = burngfx::table_combat::preview(&t.damage_table, 0, width).unwrap_or((0, 0));
    let last = burngfx::table_combat::preview(&t.damage_table, width * 3, width).unwrap_or((0, 0));
    (
        art.label(18).replace("|A", &crate::panels::num(first.0)).replace("|B", &crate::panels::num(first.1)),
        art.label(19).replace("|A", &crate::panels::num(last.0)).replace("|B", &crate::panels::num(last.1)),
    )
}

/// `GetEquipmentStatistic`.
fn equipment_statistic(art: &ManualArt, g: &Game, id: &str, k: usize) -> String {
    if let Some(p) = g.production(id) {
        let one = p.rate(1, 1, 0).food_per_day;
        let most = p.rate(p.max_combination, 1, 0).food_per_day;
        return art.label(1).replace("|A", &crate::panels::num(one)).replace("|B", &crate::panels::num(most));
    }
    if id == "item_hand_pump" || id == "item_industrial_pump" {
        return art.label(2).replace("|B", &crate::panels::num(g.water_output(2, id)));
    }
    let water = g.types.water_of(k);
    if water > 0 {
        return art.label(8).replace("|A", &crate::panels::num(water));
    }
    let defense = g.types.defense_of(k);
    if defense > 0 {
        return art.label(9).replace("|A", &crate::panels::num(defense));
    }
    if let Some(t) = g.types.get(k).filter(|t| !t.protection.is_empty()) {
        let gas = t.protection_against(burngfx::gamdat::Danger::Gas);
        let radiation = t.protection_against(burngfx::gamdat::Danger::Radiation);
        return art.label(3).replace("|A", &crate::panels::num(gas as i32)).replace("|B", &crate::panels::num(radiation as i32));
    }
    String::new()
}

/// `GetMaterialLine`: the parts joined with " + ", or "needs ammunition"
/// for a recipe any class may follow.
fn material_line(art: &ManualArt, g: &Game, id: &str) -> String {
    let Some(r) = g.recipe(id) else { return String::new() };
    let any = ["boss", "mercenary", "technician", "doctor"].iter().all(|c| r.allows(c));
    if any {
        return art.label(6);
    }
    let names: Vec<String> = r.items.iter().map(|i| g.kind(i).map_or_else(|| i.clone(), |k| (g.title)(k))).collect();
    art.label(4).replace("|M", &names.join(" + "))
}

/// `RenderCompactContextItem`.
#[allow(clippy::too_many_arguments)]
fn context_item(t: &mut Target, f: &Face, hud: &Hud, art: &ManualArt, g: &Game, id: &str, at: (i32, i32), available: i32) {
    let Some(k) = g.kind(id) else { return };
    let Some(ty) = g.types.get(k) else { return };
    let weapon = !ty.damage_table.is_empty();
    let (statistic, detail) = context_text(art, g, id, k);
    // `_detailFont` is `font-small` in MenuText; the normal detail is in
    // LightGray (weapons and pumps).
    let normal = weapon || id == "item_hand_pump" || id == "item_industrial_pump";
    let detail_ink = if normal { LIGHT_GRAY } else { MENU_TEXT };
    let Some(sprite) = g.picture(id) else { return };
    let title = (g.title)(k);
    let (stat_text, detail_text) = if weapon {
        let mut a = statistic.clone();
        a.push_str(" ~d1");
        let mut b = detail.clone();
        b.push_str(" ~d4");
        (a, b)
    } else {
        (statistic.clone(), detail.clone())
    };
    let text_w = coded_width(f, &title).max(coded_width(f, &stat_text).max(coded_width(f, &detail_text)));
    let block = sprite.width as i32 + 3 + text_w;
    let x = at.0 + ((available - block) / 2).max(0);
    t.blit(&sprite, x, at.1);
    let (tx, ty_) = (x + sprite.width as i32 + 3, at.1 + 1);
    let room = available - sprite.width as i32 - 3;
    let fitted_stat = if weapon {
        let mut s = fit(f, &statistic, room - coded_width(f, " ~d1"));
        s.push_str(" ~d1");
        s
    } else {
        fit(f, &statistic, room)
    };
    let fitted_detail = if weapon {
        let mut s = fit(f, &detail, room - coded_width(f, " ~d4"));
        s.push_str(" ~d4");
        s
    } else {
        fit(f, &detail, room)
    };
    let lh = f.line_height();
    draw_coded(t, f, hud, &fit(f, &title, room), tx, ty_, DIALOG_TEXT);
    draw_coded(t, f, hud, &fitted_stat, tx, ty_ + lh, LIGHT_GRAY);
    draw_coded(t, f, hud, &fitted_detail, tx, ty_ + lh + lh, detail_ink);
}
