//! The dogs and mutants standing about: what they are worth in a fight,
//! what they leave behind, and when they come back.

use burngfx::combat::creature_drop;
use burngfx::game::{ItemBag, ItemFacts, Point, MAX_HEALTH, PACK_CAPACITY};
use burngfx::gamdat::{CharClass, CharacterRecord, Placed};
use burngfx::hire::People;
use burngfx::settings::GameSettings;

/// The classic file's own numbers at difficulty 0, which is what this is
/// all measured against.
fn settings() -> GameSettings {
    GameSettings::parse(
        "[0]\n\
         npc_respawn = 10\n\
         trader_respawn = 20\n\
         mutant_respawn = 30\n\
         dog_respawn = 30\n\
         trader_health = 100\n\
         mutant_health = 31\n\
         dog_health = 31\n\
         trader_attack = 60\n\
         mutant_attack = 40\n\
         dog_attack = 30\n",
        0,
    )
}

struct Table;

impl ItemFacts for Table {
    fn food(&self, _kind: usize) -> i32 {
        0
    }
    fn water(&self, _kind: usize) -> i32 {
        0
    }
    fn full(&self, _kind: usize) -> Option<usize> {
        None
    }
    fn empty(&self, _kind: usize) -> Option<usize> {
        None
    }
    fn kind_named(&self, _name: &str) -> Option<usize> {
        None
    }
    fn name_of(&self, _kind: usize) -> Option<&str> {
        None
    }
}

fn record(class: CharClass, health: u8) -> CharacterRecord {
    CharacterRecord {
        placed: Placed::At(3),
        class,
        // 178 is what makes a mutant record a dog; the class is already
        // decided by the time it reaches here.
        name_id: 178,
        face_id: 0,
        text_id: 0,
        sprite_id: 0,
        at: Point { x: 100, y: 100 },
        water: 1,
        food: 1,
        health,
        experience: 100,
        hire_item_id: 0,
        employer: 0,
    }
}

fn world(classes: &[CharClass]) -> People {
    let records: Vec<CharacterRecord> =
        classes.iter().map(|&c| record(c, 80)).collect();
    People::from_records(&records, &Table, &settings())
}

// -- what they are ----------------------------------------------------------

/// **A creature's health is not its record's.** `LoadNPCs` overwrites it
/// with the class number, and only the four professions keep what the file
/// says.
#[test]
fn a_dog_has_the_settings_health_and_not_the_records() {
    let people = world(&[CharClass::Dog, CharClass::Mutant, CharClass::Mercenary]);

    assert_eq!(people.get(0).unwrap().needs.health, 31, "dog_health");
    assert_eq!(people.get(1).unwrap().needs.health, 31, "mutant_health");
    assert_eq!(people.get(2).unwrap().needs.health, 80, "the record's own");
}

/// `BaseAttackValue` is the class's, and a dog's is three times a bare
/// hand's.
#[test]
fn what_they_hit_for_with_nothing_in_hand() {
    let people = world(&[CharClass::Dog, CharClass::Mutant, CharClass::Doctor]);

    assert_eq!(people.get(0).unwrap().base_attack, 30);
    assert_eq!(people.get(1).unwrap().base_attack, 40);
    assert_eq!(people.get(2).unwrap().base_attack, 10, "a person's ten");
}

/// **A wounded creature is whole again in the morning.** `Dog.Turn` opens
/// with `Health = DogHealth` before anything else, so walking away from a
/// half-killed dog is worth nothing.
#[test]
fn a_creature_heals_overnight_and_a_person_does_not() {
    let mut people = world(&[CharClass::Dog, CharClass::Mercenary]);
    people.get_mut(0).unwrap().needs.health = 4;
    people.get_mut(1).unwrap().needs.health = 40;

    people.turn(1993, 1, None);

    assert_eq!(people.get(0).unwrap().needs.health, 31, "whole again");
    assert_eq!(
        people.get(1).unwrap().needs.health,
        40,
        "a free person's day never touches their health"
    );
}

// -- dying ------------------------------------------------------------------

