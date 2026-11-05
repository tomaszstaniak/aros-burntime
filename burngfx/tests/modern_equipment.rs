//! `modern_ai::equipment` and the leader's kept places
//! (`group::maintain_leader_roles`), against Extended's v1.2 item table.

use burngfx::combat::Kit;
use burngfx::game::ItemBag;
use burngfx::items::ItemTypes;
use burngfx::modern_ai::equipment::*;
use burngfx::modern_ai::group::{self, Items, OwnCamp};
use burngfx::modern_ai::policy::AiConfig;
use burngfx::modern_ai::state::ModernAi;
use burngfx::rival::Rival;
use burngfx::trade::Prices;

fn types() -> ItemTypes {
    let p = format!("{}/testdata/v1.2/rules/extended/items.txt", env!("CARGO_MANIFEST_DIR"));
    ItemTypes::parse_v12(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn k(t: &ItemTypes, n: &str) -> usize {
    t.index_of(n).unwrap_or_else(|| panic!("{n}"))
}

/// A policy with the limits a test wants; the rest from a minimal file.
fn policy(firearm_limit: i32, armour_limit: i32) -> burngfx::modern_ai::policy::AiPolicy {
    let section = |n: u8| {
        format!(
            "[{n}]\nrecruit_experience = 0 40\nslump_material_grants = 0\nfirearm_limit = {firearm_limit}\narmour_limit = {armour_limit}\n\
             equipment_upgrade_chance = 0\nfirearm_purchase_chance = 0\n"
        )
    };
    AiConfig::parse(&format!("{}{}{}", section(0), section(1), section(2))).unwrap().policy(1).clone()
}

struct Body {
    pack: ItemBag,
    kit: Kit,
}

fn body(t: &ItemTypes, items: &[&str]) -> Body {
    Body { pack: ItemBag::with(6, &items.iter().map(|n| k(t, n)).collect::<Vec<_>>()), kit: Kit::default() }
}

fn members(bodies: &mut [Body], party: usize) -> Vec<Member<'_>> {
    bodies
        .iter_mut()
        .enumerate()
        .map(|(i, b)| Member { pack: &mut b.pack, kit: &mut b.kit, experience: 0, in_party: i < party, location: 0 })
        .collect()
}

fn names(t: &ItemTypes, pack: &ItemBag) -> Vec<String> {
    pack.iter().map(|k| t.name_of(k).unwrap().to_string()).collect()
}

#[test]
fn a_spare_gun_goes_to_the_first_who_needs_it() {
    let t = types();
    let mut b = vec![body(&t, &["item_knife"]), body(&t, &["item_axe", "item_loaded_rifle"])];
    distribute_firearms(&t, &mut members(&mut b, 2), &policy(-1, 0));
    assert_eq!((names(&t, &b[0].pack), names(&t, &b[1].pack)), (vec!["item_loaded_rifle".into()], vec!["item_axe".into(), "item_knife".into()]));
    assert_eq!(b[0].kit.weapon, Some(0), "the gun is his weapon now");
    // A limit of one: the follower carrying the spare rifle already counts
    // as the group's one gun (`CanAssignFirearm` counts everybody else who
    // has one), so it does not move — upstream's rule, kept.
    let mut b = vec![body(&t, &["item_knife"]), body(&t, &["item_axe", "item_loaded_rifle"])];
    distribute_firearms(&t, &mut members(&mut b, 2), &policy(1, 0));
    assert_eq!(names(&t, &b[0].pack), vec!["item_knife".to_string()]);
    // A full pack with no weapon in it takes nothing.
    let full = ["item_wire"; 6];
    let mut b = vec![body(&t, &full), body(&t, &["item_axe", "item_loaded_rifle"])];
    distribute_firearms(&t, &mut members(&mut b, 2), &policy(-1, 0));
    assert_eq!(b[1].pack.len(), 2);
}

#[test]
fn an_empty_gun_is_loaded_from_the_reserve_or_stays_empty() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let mut ai = ModernAi::new(1, 5);
    ai.reserve.push((k(&t, "item_ammunition"), 1));
    let mut b = vec![body(&t, &["item_unloaded_rifle"])];
    maintain(items, &mut members(&mut b, 1), &mut ai, false, &policy(1, 0), &mut []);
    assert_eq!(names(&t, &b[0].pack), vec!["item_loaded_rifle".to_string()]);
    assert_eq!((b[0].pack.rounds_left(0), ai.reserve_count(k(&t, "item_ammunition"))), (Some(6), 0));
    assert_eq!(b[0].kit.weapon, Some(0));
    // Nothing to load it with.
    let mut b = vec![body(&t, &["item_unloaded_rifle"])];
    maintain(items, &mut members(&mut b, 1), &mut ai, false, &policy(1, 0), &mut []);
    assert_eq!(names(&t, &b[0].pack), vec!["item_unloaded_rifle".to_string()]);
}

