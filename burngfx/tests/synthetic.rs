//! Tests that need no Burntime data at all: the fixtures are assembled here,
//! byte by byte, from the format description. They are what CI can run, and
//! they are what pins the decoder down when nobody has an original installation
//! at hand.

use burngfx::{decode_frame_bytes, Palette, SpriteFile};

/// A palette in `MAT_xxx.RAW` layout: 16 bytes of header, then 256 six-bit RGB
/// triples. Entry `i` is set to `(i, i, i)`, so an index is recoverable from any
/// channel of the colour it produces.
fn ramp_palette_file() -> Vec<u8> {
    let mut bytes = vec![0u8; 16 + 256 * 3];
    for i in 0..256 {
        let v = (i % 64) as u8; // stay inside the 6-bit range
        bytes[16 + i * 3] = v;
        bytes[16 + i * 3 + 1] = v;
        bytes[16 + i * 3 + 2] = v;
    }
    bytes
}

/// One frame: `u16` width, `u16` height, then `(skip, count)` pairs.
fn frame(width: u16, height: u16, body: &[u8]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&width.to_le_bytes());
    f.extend_from_slice(&height.to_le_bytes());
    f.extend_from_slice(body);
    f
}

/// End a pass. The skip byte is ignored when the count is 0xFF.
const END_PASS: [u8; 2] = [0, 0xff];

#[test]
fn palette_scales_six_bit_values_and_keeps_index_zero_transparent() {
    let pal = Palette::from_mat_raw(&ramp_palette_file()).unwrap();
    assert_eq!(pal.color(0).a, 0);
    assert_eq!(pal.color(1).a, 255);
    assert_eq!(pal.color(5).r, 20); // 5 * 4
    assert_eq!(pal.color(63).r, 252);
}

#[test]
fn a_run_fills_every_fourth_pixel_of_the_first_pass() {
    // Pass 0 writes x = 0, 4, 8, 12 — mode-X plane 0.
    let mut body = vec![0u8, 4]; // skip 0, then 4 indices
    body.extend_from_slice(&[11, 22, 33, 44]);
    body.extend_from_slice(&END_PASS);

    let img = decode_frame_bytes(&frame(16, 1, &body)).unwrap();
    assert_eq!((img.width, img.height), (16, 1));
    assert_eq!(img.indices[0], 11);
    assert_eq!(img.indices[4], 22);
    assert_eq!(img.indices[8], 33);
    assert_eq!(img.indices[12], 44);
    // Everything the pass did not address stays transparent.
    assert_eq!(img.indices[1], 0);
    assert_eq!(img.indices[2], 0);
}

#[test]
fn each_pass_starts_one_pixel_further_right() {
    let mut body = Vec::new();
    for pass in 0..4u8 {
        body.extend_from_slice(&[0, 2]); // two indices in this pass
        body.extend_from_slice(&[100 + pass, 200 + pass]);
        body.extend_from_slice(&END_PASS);
    }
    // A fifth pair must be readable but never acted on: after four terminators
    // the decoder is done.
    body.extend_from_slice(&[0, 1]);
    body.push(255);

    let img = decode_frame_bytes(&frame(8, 1, &body)).unwrap();
    for pass in 0..4usize {
        assert_eq!(img.indices[pass], 100 + pass as u8, "pass {pass} first pixel");
        assert_eq!(img.indices[pass + 4], 200 + pass as u8, "pass {pass} second pixel");
    }
}

#[test]
fn the_write_cursor_wraps_at_320_not_at_the_sprite_width() {
    // A 4-wide sprite: one pixel per line per pass, then a skip of (320-4)/4 = 79
    // to reach the next line. This is the detail the format hinges on.
    let mut body = vec![0u8, 1, 7]; // line 0: index 7 at x = 0
    body.extend_from_slice(&[79, 1, 9]); // skip to line 1, index 9 at x = 0
    body.extend_from_slice(&END_PASS);

    let img = decode_frame_bytes(&frame(4, 2, &body)).unwrap();
    assert_eq!(img.indices[0], 7, "line 0");
    assert_eq!(img.indices[4], 9, "line 1 — reached by the 320-wide skip");
}

#[test]
fn pixels_outside_the_declared_size_are_dropped_not_written() {
    // A run long enough to walk past the right edge of a narrow sprite. The
    // original silently drops those pixels; so do we, and without panicking.
    let mut body = vec![0u8, 40];
    body.extend_from_slice(&[1u8; 40]);
    body.extend_from_slice(&END_PASS);

    let img = decode_frame_bytes(&frame(8, 1, &body)).unwrap();
    assert_eq!(img.indices.len(), 8);
    assert_eq!(img.indices[0], 1);
    assert_eq!(img.indices[4], 1);
}

#[test]
fn implausible_and_truncated_frames_are_rejected() {
    assert!(decode_frame_bytes(&[]).is_none());
    assert!(decode_frame_bytes(&[1, 2]).is_none());
    assert!(decode_frame_bytes(&frame(0, 10, &[])).is_none(), "zero width");
    assert!(decode_frame_bytes(&frame(4096, 10, &[])).is_none(), "over the 2048 limit");
}

#[test]
fn the_offset_table_ends_at_the_first_zero_entry() {
    // Two frames. The table is 3 entries wide because entry 0 says so.
    let f0 = frame(4, 1, &[0, 1, 5, 0, 0xff]);
    let f1 = frame(4, 1, &[0, 1, 6, 0, 0xff]);

    let table_len = 6usize;
    let mut data = Vec::new();
    data.extend_from_slice(&(table_len as u16).to_le_bytes());
    // The reader hands a frame the bytes up to `next - 1`, so leave a spare byte
    // between frames for it to discard.
    data.extend_from_slice(&((table_len + f0.len() + 1) as u16).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // terminator
    data.extend_from_slice(&f0);
    data.push(0xAA); // the byte the -1 drops
    data.extend_from_slice(&f1);

    let sprites = SpriteFile::parse(&data);
    assert_eq!(sprites.frame_count(), 2);
    assert_eq!(sprites.decode_frame(0).unwrap().indices[0], 5);
    assert_eq!(sprites.decode_frame(1).unwrap().indices[0], 6);
    assert!(sprites.decode_frame(2).is_none());
}

#[test]
fn indices_map_through_the_palette_on_request() {
    let pal = Palette::from_mat_raw(&ramp_palette_file()).unwrap();
    let img = decode_frame_bytes(&frame(4, 1, &[0, 1, 5, 0, 0xff])).unwrap();
    let rgba = img.to_rgba(&pal);

    assert_eq!(&rgba.rgba[0..4], &[20, 20, 20, 255], "index 5 -> 5*4, opaque");
    assert_eq!(&rgba.rgba[4..8], &[0, 0, 0, 0], "index 0 -> transparent");
}
