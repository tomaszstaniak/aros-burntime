//! `modern_ai::recruitment`: whom the opponent hires and how it pays, on
//! Extended's v1.2 item table.

use burngfx::combat::Kit;
use burngfx::game::{CharacterNeeds, ItemBag};
use burngfx::gamdat::CharClass;
use burngfx::items::ItemTypes;
use burngfx::modern_ai::group::Items;
use burngfx::modern_ai::policy::{AiConfig, AiPolicy};
use burngfx::modern_ai::recruitment::*;
use burngfx::rival::Rival;
use burngfx::trade::Prices;

fn types() -> ItemTypes {
    let p = format!("{}/testdata/v1.2/rules/extended/items.txt", env!("CARGO_MANIFEST_DIR"));
    ItemTypes::parse_v12(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// Maggots 2, rats 3, snake 5.25, meat 6.75, knife 4.
fn prices() -> Prices {
    Prices::parse("0 = 8\n1 = 12\n2 = 21\n3 = 27\n")
}

fn policy() -> AiPolicy {
    let s = |n: u8| format!("[{n}]\nrecruit_experience = 20 60\nslump_material_grants = 0\n");
    AiConfig::parse(&format!("{}{}{}", s(0), s(1), s(2))).unwrap().policy(1).clone()
}

fn k(t: &ItemTypes, n: &str) -> usize {
    t.index_of(n).unwrap_or_else(|| panic!("{n}"))
}

fn person(record: usize, name: &str, xp: i32, asks: &[usize]) -> Candidate {
    Candidate { record, name: name.into(), class: CharClass::Mercenary, experience: xp, health: 100, hire_items: asks.to_vec(), pack: ItemBag::new(6), available: true }
}

fn leader(food: i32, pack: ItemBag) -> Rival {
    let mut r = Rival::new(1, 0);
    r.needs = CharacterNeeds { food, max_food: 9, water: 5, max_water: 5, health: 100 };
    r.pack = pack;
    r
}

#[test]
fn the_policys_experience_is_preferred_never_required() {
    let p = policy();
    let people = [person(1, "a", 10, &[]), person(2, "b", 30, &[]), Candidate { available: false, ..person(3, "c", 40, &[]) }];
    assert_eq!(hireable(&people, &p, &[]).iter().map(|c| c.record).collect::<Vec<_>>(), vec![2]);
    assert_eq!(hireable(&people, &p, &[2]).iter().map(|c| c.record).collect::<Vec<_>>(), vec![1], "sent home: not again; then anyone");
}

#[test]
fn how_a_recruit_is_paid() {
    let t = types();
    let prices = prices();
    let items = Items { types: &t, prices: &prices };
    let (meat, rats, maggots) = (k(&t, "item_meat"), k(&t, "item_rats"), k(&t, "item_maggots"));
    let kit = Kit::default();
    let asks_meat = person(5, "Lee", 30, &[meat]);
    // A leader alone with no camp: the first is free.
    let r = leader(9, ItemBag::with(6, &[rats]));
    let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &[], owned_camps: 0 };
    assert_eq!(recruitment_plan(&purse, &asks_meat, false).payment, Payment::Free);
    // With a camp: the meat he carries, its food a cost.
    let r = leader(9, ItemBag::with(6, &[rats, meat]));
    let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &[], owned_camps: 1 };
    let plan = recruitment_plan(&purse, &asks_meat, false);
    assert_eq!((plan.payment.clone(), plan.food, plan.funded), (Payment::Exact { slot: 1 }, 9, true));
    // No meat: unfunded without a generated payment; with one, food worth
    // 6.75 — rats (3) and maggots (2) are not enough, the snake is.
    let r = leader(9, ItemBag::with(6, &[rats, maggots, k(&t, "item_snake")]));
    let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &[], owned_camps: 1 };
    assert!(!recruitment_plan(&purse, &asks_meat, false).funded);
    let plan = recruitment_plan(&purse, &asks_meat, true);
    match plan.payment {
        Payment::Generated { kind, ref assets } => {
            assert_eq!(kind, meat);
            // Cheapest first: maggots, rats, snake make 10.25. Handing back the
            // dearest first: not the snake (5 would not pay), the rats (7.25
            // still pays), not the maggots (5.25 would not).
            let kinds: Vec<usize> = assets.iter().map(|s| match *s { Source::Member { slot, .. } => r.pack.get(slot).unwrap(), _ => usize::MAX }).collect();
            assert_eq!(kinds, vec![maggots, k(&t, "item_snake")]);
        }
        ref other => panic!("{other:?}"),
    }
    // Never what would leave the group under three days' food: food 0, a
    // snake (7) carried, the snake cannot go (0 + 7 - 7 < 3).
    let r = leader(0, ItemBag::with(6, &[k(&t, "item_snake")]));
    let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &[], owned_camps: 1 };
    assert!(!recruitment_plan(&purse, &person(6, "Al", 30, &[meat]), true).funded);
    // Nor his weapon in hand, though it is food.
    let r = leader(9, ItemBag::with(6, &[k(&t, "item_snake")]));
    let held = Kit { weapon: Some(0), protection: None };
    let purse = Purse { items, r: &r, leader_kit: &held, camp_rooms: &[], owned_camps: 1 };
    assert!(!recruitment_plan(&purse, &person(6, "Al", 30, &[meat]), true).funded);
}

