//! Check the decoder against renders of the same frames produced by Burntime's
//! own ripper.
//!
//! These tests need files this repository does not ship (see `testdata/README.md`)
//! and skip themselves when the files are absent, so a clone without them still
//! has a green `cargo test` from `synthetic.rs`.
//!
//! What the comparison can and cannot establish, stated plainly:
//!
//! - **Transparency is exact.** Index 0 and nothing else is transparent, which
//!   is true whatever the palette, so the mask comparison is a real per-pixel
//!   check of where every pixel landed.
//! - **The colours are consistent, not verified.** `STA.ANI`'s palette lives in
//!   `MAT_000.RAW`, which only an original installation has. Decoded index plus
//!   reference colour reconstructs a palette; a misplaced pixel whose index
//!   differs from its neighbour's would normally collide. Swapping two pixels
//!   that share an index would not. Together with the exact masks this is strong
//!   evidence, not proof — full proof needs an RGBA comparison against the real
//!   `MAT_000.RAW`.

use burngfx::{Palette, SpriteFile};
use std::collections::HashMap;
use std::path::PathBuf;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(name)
}

/// Returns None, with a note, when the originals are not present.
fn sta_ani() -> Option<Vec<u8>> {
    let path = testdata("STA.ANI");
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping: no {} — see testdata/README.md", path.display());
            None
        }
    }
}

fn reference(index: usize) -> Option<(u32, u32, Vec<u8>)> {
    let path = testdata(&format!("ref-STA/{index}.png"));
    let file = std::fs::File::open(&path).ok()?;
    let mut reader = png::Decoder::new(file).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    assert_eq!(info.color_type, png::ColorType::Rgba);
    buf.truncate(info.buffer_size());
    Some((info.width, info.height, buf))
}

#[test]
fn frame_count_matches_reference_set() {
    let Some(sta) = sta_ani() else { return };
    assert_eq!(SpriteFile::parse(&sta).frame_count(), 21);
}

#[test]
fn map_file_carries_a_six_bit_palette() {
    let Ok(mat) = std::fs::read(testdata("MAT_090.RAW")) else {
        eprintln!("skipping: no MAT_090.RAW — see testdata/README.md");
        return;
    };
    let pal = Palette::from_mat_raw(&mat).expect("MAT_090.RAW should carry a palette");
    assert_eq!(pal.color(0).a, 0, "index 0 is the transparent colour");
    assert_eq!(pal.color(1).a, 255);
    // 0x00,0x14,0x1c scaled by 4 — the first real entry in MAT_090.RAW.
    let c = pal.color(1);
    assert_eq!((c.r, c.g, c.b), (0, 80, 112));
}

/// Palette-independent, and therefore an exact per-pixel check.
#[test]
fn transparency_mask_matches_every_reference_frame() {
    let Some(sta) = sta_ani() else { return };
    let sprites = SpriteFile::parse(&sta);
    let mut checked = 0;

    for i in 0..sprites.frame_count() {
        let Some((w, h, want)) = reference(i) else { continue };
        let got = sprites.decode_frame(i).unwrap_or_else(|| panic!("frame {i} failed to decode"));
        assert_eq!((got.width, got.height), (w, h), "frame {i}: size");

        let mismatches = got
            .indices
            .iter()
            .zip(want.chunks(4))
            .filter(|(&index, px)| (index == 0) != (px[3] == 0))
            .count();
        assert_eq!(mismatches, 0, "frame {i}: transparency differs in {mismatches} pixels");
        checked += 1;
    }
    assert_eq!(checked, 21, "expected all 21 reference frames");
}

/// Strong evidence rather than proof — see the module comment.
#[test]
fn indices_reconstruct_a_single_consistent_palette() {
    let Some(sta) = sta_ani() else { return };
    let sprites = SpriteFile::parse(&sta);
    let mut palette: HashMap<u8, [u8; 3]> = HashMap::new();
    let mut samples = 0usize;

    for i in 0..sprites.frame_count() {
        let Some((_, _, want)) = reference(i) else { continue };
        let got = sprites.decode_frame(i).unwrap();

        for (&index, px) in got.indices.iter().zip(want.chunks(4)) {
            if index == 0 {
                continue;
            }
            samples += 1;
            let color = [px[0], px[1], px[2]];
            match palette.get(&index) {
                Some(&seen) => assert_eq!(seen, color, "index {index} decodes to two different colours"),
                None => {
                    palette.insert(index, color);
                }
            }
        }
    }

    // One colliding index would still "pass" if it were only ever seen once, so
    // require real coverage before believing the consistency means anything.
    assert!(samples > 10_000, "only {samples} opaque pixels sampled");
    assert!(palette.len() > 40, "only {} palette entries recovered", palette.len());

    for (index, c) in &palette {
        assert!(
            c.iter().all(|v| v % 4 == 0),
            "index {index} -> {c:?} is not a 6-bit VGA colour"
        );
    }
}
