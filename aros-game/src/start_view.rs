//! Drawing v0.9.1's start screen, `MenuScene`, from [`burngfx::start`]'s
//! state: the pictures and fonts it names, where it puts them, in the order
//! its layers stack them. The classic look — the NewGfx borders, cracks and
//! "PLAYER 1" captions are drawn only when that setting is on, and it is not
//! built yet.

use alloc::string::String;
use alloc::vec::Vec;
use burngfx::font::Font;
use burngfx::pack::{names, Pack, PackedImage, NAME_LEN};
use burngfx::start::{at, tooltip, Hover, StartScreen, Toggle};

use crate::gfx::{Align, Face, Target};

/// The name fields' ink, `PixelColor(184, 184, 184)`.
const NAME_INK: u32 = 0x00B8_B8B8;
/// The tooltips', `PixelColor(212, 212, 212)`.
const TIP_INK: u32 = 0x00D4_D4D4;

pub struct StartView<'a> {
    background: Option<PackedImage<'a>>,
    /// NewGfx's own pieces (`MenuScene.cs:105-115`): the five cracks, the
    /// four border sheets' cells, the buttons cell by cell.
    cracks: [Option<PackedImage<'a>>; 5],
    borders: [Option<PackedImage<'a>>; 4],
    ng_buttons: [Option<PackedImage<'a>>; 4],
    buttons: Option<PackedImage<'a>>,
    exit: Option<PackedImage<'a>>,
    exit_hover: Option<PackedImage<'a>>,
    /// v1.2's NOTES button: its 36-pixel `menu_exit.png` and hover.
    notes: Option<PackedImage<'a>>,
    notes_hover: Option<PackedImage<'a>>,
    /// v1.2's `tooltip.txt` by section, for the toggles' tooltip windows
    /// (`tooltip?s0..s11`).
    tips: Vec<Vec<String>>,
    copyright: Option<PackedImage<'a>>,
    /// Per toggle, per state: (picture, the `_down` picture shown on hover).
    toggles: [Vec<(Option<PackedImage<'a>>, Option<PackedImage<'a>>)>; 3],
    /// `sta.ani` frames 8 to 16.
    sta: Vec<Option<PackedImage<'a>>>,
    /// v1.3's blue and gray flags, `gfx/flag_blue.png` and
    /// `gfx/flag_black.png` (NewGfx: `start_flags.png` cells 3 and 2).
    flags: [Option<PackedImage<'a>>; 2],
    faces: Vec<Vec<(PackedImage<'a>, (i32, i32))>>,
    pub cursor: Option<PackedImage<'a>>,
    button_font: Option<Face<'a>>,
    /// `gfx/ui/start_font_player.txt`, for the captions on `scene/sta_en`.
    player_font: Option<Face<'a>>,
    /// Whether the background is the localised copy, which wants captions.
    localised: bool,
    /// `BurntimeClassic.FontName`: the high-resolution font, since
    /// `settings.txt` turns `highres_font` on; the plain one if it is missing.
    pub text_font: Option<Face<'a>>,
    newburn: Vec<&'a str>,
    /// v1.2's `newburn.txt`, for the toggles' tooltips (lines 105–110).
    newburn12: Vec<&'a str>,
    pub conversion: Vec<(String, String)>,
    /// The player hides v1.2's hints: the toggles' one-line tooltips
    /// (`ToolTipFont`) instead of their `TooltipWindow`s (`MenuScene.cs:687-692`).
    pub hide_hints: bool,
}

/// A picture by its classic key. In the NewGfx look, its replacement
/// (`ng/<key>`, at its own scale) where the pack has one; the classic
/// picture where it has none, as v1.2's `GetSprite` falls back
/// (`ResourceManager.cs:147-161`).
pub fn image<'a>(pack: &Pack<'a>, key: &str) -> Option<PackedImage<'a>> {
    if crate::gfx::newgfx() {
        if let Some(i) = newgfx_image(pack, key) {
            return Some(i);
        }
    }
    classic_image(pack, key)
}

/// The NewGfx replacement alone.
pub fn newgfx_image<'a>(pack: &Pack<'a>, key: &str) -> Option<PackedImage<'a>> {
    let mut k = String::from(names::NEWGFX_PREFIX);
    k.push_str(key);
    burngfx::pack::ScaledImage::parse(pack.get(&k)?).map(|s| s.image)
}

