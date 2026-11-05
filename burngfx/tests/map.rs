//! `.burnmap` parsing.
//!
//! The synthetic cases build their fixtures here from the format description
//! and need no game data. The one test that reads a real map skips itself when
//! the file is absent.

use burngfx::map::{self, MapError, TileSet};
use std::path::PathBuf;

/// A .NET `BinaryWriter` string: 7-bit-encoded byte length, then the bytes.
fn dotnet_string(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut len = s.len();
    while len >= 0x80 {
        out.push((len as u8) | 0x80);
        len >>= 7;
    }
    out.push(len as u8);
    out.extend_from_slice(s.as_bytes());
    out
}

struct MapBuilder {
    bytes: Vec<u8>,
}

impl MapBuilder {
    /// A well-formed header with the given size and tile sets.
    fn new(width: i32, height: i32, tile_size: i32, sets: &[&str]) -> Self {
        let mut bytes = Vec::new();
        bytes.extend(dotnet_string("Burntime Map"));
        bytes.extend(dotnet_string("0.2"));
        for v in [width, height, tile_size, sets.len() as i32] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for s in sets {
            bytes.extend(dotnet_string(s));
        }
        MapBuilder { bytes }
    }

    fn tile(mut self, item: u8, section: u8, set: u8) -> Self {
        self.bytes.extend_from_slice(&[item, section, set]);
        self
    }

    fn entrances(mut self, rects: &[[i32; 4]]) -> Self {
        self.bytes.push(rects.len() as u8);
        for r in rects {
            for v in r {
                self.bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        self
    }

    fn done(self) -> Vec<u8> {
        self.bytes
    }
}

#[test]
fn reads_header_tiles_and_entrances() {
    let bytes = MapBuilder::new(2, 2, 32, &["classic", "_"])
        .tile(48, 2, 0) // classic, zei_002 entry 48
        .tile(0, 0, 0) // empty
        .tile(5, 1, 1) // the map's own sheet
        .tile(7, 3, 0)
        .entrances(&[[10, 20, 30, 40]])
        .done();

    let m = map::parse(&bytes).unwrap();
    assert_eq!((m.width, m.height, m.tile_size), (2, 2, 32));
    assert_eq!(m.set_names, ["classic", "_"]);
    assert_eq!(m.pixel_size(), (64, 64));

    assert_eq!(m.tile(0, 0).set, TileSet::Classic);
    assert_eq!(m.tile(0, 0).classic_name(), Some((2, 48)));
    assert!(m.tile(1, 0).is_empty());
    assert_eq!(m.tile(0, 1).set, TileSet::Custom);
    assert_eq!(m.tile(1, 1).classic_name(), Some((3, 7)));

    assert_eq!(m.entrances.len(), 1);
    assert_eq!((m.entrances[0].left, m.entrances[0].height), (10, 40));
}

/// Section 0 means "no tile"; the other two bytes are then meaningless and
/// must not be mistaken for a real tile at item 0 of set 0.
#[test]
fn section_zero_means_empty_whatever_the_other_bytes_say() {
    let bytes = MapBuilder::new(1, 1, 32, &["classic"])
        .tile(9, 0, 0)
        .entrances(&[])
        .done();
    let m = map::parse(&bytes).unwrap();
    assert!(m.tile(0, 0).is_empty());
    assert_eq!(m.tile(0, 0).classic_name(), None);
    assert_eq!(m.tile(0, 0).custom_sheet_index(), None);
}

/// The custom sheet is a flat run of cells banded by section, 63 to a band.
#[test]
fn custom_tiles_index_into_the_maps_own_sheet() {
    let bytes = MapBuilder::new(3, 1, 32, &["classic", "_"])
        .tile(0, 1, 1)
        .tile(5, 1, 1)
        .tile(2, 2, 1)
        .entrances(&[])
        .done();
    let m = map::parse(&bytes).unwrap();
    assert_eq!(m.tile(0, 0).custom_sheet_index(), Some(0));
    assert_eq!(m.tile(1, 0).custom_sheet_index(), Some(5));
    assert_eq!(m.tile(2, 0).custom_sheet_index(), Some(65)); // (2-1)*63 + 2
}

/// A set name we do not know keeps its index rather than being guessed at.
#[test]
fn unknown_tile_sets_are_preserved_not_guessed() {
    let bytes = MapBuilder::new(1, 1, 32, &["something-else"])
        .tile(1, 1, 0)
        .entrances(&[])
        .done();
    let m = map::parse(&bytes).unwrap();
    assert_eq!(m.tile(0, 0).set, TileSet::Other(0));
    assert_eq!(m.tile(0, 0).classic_name(), None);
}

#[test]
fn rejects_files_that_are_not_maps() {
    assert_eq!(map::parse(&[]).unwrap_err(), MapError::Truncated);
    assert_eq!(map::parse(&dotnet_string("Something Else")).unwrap_err(), MapError::NotAMap);

    let mut wrong_version = dotnet_string("Burntime Map");
    wrong_version.extend(dotnet_string("0.1"));
    assert_eq!(map::parse(&wrong_version).unwrap_err(), MapError::UnsupportedVersion);
}

#[test]
fn rejects_implausible_dimensions_and_truncation() {
    let huge = MapBuilder::new(9999, 2, 32, &[]).done();
    assert_eq!(map::parse(&huge).unwrap_err(), MapError::ImplausibleSize);

    // Header promises four tiles; only one is present.
    let short = MapBuilder::new(2, 2, 32, &["classic"]).tile(1, 1, 0).done();
    assert_eq!(map::parse(&short).unwrap_err(), MapError::Truncated);
}

#[test]
fn reads_the_shipped_mat_000() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/maps/MAT_000.burnmap");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: no {} — see testdata/README.md", path.display());
        return;
    };

    let m = map::parse(&bytes).expect("MAT_000 should parse");
    assert_eq!((m.width, m.height, m.tile_size), (15, 12, 32));
    assert_eq!(m.pixel_size(), (480, 384));
    assert_eq!(m.set_names, ["classic", "_"]);

    // Every cell is filled; this is a world map, not a sparse layer.
    assert!(m.tiles.iter().all(|t| !t.is_empty()), "MAT_000 has no empty cells");

    let classic = m.tiles.iter().filter(|t| t.set == TileSet::Classic).count();
    let custom = m.tiles.iter().filter(|t| t.set == TileSet::Custom).count();
    assert_eq!(classic + custom, 15 * 12);
    assert_eq!((classic, custom), (169, 11));

    // 38 entrances, not the 39 rooms MAT_000.txt declares. The two files are
    // not in lockstep: the settings file describes room0..room38 while the
    // binary carries 38 rectangles, and the original only ever looks up
    // `room<i>` for an entrance it actually has, so the last one is unused.
    assert_eq!(m.entrances.len(), 38);
    assert!(m.entrances.iter().all(|e| e.width > 0 && e.height > 0));
}
