//! Drawing the way v0.9.1 draws, with its four separate quantities kept
//! separate (`Burntime.Platform.Utils.Resolution`):
//!
//! * **Ratio correction**, 0.9375 × 1.125 — the remaster's 4:3 correction of
//!   the 1993 8:5 pixels, `RatioCorrection` in `Application.cs`. The same in
//!   both looks.
//! * **The doubling**, 2 (`DOUBLED_RESOLUTION`). Together with the correction
//!   it is the *scale*, 1.875 × 2.25: backbuffer pixels per game pixel.
//! * **The output scale**, in half steps, chosen from the window (`Native`)
//!   as v1.2 chooses it (`Resolution.cs:65-85`, [`scaled::v12_resolution`]):
//!   the smaller axis' `floor(native / 600 × 450 × 2) / 2`, at least 1. The
//!   backbuffer is the window divided by it, and SDL stretches it back up:
//!   640 × 480 is 1× (B7's window), 1280 × 720 is 1.5× and an 853 × 480
//!   backbuffer. The window is the client area as it is now — resized, or
//!   fullscreen at the desktop's size — and a change of it makes all of
//!   this again ([`Target::geometry_for`], UI-42). (v0.9.1 took a whole
//!   number, [`window_factor`].)
//! * **The game resolution**, the backbuffer divided by the scale. Scenes lay
//!   themselves out in it: the menu centres its 320 × 200, the map fills it.
//!
//! What varies per picture is a fifth thing, **its own resolution**: a 1993
//! sprite has one pixel per game pixel, `highres-font.txt` two (its
//! `factor=0.5`), a NewGfx replacement its `sprite_scale`
//! ([`PackedImage::scale`]). [`Target::blit_part`] takes the first two as
//! `k`; in the NewGfx look ([`newgfx`]) every draw goes through
//! [`scaled`] instead, at the picture's scale times `k`.
//!
//! The mouse goes the other way through the same numbers
//! ([`Target::window_to_scene`]), so a click lands where the picture is at any
//! window size. Everything is nearest-neighbour: the reference samples with
//! `SamplerState.PointClamp`.

use alloc::vec;
use alloc::vec::Vec;
use burngfx::font::Font;
use burngfx::pack::PackedImage;

/// The scale as fractions, `DOUBLED_RESOLUTION * RatioCorrection`:
/// 2 × 60/64 = 15/8 across, 2 × 72/64 = 9/4 down.
const SX_NUM: i64 = 15;
const SX_DEN: i64 = 8;
const SY_NUM: i64 = 9;
const SY_DEN: i64 = 4;
/// `BurntimeClassic.MinResolution` and `MaxResolution`.
pub const MIN_GAME: (i64, i64) = (320, 200);
pub const MAX_GAME: (i64, i64) = (680, 320);

/// v0.9.1's `SelectBestGameResolution`: the whole-number factor between the
/// window and the backbuffer. v1.2 replaced it with half steps
/// ([`scaled::v12_resolution`], UI-41); kept for the record and its test.
#[allow(dead_code)]
pub fn window_factor(native: (usize, usize)) -> usize {
    let (nx, ny) = (native.0 as i64, native.1 as i64);
    // `native / (resolution * scale)`, floored, per axis, for both bounds.
    let fit = |n: i64, res: i64, num: i64, den: i64| n * den / (res * num);
    let max_f = (fit(nx, MAX_GAME.0, SX_NUM, SX_DEN), fit(ny, MAX_GAME.1, SY_NUM, SY_DEN));
    let min_f = (fit(nx, MIN_GAME.0, SX_NUM, SX_DEN), fit(ny, MIN_GAME.1, SY_NUM, SY_DEN));
    let vertical = 1.max(max_f.1).max(min_f.1);
    let horizontal = 1.max(max_f.0).max(min_f.0);
    vertical.min(horizontal) as usize
}

/// Which look draws: v1.2's `IsNewGfx` (`Application.cs:171`, the `newgfx`
/// key of `user.txt`, default on; F8 and the Settings line switch it). One
/// switch for the whole program, as upstream's is the application's: every
/// draw of a frame goes through the same path, and every picture is read
/// again when it changes.
static NEWGFX: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub fn newgfx() -> bool {
    NEWGFX.load(core::sync::atomic::Ordering::Relaxed)
}

pub fn set_newgfx(on: bool) {
    NEWGFX.store(on, core::sync::atomic::Ordering::Relaxed);
}

pub struct Target {
    pub px: Vec<u32>,
    /// The backbuffer, in its own pixels.
    pub w: usize,
    pub h: usize,
    /// Window pixels per backbuffer pixel, in halves: v1.2's `OutputScale`
    /// (`Resolution.cs:65-85`), 2 for 1×, 3 for 1.5×.
    pub halves: usize,
    /// Where game coordinate (0, 0) of what is being drawn sits, in game
    /// coordinates: a 320 × 200 scene is centred by setting this.
    pub origin: (i32, i32),
    /// What is drawn is drawn at this opacity too, 0–255 (`alpha` in
    /// v1.2's `GuiTextBars.Draw`); 255 but for the moment that needs it.
    pub alpha: u32,
    /// The NewGfx draws keep inside this backbuffer rectangle, `(x0, y0,
    /// x1, y1)`, when set: the map view's sprites stay in the view.
    pub clip: Option<(i64, i64, i64, i64)>,
    /// The NewGfx pictures as drawn, kept: [`scaled::Cache`].
    pub cache: scaled::Cache,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Centre,
    End,
}

impl Target {
    /// For a window of this size.
    pub fn new(window_w: usize, window_h: usize) -> Target {
        let mut t = Target::geometry(window_w, window_h);
        t.px = vec![0xFF00_0000; t.w * t.h];
        t
    }

    /// Only the geometry, for turning a scripted scene coordinate into the
    /// window pixel a real click there would have. v1.2's half steps
    /// (UI-41): 640 × 480 is 1× and a 640 × 480 backbuffer, as before;
    /// 1280 × 720 is 1.5× and 853 × 480.
    pub fn geometry(window_w: usize, window_h: usize) -> Target {
        let r = scaled::v12_resolution((window_w as i64, window_h as i64));
        Target {
            px: Vec::new(),
            w: r.back_buffer.0.max(1) as usize,
            h: r.back_buffer.1.max(1) as usize,
            halves: r.output_halves as usize,
            origin: (0, 0),
            alpha: 255,
            clip: None,
            cache: scaled::Cache::new(),
        }
    }

    /// What a window of this size would make of the backbuffer — its size
    /// and the output scale, `(w, h, halves)` — when that is not what this
    /// target already is. `None` for a window with no area, which is how
    /// some systems report a minimised one: v1.2 would compute a zero
    /// backbuffer from it, and nothing is drawn into a minimised window
    /// anyway, so the geometry stays until the window has a size again.
    pub fn geometry_for(&self, window_w: usize, window_h: usize) -> Option<(usize, usize, usize)> {
        if window_w == 0 || window_h == 0 {
            return None;
        }
        let g = Target::geometry(window_w, window_h);
        Some((g.w, g.h, g.halves)).filter(|&n| n != (self.w, self.h, self.halves))
    }

    /// Become the backbuffer [`Target::geometry_for`] described: v1.2's new
    /// `MainTarget` after `ResizeScene` (`BurntimeGame.cs:362-364`). The
    /// pixels start black. The scaled pictures are kept: a picture's columns
    /// and rows follow from the scale and its position, and no window
    /// changes the scale (`Resolution.Scale` is the doubling times the
    /// ratio correction), only how much of the game fits.
    pub fn set_geometry(&mut self, (w, h, halves): (usize, usize, usize)) {
        self.w = w;
        self.h = h;
        self.halves = halves;
        self.px = vec![0xFF00_0000; w * h];
    }

    /// Where a click at `(x, y)` of a `size` scene centred in the game (as
    /// [`Target::centre`] places it) lands in the window, whatever the
    /// origin is now.
    pub fn centred_to_window(&self, size: (i32, i32), x: i32, y: i32) -> (i32, i32) {
        let (gw, gh) = self.game_size();
        let at = ((gw - size.0) / 2, (gh - size.1) / 2);
        self.scene_to_window(x + at.0 - self.origin.0, y + at.1 - self.origin.1)
    }

    /// The output scale as text, `1` or `1.5`, for the logs that say it.
    pub fn factor_text(&self) -> alloc::string::String {
        if self.halves % 2 == 0 {
            alloc::format!("{}", self.halves / 2)
        } else {
            alloc::format!("{}.5", self.halves / 2)
        }
    }

    /// A window pixel to scene coordinates, for the mouse.
    pub fn window_to_scene(&self, x: i32, y: i32) -> (i32, i32) {
        let f = self.halves as i32;
        self.to_scene((x * 2).div_euclid(f), (y * 2).div_euclid(f))
    }

    /// Scene coordinates to the window pixel in the middle of that scene
    /// pixel.
    pub fn scene_to_window(&self, x: i32, y: i32) -> (i32, i32) {
        let (dx, dy) = self.to_device(x, y);
        let f = self.halves as i32;
        ((2 * dx + 1) * f / 4, (2 * dy + 1) * f / 4)
    }

    /// `Resolution.Game`: the backbuffer divided by the scale, made whole
    /// as `Vector2` does, `(int)(v + 0.5f)` (341 × 213 at 640 × 480 either
    /// way; 455, not 454, across an 853 backbuffer).
    pub fn game_size(&self) -> (i32, i32) {
        (
            ((self.w as i64 * SX_DEN * 2 + SX_NUM) / (2 * SX_NUM)) as i32,
            ((self.h as i64 * SY_DEN * 2 + SY_NUM) / (2 * SY_NUM)) as i32,
        )
    }

    pub fn clear(&mut self) {
        self.px.fill(0xFF00_0000);
    }