/// The 1993 (or v1.2 classic) picture, whatever the look.
pub fn classic_image<'a>(pack: &Pack<'a>, key: &str) -> Option<PackedImage<'a>> {
    PackedImage::parse(pack.get(key)?)
}

pub fn frame<'a>(pack: &Pack<'a>, stem: &str, n: u8) -> Option<PackedImage<'a>> {
    let mut key = [0u8; NAME_LEN];
    let len = names::frame(&mut key, stem, n)?;
    image(pack, core::str::from_utf8(&key[..len]).ok()?)
}

/// A font and its strip, with the font NewGfx swaps in for it (`font.txt`
/// and v0.9.1's `highres-font.txt` → `font-highres`, `font-small.txt` →
/// `font-small-highres`).
pub fn face<'a>(pack: &Pack<'a>, key: &str) -> Option<Face<'a>> {
    let font = Font::parse(core::str::from_utf8(pack.get(key)?).ok()?)?;
    let mut strip_key = String::from(key);
    strip_key.push_str(".strip");
    let ng = match key {
        "font/plain" | "font/hires" => scaled_face(pack, "ng/font/plain"),
        "font/small" => scaled_face(pack, "ng/font/small"),
        _ => None,
    };
    Some(Face { font, strip: classic_image(pack, &strip_key)?, ng })
}

/// A v1.2 font description and its atlas, as the packer keeps NewGfx's.
pub fn scaled_face<'a>(pack: &Pack<'a>, key: &str) -> Option<crate::gfx::scaled::ScaledFace<'a>> {
    let mut strip_key = String::from(key);
    strip_key.push_str(".strip");
    let atlas = PackedImage::parse(pack.get(&strip_key)?)?;
    let font = burngfx::font::ScaledFont::parse(core::str::from_utf8(pack.get(key)?).ok()?, atlas.height as i64)?;
    Some(crate::gfx::scaled::ScaledFace { font, atlas })
}

/// The text font (`FontName`): v1.2 draws `font.txt` in its classic look
/// (UI-40; NewGfx swaps it for `font-highres`), v0.9.1 `highres-font.txt`
/// (`settings.txt`'s `highres_font`). A legacy game, and the legacy start
/// screen, keep v0.9.1's.
pub fn text_face<'a>(pack: &Pack<'a>) -> Option<Face<'a>> {
    let (first, second) = if crate::legacy_look() { ("font/hires", "font/plain") } else { ("font/plain", "font/hires") };
    face(pack, first).or_else(|| face(pack, second))
}

/// `@newburn?N`: the lines of `newburn.txt`, counted from 0.
pub fn newburn_lines<'a>(pack: &Pack<'a>) -> Vec<&'a str> {
    pack.get("text/newburn.txt")
        .and_then(|b| core::str::from_utf8(b).ok())
        .map(|t| t.trim_start_matches('\u{feff}').lines().map(|l| l.trim_end_matches('\r')).collect())
        .unwrap_or_default()
}

