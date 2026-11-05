//! v1.2's in-game manual and the start screen's notes, as state and
//! arithmetic: `GUI/ManualWindow.cs` and `GUI/ManualTextLayout.cs`. The
//! program draws what this says.
//!
//! The window is 300 wide and as tall as the scene it is opened over,
//! centred across it (`ManualWindow` ctor, `CenterIn`). Five tabs — GOAL,
//! SURVIVE, PARTY, CAMPS, COMBAT, `manualui?0..4` — each one section of
//! `manual.txt` (`manual?s0..s4`); the notes have three, 93 wide. A tab's
//! text is wrapped into lines ([`wrap`]), the lines become entries
//! ([`entries`]), and an entry is drawn only when the whole of it is inside
//! the lines that show (`RenderTextPage`).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// `Size.x`.
pub const WIDTH: i32 = 300;
/// `TabsLeft`.
pub const TABS_LEFT: i32 = 10;
/// `FooterHeight`.
pub const FOOTER: i32 = 30;
/// The text area: `Rect(10, 24, Size.x - 20, ContentHeight)`.
pub const CONTENT_LEFT: i32 = 10;
pub const CONTENT_TOP: i32 = 24;
/// The pages' labels (`LabelsResource`, `manual?s5`).
pub const LABELS: usize = 5;
/// Held Up/Down: the first repeat after 0.3 s, then every 0.06 s.
pub const REPEAT_DELAY_MS: i32 = 300;
pub const REPEAT_MS: i32 = 60;

/// `GetStrings("manual?sN")`: a file's sections, which a line `}#` ends.
/// A trailing `}` on a line closes the section too, as the string tables
/// write it.
pub fn sections(text: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim_end_matches('\r');
        if line.trim_end() == "}#" {
            out.push(core::mem::take(&mut current));
            continue;
        }
        current.push(line.to_string());
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `ManualTextLayout.Wrap`: each line is a paragraph, wrapped to `width`.
/// Empty lines and `@` directives pass as they are; `#` headings are
/// measured in the heading font, `##` subheadings and text in the text
/// font, and their marker is kept on every line they wrap to.
pub fn wrap(paragraphs: &[String], width: i32, text: &dyn Fn(&str) -> i32, heading: &dyn Fn(&str) -> i32) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in paragraphs {
        if paragraph.is_empty() || paragraph.starts_with('@') {
            out.push(paragraph.clone());
            continue;
        }
        let is_heading = paragraph.starts_with('#');
        let sub = paragraph.starts_with("##");
        let prefix = if sub { "##" } else if is_heading { "#" } else { "" };
        let measure: &dyn Fn(&str) -> i32 = if is_heading && !sub { heading } else { text };
        let mut line = String::new();
        for word in paragraph[prefix.len()..].split(' ').filter(|w| !w.is_empty()) {
            let mut next = line.clone();
            if !next.is_empty() {
                next.push(' ');
            }
            next.push_str(word);
            if !line.is_empty() && measure(&next) > width {
                let mut done = String::from(prefix);
                done.push_str(&line);
                out.push(done);
                line = word.to_string();
            } else {
                line = next;
            }
        }
        let mut done = String::from(prefix);
        done.push_str(&line);
        out.push(done);
    }
    out
}

/// `ManualEntryKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Heading,
    Subheading,
    Tiers,
    ClassImage,
    SupplyExample,
    Goal,
    ContextItem,
    Production,
    Construction,
}

/// `ManualEntry`: what to draw, its payload (the line without its marker),
/// and how many text lines it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    pub payload: String,
    pub lines: i32,
}

