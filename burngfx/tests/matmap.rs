//! `mat_NNN.raw` doors: the 1993 source for what kind of room lies behind one.
//!
//! The synthetic cases build a file byte by byte. The real ones read every
//! classic map out of `burntime.pak` and check the room types against the
//! remaster's own `.txt` files — which is the point of the exercise, because
//! those exist for three maps and the raws exist for all of them. Where both
//! are present they must agree; where only the raw is, it is now readable.

use burngfx::map;
use burngfx::matmap::{self, MatError, RoomType, TILES_AT};
use burngfx::pak::ClassicPak;
use burngfx::text::MapSettings;
use std::path::PathBuf;

fn testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(rel)
}

/// A map of `w` x `h` tiles with the given doors appended.
fn build(w: usize, h: usize, doors: &[(u16, u16, u16, u16, u16, u8)]) -> Vec<u8> {
    let mut out = vec![0u8; matmap::doors_at(w, h)];
    out[0..2].copy_from_slice(&((w * 2) as u16).to_le_bytes());
    out[2..4].copy_from_slice(&(h as u16).to_le_bytes());
    out[8..10].copy_from_slice(&(matmap::doors_at(w, h) as u16).to_le_bytes());

    for &(id, x, y, dw, dh, room) in doors {
        let mut r = [0u8; 20];
        r[0..2].copy_from_slice(&id.to_le_bytes());
        r[2..4].copy_from_slice(&x.to_le_bytes());
        r[4..6].copy_from_slice(&y.to_le_bytes());
        r[6..8].copy_from_slice(&dw.to_le_bytes());
        r[8..10].copy_from_slice(&dh.to_le_bytes());
        r[12] = room;
        out.extend_from_slice(&r);
    }
    // The terminator: one more record whose x is zero.
    out.extend_from_slice(&[0u8; 20]);
    out
}

#[test]
fn the_width_field_is_bytes_per_row_not_tiles() {
    let m = matmap::parse(&build(12, 10, &[])).unwrap();
    assert_eq!((m.width, m.height), (12, 10));
    // Which is where the door table lands: header, palette, an unknown block,
    // then two bytes a tile.
    assert_eq!(TILES_AT, 16 + 768 + 256);
    assert_eq!(matmap::doors_at(12, 10), TILES_AT + 240);
}

#[test]
fn doors_are_read_until_one_has_no_x() {
    let m = matmap::parse(&build(4, 4, &[(1, 10, 20, 8, 6, 0), (2, 30, 40, 9, 7, 5)])).unwrap();
    assert_eq!(m.doors.len(), 2);

    assert_eq!(m.doors[0].id, 1);
    assert_eq!(m.doors[0].area, map::Entrance { left: 10, top: 20, width: 8, height: 6 });
    assert_eq!(m.doors[0].kind, RoomType::Normal);

    assert_eq!(m.doors[1].area.left, 30);
    assert_eq!(m.doors[1].kind, RoomType::WaterSource);
    assert_eq!(m.doors[1].title_index(), 665, "660 + the room id");
    assert_eq!(m.doors[1].background(), 5);
}

/// A door with x == 0 ends the list, so anything after it is padding rather
/// than a door that happens to sit against the left edge.
#[test]
fn a_zero_x_ends_the_list_even_with_bytes_left() {
    let mut bytes = build(4, 4, &[(1, 10, 20, 8, 6, 0)]);
    // Append a perfectly well-formed door *after* the terminator.
    let mut extra = [0u8; 20];
    extra[0..2].copy_from_slice(&9u16.to_le_bytes());
    extra[2..4].copy_from_slice(&99u16.to_le_bytes());
    bytes.extend_from_slice(&extra);

    let m = matmap::parse(&bytes).unwrap();
    assert_eq!(m.doors.len(), 1, "reading must stop at the terminator");
}

