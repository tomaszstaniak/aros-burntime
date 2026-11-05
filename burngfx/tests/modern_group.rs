//! `modern_ai::group`: the opponent's group looked after at the start of a
//! turn, against Extended's v1.2 item table.

use burngfx::game::{CharacterNeeds, ItemBag, WaterSource};
use burngfx::items::ItemTypes;
use burngfx::modern_ai::group::*;
use burngfx::rival::Rival;
use burngfx::trade::Prices;

fn types() -> ItemTypes {
    let p = format!("{}/testdata/v1.2/rules/extended/items.txt", env!("CARGO_MANIFEST_DIR"));
    ItemTypes::parse_v12(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn k(t: &ItemTypes, n: &str) -> usize {
    t.index_of(n).unwrap_or_else(|| panic!("{n}"))
}

fn needs(food: i32, water: i32, health: i32) -> CharacterNeeds {
    CharacterNeeds { food, max_food: 9, water, max_water: 5, health }
}

/// A leader and `followers`, all with the same needs.
fn group(n: CharacterNeeds, followers: usize) -> Rival {
    let mut r = Rival::new(1, 0);
    r.needs = n;
    for _ in 0..followers {
        let id = r.party.recruit();
        r.party.companions.iter_mut().find(|c| c.id == id).unwrap().needs = n;
    }
    r
}

#[test]
fn a_city_gives_three_and_one_refill_each() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let (bottle, canteen) = (k(&t, "item_bottle"), k(&t, "item_empty_canteen"));
    let mut r = group(needs(1, 0, 100), 1);
    r.pack = ItemBag::with(6, &[bottle, canteen, bottle]);
    // Water first, as the turn does: the leader is left out while no camp is held.
    let given = city_minimum(&mut r, items, true, 0, false, true, true);
    assert_eq!((r.needs.water, r.party.companions[0].needs.water), (0, 3));
    assert_eq!(given, (0, 3, vec![t.full_of(canteen).unwrap(), t.full_of(bottle).unwrap()]), "two members: two containers, the canteen first");
    assert_eq!(r.pack.get(1), t.full_of(canteen));
    assert_eq!(r.pack.get(0), t.full_of(bottle));
    assert_eq!(r.pack.get(2), Some(bottle), "one each, no more");
    // With a camp the leader is fed too; outside a city nothing.
    assert_eq!(city_minimum(&mut r, items, true, 1, true, false, false).0, 2 + 2);
    assert_eq!(city_minimum(&mut group(needs(0, 0, 100), 0), items, false, 1, true, true, true), (0, 0, vec![]));
}

#[test]
fn the_spring_waters_the_group_then_fills_its_bottles() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let bottle = k(&t, "item_bottle");
    let mut r = group(needs(5, 3, 100), 1);
    r.pack = ItemBag::with(6, &[bottle, bottle]);
    let mut source = WaterSource { refresh: 5, reserve: 7, capacity: 10 };
    use_local_water_source(&mut r, items, &mut source);
    assert_eq!((r.needs.water, r.party.companions[0].needs.water), (5, 5));
    assert_eq!((r.pack.get(0), r.pack.get(1), source.reserve), (t.full_of(bottle), Some(bottle), 1), "a bottle is two: one left over");
}

#[test]
fn the_group_eats_the_least_first_and_never_the_settlers_pay() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let (maggots, meat, snake, bottle) = (k(&t, "item_maggots"), k(&t, "item_meat"), k(&t, "item_snake"), k(&t, "item_water_bottle"));
    let mut r = group(needs(2, 5, 100), 0);
    r.pack = ItemBag::with(6, &[meat, maggots, snake]);
    assert!(consume_available_supplies(&mut r, items, None, None));
    // Maggots (3) to five; the snake (7) is more than the four of room, so
    // eating stops there rather than passing to anything else.
    assert_eq!(r.needs.food, 5);
    assert_eq!(r.pack.iter().collect::<Vec<_>>(), vec![meat, snake]);
    // A kind kept back is not eaten; a room's food only when the camp may be looted.
    let mut r = group(needs(1, 5, 100), 0);
    r.pack = ItemBag::with(6, &[snake]);
    let mut rooms = [ItemBag::with(4, &[maggots])];
    assert!(consume_available_supplies(&mut r, items, Some(snake), Some(&mut rooms)));
    assert_eq!((r.needs.food, r.pack.len(), rooms[0].len()), (4, 1, 0));
    assert!(!consume_available_supplies(&mut r, items, Some(snake), None));
    // Thirst: the fullest container, which is left empty.
    let mut r = group(needs(9, 1, 100), 0);
    r.pack = ItemBag::with(6, &[bottle]);
    assert!(consume_available_supplies(&mut r, items, None, None));
    assert_eq!((r.needs.water, r.pack.get(0)), (3, t.empty_of(bottle)));
}

#[test]
fn the_doctor_takes_just_enough() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let (maggots, rats, meat) = (k(&t, "item_maggots"), k(&t, "item_rats"), k(&t, "item_meat"));
    let doctor = Doctor { healing_factor: 1.0, health_cap: 100 };
    let mut r = group(needs(5, 5, 30), 1);
    r.party.companions[0].needs.health = 60;
    r.pack = ItemBag::with(6, &[meat, rats, maggots]);
    // Thirty to seventy is forty: in the group's order, meat (36), then rats
    // (56) — enough, and neither can be handed back. The doctor heals all of
    // it, up to the cap and not to seventy.
    let done = use_doctor(&mut r, items, true, doctor, |_| true, |_| 0);
    assert_eq!(done.len(), 1, "the follower at sixty is not a patient");
    assert_eq!((done[0].patient, done[0].healed, done[0].paid.clone()), (0, 56, vec![(0, meat), (0, rats)]));
    assert_eq!(r.pack.iter().collect::<Vec<_>>(), vec![maggots]);
    // What can be sold goes first: rats, maggots, then the meat to make
    // forty; handing back the most healing first, the rats go back (48 is
    // still enough) and the meat does not (32 is not).
    let mut r = group(needs(5, 5, 30), 0);
    r.pack = ItemBag::with(6, &[meat, rats, maggots]);
    let done = use_doctor(&mut r, items, true, doctor, |k| k != meat, |_| 0);
    assert_eq!(done[0].paid, vec![(0, maggots), (0, meat)]);
    // No doctor here: nothing.
    let mut r = group(needs(5, 5, 30), 0);
    r.pack = ItemBag::with(6, &[meat]);
    assert!(use_doctor(&mut r, items, false, doctor, |_| true, |_| 0).is_empty());
    assert!(needs_doctor_payment(&r, true) && !needs_doctor_payment(&r, false));
}
