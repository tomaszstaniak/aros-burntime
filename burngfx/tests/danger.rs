//! Gas, radiation and what keeps them off.
//!
//! The numbers are v0.9.1's, and `v1.1` does not touch one of them: the two
//! `Danger`/`DangerProtection` files, `Character.GetDangerRate`,
//! `Character.Update` and every `protection=` line of both item tables are
//! byte-identical between the tags.

use burngfx::danger::{damage, health_decrease, rate_percent, Exposure, UNIT};
use burngfx::gamdat::Danger;
use burngfx::items::ItemTypes;
use burngfx::mind::SECOND;
use std::path::PathBuf;

fn types(file: &str) -> Option<ItemTypes> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(file);
    Some(ItemTypes::parse(&std::fs::read_to_string(path).ok()?))
}

/// Whole points lost standing in it for so many seconds, step by step.
fn lost(danger: Danger, protection: u32, seconds: u32) -> i32 {
    let mut exposure = Exposure::default();
    (0..seconds * SECOND).map(|_| exposure.step(danger, protection, 1)).sum()
}

/// `HealthDecrease` is hard-coded and does not read the location's number.
#[test]
fn radiation_hurts_nearly_three_times_as_fast_as_gas() {
    assert_eq!(health_decrease(Danger::Radiation), 1.35);
    assert_eq!(health_decrease(Danger::Gas), 0.5);
    assert_eq!(health_decrease(Danger::None), 0.0);
}

#[test]
fn the_classic_table_protects_against_gas_with_a_mask_and_both_with_a_suit() {
    let Some(types) = types("items_original.txt") else { return };
    let mask = types.iter().find(|t| t.name == "item_gas_mask").unwrap();
    assert_eq!(mask.protection_against(Danger::Gas), 100);
    assert_eq!(mask.protection_against(Danger::Radiation), 0);

    let suit = types.iter().find(|t| t.name == "item_protective_suit").unwrap();
    assert_eq!(suit.protection_against(Danger::Gas), 100);
    assert_eq!(suit.protection_against(Danger::Radiation), 100);
}

/// Extended's paper helmet is the only partial protection in either table,
/// and the only reason the rate is not a yes-or-no.
#[test]
fn the_paper_helmet_is_extendeds_alone_and_stops_a_fifth() {
    let Some(original) = types("items_original.txt") else { return };
    assert!(original.iter().all(|t| t.name != "item_paper_helmet"));

    let Some(extended) = types("items.txt") else { return };
    let helmet = extended.iter().find(|t| t.name == "item_paper_helmet").unwrap();
    assert_eq!(helmet.protection_against(Danger::Gas), 20);
    assert_eq!(helmet.protection_against(Danger::Radiation), 20);
}

/// Full protection takes the whole branch away rather than scaling it down.
#[test]
fn a_suit_stops_radiation_entirely_and_a_mask_does_not_help() {
    assert_eq!(rate_percent(Danger::Radiation, 100), 0);
    assert_eq!(lost(Danger::Radiation, 100, 60), 0);
    assert_eq!(rate_percent(Danger::Radiation, 0), 100);
    assert!((damage(Danger::Radiation, 0, 10.0) - 13.5).abs() < 0.001);
}

/// Twenty seconds of radiation is 27 points. The first version of this
/// carried the fraction in an `f32` and made it 26: 360 additions of 0.075
/// do not reach 27.
#[test]
fn twenty_seconds_of_radiation_is_exactly_twenty_seven() {
    assert_eq!(lost(Danger::Radiation, 0, 20), 27);
    assert_eq!(lost(Danger::Radiation, 0, 40), 54);
}

#[test]
fn a_paper_helmet_leaves_four_fifths_of_it() {
    // 21.6 over twenty seconds; the 0.6 stays carried.
    assert_eq!(lost(Danger::Radiation, 20, 20), 21);
    assert_eq!(lost(Danger::Gas, 20, 10), 4);
}

/// The fraction is carried between steps instead of being rounded away
/// eighteen times a second, which would make gas harmless.
#[test]
fn a_minute_of_gas_costs_thirty_points() {
    assert_eq!(lost(Danger::Gas, 0, 2), 1);
    assert_eq!(lost(Danger::Gas, 0, 60), 30);
}