/// What the entries need to know about the game they are shown in.
pub trait Content {
    /// `game.ItemTypes.Contains(id)`.
    fn has_item(&self, id: &str) -> bool;
    /// The height of the item's picture, `None` with no picture.
    fn item_height(&self, id: &str) -> Option<i32>;
    /// The height of `syssze.raw?32`, `?48`, `?16`.
    fn class_height(&self, sprite: u8) -> i32;
    /// `gfx/ui/manual_city.png`'s height.
    fn city_height(&self) -> i32;
    /// The active player's flag's height, uncropped.
    fn flag_height(&self) -> i32;
    /// The parts of the available recipe that makes `result`
    /// (`GetRecipes(game).FirstOrDefault`), `None` if there is none.
    fn recipe_items(&self, result: &str) -> Option<Vec<String>>;
}

/// The fonts' line heights the entries are counted in.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// `_textFont.LineHeight` (`font-small.txt`: 11).
    pub line: i32,
    /// `_titleFont.LineHeight`.
    pub title_line: i32,
}

impl Metrics {
    /// `PixelLineCount`: how many text lines a picture of this height takes.
    pub fn lines_for(self, height: i32) -> i32 {
        ((height + self.line - 1) / self.line).max(1)
    }
}

/// `GetClassSpriteId`: `@technician` 48, `@doctor` 16, else 32.
pub fn class_sprite(source: &str) -> u8 {
    if source.starts_with("@technician") {
        48
    } else if source.starts_with("@doctor") {
        16
    } else {
        32
    }
}

/// `ItemLineCount`: the tallest picture among the items the game has, and
/// never under three text lines.
fn item_lines<'a>(m: Metrics, c: &dyn Content, ids: impl Iterator<Item = &'a str>) -> i32 {
    let tallest = ids.filter(|id| c.has_item(id)).filter_map(|id| c.item_height(id)).max().unwrap_or(3 * m.line);
    m.lines_for((3 * m.line).max(tallest))
}

/// `GoalLineCount` for `@flag`: the flag less 1/36 above and 8/36 below.
pub fn flag_crop(width: i32, height: i32) -> (i32, i32, i32, i32) {
    // `MathF.Round`: halves to the even neighbour.
    let round = |n: i32, d: i32| {
        let (q, r) = (n / d, n % d);
        if 2 * r > d || (2 * r == d && q % 2 == 1) { q + 1 } else { q }
    };
    let left = round(width, 30).max(0);
    let top = round(height, 36).max(0);
    let right = round(width * 3, 30).max(0);
    let bottom = round(height * 8, 36).max(0);
    (left, top, right, bottom)
}

/// `ParseEntry`.
fn parse(line: &str, notes: bool, m: Metrics, c: &dyn Content) -> Option<Entry> {
    let entry = |kind, payload: &str, lines| Some(Entry { kind, payload: payload.to_string(), lines });
    if line == "@tiers" {
        return entry(Kind::Tiers, "", 1);
    }
    if matches!(line, "@mercenary" | "@technician" | "@doctor") {
        return entry(Kind::ClassImage, line, m.lines_for(c.class_height(class_sprite(line))));
    }
    if line == "@city" {
        return entry(Kind::Goal, line, m.lines_for(c.city_height()));
    }
    if line == "@flag" {
        let h = c.flag_height();
        let (_, top, _, bottom) = flag_crop(0, h);
        return entry(Kind::Goal, line, m.lines_for(h - top - bottom));
    }
    if let Some(id) = line.strip_prefix("@supply-example|") {
        return entry(Kind::SupplyExample, id, item_lines(m, c, core::iter::once(id)));
    }
    if let Some(rest) = line.strip_prefix("@entry|") {
        if !rest.split('|').any(|id| c.has_item(id)) {
            return None;
        }
        return entry(Kind::ContextItem, rest, item_lines(m, c, rest.split('|')));
    }
    if let Some(rest) = line.strip_prefix("@production|") {
        return entry(Kind::Production, rest, item_lines(m, c, rest.split('|').take(1)));
    }
    if let Some(result) = line.strip_prefix("@construction|") {
        let lines = match c.recipe_items(result) {
            Some(items) => item_lines(m, c, items.iter().map(String::as_str).chain(core::iter::once(result))),
            None => item_lines(m, c, core::iter::once(result)),
        };
        return entry(Kind::Construction, result, lines);
    }
    if let Some(rest) = line.strip_prefix("##") {
        return entry(Kind::Subheading, rest, 1);
    }
    if let Some(rest) = line.strip_prefix('#') {
        return entry(Kind::Heading, rest, m.lines_for((if notes { 1 } else { 2 }) * m.title_line));
    }
    entry(Kind::Text, line, 1)
}

