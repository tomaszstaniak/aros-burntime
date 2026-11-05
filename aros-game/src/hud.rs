//! The frame around the map and what is drawn over it, as v0.9.1's
//! `MainUiOriginalWindow`, `MenuWindow` and `MapScene.OnRender` draw them:
//! the black border, `munt.raw` 1 and 22, the leader's face, the health and
//! time bars, the name, the status line, the day, the right-click menu, the
//! hover names and the cursors.
//!
//! Positions are the reference's, from the game resolution (`Size`), so the
//! frame follows the window as its does.

use alloc::string::String;
use alloc::vec::Vec;
use burngfx::mapscene::{Command, Menu, Status, MENU_LINE, MENU_TOP, MENU_W};
use burngfx::pack::{Pack, PackedImage};

use crate::gfx::{Align, Face, Target};
use crate::start_view::{face, frame, newburn_lines};

/// `new PixelColor(92, 92, 148)`: the day, and a danger's quiet line.
const DAY_INK: u32 = 0x005C_5C94;
/// `new PixelColor(252, 180, 56)`: the warnings.
const WARN_INK: u32 = 0x00FC_B438;
/// `new PixelColor(240, 64, 56)`: both bars.
const BAR: u32 = 0x00F0_4038;
/// `MenuWindow`'s ink and its hover.
const MENU_INK: u32 = 0x006C_74A8;
const MENU_HOVER: u32 = 0x00F0_4038;
/// `MapViewHoverInfo`'s colour, `(212, 212, 212)`.
pub const HOVER_INK: u32 = 0x00D4_D4D4;
/// `ClassicColors.MenuTextHover`, `(240, 64, 56)`: a tooltip's header.
const TOOLTIP_HEADER: u32 = 0x00F0_4038;
/// `ClassicColors.HudTextHover`, `(144, 160, 212)`: a `TooltipWindow`'s
/// header in its own palette.
pub const HUD_TEXT_HOVER: u32 = 0x0090_A0D4;
/// `ClassicColors.DialogText`, `(240, 164, 56)`: a tooltip's status.
const DIALOG_TEXT: u32 = 0x00F0_A438;

/// An item tooltip's footer (`ItemTooltipDetails`), resolved: the status
/// line and the prompt's label with its control.
pub struct TipDetails {
    pub status: Option<String>,
    pub prompt: Option<(String, burngfx::prompts::Control)>,
    /// NewGfx's primary, the transfer (`@prompts?37`/`?39`); the classic
    /// prompt is then the secondary.
    pub transfer: Option<(String, burngfx::prompts::Control)>,
}

/// What a [`Hud::draw_tip_window`] shows.
pub struct TipWindow<'s> {
    pub header: &'s str,
    pub header_ink: u32,
    pub lines: &'s [String],
    pub status: Option<&'s str>,
    /// v1.3's `TooltipWindow.HighlightedTextLine`: this line in the status
    /// font's colour.
    pub highlight: Option<usize>,
    /// The prompts, the primary first: each label and the prompt whose
    /// control is drawn after it (`Prompt`, `SecondaryPrompt`).
    pub prompts: &'s [(String, burngfx::prompts::Prompt)],
    pub min_width: i32,
}

/// A menu line's `InputShortcut`: the action, the keys it prefers at the
/// keyboard and with the mouse, the pad control, and the hold mark.
#[derive(Clone, Copy)]
struct Shortcut {
    action: burngfx::input::Action,
    key: Option<burngfx::input::Key>,
    mouse_key: Option<burngfx::input::Key>,
    pad: Option<burngfx::input::Pad>,
    hold: bool,
}

/// The shortcuts `MapScene.ConfigureMenu` (`MapScene.cs:243-268`) and
/// `LocationScene.ShowActionsMenu` (`LocationScene.cs:926-995`) give their
/// lines; MANUAL and the group lines have none.
fn shortcut(c: Command, inside: bool) -> Option<Shortcut> {
    use burngfx::input::{Action as A, Key, Pad};
    let plain = |action| Shortcut { action, key: None, mouse_key: None, pad: None, hold: false };
    Some(match c {
        Command::Travel | Command::Speak | Command::Fight => plain(A::ToggleInteractionMode),
        Command::Info if inside => Shortcut { pad: Some(Pad::DPadRight), ..plain(A::LocationInfo) },
        Command::Info => plain(A::ToggleInteractionMode),
        Command::Inventory => Shortcut { key: Some(Key::Char('e')), mouse_key: Some(Key::Char('i')), ..plain(A::Inventory) },
        Command::MainMap => Shortcut { key: Some(Key::Char('v')), mouse_key: Some(Key::Char('m')), ..plain(A::WorldMap) },
        Command::Statistics => plain(A::Statistics),
        Command::Options => plain(A::Options),
        Command::Next => Shortcut { hold: true, ..plain(A::NextTurn) },
        _ => return None,
    })
}

/// The animated sign beside the pointer: the map's travel or info
/// (`syst.raw` 24–27, 20–23), a location's speak or fight (`munt.raw` 10–13,
/// 14–17).
#[derive(Clone, Copy)]
pub enum ModeCursor {
    Syst(u8),
    Munt(u8),
}

