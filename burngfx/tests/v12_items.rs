//! v1.2's item rules (`burngfx::v12_items`) and the bible and hazards in the
//! day (`burngfx::v12_day`), against the preset tables at tag `v1.2`.

use burngfx::game::{CharacterNeeds, Companion, ItemBag, Party, Point};
use burngfx::gamdat::Danger;
use burngfx::items::ItemTypes;
use burngfx::rules::{RuleBook, Survival};
use burngfx::spawn::{Dropped, Rng};
use burngfx::v12_day::{party_day, party_day_in, Hazard, Upkeep};
use burngfx::v12_items::*;

fn testdata(rel: &str) -> String {
    format!("{}/testdata/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn types(set: &str) -> ItemTypes {
    ItemTypes::parse_v12(&std::fs::read_to_string(testdata(&format!("v1.2/rules/{set}/items.txt"))).unwrap()).unwrap()
}

fn book(set: &str) -> RuleBook {
    RuleBook::parse(&std::fs::read_to_string(testdata(&format!("v1.2/rules/{set}/game.txt"))).unwrap())
}

fn kind(t: &ItemTypes, name: &str) -> usize {
    t.index_of(name).unwrap_or_else(|| panic!("{name}"))
}

#[test]
fn the_presets_say_where_this_applies() {
    let (c, e) = (book("classic"), book("extended"));
    assert_eq!(c.rules.dropped_food_decay_interval, 0);
    assert_eq!(e.rules.dropped_food_decay_interval, 10);
    assert_eq!(c.difficulty[0].mutant_drop_chance, 0.0);
    assert_eq!(e.difficulty[0].mutant_drop_chance, 0.2);
    assert_eq!(c.rules.gas_immune_faces, vec![10]);
    let classic = types("dos");
    assert!((0..classic.len()).all(|k| classic.get(k).unwrap().functions.is_empty()), "Classic's items do nothing special");
}

#[test]
fn food_rots_on_its_locations_day_first_food_first() {
    let t = types("extended");
    let (meat, knife, snake) = (kind(&t, "item_meat"), kind(&t, "item_knife"), kind(&t, "item_snake"));
    let d = |location, kind| Dropped { location, at: Point::new(0, 0), kind, rounds: None, icon: burngfx::spawn::PILE_UNDRAWN };
    let mut ground = vec![d(3, knife), d(4, meat), d(3, snake), d(3, meat)];
    assert!(decay_dropped_food(&mut ground, 3, 6, 10, &t).is_none(), "(6 + 3) % 10 != 0");
    let gone = decay_dropped_food(&mut ground, 3, 7, 10, &t).unwrap();
    assert_eq!(gone.kind, snake, "the first food lying there, not the knife");
    assert_eq!(ground.len(), 3);
    assert!(decay_dropped_food(&mut ground, 3, 7, 0, &t).is_none(), "Classic: never");
}

#[test]
fn a_mutant_drops_by_the_presets_chance() {
    let t = types("extended");
    let useless = vec![String::from("useless")];
    let mut dropped = 0;
    for seed in 0..1000 {
        if let Some(k) = mutant_drop(&t, &useless, 0.2, &mut Rng::seeded(seed)) {
            dropped += 1;
            assert!(t.get(k).unwrap().is_class("useless") && !t.get(k).unwrap().is_class("nodrop"));
        }
    }
    assert!((150..=250).contains(&dropped), "{dropped} of 1000");
    assert!(mutant_drop(&t, &useless, 0.0, &mut Rng::seeded(1)).is_none(), "Classic's chance is nought");
}

#[test]
fn the_detector_and_the_radio() {
    assert_eq!(discovery_chance(0), 0.0);
    assert!((discovery_chance(1) - 0.01).abs() < 1e-12);
    assert!((discovery_chance(3) - (1.0 - 0.99f64.powi(3))).abs() < 1e-12);
    assert_eq!(threat_level(0.0), 0);
    assert_eq!(threat_level(1.0), 1);
    assert_eq!(threat_level(25.0), 1);
    assert_eq!(threat_level(25.5), 2);
    assert_eq!(threat_level(1000.0), 4);
}

#[test]
fn protection_is_chosen_for_the_danger_first() {
    let t = types("extended");
    let mask = kind(&t, "item_gas_mask");
    let jacket = kind(&t, "item_leather_jacket");
    let pack = ItemBag::with(6, &[jacket, mask]);
    assert_eq!(preferred_protection(&pack, None, Danger::Gas, &t), Some(1), "gas: the mask, though the jacket defends better");
    assert_eq!(preferred_protection(&pack, None, Danger::None, &t), Some(0), "no danger: the best defence");
    assert_eq!(preferred_protection(&pack, Some(1), Danger::Radiation, &t), Some(0),
               "nothing keeps radiation off: the best defence, even over what is chosen");
}

fn upkeep(survival: Survival, travelling: bool) -> Upkeep {
    Upkeep { survival, stabilization: 0, doctor: false, threshold: None, travelling }
}

#[test]
fn a_bible_spares_food_and_water_at_rest_only() {
    let t = types("extended");
    let bible = kind(&t, "item_bible");
    for survival in [Survival::DosDailyConsumption, Survival::RemasterSupplyPool] {
        let mut leader = CharacterNeeds { food: 0, max_food: 9, water: 0, max_water: 5, health: 80 };
        let mut pack = ItemBag::with(6, &[bible]);
        party_day(&mut leader, &mut pack, &mut Party::new(), &t, upkeep(survival, false));
        assert_eq!((leader.food, leader.water), (0, 0), "{survival:?}: nothing spent");
        assert_eq!(leader.health, 82, "{survival:?}: healed, not starved");
        party_day(&mut leader, &mut pack, &mut Party::new(), &t, upkeep(survival, true));
        assert!(leader.health < 80, "{survival:?}: on the road the bible spares nobody");
    }
}

#[test]
fn the_daily_check_needs_full_protection() {
    let t = types("extended");
    let suit = kind(&t, "item_protective_suit");
    let mask = kind(&t, "item_gas_mask");
    let face_list = [(77usize, 10u8)];
    let gas = Hazard { danger: Danger::Gas, immune: &[10], leader_face: 0, faces: &face_list, exempt: false };
    let well = || CharacterNeeds { food: 5, max_food: 9, water: 5, max_water: 5, health: 100 };

    // A leader with nothing: dead at the day's end.
    let mut leader = well();
    let out = party_day_in(&mut leader, &mut ItemBag::new(6), &mut Party::new(), &t, upkeep(Survival::DosDailyConsumption, false), Some(&gas));
    assert!(out.leader_died);

    // With a mask (full gas protection): alive. On the road: not checked.
    let mut leader = well();
    let out = party_day_in(&mut leader, &mut ItemBag::with(6, &[mask]), &mut Party::new(), &t, upkeep(Survival::DosDailyConsumption, false), Some(&gas));
    assert!(!out.leader_died, "{}", t.get(mask).unwrap().protection_against(Danger::Gas));
    let mut leader = well();
    let out = party_day_in(&mut leader, &mut ItemBag::new(6), &mut Party::new(), &t, upkeep(Survival::DosDailyConsumption, true), Some(&gas));
    assert!(!out.leader_died);

    // A companion with face 10 is immune to gas; an AI's group is spared.
    let mut party = Party::new();
    let id = party.recruit();
    if let Some(c) = party.companions.iter_mut().find(|c| c.id == id) {
        *c = Companion { origin: Some(77), needs: well(), ..Companion::new(id) };
    }
    let mut leader = well();
    let out = party_day_in(&mut leader, &mut ItemBag::with(6, &[suit]), &mut party, &t, upkeep(Survival::DosDailyConsumption, false), Some(&gas));
    assert!(!out.leader_died && out.deaths.is_empty() && party.len() == 1);
    let mut leader = well();
    let spared = Hazard { exempt: true, ..gas };
    let out = party_day_in(&mut leader, &mut ItemBag::new(6), &mut Party::new(), &t, upkeep(Survival::DosDailyConsumption, false), Some(&spared));
    assert!(!out.leader_died);
}