impl<'a> StartView<'a> {
    pub fn load(pack: &Pack<'a>) -> StartView<'a> {
        // English is the only language here, so the localised picture is
        // taken when the pack has it, and the original otherwise.
        // NewGfx: `sta.pac` → `gfx/start.png`, whose captions are drawn
        // (`?43`/`?44`, as on the localised picture).
        let ng = if crate::gfx::newgfx() { newgfx_image(pack, "scene/sta") } else { None };
        let localised_bg = ng.or_else(|| image(pack, "scene/sta_en"));
        let localised = localised_bg.is_some();
        let ng_piece = |k: &str| if ng.is_some() { newgfx_image(pack, k) } else { None };
        let mut key = [0u8; NAME_LEN];
        let background = localised_bg.or_else(|| {
            names::scene_image(&mut key, "sta").and_then(|n| image(pack, core::str::from_utf8(&key[..n]).ok()?))
        });
        let pair = |a: &str, b: &str| (image(pack, a), image(pack, b));
        StartView {
            background,
            cracks: core::array::from_fn(|i| ng_piece(["n/crack1", "n/crack2", "n/crack3", "n/crack4", "n/crack5"][i])),
            borders: core::array::from_fn(|i| ng_piece(["n/borders/0", "n/borders/1", "n/borders/2", "n/borders/3"][i])),
            ng_buttons: core::array::from_fn(|i| ng_piece(["ui/start_buttons/0", "ui/start_buttons/1", "ui/start_buttons/2",
                                                           "ui/start_buttons/3"][i])),
            buttons: image(pack, "ui/start_buttons"),
            exit: image(pack, "ui/menu_exit"),
            exit_hover: image(pack, "ui/menu_exit_hover"),
            notes: image(pack, "ui/notes"),
            notes_hover: image(pack, "ui/notes_hover"),
            tips: burngfx::manual_window::sections(
                crate::localized(pack, "v12/tooltip").and_then(|b| core::str::from_utf8(b).ok()).unwrap_or(""),
            )
            .into_iter()
            .map(|section| section.into_iter().map(|l| { let l = l.trim_end(); String::from(l.strip_suffix('}').unwrap_or(l)) }).collect())
            .collect(),
            copyright: image(pack, "ui/start_maxdesign"),
            toggles: [
                alloc::vec![
                    pair("ui/level1", "ui/level1_down"),
                    pair("ui/level2", "ui/level2_down"),
                    pair("ui/level3", "ui/level3_down"),
                ],
                alloc::vec![pair("ui/remake", "ui/remake_down"), pair("ui/original", "ui/original_down")],
                alloc::vec![pair("ui/ai", "ui/ai_down"), pair("ui/amiga", "ui/amiga_down"), pair("ui/noai", "ui/noai_down")],
            ],
            sta: (8..17).map(|n| frame(pack, "sta", n)).collect(),
            flags: [image(pack, "f/flag_blue"), image(pack, "f/flag_black")],
            faces: (0..6u8).map(|f| crate::hud::face_frames(pack, f)).collect(),
            cursor: frame(pack, "munt", 0),
            button_font: face(pack, "font/start"),
            player_font: face(pack, "font/player"),
            localised,
            text_font: text_face(pack),
            newburn: newburn_lines(pack),
            newburn12: crate::localized(pack, "v12/newburn")
                .and_then(|b| core::str::from_utf8(b).ok())
                .map(|t| t.trim_start_matches('\u{feff}').lines().map(|l| l.trim_end_matches('\r')).collect())
                .unwrap_or_default(),
            conversion: pack
                .get("text/conversion")
                .and_then(|b| core::str::from_utf8(b).ok())
                .map(burngfx::start::parse_conversion_table)
                .unwrap_or_default(),
            hide_hints: false,
        }
    }

    /// What the pack is missing for this screen, so a gap is reported rather
    /// than drawn as nothing.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.background.is_none() { out.push("sta.pac"); }
        if self.buttons.is_none() { out.push("gfx/ui/start_buttons.png"); }
        if self.exit.is_none() { out.push("gfx/menu_exit.png"); }
        if self.button_font.is_none() { out.push("gfx/ui/start_font.txt"); }
        if self.text_font.is_none() { out.push("font.txt"); }
        if self.faces.iter().any(|f| f.is_empty()) { out.push("ges_NN.ani"); }
        if self.sta.iter().any(|f| f.is_none()) { out.push("sta.ani"); }
        if self.flags.iter().any(|f| f.is_none()) { out.push("gfx/flag_blue.png, gfx/flag_black.png"); }
        if self.newburn.is_empty() { out.push("newburn.txt"); }
        if self.newburn12.is_empty() { out.push("v1.2 newburn.txt"); }
        out
    }

    fn text(&self, n: u32) -> &str {
        self.newburn.get(n as usize).copied().unwrap_or("?")
    }

    /// Whether the name font has a character, for `NameWindow.OnKeyPress`.
    pub fn supports(&self, c: char) -> bool {
        self.text_font.as_ref().is_some_and(|f| f.supports(c))
    }