/// The ladder in `MapProcessor` is a list of magic numbers, not ranges with a
/// pattern — two separate ids are traders, two more are pubs. Pinned exactly,
/// because a tidier-looking version would be wrong in ways nothing else here
/// would catch.
#[test]
fn the_room_id_ladder_matches_the_original() {
    use RoomType::*;
    let expected = [
        (0, Normal), (1, Normal), (2, Normal), (3, Normal),
        (4, WaterSource), (5, WaterSource), (8, WaterSource),
        (9, Rope),
        (10, Scene), (11, Scene), (12, Scene),
        (13, Trader), (17, Trader),
        (14, Pub), (23, Pub),
        (15, Scene),
        (16, Church),
        (20, Doctor),
        (22, Restaurant), (24, Restaurant),
        (25, Scene), (255, Scene),
    ];
    for (id, want) in expected {
        assert_eq!(RoomType::from_room_id(id), want, "room id {id}");
    }
}

/// A rope room is the one entry rule that differs, and the difference is large:
/// you reach it from five times as far.
#[test]
fn a_rope_room_is_entered_from_much_further_away() {
    assert_eq!(RoomType::Normal.entry_reach(), 15);
    assert_eq!(RoomType::WaterSource.entry_reach(), 15);
    assert_eq!(RoomType::Rope.entry_reach(), 75);

    // And only rooms you stand in hold items.
    assert_eq!(RoomType::Normal.item_capacity(), 32);
    assert_eq!(RoomType::Rope.item_capacity(), 32);
    assert_eq!(RoomType::WaterSource.item_capacity(), 8);
    assert_eq!(RoomType::Trader.item_capacity(), 0);
    assert_eq!(RoomType::Scene.item_capacity(), 0);
}

#[test]
fn a_file_too_short_to_hold_a_grid_is_refused() {
    assert_eq!(matmap::parse(&[]).unwrap_err(), MatError::Truncated);
    assert_eq!(matmap::parse(&vec![0u8; TILES_AT - 1]).unwrap_err(), MatError::Truncated);

    // A plausible header whose grid runs off the end.
    let mut bytes = vec![0u8; TILES_AT + 8];
    bytes[0..2].copy_from_slice(&200u16.to_le_bytes());
    bytes[2..4].copy_from_slice(&200u16.to_le_bytes());
    assert_eq!(matmap::parse(&bytes).unwrap_err(), MatError::Truncated);

    // Zero tiles is not a map.
    let bytes = vec![0u8; TILES_AT + 64];
    assert_eq!(matmap::parse(&bytes).unwrap_err(), MatError::ImplausibleSize(0, 0));
}

fn pak() -> Option<Vec<u8>> {
    std::fs::read(testdata("burntime.pak")).ok()
}

/// Every classic map parses, and the door table lands where the header says.
///
/// Upstream reads the doors by walking past the tiles and never consults the
/// offset it stored in the header. If the two ever disagreed, one of the two
/// readings would be wrong and nothing upstream would notice — so check.
#[test]
fn every_classic_map_has_doors_where_its_header_says() {
    let Some(bytes) = pak() else {
        eprintln!("skipping: no burntime.pak — see testdata/README.md");
        return;
    };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut maps = 0;
    let mut doors = 0;
    let mut empty = 0;
    for i in 0..=37u32 {
        let Some(raw) = pak.get(&format!("burn_gfx/mat_{i:03}.raw")) else { continue };
        let m = matmap::parse(raw).unwrap_or_else(|e| panic!("mat_{i:03}: {e:?}"));

        let stated = u16::from_le_bytes([raw[8], raw[9]]) as usize;
        assert_eq!(
            stated,
            matmap::doors_at(m.width, m.height),
            "mat_{i:03}: header says doors at {stated}, the grid ends at {}",
            matmap::doors_at(m.width, m.height)
        );

        // Outback (mat_028) genuinely has none — it is empty desert, and the
        // .burnmap agrees. So this counts rather than asserting.
        // Door ids are the original's own, counted from one and in order —
        // `Map.GetDoor(id)` indexes `doors[id - 1]` and trusts it.
        for (k, d) in m.doors.iter().enumerate() {
            assert_eq!(d.id as usize, k + 1, "mat_{i:03}: door {k} is numbered {}", d.id);
            assert!(d.area.width > 0 && d.area.height > 0, "mat_{i:03}: empty door {k}");
        }
        maps += 1;
        doors += m.doors.len();
        if m.doors.is_empty() {
            empty += 1;
        }
    }
    assert!(maps >= 37, "only found {maps} classic maps");
    assert!(doors > 100, "only {doors} doors across {maps} maps");
    assert_eq!(empty, 1, "exactly one location has no rooms at all; found {empty}");
}

