//! v1.3's `LocationPopulationCreator` on its real input, New Village's
//! `MAT_038.txt` at `v1.3`, and `gam.dat`'s templates.

mod common;

use burngfx::game::Point;
use burngfx::gamdat::{self, CharClass, Placed};
use burngfx::population::{nearest_walkable, resident_records, Population, NEWBURN_NAME};
use burngfx::zei::WalkGrid;

/// A missing input fails the test (`common`), or skips it with
/// `BURNTIME_ALLOW_MISSING_TESTDATA`.
fn testdata(rel: &str) -> Option<Vec<u8>> {
    common::read(rel)
}

#[test]
fn new_villages_settings_name_five_residents_and_three_items() {
    let Some(text) = testdata("v1.3/maps/MAT_038.txt") else { return };
    let p = Population::parse(&String::from_utf8(text).unwrap());
    let classes: Vec<_> = p.residents.iter().map(|r| r.class).collect();
    assert_eq!(classes, [CharClass::Dog, CharClass::Dog, CharClass::Dog, CharClass::Technician, CharClass::Mercenary]);
    assert_eq!(p.residents[3].name, Some(("newburn".into(), 93)));
    assert_eq!((p.residents[3].dialog, p.residents[4].dialog), (39, 54));
    assert_eq!(p.residents[0].name, None);
    assert_eq!(p.residents[0].position, Point::new(116, 380));
    assert_eq!(p.ground, ["item_meat"]);
    assert_eq!(p.rooms, ["item_bottle", "item_wire"]);
    // v1.2's file has none of it.
    let Some(old) = testdata("v1.2/maps/MAT_038.txt") else { return };
    assert_eq!(Population::parse(&String::from_utf8(old).unwrap()), Population::default());
    // An unknown class is skipped, as `Enum.TryParse` fails.
    let odd = Population::parse("[resident0]\nclass = Mutant\nposition = 1x1\n[resident1]\nclass = Doctor\n");
    assert_eq!(odd.residents.len(), 1);
}

#[test]
fn residents_borrow_their_templates() {
    let (Some(text), Some(pak)) = (testdata("v1.3/maps/MAT_038.txt"), testdata("burntime.pak")) else { return };
    let pak = burngfx::pak::ClassicPak::open(&pak).unwrap();
    let g = gamdat::parse(pak.get("burn_gfx/gam.dat").unwrap()).unwrap();
    let p = Population::parse(&String::from_utf8(text).unwrap());
    let r = resident_records(&g, gamdat::NEW_VILLAGE, &p, None);
    assert_eq!(r.len(), 5);
    let dog = g.characters.iter().find(|c| c.name_id == 178).unwrap();
    for d in &r[..3] {
        assert_eq!((d.class, d.name_id, d.sprite_id, d.face_id), (CharClass::Dog, dog.name_id, dog.sprite_id, dog.face_id));
    }
    let tech = g.characters.iter().find(|c| c.class == CharClass::Technician && matches!(c.placed, Placed::At(_))).unwrap();
    assert_eq!((r[3].name_id, r[3].text_id, r[3].face_id, r[3].experience), (NEWBURN_NAME + 93, 39, tech.face_id, tech.experience));
    assert_eq!(u16::from(r[3].sprite_id), burngfx::game::respawn_body(CharClass::Technician, 0).unwrap(), "GetBodyId(set, 0)");
    assert_eq!(r[4].name_id, NEWBURN_NAME + 94);
    for x in &r {
        assert_eq!((x.placed, x.hire_item_id, x.employer), (Placed::At(gamdat::NEW_VILLAGE), 0, 0));
    }
    assert_eq!(r[0].at, Point::new(116, 380), "no mask: the position as written");
}

#[test]
fn nearest_walkable_is_upstreams() {
    // A 4 x 2 mask, walkable only at (2, 1) and (3, 0).
    let w = WalkGrid::from_packed(&[4, 0, 2, 0, 0, 0, 0, 1, 0, 0, 1, 0]).unwrap();
    let cell = burngfx::zei::MASK_CELL as i32;
    let inside = Point::new(2 * cell + 1, cell + 1);
    assert_eq!(nearest_walkable(&w, inside), inside, "walkable: unchanged");
    let c = |x: i32, y: i32| Point::new(x * cell + cell / 2, y * cell + cell / 2);
    assert_eq!(nearest_walkable(&w, Point::new(0, cell)), c(2, 1));
    assert_eq!(nearest_walkable(&w, Point::new(4 * cell + 50, 0)), c(3, 0), "outside the mask");
    assert_eq!(nearest_walkable(&WalkGrid::from_packed(&[2, 0, 2, 0, 0, 0, 0, 0]).unwrap(), Point::new(5, 5)), Point::new(5, 5), "nothing walkable");
}
