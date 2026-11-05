//! Pick the assets the viewer compiles in.
//!
//! AROS gets the data embedded rather than loaded, so the choice has to be made
//! at build time. If an original `STA.ANI` and a palette are sitting in
//! `burngfx/testdata`, use them. Otherwise generate a small sprite in the same
//! 1993 format, so that a fresh clone — which ships no game data — still builds
//! and still exercises the whole decode-and-draw path.

use std::path::{Path, PathBuf};

const W: usize = 32;
const H: usize = 32;
const FRAMES: usize = 8;
/// The write cursor addresses a 320-pixel-wide screen; see burngfx.
const SCREEN_WIDTH: usize = 320;

fn main() {
    // "aros" is a custom target, so rustc does not know the name.
    println!("cargo::rustc-check-cfg=cfg(target_os, values(\"aros\"))");
    println!("cargo::rustc-check-cfg=cfg(synthetic_assets)");

    let testdata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata");
    let sprites = testdata.join("STA.ANI");
    let palette = testdata.join("map0-recovered.pal");
    println!("cargo:rerun-if-changed={}", sprites.display());
    println!("cargo:rerun-if-changed={}", palette.display());

    if sprites.is_file() && palette.is_file() {
        emit("BURNTIME_SPRITES", &sprites);
        emit("BURNTIME_PALETTE", &palette);
        return;
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let sprites = out.join("synthetic.ani");
    let palette = out.join("synthetic.pal");
    std::fs::write(&sprites, synthetic_sprites()).unwrap();
    std::fs::write(&palette, synthetic_palette()).unwrap();
    emit("BURNTIME_SPRITES", &sprites);
    emit("BURNTIME_PALETTE", &palette);
    println!("cargo::rustc-cfg=synthetic_assets");
}

fn emit(var: &str, path: &Path) {
    println!("cargo:rustc-env={var}={}", path.display());
}

/// 256 entries in `MAT_xxx.RAW` layout: 16 bytes of header, then six-bit RGB.
fn synthetic_palette() -> Vec<u8> {
    let mut bytes = vec![0u8; 16 + 256 * 3];
    for i in 1..256usize {
        let t = (i % 64) as u8;
        let (r, g, b) = match i / 64 {
            0 => (t, 0, 63 - t),
            1 => (63, t, 0),
            2 => (63 - t, 63, t),
            _ => (t, 63 - t, 63),
        };
        bytes[16 + i * 3] = r;
        bytes[16 + i * 3 + 1] = g;
        bytes[16 + i * 3 + 2] = b;
    }
    bytes
}

/// A sprite file: `u16` offsets, then frames. Encoding rather than hand-writing
/// the bytes keeps the fixture honest — it has to survive our own decoder.
fn synthetic_sprites() -> Vec<u8> {
    let frames: Vec<Vec<u8>> = (0..FRAMES).map(|f| encode_frame(&paint(f))).collect();

    // The table is as wide as it needs to be, plus a zero terminator.
    let table = (FRAMES + 1) * 2;
    let mut offsets = Vec::new();
    let mut at = table;
    for frame in &frames {
        offsets.push(at as u16);
        at += frame.len() + 1; // one spare byte: a frame ends at `next - 1`
    }

    let mut out = Vec::new();
    for offset in &offsets {
        out.extend_from_slice(&offset.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    for frame in &frames {
        out.extend_from_slice(frame);
        out.push(0);
    }
    out
}

/// A ring that rotates through the palette, one frame at a time.
fn paint(frame: usize) -> Vec<u8> {
    let mut px = vec![0u8; W * H];
    let centre = (W as f32 / 2.0, H as f32 / 2.0);

    for y in 0..H {
        for x in 0..W {
            let (dx, dy) = (x as f32 + 0.5 - centre.0, y as f32 + 0.5 - centre.1);
            let dist = (dx * dx + dy * dy).sqrt();
            if !(6.0..14.0).contains(&dist) {
                continue;
            }
            let angle = dy.atan2(dx); // -PI..PI
            let turn = (angle / core::f32::consts::TAU + 0.5 + frame as f32 / FRAMES as f32) % 1.0;
            px[y * W + x] = 1 + (turn * 254.0) as u8;
        }
    }
    px
}

/// Run-length encode one frame the way the 1993 files do: four mode-X passes of
/// `(skip, count)` pairs, each terminated by a count of 0xFF.
fn encode_frame(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(W as u16).to_le_bytes());
    out.extend_from_slice(&(H as u16).to_le_bytes());

    for pass in 0..4usize {
        let mut cursor = pass;

        for y in 0..H {
            // The pixels this pass owns on this line, in write order.
            let mut run = Vec::new();
            let mut x = pass;
            while x < W {
                run.push(pixels[y * W + x]);
                x += 4;
            }
            // Trailing transparent pixels cost nothing to omit.
            while run.last() == Some(&0) {
                run.pop();
            }
            if run.is_empty() {
                continue;
            }

            let target = y * SCREEN_WIDTH + pass;
            let skip = (target - cursor) / 4;
            assert!(skip <= u8::MAX as usize, "skip {skip} does not fit in a byte");
            assert!(run.len() <= 0xfe, "run of {} is too long", run.len());

            out.push(skip as u8);
            out.push(run.len() as u8);
            out.extend_from_slice(&run);
            cursor = target + 4 * run.len();
        }

        out.push(0);
        out.push(0xff);
    }
    out
}
