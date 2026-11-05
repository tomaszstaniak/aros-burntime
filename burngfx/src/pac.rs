//! `*.PAC` — the full-screen pictures, including the room interiors.
//!
//! `RAUM_0.PAC` upwards are what you see on walking into a room, and
//! [`crate::matmap::Door::background`] says which one. The title screen and the
//! intro pages are the same format.
//!
//! Nothing like the sprites. A PAC is 320x200 palette indices produced by a
//! stream of *differences*: there is a running colour index, and every command
//! nudges it. That is why a room decodes to something recognisable even with
//! the wrong palette, and to noise if a single delta is misread: one wrong
//! delta shifts every pixel after it. That makes most mistakes loud, but it is
//! not a proof of correctness — two wrong deltas can cancel out, and a wrong
//! value need not change how many bytes the stream consumes.
//!
//! ```text
//! 15 bytes   the fifteen commonest deltas, signed
//! commands   until 320*200 pixels have been produced
//! (colours)  an embedded colour table, when the file has not ended
//! ```
//!
//! A command byte's top two bits choose what follows:
//!
//! | `cmd` | meaning |
//! |---|---|
//! | 0 | `(n & 0x3f) + 1` pixels, each an unsigned byte added to the index |
//! | 1 | `(n & 0x3f) + 1` pixels, each a nibble: 1-15 pick a header delta, 0 means a whole byte follows in a second block |
//! | 2 | nothing at all |
//! | 3 | `0x3f - (n & 0x3f) + 1` pixels of the colour already current |
//!
//! Ported from `Burntime.BurnGfx/PacImageFileReader.cs`. The original decodes
//! straight to colours and runs the whole stream twice — once to find where
//! the colour table starts, then again to draw with it. This produces indices,
//! which are palette-independent, so once is enough.

use alloc::vec;
use alloc::vec::Vec;

use crate::IndexedImage;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 200;
/// The deltas addressed by a nibble in a `cmd 1` block.
pub const DELTAS: usize = 15;

#[derive(Debug, PartialEq, Eq)]
pub enum PacError {
    /// The stream ended before the picture was full.
    Truncated,
}

#[derive(Debug)]
pub struct Pac {
    /// `WIDTH * HEIGHT` palette indices.
    pub indices: Vec<u8>,
    /// The fifteen deltas from the header, signed.
    pub deltas: [i16; DELTAS],
    /// Where the pixel stream stopped. Equal to the file's length when there
    /// is no embedded colour table, and the original uses exactly that
    /// comparison to decide whether one is there.
    pub table_offset: usize,
}

impl Pac {
    pub fn has_colour_table(&self, file_len: usize) -> bool {
        self.table_offset != file_len
    }

    pub fn to_indexed(&self) -> IndexedImage {
        IndexedImage {
            width: WIDTH as u32,
            height: HEIGHT as u32,
            indices: self.indices.clone(),
        }
    }
}

pub fn decode(bytes: &[u8]) -> Result<Pac, PacError> {
    if bytes.len() < DELTAS {
        return Err(PacError::Truncated);
    }

    // Signed, and the original sign-extends by hand: anything from 0x80 up is
    // negative. A delta of 0xF0 is -16, not 240, and reading it unsigned walks
    // the colour index the wrong way for the rest of the picture.
    let mut deltas = [0i16; DELTAS];
    for (i, slot) in deltas.iter_mut().enumerate() {
        let b = bytes[i] as i16;
        *slot = if b >= 0x80 { b - 0x100 } else { b };
    }

    let mut indices = vec![0u8; WIDTH * HEIGHT];
    let mut at = DELTAS;
    let mut pos = 0usize;
    // The running colour index every command moves. It is not reset anywhere.
    let mut colour = 0i32;

    while pos < WIDTH * HEIGHT {
        let data = *bytes.get(at).ok_or(PacError::Truncated)?;
        at += 1;

        match (data & 0xC0) >> 6 {
            // Plain: one byte a pixel, added to the index.
            0 => {
                for _ in 0..(data & 0x3F) as usize + 1 {
                    let step = *bytes.get(at).ok_or(PacError::Truncated)?;
                    at += 1;
                    colour += step as i32;
                    if colour > 255 {
                        colour -= 256;
                    }
                    indices[pos] = colour as u8;
                    pos += 1;
                    if pos == WIDTH * HEIGHT {
                        break;
                    }
                }
            }

            // Packed: a nibble a pixel, in two blocks. The nibbles come first,
            // then — only for the nibbles that were zero, counted in advance —
            // the whole bytes they stand in for.
            1 => {
                let n = (data & 0x3F) as usize + 1;
                let nibble_bytes = n.div_ceil(2);
                let nibbles = bytes.get(at..at + nibble_bytes).ok_or(PacError::Truncated)?;
                at += nibble_bytes;

                let nibble = |i: usize| -> u8 {
                    if i.is_multiple_of(2) {
                        nibbles[i / 2] >> 4
                    } else {
                        nibbles[i / 2] & 0x0F
                    }
                };

                let wide = (0..n).filter(|&i| nibble(i) == 0).count();
                let extra = bytes.get(at..at + wide).ok_or(PacError::Truncated)?;
                at += wide;

                let mut taken = 0usize;
                for i in 0..n {
                    let d = nibble(i);
                    let step = if d == 0 {
                        let b = extra[taken] as i32;
                        taken += 1;
                        b
                    } else {
                        deltas[d as usize - 1] as i32
                    };
                    colour += step;
                    // Both ways here, because a header delta can be negative
                    // where a plain byte never is.
                    if colour < 0 {
                        colour += 256;
                    }
                    if colour > 255 {
                        colour -= 256;
                    }
                    indices[pos] = colour as u8;
                    pos += 1;
                    if pos == WIDTH * HEIGHT {
                        break;
                    }
                }
            }

            // Nothing. Not a run of zero pixels and not an error: the byte is
            // consumed and the picture is no further on.
            2 => {}

            // A run of the colour already current. Counted downwards, so 0x3F
            // means one pixel and 0x00 means sixty four.
            _ => {
                let run = 0x3F - (data & 0x3F) as usize + 1;
                let end = (pos + run).min(WIDTH * HEIGHT);
                indices[pos..end].fill(colour as u8);
                pos = end;
            }
        }
    }

    Ok(Pac { indices, deltas, table_offset: at })
}

