//! The character table in `gam.dat`, and what `LoadNPCs` makes of it.

use burngfx::gamdat::{self, CharClass, Placed, CHARACTER_COUNT};

fn gamdat() -> gamdat::Gamdat {
    let bytes = std::fs::read("testdata/burntime.pak").expect("testdata pack");
    let pak = burngfx::pak::ClassicPak::open(&bytes).expect("pak");
    gamdat::parse(pak.get("burn_gfx/gam.dat").expect("gam.dat")).expect("parse")
}

/// Every record accounted for: the four player slots, the placed characters,
/// and the five whose `LocationId` is out of range.
#[test]
fn the_table_adds_up() {
    let g = gamdat();
    assert_eq!(g.characters.len(), CHARACTER_COUNT);

    let placed = g.characters.iter().filter(|c| matches!(c.placed, Placed::At(_))).count();
    let homeless =
        g.characters.iter().filter(|c| c.placed == Placed::HomelessTrader).count();
    let in_group = g.characters.iter().filter(|c| c.placed == Placed::InGroupOf).count();

    assert_eq!(placed, 300, "the characters the world is stocked with");
    assert_eq!(homeless, 5, "traders LoadNPCs builds with no home area");
    // The four player slots carry `LocationId == 0` until a game starts, so
    // they land here rather than in the world. That is exactly why
    // `SaveGame` guards its group-building with `i >= 4`: without it the
    // player slots would be read as somebody's employees.
    assert_eq!(in_group, 4);
    for i in 0..4 {
        assert_eq!(g.characters[i].placed, Placed::InGroupOf);
    }
    assert_eq!(placed + homeless + in_group, CHARACTER_COUNT);
}

/// The five out-of-range records are all the same kind of thing.
#[test]
fn the_homeless_five_are_traders() {
    let g = gamdat();
    let homeless: Vec<_> =
        g.characters.iter().filter(|c| c.placed == Placed::HomelessTrader).collect();
    assert_eq!(homeless.len(), 5);
    for c in &homeless {
        assert_eq!(c.class, CharClass::Trader);
        assert_eq!(c.health, 97);
    }
    let mut names: Vec<u16> = homeless.iter().map(|c| c.name_id).collect();
    names.sort();
    assert_eq!(names, [196, 197, 198, 199, 200]);
}

/// **The dog rule is `LoadNPCs`', not the data layer's.**
///
/// `Save/Character.Type` would call a `CharType == 4` record a dog when its
/// `FaceId` is zero, which gives 64. `LoadNPCs` tests `NameId == 178`, which
/// gives 66 — the two extra are at location 36 with `FaceId = 20`.
#[test]
fn dogs_are_counted_the_way_the_world_is_built() {
    let g = gamdat();
    let dogs = g.characters.iter().filter(|c| c.class == CharClass::Dog).count();
    let mutants = g.characters.iter().filter(|c| c.class == CharClass::Mutant).count();
    assert_eq!(dogs, 66);
    assert_eq!(mutants, 44);
    assert_eq!(dogs + mutants, 110, "every CharType == 4 record is one or the other");

    let by_the_data_layer = g
        .characters
        .iter()
        .filter(|c| matches!(c.class, CharClass::Dog | CharClass::Mutant) && c.face_id == 0)
        .count();
    assert_eq!(by_the_data_layer, 64, "the rule we deliberately do not use");
}

/// Who may be offered work, and who may not.
///
/// The trader is the one worth stating: he is excluded not because the hire
/// branch rejects him but because the *trade* branch is tested first and
/// takes him. Same outcome, different reason.
#[test]
fn only_the_four_professions_can_be_hired() {
    assert!(CharClass::Mercenary.can_be_hired());
    assert!(CharClass::Technician.can_be_hired());
    assert!(CharClass::Doctor.can_be_hired());
    assert!(CharClass::Boss.can_be_hired());

    assert!(!CharClass::Dog.can_be_hired());
    assert!(!CharClass::Mutant.can_be_hired());
    assert!(!CharClass::Trader.can_be_hired(), "the trade branch takes him first");
    assert!(!CharClass::Unknown(9).can_be_hired());
}

/// `HireItemId` resolves the way `LoadNPCs` writes it: the three staples on
/// their own when it is zero, the named item on its own when it is 4 or
/// above, and both when it is 1, 2 or 3.
#[test]
fn hire_items_follow_the_load_rule() {
    let g = gamdat();
    let (snake, meat, bottle) = (10, 11, 12);

    let mut a_staple_one = None;
    let mut a_special_one = None;
    let mut a_low_one = None;
    for c in &g.characters {
        match c.hire_item_id {
            0 => a_staple_one = a_staple_one.or(Some(c)),
            1..=3 => a_low_one = a_low_one.or(Some(c)),
            _ => a_special_one = a_special_one.or(Some(c)),
        }
    }

    let staple = a_staple_one.expect("a character with no special hire item");
    assert_eq!(staple.hire_item_ids(snake, meat, bottle), [snake, meat, bottle]);

    let special = a_special_one.expect("a character with a special hire item");
    assert_eq!(
        special.hire_item_ids(snake, meat, bottle),
        [special.hire_item_id as usize],
        "an id of 4 or more replaces the staples rather than adding to them"
    );

    if let Some(low) = a_low_one {
        assert_eq!(
            low.hire_item_ids(snake, meat, bottle),
            [snake, meat, bottle, low.hire_item_id as usize]
        );
    }
}

/// The distribution of `HireItemId` covers every placed record.
#[test]
fn every_placed_character_states_a_price() {
    let g = gamdat();
    let placed = g.characters.iter().filter(|c| matches!(c.placed, Placed::At(_)));
    // 300 placed records, and each resolves to at least one accepted item.
    let mut n = 0;
    for c in placed {
        assert!(!c.hire_item_ids(10, 11, 12).is_empty());
        n += 1;
    }
    assert_eq!(n, 300);
}

