//! Drawing the intro, `IntroScene`, from [`burngfx::intro`]'s state: the
//! page's 1993 map in its 288 × 160 view, the animation over it, the black
//! masks over both, and the scene blend over everything — in the start
//! screen's centred 320 × 200, as `Position = (Resolution.Game - 320x200) / 2`
//! puts it.

use alloc::vec::Vec;
use burngfx::input::{Action, Bindings};
use burngfx::intro::{self, Intro, PageMap, Script, MASKS, TILE, VIEW};
use burngfx::pack::{names, Pack, PackedImage, NAME_LEN};
use sdlmini::{scancode, Event};

use crate::gfx::Target;
use crate::play_keys;
use crate::start_view::image;

struct PageArt<'a> {
    map: PageMap<'a>,
    /// A tile's picture, row by row; `None` where the pack has none.
    tiles: Vec<Option<PackedImage<'a>>>,
}

pub struct IntroView<'a> {
    pub script: Script,
    /// Per page; `None` when its map is not packed, and the page is black.
    pages: Vec<Option<PageArt<'a>>>,
    /// Per animation, its frames from the first of its range.
    frames: Vec<Vec<Option<PackedImage<'a>>>>,
}

impl<'a> IntroView<'a> {
    /// `None` when the pack has no intro (`v12/intro`): the program then goes
    /// straight to the start screen, and says so.
    pub fn load(pack: &Pack<'a>) -> Option<IntroView<'a>> {
        let script = intro::parse(core::str::from_utf8(pack.get("v12/intro")?).ok()?)?;
        let pages = script
            .pages
            .iter()
            .map(|p| {
                let map = PageMap::parse(pack.get(&intro::map_key(p.map))?)?;
                let colours = intro::palette_of_map(p.map);
                let mut tiles = Vec::with_capacity(map.width * map.height);
                for y in 0..map.height {
                    for x in 0..map.width {
                        let (section, item) = map.tile(x, y);
                        // NewGfx: the remaster's tile where the table has one
                        // (`ClassicMapProcessor` asks for `burngfxtile@zei_…`).
                        let mut ng = [0u8; NAME_LEN];
                        let replaced = crate::gfx::newgfx()
                            .then(|| names::tile(&mut ng, section, item))
                            .flatten()
                            .and_then(|n| crate::start_view::newgfx_image(pack, core::str::from_utf8(&ng[..n]).ok()?));
                        let mut key = [0u8; NAME_LEN];
                        let n = names::map_tile(&mut key, colours, section, item)?;
                        tiles.push(replaced.or_else(|| image(pack, core::str::from_utf8(&key[..n]).ok()?)));
                    }
                }
                Some(PageArt { map, tiles })
            })
            .collect();
        let frames = script
            .animations
            .iter()
            .map(|a| (a.first..=a.last).map(|f| image(pack, &intro::frame_key(&a.stem, f))).collect())
            .collect();
        Some(IntroView { script, pages, frames })
    }

    /// What of the intro the pack lacks, for the console.
    pub fn missing(&self) -> usize {
        self.pages.iter().filter(|p| p.is_none()).count()
            + self.frames.iter().flatten().filter(|f| f.is_none()).count()
    }

    pub fn draw(&self, target: &mut Target, intro: &Intro) {
        let f = intro.frame();
        target.clear();
        target.centre(320, 200);
        // `MapView.OnRender`: the tiles in view, at the view's place plus the
        // scroll, unclipped.
        if let Some(Some(art)) = self.pages.get(f.page) {
            let offset = art.map.offset(f.scroll_x);
            let (x0, y0, x1, y1) = art.map.visible(offset);
            for ty in y0..y1 {
                for tx in x0..x1 {
                    if let Some(img) = &art.tiles[ty * art.map.width + tx] {
                        target.blit(img, VIEW.0 + tx as i32 * TILE + offset.0, VIEW.1 + ty as i32 * TILE + offset.1);
                    }
                }
            }
        }
        // The `Image` window, twenty layers up: the animation's frame at its
        // position in the scene, not over its first frame
        // (`Progressive = false`).
        if let Some((i, frame)) = f.animation {
            let a = &self.script.animations[i];
            if let Some(Some(img)) = self.frames[i].get((frame - a.first) as usize) {
                target.blit(img, a.pos.0, a.pos.1);
            }
        }
        // Fifty layers up, the masks.
        for (x, y, w, h) in MASKS {
            target.fill(x, y, w, h, 0, 255);
        }
        // `BlendOverlay.Render`: black over the whole screen at the blend's
        // state, `new Color(0, 0, 0, state)`.
        if f.black > 0.0 {
            target.origin = (0, 0);
            let (gw, gh) = target.game_size();
            target.fill(0, 0, gw + 1, gh + 1, 0, (f.black * 255.0) as u32);
        }
    }
}

/// Whether a key or a click leaves the intro.
///
/// Any click, either version (`OnMouseClick`). v1.2: a key bound to
/// Primary or Back (`OnInputAction`, `IntroScene.cs:141-150`) — the pad's
/// too. v0.9.1: any virtual key but F9 (`OnVKeyPress`; F8 never reaches the
/// scene) — its virtual keys are Escape, Pause, Enter and F1 to F4, every
/// other key being a character (`BurntimeGame.Window_TextInput`).
pub fn skips(e: &Event, legacy: bool, bindings: &Bindings) -> bool {
    match *e {
        Event::MouseDown { .. } => true,
        Event::KeyDown(k) if legacy => matches!(k, scancode::ESCAPE | scancode::RETURN | 88 | 72 | 58..=61),
        Event::KeyDown(k) => play_keys::key_of(k)
            .and_then(|key| bindings.key_action(key, false))
            .is_some_and(|a| matches!(a, Action::Primary | Action::Back)),
        Event::PadDown(b) if !legacy => play_keys::pad_of(play_keys::PAD + b as u32)
            .and_then(|p| bindings.pad_action(p))
            .is_some_and(|a| matches!(a, Action::Primary | Action::Back)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v1.2: a click, or a key bound to Primary (`accept = space enter`) or
    /// Back (`back = escape`); nothing else (`IntroScene.cs:135-150`).
    #[test]
    fn v12_leaves_on_a_click_primary_or_back() {
        let b = Bindings::defaults();
        let click = Event::MouseDown { x: 1, y: 1, button: sdlmini::BUTTON_RIGHT };
        assert!(skips(&click, false, &b));
        for k in [scancode::ESCAPE, scancode::RETURN, scancode::SPACE] {
            assert!(skips(&Event::KeyDown(k), false, &b), "{k}");
        }
        for k in [scancode::F9, scancode::N, scancode::UP] {
            assert!(!skips(&Event::KeyDown(k), false, &b), "{k}");
        }
    }

    /// v0.9.1: a click, or a virtual key but F9 — Escape, Pause, Enter,
    /// F1–F4 (`OnVKeyPress`; `BurntimeGame.Window_TextInput` makes every
    /// other key a character).
    #[test]
    fn v091_leaves_on_a_click_or_a_virtual_key() {
        let b = Bindings::defaults();
        assert!(skips(&Event::MouseDown { x: 1, y: 1, button: sdlmini::BUTTON_LEFT }, true, &b));
        for k in [scancode::ESCAPE, scancode::RETURN, 72, 58, 61] {
            assert!(skips(&Event::KeyDown(k), true, &b), "{k}");
        }
        for k in [scancode::F9, scancode::SPACE, scancode::N] {
            assert!(!skips(&Event::KeyDown(k), true, &b), "{k}");
        }
    }
}