/// `Character.Die`: what they carried on the ground where they fell, out of
/// anybody's employ, and no health left.
#[test]
fn dying_drops_everything_where_it_fell() {
    let mut people = world(&[CharClass::Dog]);
    people.get_mut(0).unwrap().pack = ItemBag::with(PACK_CAPACITY, &[7, 9]);

    let fall = people.fell(0).expect("it died");

    assert_eq!(fall.at, Point { x: 100, y: 100 }, "where it was standing");
    assert_eq!(fall.dropped.iter().collect::<Vec<_>>(), vec![7, 9]);
    assert_eq!(people.get(0).unwrap().needs.health, 0);
    assert!(people.get(0).unwrap().pack.is_empty());
    assert_eq!(people.at(3).count(), 0, "and it is not standing there any more");
}

/// **Nobody dies twice.** A second blow on a body would drop a second copy
/// of everything it had.
#[test]
fn a_corpse_cannot_fall_again() {
    let mut people = world(&[CharClass::Dog]);
    assert!(people.fell(0).is_some());
    assert!(people.fell(0).is_none());
}

/// A dog always leaves meat: `Dog.Die` drops it before the base call does
/// anything else. It is the reason to hunt one.
#[test]
fn a_dog_leaves_meat() {
    let Ok(text) = std::fs::read_to_string("testdata/items_original.txt") else { return };
    let types = burngfx::items::ItemTypes::parse(&text);
    let meat = types.index_of("item_meat").expect("meat");

    for day in 0..20 {
        let mut rng = burngfx::spawn::Rng::seeded(1993 + day);
        assert_eq!(creature_drop(CharClass::Dog, &types, &mut rng), Some(meat));
    }
}

/// A mutant leaves something one time in ten, and a person leaves nothing
/// but what they carried.
#[test]
fn a_mutant_leaves_something_now_and_then() {
    let Ok(text) = std::fs::read_to_string("testdata/items_original.txt") else { return };
    let types = burngfx::items::ItemTypes::parse(&text);

    let mut dropped = 0;
    for seed in 0..200 {
        let mut rng = burngfx::spawn::Rng::seeded(seed);
        if creature_drop(CharClass::Mutant, &types, &mut rng).is_some() {
            dropped += 1;
        }
    }
    assert!((5..=45).contains(&dropped), "about one in ten of 200, got {dropped}");

    let mut rng = burngfx::spawn::Rng::seeded(7);
    assert_eq!(creature_drop(CharClass::Mercenary, &types, &mut rng), None);
}

// -- coming back ------------------------------------------------------------

/// Each class waits its own count, and **a dog does not come back at a
/// hundred**: `Dog.Revive` overrides the base with `DogHealth`.
#[test]
fn each_class_comes_back_on_its_own_terms() {
    let rules = settings();

    assert_eq!(rules.respawn_days(CharClass::Dog), 30);
    assert_eq!(rules.respawn_days(CharClass::Mutant), 30);
    assert_eq!(rules.respawn_days(CharClass::Trader), 20);
    assert_eq!(rules.respawn_days(CharClass::Mercenary), 10, "npc_respawn");

    assert_eq!(rules.full_health(CharClass::Dog), 31);
    assert_eq!(rules.full_health(CharClass::Trader), 100);
    assert_eq!(
        rules.full_health(CharClass::Doctor),
        MAX_HEALTH,
        "`Character.Revive` sets a flat hundred"
    );
}

/// The whole of it: a dog dies, waits its thirty days, and stands up with
/// thirty-one of health rather than a hundred.
#[test]
fn a_dog_killed_today_is_back_in_thirty_days() {
    let rules = settings();
    let mut people = world(&[CharClass::Dog]);
    let mut waiting = burngfx::game::Respawns::new();

    let fall = people.fell(0).expect("it died");
    people.died(fall.record);
    waiting.schedule(burngfx::game::Pending {
        record: fall.record,
        location: fall.location,
        days: rules.respawn_days(fall.class),
        food: fall.needs.food,
        water: fall.needs.water,
        revive_health: rules.full_health(fall.class),
    });

    // The day of the death counts, so twenty-nine more days change nothing.
    for _ in 0..29 {
        assert!(waiting.advance_day().is_empty());
    }
    let back = waiting.advance_day();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].needs.health, 31, "not the hundred a person gets");

    assert!(people.returned(back[0].record, back[0].location, Point { x: 8, y: 8 }, back[0].needs));
    assert_eq!(people.at(3).count(), 1, "standing there again");
    assert_eq!(people.get(0).unwrap().needs.health, 31);
}
