//! v1.2 table combat (`TableCombat.cs`, `RuleFormulas.cs`), against the
//! shipped Classic tables where they are supplied (`testdata/v1.2`).

use burngfx::combat::Kit;
use burngfx::game::ItemBag;
use burngfx::gamdat::CharClass;
use burngfx::items::ItemTypes;
use burngfx::rules::{RuleBook, RuleSet};
use burngfx::table_combat::*;
use std::path::PathBuf;

fn read(rel: &str) -> Option<String> {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/v1.2").join(rel)).ok()
}

fn classic_items() -> Option<ItemTypes> {
    ItemTypes::parse_v12(&read(RuleSet::Classic.files().items)?).ok()
}

const KNIFE: [i32; 16] = [9, 11, 14, 16, 10, 13, 16, 18, 12, 14, 17, 20, 14, 16, 18, 20];

#[test]
fn tiers_by_combat_experience() {
    assert_eq!(combat_experience(true, 37), 37);
    assert_eq!(combat_experience(false, 37), 18);
    assert_eq!(experience_tier(18, 25), Ok(0));
    assert_eq!(experience_tier(37, 25), Ok(1));
    assert_eq!(experience_tier(75, 25), Ok(3));
    assert_eq!(experience_tier(99, 25), Ok(3));
    assert_eq!(experience_tier(-5, 25), Ok(0));
    assert_eq!(experience_tier(10, 0), Err(TableError::BadTierWidth(0)));
}

#[test]
fn a_blow_is_one_of_the_tiers_four() {
    assert_eq!(damage_row(&KNIFE, 37, 25), Ok(&[10, 13, 16, 18][..]));
    assert_eq!(damage(&KNIFE, 37, 25, 2), Ok(16));
    assert_eq!(damage(&KNIFE, 99, 25, 0), Ok(14));
    // A doctor at 37 fights at 18: the lowest tier.
    assert_eq!(damage(&KNIFE, combat_experience(false, 37), 25, 3), Ok(16));
    // Four rolls: the table itself, any experience. One value: always it.
    assert_eq!(damage(&[2, 3, 4, 6], 99, 25, 3), Ok(6));
    assert_eq!(damage(&[7], 0, 25, 3), Ok(7));
    assert_eq!(preview(&KNIFE, 60, 25), Ok((12, 20)));
    assert_eq!(damage(&[1, 2], 0, 25, 0), Err(TableError::BadLength(2)));
    assert_eq!(damage(&[], 0, 25, 0), Err(TableError::BadLength(0)));
    let s = strategic_strength(&[2, 3, 4, 6], 0, 25, 80, true).unwrap();
    assert!((s - (15.0 / 4.0 + 8.0)).abs() < 1e-6);
}

/// `ApplyArmour`: `Math.Round(damage * (100 - p) / 100f, AwayFromZero)`,
/// at least one.
#[test]
fn armour_takes_its_share_off() {
    assert_eq!(apply_armour(15, 25), 11); // 11.25
    assert_eq!(apply_armour(10, 25), 8); // 7.5, away from zero
    assert_eq!(apply_armour(1, 90), 1); // 0.1, but at least one
    assert_eq!(apply_armour(20, 100), 1);
    assert_eq!(apply_armour(0, 50), 0);
    assert_eq!(apply_armour(12, -10), 12); // clamped to 0..100
    assert_eq!(apply_armour(12, 250), 1);
}

#[test]
fn creatures_fight_from_the_difficulty_tables() {
    let Some(text) = read("rules/classic/game.txt") else { return };
    let book = RuleBook::parse(&text);
    assert_eq!(creature_table(&book, CharClass::Trader, 1), Some(&[9, 11, 13, 15][..]));
    assert_eq!(creature_table(&book, CharClass::Dog, 7), Some(&[2, 3, 4, 6][..]));
    assert_eq!(creature_table(&book, CharClass::Mercenary, 1), None);
}

#[test]
fn the_shipped_unarmed_line_and_knife() {
    let Some(t) = classic_items() else { return };
    assert_eq!(damage(t.unarmed(), 37, 25, 1), Ok(8));
    let knife = t.index_of("item_knife").unwrap();
    assert_eq!(<ItemTypes as Tables>::damage_table(&t, knife), &KNIFE[..]);
}

/// A human's man never reaches for a firearm on his own; a computer
/// player's ranks by priority, and a held weapon is replaced only by a
/// better one that needs no rounds.
#[test]
fn who_picks_which_weapon() {
    let Some(t) = classic_items() else { return };
    let knife = t.index_of("item_knife").unwrap();
    let rifle = t.index_of("item_loaded_rifle").unwrap();

    let rifle_first = ItemBag::with(6, &[rifle, knife]);
    assert_eq!(find_original_weapon(&rifle_first, None, true, false, &t), Some(1), "human: the knife");
    assert_eq!(find_original_weapon(&rifle_first, None, false, false, &t), Some(0), "AI: the first weapon, kept");

    let knife_first = ItemBag::with(6, &[knife, rifle]);
    assert_eq!(find_original_weapon(&knife_first, None, false, false, &t), Some(0), "the rifle needs rounds");
    // Chosen by hand, the rifle stays.
    assert_eq!(find_original_weapon(&knife_first, Some(1), true, false, &t), Some(1));

    let nothing = ItemBag::with(6, &[]);
    assert_eq!(find_original_weapon(&nothing, None, true, false, &t), None);
}

/// Six rounds, then the empty form; reloaded from ammunition in the pack,
/// which is used up and closes the pack up.
#[test]
fn a_rifle_spends_its_rounds_and_reloads() {
    let Some(t) = classic_items() else { return };
    let rifle = t.index_of("item_loaded_rifle").unwrap();
    let ammo = t.index_of("item_ammunition").unwrap();
    let knife = t.index_of("item_knife").unwrap();

    let mut pack = ItemBag::with(6, &[ammo, rifle, knife]);
    let mut kit = Kit { weapon: Some(1), protection: Some(2) };
    let mut slot = 1;
    for _ in 0..5 {
        slot = use_original_weapon(&mut pack, &mut kit, slot, &t).unwrap();
        assert_eq!(pack.get(slot), Some(rifle));
    }
    assert_eq!(pack.rounds_left(slot), Some(1));
    // The sixth: empty, then loaded again from the ammunition before it.
    slot = use_original_weapon(&mut pack, &mut kit, slot, &t).unwrap();
    assert_eq!(slot, 0);
    assert_eq!(pack.get(0), Some(rifle));
    assert_eq!(pack.rounds_left(0), None, "a fresh load: the type's own rounds");
    assert_eq!(pack.len(), 2);
    assert_eq!(kit, Kit { weapon: Some(0), protection: Some(1) });

    // No ammunition: the rifle is left empty and is no longer the weapon.
    for _ in 0..6 {
        slot = use_original_weapon(&mut pack, &mut kit, slot, &t).unwrap();
    }
    assert_eq!(pack.get(slot), t.index_of("item_unloaded_rifle"));
    assert_eq!(kit.weapon, None);
}