    /// `MenuScene.OnRender`'s NewGfx part (`MenuScene.cs:364-420`): over the
    /// game area clipped to the picture, a light strip along the top and
    /// left and a dark one along the bottom and right, the four border
    /// pieces in the corners, and the five cracks. `Vector2.Max` keeps the
    /// smaller of each axis and `Min(0)` the larger, as upstream's are named.
    fn draw_frame(&self, t: &mut Target, bg: &PackedImage) {
        let (gw, gh) = t.game_size();
        let size = (gw.min(bg.game_w()), gh.min(bg.game_h()));
        let off = (((gw - bg.game_w()) / 2).max(0) - t.origin.0, ((gh - bg.game_h()) / 2).max(0) - t.origin.1);
        const MARGIN: i32 = 3;
        const MARGINX: i32 = 4;
        // `PixelColor(255 * 2 / 10, 255, 255, 255)` and `(255 * 4 / 10, 0, 0, 0)`.
        let (light, dark) = ((0xFF_FFFF, 51), (0, 102));
        t.fill(off.0, off.1, size.0, MARGIN, light.0, light.1);
        t.fill(off.0, size.1 - MARGIN + off.1, size.0, MARGIN + 1, dark.0, dark.1);
        t.fill(off.0, off.1 + MARGIN, MARGINX, size.1 - MARGIN * 2, light.0, light.1);
        t.fill(off.0 + size.0 - MARGINX, off.1 + MARGIN, MARGINX + 1, size.1 - MARGIN * 2, dark.0, dark.1);
        let [tl, tr, bl, br] = &self.borders;
        if let Some(i) = tl {
            t.blit(i, off.0, off.1);
        }
        if let Some(i) = bl {
            t.blit(i, off.0, off.1 + size.1 - i.game_h() - 1);
        }
        if let Some(i) = tr {
            t.blit(i, off.0 + size.0 - i.game_w(), off.1);
        }
        if let Some(i) = br {
            t.blit(i, off.0 + size.0 - i.game_w(), off.1 + size.1 - i.game_h() - 1);
        }
        let [c1, c2, c3, c4, c5] = &self.cracks;
        if let Some(i) = c4 {
            t.blit(i, -15, -2);
        }
        if let Some(i) = c5 {
            t.blit(i, 204, 15);
        }
        if let Some(i) = c1 {
            t.blit(i, off.0, size.1 - i.game_h() + off.1);
        }
        if let Some(i) = c2 {
            t.blit(i, size.0 - i.game_w() + off.0, 92 + off.1);
        }
        if let Some(i) = c3 {
            t.blit(i, 230, size.1 - i.game_h() + off.1);
        }
    }