#[test]
fn the_cheapest_asked_then_by_name() {
    let t = types();
    let prices = prices();
    let items = Items { types: &t, prices: &prices };
    let r = leader(9, ItemBag::new(6));
    let kit = Kit::default();
    let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &[], owned_camps: 1 };
    let people = [person(1, "Zed", 30, &[k(&t, "item_meat")]), person(2, "Bo", 30, &[k(&t, "item_rats")]), person(3, "Al", 30, &[k(&t, "item_rats")])];
    assert_eq!(find_recruit_at(&purse, &policy(), &people, &[], false, false).map(|p| p.record), Some(3));
    assert_eq!(find_recruit_at(&purse, &policy(), &people, &[], true, false), None, "nobody affordable");
}

#[test]
fn paying_takes_all_of_it_or_nothing() {
    let t = types();
    let prices = prices();
    let items = Items { types: &t, prices: &prices };
    let (meat, rats, snake) = (k(&t, "item_meat"), k(&t, "item_rats"), k(&t, "item_snake"));
    let mut r = leader(9, ItemBag::with(6, &[k(&t, "item_knife"), meat]));
    let mut kit = Kit { weapon: Some(0), protection: None };
    let plan = {
        let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &[], owned_camps: 1 };
        recruitment_plan(&purse, &person(5, "Lee", 30, &[meat]), false)
    };
    assert!(pay(&mut r, &mut kit, &mut [], &plan));
    assert_eq!((r.pack.len(), kit.weapon), (1, Some(0)));
    // Food from the pack and from a camp's room.
    let kit = Kit::default();
    let mut r = leader(9, ItemBag::with(6, &[rats]));
    let mut rooms = vec![vec![ItemBag::with(4, &[snake])]];
    let plan = {
        let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &rooms, owned_camps: 1 };
        recruitment_plan(&purse, &person(5, "Lee", 30, &[meat]), true)
    };
    assert!(plan.funded);
    let mut kit = Kit::default();
    assert!(pay(&mut r, &mut kit, &mut rooms, &plan));
    assert_eq!((r.pack.len(), rooms[0][0].len()), (0, 0));
    // A plan gone stale (the rats eaten since) takes nothing at all.
    let mut r = leader(9, ItemBag::with(6, &[rats]));
    let mut rooms = vec![vec![ItemBag::with(4, &[snake])]];
    let plan = {
        let purse = Purse { items, r: &r, leader_kit: &kit, camp_rooms: &rooms, owned_camps: 1 };
        recruitment_plan(&purse, &person(5, "Lee", 30, &[meat]), true)
    };
    r.pack = ItemBag::new(6);
    let before = rooms.clone();
    assert!(!pay(&mut r, &mut kit, &mut rooms, &plan));
    assert_eq!(rooms, before);
}