/// The whole reason for reading these: where a `.txt` exists it agrees with
/// its raw about what *kind* of room each door is — so the 35 maps with no
/// `.txt` can be trusted to the raw alone.
///
/// Titles are a different matter and deliberately not compared. `BurnmapProcessor`
/// fills the entrance from the raw and then lets the `.txt` override the title,
/// which means the two are *expected* to differ: Muarab's spring is room id 4,
/// `burn?664`, "House with water", and `MAT_005.txt` corrects it to `burn?665`,
/// "Shack with water" — which is what the artwork shows. The raw is the
/// original and the `.txt` is the remaster's correction to it; neither is a
/// check on the other.
#[test]
fn the_shipped_txt_files_agree_with_the_raw_about_room_kinds() {
    let Some(bytes) = pak() else { return };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut checked = 0;
    for i in 0..=37u32 {
        let path = testdata(&format!("maps/MAT_{i:03}.txt"));
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Some(raw) = pak.get(&format!("burn_gfx/mat_{i:03}.raw")) else { continue };

        let settings = MapSettings::parse(&text);
        let m = matmap::parse(raw).unwrap();

        for (k, door) in m.doors.iter().enumerate() {
            let Some(room) = settings.room(k) else { continue };
            if room.kind.is_empty() {
                // MAT_000.txt states no room types at all — its rooms are the
                // locations, and its titles are their names. Only its `type`
                // and `image` fields are blank, which is the half that the raw
                // supplies.
                continue;
            }
            assert_eq!(
                door.kind == RoomType::WaterSource,
                room.kind == "water",
                "MAT_{i:03} room {k}: the .txt says {:?}, the raw says {:?} (room id {})",
                room.kind,
                door.kind,
                door.room_id
            );
            checked += 1;
        }
    }
    // MAT_005 is the only shipped .txt that states room types at all: MAT_000's
    // are generated blanks, and MAT_038 is the remaster's own location with no
    // classic raw behind it. Four rooms is the whole of the overlap.
    assert!(checked >= 4, "only compared {checked} rooms — are the .txt files present?");
}