    pub fn draw(&self, t: &mut Target, s: &StartScreen) {
        t.clear();
        t.centre(320, 200);
        if let Some(bg) = &self.background {
            // `Size = 320 × 200`: centred on it — NewGfx's 546 × 341.
            t.blit(bg, (320 - bg.game_w()) / 2, (200 - bg.game_h()) / 2);
            if crate::gfx::newgfx() {
                self.draw_frame(t, bg);
            }
        }
        // The captions NewGfx draws, `_playerFont.DrawText((67, 9), @newburn?43,
        // Center, Top)` and `(255, 9)` for `?44`, here on the picture whose
        // German ones were taken out.
        if let (true, Some(f)) = (self.localised, &self.player_font) {
            f.draw(t, self.text(43), 67, 9, Align::Centre, Align::Start, None);
            f.draw(t, self.text(44), 255, 9, Align::Centre, Align::Start, None);
        }

        // The buttons: picture, then the caption centred on it.
        for (rect, hover, text) in [(at::START, Hover::Start, 41), (at::LOAD, Hover::Load, 40)] {
            let n = if s.hover == Some(hover) { 3 } else { 2 };
            if let Some(cell) = &self.ng_buttons[n as usize] {
                t.blit(cell, rect.x, rect.y);
            } else if let Some(sheet) = &self.buttons {
                t.blit_sheet(sheet, n, 64, 24, rect.x, rect.y);
            }
            if let Some(f) = &self.button_font {
                f.draw(t, self.text(text), rect.x + rect.w / 2, rect.y + rect.h / 2,
                       Align::Centre, Align::Centre, None);
            }
        }
        let exit = if s.hover == Some(Hover::Exit) { &self.exit_hover } else { &self.exit };
        if let Some(img) = exit {
            t.blit(img, at::EXIT.x, at::EXIT.y);
        }
        // v1.2's NOTES: the picture, "NOTES" centred on it in the buttons'
        // font.
        let notes = if s.hover == Some(Hover::Notes) { &self.notes_hover } else { &self.notes };
        if let Some(img) = notes.as_ref().or(self.notes.as_ref()) {
            t.blit(img, at::NOTES.x, at::NOTES.y);
            if let Some(f) = &self.button_font {
                f.draw(t, "NOTES", at::NOTES.x + at::NOTES.w / 2, at::NOTES.y + at::NOTES.h / 2, Align::Centre, Align::Centre, None);
            }
        }

        // The name fields: the pressed picture while the slot has focus, and
        // the label centred on the field's 104 × 24, not the picture's 112.
        for (i, slot) in s.slots.iter().enumerate() {
            let r = at::NAME[i];
            let n = if slot.focused { 1 } else { 0 };
            if let Some(cell) = &self.ng_buttons[n] {
                t.blit(cell, r.x, r.y);
            } else if let Some(sheet) = &self.buttons {
                t.blit_sheet(sheet, n as u32, 112, 24, r.x, r.y);
            }
            if let Some(f) = &self.text_font {
                f.draw(t, &slot.label(), r.x + r.w / 2, r.y + r.h / 2, Align::Centre, Align::Centre,
                       Some(NAME_INK));
            }
        }

        // The players' flags (v1.3's `Toggle`s, one picture a state, the
        // same hovered and pressed): `sta.ani?8` green, `?9` red,
        // `gfx/flag_blue.png`, `gfx/flag_black.png`.
        for i in 0..2 {
            use burngfx::player_colors::PlayerColor;
            let img = match s.colors[i] {
                PlayerColor::Green => self.sta.first().and_then(|f| f.as_ref()),
                PlayerColor::Red => self.sta.get(1).and_then(|f| f.as_ref()),
                PlayerColor::Blue => self.flags[0].as_ref(),
                PlayerColor::Black => self.flags[1].as_ref(),
            };
            if let Some(img) = img {
                t.blit(img, at::COLOUR[i].x, at::COLOUR[i].y);
            }
        }

        // The toggles, the `_down` picture under the pointer.
        let toggles = [(Toggle::Difficulty, at::DIFFICULTY, s.difficulty),
                       (Toggle::Mode, at::MODE, s.mode_state),
                       (Toggle::Ai, at::AI, s.ai_state)];
        for (n, (which, rect, state)) in toggles.iter().enumerate() {
            let Some((up, down)) = self.toggles[n].get(*state as usize) else { continue };
            let img = if s.hover == Some(Hover::Toggle(*which)) { down } else { up };
            if let Some(img) = img {
                t.blit(img, rect.x, rect.y);
            }
        }

        // Faces at layer + 5, the covers at + 10 over them. Both are
        // animations, and `SpriteAnimation.Progressive` is true unless said
        // otherwise, so the renderer puts the first frame down and the
        // current one over it: frames 1–39 of a face are only the flames
        // that move.
        for (i, slot) in s.slots.iter().enumerate() {
            if let Some(face) = slot.face_shown {
                if let Some(frames) = self.faces.get(face as usize) {
                    let (x, y) = (at::FACE[i].x, at::FACE[i].y);
                    if let Some((first, _)) = frames.first() {
                        t.blit(first, x, y);
                    }
                    let n = s.face_frame(frames.len() as u32) as usize;
                    if let Some((img, (dx, dy))) = frames.get(n).filter(|_| n > 0) {
                        t.blit(img, x + dx, y + dy);
                    }
                }
            }
            let (x, y) = at::SLIDE[i];
            if let Some(Some(first)) = self.sta.get(2) {
                t.blit(first, x, y);
            }
            let n = slot.slide_frame() as usize;
            if let Some(Some(img)) = self.sta.get(2 + n).filter(|_| n > 0) {
                t.blit(img, x, y);
            }
        }

        // `OnRender`, classic: the copyright line two layers up.
        if let Some(img) = &self.copyright {
            t.blit(img, at::COPYRIGHT.0, at::COPYRIGHT.1);
        }

        // A toggle's one-line tooltip, above it and kept on screen
        // (`TextBorders.Screen`): v1.2's with the hints off, here only when
        // the pack has no `tooltip.txt` for [`StartView::draw_tips`].
        if let (Some(Hover::Toggle(which)), Some(f), true) = (s.hover, &self.text_font, self.tips.len() < 12 || self.hide_hints) {
            let rect = match which {
                Toggle::Difficulty => at::DIFFICULTY,
                Toggle::Mode => at::MODE,
                Toggle::Ai => at::AI,
            };
            let tip = self.newburn12.get(tooltip(which, s) as usize).copied().unwrap_or("?");
            let (x, y) = f.place(tip, rect.x + rect.w / 2, rect.y - 2, Align::Centre, Align::End);
            let (gw, gh) = t.game_size();
            let (ox, oy) = t.origin;
            let lo = (2 - ox, 2 - oy);
            let hi = (gw - ox - f.width(tip) - 2, gh - oy - f.metric_height() - 2);
            let x = x.clamp(lo.0, hi.0.max(lo.0));
            let y = y.clamp(lo.1, hi.1.max(lo.1));
            f.draw_at(t, tip, x, y, Some(TIP_INK));
        }
    }

