//! Room settings, string tables and font metrics.
//!
//! The synthetic cases need no game data. The two that read shipped files skip
//! themselves when those are absent.

use burngfx::font::Font;
use burngfx::text::{MapSettings, TextDb, TextRef};
use std::path::PathBuf;

fn testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(rel)
}

#[test]
fn reads_room_sections_and_ignores_the_rest() {
    let s = MapSettings::parse(
        "entry_point = 0x0\n\
         rooms = 2\n\
         \n\
         [room0]\n\
         type = water\n\
         title = burn?7\n\
         image = raum_3.pac\n\
         \n\
         [room1]\n\
         type = \n\
         title = burn?8\n\
         image = \n\
         \n\
         [something_else]\n\
         title = ignored\n",
    );

    assert_eq!(s.rooms.len(), 2);
    assert_eq!(s.room(0).unwrap().kind, "water");
    assert_eq!(s.room(0).unwrap().title, "burn?7");
    assert_eq!(s.room(0).unwrap().image, "raum_3.pac");
    assert_eq!(s.room(1).unwrap().kind, "");
    assert_eq!(s.room(1).unwrap().title, "burn?8");
    assert!(s.room(2).is_none());
}

/// Sections need not appear in order, and a gap must not shift the rest along.
#[test]
fn room_indices_come_from_the_section_name_not_the_order() {
    let s = MapSettings::parse("[room3]\ntitle = c\n\n[room1]\ntitle = a\n");
    assert_eq!(s.rooms.len(), 4);
    assert_eq!(s.room(1).unwrap().title, "a");
    assert_eq!(s.room(3).unwrap().title, "c");
    assert_eq!(s.room(0).unwrap().title, "");
}

#[test]
fn splits_database_references() {
    let r = TextRef::parse("burn?4").unwrap();
    assert_eq!((r.database, r.index), ("burn", 4));
    assert_eq!(r.file_name(), "BURN.TXT");

    assert!(TextRef::parse("burn").is_none(), "no separator");
    assert!(TextRef::parse("?4").is_none(), "no database");
    assert!(TextRef::parse("burn?x").is_none(), "index is not a number");
}

/// `}` ends a string; the original's tables carry it on every line.
#[test]
fn string_tables_stop_at_the_terminator() {
    let db = TextDb::parse("New Sandez}\nSana}  \nNo terminator\n");
    assert_eq!(db.get(0), Some("New Sandez"));
    assert_eq!(db.get(1), Some("Sana"));
    assert_eq!(db.get(2), Some("No terminator"));
    assert_eq!(db.get(3), None);
    assert_eq!(db.len(), 3);
}

#[test]
fn font_metrics_place_glyphs_side_by_side() {
    // Three characters of widths 3, 5, 2: they start at 0, 3 and 8.
    let font = Font::parse("image=f.png\nheight=12\noffset=-2\nchar0=\"abc\"\nwidth0=3 5 2\n").unwrap();
    assert_eq!(font.image, "f.png");
    assert_eq!(font.height, 12);
    assert_eq!(font.strip_width(), 10);

    let glyphs = font.layout("abc");
    assert_eq!(glyphs.len(), 3);
    assert_eq!((glyphs[0].src_x, glyphs[0].dst_x, glyphs[0].width), (0, 0, 3));
    assert_eq!((glyphs[1].src_x, glyphs[1].dst_x), (3, 3));
    assert_eq!((glyphs[2].src_x, glyphs[2].dst_x), (8, 8));
    assert_eq!(font.measure("abc"), 10);

    // The declared offset is parsed but deliberately not applied: the
    // original advances by the glyph width alone.
    assert_eq!(font.declared_offset, -2);
}

#[test]
fn font_handles_escaped_quotes_and_unknown_characters() {
    let font = Font::parse("image=f.png\nheight=8\nchar0=\"a\"\"b\"\nwidth0=1 2 3\n").unwrap();
    // `""` is one quote, so the characters are a, " and b.
    assert!(font.glyph('"').is_some());
    assert_eq!(font.measure("a\"b"), 6);
    // Characters the font lacks are skipped rather than substituted, so they
    // take no width at all: a(1) + b(3), with Z contributing nothing.
    assert_eq!(font.measure("aZb"), 4);
}

#[test]
fn font_rejects_descriptions_that_do_not_line_up() {
    assert!(Font::parse("image=f.png\nheight=8\nchar0=\"ab\"\nwidth0=1\n").is_none());
    assert!(Font::parse("image=f.png\nchar0=\"ab\"\nwidth0=1 2\n").is_none(), "no height");
    assert!(Font::parse("").is_none());
}