/// `BuildEntries`: the wrapped lines of one tab as entries. Trailing empty
/// lines are dropped; `@extended-only-begin`…`-end` shows only under
/// Extended (never in the notes); in the notes, a heading drops the blank text between it and
/// a heading right before it.
pub fn entries(lines: &[String], extended: bool, notes: bool, m: Metrics, c: &dyn Content) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let mut skipping = false;
    let last = lines.iter().rposition(|l| !l.is_empty()).map_or(0, |i| i + 1);
    for line in &lines[..last] {
        if line == "@extended-only-begin" {
            skipping = notes || !extended;
            continue;
        }
        if line == "@extended-only-end" {
            skipping = false;
            continue;
        }
        if skipping {
            continue;
        }
        let Some(entry) = parse(line, notes, m, c) else { continue };
        if notes && entry.kind == Kind::Heading {
            let mut previous = out.len();
            while previous > 0 && out[previous - 1].kind == Kind::Text && out[previous - 1].payload.is_empty() {
                previous -= 1;
            }
            if previous > 0 && out[previous - 1].kind == Kind::Heading {
                out.truncate(previous);
            }
        }
        out.push(entry);
    }
    out
}

/// `ContentHeight`: the text area's height in a window `height` tall.
pub fn content_height(height: i32, line: i32) -> i32 {
    line.max(height - CONTENT_TOP - FOOTER)
}

/// `VisibleLineCount`: `font_height` is the taller of the two fonts'
/// `GetHeight()`.
pub fn visible_lines(height: i32, line: i32, font_height: i32) -> i32 {
    (1 + (content_height(height, line) - 2 - font_height) / line).max(1)
}

/// `SetupPatchNotes.Format`: a Markdown changelog as the notes' first page.
/// `# ` lines are dropped, `## ` becomes a heading without brackets, `### ` a
/// subheading, any other `#` line a heading; blank lines only between text.
pub fn patch_notes(markdown: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in markdown.split('\n') {
        let mut line = raw.trim().trim_start_matches('\u{feff}').to_string();
        if line.starts_with("# ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            let mut h = String::from("#");
            h.push_str(&rest.replace(['[', ']'], ""));
            out.push(h);
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            let mut h = String::from("##");
            h.push_str(rest);
            line = h;
        } else if line.starts_with('#') {
            let mut h = String::from("#");
            h.push_str(line.trim_start_matches(['#', ' ']));
            line = h;
        }
        if line.is_empty() && out.last().is_none_or(|l| l.is_empty() || l.starts_with('#')) {
            continue;
        }
        out.push(line);
    }
    out
}

/// What a press does to an open window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Back,
    Up,
    Down,
    Left,
    Right,
}

/// The window's state: open or not, the tab, each tab's scroll, the tab
/// under the pointer, and a held Up or Down.
#[derive(Clone, Debug)]
pub struct Window {
    pub open: bool,
    pub page: usize,
    pub hovered: Option<usize>,
    scroll: Vec<i32>,
    held: i32,
    repeat_ms: i32,
}

impl Window {
    /// The manual (five tabs) or the notes (three).
    pub fn new(pages: usize) -> Window {
        Window { open: false, page: 0, hovered: None, scroll: alloc::vec![0; pages], held: 0, repeat_ms: REPEAT_DELAY_MS }
    }

    pub fn pages(&self) -> usize {
        self.scroll.len()
    }