/// The raw's doors and the `.burnmap`'s entrances are the same doors in the
/// same order — which is the whole basis for attaching a room type to an
/// entrance by index, so it is worth pinning rather than assuming.
///
/// They are emphatically *not* in the same places. The remaster redrew several
/// location maps larger: MAT_033 is 15x6 tiles in the raw and 20x8 in the
/// `.burnmap`, and its doors moved about 158 pixels right along with the
/// scenery. `BurnmapProcessor` copies the raw's entrance and then overwrites
/// `Area` from the burnmap, so the drawn map always wins on position.
///
/// What survives the redrawing is the vertical placement — every door sits
/// within a few pixels of where it was — and the order. Those are what this
/// checks.
#[test]
fn the_doors_and_the_burnmap_entrances_are_the_same_doors_in_order() {
    let Some(bytes) = pak() else { return };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut compared = 0;
    let mut redrawn = 0;
    for i in 0..=37u32 {
        let Ok(bm) = std::fs::read(testdata(&format!("maps/MAT_{i:03}.burnmap"))) else { continue };
        let Some(raw) = pak.get(&format!("burn_gfx/mat_{i:03}.raw")) else { continue };
        let (Ok(burnmap), Ok(classic)) = (map::parse(&bm), matmap::parse(raw)) else { continue };

        // The .burnmap may carry more: the world map has 38 entrances to the
        // raw's 37, because the remaster added a location of its own. It never
        // carries fewer.
        assert!(
            burnmap.entrances.len() >= classic.doors.len(),
            "MAT_{i:03}: {} entrances in the .burnmap, {} doors in the raw",
            burnmap.entrances.len(),
            classic.doors.len()
        );

        if (burnmap.width, burnmap.height) != (classic.width, classic.height) {
            redrawn += 1;
        }

        for (k, (e, d)) in burnmap.entrances.iter().zip(&classic.doors).enumerate() {
            let dy = (e.top - d.area.top).abs();
            assert!(
                dy <= 8,
                "MAT_{i:03} door {k}: the raw has it at y={} and the .burnmap at y={} — \
                 if the order no longer lines up, room types are being attached to the \
                 wrong doors",
                d.area.top,
                e.top
            );
        }
        compared += 1;
    }
    assert!(compared >= 30, "only compared {compared} maps");
    assert!(redrawn > 0, "no map was redrawn — has the .burnmap set changed?");
}

// -- which picture a room shows ---------------------------------------------

/// Only the rooms you walk around in have a `raum_N.pac` at all.
///
/// `LocationScene` sends a trader, pub, doctor, church or restaurant to a
/// scene of its own and passes the room id on as `InventoryBackground` — which
/// is why a pub is 14 and a restaurant 22, numbers no `raum_` file has. Asking
/// for one would be asking for a file that is not there.
#[test]
fn only_walk_in_rooms_have_an_interior() {
    for id in 0..=255u8 {
        let want = match matmap::RoomType::from_room_id(id) {
            RoomType::Normal | RoomType::WaterSource => Some(id),
            // Every rope shows the same cave, whatever its id: the original
            // hard-codes `InventoryBackground = 3` for them.
            RoomType::Rope => Some(3),
            _ => None,
        };
        assert_eq!(matmap::interior_of(id), want, "room id {id}");
    }

    // And the ids that do reach a file are exactly the nine that exist.
    let reachable: Vec<u8> =
        (0..=255u8).filter_map(matmap::interior_of).collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
    assert_eq!(reachable, (0..=8u8).collect::<Vec<_>>());
}

/// Every door in the archive asks for a picture that is actually in it, and
/// the ones that ask for nothing are the room kinds with a scene of their own.
#[test]
fn every_door_in_the_archive_asks_for_a_file_that_exists() {
    let Ok(bytes) = std::fs::read(testdata("burntime.pak")) else { return };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut wanted = std::collections::BTreeSet::new();
    let (mut with, mut without) = (0, 0);
    for i in 0..=37u32 {
        let Some(raw) = pak.get(&format!("burn_gfx/mat_{i:03}.raw")) else { continue };
        let Ok(classic) = matmap::parse(raw) else { continue };
        for d in &classic.doors {
            match d.interior() {
                Some(n) => {
                    assert!(
                        pak.contains(&format!("burn_gfx/raum_{n}.pac")),
                        "MAT_{i:03} door {}: wants raum_{n}.pac, which is not in the archive",
                        d.id
                    );
                    wanted.insert(n);
                    with += 1;
                }
                None => {
                    assert!(
                        !matches!(d.kind, RoomType::Normal | RoomType::WaterSource | RoomType::Rope),
                        "MAT_{i:03} door {}: a {:?} room with no picture",
                        d.id,
                        d.kind
                    );
                    without += 1;
                }
            }
        }
    }

    assert!(with > 100, "only {with} doors have an interior");
    assert!(without > 0, "no door in the whole world is a scene of its own");
    // All nine files are reached by the world as shipped; a change that leaves
    // one unreachable is a change worth noticing.
    assert_eq!(wanted, (0..=8u8).collect::<std::collections::BTreeSet<_>>());
}