#[test]
fn resolves_a_real_entrance_to_its_name() {
    let (Ok(settings), Ok(db)) = (
        std::fs::read_to_string(testdata("maps/MAT_000.txt")),
        std::fs::read_to_string(testdata("lang/BURN.TXT")),
    ) else {
        eprintln!("skipping: no MAT_000.txt or BURN.TXT — see testdata/README.md");
        return;
    };

    let settings = MapSettings::parse(&settings);
    let db = TextDb::parse(&db);

    // Entrance 4 of the first map is Nob Hill; the chain is
    // [room4] title = burn?4 -> line 4 of BURN.TXT.
    let title = &settings.room(4).unwrap().title;
    assert_eq!(title, "burn?4");
    let r = TextRef::parse(title).unwrap();
    assert_eq!(r.file_name(), "BURN.TXT");
    assert_eq!(db.get(r.index), Some("Nob Hill"));

    assert_eq!(db.get(0), Some("New Sandez"));
    assert_eq!(db.get(2), Some("Death Town"));

    // Every room the file declares should point somewhere resolvable.
    for (i, room) in settings.rooms.iter().enumerate() {
        let Some(r) = TextRef::parse(&room.title) else {
            panic!("room {i}: title {:?} is not a reference", room.title);
        };
        if r.database == "burn" {
            assert!(db.get(r.index).is_some(), "room {i}: burn?{} is past the table", r.index);
        }
    }
}

#[test]
fn reads_the_shipped_font() {
    let Ok(text) = std::fs::read_to_string(testdata("font.txt")) else {
        eprintln!("skipping: no font.txt — see testdata/README.md");
        return;
    };
    let font = Font::parse(&text).expect("font.txt should parse");

    assert_eq!(font.image, "font.png");
    assert_eq!(font.height, 12);
    // ASCII 0x20..=0x7A, which is what the original's own font covers.
    assert!(font.glyph(' ').is_some());
    assert!(font.glyph('z').is_some());
    assert!(font.glyph('{').is_none());
    // The glyphs have to fit inside the 512-pixel strip.
    assert!(font.strip_width() <= 512, "strip is {} wide", font.strip_width());
    assert!(font.measure("Nob Hill") > 0);
}

/// A `.txt` overrides which picture a room shows, the same way it overrides
/// the title — and says nothing at all when it has no `image` line, which is
/// different from saying `raum_1.pac`.
#[test]
fn a_rooms_image_line_names_an_interior() {
    let s = MapSettings::parse(
        "[room0]\nimage = raum_0.pac\n\
         [room1]\nimage = raum_8.pac\n\
         [room2]\nimage = \n\
         [room3]\nimage = something_else.pac\n\
         [room4]\ntitle = burn?661\n",
    );
    assert_eq!(s.room(0).unwrap().interior(), Some(0));
    assert_eq!(s.room(1).unwrap().interior(), Some(8));
    assert_eq!(s.room(2).unwrap().interior(), None, "an empty line says nothing");
    // `BurnmapProcessor` falls back to raum_1 for a name it does not know, so
    // an unreadable line is a picture rather than a hole.
    assert_eq!(s.room(3).unwrap().interior(), Some(1));
    assert_eq!(s.room(4).unwrap().interior(), None, "no image line at all");
}

/// v1.2's `Font.LineHeight` (`FontResource.Load`): the declared
/// `line_height`, else the glyph height plus the offset. `font-small.txt`
/// declares 11; v0.9.1's `font.txt` declares none, so 12 − 2.
#[test]
fn a_fonts_line_height_is_declared_or_derived() {
    use burngfx::font::Font;
    let small = Font::parse("line_height=11\nimage=font.png\noffset=-2\nheight=12\nchar0=\"ab\"\nwidth0=5 5\n").unwrap();
    assert_eq!(small.line_height(), 11);
    let plain = Font::parse("image=font.png\noffset=-2\nheight=12\nchar0=\"ab\"\nwidth0=5 5\n").unwrap();
    assert_eq!(plain.line_height(), 10);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/v1.2/font-small.txt");
    if let Ok(text) = std::fs::read_to_string(path) {
        let f = Font::parse(&text).unwrap();
        assert_eq!((f.line_height(), f.metric_height(), f.supports('ü')), (11, 8, true));
    }
}
