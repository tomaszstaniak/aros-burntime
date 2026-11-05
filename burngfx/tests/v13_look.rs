//! v1.3's look (`13e12ef`, `bae043c`): the NewGfx table's new flags and its
//! character artwork left off, checked against v1.3's own `newgfx.txt`.

mod common;

use burngfx::newgfx::{resolve, Resolved, Table};

fn table() -> Option<Table> {
    let text = common::read_text("v1.3/newgfx/newgfx.txt")?;
    Some(Table::parse(&text).expect("v1.3's newgfx.txt reads"))
}

fn reached(tables: &[Table], id: &str) -> Option<String> {
    match resolve(tables, id, &|_| true) {
        Resolved::New(r) => Some(r.id.id),
        Resolved::Classic(_) => None,
    }
}

/// `ClothingRecolorTests` "character artwork requires explicit opt-in":
/// without it the bodies are the existing `gfx/syssze.png`, never the
/// `char_*` sheets (which neither the source nor the release carries).
#[test]
fn the_new_character_artwork_stays_off() {
    let Some(t) = table() else { return };
    let tables = [t];
    for id in ["burngfxani@syssze.raw?80-95", "burngfxani@syssze.raw?240-255", "burngfxani@syssze.raw?64-79",
               "burngfxani@syssze.raw?256-271", "syssze.raw?92", "syssze.raw?141"] {
        let got = reached(&tables, id).unwrap_or_default();
        assert!(got.contains("gfx/syssze.png"), "{id} → {got}");
    }
}

/// `newgfx.txt:102-103` at v1.3: the blue and gray start flags are cells 3
/// and 2 of `start_flags.png`; green and red stay cells 0 and 1.
#[test]
fn the_four_flags() {
    let Some(t) = table() else { return };
    let tables = [t];
    assert_eq!(reached(&tables, "sta.ani?8").as_deref(), Some("pngsheet@gfx/start_flags.png?0?90x54"));
    assert_eq!(reached(&tables, "sta.ani?9").as_deref(), Some("pngsheet@gfx/start_flags.png?1?90x54"));
    assert_eq!(reached(&tables, "gfx/flag_black.png").as_deref(), Some("pngsheet@gfx/start_flags.png?2?90x54"));
    assert_eq!(reached(&tables, "gfx/flag_blue.png").as_deref(), Some("pngsheet@gfx/start_flags.png?3?90x54"));
}

/// `Character.Hire` dresses a recruit in his boss's colours
/// (`Character.cs:466-472` at v1.3) and a load dresses everyone employed
/// again (`AfterDeserialization`): a gray boss's mercenary wears the green
/// set recoloured, a blue one's set 1, a dog keeps his body.
#[test]
fn recruits_wear_their_bosses_colours() {
    use burngfx::gamdat::CharClass;
    use burngfx::hire::People;
    use burngfx::player_colors::{assign, Body, PlayerColor};
    let Some(bytes) = common::read("burntime.pak") else { return };
    let pak = burngfx::pak::ClassicPak::open(&bytes).expect("burntime.pak");
    let gam = burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat").expect("gam.dat")).expect("gam.dat parses");
    let types = burngfx::items::ItemTypes::parse("[item_meat]\nclass=food\nfood=3\n");
    let mut people = People::from_records(&gam.characters, &types, &Default::default());
    let colours = assign(PlayerColor::Black, PlayerColor::Red);
    people.outfits = colours.map(|c| burngfx::player_colors::body_set_of_icon(c.icon_id()));
    let find = |p: &People, class: CharClass| (0..p.len()).find(|&i| p.get(i).is_some_and(|q| q.class == class)).unwrap();
    let merc = find(&people, CharClass::Mercenary);
    let dog = find(&people, CharClass::Dog);
    let dog_body = people.get(dog).unwrap().sprite_id;
    for (who, seat) in [(merc, 0), (dog, 2)] {
        let record = people.get(who).unwrap().record;
        people.set_hired(record, seat);
    }
    people.dress_employed();
    assert_eq!(Body::from_id(people.get(merc).unwrap().sprite_id), Body { sprite: 128, gray: true }, "gray: green's mercenary, recoloured");
    assert_eq!(people.get(dog).unwrap().sprite_id, dog_body, "no body set of his own");
    // Seat 2 of `assign(Black, Red)` is green, the first colour left.
    assert_eq!(colours[2], PlayerColor::Green);
}