    /// v1.2's toggles' tooltip windows (`MenuScene.UpdateAiTooltip`,
    /// `UpdateGameModeTooltip`, `UpdateDifficultyTooltip`, `SetSetupTooltip`):
    /// while a toggle is under the pointer or selected at the keys, a
    /// `TooltipWindow` from `tooltip?sN` — its first line the header, its
    /// last the prompt, the rest the text. Difficulty: s9–s11, right edge
    /// at 132; mode: s4 (Extended) or s5 (Classic), centred on 161, at least
    /// 160 wide; opponent: s0–s2, left edge at 190; all with their bottom at
    /// 145 and at least 140 wide.
    pub fn draw_tips(&self, t: &mut Target, s: &StartScreen, hud: &crate::hud::Hud) {
        use burngfx::start::Setup;
        let keyed = |setup: Setup| s.setup_shown && s.setup == setup;
        let shown = |toggle: Toggle, setup: Setup| s.hover == Some(Hover::Toggle(toggle)) || keyed(setup);
        let mut windows: Vec<(usize, (i32, i32), Align, i32)> = Vec::new();
        let section = |toggle| burngfx::start::tooltip_section(toggle, s);
        if shown(Toggle::Ai, Setup::AiPlayers) {
            windows.push((section(Toggle::Ai), (190, 145), Align::Start, 140));
        }
        if shown(Toggle::Mode, Setup::GameMode) {
            windows.push((section(Toggle::Mode), (161, 145), Align::Centre, 160));
        }
        if shown(Toggle::Difficulty, Setup::Difficulty) {
            windows.push((section(Toggle::Difficulty), (132, 145), Align::End, 140));
        }
        let (gw, gh) = t.game_size();
        let bounds = (-t.origin.0, -t.origin.1, gw, gh);
        for (section, at, h, min_width) in windows {
            let Some(content) = self.tips.get(section).filter(|c| c.len() >= 3) else { continue };
            let lines: Vec<String> = content[1..content.len() - 1].iter().filter(|l| !l.is_empty()).cloned().collect();
            // `new InputPrompt(InputAction.Primary, content[^1]) { KeyboardControl = Enter }`.
            let prompt = [(
                content[content.len() - 1].clone(),
                burngfx::prompts::Prompt::action(burngfx::input::Action::Primary, burngfx::prompts::Label::Text(""))
                    .with_key(burngfx::prompts::Chord::plain(burngfx::input::Key::Enter)),
            )];
            let tip = crate::hud::TipWindow {
                header: &content[0],
                header_ink: crate::hud::HUD_TEXT_HOVER,
                lines: &lines,
                status: None,
                highlight: None,
                prompts: &prompt,
                min_width,
            };
            hud.draw_tip_window(t, &tip, at, h, Align::End, bounds);
        }
    }

    /// The pointer, `munt.raw` frame 0, drawn last.
    pub fn draw_cursor(&self, t: &mut Target, x: i32, y: i32) {
        if let Some(img) = &self.cursor {
            t.blit(img, x, y);
        }
    }
}
