//! The asset archive: what it accepts, and — more to the point — what it must
//! refuse. Everything here is built byte by byte, so none of it needs game
//! data.
//!
//! The archive is the one thing both sides of the build agree on, and the AROS
//! side compiles it in and trusts it. A reader that walks off the end of a
//! malformed one would do it inside a `no_std` binary with no unwinder.

use burngfx::pack::{names, Pack, PackError, PackedImage, MAGIC, NAME_LEN, VERSION};

const ENTRY_LEN: usize = NAME_LEN + 8;
const HEADER_LEN: usize = 16;

struct Builder {
    entries: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Builder {
    fn new() -> Builder {
        Builder { entries: Vec::new() }
    }

    fn add(mut self, name: &str, data: &[u8]) -> Builder {
        self.entries.push((name.as_bytes().to_vec(), data.to_vec()));
        self
    }

    fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());

        let mut offset = HEADER_LEN + self.entries.len() * ENTRY_LEN;
        for (name, data) in &self.entries {
            let mut padded = [0u8; NAME_LEN];
            padded[..name.len()].copy_from_slice(name);
            out.extend_from_slice(&padded);
            out.extend_from_slice(&(offset as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            offset += data.len();
        }
        for (_, data) in &self.entries {
            out.extend_from_slice(data);
        }
        out
    }
}

#[test]
fn round_trips_entries() {
    let bytes = Builder::new()
        .add("map/MAT_000.burnmap", b"a map")
        .add("tile/02_48", b"pixels")
        .add("text/BURN.TXT", b"New Sandez}")
        .build();

    let pack = Pack::open(&bytes).unwrap();
    assert_eq!(pack.len(), 3);
    assert!(!pack.is_empty());

    assert_eq!(pack.get("map/MAT_000.burnmap"), Some(b"a map".as_slice()));
    assert_eq!(pack.get("tile/02_48"), Some(b"pixels".as_slice()));
    assert_eq!(pack.get("text/BURN.TXT"), Some(b"New Sandez}".as_slice()));
    assert!(pack.contains("tile/02_48"));

    // A name that is a prefix of another must not match it.
    assert_eq!(pack.get("tile/02_4"), None);
    assert_eq!(pack.get("tile/02_480"), None);
    assert_eq!(pack.get(""), None);

    assert_eq!(pack.name(1), b"tile/02_48");
    assert_eq!(pack.data(1), b"pixels");
}

#[test]
fn an_empty_pack_is_valid() {
    let bytes = Builder::new().build();
    let pack = Pack::open(&bytes).unwrap();
    assert!(pack.is_empty());
    assert_eq!(pack.get("anything"), None);
}

#[test]
fn refuses_what_is_not_a_pack() {
    assert_eq!(Pack::open(&[]).unwrap_err(), PackError::Malformed);
    assert_eq!(Pack::open(b"short").unwrap_err(), PackError::Malformed);

    let mut wrong = Builder::new().build();
    wrong[..8].copy_from_slice(b"NOTAPACK");
    assert_eq!(Pack::open(&wrong).unwrap_err(), PackError::NotAPack);

    let mut future = Builder::new().build();
    future[8..10].copy_from_slice(&99u16.to_le_bytes());
    assert_eq!(Pack::open(&future).unwrap_err(), PackError::UnsupportedVersion(99));
}

/// A count large enough to wrap the table-size multiply must be refused, not
/// wrapped into something small and plausible.
#[test]
fn refuses_an_entry_count_that_would_overflow() {
    let mut bytes = Builder::new().add("a", b"x").build();
    bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(Pack::open(&bytes).unwrap_err(), PackError::Malformed);

    // Merely more entries than the file can hold is the same answer.
    let mut bytes = Builder::new().add("a", b"x").build();
    bytes[12..16].copy_from_slice(&1000u32.to_le_bytes());
    assert_eq!(Pack::open(&bytes).unwrap_err(), PackError::Malformed);
}

#[test]
fn refuses_entries_that_point_outside_the_file() {
    // Past the end.
    let mut bytes = Builder::new().add("a", b"xyz").build();
    let at = HEADER_LEN + NAME_LEN;
    bytes[at + 4..at + 8].copy_from_slice(&9999u32.to_le_bytes());
    assert_eq!(Pack::open(&bytes).unwrap_err(), PackError::Malformed);

    // An offset that would wrap when the length is added.
    let mut bytes = Builder::new().add("a", b"xyz").build();
    bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes[at + 4..at + 8].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(Pack::open(&bytes).unwrap_err(), PackError::Malformed);

    // An offset inside the entry table is not a blob.
    let mut bytes = Builder::new().add("a", b"xyz").build();
    bytes[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(Pack::open(&bytes).unwrap_err(), PackError::Malformed);
}

#[test]
fn images_declare_their_size_and_are_checked_against_it() {
    let mut img = Vec::new();
    img.extend_from_slice(&2u16.to_le_bytes());
    img.extend_from_slice(&2u16.to_le_bytes());
    img.extend_from_slice(&[1, 2, 3, 4]); // (0,0)
    img.extend_from_slice(&[5, 6, 7, 8]); // (1,0)
    img.extend_from_slice(&[9, 10, 11, 12]); // (0,1)
    img.extend_from_slice(&[13, 14, 15, 16]); // (1,1)

    let parsed = PackedImage::parse(&img).unwrap();
    assert_eq!((parsed.width, parsed.height), (2, 2));
    assert_eq!(parsed.pixel(0, 0), Some([1, 2, 3, 4].as_slice()));
    assert_eq!(parsed.pixel(1, 1), Some([13, 14, 15, 16].as_slice()));
    assert_eq!(parsed.pixel(2, 0), None, "outside the width");
    assert_eq!(parsed.pixel(0, 2), None, "outside the height");

    // Fewer pixels than the header promises.
    assert!(PackedImage::parse(&img[..img.len() - 1]).is_none());
    assert!(PackedImage::parse(&[0, 0]).is_none());
}

/// 65535 x 65535 x 4 does not fit in a u32; the check has to be widened or it
/// wraps and lets a tiny buffer claim to be an enormous image.
#[test]
fn a_huge_declared_image_cannot_wrap_the_size_check() {
    let mut img = Vec::new();
    img.extend_from_slice(&u16::MAX.to_le_bytes());
    img.extend_from_slice(&u16::MAX.to_le_bytes());
    img.extend_from_slice(&[0u8; 64]);
    assert!(PackedImage::parse(&img).is_none());
}

#[test]
fn names_are_built_the_same_way_on_both_sides() {
    let mut buf = [0u8; NAME_LEN];

    let n = names::map(&mut buf, "MAT_000").unwrap();
    assert_eq!(&buf[..n], b"map/MAT_000.burnmap");

    let n = names::settings(&mut buf, "MAT_005").unwrap();
    assert_eq!(&buf[..n], b"set/MAT_005.txt");

    let n = names::tile(&mut buf, 2, 48).unwrap();
    assert_eq!(&buf[..n], b"tile/02_48");

    let n = names::tile(&mut buf, 0, 7).unwrap();
    assert_eq!(&buf[..n], b"tile/00_07");
}

/// Truncating a name would make two entries collide and still look up cleanly,
/// so it has to be refused where it happens.
#[test]
fn a_name_that_does_not_fit_is_refused_rather_than_cut() {
    let mut buf = [0u8; NAME_LEN];
    assert_eq!(names::map(&mut buf, "A_VERY_LONG_MAP_NAME_INDEED"), None);
    assert_eq!(names::settings(&mut buf, "ALSO_MUCH_TOO_LONG_TO_FIT_HERE"), None);

    // The longest name that does fit still works, and the buffer is untouched
    // beyond it.
    let longest = "X".repeat(NAME_LEN - "map/".len() - ".burnmap".len());
    let n = names::map(&mut buf, &longest).unwrap();
    assert_eq!(n, NAME_LEN);
}

/// A save is bound to what shapes the world, not to the pictures and texts
/// around it: one more picture or line of text keeps the fingerprint, a
/// changed `gam.dat` or rule table does not, and neither does the order the
/// packer happened to write them in.
#[test]
fn the_rules_fingerprint_ignores_pictures_and_texts() {
    let base = Builder::new()
        .add("classic/gam.dat", b"world")
        .add("rules/prices", b"1 2 3")
        .add("v12/extended/game", b"preset")
        .add("v12/newburn", b"Okay")
        .add("tile/02_48", b"pixels")
        .build();
    let fp = |b: &[u8]| Pack::open(b).unwrap().rules_fingerprint();
    let more = Builder::new()
        .add("classic/gam.dat", b"world")
        .add("rules/prices", b"1 2 3")
        .add("v12/extended/game", b"preset")
        .add("v12/newburn", b"Okay, more")
        .add("v12/cityinfo", b"a city")
        .add("v12/men/001", b"what people say")
        .add("v12/burn", b"Bottle")
        .add("ui/item_focus", b"frame")
        .add("tile/02_48", b"other pixels")
        .build();
    assert_eq!(fp(&base), fp(&more));
    let reordered = Builder::new()
        .add("v12/extended/game", b"preset")
        .add("rules/prices", b"1 2 3")
        .add("classic/gam.dat", b"world")
        .build();
    assert_eq!(fp(&base), fp(&reordered));
    let other_world = Builder::new()
        .add("classic/gam.dat", b"worle")
        .add("rules/prices", b"1 2 3")
        .add("v12/extended/game", b"preset")
        .build();
    assert_ne!(fp(&base), fp(&other_world));
    let other_preset = Builder::new()
        .add("classic/gam.dat", b"world")
        .add("rules/prices", b"1 2 3")
        .add("v12/extended/game", b"presets")
        .build();
    assert_ne!(fp(&base), fp(&other_preset));
    let with_ai = Builder::new()
        .add("classic/gam.dat", b"world")
        .add("rules/prices", b"1 2 3")
        .add("v12/extended/game", b"preset")
        .add("v12/ai", b"policy")
        .build();
    assert_ne!(fp(&base), fp(&with_ai));
}

/// A table in name order is searched, one in any other order read through:
/// both find every entry, the first of a repeated name, and nothing else.
#[test]
fn lookups_agree_in_name_order_and_out_of_it() {
    let names = ["ng/tile/04_12", "body/16", "ng/f/syst/30", "a", "zz", "body/160", "ng/a/flag0/11"];
    let mut sorted: Vec<&str> = names.to_vec();
    sorted.sort();
    for order in [&names[..], &sorted[..]] {
        let mut b = Builder::new();
        for (i, n) in order.iter().enumerate() {
            b = b.add(n, &[i as u8]);
            // A second "body/16", right after the first: still in order.
            if *n == "body/16" {
                b = b.add(n, &[99]);
            }
        }
        let bytes = b.build();
        let pack = Pack::open(&bytes).unwrap();
        for (i, n) in order.iter().enumerate() {
            assert_eq!(pack.get(n), Some(&[i as u8][..]), "{n}");
        }
        assert_eq!(pack.get("body/1"), None);
        assert_eq!(pack.get("zzz"), None);
        assert_eq!(pack.get(""), None);
    }
}