    /// Centre a scene of this size, as `MenuScene` does:
    /// `Position = (Resolution.Game - Size) / 2`.
    pub fn centre(&mut self, w: i32, h: i32) {
        let (gw, gh) = self.game_size();
        self.origin = ((gw - w) / 2, (gh - h) / 2);
    }

    /// The middle of a scene pixel, in backbuffer pixels.
    pub fn to_device(&self, x: i32, y: i32) -> (i32, i32) {
        let x0 = self.dev_x(x as i64, 1);
        let x1 = self.dev_x(x as i64 + 1, 1);
        let y0 = self.dev_y(y as i64, 1);
        let y1 = self.dev_y(y as i64 + 1, 1);
        (((x0 + x1) / 2) as i32, ((y0 + y1) / 2) as i32)
    }

    /// Everything outside the scene rectangle `(x, y, w, h)` put back as
    /// it was in `saved` (this target's pixels, taken before): what was
    /// drawn since is kept inside the rectangle only — a clip after the
    /// fact, as `RenderTarget.GetSubBuffer` clips what is drawn into it.
    pub fn keep_inside(&mut self, saved: &[u32], x: i32, y: i32, w: i32, h: i32) {
        let ((x0, x1), (y0, y1)) = if newgfx() {
            let (a, b) = self.ng_rect(x, y, w, h);
            ((a.0.max(0), a.1.min(self.w as i64)), (b.0.max(0), b.1.min(self.h as i64)))
        } else {
            ((self.dev_x(x as i64, 1).max(0), self.dev_x((x + w) as i64, 1).min(self.w as i64)),
             (self.dev_y(y as i64, 1).max(0), self.dev_y((y + h) as i64, 1).min(self.h as i64)))
        };
        for row in 0..self.h as i64 {
            let line = row as usize * self.w;
            if row < y0 || row >= y1 {
                self.px[line..line + self.w].copy_from_slice(&saved[line..line + self.w]);
            } else {
                let (a, b) = (x0.clamp(0, self.w as i64) as usize, x1.clamp(0, self.w as i64) as usize);
                self.px[line..line + a].copy_from_slice(&saved[line..line + a]);
                if b > a {
                    self.px[line + b..line + self.w].copy_from_slice(&saved[line + b..line + self.w]);
                }
            }
        }
    }

    /// A backbuffer pixel back to scene coordinates.
    pub fn to_scene(&self, dx: i32, dy: i32) -> (i32, i32) {
        (
            (dx as i64 * SX_DEN / SX_NUM) as i32 - self.origin.0,
            (dy as i64 * SY_DEN / SY_NUM) as i32 - self.origin.1,
        )
    }

    /// Device x of a position given in `1/k` scene pixels.
    fn dev_x(&self, v: i64, k: i64) -> i64 {
        ((v + self.origin.0 as i64 * k) * SX_NUM).div_euclid(SX_DEN * k)
    }

    fn dev_y(&self, v: i64, k: i64) -> i64 {
        ((v + self.origin.1 as i64 * k) * SY_NUM).div_euclid(SY_DEN * k)
    }

    #[inline]
    fn put(&mut self, x: i64, y: i64, r: u32, g: u32, b: u32, a: u32) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 || a == 0 {
            return;
        }
        let at = y as usize * self.w + x as usize;
        if a >= 255 {
            self.px[at] = 0xFF00_0000 | r << 16 | g << 8 | b;
        } else {
            let d = self.px[at];
            let mix = |s: u32, d: u32| (s * a + d * (255 - a)) / 255;
            self.px[at] = 0xFF00_0000
                | mix(r, d >> 16 & 0xFF) << 16
                | mix(g, d >> 8 & 0xFF) << 8
                | mix(b, d & 0xFF);
        }
    }

    /// Copy part of an image: `src` is `(x, y, w, h)` in image pixels, the
    /// image has `k` pixels per scene pixel, and its top left goes at scene
    /// `(x, y)`. `tint` recolours as `FontProcessorTxt.MixColor` does: the
    /// colour times the source's red channel, keeping its alpha.
    pub fn blit_part(&mut self, img: &PackedImage, src: (u32, u32, u32, u32), k: u32,
                     x: i32, y: i32, tint: Option<u32>) {
        let (sx, sy, sw, sh) = src;
        if sw == 0 || sh == 0 {
            return;
        }
        if newgfx() {
            // The picture's own resolution times `k`, through v1.2's one
            // transform.
            use burngfx::ratio::{Pair, Ratio};
            let k = Ratio::int(k as i64);
            let scale = Pair::new(img.scale.x * k, img.scale.y * k);
            self.ng_draw(img, src, scale, Ratio::int(x as i64), Ratio::int(y as i64), tint);
            return;
        }
        let k = k as i64;
        let (x0, x1) = (self.dev_x(x as i64 * k, k), self.dev_x(x as i64 * k + sw as i64, k));
        let (y0, y1) = (self.dev_y(y as i64 * k, k), self.dev_y(y as i64 * k + sh as i64, k));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        // Clip to the backbuffer before walking, so a big picture half off
        // screen costs only what shows.
        let (cx0, cx1) = (x0.max(0), x1.min(self.w as i64));
        let (cy0, cy1) = (y0.max(0), y1.min(self.h as i64));
        for dy in cy0..cy1 {
            let py = sy + ((dy - y0) * sh as i64 / (y1 - y0)) as u32;
            for dx in cx0..cx1 {
                let px = sx + ((dx - x0) * sw as i64 / (x1 - x0)) as u32;
                let Some(c) = img.pixel(px, py) else { continue };
                let a = if self.alpha == 255 { c[3] as u32 } else { c[3] as u32 * self.alpha / 255 };
                if a == 0 {
                    continue;
                }
                match tint {
                    None => self.put(dx, dy, c[0] as u32, c[1] as u32, c[2] as u32, a),
                    Some(t) => {
                        let f = c[0] as u32;
                        self.put(dx, dy, (t >> 16 & 0xFF) * f / 255, (t >> 8 & 0xFF) * f / 255,
                                 (t & 0xFF) * f / 255, a);
                    }
                }
            }
        }
    }

    /// The backbuffer rectangle `[x0, x1) × [y0, y1)` a scene rectangle
    /// covers in the classic look, as [`Target::blit_pixels`] maps it.
    pub fn classic_rect(&self, x: i32, y: i32, w: i32, h: i32) -> (i64, i64, i64, i64) {
        (self.dev_x(x as i64, 1), self.dev_y(y as i64, 1), self.dev_x((x + w) as i64, 1), self.dev_y((y + h) as i64, 1))
    }

    /// A whole picture with its top left at scene `(xn / den, yn / den)`: a
    /// character drawn where his path says he is to the fraction of a pixel
    /// (v1.3's `RenderPosition`, `DrawSpriteF`). NewGfx draws it through
    /// v1.2's one transform at that point; the classic look maps the same
    /// fraction onto the backbuffer as [`Target::blit_pixels`] maps whole
    /// pixels, so at one game pixel a device pixel it is where it always was.
    /// Kept inside the clip.
    pub fn blit_sub(&mut self, img: &PackedImage, xn: i64, yn: i64, den: i64) {
        let (w, h) = (img.width, img.height);
        if w == 0 || h == 0 || den <= 0 {
            return;
        }
        if newgfx() {
            use burngfx::ratio::Ratio;
            self.ng_draw(img, (0, 0, w, h), img.scale, Ratio::new(xn, den), Ratio::new(yn, den), None);
            return;
        }
        let (x0, x1) = (self.dev_x(xn, den), self.dev_x(xn + w as i64 * den, den));
        let (y0, y1) = (self.dev_y(yn, den), self.dev_y(yn + h as i64 * den, den));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (bx0, by0, bx1, by1) = self.bounds();
        let (cx0, cx1) = (x0.max(bx0), x1.min(bx1));
        let (cy0, cy1) = (y0.max(by0), y1.min(by1));
        for dy in cy0..cy1 {
            let py = ((dy - y0) * h as i64 / (y1 - y0)) as u32;
            for dx in cx0..cx1 {
                let px = ((dx - x0) * w as i64 / (x1 - x0)) as u32;
                let Some(c) = img.pixel(px, py) else { continue };
                let a = if self.alpha == 255 { c[3] as u32 } else { c[3] as u32 * self.alpha / 255 };
                if a == 0 {
                    continue;
                }
                self.put(dx, dy, c[0] as u32, c[1] as u32, c[2] as u32, a);
            }
        }
    }

    /// A buffer drawn at one pixel per game pixel — the map view, which the
    /// old renderer still draws — with its top left at scene `(x, y)`.
    pub fn blit_pixels(&mut self, px: &[u32], w: usize, h: usize, x: i32, y: i32) {
        if w == 0 || h == 0 || px.len() < w * h {
            return;
        }
        if newgfx() {
            self.ng_pixels(px, w, h, x, y, false);
            return;
        }
        let (x0, x1) = (self.dev_x(x as i64, 1), self.dev_x((x + w as i32) as i64, 1));
        let (y0, y1) = (self.dev_y(y as i64, 1), self.dev_y((y + h as i32) as i64, 1));
        let (cx0, cx1) = (x0.max(0), x1.min(self.w as i64));
        let (cy0, cy1) = (y0.max(0), y1.min(self.h as i64));
        for dy in cy0..cy1 {
            let sy = ((dy - y0) * h as i64 / (y1 - y0)) as usize;
            let row = &px[sy * w..sy * w + w];
            let out = dy as usize * self.w;
            for dx in cx0..cx1 {
                let sx = ((dx - x0) * w as i64 / (x1 - x0)) as usize;
                self.px[out + dx as usize] = row[sx] | 0xFF00_0000;
            }
        }
    }

    /// A whole 1993-resolution picture at scene `(x, y)`.
    pub fn blit(&mut self, img: &PackedImage, x: i32, y: i32) {
        self.blit_part(img, (0, 0, img.width, img.height), 1, x, y, None);
    }

    /// One frame of a `pngsheet@…?frame?WxH` sheet: frames run left to
    /// right, then down, `WxH` each.
    pub fn blit_sheet(&mut self, img: &PackedImage, frame: u32, fw: u32, fh: u32, x: i32, y: i32) {
        // A replacement's cells are its scale times the classic ones.
        let fw = (burngfx::ratio::Ratio::int(fw as i64) * img.scale.x).floor().max(1) as u32;
        let fh = (burngfx::ratio::Ratio::int(fh as i64) * img.scale.y).floor().max(1) as u32;
        let per_row = (img.width / fw).max(1);
        let (col, row) = (frame % per_row, frame / per_row);
        self.blit_part(img, (col * fw, row * fh, fw, fh), 1, x, y, None);
    }

    /// Part of a picture at the backbuffer's own resolution, one image pixel
    /// a backbuffer pixel, its top left where scene pixel `(x, y)` begins:
    /// v1.2's input glyphs, which keep their 22-pixel cells whatever the
    /// ratio correction (`GlyphResolution = 1 / (1.875, 2.25)`). `tint`
    /// multiplies each channel, as a sprite's colour does.
    pub fn blit_native(&mut self, img: &PackedImage, src: (u32, u32, u32, u32), x: i32, y: i32, tint: Option<u32>) {
        self.blit_glyph(img, src, x, y, false, tint);
    }

    /// [`Target::blit_native`], and in NewGfx `half` a game pixel lower:
    /// `DrawGlyph`'s `_glyphVerticalOffset` (0.5 by default, 0 in a
    /// tooltip and a small-font row; `InputControlLabelRenderer.cs:147-151`).
    pub fn blit_glyph(&mut self, img: &PackedImage, src: (u32, u32, u32, u32), x: i32, y: i32, half: bool,
                      tint: Option<u32>) {
        let (sx, sy, sw, sh) = src;
        // NewGfx: where the game position falls under the transform.
        let (x0, y0) = if newgfx() {
            let (a, _) = self.ng_rect(x, y, 1, 1);
            let d = scaled::v12_device_scale().y;
            let gy = burngfx::ratio::Ratio::int((y + self.origin.1) as i64) + burngfx::ratio::Ratio::new(half as i64, 2);
            (a.0, (gy * d - burngfx::ratio::Ratio::new(1, 2)).ceil())
        } else {
            (self.dev_x(x as i64, 1), self.dev_y(y as i64, 1))
        };
        for row in 0..sh as i64 {
            for col in 0..sw as i64 {
                let Some(c) = img.pixel(sx + col as u32, sy + row as u32) else { continue };
                let a = c[3] as u32 * self.alpha / 255;
                let (mut r, mut g, mut b) = (c[0] as u32, c[1] as u32, c[2] as u32);
                if let Some(t) = tint {
                    r = r * (t >> 16 & 0xFF) / 255;
                    g = g * (t >> 8 & 0xFF) / 255;
                    b = b * (t & 0xFF) / 255;
                }
                self.put(x0 + col, y0 + row, r, g, b, a);
            }
        }
    }

    /// A filled rectangle in scene coordinates, `a` 0–255.
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32, a: u32) {
        if newgfx() {
            self.ng_fill(x, y, w, h, rgb, a);
            return;
        }
        let (x0, x1) = (self.dev_x(x as i64, 1), self.dev_x((x + w) as i64, 1));
        let (y0, y1) = (self.dev_y(y as i64, 1), self.dev_y((y + h) as i64, 1));
        for dy in y0.max(0)..y1.min(self.h as i64) {
            for dx in x0.max(0)..x1.min(self.w as i64) {
                self.put(dx, dy, rgb >> 16 & 0xFF, rgb >> 8 & 0xFF, rgb & 0xFF, a * self.alpha / 255);
            }
        }
    }
}

