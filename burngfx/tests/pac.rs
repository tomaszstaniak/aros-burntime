//! `*.PAC`: the room interiors, and everything else drawn full screen.
//!
//! A PAC is a stream of *differences* to a running colour index, so a single
//! misread delta does not spoil one pixel — it shifts every pixel after it to
//! the end of the picture. That makes the synthetic cases below worth having
//! one command at a time, and makes the real-data check unusually strong: nine
//! independent files whose pixel streams all end within three bytes of the
//! same distance from the end cannot all be drifting.

use burngfx::pac::{self, PacError, DELTAS, HEIGHT, WIDTH};
use burngfx::pak::ClassicPak;
use std::path::PathBuf;

const PIXELS: usize = WIDTH * HEIGHT;

/// A file: fifteen header deltas, the commands under test, then enough runs of
/// the current colour to fill the picture.
fn pac(deltas: [u8; DELTAS], commands: &[u8], produced: usize) -> Vec<u8> {
    let mut out = Vec::from(deltas);
    out.extend_from_slice(commands);
    // `cmd 3` counts downwards: 0xC0 is sixty four pixels.
    let mut left = PIXELS - produced;
    while left > 0 {
        let run = left.min(64);
        out.push(0xC0 | (0x3F - (run - 1)) as u8);
        left -= run;
    }
    out
}

#[test]
fn a_plain_run_adds_one_byte_per_pixel() {
    // cmd 0, three pixels: +10, +20, +5.
    let bytes = pac([0; DELTAS], &[0x02, 10, 20, 5], 3);
    let p = pac::decode(&bytes).unwrap();
    assert_eq!(&p.indices[..4], &[10, 30, 35, 35], "the fourth is the fill run");
    assert_eq!(p.indices.len(), PIXELS);
}

/// The count is `+ 1`, so a command byte of zero still produces a pixel.
#[test]
fn a_length_of_zero_still_means_one_pixel() {
    let p = pac::decode(&pac([0; DELTAS], &[0x00, 7], 1)).unwrap();
    assert_eq!(&p.indices[..2], &[7, 7]);
}

/// A run repeats the colour already current, and its length is counted
/// backwards — 0x3F is one pixel, 0x00 is sixty four.
#[test]
fn a_run_is_counted_downwards_from_sixty_four() {
    // Set the colour to 9, then a run of four.
    let bytes = pac([0; DELTAS], &[0x00, 9, 0xC0 | (0x3F - 3)], 5);
    let p = pac::decode(&bytes).unwrap();
    assert_eq!(&p.indices[..5], &[9, 9, 9, 9, 9]);

    // 0xC0 exactly is the longest run there is.
    let bytes = pac([0; DELTAS], &[0x00, 3, 0xC0], 65);
    let p = pac::decode(&bytes).unwrap();
    assert!(p.indices[..65].iter().all(|&i| i == 3), "sixty four more of the same");
}

/// The packed form: a nibble a pixel. 1-15 pick a header delta; 0 means a
/// whole byte, and those bytes come after *all* the nibbles rather than inline.
#[test]
fn packed_pixels_take_their_deltas_from_the_header() {
    let mut deltas = [0u8; DELTAS];
    deltas[0] = 5; // nibble 1
    deltas[2] = 1; // nibble 3

    // Four pixels: nibbles 1, 3, 0, 1 -> two nibble bytes, one wide byte.
    let bytes = pac(deltas, &[0x40 | 3, 0x13, 0x01, 100], 4);
    let p = pac::decode(&bytes).unwrap();
    assert_eq!(&p.indices[..4], &[5, 6, 106, 111]);
}

/// An odd number of nibbles still costs a whole byte, and the spare low nibble
/// is not a pixel.
#[test]
fn an_odd_run_of_nibbles_rounds_up_to_a_whole_byte() {
    let mut deltas = [0u8; DELTAS];
    deltas[0] = 2;

    // Three pixels from two nibble bytes; the fourth nibble is padding.
    let bytes = pac(deltas, &[0x40 | 2, 0x11, 0x10], 3);
    let p = pac::decode(&bytes).unwrap();
    assert_eq!(&p.indices[..3], &[2, 4, 6]);
    // If the padding nibble had been read as a pixel the fill would start
    // from 8 rather than 6.
    assert_eq!(p.indices[3], 6);
}