#[test]
fn armour_moves_through_the_reserve_in_extended_only() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let fresh = || vec![body(&t, &["item_sweater"]), body(&t, &["item_pants", "item_leather_jacket"])];
    // Classic: nobody's clothing moves.
    let mut ai = ModernAi::new(1, 5);
    let mut b = fresh();
    maintain(items, &mut members(&mut b, 2), &mut ai, false, &policy(1, 10), &mut []);
    assert_eq!((b[0].pack.len(), b[1].pack.len(), ai.reserve.len()), (1, 2, 0));
    // Extended, limit ten: the jacket (twenty) is spare and goes into the
    // reserve; the pants go to the leader, who chose them; the sweater down.
    let mut b = fresh();
    let out = maintain(items, &mut members(&mut b, 2), &mut ai, true, &policy(1, 10), &mut []);
    assert_eq!((names(&t, &b[0].pack), names(&t, &b[1].pack)), (vec!["item_pants".into()], vec!["item_sweater".into()]));
    assert_eq!(b[0].kit.protection, Some(0));
    assert_eq!(ai.reserve_count(k(&t, "item_leather_jacket")), 1);
    assert!(out.left.is_empty());
    // A full reserve: the jacket is left where its wearer stands.
    let mut ai = ModernAi::new(1, 5);
    ai.reserve.push((k(&t, "item_leather_jacket"), 5));
    let mut b = fresh();
    let out = maintain(items, &mut members(&mut b, 2), &mut ai, true, &policy(1, 10), &mut []);
    assert_eq!(out.left, vec![(0, k(&t, "item_leather_jacket"), None)]);
}

#[test]
fn the_reserve_upgrades_and_takes_the_old_back() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let mut ai = ModernAi::new(1, 5);
    ai.reserve.push((k(&t, "item_pants"), 1));
    let mut b = vec![body(&t, &["item_knife", "item_sweater"])];
    let out = maintain(items, &mut members(&mut b, 1), &mut ai, true, &policy(1, 10), &mut []);
    assert_eq!(names(&t, &b[0].pack), vec!["item_knife".to_string(), "item_pants".into()]);
    assert_eq!((b[0].kit.protection, b[0].kit.weapon), (Some(1), Some(0)));
    assert_eq!((ai.reserve_count(k(&t, "item_pants")), ai.reserve_count(k(&t, "item_sweater"))), (0, 1));
    assert_eq!(out.from_reserve, vec![(0, k(&t, "item_pants"))]);
    // Clothing in the camp where he stands joins the reserve first.
    let mut ai = ModernAi::new(1, 5);
    let mut room = ItemBag::with(4, &[k(&t, "item_pants")]);
    let mut b = vec![body(&t, &[])];
    maintain(items, &mut members(&mut b, 1), &mut ai, true, &policy(1, 10), &mut [&mut room]);
    assert_eq!((room.len(), names(&t, &b[0].pack)), (0, vec!["item_pants".to_string()]));
}

#[test]
fn demand_is_a_days_stable_chance_less_what_is_spare() {
    let t = types();
    assert_eq!(opportunity(4, 1, "item_axe", 37), opportunity(4, 1, "item_axe", 37));
    assert!(opportunity(4, 1, "item_axe", 100) && !opportunity(4, 1, "item_axe", 0));
    let mut ai = ModernAi::new(1, 5);
    let mut b = vec![body(&t, &[]), body(&t, &[]), body(&t, &["item_axe"])];
    // Two unarmed want a knife whatever the chance (normal difficulty); one in the reserve.
    ai.reserve.push((k(&t, "item_knife"), 1));
    assert_eq!(demand(&t, &members(&mut b, 3), &ai, &policy(1, 10), true, 3, 1, k(&t, "item_knife")), 1);
    // Armour above the limit, or in Classic, is never wanted.
    assert_eq!(demand(&t, &members(&mut b, 3), &ai, &policy(1, 10), true, 3, 1, k(&t, "item_leather_jacket")), 0);
    assert_eq!(demand(&t, &members(&mut b, 3), &ai, &policy(1, 30), false, 3, 1, k(&t, "item_leather_jacket")), 0);
}

#[test]
fn the_leader_keeps_room_for_food_water_and_protection() {
    let t = types();
    let prices = Prices::default();
    let items = Items { types: &t, prices: &prices };
    let mut r = Rival::new(1, 0);
    r.pack = ItemBag::with(6, &["item_wire", "item_screws", "item_woodpile", "item_tin", "item_rags", "item_knife"].map(|n| k(&t, n)));
    let id = r.party.recruit();
    r.party.companions.iter_mut().find(|c| c.id == id).unwrap().pack = ItemBag::with(6, &["item_meat", "item_water_bottle", "item_gas_mask"].map(|n| k(&t, n)));
    let mut kit = Kit { weapon: Some(5), protection: None };
    // The knife is his weapon and stays; three things go to the follower,
    // and the three places are filled from him.
    group::maintain_leader_roles(&mut r, &mut kit, items, None, |_| 0.0, None);
    let leader = names(&t, &r.pack);
    assert!(leader.contains(&"item_knife".to_string()));
    for n in ["item_meat", "item_water_bottle", "item_gas_mask"] {
        assert!(leader.contains(&n.to_string()), "{n} in {leader:?}");
    }
    assert_eq!(r.pack.get(kit.weapon.unwrap()), Some(k(&t, "item_knife")), "the weapon slot follows the knife");
    assert_eq!(r.party.companions[0].pack.len(), 3);
    // No room anywhere: the leader keeps what he has.
    let mut r2 = Rival::new(1, 0);
    r2.pack = ItemBag::with(6, &["item_wire"; 6].map(|n| k(&t, n)));
    let before = r2.pack.clone();
    group::maintain_leader_roles(&mut r2, &mut Kit::default(), items, None, |_| 0.0, None);
    assert_eq!(r2.pack, before);
    // With a camp that takes things, they go there.
    let mut stored = Vec::new();
    let mut store = |kind: usize, _| {
        stored.push(kind);
        true
    };
    group::maintain_leader_roles(&mut r2, &mut Kit::default(), items, None, |_| 0.0, Some(OwnCamp { store: &mut store, rooms: None }));
    assert_eq!((stored.len(), r2.pack.len()), (3, 3));
}