/// A bitmap font and its strip — and, where the pack has one, the font
/// NewGfx swaps it for (`newgfx.txt`'s `[noscale]`: `font.txt` →
/// `font-highres.txt`, `font-small.txt` → `font-small-highres.txt`), which
/// measures and draws while the look is NewGfx, as v1.2's `ReloadGraphics`
/// makes every `Font` read the replacement.
pub struct Face<'a> {
    pub font: Font,
    pub strip: PackedImage<'a>,
    pub ng: Option<scaled::ScaledFace<'a>>,
}

impl Face<'_> {
    /// The NewGfx font, while that look is on.
    fn swapped(&self) -> Option<&scaled::ScaledFace<'_>> {
        if newgfx() { self.ng.as_ref() } else { None }
    }

    pub fn width(&self, s: &str) -> i32 {
        match self.swapped() {
            Some(n) => n.font.width(s) as i32,
            None => self.font.measure(s) as i32,
        }
    }

    /// `Font.GetHeight()`.
    pub fn metric_height(&self) -> i32 {
        match self.swapped() {
            Some(n) => n.font.metric_height() as i32,
            None => self.font.metric_height(),
        }
    }

    /// `Font.LineHeight`.
    pub fn line_height(&self) -> i32 {
        match self.swapped() {
            Some(n) => n.font.line_height() as i32,
            None => self.font.line_height(),
        }
    }

    /// Where the glyphs sit below the line's top.
    pub fn draw_offset(&self) -> i32 {
        match self.swapped() {
            Some(n) => n.font.offset as i32,
            None => self.font.draw_offset(),
        }
    }

    pub fn supports(&self, c: char) -> bool {
        match self.swapped() {
            Some(n) => n.font.supports(c),
            None => self.font.supports(c),
        }
    }

    /// `Font.DrawText`: `colour` None draws the strip's own colours
    /// (`PixelColor.Transparent`), otherwise the strip is tinted. Returns the
    /// top left the text was laid out from, after alignment.
    pub fn draw(&self, t: &mut Target, s: &str, x: i32, y: i32, h: Align, v: Align,
                colour: Option<u32>) -> (i32, i32) {
        if let Some(n) = self.swapped() {
            let (x, y) = n.draw(t, s, x, y, h, v, colour);
            return (x.floor() as i32, y.floor() as i32);
        }
        let (x, y) = self.place(s, x, y, h, v);
        self.draw_at(t, s, x, y, colour);
        (x, y)
    }

    /// Where aligned text starts. Vertical centring halves
    /// [`Font::metric_height`], as `DrawText` halves `GetHeight()`.
    pub fn place(&self, s: &str, x: i32, y: i32, h: Align, v: Align) -> (i32, i32) {
        let x = match h {
            Align::Start => x,
            Align::Centre => x - self.width(s) / 2,
            Align::End => x - self.width(s),
        };
        let y = match v {
            Align::Start => y,
            Align::Centre => y - self.metric_height() / 2,
            Align::End => y - self.metric_height(),
        };
        (x, y)
    }

    pub fn draw_at(&self, t: &mut Target, s: &str, x: i32, y: i32, colour: Option<u32>) {
        if let Some(n) = self.swapped() {
            n.draw(t, s, x, y, Align::Start, Align::Start, colour);
            return;
        }
        let k = self.font.scale;
        let rows = self.font.height * k;
        for g in self.font.layout(s) {
            t.blit_part(&self.strip, (g.src_x, 0, g.src_width, rows), k,
                        x + g.dst_x, y + self.font.draw_offset(), colour);
        }
    }
}

/// The prompts measure with whichever font draws.
impl burngfx::prompts::Metrics for Face<'_> {
    fn width(&self, s: &str) -> i32 {
        Face::width(self, s)
    }

    fn height(&self) -> i32 {
        self.metric_height()
    }

    fn line_height(&self) -> i32 {
        Face::line_height(self)
    }
}

// -- v1.2's NewGfx draw ------------------------------------------------------
//
// In NewGfx v1.2 draws every sprite straight into the backbuffer through one
// transform, `Matrix.CreateScale(1.875 + 0.0001, 2.25 + 0.0001)`
// (`RenderDevice.Render`, `RenderDevice.cs:363-381`; the target is
// `Resolution.BackBuffer`, `:651-667`), each sprite at its game position and
// scaled by its own factor, `1 / sprite_scale`. A 1993 sprite (factor 1)
// comes out in 1.875 × 2.25 blocks, a replacement drawn at 60 × 72 per tile
// lands one to one. The GPU then fills the pixels whose centres lie inside
// the quad (left and top edges in, right and bottom out) and point-samples
// the texel under each centre (`SamplerState.PointClamp`), or, for a font
// drawn smaller than its atlas, interpolates (`LinearClamp`, `:416-418`).
//
// [`Target::blit_scaled`] does that in exact fractions. It is not
// [`Target::blit_part`]'s arithmetic: that one floors each scene pixel's
// edge (v0.9.1's integer path), so a 1993 sprite's first column is 1 pixel
// wide there and 2 here. A NewGfx frame draws everything, classic sprites
// included, through this one.

pub mod scaled {
    use super::*;
    use alloc::collections::BTreeMap;
    use burngfx::ratio::{Pair, Ratio};

    /// `Resolution.Scale`, `2 * RatioCorrection`: 15/8 × 9/4.
    pub fn v12_scale() -> Pair {
        Pair::new(Ratio::new(SX_NUM, SX_DEN), Ratio::new(SY_NUM, SY_DEN))
    }

    /// The NewGfx transform's scale, `Resolution.Scale + PIXEL_CORRECTION`
    /// (`RenderDevice.cs:363,375-377`): the 0.0001 keeps a sprite edge off a
    /// pixel centre, so it is part of where edges fall, not noise.
    pub fn v12_device_scale() -> Pair {
        let e = Ratio::new(1, 10_000);
        let s = v12_scale();
        Pair::new(s.x + e, s.y + e)
    }

