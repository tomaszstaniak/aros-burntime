//! Show Burntime's original 1993 sprites, decoded in Rust, drawn through SDL2.
//!
//! The point of this program is narrow and deliberate: prove that the asset
//! decoder and a pixel pipeline both work on AROS, using the same source that
//! runs on the host. So the assets are compiled in rather than loaded — file IO
//! is a separate question, and mixing the two would blur which one failed.
//!
//! Host:  cargo run -p viewer
//! AROS:  cargo run --release          (from this directory; see .cargo/config.toml)

#![cfg_attr(target_os = "aros", no_std)]
#![cfg_attr(target_os = "aros", no_main)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use burngfx::{Palette, SpriteFile};
use sdlmini::Screen;

/// The 1993 screen. Everything in the game is laid out for it.
const W: usize = 320;
const H: usize = 200;
const SCALE: i32 = 3;
const RUN_MS: u32 = 20_000;
const MS_PER_FRAME: u32 = 120;

// Chosen by build.rs: an original STA.ANI and palette when they are present,
// otherwise a sprite generated in the same format so a fresh clone still builds.
const SPRITES: &[u8] = include_bytes!(env!("BURNTIME_SPRITES"));
const PALETTE: &[u8] = include_bytes!(env!("BURNTIME_PALETTE"));

fn run() {
    #[cfg(synthetic_assets)]
    println_(b"no original assets found - drawing a generated sprite instead\n");

    let palette = match Palette::from_mat_raw(PALETTE) {
        Some(p) => p,
        None => {
            println_(b"palette file too short\n");
            return;
        }
    };

    let sprites = SpriteFile::parse(SPRITES);
    let frames: Vec<_> = (0..sprites.frame_count())
        .filter_map(|i| sprites.decode_frame(i))
        .map(|f| f.to_rgba(&palette))
        .collect();

    if frames.is_empty() {
        println_(b"no frames decoded\n");
        return;
    }

    let mut screen = match Screen::open(b"Burntime assets\0", W, H, W as i32 * SCALE, H as i32 * SCALE) {
        Ok(s) => s,
        Err(step) => {
            // `step` says which call failed; SDL says why.
            println_(step.as_bytes());
            println_(sdlmini::error().as_str().as_bytes());
            return;
        }
    };

    let mut canvas = vec![0u32; W * H];
    let start = sdlmini::ticks();

    loop {
        let elapsed = sdlmini::ticks().wrapping_sub(start);
        if elapsed > RUN_MS || screen.should_quit() {
            break;
        }

        // Dark blue-grey, so transparent pixels are visibly transparent rather
        // than ambiguously black.
        canvas.fill(0xFF10_1820);

        // Top row: every frame side by side, at 1:1.
        let mut x = 4;
        for frame in &frames {
            blit(&mut canvas, frame, x, 8);
            x += frame.width as usize + 2;
            if x + 32 > W {
                break;
            }
        }

        // Below it: one frame at a time, tripled, cycling — the animation the
        // file actually describes.
        let current = &frames[(elapsed / MS_PER_FRAME) as usize % frames.len()];
        blit_scaled(&mut canvas, current, 120, 70, 3);

        screen.present(&canvas);
    }
}

/// Copy an RGBA image onto the ARGB canvas, skipping fully transparent pixels.
fn blit(canvas: &mut [u32], img: &burngfx::Image, at_x: usize, at_y: usize) {
    for y in 0..img.height as usize {
        for x in 0..img.width as usize {
            let px = &img.rgba[(y * img.width as usize + x) * 4..][..4];
            if px[3] == 0 {
                continue;
            }
            let (cx, cy) = (at_x + x, at_y + y);
            if cx < W && cy < H {
                canvas[cy * W + cx] = argb(px);
            }
        }
    }
}

fn blit_scaled(canvas: &mut [u32], img: &burngfx::Image, at_x: usize, at_y: usize, scale: usize) {
    for y in 0..img.height as usize * scale {
        for x in 0..img.width as usize * scale {
            let src = ((y / scale) * img.width as usize + x / scale) * 4;
            let px = &img.rgba[src..src + 4];
            if px[3] == 0 {
                continue;
            }
            let (cx, cy) = (at_x + x, at_y + y);
            if cx < W && cy < H {
                canvas[cy * W + cx] = argb(px);
            }
        }
    }
}

fn argb(px: &[u8]) -> u32 {
    0xFF00_0000 | (px[0] as u32) << 16 | (px[1] as u32) << 8 | px[2] as u32
}

// -- entry points -----------------------------------------------------------

#[cfg(target_os = "aros")]
fn println_(msg: &[u8]) {
    aros::println!("{}", core::str::from_utf8(msg).unwrap_or("?"));
}

#[cfg(not(target_os = "aros"))]
fn println_(msg: &[u8]) {
    println!("{}", core::str::from_utf8(msg).unwrap_or("?"));
}

#[cfg(target_os = "aros")]
aros::aros_main!(run);

#[cfg(not(target_os = "aros"))]
fn main() {
    run();
}
