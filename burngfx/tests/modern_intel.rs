//! `modern_ai::intel`: v1.2's `DefenseIntelligence` and `AttackRetryMemory`.

use burngfx::modern_ai::intel::*;
use burngfx::modern_ai::state::{Force, ModernAi};

const FACTS: CampFacts = CampFacts { capacity: 4, owner_camps: 0, strategic: false };

#[test]
fn an_owner_change_is_remembered_from_its_day() {
    let mut ai = ModernAi::new(1, 5);
    update_knowledge(&mut ai, 3, [(5, Some(2)), (6, None)]);
    update_knowledge(&mut ai, 9, [(5, Some(2)), (6, Some(1))]);
    assert_eq!(observation(&ai, 5).unwrap().owner_since, 3, "the same owner: the old day");
    assert_eq!(observation(&ai, 6).unwrap().owner_since, 9);
}

#[test]
fn a_camp_held_longer_is_expected_to_hold_more() {
    let mut ai = ModernAi::new(1, 5);
    update_knowledge(&mut ai, 0, [(5, Some(2))]);
    assert_eq!(estimate(&ai, 0, 5, FACTS).expected_defenders, 1);
    // Held 60 days, on day 60, by an owner of 4 camps:
    // 0.05 + min(.35, .5) + min(.25, .2) + min(.2, .1) = 0.70.
    let e = estimate(&ai, 60, 5, CampFacts { owner_camps: 4, ..FACTS });
    assert_eq!((e.expected_defenders, e.estimated_strength, e.based_on_contact), (2, 44.0, false));
    // Capacity caps it.
    assert_eq!(estimate(&ai, 300, 5, CampFacts { capacity: 1, owner_camps: 8, strategic: true }).expected_defenders, 1);
}

#[test]
fn contact_is_believed_for_twenty_days() {
    let mut ai = ModernAi::new(1, 5);
    update_knowledge(&mut ai, 1, [(5, Some(2))]);
    observe_encounter(&mut ai, 10, 5, Some(2), 3, 90.0);
    let e = estimate(&ai, 25, 5, FACTS);
    assert_eq!((e.expected_defenders, e.estimated_strength, e.based_on_contact), (4, 120.0, true), "3 seen and one more for ten days");
    let stale = estimate(&ai, 31, 5, FACTS);
    assert!(!stale.based_on_contact && stale.estimated_strength == stale.expected_defenders as f32 * 22.0);
    // Seen by the opponent, kept through a save.
    assert_eq!(observation(&ai, 5).unwrap().defenders, 3);
}

#[test]
fn capacity_is_the_lesser_of_water_and_food() {
    assert_eq!(sustainable_capacity(3, |g| g <= 4), 3);
    assert_eq!(sustainable_capacity(9, |g| g <= 2), 2);
    assert_eq!(sustainable_capacity(0, |_| false), 1);
}

fn force(count: i32, health: i32, strength: f32, boss: i32) -> Force {
    Force { count, health, strength, boss_health: boss }
}

#[test]
fn a_stalemate_blocks_until_the_forces_change() {
    let mut ai = ModernAi::new(1, 5);
    let us = force(2, 180, 50.0, 90);
    let them = force(2, 200, 60.0, 0);
    assert!(!record_attempt(&mut ai, 5, 2, us, them, 0, false).blocked, "the first time: unknown");
    assert!(record_attempt(&mut ai, 5, 2, us, them, 0, false).blocked, "the same again: blocked");
    assert!(!can_retry(&ai, 5, 2, &us, None));
    assert!(!can_retry(&ai, 5, 2, &force(2, 180, 54.0, 90), None), "4 more is not ten per cent");
    assert!(can_retry(&ai, 5, 2, &force(3, 180, 50.0, 90), None), "one more fighter");
    assert!(can_retry(&ai, 5, 2, &us, Some(&force(1, 100, 30.0, 0))), "fewer defenders seen there");
    assert!(can_retry(&ai, 5, 3, &us, None), "another owner now");
    assert!(!record_attempt(&mut ai, 5, 2, us, them, 1, false).blocked, "a defender killed is progress");
    assert!(!record_attempt(&mut ai, 5, 2, us, them, 0, true).blocked && ai.retries.is_empty(), "taken: forgotten");
}