/// A carry is always less than one point, which is what lets a save refuse
/// anything else.
#[test]
fn what_is_carried_is_never_a_whole_point() {
    let mut exposure = Exposure::default();
    for _ in 0..1000 {
        exposure.step(Danger::Radiation, 0, 1);
        assert!(exposure.carried < UNIT);
    }
}

/// Stepping twice by ten is stepping once by twenty: a save in between must
/// not change the total, and it does not if the carry is kept.
#[test]
fn cutting_the_time_in_two_changes_nothing() {
    let mut once = Exposure::default();
    let whole = once.step(Danger::Gas, 20, 20 * SECOND);

    let mut first = Exposure::default();
    let a = first.step(Danger::Gas, 20, 7 * SECOND + 5);
    let resumed = Exposure { carried: first.carried, lifted: false };
    let mut second = resumed;
    let b = second.step(Danger::Gas, 20, 13 * SECOND - 5);
    assert_eq!(a + b, whole);
    assert_eq!(second, once);
}

#[test]
fn leaving_does_not_bank_the_fraction() {
    let mut exposure = Exposure::default();
    exposure.step(Danger::Gas, 0, 10);
    exposure.clear();
    assert_eq!(exposure, Exposure::default());
}

/// v1.2 reads the two rates from the preset's `game.txt`
/// (`GameSettings.HazardDamage`: `[rules] gas_damage_per_second`,
/// `radiation_damage_per_second`, `UP/Logic/Generation/GameSettings.cs:190`)
/// and `Character.ApplyContinuousHazard` spends `HazardDamage(type) *
/// elapsed * rate` (`Character.cs:938-954`). A changed file changes the
/// damage; a missing key is `GetFloat`'s 0.
#[test]
fn v12_reads_the_hazard_rates_from_game_txt() {
    let book = burngfx::rules::RuleBook::parse("[rules]\ngas_damage_per_second = 0.75\nradiation_damage_per_second = 2\n");
    assert_eq!(book.hazard_damage_milli(Danger::Gas), 750);
    assert_eq!(book.hazard_damage_milli(Danger::Radiation), 2000);
    assert_eq!(book.hazard_damage_milli(Danger::None), 0);
    let mut e = Exposure::default();
    // Four seconds of gas at 0.75 unprotected: three points.
    assert_eq!(e.step_at(book.hazard_damage_milli(Danger::Gas), Danger::Gas, 0, 4 * burngfx::mind::SECOND), 3);
    assert_eq!(burngfx::rules::RuleBook::parse("[rules]\n").hazard_damage_milli(Danger::Gas), 0);
}

/// **RULE-41, v1.2's float health** (`Character.cs:45,196-200,351-354`,
/// `ApplyContinuousHazard`): the number held is the getter, `(int)h`, the
/// fraction above it is carried, leaving the danger keeps it, a write drops
/// it, and between nought and one he lives with a getter of nought.
#[test]
fn a_v12_game_holds_v1_2_s_float_health() {
    use burngfx::danger::{spend, whole};
    let rad = |h: &mut i32, e: &mut Exposure, steps: u32| e.wear(h, 1350, Danger::Radiation, 0, steps);
    // 50 − 1.35 = 48.65: reads 48, not 49.
    let (mut h, mut e) = (50, Exposure::default());
    rad(&mut h, &mut e, SECOND);
    assert_eq!((h, whole(h, &e), e.carried), (48, 48, 65 * UNIT / 100));
    // Another second: 47.30 — the fraction kept between the two.
    rad(&mut h, &mut e, SECOND);
    assert_eq!((h, e.carried), (47, 30 * UNIT / 100));
    // A write (`Health += 2`): 47 + 2 = 49, nothing above.
    spend(&mut h, &mut e);
    h += 2;
    assert_eq!((h, e), (49, Exposure::default()));
    // 1.0 − 0.675 = 0.325: alive (`IsDead` is `health <= 0`), reads 0.
    let (mut h, mut e) = (1, Exposure::default());
    rad(&mut h, &mut e, SECOND / 2);
    assert!(h > 0 && e.lifted);
    assert_eq!(whole(h, &e), 0);
    // `Health -= 5` from there: 0 − 5.
    spend(&mut h, &mut e);
    h -= 5;
    assert_eq!(h, -5);
    // And at nought or below he is dead: 0.325 − 0.675 = −0.35, `(int)` 0.
    let (mut h, mut e) = (1, Exposure::default());
    rad(&mut h, &mut e, SECOND);
    assert!(h <= 0 && !e.lifted);
}