/// Who is playing, as the start screen settled it.
pub struct Leader {
    pub name: String,
    pub face: i32,
    /// `Player.Color`, the name's ink.
    pub ink: u32,
}

pub struct Hud<'a> {
    munt: Vec<Option<PackedImage<'a>>>,
    syst: Vec<Option<PackedImage<'a>>>,
    /// All thirty faces: each frame, and where it goes over frame 0.
    faces: Vec<Vec<(PackedImage<'a>, (i32, i32))>>,
    pub font: Option<Face<'a>>,
    /// v1.2's `font-small.txt` (with v1.2's `font.png`): the Options pages.
    pub small: Option<Face<'a>>,
    newburn: Vec<&'a str>,
    /// v1.2's `gfx/ui/item_focus.png`, the frame on a grid's focused slot.
    pub item_focus: Option<PackedImage<'a>>,
    /// v1.2's font marks (`font-indicators.png`) and icons
    /// (`font-icons.png`), for the labels' bars.
    indicators: Option<PackedImage<'a>>,
    font_icons: Option<PackedImage<'a>>,
    /// NewGfx's `gfx/char_shadow.png` (`[newgfx_native]`), under each
    /// character on the map.
    pub shadow: Option<PackedImage<'a>>,
    /// NewGfx's animations by the id their scene asks for, where the
    /// replacement has frames of its own (`ng/a/<key>/<k>`): the flags'
    /// twelve, the mode cursors, the location's cursors.
    anims: Vec<(&'static str, Vec<PackedImage<'a>>)>,
    /// NewGfx's `player_icons.png`: the players' faces on the world map.
    pub player_icons: Option<PackedImage<'a>>,
    /// v1.2's input prompts: the glyphs, their words, the input mode.
    pub prompts: crate::prompt_view::Prompts<'a>,
}

/// `GuiTextBarType`: a row of `font-indicators.png` — its first frame, how
/// many, and the advance (`font.txt`'s `indicator_starts`, `_counts`,
/// `_widths`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    Blue,
    Red,
    Dots,
    People,
}

impl Bar {
    /// `font.txt`'s `indicator_codes=brdn`.
    pub fn from_code(c: char) -> Option<Bar> {
        Some(match c {
            'b' => Bar::Blue,
            'r' => Bar::Red,
            'd' => Bar::Dots,
            'n' => Bar::People,
            _ => return None,
        })
    }

    pub fn frames(self) -> (u32, i32, i32) {
        match self {
            Bar::Blue => (0, 15, 4),
            Bar::Red => (15, 15, 4),
            Bar::Dots => (30, 5, 2),
            Bar::People => (45, 5, 3),
        }
    }
}

/// `GuiTextBar`: a mark, its value (the frame, clamped), and a space after.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    pub bar: Bar,
    pub value: i32,
    pub gap_after: bool,
}

impl Mark {
    pub fn new(bar: Bar, value: i32) -> Mark {
        Mark { bar, value, gap_after: false }
    }
}

/// `FontIcons`: `font.txt`'s `icon_codes`, ⚠ ☢ ☠.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Warning,
    Radiation,
    Toxic,
}

/// `ges_NN.ani` as the packer cut it: frame 0 whole, the rest with the
/// offsets in `fo/ges_NN`.
pub fn face_frames<'a>(pack: &Pack<'a>, face: u8) -> Vec<(PackedImage<'a>, (i32, i32))> {
    let mut stem = String::from("ges_");
    stem.push((b'0' + face / 10) as char);
    stem.push((b'0' + face % 10) as char);
    // NewGfx: `ges_NN.ani` → `pngani@…/ges_NN.png`, one picture and no
    // index, so one frame (`AniProcessorPng`: `frameCount = 1`) — the
    // remaster's faces do not move.
    if crate::gfx::newgfx() {
        let mut ng = String::from("f/");
        ng.push_str(&stem);
        if let Some(img) = crate::start_view::newgfx_image(pack, &ng) {
            return alloc::vec![(img, (0, 0))];
        }
    }
    let mut key = String::from("fo/");
    key.push_str(&stem);
    let offsets = pack.get(&key).unwrap_or(&[]);
    (0..40u8)
        .filter_map(|n| {
            let img = frame(pack, &stem, n)?;
            let o = offsets.get(n as usize * 2..n as usize * 2 + 2).map_or((0, 0), |o| (o[0] as i32, o[1] as i32));
            Some((img, o))
        })
        .collect()
}

/// NewGfx's animation frames, `ng/a/<key>/<k>` (`packer::newgfx`).
fn newgfx_anims<'a>(pack: &Pack<'a>) -> Vec<(&'static str, Vec<PackedImage<'a>>)> {
    ["a/flag0", "a/flag1", "a/flag2", "a/flag3", "a/syst20", "a/syst24", "a/cursor", "a/fight"]
        .into_iter()
        .map(|key| {
            let frames = (0..16)
                .map_while(|k| crate::start_view::newgfx_image(pack, &alloc::format!("{key}/{k}")))
                .collect();
            (key, frames)
        })
        .collect()
}

/// What the frame shows this frame.
pub struct Showing<'s> {
    pub leader: &'s Leader,
    pub health: i32,
    /// `(int)(World.Time * 60)`.
    pub time_bar: i32,
    pub status: Option<Status>,
    /// The location's danger line (`Danger.InfoString`), for the two branches
    /// that show it.
    pub danger_line: Option<String>,
    pub ms: u32,
    /// v1.2's layout, in a v1.2 game.
    pub v12: Option<V12Hud>,
}