    /// `TabWidth`: 56 for the manual, 93 for the notes.
    pub fn tab_width(&self) -> i32 {
        if self.pages() == 3 { 93 } else { 56 }
    }

    /// `Open`: the first tab, every tab at its top.
    pub fn open(&mut self) {
        self.page = 0;
        self.scroll.iter_mut().for_each(|s| *s = 0);
        self.open = true;
        self.held = 0;
        self.repeat_ms = REPEAT_DELAY_MS;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.hovered = None;
    }

    /// `MovePage`: round the tabs.
    pub fn move_page(&mut self, by: i32) {
        let n = self.pages() as i32;
        self.page = ((self.page as i32 + by + n) % n) as usize;
    }

    /// `MoveTextScroll`: never above the top; the bottom is kept by
    /// [`Window::clamp`] when the page is laid out.
    pub fn scroll_by(&mut self, by: i32) {
        if let Some(s) = self.scroll.get_mut(self.page) {
            *s = (*s + by).max(0);
        }
    }

    pub fn scroll(&self) -> i32 {
        self.scroll.get(self.page).copied().unwrap_or(0)
    }

    /// `_textScroll[_page] = Clamp(…, 0, maximum)`, as each frame lays the
    /// page out.
    pub fn clamp(&mut self, maximum: i32) {
        if let Some(s) = self.scroll.get_mut(self.page) {
            *s = (*s).clamp(0, maximum.max(0));
        }
    }

    /// `OnInputAction`: Back closes; Up and Down scroll a line; Left (and
    /// the left shoulder) the tab before, Right (the right shoulder,
    /// Primary) the tab after. True when the window took it.
    pub fn press(&mut self, p: Press) {
        match p {
            Press::Back => self.close(),
            Press::Up => self.scroll_by(-1),
            Press::Down => self.scroll_by(1),
            Press::Left => self.move_page(-1),
            Press::Right => self.move_page(1),
        }
    }

    /// `OnHeldInputAction` and `OnUpdate`: `dir` is -1 while Up is held,
    /// 1 while Down is, 0 with neither; the first repeat after 0.3 s, then
    /// every 0.06 s. A change of direction starts the delay again.
    pub fn hold(&mut self, dir: i32, elapsed_ms: i32) {
        if dir == 0 || dir != self.held {
            self.repeat_ms = REPEAT_DELAY_MS;
        }
        self.held = dir;
        if dir == 0 {
            return;
        }
        self.repeat_ms -= elapsed_ms;
        while self.repeat_ms <= 0 {
            self.scroll_by(dir);
            self.repeat_ms += REPEAT_MS;
        }
    }

    /// `OnMouseWheel`: a line up for a turn away from the user, a line down
    /// for one towards; `over` is whether the pointer is over the window,
    /// the only place the wheel reaches it (`Window.MouseWheel`). True when
    /// the window took it.
    pub fn wheel(&mut self, over: bool, delta: i32) -> bool {
        if !over || delta == 0 {
            return false;
        }
        self.scroll_by(if delta > 0 { -1 } else { 1 });
        true
    }

    /// `GetTabAt`, in the window's own coordinates.
    pub fn tab_at(&self, x: i32, y: i32) -> Option<usize> {
        let w = self.tab_width();
        if !(2..22).contains(&y) || x < TABS_LEFT || x >= TABS_LEFT + w * self.pages() as i32 {
            return None;
        }
        Some(((x - TABS_LEFT) / w) as usize)
    }

    /// `OnMouseClick` with the left button, in the window's coordinates:
    /// on Close it closes; on the tab strip it picks that tab; anything
    /// else is swallowed. `close` is the Close button's rectangle.
    pub fn click(&mut self, x: i32, y: i32, close: (i32, i32, i32, i32)) {
        let (cx, cy, cw, ch) = close;
        if x >= cx && x < cx + cw && y >= cy && y < cy + ch {
            self.close();
            return;
        }
        if let Some(tab) = self.tab_at(x, y) {
            self.page = tab;
        }
    }
}