/// A header delta from 0x80 up is negative. Reading it unsigned sends the
/// colour index the wrong way and never recovers.
#[test]
fn header_deltas_are_signed() {
    let mut deltas = [0u8; DELTAS];
    deltas[0] = 10;
    deltas[1] = 0xF0; // -16

    let p = pac::decode(&pac(deltas, &[], 0)).unwrap();
    assert_eq!(p.deltas[0], 10);
    assert_eq!(p.deltas[1], -16);
    assert_eq!(p.deltas[14], 0);

    // 100, then -16 twice.
    let bytes = pac(deltas, &[0x00, 100, 0x40 | 1, 0x22], 3);
    let p = pac::decode(&bytes).unwrap();
    assert_eq!(&p.indices[..3], &[100, 84, 68]);
}

/// The index wraps within a byte, and the two forms wrap differently: a plain
/// byte is never negative, so only the packed form can go below zero.
#[test]
fn the_colour_index_wraps_both_ways() {
    let mut deltas = [0u8; DELTAS];
    deltas[0] = 0xFF; // -1

    // 200 + 100 = 300 -> 44.
    let p = pac::decode(&pac([0; DELTAS], &[0x01, 200, 100], 2)).unwrap();
    assert_eq!(&p.indices[..2], &[200, 44]);

    // 0 - 1 -> 255.
    let p = pac::decode(&pac(deltas, &[0x40, 0x10], 1)).unwrap();
    assert_eq!(p.indices[0], 255);
}

/// `cmd 2` consumes its byte and produces nothing. Not a run of zero pixels,
/// and not a reason to stop.
#[test]
fn the_second_command_does_nothing_at_all() {
    let plain = pac::decode(&pac([0; DELTAS], &[0x00, 42], 1)).unwrap();
    let with_nops = pac::decode(&pac([0; DELTAS], &[0x80, 0x00, 42, 0xBF], 1)).unwrap();
    assert_eq!(plain.indices, with_nops.indices);
    // It cost two bytes, and the offsets say so.
    assert_eq!(with_nops.table_offset, plain.table_offset + 2);
}

#[test]
fn a_stream_that_stops_early_is_refused() {
    assert_eq!(pac::decode(&[]).unwrap_err(), PacError::Truncated);
    assert_eq!(pac::decode(&[0u8; DELTAS - 1]).unwrap_err(), PacError::Truncated);
    // A header and nothing else cannot fill a picture.
    assert_eq!(pac::decode(&[0u8; DELTAS]).unwrap_err(), PacError::Truncated);
    // A command whose data runs off the end.
    let mut bytes = vec![0u8; DELTAS];
    bytes.extend_from_slice(&[0x05, 1, 2]);
    assert_eq!(pac::decode(&bytes).unwrap_err(), PacError::Truncated);
}

/// A command that would overrun the last pixel fills to the end and stops,
/// rather than writing past it.
#[test]
fn the_last_command_is_clipped_to_the_picture() {
    let mut bytes = Vec::from([0u8; DELTAS]);
    // Fill the whole picture, then ask for sixty four more.
    let mut left = PIXELS;
    while left > 0 {
        let run = left.min(64);
        bytes.push(0xC0 | (0x3F - (run - 1)) as u8);
        left -= run;
    }
    let p = pac::decode(&bytes).unwrap();
    assert_eq!(p.indices.len(), PIXELS);
    assert_eq!(p.table_offset, bytes.len(), "and no colour table follows");
    assert!(!p.has_colour_table(bytes.len()));
}

fn pak() -> Option<Vec<u8>> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join("burntime.pak"))
        .ok()
}