/// What v1.2's `MainUiOriginalWindow` adds: the selected character's place
/// in the group (`2/3`, when it is more than one), and the journey's days
/// to the place under the pointer or selected (`newburn?104`).
pub struct V12Hud {
    pub party: Option<(usize, usize)>,
    pub travel: Option<String>,
}

impl<'a> Hud<'a> {
    pub fn load(pack: &Pack<'a>) -> Hud<'a> {
        Hud {
            munt: (0..27).map(|n| frame(pack, "munt", n)).collect(),
            syst: (0..112).map(|n| frame(pack, "syst", n)).collect(),
            faces: (0..30u8).map(|f| face_frames(pack, f)).collect(),
            font: crate::start_view::text_face(pack),
            small: face(pack, "font/small"),
            newburn: newburn_lines(pack),
            item_focus: crate::start_view::image(pack, "ui/item_focus"),
            indicators: crate::start_view::image(pack, "ui/indicators"),
            font_icons: crate::start_view::image(pack, "ui/font_icons"),
            shadow: if crate::gfx::newgfx() { crate::start_view::newgfx_image(pack, "n/char_shadow") } else { None },
            anims: if crate::gfx::newgfx() { newgfx_anims(pack) } else { Vec::new() },
            player_icons: if crate::gfx::newgfx() { crate::start_view::newgfx_image(pack, "n/player_icons") } else { None },
            prompts: crate::prompt_view::Prompts::load(pack),
        }
    }

    /// Every picture and font read again, the prompts' state kept: the look
    /// changed (`Engine.ReloadGraphics`).
    pub fn reload_art(&mut self, pack: &Pack<'a>) {
        let fresh = Hud::load(pack);
        self.munt = fresh.munt;
        self.syst = fresh.syst;
        self.faces = fresh.faces;
        self.font = fresh.font;
        self.small = fresh.small;
        self.item_focus = fresh.item_focus;
        self.indicators = fresh.indicators;
        self.font_icons = fresh.font_icons;
        self.shadow = fresh.shadow;
        self.anims = fresh.anims;
        self.player_icons = fresh.player_icons;
    }

    /// `font-indicators.png`, 8 × 12 a frame (`~b`, `~r`, `~d`, `~n`).
    pub fn indicators(&self) -> Option<&PackedImage<'a>> {
        self.indicators.as_ref()
    }

