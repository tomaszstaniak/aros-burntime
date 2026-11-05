//! Decoders for Burntime's original 1993 asset formats.
//!
//! Ported from `Burntime.BurnGfx` (MIT, Jakob Harder) — specifically
//! `RawImageFileReader.cs`, `SpriteProcessor/ItemImage.cs` and `ColorTable.cs`.
//! The port is deliberately faithful rather than tidy: where the original has a
//! quirk, the quirk is reproduced and commented, because the only thing that
//! makes a format decoder correct is agreeing with the files that already exist.
//!
//! `no_std` + `alloc`, no dependencies — the same source builds for the host and
//! for AROS.

#![cfg_attr(not(test), no_std)]

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

pub mod amiga_ai;
pub mod population;
pub mod burial;
pub mod cheat;
pub mod clock;
pub mod classic_ai;
pub mod combat;
pub mod complex_path;
pub mod config;
pub mod construction;
pub mod danger;
pub mod diner;
pub mod dialog;
pub mod encounter;
pub mod doctor;
pub mod font;
pub mod gait;
pub mod game;
pub mod gamdat;
pub mod hire;
pub mod input;
pub mod intro;
pub mod items;
pub mod settings;
pub mod slots;
pub mod table_combat;
pub mod targets;
pub mod spawn;
pub mod start;
pub mod manual_path;
pub mod manual_window;
pub mod map;
pub mod mapscene;
pub mod music_rules;
pub mod matmap;
pub mod menu;
pub mod mind;
pub mod net_random;
pub mod newgfx;
pub mod modern_ai;
pub mod pac;
pub mod player_colors;
pub mod pack;
pub mod production;
pub mod prompts;
pub mod prompt_sets;
pub mod pak;
pub mod ratio;
pub mod rival;
pub mod rivals;
pub mod rules;
pub mod save;
pub mod scenes;
pub mod scene_blend;
pub mod text;
pub mod trade;
pub mod trader;
pub mod tradescene;
pub mod turns;
pub mod v12_day;
pub mod v12_economy;
pub mod v12_items;
pub mod view;
pub mod zei;

/// Sprite data is addressed as a region of a 320-pixel-wide VGA screen, not as a
/// tight w*h buffer. A 32-wide sprite therefore stores 8 pixels per pass per
/// line and then skips (320-32)/4 = 72 to reach the next line. This constant is
/// the reason `x` and `y` are derived from a single position counter.
const SCREEN_WIDTH: usize = 320;

/// Straight RGBA, 8 bits per channel. (The C# original keeps BGRA; callers there
/// hand it to Direct3D. We convert once, at the point of upload, instead.)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// A 256-entry VGA palette.
#[derive(Clone)]
pub struct Palette {
    colors: [Rgba; 256],
}

impl Palette {
    /// Build one from colours somebody else decoded, e.g. the table a `.PAC`
    /// carries after its pixels. Index 0 is transparent whatever is passed.
    pub fn from_colors(colors: [Rgba; 256]) -> Palette {
        Palette { colors }
    }

    /// Read the palette out of a `MAT_xxx.RAW` map file.
    ///
    /// These files do double duty: the game loads every `MAT_000.RAW` ..
    /// `MAT_037.RAW` purely to register its palette under the name `map<n>`, and
    /// `map0` becomes the default palette for everything that has no entry of its
    /// own. The palette is 256 RGB triples at offset 16, in 6-bit VGA range.
    pub fn from_mat_raw(bytes: &[u8]) -> Option<Palette> {
        const OFFSET: usize = 16;
        if bytes.len() < OFFSET + 3 {
            return None;
        }

        let mut colors = [Rgba::default(); 256];
        for (i, slot) in colors.iter_mut().enumerate() {
            let at = OFFSET + i * 3;
            if at + 3 > bytes.len() {
                break;
            }

            // 6-bit DAC values scaled to 8 bits. The original guards against
            // out-of-range triples by emitting black; a file with 8-bit values
            // would otherwise decode as noise.
            let (mut r, mut g, mut b) = (
                bytes[at] as u16 * 4,
                bytes[at + 1] as u16 * 4,
                bytes[at + 2] as u16 * 4,
            );
            if r >= 256 || g >= 256 || b >= 256 {
                r = 0;
                g = 0;
                b = 0;
            }

            // Index 0 is the transparent colour, always.
            *slot = Rgba { r: r as u8, g: g as u8, b: b as u8, a: if i == 0 { 0 } else { 255 } };
        }
        Some(Palette { colors })
    }

    pub fn color(&self, index: u8) -> Rgba {
        self.colors[index as usize]
    }
}

/// A decoded frame, still in palette indices.
///
/// This is the honest output of the format: the file stores indices, and which
/// palette applies is a separate decision the caller makes (see
/// `Palette::from_mat_raw`). Index 0 means transparent, and is also what an
/// untouched pixel holds.
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    /// `width * height` palette indices.
    pub indices: Vec<u8>,
}