/// Every room interior in the archive decodes, and stops in the same place.
///
/// The strong part is the trailer. A stream that drifted would end wherever it
/// happened to; these nine end between 774 and 777 bytes from the end of their
/// files, which is the embedded colour table the original also expects to find
/// there (its own commented-out arithmetic says `length - 0x305`).
#[test]
fn every_room_interior_decodes_and_leaves_the_same_trailer() {
    let Some(bytes) = pak() else {
        eprintln!("skipping: no burntime.pak — see testdata/README.md");
        return;
    };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut rooms = 0;
    for n in 0..16u8 {
        let Some(data) = pak.get(&format!("burn_gfx/raum_{n}.pac")) else { continue };
        let p = pac::decode(data)
            .unwrap_or_else(|e| panic!("raum_{n}.pac: {e:?} after {} bytes", data.len()));

        assert_eq!(p.indices.len(), PIXELS);
        let trailer = data.len() - p.table_offset;
        assert!(
            (770..=780).contains(&trailer),
            "raum_{n}.pac: {trailer} bytes left over — the stream drifted"
        );
        assert!(p.has_colour_table(data.len()), "raum_{n}.pac should carry a colour table");

        // Real artwork rather than a flat fill, and not so busy that every
        // index appears — either would suggest the deltas are being misread.
        let distinct: std::collections::HashSet<u8> = p.indices.iter().copied().collect();
        assert!(
            (8..200).contains(&distinct.len()),
            "raum_{n}.pac has {} distinct indices",
            distinct.len()
        );
        rooms += 1;
    }
    assert!(rooms >= 9, "only decoded {rooms} room interiors");
}

/// The other PACs in the archive are the same format, and decode with the same
/// code and no special case.
#[test]
fn the_other_full_screen_pictures_decode_too() {
    let Some(bytes) = pak() else { return };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut seen = 0;
    let mut failed: Vec<String> = Vec::new();
    for entry in pak.entries() {
        if !entry.name.ends_with(".pac") || entry.name.contains("raum_") {
            continue;
        }
        seen += 1;
        if pac::decode(pak.data(entry)).is_err() {
            failed.push(entry.name.clone());
        }
    }
    assert!(seen > 0, "the archive holds no other .pac files");
    assert!(failed.is_empty(), "{} of {seen} would not decode: {failed:?}", failed.len());
}

/// The colour table a PAC carries after its pixels.
///
/// There is no rendered oracle for these — the remaster ships PNGs for sprites
/// and tiles and none for the full-screen pictures — so this leans on the one
/// independent thing available: the game's map palettes. The two are different
/// files in different formats decoded by different code, and if this decoder
/// works they have to agree.
mod colours {
    use super::*;
    use burngfx::Palette;

    /// The eight slots a room is allowed to have to itself.
    const ROOM_RAMP: std::ops::Range<u8> = 176..184;

    fn map_palette(pak: &ClassicPak, n: u32) -> Palette {
        Palette::from_mat_raw(pak.get(&format!("burn_gfx/mat_{n:03}.raw")).unwrap()).unwrap()
    }

    /// Every room's table decodes and lands on the end of its own file.
    ///
    /// The stream is told nothing about how long it is: it stops after 255
    /// colours and happens to have consumed the trailer. Nine files agreeing
    /// on that to within three bytes is the check.
    #[test]
    fn each_table_consumes_the_trailer_it_was_given() {
        let Some(bytes) = pak() else { return };
        let pak = ClassicPak::open(&bytes).unwrap();

        let mut rooms = 0;
        for n in 0..9u8 {
            let data = pak.get(&format!("burn_gfx/raum_{n}.pac")).unwrap();
            let p = pac::decode(data).unwrap();
            let (_, used) = pac::colour_table(data, &p)
                .unwrap_or_else(|| panic!("raum_{n}.pac: no colour table"));

            let trailer = data.len() - p.table_offset;
            let left = trailer as i64 - used as i64;
            assert!(
                (0..=4).contains(&left),
                "raum_{n}.pac: read {used} of {trailer} trailer bytes, {left} left over"
            );
            rooms += 1;
        }
        assert!(rooms >= 9);
    }