    pub fn syst(&self, n: u8) -> Option<&PackedImage<'a>> {
        self.syst.get(n as usize).and_then(|f| f.as_ref())
    }

    /// A face, animated as every face is — progressive, five frames a
    /// second: frame 0, and the current frame over it where it belongs.
    pub fn draw_face(&self, t: &mut Target, face: usize, x: i32, y: i32, ms: u32) {
        let Some(frames) = self.faces.get(face) else { return };
        if let Some((first, _)) = frames.first() {
            t.blit(first, x, y);
        }
        let n = if frames.is_empty() { 0 } else { (ms * 5 / 1000) as usize % frames.len() };
        if n > 0 {
            if let Some((img, (dx, dy))) = frames.get(n) {
                t.blit(img, x + dx, y + dy);
            }
        }
    }

    /// A NewGfx animation's frames (empty in the classic look or where the
    /// pack has none).
    pub fn anim(&self, key: &str) -> &[PackedImage<'a>] {
        self.anims.iter().find(|(k, _)| *k == key).map_or(&[], |(_, f)| f.as_slice())
    }

    /// `burngfxani@syst.raw?F-F+3`'s frame now, `F` 0, 4, 8 or 12: the
    /// classic four frames, or NewGfx's (`flags.png`, twelve) on the same
    /// clock (`SpriteAnimation(12)` at 15).
    pub fn flag(&self, first: u8, ms: u32) -> Option<&PackedImage<'a>> {
        let ng = self.anim(["a/flag0", "a/flag1", "a/flag2", "a/flag3"][(first / 4).min(3) as usize]);
        if ng.is_empty() {
            return self.syst(first + burngfx::mapscene::flag_frame(ms));
        }
        ng.get(((ms * 15 / 1000) % 12) as usize % ng.len())
    }

    pub fn munt(&self, n: u8) -> Option<&PackedImage<'a>> {
        self.munt.get(n as usize).and_then(|f| f.as_ref())
    }

    /// `MainUiOriginalWindow.OnRender`, and its children: two `munt.raw`
    /// pictures and a `FaceWindow`.
    pub fn draw_frame(&self, t: &mut Target, s: &Showing, day_text: &str) {
        t.origin = (0, 0);
        let (w, h) = t.game_size();
        // The black border the map is framed in.
        t.fill(0, 0, 16, h - 40, 0, 255);
        t.fill(w - 16, 0, 17, h - 40, 0, 255);
        t.fill(0, h - 40, w + 1, 41, 0, 255);
        if let Some(img) = self.munt(1) {
            t.blit(img, w / 2 - 60, h - 40);
        }
        if let Some(img) = self.munt(22) {
            t.blit(img, w / 2 - 42, 0);
        }
        // The leader's face, a layer up.
        if let Ok(face) = usize::try_from(s.leader.face) {
            self.draw_face(t, face, w / 2 - 31, h - 56, s.ms);
        }
        // Health: 75 pixels for 100.
        t.fill(w / 2 + 64, h - 30, 75 * s.health.clamp(0, 100) / 100, 6, BAR, 255);
        // The day's clock, along the top piece.
        t.fill(w / 2 - 30, 2, s.time_bar.clamp(0, 60), 3, BAR, 255);
        if let (Some(f), Some(v)) = (&self.font, &s.v12) {
            // v1.2: the name and the line under it end 24 left of the face;
            // the line is the day when it has nothing else to say.
            let right = w / 2 - 31 - 24;
            f.draw(t, &s.leader.name, right, h - 30, Align::End, Align::Start, Some(s.leader.ink));
            if let Some((i, n)) = v.party {
                let mut at = String::new();
                at.push_str(&crate::panels::num(i as i32 + 1));
                at.push('/');
                at.push_str(&crate::panels::num(n as i32));
                let x = right - f.width(&s.leader.name) - f.width(" ");
                f.draw(t, &at, x, h - 30, Align::End, Align::Start, Some(DAY_INK));
            }
            if let Some(days) = &v.travel {
                f.draw(t, days, w / 2 + 61, h - 16, Align::Start, Align::Start, Some(DAY_INK));
            }
            let danger = s.danger_line.as_deref().unwrap_or("");
            let (line, ink) = match s.status {
                Some(Status::InDanger) => (danger, WARN_INK),
                Some(Status::DangerHere) => (danger, DAY_INK),
                Some(other) => (other.newburn().and_then(|n| self.newburn.get(n as usize).copied()).unwrap_or(""), WARN_INK),
                None => (day_text, DAY_INK),
            };
            f.draw(t, line, right, h - 16, Align::End, Align::Start, Some(ink));
        } else if let Some(f) = &self.font {
            f.draw(t, &s.leader.name, w / 2 - 97, h - 30, Align::Centre, Align::Start, Some(s.leader.ink));
            if let Some(status) = s.status {
                let danger = s.danger_line.as_deref().unwrap_or("");
                let (line, ink) = match status {
                    Status::InDanger => (danger, WARN_INK),
                    Status::DangerHere => (danger, DAY_INK),
                    other => (
                        other.newburn().and_then(|n| self.newburn.get(n as usize).copied()).unwrap_or(""),
                        WARN_INK,
                    ),
                };
                f.draw(t, line, w / 2 - 97, h - 17, Align::Centre, Align::Start, Some(ink));
            }
            f.draw(t, day_text, w / 2 + 100, h - 16, Align::Centre, Align::Start, Some(DAY_INK));
        }
    }

    /// `MenuWindow.OnRender`: `munt.raw` 24 on top, 25 per line, 26 under,
    /// each line's text centred on 34. In a v1.2 game, while hints are
    /// shown, each line's shortcut right of the menu on half-black (the hold
    /// mark before NEXT's), and the group menu's Secondary before its first
    /// line at the keys (`MenuWindow.cs:125-218`). `inside`: a place's menu
    /// (`LocationScene`), else the map's.
    pub fn draw_menu(&self, t: &mut Target, menu: &Menu, text: &dyn Fn(Command) -> String, inside: bool) {
        use burngfx::prompts::Mode;
        let Some((left, top)) = menu.at else { return };
        t.origin = (0, 0);
        if let Some(img) = self.munt(24) {
            t.blit(img, left, top);
        }
        let mode = self.prompts.shown_mode();
        let shortcuts: Vec<Option<Shortcut>> =
            menu.lines.iter().map(|&c| mode.and_then(|_| shortcut(c, inside))).collect();
        // `promptRight`: the menu's width, 2, and a hold glyph's room.
        let prompt_right = left + MENU_W + 2 + burngfx::prompts::GLYPH_W;
        if shortcuts.iter().any(Option::is_some) {
            let bx = left + MENU_W - 1;
            t.fill(bx, top + MENU_TOP, prompt_right - bx + 1, MENU_LINE * menu.lines.len() as i32 + 2, 0,
                   burngfx::prompts::BACKGROUND_ALPHA as u32);
        }
        // `FirstLineAction`: the group menu's Secondary, at the keys.
        let group = matches!(menu.lines.first(), Some(Command::Single | Command::All | Command::Dismiss | Command::StayThere | Command::Rejoin));
        let first_control = match mode {
            Some(m @ (Mode::Keyboard | Mode::Gamepad)) if group => {
                self.prompts.action_control(m, burngfx::input::Action::Secondary, None, None)
            }
            _ => Vec::new(),
        };
        let brackets = burngfx::prompts::Brackets { always: false, text_controls: true };
        for (i, &c) in menu.lines.iter().enumerate() {
            let y = top + MENU_TOP + MENU_LINE * i as i32;
            if let Some(img) = self.munt(25) {
                t.blit(img, left, y);
            }
            let Some(f) = &self.font else { continue };
            let line = text(c);
            let ink = if menu.hover == Some(i) { MENU_HOVER } else { MENU_INK };
            let mut x = left + MENU_W / 2 - f.width(&line) / 2;
            if i == 0 && !first_control.is_empty() {
                let w = burngfx::prompts::measure(f, brackets, &first_control, "", true);
                let combined = left + (MENU_W - (w + 2 + f.width(&line))) / 2;
                self.prompts.draw_label_first(t, f, brackets, &first_control, "", combined + w, y + 2, HUD_TEXT_HOVER, HUD_TEXT_HOVER, true);
                x = combined + w + 2;
            }
            if let (Some(s), Some(m)) = (shortcuts[i], mode) {
                // `shortcutInputMode`: the pad's at the pad, else the keys'.
                let (at, key) = if m == Mode::Gamepad { (Mode::Gamepad, s.key) } else { (Mode::Keyboard, if m == Mode::Mouse { s.mouse_key.or(s.key) } else { s.key }) };
                let control = self.prompts.action_control(at, s.action, key.map(burngfx::prompts::Chord::plain), s.pad);
                if !control.is_empty() {
                    if s.hold {
                        let w = burngfx::prompts::measure(f, brackets, &control, "", true);
                        let gy = y + 2 + (f.metric_height() - burngfx::prompts::GLYPH_H) / 2;
                        self.prompts.draw_hold(t, prompt_right - w - burngfx::prompts::GLYPH_W, gy, HUD_TEXT_HOVER);
                    }
                    self.prompts.draw_label_first(t, f, brackets, &control, "", prompt_right, y + 2, HUD_TEXT_HOVER, HUD_TEXT_HOVER, true);
                }
            }
            f.draw_at(t, &line, x, y + 2, Some(ink));
        }
        if let Some(img) = self.munt(26) {
            t.blit(img, left, top + MENU_TOP + MENU_LINE * menu.lines.len() as i32);
        }
    }

    /// `MapViewOverlayHoverText`: the name, centred both ways on the point.
    pub fn draw_hover(&self, t: &mut Target, title: &str, x: i32, y: i32) {
        t.origin = (0, 0);
        if let Some(f) = &self.font {
            f.draw(t, title, x, y, Align::Centre, Align::Centre, Some(HOVER_INK));
        }
    }

    /// `GuiTextBars.Draw`: the title, a space and its marks, one line
    /// centred both ways on the point (`Font.DrawInline`), after an icon if
    /// there is one, on half-black when `backing` (the world map's places).
    /// Without the pictures the marks are left out and the title stays.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_marked(&self, t: &mut Target, title: &str, x: i32, y: i32, ink: u32, marks: &[Mark],
                       icon: Option<Icon>, backing: bool) {
        t.origin = (0, 0);
        let Some(f) = &self.font else { return };
        // With the hints hidden, the name alone (`GuiTextBars.cs:41-46,65`).
        let hidden = self.prompts.hides_hints();
        let marks = if self.indicators.is_some() && !hidden { marks } else { &[] };
        let icon = icon.filter(|_| self.font_icons.is_some() && !hidden);
        let backing = backing && !hidden;
        let mut text = String::from(title);
        if !marks.is_empty() {
            text.push(' ');
        }
        let space = f.width(" ");
        let width = icon.map_or(0, |_| 13)
            + f.width(&text)
            + marks.iter().map(|m| m.bar.frames().2 + if m.gap_after { space } else { 0 }).sum::<i32>();
        let height = f.metric_height();
        let left = x - width / 2;
        let top = y - height / 2;
        if backing {
            // Padding: 2 left (none before an icon), 1 right, 1 above and below.
            let pad_left = if icon.is_some() { 0 } else { 2 };
            t.fill(left - pad_left, top - 1, width + pad_left + 1, height + 2, 0, 128);
        }
        let mut cursor = left;
        if let (Some(i), Some(sheet)) = (icon, &self.font_icons) {
            let frame = match i {
                Icon::Warning => 0,
                Icon::Radiation => 1,
                Icon::Toxic => 2,
            };
            t.blit_sheet(sheet, frame, 12, 12, cursor, top + (height - 12) / 2);
            cursor += 13;
        }
        f.draw_at(t, &text, cursor, top, Some(ink));
        cursor += f.width(&text);
        if let Some(sheet) = &self.indicators {
            for m in marks {
                let (first, count, advance) = m.bar.frames();
                let frame = first + m.value.clamp(0, count - 1) as u32;
                t.blit_sheet(sheet, frame, 8, 12, cursor, top + f.draw_offset());
                cursor += advance + if m.gap_after { space } else { 0 };
            }
        }
    }

    /// Indicator marks alone, left at `x` and centred on `y` as one line of
    /// the font (`Font.DrawText` of a `~` run, `VerticalTextAlignment.Center`).
    /// Nothing without the pictures; unlike [`Hud::draw_marked`]'s bars,
    /// hidden hints do not hide them (they are text, not `GuiTextBars`).
    pub fn draw_marks(&self, t: &mut Target, marks: &[Mark], x: i32, y: i32) {
        let (Some(f), Some(sheet)) = (&self.font, &self.indicators) else { return };
        let top = y - f.metric_height() / 2;
        let mut cursor = x;
        for m in marks {
            let (first, count, advance) = m.bar.frames();
            let frame = first + m.value.clamp(0, count - 1) as u32;
            t.blit_sheet(sheet, frame, 8, 12, cursor, top + f.draw_offset());
            cursor += advance;
        }
    }

    /// A hover name in an ink of its own (`MapViewHoverInfo.Color`).
    pub fn draw_label(&self, t: &mut Target, title: &str, x: i32, y: i32, ink: u32) {
        t.origin = (0, 0);
        if let Some(f) = &self.font {
            f.draw(t, title, x, y, Align::Centre, Align::Centre, Some(ink));
        }
    }

    /// v1.2's `TooltipWindow` in the classic look: half-black, the header
    /// in `MenuTextHover`, the lines in `LightGray`; 5 across and 4 down of
    /// padding, 10 a line, 4 between header and lines, at least 100 wide
    /// and "MM" wider; beside `edge` (right of it, or left), its top at `y`,
    /// kept inside the scene.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_tooltip(&self, t: &mut Target, header: &str, lines: &[String], edge: i32, y: i32, right: bool, scene: (i32, i32),
                        details: Option<&TipDetails>) {
        // A `TooltipWindow`: not with the hints hidden.
        if self.prompts.hides_hints() {
            return;
        }
        let Some(f) = &self.font else { return };
        let small = self.small.as_ref().unwrap_or(f);
        if crate::gfx::newgfx() {
            self.draw_tooltip_newgfx(t, f, small, header, lines, edge, y, right, scene, details);
            return;
        }
        // The footer (`ItemGridTooltip`'s details, `neutralPalette`: all
        // LightGray): the status right-aligned, the prompt under it,
        // label first, no brackets.
        let brackets = burngfx::prompts::Brackets { always: false, text_controls: false };
        let status = details.and_then(|d| d.status.as_deref());
        let prompt = details.and_then(|d| d.prompt.as_ref()).filter(|(_, c)| !c.is_empty());
        let status_w = status.map_or(0, |s| small.width(s));
        let prompt_w = prompt.map_or(0, |(l, c)| burngfx::prompts::measure(small, brackets, c, l, true));
        let footer = status.is_some() || prompt.is_some();
        let content = lines.iter().map(|l| f.width(l)).chain([f.width(header), status_w, prompt_w]).max().unwrap_or(0);
        let w = ((content + 10).max(100) + f.width("MM")).min(scene.0);
        let h = 4 + 4 + 10 + if lines.is_empty() { 0 } else { 4 + 10 * lines.len() as i32 }
            + if footer { 4 + status.map_or(0, |_| if prompt.is_some() { 10 } else { 8 }) + if prompt.is_some() { 8 } else { 0 } } else { 0 };
        let x = if right { edge } else { edge - w };
        let (x, y) = (x.clamp(0, (scene.0 - w).max(0)), y.clamp(0, (scene.1 - h).max(0)));
        t.fill(x, y, w, h, 0, 128);
        f.draw_at(t, header, x + 5, y + 4, Some(TOOLTIP_HEADER));
        for (i, l) in lines.iter().enumerate() {
            f.draw_at(t, l, x + 5, y + 4 + 10 + 4 + 10 * i as i32, Some(HOVER_INK));
        }
        let mut row = y + 4 + 10 + 4 + if lines.is_empty() { 0 } else { 10 * lines.len() as i32 + 4 };
        if let Some(s) = status {
            small.draw_at(t, s, x + w - 5 - status_w, row, Some(HOVER_INK));
            row += 10;
        }
        if let Some((label, control)) = prompt {
            self.prompts.draw_label_first(t, small, brackets, control, label, x + w - 5, row, HOVER_INK, 0x00FF_FFFF, false);
        }
    }

    /// [`Hud::draw_tooltip`] in NewGfx (`TooltipWindow.RefreshLayout` and
    /// `OnRender` with `IsNewGfx`, as `ItemGridTooltip` sets it:
    /// `StackPrompts`, `StatusReplacesSecondaryPrompt`): the text in
    /// `font-small`, advances by the fonts' `LineHeight`, 2 below and 2
    /// between sections, prompt rows `max(LineHeight, 10)` high with the
    /// text centred in them; the status takes the secondary prompt's row,
    /// under the primary; a row ending in a glyph keeps 2 from the edge,
    /// and the window is a pixel shorter when its last row is a glyph's.
    #[allow(clippy::too_many_arguments)]
    fn draw_tooltip_newgfx(&self, t: &mut Target, f: &Face, small: &Face, header: &str, lines: &[String], edge: i32, y: i32,
                           right: bool, scene: (i32, i32), details: Option<&TipDetails>) {
        use burngfx::prompts::{measure, Brackets, Part};
        let brackets = Brackets { always: false, text_controls: false };
        let (top, bottom, gap, pad) = (4, 2, 2, 5);
        let line_h = small.line_height();
        let row_h = line_h.max(burngfx::prompts::GLYPH_H);
        let text_offset = (row_h - small.metric_height()) / 2;
        let status = details.and_then(|d| d.status.as_deref()).filter(|s| !s.is_empty());
        let prompt = details.and_then(|d| d.transfer.as_ref()).filter(|(_, c)| !c.is_empty());
        // `VisibleSecondaryPrompt`: none while a status shows.
        let secondary = details.and_then(|d| d.prompt.as_ref()).filter(|(_, c)| !c.is_empty() && status.is_none());
        let width_of = |p: Option<&(String, burngfx::prompts::Control)>| p.map_or(0, |(l, c)| measure(small, brackets, c, l, true));
        let glyph_last = |p: Option<&(String, burngfx::prompts::Control)>| p.is_some_and(|(_, c)| c.last().is_some_and(Part::has_glyph));
        let (pw, sw) = (width_of(prompt), width_of(secondary));
        let has_prompts = pw > 0 || sw > 0;
        let footer = status.is_some() || has_prompts;
        let content = lines.iter().map(|l| small.width(l)).chain([f.width(header), status.map_or(0, |s| small.width(s)), pw.max(sw)])
            .max().unwrap_or(0);
        let mut rows = (pw > 0) as i32 + (sw > 0) as i32;
        if status.is_some() {
            rows = (pw > 0) as i32;
        }
        let sections = (!header.is_empty()) as i32 + (!lines.is_empty()) as i32 + footer as i32;
        let mut h = top + bottom + if header.is_empty() { 0 } else { f.line_height() } + line_h * lines.len() as i32
            + if status.is_some() { row_h } else { 0 } + rows * row_h + (sections - 1).max(0) * gap;
        if status.is_some() || (if sw > 0 { glyph_last(secondary) } else { glyph_last(prompt) || glyph_last(secondary) }) {
            h -= 1;
        }
        let w = ((content + 2 * pad).max(100) + small.width("MM")).min(scene.0);
        let x = if right { edge } else { edge - w };
        let (x, y) = (x.clamp(0, (scene.0 - w).max(0)), y.clamp(0, (scene.1 - h).max(0)));
        t.fill(x, y, w, h, 0, 128);
        let mut row = y + top;
        if !header.is_empty() {
            f.draw_at(t, header, x + pad, row, Some(TOOLTIP_HEADER));
            row += f.line_height();
            if !lines.is_empty() || footer {
                row += gap;
            }
        }
        for l in lines {
            small.draw_at(t, l, x + pad, row, Some(HOVER_INK));
            row += line_h;
        }
        if !lines.is_empty() && footer {
            row += gap;
        }
        if let Some(s) = status {
            let sy = row + text_offset + if pw > 0 { row_h } else { 0 };
            small.draw_at(t, s, x + w - pad - small.width(s), sy, Some(HOVER_INK));
        }
        if !has_prompts {
            return;
        }
        row += text_offset;
        let edge_of = |p: Option<&(String, burngfx::prompts::Control)>| x + w - if glyph_last(p) { bottom } else { pad };
        let mut r = edge_of(if sw > 0 { secondary } else { prompt });
        if let Some((label, control)) = secondary {
            let sy = row + if pw > 0 { row_h } else { 0 };
            self.prompts.draw_label_first(t, small, brackets, control, label, r, sy, HOVER_INK, 0x00FF_FFFF, false);
            r = edge_of(prompt);
        }
        if let Some((label, control)) = prompt {
            self.prompts.draw_label_first(t, small, brackets, control, label, r, row, HOVER_INK, 0x00FF_FFFF, false);
        }
    }

    /// v1.2's `TooltipWindow` in the classic look, with its own palette
    /// (`neutralPalette: false`): the header in `HudTextHover`, the lines in
    /// `font-small` LightGray, a status right-aligned in `DialogText`, and
    /// the prompts' row right-aligned (secondary rightmost, 8 apart), each
    /// its label and its key or button glyph (`InputControlLabelRenderer`
    /// with no brackets). 5 across and 4 down of padding, 10 a line, 8 for
    /// the prompts' row, 4 between sections; at least `tip.min_width` wide.
    /// `pos` is the edge or centre that `h` and `v` name
    /// (`HorizontalAlignment`, `VerticalAlignment`), and the window is kept
    /// inside `bounds` (`MoveInsideScreen`). Not drawn while the hints are
    /// hidden (`TooltipWindow.cs:137`).
    pub fn draw_tip_window(&self, t: &mut Target, tip: &TipWindow, pos: (i32, i32), h: Align, v: Align,
                           bounds: (i32, i32, i32, i32)) {
        if self.prompts.hides_hints() {
            return;
        }
        let Some(f) = self.small.as_ref().or(self.font.as_ref()) else { return };
        // NewGfx's metrics (`TooltipWindow.cs:15-23`): 2 between sections
        // and below, the fonts' `LineHeight` as advances, prompt rows
        // `max(LineHeight, 10)` with the text centred, the header in the
        // main font (`HeaderFont = FontName`).
        let ng = crate::gfx::newgfx();
        let head = if ng { self.font.as_ref().unwrap_or(f) } else { f };
        let (gap, bottom) = if ng { (2, 2) } else { (4, 4) };
        let line_h = if ng { f.line_height() } else { 10 };
        let header_h = if ng { head.line_height() } else { 10 };
        let row_h = if ng { f.line_height().max(burngfx::prompts::GLYPH_H) } else { 8 };
        let text_offset = if ng { (row_h - f.metric_height()) / 2 } else { 0 };
        let brackets = burngfx::prompts::Brackets { always: false, text_controls: false };
        // `MeasurePrompt`: no control, no prompt.
        let shown: Vec<(&str, burngfx::prompts::Control, i32)> = tip
            .prompts
            .iter()
            .map(|(label, p)| {
                let control = self.prompts.mode.get().map_or(Vec::new(), |m| self.prompts.control(m, p, burngfx::prompts::PREFER_SPACE));
                let w = if control.is_empty() { 0 } else { burngfx::prompts::measure(f, brackets, &control, label, true) };
                (label.as_str(), control, w)
            })
            .collect();
        let widths: Vec<i32> = shown.iter().map(|s| s.2).filter(|&w| w > 0).collect();
        let footer_w = widths.iter().sum::<i32>() + if widths.len() > 1 { 2 * gap } else { 0 };
        let status_w = tip.status.map_or(0, |s| f.width(s));
        let content = tip.lines.iter().map(|l| f.width(l)).chain([head.width(tip.header), status_w, footer_w]).max().unwrap_or(0);
        let has_header = !tip.header.is_empty();
        let has_text = !tip.lines.is_empty();
        let has_prompts = !widths.is_empty();
        let has_footer = tip.status.is_some() || has_prompts;
        let sections = has_header as i32 + has_text as i32 + has_footer as i32;
        let glyph_last = |c: &burngfx::prompts::Control| c.last().is_some_and(burngfx::prompts::Part::has_glyph);
        let mut height = 4 + bottom + if has_header { header_h } else { 0 } + line_h * tip.lines.len() as i32
            + tip.status.map_or(0, |_| if ng { f.line_height() } else if has_prompts { 10 } else { 8 })
            + if has_prompts { row_h } else { 0 }
            + (sections - 1).max(0) * gap;
        if ng && shown.iter().filter(|s| s.2 > 0).any(|s| glyph_last(&s.1)) {
            height -= 1;
        }
        let width = tip.min_width.max(content + 10);
        let x = match h {
            Align::Start => pos.0,
            Align::Centre => pos.0 - width / 2,
            Align::End => pos.0 - width,
        };
        let y = match v {
            Align::Start => pos.1,
            Align::Centre => pos.1 - height / 2,
            Align::End => pos.1 - height,
        };
        let (bx, by, bw, bh) = bounds;
        let x = x.min(bx + bw - width).max(bx);
        let y = y.min(by + bh - height).max(by);
        t.fill(x, y, width, height, 0, 128);
        let mut row = y + 4;
        if has_header {
            head.draw_at(t, tip.header, x + 5, row, Some(tip.header_ink));
            row += header_h;
            if has_text || has_footer {
                row += gap;
            }
        }
        for (i, l) in tip.lines.iter().enumerate() {
            let ink = if tip.highlight == Some(i) { DIALOG_TEXT } else { HOVER_INK };
            f.draw_at(t, l, x + 5, row, Some(ink));
            row += line_h;
        }
        if has_text && has_footer {
            row += gap;
        }
        if let Some(status) = tip.status {
            f.draw_at(t, status, x + width - 5 - status_w, row, Some(DIALOG_TEXT));
            row += if ng { f.line_height() } else { 10 };
        }
        row += text_offset;
        // The secondary at the right edge, the primary left of it; a glyph
        // at the edge keeps 2 from it in NewGfx (`PromptRightEdge`).
        let edge = |c: &burngfx::prompts::Control| x + width - if ng && glyph_last(c) { bottom } else { 5 };
        let mut right = shown.iter().rev().find(|s| s.2 > 0).map_or(x + width - 5, |s| edge(&s.1));
        for (label, control, w) in shown.iter().rev().filter(|s| s.2 > 0) {
            self.prompts.draw_label_first(t, f, brackets, control, label, right, row, HOVER_INK, 0x00FF_FFFF, false);
            right -= w + 2 * gap;
        }
    }

    /// `MapScene.OnRender`: the mode cursor at the pointer plus (8, 11), then
    /// `munt.raw` 0 at the pointer. Both a layer under the frame, so the
    /// frame covers them.
    pub fn draw_cursor(&self, t: &mut Target, x: i32, y: i32, mode: Option<ModeCursor>) {
        t.origin = (0, 0);
        // NewGfx: the animation's own frames, the same step of it
        // (`n - first` modulo its length; a two-frame one alternates).
        let ng = |key: &str, step: u8| {
            let f = self.anim(key);
            (!f.is_empty()).then(|| &f[step as usize % f.len()])
        };
        let img = match mode {
            Some(ModeCursor::Syst(n @ 20..=23)) => ng("a/syst20", n - 20).or_else(|| self.syst(n)),
            Some(ModeCursor::Syst(n @ 24..=27)) => ng("a/syst24", n - 24).or_else(|| self.syst(n)),
            Some(ModeCursor::Syst(n)) => self.syst(n),
            Some(ModeCursor::Munt(n @ 10..=13)) => ng("a/cursor", n - 10).or_else(|| self.munt(n)),
            Some(ModeCursor::Munt(n @ 14..=17)) => ng("a/fight", n - 14).or_else(|| self.munt(n)),
            Some(ModeCursor::Munt(n)) => self.munt(n),
            None => None,
        };
        if let Some(img) = img {
            t.blit(img, x + 8, y + 11);
        }
        if let Some(img) = self.munt(0) {
            t.blit(img, x, y);
        }
    }

    pub fn menu_text_width(&self, line: &str) -> i32 {
        self.font.as_ref().map_or(0, |f| f.width(line))
    }
}
