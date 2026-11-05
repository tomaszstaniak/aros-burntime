//! The remaster's `burntime.pak`, which carries the classic game data.
//!
//! The synthetic cases build archives byte by byte. One test opens the real
//! file when it is present and skips otherwise, because that file belongs to
//! an installation of the game and is not ours to ship.

use burngfx::pak::{ClassicPak, PakError};
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

fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut table = Vec::new();
    let mut position = 0usize;
    for (name, data) in entries {
        table.extend_from_slice(&(position as i32).to_le_bytes());
        table.extend_from_slice(&(data.len() as i32).to_le_bytes());
        table.extend(dotnet_string(name));
        position += data.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as i32).to_le_bytes());
    out.extend_from_slice(&10i32.to_le_bytes()); // version
    out.extend_from_slice(&0i32.to_le_bytes()); // type: Main
    out.extend_from_slice(&table);
    for (_, data) in entries {
        out.extend_from_slice(data);
    }
    out
}

#[test]
fn reads_entries_at_offsets_relative_to_the_table() {
    let bytes = build(&[
        ("burn_gfx/gam.dat", b"gamdat"),
        ("burn_gfx/zei_014.raw", b"tiles!"),
        ("lang/en/burn.txt", b"New Sandez}"),
    ]);

    let pak = ClassicPak::open(&bytes).unwrap();
    assert_eq!(pak.len(), 3);
    assert_eq!(pak.version, 10);
    assert_eq!(pak.kind, 0);

    assert_eq!(pak.get("burn_gfx/gam.dat"), Some(b"gamdat".as_slice()));
    assert_eq!(pak.get("burn_gfx/zei_014.raw"), Some(b"tiles!".as_slice()));
    assert_eq!(pak.get("lang/en/burn.txt"), Some(b"New Sandez}".as_slice()));

    // The archive lower-cases what it stores, so lookups ignore case.
    assert_eq!(pak.get("BURN_GFX/GAM.DAT"), Some(b"gamdat".as_slice()));

    assert!(pak.contains("burn_gfx/gam.dat"));
    assert_eq!(pak.get("nothing"), None);
    // A prefix must not match.
    assert_eq!(pak.get("burn_gfx/gam"), None);
}

#[test]
fn an_empty_archive_is_valid() {
    let bytes = build(&[]);
    let pak = ClassicPak::open(&bytes).unwrap();
    assert!(pak.is_empty());
    assert_eq!(pak.get("anything"), None);
}

#[test]
fn refuses_a_truncated_header_or_table() {
    assert_eq!(ClassicPak::open(&[]).unwrap_err(), PakError::Malformed);
    assert_eq!(ClassicPak::open(&[0; 8]).unwrap_err(), PakError::Malformed);

    // Claims one entry, then stops.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1i32.to_le_bytes());
    bytes.extend_from_slice(&10i32.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    assert_eq!(ClassicPak::open(&bytes).unwrap_err(), PakError::Malformed);
}

#[test]
fn refuses_counts_and_offsets_that_cannot_be_right() {
    let mut bytes = build(&[("a", b"x")]);
    bytes[..4].copy_from_slice(&i32::MAX.to_le_bytes());
    assert_eq!(ClassicPak::open(&bytes).unwrap_err(), PakError::Implausible);

    let mut bytes = build(&[("a", b"x")]);
    bytes[..4].copy_from_slice(&(-1i32).to_le_bytes());
    assert_eq!(ClassicPak::open(&bytes).unwrap_err(), PakError::Implausible);

    // A negative position or length.
    let mut bytes = build(&[("a", b"x")]);
    bytes[12..16].copy_from_slice(&(-8i32).to_le_bytes());
    assert_eq!(ClassicPak::open(&bytes).unwrap_err(), PakError::Implausible);
}

#[test]
fn refuses_entries_that_reach_past_the_end() {
    let mut bytes = build(&[("a", b"xyz")]);
    // Length far beyond the file.
    bytes[16..20].copy_from_slice(&9999i32.to_le_bytes());
    assert_eq!(ClassicPak::open(&bytes).unwrap_err(), PakError::Malformed);
}

/// Opens the real archive when someone has put it in place. It carries the
/// classic data the rest of this project has been working around.
#[test]
fn reads_the_real_archive_when_it_is_there() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/burntime.pak");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: no {} — see testdata/README.md", path.display());
        return;
    };

    let pak = ClassicPak::open(&bytes).expect("the real archive should open");
    assert!(pak.len() > 200, "only {} entries", pak.len());

    // The three things this unblocks.
    let gamdat = pak.get("burn_gfx/gam.dat").expect("gam.dat");
    assert_eq!(gamdat.len(), 31_140, "gam.dat is a fixed size");

    for zei in ["burn_gfx/zei_014.raw", "burn_gfx/zei_015.raw"] {
        let data = pak.get(zei).unwrap_or_else(|| panic!("{zei} should be in the archive"));
        assert_eq!(data.len(), 65_520, "{zei}");
    }

    assert!(pak.contains("burn_gfx/raum_0.pac"), "room interiors");
    assert!(pak.contains("burn_gfx/mat_000.raw"), "the world map's own file");

    // Every entry must be readable, not just the ones we name.
    for entry in pak.entries() {
        let data = pak.data(entry);
        assert!(!entry.name.is_empty());
        assert_eq!(data.len(), pak.get(&entry.name).map(|d| d.len()).unwrap_or(0));
    }
}