    /// Six-bit DAC values scaled by four, like every other palette here — so
    /// every component is a multiple of four and none exceeds 252.
    #[test]
    fn every_component_stays_inside_the_dac_range() {
        let Some(bytes) = pak() else { return };
        let pak = ClassicPak::open(&bytes).unwrap();

        for n in 0..9u8 {
            let data = pak.get(&format!("burn_gfx/raum_{n}.pac")).unwrap();
            let p = pac::decode(data).unwrap();
            let (palette, _) = pac::colour_table(data, &p).unwrap();
            for i in 0..=255u8 {
                let c = palette.color(i);
                for (name, v) in [("r", c.r), ("g", c.g), ("b", c.b)] {
                    assert!(v <= 252, "raum_{n}.pac colour {i}: {name} = {v}");
                    assert_eq!(v % 4, 0, "raum_{n}.pac colour {i}: {name} = {v} is not 6-bit");
                }
            }
            // Opaque, unlike a sprite palette: a room is a background and
            // nothing may show through it. `PixelColor(0, 0, 0)` upstream.
            let zero = palette.color(0);
            assert_eq!(
                (zero.r, zero.g, zero.b, zero.a),
                (0, 0, 0, 255),
                "raum_{n}.pac: index 0 must be opaque black"
            );
        }
    }

    /// The load-bearing check, and the reason to believe any of this.
    ///
    /// The game's palette is very nearly one palette: map0 and map5 differ in
    /// eight entries of 256. A room's embedded table is decoded from a
    /// different file, in a different encoding, by different code — and comes
    /// out identical to it everywhere except the eight slots a room is allowed
    /// to have to itself. Nothing but a correct decoder produces that.
    #[test]
    fn a_rooms_table_is_the_games_palette_apart_from_its_own_eight_slots() {
        let Some(bytes) = pak() else { return };
        let pak = ClassicPak::open(&bytes).unwrap();
        let base = map_palette(&pak, 1);

        // First: the premise. If the map palettes ever stop being nearly the
        // same, this test is comparing against something else and should say
        // so rather than quietly passing.
        for m in [0, 5, 17, 33] {
            let other = map_palette(&pak, m);
            let differ = (0..=255u8).filter(|&i| base.color(i) != other.color(i)).count();
            assert!(differ <= 16, "map1 and map{m} differ in {differ} entries");
        }

        let mut rooms_with_own_colours = 0;
        for n in 0..9u8 {
            let data = pak.get(&format!("burn_gfx/raum_{n}.pac")).unwrap();
            let p = pac::decode(data).unwrap();
            let (palette, _) = pac::colour_table(data, &p).unwrap();

            let differing: Vec<u8> =
                (1..=255u8).filter(|&i| palette.color(i) != base.color(i)).collect();
            assert!(
                differing.iter().all(|i| ROOM_RAMP.contains(i)),
                "raum_{n}.pac differs from the map palette outside its own slots: {differing:?}"
            );
            if !differing.is_empty() {
                assert_eq!(differing.len(), 8, "raum_{n}.pac: a partial ramp");
                rooms_with_own_colours += 1;
            }
        }
        assert!(
            rooms_with_own_colours >= 4,
            "only {rooms_with_own_colours} rooms carry colours of their own — if none did, \
             this test would pass while proving nothing"
        );
    }

    /// And those slots are not decoration: at least one room paints with them.
    #[test]
    fn a_room_actually_uses_the_colours_it_carries() {
        let Some(bytes) = pak() else { return };
        let pak = ClassicPak::open(&bytes).unwrap();
        let base = map_palette(&pak, 1);

        let mut painted = 0;
        for n in 0..9u8 {
            let data = pak.get(&format!("burn_gfx/raum_{n}.pac")).unwrap();
            let p = pac::decode(data).unwrap();
            let (palette, _) = pac::colour_table(data, &p).unwrap();

            let used: std::collections::HashSet<u8> = p.indices.iter().copied().collect();
            let own = ROOM_RAMP
                .filter(|&i| palette.color(i) != base.color(i) && used.contains(&i))
                .count();
            if own > 0 {
                painted += 1;
                // A ramp, not eight unrelated colours: consecutive entries
                // step by a little, never by a lot.
                for i in 177..184u8 {
                    let (a, b) = (palette.color(i - 1), palette.color(i));
                    let step = (a.r as i32 - b.r as i32).abs().max(
                        (a.g as i32 - b.g as i32).abs().max((a.b as i32 - b.b as i32).abs()),
                    );
                    assert!(step <= 32, "raum_{n}.pac {}->{i} jumps by {step}", i - 1);
                }
            }
        }
        assert!(painted > 0, "no room paints with its own colours — the ramp would be pointless");
    }
}