/// The nine interiors are nine different pictures.
///
/// The reason to check: a room is drawn from whichever file its id names, and
/// a decoder that returned the same buffer for two ids — or a cache that
/// failed to clear — would show the previous room and look like nothing at
/// all. Distinctness at the data level is what makes that impossible.
#[test]
fn the_nine_interiors_are_nine_different_pictures() {
    let Ok(bytes) = std::fs::read(testdata("burntime.pak")) else { return };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut seen: Vec<(u8, Vec<u8>)> = Vec::new();
    for n in 0..=8u8 {
        let data = pak.get(&format!("burn_gfx/raum_{n}.pac")).unwrap();
        let pac = burngfx::pac::decode(data).unwrap();
        for (m, other) in &seen {
            assert_ne!(&pac.indices, other, "raum_{n}.pac and raum_{m}.pac are the same picture");
        }
        seen.push((n, pac.indices));
    }
    assert_eq!(seen.len(), 9);
}

/// The left of every interior is blank, and that is not a decoding fault.
///
/// It is where the character's inventory goes: `InventoryScene` puts its
/// panel at (2, 5) and the room's own item grid at x=160 or 170, so the
/// artist left the left of the picture empty for the panel to sit on. A
/// screenshot of a room with nothing in that band is showing the data as it
/// is, not losing a third of the picture.
#[test]
fn the_left_of_an_interior_is_left_blank_for_the_inventory() {
    let Ok(bytes) = std::fs::read(testdata("burntime.pak")) else { return };
    let pak = ClassicPak::open(&bytes).unwrap();

    for n in 0..=8u8 {
        let data = pak.get(&format!("burn_gfx/raum_{n}.pac")).unwrap();
        let pac = burngfx::pac::decode(data).unwrap();
        let w = burngfx::pac::WIDTH;

        let painted = |x: usize| (0..burngfx::pac::HEIGHT).any(|y| pac.indices[y * w + x] != 0);
        assert!(
            (0..100).all(|x| !painted(x)),
            "raum_{n}.pac paints inside the inventory panel's own hundred columns"
        );
        // And the rest is a picture rather than more of the same nothing.
        let painted_columns = (100..w).filter(|&x| painted(x)).count();
        assert!(painted_columns > 150, "raum_{n}.pac only paints {painted_columns} columns");
    }
}

/// The intro's maps have no `.burnmap`; their tiles come out of the raw as
/// `MapProcessor` reads them: a `u16` a tile, section in the low byte, item
/// in the high.
#[test]
fn tiles_are_section_low_item_high() {
    let mut bytes = build(2, 1, &[]);
    bytes[TILES_AT..TILES_AT + 4].copy_from_slice(&[48, 40, 1, 54]);
    let (w, h, tiles) = matmap::tiles(&bytes).unwrap();
    assert_eq!((w, h), (2, 1));
    assert_eq!(tiles, [(48, 40), (1, 54)]);
}

/// The intro's pages that the archive carries (all but `mat_090.raw`, which
/// the remaster ships beside it): 288 wide but for the two that scroll.
#[test]
fn the_intro_maps_in_the_archive() {
    let Ok(bytes) = std::fs::read(testdata("burntime.pak")) else { return };
    let pak = ClassicPak::open(&bytes).unwrap();
    for (id, w) in [(80, 15), (81, 9), (82, 9), (83, 19), (84, 9), (89, 9)] {
        let (tw, th, tiles) = matmap::tiles(pak.get(&format!("burn_gfx/mat_{id:03}.raw")).unwrap()).unwrap();
        assert_eq!((tw, th), (w, 6), "map {id}");
        assert_eq!(tiles.len(), w * 6);
    }
    assert!(pak.get("burn_gfx/mat_090.raw").is_none());
}