    /// v1.2's `Resolution.SelectBestGameResolution` (`Resolution.cs:65-85`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct V12Resolution {
        /// `OutputScale` in halves: 2 is 1×, 3 is 1.5×.
        pub output_halves: i64,
        pub back_buffer: (i64, i64),
        pub game: (i64, i64),
    }

    /// `min = MinResolution * 2 * RatioCorrection` = 600 × 450; the output
    /// scale is the smaller axis' `floor(native / min * 2) / 2`, at least 1;
    /// `BackBuffer = native / OutputScale` and `Game = BackBuffer / Scale`, each
    /// turned into whole pixels by `Vector2`'s `(int)(v + 0.5f)`. `MaxResolution`
    /// takes no part.
    pub fn v12_resolution(native: (i64, i64)) -> V12Resolution {
        let s = v12_scale();
        let min = (Ratio::int(MIN_GAME.0 * 2) * Ratio::new(15, 16), Ratio::int(MIN_GAME.1 * 2) * Ratio::new(9, 8));
        let halves = |n: i64, m: Ratio| (Ratio::int(n * 2) / m).floor();
        let output_halves = halves(native.1, min.1).min(halves(native.0, min.0)).max(2);
        let to_int = |v: Ratio| (v + Ratio::new(1, 2)).floor();
        let bb = (to_int(Ratio::new(native.0 * 2, output_halves)), to_int(Ratio::new(native.1 * 2, output_halves)));
        let game = (to_int(Ratio::int(bb.0) / s.x), to_int(Ratio::int(bb.1) / s.y));
        V12Resolution { output_halves, back_buffer: bb, game }
    }

    /// **The smallest window, from `start` on, that shows the whole
    /// interface**: each side grown, one pixel at a time, until
    /// [`v12_resolution`] gives a game area of at least [`MIN_GAME`]
    /// (`BurntimeClassic.MinResolution`, 320 × 200 — the start screen, a
    /// room, a service and the trader at its narrow width are all that
    /// size). Below 600 × 450 the output scale is 1 and the game is the
    /// window divided by `Scale` (15/8 × 9/4), rounded: 600 is the first
    /// width giving 320 (599 gives 319.47 → 319) and 449 the first height
    /// giving 200 (448 gives 199.1 → 199). Past those the half steps never
    /// take the game below the minimum again (`OutputScale` is at most
    /// `native / min`), so the search ends after at most a few hundred
    /// steps per side. A side already large enough is left alone.
    #[cfg_attr(not(target_os = "aros"), allow(dead_code))]
    pub fn grown_for_ui(start: (i64, i64)) -> (i64, i64) {
        let (mut w, mut h) = (start.0.max(1), start.1.max(1));
        loop {
            let g = v12_resolution((w, h)).game;
            if g.0 >= MIN_GAME.0 && g.1 >= MIN_GAME.1 {
                return (w, h);
            }
            if g.0 < MIN_GAME.0 {
                w += 1;
            }
            if g.1 < MIN_GAME.1 {
                h += 1;
            }
        }
    }

    /// **The window AROS opens at** when no `window WxH` is given and no
    /// earlier window was left at a size: v1.2's half the desktop
    /// (`BurntimeGame.cs:336`), grown where that would crop the interface
    /// ([`grown_for_ui`]), and kept inside the part of the desktop windows
    /// may use once the frame is around it. On AROS One's 1024 × 768
    /// Workbench half the desktop is 512 × 384, a 273 × 171 game, and the
    /// 320 × 200 start screen is cut off; this makes it 600 × 449. Only the
    /// opening size: resizing and fullscreen are v1.2's.
    /// `usable` is (width, height); `borders` top, left, bottom, right.
    #[cfg_attr(not(target_os = "aros"), allow(dead_code))]
    pub fn aros_first_window(desktop: (i64, i64), usable: (i64, i64), borders: (i64, i64, i64, i64)) -> (i64, i64) {
        let half = ((desktop.0 / 2).max(1), (desktop.1 / 2).max(1));
        let want = grown_for_ui(half);
        let room = (
            (usable.0 - borders.1 - borders.3).max(1),
            (usable.1 - borders.0 - borders.2).max(1),
        );
        (want.0.min(room.0), want.1.min(room.1))
    }

    /// How a sprite is sampled.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Filter {
        /// `PointClamp`: the texel under the pixel centre.
        Point,
        /// `LinearClamp`: the four texels around it, weighted.
        Linear,
    }

    /// `put`'s mix, on a whole pixel.
    #[inline]
    fn blend(d: u32, c: u32, a: u32) -> u32 {
        let mix = |s: u32, d: u32| (s * a + d * (255 - a)) / 255;
        0xFF00_0000 | mix(c >> 16 & 0xFF, d >> 16 & 0xFF) << 16 | mix(c >> 8 & 0xFF, d >> 8 & 0xFF) << 8 | mix(c & 0xFF, d & 0xFF)
    }

    /// **Pictures scaled once.** The NewGfx scale is the same for every
    /// frame of a window, so a picture drawn at the same sub-pixel phase
    /// comes out the same every time: v1.2's GPU draws it again each frame,
    /// this keeps the result. A draw is described exactly by which texel
    /// each covered column and row takes ([`Axis::point`] over `[d0, d1)`);
    /// two draws of the same picture with the same columns and rows are the
    /// same pixels, wherever they are. So that is the key — no reasoning
    /// about phases is needed for the copy to equal [`Target::blit_scaled`].
    /// In practice a 60 × 72 replacement takes the same columns at every
    /// position (one to one) and a 1993 sprite one of a handful of
    /// patterns, since the edges of game column `q` fall at
    /// `ceil(q · 1.8751 − ½)` and repeat every eight columns.
    pub struct Cache {
        entries: BTreeMap<Key, Vec<Variant>>,
        bytes: usize,
        pub hits: u64,
        pub misses: u64,
        cols: Vec<u32>,
        rows: Vec<u32>,
    }

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct Key {
        ptr: usize,
        len: usize,
        src: (u32, u32, u32, u32),
        scale: [i64; 4],
    }

    struct Variant {
        cols: Vec<u32>,
        rows: Vec<u32>,
        /// ARGB, `cols.len()` × `rows.len()`, the source alpha kept.
        px: Vec<u32>,
        opaque: bool,
    }

    /// What the kept pictures may take before they are all dropped and
    /// made again as they are drawn.
    pub const CACHE_BUDGET: usize = 48 << 20;

    impl Default for Cache {
        fn default() -> Cache {
            Cache::new()
        }
    }

    impl Cache {
        pub fn new() -> Cache {
            Cache { entries: BTreeMap::new(), bytes: 0, hits: 0, misses: 0, cols: Vec::new(), rows: Vec::new() }
        }

        /// Everything dropped: the look changed, or the pictures did.
        pub fn clear(&mut self) {
            self.entries.clear();
            self.bytes = 0;
        }

        pub fn bytes(&self) -> usize {
            self.bytes
        }

        /// The variant for this draw, made if it is not kept yet.
        fn get(&mut self, img: &PackedImage, src: (u32, u32, u32, u32), scale: Pair, ax: &Axis, ay: &Axis)
               -> (Key, usize) {
            let (sw, sh) = (src.2 as i64, src.3 as i64);
            let mut cols = core::mem::take(&mut self.cols);
            let mut rows = core::mem::take(&mut self.rows);
            cols.clear();
            rows.clear();
            cols.extend((ax.d0..ax.d1).map(|d| ax.point(d, sw) as u32));
            rows.extend((ay.d0..ay.d1).map(|d| ay.point(d, sh) as u32));
            let key = Key {
                ptr: img.rgba.as_ptr() as usize,
                len: img.rgba.len(),
                src,
                scale: [scale.x.num(), scale.x.den(), scale.y.num(), scale.y.den()],
            };
            let found = self.entries.get(&key).and_then(|l| l.iter().position(|v| v.cols == cols && v.rows == rows));
            let at = match found {
                Some(i) => {
                    self.hits += 1;
                    self.cols = cols;
                    self.rows = rows;
                    i
                }
                None => {
                    self.misses += 1;
                    let size = cols.len() * rows.len() * 4;
                    if self.bytes + size > CACHE_BUDGET {
                        self.clear();
                    }
                    let iw = img.width as usize;
                    let mut px = Vec::with_capacity(cols.len() * rows.len());
                    let mut opaque = true;
                    for &r in &rows {
                        let line = (src.1 as usize + r as usize) * iw;
                        for &c in &cols {
                            let at = (line + src.0 as usize + c as usize) * 4;
                            let p = &img.rgba[at..at + 4];
                            opaque &= p[3] == 255;
                            px.push((p[3] as u32) << 24 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32);
                        }
                    }
                    self.bytes += size;
                    let list = self.entries.entry(key).or_default();
                    list.push(Variant { cols, rows, px, opaque });
                    list.len() - 1
                }
            };
            (key, at)
        }
    }

    /// Columns worked out at a time: small enough for a modest AROS stack.
    const CHUNK: usize = 128;

    /// One axis of a draw: the device pixels `[d0, d1)` the quad covers, and
    /// the texel coordinate under pixel `d`, `u(d) = ((2d+1)·K − 2A) / 2` with
    /// `K = sprite_scale / device_scale` and `A = position · sprite_scale`,
    /// kept as `num(d) / den` in integers.
    struct Axis {
        d0: i64,
        d1: i64,
        kn: i128,
        an: i128,
        kd: i128,
        ad: i128,
    }

    impl Axis {
        fn new(pos: Ratio, size: i64, scale: Ratio, device: Ratio) -> Axis {
            let half = Ratio::new(1, 2);
            // Covered: `pos·S' ≤ d + ½ < (pos + size/scale)·S'`.
            let e0 = pos * device;
            let e1 = (pos + Ratio::int(size) / scale) * device;
            let k = scale / device;
            let a = pos * scale;
            Axis {
                d0: (e0 - half).ceil(),
                d1: (e1 - half).ceil(),
                kn: k.num() as i128,
                kd: k.den() as i128,
                an: a.num() as i128,
                ad: a.den() as i128,
            }
        }

        /// `u(d)` as `(num, den)`.
        fn u(&self, d: i64) -> (i128, i128) {
            ((2 * d as i128 + 1) * self.kn * self.ad - 2 * self.an * self.kd, 2 * self.kd * self.ad)
        }

        /// The texel under pixel `d`, within `[0, size)`.
        fn point(&self, d: i64, size: i64) -> i64 {
            let (n, q) = self.u(d);
            (n.div_euclid(q) as i64).clamp(0, size - 1)
        }

        /// The left texel of the pair around pixel `d`'s centre and the right
        /// one's weight, 0–256: `t = u − ½`, `i = floor(t)`.
        fn linear(&self, d: i64) -> (i64, u32) {
            let (n, q) = self.u(d);
            let t = n - q / 2;
            let i = t.div_euclid(q);
            let rest = t.rem_euclid(q);
            (i as i64, ((rest * 512 + q) / (2 * q)) as u32)
        }
    }

    impl Target {
        /// Draw `src` of `img` — an image with `scale` pixels per game pixel —
        /// with its top left at game `(x, y)` (plus [`Target::origin`]), as
        /// v1.2's NewGfx renderer does. `tint` as [`Target::blit_part`].
        #[allow(clippy::too_many_arguments)]
        pub fn blit_scaled(&mut self, img: &PackedImage, src: (u32, u32, u32, u32), scale: Pair,
                           x: Ratio, y: Ratio, tint: Option<u32>, filter: Filter) {
            let (sx, sy, sw, sh) = (src.0 as i64, src.1 as i64, src.2 as i64, src.3 as i64);
            if sw <= 0 || sh <= 0 {
                return;
            }
            let dev = v12_device_scale();
            let ax = Axis::new(x + Ratio::int(self.origin.0 as i64), sw, scale.x, dev.x);
            let ay = Axis::new(y + Ratio::int(self.origin.1 as i64), sh, scale.y, dev.y);
            let (bx0, by0, bx1, by1) = self.bounds();
            let (cx0, cx1) = (ax.d0.max(bx0), ax.d1.min(bx1));
            let (cy0, cy1) = (ay.d0.max(by0), ay.d1.min(by1));
            if cx1 <= cx0 || cy1 <= cy0 {
                return;
            }
            let (iw, ih) = (img.width as i64, img.height as i64);
            if sx < 0 || sy < 0 || sx + sw > iw || sy + sh > ih || (img.rgba.len() as i64) < iw * ih * 4 {
                return;
            }
            // The columns once per chunk, rather than a division per pixel.
            let mut cols = [(0i64, 0u32); CHUNK];
            let mut c0 = cx0;
            while c0 < cx1 {
                let span = ((cx1 - c0) as usize).min(CHUNK);
                for (i, c) in cols[..span].iter_mut().enumerate() {
                    let d = c0 + i as i64;
                    *c = match filter {
                        Filter::Point => (sx + ax.point(d, sw), 0),
                        Filter::Linear => {
                            let (t, w) = ax.linear(d);
                            (sx + t, w)
                        }
                    };
                }
                for dy in cy0..cy1 {
                    let (ty, wy) = match filter {
                        Filter::Point => (sy + ay.point(dy, sh), 0),
                        Filter::Linear => {
                            let (t, w) = ay.linear(dy);
                            (sy + t, w)
                        }
                    };
                    if filter == Filter::Point {
                        // The common case, without a call per pixel: a row of
                        // the image, a row of the backbuffer.
                        let row = (ty * iw) as usize * 4;
                        let out = dy as usize * self.w + c0 as usize;
                        for (i, &(tx, _)) in cols[..span].iter().enumerate() {
                            let at = row + tx as usize * 4;
                            let c = &img.rgba[at..at + 4];
                            let a = if self.alpha == 255 { c[3] as u32 } else { c[3] as u32 * self.alpha / 255 };
                            if a == 0 {
                                continue;
                            }
                            let (r, g, b) = match tint {
                                None => (c[0] as u32, c[1] as u32, c[2] as u32),
                                Some(t) => {
                                    let f = c[0] as u32;
                                    ((t >> 16 & 0xFF) * f / 255, (t >> 8 & 0xFF) * f / 255, (t & 0xFF) * f / 255)
                                }
                            };
                            if a >= 255 {
                                self.px[out + i] = 0xFF00_0000 | r << 16 | g << 8 | b;
                            } else {
                                self.put(c0 + i as i64, dy, r, g, b, a);
                            }
                        }
                        continue;
                    }
                    for (i, &(tx, wx)) in cols[..span].iter().enumerate() {
                        let dx = c0 + i as i64;
                        let c = match filter {
                            Filter::Point => match img.pixel(tx as u32, ty as u32) {
                                Some(p) => [p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32],
                                None => continue,
                            },
                            // `LinearClamp` clamps to the whole texture, not to the
                            // source rectangle: a glyph's edge reads its neighbour.
                            Filter::Linear => {
                                let at = |x: i64, y: i64| {
                                    let p = img.pixel(x.clamp(0, iw - 1) as u32, y.clamp(0, ih - 1) as u32).unwrap_or(&[0, 0, 0, 0]);
                                    [p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32]
                                };
                                let (a, b, c, d) = (at(tx, ty), at(tx + 1, ty), at(tx, ty + 1), at(tx + 1, ty + 1));
                                let mut out = [0u32; 4];
                                for k in 0..4 {
                                    let top = a[k] * (256 - wx) + b[k] * wx;
                                    let bottom = c[k] * (256 - wx) + d[k] * wx;
                                    out[k] = (top * (256 - wy) + bottom * wy + 32768) >> 16;
                                }
                                out
                            }
                        };
                        let a = if self.alpha == 255 { c[3] } else { c[3] * self.alpha / 255 };
                        if a == 0 {
                            continue;
                        }
                        match tint {
                            None => self.put(dx, dy, c[0], c[1], c[2], a),
                            Some(t) => self.put(dx, dy, (t >> 16 & 0xFF) * c[0] / 255, (t >> 8 & 0xFF) * c[0] / 255,
                                                (t & 0xFF) * c[0] / 255, a),
                        }
                    }
                }
                c0 += span as i64;
            }
        }

        /// The backbuffer, or the part of it [`Target::clip`] leaves.
        pub fn bounds(&self) -> (i64, i64, i64, i64) {
            let (w, h) = (self.w as i64, self.h as i64);
            match self.clip {
                Some((x0, y0, x1, y1)) => (x0.max(0), y0.max(0), x1.min(w), y1.min(h)),
                None => (0, 0, w, h),
            }
        }

        /// The backbuffer pixels a game rectangle covers under the NewGfx
        /// transform: `[d0, d1)` across and down, the centres inside.
        pub fn ng_rect(&self, x: i32, y: i32, w: i32, h: i32) -> ((i64, i64), (i64, i64)) {
            let dev = v12_device_scale();
            let ax = Axis::new(Ratio::int((x + self.origin.0) as i64), w.max(0) as i64, Ratio::ONE, dev.x);
            let ay = Axis::new(Ratio::int((y + self.origin.1) as i64), h.max(0) as i64, Ratio::ONE, dev.y);
            ((ax.d0, ax.d1), (ay.d0, ay.d1))
        }

        /// `RenderRect` in NewGfx: the pixels whose centres the transformed
        /// rectangle covers, at `a`.
        pub fn ng_fill(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32, a: u32) {
            if w <= 0 || h <= 0 {
                return;
            }
            let ((x0, x1), (y0, y1)) = self.ng_rect(x, y, w, h);
            let (bx0, by0, bx1, by1) = self.bounds();
            let a = a * self.alpha / 255;
            for dy in y0.max(by0)..y1.min(by1) {
                for dx in x0.max(bx0)..x1.min(bx1) {
                    self.put(dx, dy, rgb >> 16 & 0xFF, rgb >> 8 & 0xFF, rgb & 0xFF, a);
                }
            }
        }

        /// A game-resolution buffer under the NewGfx transform: each of its
        /// pixels is a 1993 sprite's pixel, and lands exactly where drawing
        /// that sprite at factor 1 would put it (the columns depend on the
        /// absolute game column only). `masked`: a pixel with alpha 0 is not
        /// drawn.
        pub fn ng_pixels(&mut self, px: &[u32], w: usize, h: usize, x: i32, y: i32, masked: bool) {
            let dev = v12_device_scale();
            let ax = Axis::new(Ratio::int((x + self.origin.0) as i64), w as i64, Ratio::ONE, dev.x);
            let ay = Axis::new(Ratio::int((y + self.origin.1) as i64), h as i64, Ratio::ONE, dev.y);
            let (bx0, by0, bx1, by1) = self.bounds();
            let (cx0, cx1) = (ax.d0.max(bx0), ax.d1.min(bx1));
            let (cy0, cy1) = (ay.d0.max(by0), ay.d1.min(by1));
            if cx1 <= cx0 || cy1 <= cy0 {
                return;
            }
            let cols: Vec<usize> = (cx0..cx1).map(|d| ax.point(d, w as i64) as usize).collect();
            for dy in cy0..cy1 {
                let sy = ay.point(dy, h as i64) as usize;
                let row = &px[sy * w..sy * w + w];
                let out = dy as usize * self.w + cx0 as usize;
                for (i, &sx) in cols.iter().enumerate() {
                    let c = row[sx];
                    if masked && c >> 24 == 0 {
                        continue;
                    }
                    self.px[out + i] = c | 0xFF00_0000;
                }
            }
        }

        /// A NewGfx draw: through the cache when it can be (no tint), the
        /// reference path otherwise. Both give the same pixels.
        pub fn ng_draw(&mut self, img: &PackedImage, src: (u32, u32, u32, u32), scale: Pair, x: Ratio, y: Ratio,
                       tint: Option<u32>) {
            if tint.is_some() {
                self.blit_scaled(img, src, scale, x, y, tint, Filter::Point);
            } else {
                self.draw_cached(img, src, scale, x, y);
            }
        }

        /// [`Target::blit_scaled`] with point sampling and no tint, from a
        /// copy scaled once and kept ([`Cache`]): the same pixels, copied
        /// one to one.
        pub fn draw_cached(&mut self, img: &PackedImage, src: (u32, u32, u32, u32), scale: Pair, x: Ratio, y: Ratio) {
            let (sx, sy, sw, sh) = (src.0 as i64, src.1 as i64, src.2 as i64, src.3 as i64);
            if sw <= 0 || sh <= 0 {
                return;
            }
            let (iw, ih) = (img.width as i64, img.height as i64);
            if sx + sw > iw || sy + sh > ih || (img.rgba.len() as i64) < iw * ih * 4 {
                return;
            }
            let dev = v12_device_scale();
            let ax = Axis::new(x + Ratio::int(self.origin.0 as i64), sw, scale.x, dev.x);
            let ay = Axis::new(y + Ratio::int(self.origin.1 as i64), sh, scale.y, dev.y);
            let (bx0, by0, bx1, by1) = self.bounds();
            let (cx0, cx1) = (ax.d0.max(bx0), ax.d1.min(bx1));
            let (cy0, cy1) = (ay.d0.max(by0), ay.d1.min(by1));
            if cx1 <= cx0 || cy1 <= cy0 {
                return;
            }
            let Target { px, w, alpha, cache, .. } = self;
            let at = cache.get(img, src, scale, &ax, &ay);
            let v = &cache.entries.get(&at.0).expect("just put there")[at.1];
            let vw = (ax.d1 - ax.d0) as usize;
            let alpha = *alpha;
            for dy in cy0..cy1 {
                let row = &v.px[(dy - ay.d0) as usize * vw..][..vw];
                let from = &row[(cx0 - ax.d0) as usize..(cx1 - ax.d0) as usize];
                let out = &mut px[dy as usize * *w + cx0 as usize..][..from.len()];
                if v.opaque && alpha == 255 {
                    out.copy_from_slice(from);
                    continue;
                }
                for (o, &c) in out.iter_mut().zip(from) {
                    let a = c >> 24;
                    let a = if alpha == 255 { a } else { a * alpha / 255 };
                    if a == 0 {
                        continue;
                    }
                    if a >= 255 {
                        *o = c | 0xFF00_0000;
                    } else {
                        *o = blend(*o, c, a);
                    }
                }
            }
        }

        /// `RenderTarget.SnapToPhysicalPixels`: a game position moved to the
        /// nearest backbuffer pixel, `Math.Round(abs * Scale) / Scale`. Fonts
        /// snap the start of each line (`Font.DrawText`).
        pub fn snap(&self, x: Ratio, y: Ratio) -> (Ratio, Ratio) {
            let s = v12_scale();
            let (ox, oy) = (Ratio::int(self.origin.0 as i64), Ratio::int(self.origin.1 as i64));
            let ax = Ratio::int(((x + ox) * s.x).round_even()) / s.x;
            let ay = Ratio::int(((y + oy) * s.y).round_even()) / s.y;
            (ax - ox, ay - oy)
        }
    }

    /// A v1.2 font and its atlas, drawn the NewGfx way.
    pub struct ScaledFace<'a> {
        pub font: burngfx::font::ScaledFont,
        pub atlas: PackedImage<'a>,
    }

    impl ScaledFace<'_> {
        /// Font sprites are made with `LinearFiltering = true`
        /// (`ResourceManager.cs:92-97`), which applies only when the sprite is
        /// drawn smaller than its atlas: `factor * Scale < 1` on either axis
        /// (`RenderDevice.cs:416-418`). `font-small-highres` (½), not
        /// `font-highres` (1:1).
        pub fn filter(&self) -> Filter {
            let f = self.font.factor();
            let s = v12_scale();
            if f.x * s.x < Ratio::ONE || f.y * s.y < Ratio::ONE {
                Filter::Linear
            } else {
                Filter::Point
            }
        }

        /// `Font.DrawText` for one line: `ResolvePosition` (the window clamp
        /// aside), `y` made whole, the start snapped, each glyph at its pen
        /// position plus `offset` down. Returns the start used.
        #[allow(clippy::too_many_arguments)]
        pub fn draw(&self, t: &mut Target, s: &str, x: i32, y: i32, h: Align, v: Align,
                    colour: Option<u32>) -> (Ratio, Ratio) {
            let w = self.font.width_f(s);
            let x = match h {
                Align::Start => Ratio::int(x as i64),
                Align::Centre => Ratio::int(x as i64) - w / Ratio::int(2),
                Align::End => Ratio::int(x as i64) - w,
            };
            let hgt = Ratio::int(self.font.metric_height());
            let y = match v {
                Align::Start => Ratio::int(y as i64),
                Align::Centre => Ratio::int(y as i64) - hgt / Ratio::int(2),
                Align::End => Ratio::int(y as i64) - hgt,
            };
            let y = Ratio::int(y.trunc());
            let (x, y) = t.snap(x, y);
            let m = Ratio::int(self.font.multiplier);
            let scale = Pair::new(self.font.scale.x * m, self.font.scale.y * m);
            let filter = self.filter();
            let dy = y + Ratio::int(self.font.offset);
            for p in self.font.layout(s) {
                let g = p.glyph;
                t.blit_scaled(&self.atlas, (g.src_x as u32, g.src_y as u32, g.src_w as u32, g.src_h as u32),
                              scale, x + p.x, dy, colour, filter);
            }
            (x, y)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// `Burntime.Remaster.Tests/ResolutionTests.cs`: 720p and 1440p share a
        /// 455 × 213 game, 800p is 1.5× with an 853 × 533 target and a 455 × 237
        /// game; the AROS window, 640 × 480, is 1× and 341 × 213.
        #[test]
        fn the_resolution_is_v12s() {
            let r = v12_resolution((1280, 800));
            assert_eq!((r.output_halves, r.back_buffer, r.game), (3, (853, 533), (455, 237)));
            let r = v12_resolution((1280, 720));
            assert_eq!((r.output_halves, r.game), (3, (455, 213)));
            let r = v12_resolution((2560, 1440));
            assert_eq!((r.output_halves, r.game), (6, (455, 213)));
            let r = v12_resolution((640, 480));
            assert_eq!((r.output_halves, r.back_buffer, r.game), (2, (640, 480), (341, 213)));
            // Smaller than the minimum is still 1×.
            assert_eq!(v12_resolution((320, 200)).output_halves, 2);
        }

        /// The smallest window showing the whole interface is computed from
        /// v1.2's rule, not assumed: 600 × 449 gives exactly 320 × 200 and
        /// one pixel less on either side crops it. 640 × 480, the size every
        /// scripted run uses, is above it.
        #[test]
        fn the_whole_interface_needs_600_by_449() {
            assert_eq!(grown_for_ui((1, 1)), (600, 449));
            assert_eq!(v12_resolution((600, 449)).game, (320, 200));
            assert_eq!(v12_resolution((599, 449)).game.0, 319);
            assert_eq!(v12_resolution((600, 448)).game.1, 199);
            assert!(v12_resolution((640, 480)).game >= (320, 200));
            // Nothing above the minimum crops it again, whatever the step.
            for w in 600..2000 {
                for h in [449, 450, 480, 675, 676, 900, 1080] {
                    let g = v12_resolution((w, h)).game;
                    assert!(g.0 >= 320 && g.1 >= 200, "{w}x{h} gives {g:?}");
                }
            }
        }

        /// AROS' first window: half the desktop, grown so the interface
        /// fits, kept inside the usable area with the frame around it.
        #[test]
        fn the_first_aros_window() {
            let frame = (20, 4, 13, 4); // top, left, bottom, right
            // AROS One's Workbench: half is 512 × 384, a 273 × 171 game,
            // cropped; grown to the minimum.
            assert_eq!(v12_resolution((512, 384)).game, (273, 171));
            assert_eq!(aros_first_window((1024, 768), (1024, 768), frame), (600, 449));
            assert_eq!(aros_first_window((800, 600), (800, 600), frame), (600, 449));
            // Only the side that is short grows.
            assert_eq!(aros_first_window((1280, 720), (1280, 720), frame), (640, 449));
            assert_eq!(aros_first_window((1366, 768), (1366, 768), frame), (683, 449));
            // Half is enough: v1.2's size.
            assert_eq!(aros_first_window((1920, 1080), (1920, 1080), frame), (960, 540));
            assert_eq!(aros_first_window((1200, 900), (1200, 900), frame), (600, 450));
            assert_eq!(aros_first_window((2560, 1440), (2560, 1400), frame), (1280, 720));
            // A desktop too small for the minimum and the frame: as much as
            // fits; the rest is fullscreen's.
            assert_eq!(aros_first_window((640, 480), (640, 480), frame), (600, 447));
            assert_eq!(aros_first_window((640, 480), (640, 480), (0, 0, 0, 0)), (600, 449));
            assert_eq!(aros_first_window((320, 200), (320, 200), frame), (312, 167));
            // Nothing known about the desktop's use: never below one pixel.
            assert_eq!(aros_first_window((0, 0), (0, 0), frame), (1, 1));
        }

        /// An image whose every pixel says where it is: red = x, green = y,
        /// blue 255 so none is the cleared black.
        fn ramp(w: u32, h: u32) -> Vec<u8> {
            let mut b = Vec::new();
            b.extend_from_slice(&(w as u16).to_le_bytes());
            b.extend_from_slice(&(h as u16).to_le_bytes());
            for y in 0..h {
                for x in 0..w {
                    b.extend_from_slice(&[x as u8, y as u8, 255, 255]);
                }
            }
            b
        }

        fn columns(t: &Target, row: usize, n: usize) -> Vec<i64> {
            (0..n).map(|x| {
                let p = t.px[row * t.w + x];
                if p == 0xFF00_0000 { -1 } else { (p >> 16 & 0xFF) as i64 }
            }).collect()
        }

        fn target() -> Target {
            Target::new(640, 480)
        }

        /// `blit_sub` (v1.3's `DrawSpriteF`) in the classic look: at a whole
        /// pixel exactly what `blit_part` draws; half a game pixel on, the
        /// picture moves by the device pixels half a game pixel covers.
        #[test]
        fn a_classic_sprite_at_a_fraction_of_a_pixel() {
            let bytes = ramp(16, 4);
            let img = PackedImage::parse(&bytes).unwrap();
            let mut a = target();
            a.blit_part(&img, (0, 0, 16, 4), 1, 10, 5, None);
            let mut b = target();
            b.blit_sub(&img, 10 * 1024, 5 * 1024, 1024);
            assert_eq!(a.px, b.px, "a whole pixel: the same picture");
            let mut c = target();
            c.blit_sub(&img, 10 * 1024 + 512, 5 * 1024, 1024);
            let first = |t: &Target| (0..t.w).find(|&x| t.px[12 * t.w + x] != 0xFF00_0000);
            // 10 × 15/8 = 18.75 → column 18 (`blit_pixels`' floor); 10.5 → 19.6875 → 19.
            assert_eq!((first(&b), first(&c)), (Some(18), Some(19)));
            // Clipped like the rest of the view.
            let mut d = target();
            d.clip = Some((0, 0, 25, 480));
            d.blit_sub(&img, 10 * 1024, 5 * 1024, 1024);
            assert!((0..d.h).all(|y| (25..d.w).all(|x| d.px[y * d.w + x] == 0xFF00_0000)));
        }

        /// A 1993 sprite at factor 1 under `Scale(1.8751, 2.2501)`: game column
        /// k spans `[1.8751k, 1.8751(k+1))`, and a pixel belongs to it when its
        /// centre does — 2, 2, 2, 2, 1, 2, 2, 2 pixels, 15 in 8 columns.
        #[test]
        fn a_classic_sprite_covers_the_pixels_whose_centres_it_covers() {
            let bytes = ramp(16, 4);
            let img = PackedImage::parse(&bytes).unwrap();
            let mut t = target();
            t.blit_scaled(&img, (0, 0, 16, 4), Pair::ONE, Ratio::ZERO, Ratio::ZERO, None, Filter::Point);
            assert_eq!(columns(&t, 0, 17), [0, 0, 1, 1, 2, 2, 3, 3, 4, 5, 5, 6, 6, 7, 7, 8, 8]);
            // Rows, 2.2501 each: [0, 2.2501) holds centres 0.5 and 1.5,
            // [2.2501, 4.5002) holds 2.5, 3.5 and 4.5 — 2, 3, 2, 2: 9 in 4.
            let rows: Vec<i64> = (0..8).map(|y| {
                let p = t.px[y * t.w];
                if p == 0xFF00_0000 { -1 } else { (p >> 8 & 0xFF) as i64 }
            }).collect();
            assert_eq!(rows, [0, 0, 1, 1, 1, 2, 2, 3]);
        }

        /// A replacement at its own `sprite_scale` (`newgfx.txt:13`, 60 × 72 a
        /// tile) lands one image pixel per backbuffer pixel: a tile at game x 32
        /// starts at pixel 60 (32 × 1.8751 = 60.0032) and its 60 columns are
        /// 0..59 in order.
        #[test]
        fn a_60x72_replacement_lands_one_to_one() {
            let bytes = ramp(60, 72);
            let img = PackedImage::parse(&bytes).unwrap();
            let mut t = target();
            let s = Pair::new(Ratio::new(15, 8), Ratio::new(9, 4));
            t.blit_scaled(&img, (0, 0, 60, 72), s, Ratio::int(32), Ratio::ZERO, None, Filter::Point);
            let row = columns(&t, 0, 121);
            assert_eq!(row[59], -1);
            assert_eq!(&row[60..120], &(0..60).collect::<Vec<i64>>()[..]);
            assert_eq!(row[120], -1);
        }

        /// A picture wider than the columns worked out at a time is drawn
        /// whole and in order across the joins.
        #[test]
        fn a_wide_picture_is_whole_across_the_chunks() {
            let bytes = ramp(300, 1);
            let img = PackedImage::parse(&bytes).unwrap();
            let mut t = target();
            let s = Pair::new(Ratio::new(15, 8), Ratio::new(9, 4));
            t.blit_scaled(&img, (0, 0, 300, 1), s, Ratio::ZERO, Ratio::ZERO, None, Filter::Point);
            let row = columns(&t, 0, 301);
            assert_eq!(&row[..300], &(0..300).map(|x| x % 256).collect::<Vec<i64>>()[..]);
            assert_eq!(row[300], -1);
        }

        /// `[replacement]`'s 2 × 2 pictures (`newgfx.txt:160-166`) are drawn at
        /// 1.875/2 by point sampling: 64 columns become 60, and the first one
        /// left out is column 8 (pixel 8's centre, 8.5, is texel 9.07).
        #[test]
        fn a_2x2_picture_drops_columns_by_point_sampling() {
            let bytes = ramp(64, 2);
            let img = PackedImage::parse(&bytes).unwrap();
            let mut t = target();
            t.blit_scaled(&img, (0, 0, 64, 2), Pair::new(Ratio::int(2), Ratio::int(2)), Ratio::ZERO, Ratio::ZERO,
                          None, Filter::Point);
            let row = columns(&t, 0, 61);
            assert_eq!(&row[5..10], &[5, 6, 7, 9, 10]);
            assert_eq!((row[59], row[60]), (63, -1));
        }

        /// `font-small-highres` (3.75 × 4.5) is half its atlas: linear
        /// sampling at a pixel centre falls between two texels in each axis,
        /// so each pixel is the mean of a 2 × 2 block.
        #[test]
        fn the_small_font_is_a_two_by_two_mean() {
            // White and black columns, alternating.
            let mut b = Vec::new();
            b.extend_from_slice(&8u16.to_le_bytes());
            b.extend_from_slice(&4u16.to_le_bytes());
            for _y in 0..4 {
                for x in 0..8 {
                    let v = if x % 2 == 0 { 255 } else { 0 };
                    b.extend_from_slice(&[v, v, v, 255]);
                }
            }
            let img = PackedImage::parse(&b).unwrap();
            let mut t = target();
            let s = Pair::new(Ratio::new(15, 4), Ratio::new(9, 2));
            t.blit_scaled(&img, (0, 0, 8, 4), s, Ratio::ZERO, Ratio::ZERO, None, Filter::Linear);
            for x in 0..3 {
                let v = t.px[x] >> 16 & 0xFF;
                assert!((127..=128).contains(&v), "pixel {x}: {v}");
            }
        }

        /// `font-highres` is drawn 1:1 and point-sampled; `font-small-highres`
        /// at half its atlas, interpolated (in `float` too: 1/1.875 × 1.875 is
        /// 1.0, 1/3.75 × 1.875 is 0.5).
        #[test]
        fn only_the_small_font_is_interpolated() {
            let bytes = ramp(64, 64);
            let face = |scale: &str| ScaledFace {
                font: burngfx::font::ScaledFont::parse(&format!("lines=1\nheight=12\nscale={scale}\nchar0=\"a\"\nwidth0=4\n"), 64).unwrap(),
                atlas: PackedImage::parse(&bytes).unwrap(),
            };
            assert_eq!(face("1.875x2.25").filter(), Filter::Point);
            assert_eq!(face("3.75x4.5").filter(), Filter::Linear);
            assert_eq!(face("1").filter(), Filter::Point);
        }

        /// An image with every pixel distinct, some of it see-through and
        /// some half: what the cache keeps must be exactly what is drawn.
        fn speckled(w: u32, h: u32) -> Vec<u8> {
            let mut b = Vec::new();
            b.extend_from_slice(&(w as u16).to_le_bytes());
            b.extend_from_slice(&(h as u16).to_le_bytes());
            for y in 0..h {
                for x in 0..w {
                    let a = match (x * 7 + y * 3) % 5 { 0 => 0, 1 => 128, _ => 255 };
                    b.extend_from_slice(&[(x * 3) as u8, (y * 5) as u8, (x ^ y) as u8, a]);
                }
            }
            b
        }

        /// **The cache equals the reference draw**, pixel for pixel, for
        /// each kind of picture v1.2 scales — a 1993 sprite (1 × 1), a
        /// 60 × 72 tile and a 30 × 36 body (15/8 × 9/4), a `[replacement]`
        /// 2 × 2 and a `[replacement_stretched]` 2 × 19/8 — at positions on
        /// and off every edge, over a busy background, at full and half
        /// opacity, clipped and not, and drawn twice so the second comes
        /// from what was kept.
        #[test]
        fn the_cache_draws_what_blit_scaled_draws() {
            let s60 = Pair::new(Ratio::new(15, 8), Ratio::new(9, 4));
            let cases: [(u32, u32, Pair); 5] = [
                (16, 16, Pair::ONE),
                (60, 72, s60),
                (30, 36, s60),
                (64, 64, Pair::new(Ratio::int(2), Ratio::int(2))),
                (64, 76, Pair::new(Ratio::int(2), Ratio::new(19, 8))),
            ];
            let xs: Vec<i64> = (-40..-30).chain(0..17).chain((300..345).step_by(3)).collect();
            let ys: Vec<i64> = (-35..-28).chain(0..9).chain((190..220).step_by(4)).collect();
            let mut background = target();
            for (i, p) in background.px.iter_mut().enumerate() {
                *p = 0xFF00_0000 | (i as u32).wrapping_mul(2_654_435_761) >> 8;
            }
            let mut drawn = 0;
            for (w, h, scale) in cases {
                let bytes = speckled(w, h);
                let img = PackedImage::parse(&bytes).unwrap();
                let mut reference = target();
                let mut cached = target();
                for (n, (&x, &y)) in xs.iter().flat_map(|x| ys.iter().map(move |y| (x, y))).enumerate() {
                    for t in [&mut reference, &mut cached] {
                        t.px.copy_from_slice(&background.px);
                        t.alpha = if n % 3 == 0 { 128 } else { 255 };
                        t.origin = (n as i32 % 3, 0);
                        t.clip = if n % 4 == 0 { Some((5, 7, 600, 470)) } else { None };
                    }
                    let src = if n % 2 == 0 { (0, 0, w, h) } else { (1, 2, w - 3, h - 2) };
                    reference.blit_scaled(&img, src, scale, Ratio::int(x), Ratio::int(y), None, Filter::Point);
                    for _ in 0..2 {
                        cached.px.copy_from_slice(&background.px);
                        cached.draw_cached(&img, src, scale, Ratio::int(x), Ratio::int(y));
                    }
                    assert!(reference.px == cached.px, "{w}x{h} at {scale:?} drawn at ({x}, {y}) differs");
                    drawn += 1;
                }
                // Each phase of the picture is made once; most draws were copies.
                assert!(cached.cache.hits >= cached.cache.misses, "{w}x{h}: {} kept, {} made", cached.cache.hits, cached.cache.misses);
            }
            assert!(drawn > 2000);
        }

        /// A replacement at its own scale takes the same columns wherever it
        /// is (one to one), so the cache keeps one copy of it; a 1993 sprite
        /// repeats every eight game columns and four rows.
        #[test]
        fn a_tile_is_kept_once_and_a_1993_sprite_once_a_phase() {
            let s60 = Pair::new(Ratio::new(15, 8), Ratio::new(9, 4));
            let tile = speckled(60, 72);
            let sprite = speckled(16, 16);
            let (tile, sprite) = (PackedImage::parse(&tile).unwrap(), PackedImage::parse(&sprite).unwrap());
            let mut t = target();
            for x in 0..40 {
                for y in 0..20 {
                    t.draw_cached(&tile, (0, 0, 60, 72), s60, Ratio::int(x * 8), Ratio::int(y * 9));
                }
            }
            assert_eq!(t.cache.misses, 1);
            t.cache.clear();
            t.cache.misses = 0;
            for x in 0..40 {
                for y in 0..20 {
                    t.draw_cached(&sprite, (0, 0, 16, 16), Pair::ONE, Ratio::int(x), Ratio::int(y));
                }
            }
            assert!(t.cache.misses <= 8 * 4, "{} variants", t.cache.misses);
        }

        /// The map's composed 1993 tiles under the transform land where
        /// drawing each tile would: [`Target::ng_pixels`] against
        /// [`Target::blit_scaled`] of the same pixels at scale 1.
        #[test]
        fn the_composed_map_lands_where_its_tiles_would() {
            let bytes = speckled(97, 41);
            let img = PackedImage::parse(&bytes).unwrap();
            let opaque: Vec<u32> = (0..97 * 41).map(|i| {
                let p = img.pixel(i % 97, i / 97).unwrap();
                0xFF00_0000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32
            }).collect();
            let mut solid = bytes.clone();
            for px in solid[4..].chunks_mut(4) {
                px[3] = 255;
            }
            let solid = PackedImage::parse(&solid).unwrap();
            for (x, y) in [(0, 0), (16, 0), (-5, 3), (7, 11), (300, 190)] {
                let (mut a, mut b) = (target(), target());
                a.ng_pixels(&opaque, 97, 41, x, y, false);
                b.blit_scaled(&solid, (0, 0, 97, 41), Pair::ONE, Ratio::int(x as i64), Ratio::int(y as i64), None, Filter::Point);
                assert!(a.px == b.px, "at ({x}, {y})");
            }
        }

        /// `RenderRect` under the transform covers what a sprite of that
        /// size would: 1 game pixel at x 4 is device 8..9 (centre 8.5 lies
        /// in [7.5004, 9.3755)), at x 3 device 6..7.
        #[test]
        fn a_newgfx_rectangle_covers_the_centres_inside_it() {
            let t = target();
            assert_eq!(t.ng_rect(4, 0, 1, 1).0, (8, 9));
            assert_eq!(t.ng_rect(3, 0, 1, 1).0, (6, 8));
            assert_eq!(t.ng_rect(0, 0, 32, 32), ((0, 60), (0, 72)));
            assert_eq!(t.ng_rect(16, 0, 309, 173).0, (30, 609));
        }

        /// v1.2's half steps on the host (UI-41): 1280 × 720 is 1.5×, an
        /// 853 × 480 backbuffer and a 455 × 213 game; a click at a window
        /// pixel and the window pixel of a scene point agree; 640 × 480,
        /// AROS's, is what it was.
        #[test]
        fn the_window_scales_in_half_steps() {
            let t = Target::geometry(1280, 720);
            assert_eq!((t.w, t.h, t.halves, t.game_size()), (853, 480, 3, (455, 213)));
            assert_eq!(t.factor_text(), "1.5");
            for (x, y) in [(0, 0), (10, 7), (200, 100), (454, 212)] {
                let (wx, wy) = t.scene_to_window(x, y);
                assert_eq!(t.window_to_scene(wx, wy), (x, y), "scene ({x}, {y}) → window ({wx}, {wy})");
            }
            let a = Target::geometry(640, 480);
            assert_eq!((a.w, a.h, a.halves, a.game_size(), a.factor_text().as_str()), (640, 480, 2, (341, 213), "1"));
            assert_eq!(Target::geometry(1920, 1080).game_size(), (512, 240));
            assert_eq!(Target::geometry(1280, 800).game_size(), (455, 237));
        }

        /// **One target through a window's changes** (UI-42): each size
        /// gives v1.2's geometry, the same as a target made for it, and the
        /// pointer's round trip holds out to the game's last pixel.
        #[test]
        fn a_target_follows_the_window() {
            let mut t = Target::new(640, 480);
            for &(win, want) in &[
                ((1280, 720), (853, 480, 3, (455, 213))),
                ((1920, 1080), (960, 540, 4, (512, 240))),
                ((640, 480), (640, 480, 2, (341, 213))),
                ((2560, 1440), (853, 480, 6, (455, 213))),
                ((1024, 768), (683, 512, 3, (364, 228))),
            ] {
                let g = t.geometry_for(win.0, win.1).expect("a new size");
                t.set_geometry(g);
                assert_eq!((t.w, t.h, t.halves, t.game_size()), want, "window {win:?}");
                assert_eq!(t.px.len(), t.w * t.h);
                let (gw, gh) = t.game_size();
                // The corners and the middle of the game, and the window's
                // own corners back into the game.
                for (x, y) in [(0, 0), (gw - 1, 0), (0, gh - 1), (gw - 1, gh - 1), (gw / 2, gh / 2)] {
                    let (wx, wy) = t.scene_to_window(x, y);
                    assert!(wx >= 0 && wy >= 0 && (wx as usize) < win.0 && (wy as usize) < win.1, "{win:?}: ({x}, {y}) → ({wx}, {wy})");
                    assert_eq!(t.window_to_scene(wx, wy), (x, y), "{win:?}: ({x}, {y}) → ({wx}, {wy})");
                }
                let (lx, ly) = t.window_to_scene(win.0 as i32 - 1, win.1 as i32 - 1);
                assert!(lx <= gw && ly <= gh, "{win:?}: the window's last pixel is game ({lx}, {ly})");
                assert_eq!(t.window_to_scene(0, 0), (0, 0));
                // Nothing to do for the same size again.
                assert_eq!(t.geometry_for(win.0, win.1), None);
            }
        }

        /// Fullscreen and back, again and again: the same two geometries
        /// each time, and nothing kept from the one before.
        #[test]
        fn repeated_switches_give_the_same_geometry() {
            let mut t = Target::new(640, 480);
            for _ in 0..10 {
                for &(w, h, want) in &[(1920usize, 1080usize, (960, 540, 4)), (640, 480, (640, 480, 2))] {
                    t.set_geometry(t.geometry_for(w, h).expect("a change"));
                    assert_eq!((t.w, t.h, t.halves), want);
                    assert_eq!(t.px.len(), t.w * t.h);
                }
            }
        }

        /// A window with no area — minimised — changes nothing; a very small
        /// one is still 1× (`Math.Max(1, …)`) and a backbuffer of its size.
        #[test]
        fn a_minimised_or_tiny_window_keeps_a_usable_target() {
            let mut t = Target::new(1280, 720);
            assert_eq!(t.geometry_for(0, 0), None);
            assert_eq!(t.geometry_for(1280, 0), None);
            for &(w, h) in &[(1usize, 1usize), (100, 80), (320, 200)] {
                let g = t.geometry_for(w, h).expect("a new size");
                t.set_geometry(g);
                assert_eq!((t.w, t.h, t.halves), (w, h, 2));
                t.centre(320, 200);
                t.clear();
                // A scene larger than the game is drawn cut, not refused.
                t.fill(0, 0, 320, 200, 0xFFFFFF, 255);
                t.origin = (0, 0);
            }
        }

        /// A click on a centred 320 × 200 scene, in its own coordinates,
        /// lands where [`Target::centre`] put that scene, wherever the
        /// origin was left.
        #[test]
        fn a_centred_scene_point_is_found_in_any_window() {
            for &(w, h) in &[(640usize, 480usize), (1280, 720), (1920, 1080)] {
                let mut t = Target::new(w, h);
                t.origin = (7, -3);
                let p = t.centred_to_window((320, 200), 163, 54);
                t.centre(320, 200);
                assert_eq!(p, t.scene_to_window(163, 54));
                assert_eq!(t.window_to_scene(p.0, p.1), (163, 54));
            }
        }

        /// A line of text starts on a whole backbuffer pixel: game x 3.3 is
        /// 6.1875 device pixels, rounded to 6, which is game 3.2.
        #[test]
        fn a_line_start_snaps_to_a_backbuffer_pixel() {
            let t = target();
            let (x, y) = t.snap(Ratio::new(33, 10), Ratio::int(1));
            assert_eq!(x, Ratio::new(16, 5));
            // 2.25 → Math.Round(2.25) = 2: a tie-free case; 1 game row is 2.25.
            assert_eq!(y, Ratio::new(8, 9));
        }
    }
}