/// The colour table a PAC carries after its pixels.
///
/// 255 colours — index 0 is always black and is not stored — each three 6-bit
/// components, compressed by the same idea as the pixels: a running value and
/// a stream of differences, reusing the fifteen header deltas.
///
/// This is a faithful port of `ColorTable.FromPac`, dead branches removed. Two
/// of them are provably unreachable rather than merely unused: its `mode` flag
/// is initialised true and never set false, so the whole second half of the
/// loop never runs; and its `data == 0x42` case is caught by the bit-stream
/// test above it, since `0x42 >> 4` is 4.
///
/// **There is no oracle for this.** The remaster ships rendered PNGs for
/// sprites and tiles but none for the full-screen pictures, so nothing here is
/// checked against a picture somebody else drew. What is checked is that every
/// component comes out inside the 6-bit range the VGA DAC takes, that the
/// stream consumes very nearly all of the trailer it was given, and that the
/// result is a plausible palette rather than a flat or wildly noisy one. Those
/// are properties, not proof.
pub fn colour_table(bytes: &[u8], pac: &Pac) -> Option<(crate::Palette, usize)> {
    // The table starts at the first 0x22 at or after the pixel stream. The
    // original scans for it too, rather than trusting the offset it just
    // computed.
    let mut base = pac.table_offset;
    while *bytes.get(base)? != 0x22 {
        base += 1;
    }

    let get = |at: usize| -> Option<i32> { bytes.get(base + at).map(|&b| b as i32) };

    let mut colours = [crate::Rgba::default(); 256];
    // Index 0 is not stored: the original writes it black without reading a
    // byte for it, and `PixelColor(0, 0, 0)` is opaque. Opaque here too. The
    // transparent index 0 the sprite palettes use is a sprite convention, and
    // carrying it over to a full-screen picture would let whatever the canvas
    // held before show through the black parts of a room.
    colours[0] = crate::Rgba { r: 0, g: 0, b: 0, a: 255 };

    let mut active = 0i32;
    let mut pos = 0usize;
    let (mut run, mut leftcount, mut streampixel, mut ignore) = (0i32, 0i32, 0i32, 0i32);
    let (mut streamoff, mut streamdata, mut streamdatapos) = (0i32, 0usize, 0usize);

    for i in 0..255usize {
        let mut colour = [0i32; 3];
        // `c` steps backwards in two branches so the same component is tried
        // again with new state, which is why this is not a `for`.
        let mut c: i32 = 0;
        while c < 3 {
            let at = c.max(0) as usize;
            colour[at] = active;

            if run > 0 {
                run -= 1;
            } else if leftcount > 0 {
                leftcount -= 1;
                colour[at] = (get(pos)? + colour[at]) % 0x40;
                pos += 1;
            } else if streampixel > 0 {
                // A nibble a component: 1-15 pick a header delta, 0 means a
                // whole byte from a second block further along.
                let sd = if streamoff == 0 {
                    streamoff = 1;
                    get(pos)? >> 4
                } else {
                    let v = get(pos)? & 0x0F;
                    pos += 1;
                    streamoff = 0;
                    v
                };
                if sd != 0 {
                    colour[at] = (colour[at] + pac.deltas[sd as usize - 1] as i32) % 0x40;
                    if colour[at] < 0 {
                        colour[at] += 0x40;
                    }
                } else {
                    colour[at] = (colour[at] + get(streamdatapos + streamdata)?) % 0x40;
                    streamdata += 1;
                }
                streampixel -= 1;
                if streampixel == 0 && ignore == 0 {
                    pos += streamdata;
                }
            } else if ignore >= 1 {
                ignore -= 1;
                pos += 1 + streamdata;
                c -= 1;
            } else {
                let data = get(pos)?;
                pos += 1;
                if data >= 0xF0 {
                    // Repeat the value already current, 0 to 15 more times.
                    run = 0x100 - data - 1;
                } else if data >> 4 >= 4 {
                    // Open a nibble stream. An odd length leaves a spare
                    // nibble, which `ignore` skips past afterwards.
                    streampixel = data - 0x3F;
                    ignore = streampixel % 2;
                    streamoff = 0;
                    streamdatapos = pos + ((streampixel + ignore) / 2) as usize;
                    streamdata = 0;
                    c -= 1;
                } else if data == 0 {
                    colour[at] = (get(pos)? + colour[at]) % 0x40;
                    pos += 1;
                } else {
                    // Anything else is a count of components to read plainly,
                    // starting with this one.
                    leftcount = data;
                    colour[at] = (get(pos)? + colour[at]) % 0x40;
                    pos += 1;
                }
            }

            if c >= 0 {
                active = colour[c as usize];
            }
            c += 1;
        }

        // Six-bit DAC values, scaled the way every other palette here is.
        colours[i + 1] = crate::Rgba {
            r: (colour[0] * 4) as u8,
            g: (colour[1] * 4) as u8,
            b: (colour[2] * 4) as u8,
            a: 255,
        };
    }

    Some((crate::Palette::from_colors(colours), pos))
}