impl IndexedImage {
    pub fn to_rgba(&self, palette: &Palette) -> Image {
        let mut rgba = vec![0u8; self.indices.len() * 4];
        for (i, &index) in self.indices.iter().enumerate() {
            let c = palette.color(index);
            rgba[i * 4] = c.r;
            rgba[i * 4 + 1] = c.g;
            rgba[i * 4 + 2] = c.b;
            rgba[i * 4 + 3] = c.a;
        }
        Image { width: self.width, height: self.height, rgba }
    }
}

/// A decoded frame with a palette applied.
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, RGBA.
    pub rgba: Vec<u8>,
}

/// A multi-frame sprite file (`.ANI`, and the sprite banks inside `.RAW`).
pub struct SpriteFile<'a> {
    data: &'a [u8],
    offsets: Vec<u32>,
}

impl<'a> SpriteFile<'a> {
    /// Parse the offset table at the head of the file.
    ///
    /// The table is a list of little-endian u16 file offsets. Its own length is
    /// implied by its first entry — entry 0 points at the first frame, so
    /// everything before that is table. It ends early at the first zero entry, or
    /// at the first entry that points past the end of the file.
    pub fn parse(data: &'a [u8]) -> SpriteFile<'a> {
        let mut end_read = 0x1000usize;
        let mut count = 0usize;

        let mut i = 0usize;
        while i < end_read / 2 {
            let at = i * 2;
            if at + 2 > data.len() {
                break;
            }
            let h = u16::from_le_bytes([data[at], data[at + 1]]) as usize;

            if i == 0 {
                end_read = h;
            }
            if h == 0 && count == 0 {
                count = i;
                break;
            }
            if h >= data.len() {
                // The original logs "frame out of index" here and truncates.
                count = i;
                break;
            }
            i += 1;
        }

        let offsets = (0..count)
            .map(|i| u16::from_le_bytes([data[i * 2], data[i * 2 + 1]]) as u32)
            .collect();
        SpriteFile { data, offsets }
    }

    pub fn frame_count(&self) -> usize {
        self.offsets.len()
    }

    /// Decode one frame to palette indices. Returns `None` for an implausible
    /// frame size, matching the original's 2048-pixel sanity check.
    pub fn decode_frame(&self, index: usize) -> Option<IndexedImage> {
        let start = *self.offsets.get(index)? as usize;

        // Frame extents come from the next offset. Note the `- 1`: the original's
        // GetSubFile() takes (start, end) and the caller passes `next - 1`, so the
        // byte immediately before the next frame is not part of this one.
        let end = match self.offsets.get(index + 1) {
            Some(&next) => (next as usize).saturating_sub(1),
            None => self.data.len(),
        };
        if end <= start || end > self.data.len() {
            return None;
        }

        decode_frame_bytes(&self.data[start..end])
    }
}

/// Decode one run-length-encoded, plane-interleaved frame.
///
/// Layout: `u16 width`, `u16 height`, then pairs of bytes `(skip, count)`.
/// `count == 0xFF` ends the current pass; there are four passes, one per VGA
/// mode-X plane, each starting at pixel offset `pass`. Otherwise the write
/// position advances by `4 * skip` and `count` palette indices follow.
pub fn decode_frame_bytes(frame: &[u8]) -> Option<IndexedImage> {
    if frame.len() < 4 {
        return None;
    }
    let width = u16::from_le_bytes([frame[0], frame[1]]) as usize;
    let height = u16::from_le_bytes([frame[2], frame[3]]) as usize;
    if width > 2048 || height > 2048 || width == 0 || height == 0 {
        return None;
    }

    let mut indices = vec![0u8; width * height];
    let mut cur = 4usize;
    let mut pos = 0usize;
    let mut pass = 0usize;

    // The original reads the first pair *before* testing for end-of-file, and
    // tests again only at the top of the loop — so a pair that lands exactly on
    // the end of the frame is read and then dropped. Kept as-is.
    let mut us = match read_pair(frame, &mut cur) {
        Some(p) => p,
        None => {
            return Some(IndexedImage { width: width as u32, height: height as u32, indices })
        }
    };

    while cur < frame.len() && pass != 4 {
        if us[1] == 0xff {
            pass += 1;
            pos = pass;
        } else {
            pos += 4 * us[0] as usize;
            for _ in 0..us[1] {
                if cur >= frame.len() {
                    break;
                }
                let value = frame[cur];
                cur += 1;
                set_pixel(&mut indices, width, height, pos, value);
                pos += 4;
            }
        }

        us = match read_pair(frame, &mut cur) {
            Some(p) => p,
            None => break,
        };
    }

    Some(IndexedImage { width: width as u32, height: height as u32, indices })
}

fn read_pair(frame: &[u8], cur: &mut usize) -> Option<[u8; 2]> {
    if *cur + 2 > frame.len() {
        return None;
    }
    let p = [frame[*cur], frame[*cur + 1]];
    *cur += 2;
    Some(p)
}

fn set_pixel(indices: &mut [u8], width: usize, height: usize, pos: usize, value: u8) {
    let x = pos % SCREEN_WIDTH;
    let y = pos / SCREEN_WIDTH;
    if x >= width || y >= height {
        return;
    }
    indices[y * width + x] = value;
}
