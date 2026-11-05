//! `LocalCombatEncounter`, with the cases of v1.2's own
//! `Burntime.Remaster.Tests/CombatResolverTests.cs` (`EncounterCases`).
//! Nobody moves unless a test moves them, as upstream's `Update(0)` does
//! not move anyone either; every blow does the attacker's one damage value.

use burngfx::encounter::*;
use burngfx::gamdat::CharClass;

#[derive(Clone, Debug)]
struct Fighter {
    x: f32,
    y: f32,
    health: i32,
    damage: i32,
    range: f32,
    held: bool,
    approaching: Option<usize>,
    fleeing_from: Option<usize>,
}

fn fighter(damage: i32, range: f32, health: i32) -> Fighter {
    Fighter { x: 0.0, y: 0.0, health, damage, range, held: false, approaching: None, fleeing_from: None }
}

struct World(Vec<Fighter>);

impl Arena<usize> for World {
    fn is_dead(&self, who: usize) -> bool {
        self.0[who].health <= 0
    }
    fn same_place(&self, _: usize, _: usize) -> bool {
        true
    }
    fn attack_range(&self, who: usize) -> f32 {
        self.0[who].range
    }
    fn in_range(&self, who: usize, target: usize, range: f32) -> bool {
        let (a, b) = (&self.0[who], &self.0[target]);
        let (dx, dy) = (a.x - b.x, a.y - b.y);
        dx * dx + dy * dy <= range * range
    }
    fn strike(&mut self, attacker: usize, defender: usize) -> bool {
        if self.is_dead(attacker) || self.is_dead(defender) {
            return false;
        }
        self.0[defender].health -= self.0[attacker].damage;
        true
    }
    fn begin_approach(&mut self, who: usize, target: usize, _: f32) {
        let f = &mut self.0[who];
        f.fleeing_from = None;
        f.held = false;
        f.approaching = Some(target);
    }
    fn clear_approach(&mut self, who: usize, target: usize) {
        if self.0[who].approaching == Some(target) {
            self.0[who].approaching = None;
        }
    }
    fn hold(&mut self, who: usize) {
        let f = &mut self.0[who];
        f.fleeing_from = None;
        f.approaching = None;
        f.held = true;
    }
    fn release(&mut self, who: usize) {
        self.0[who].held = false;
    }
    fn flee(&mut self, who: usize, from: usize) {
        self.0[who].fleeing_from = Some(from);
    }
}

#[test]
fn defender_flees_only_after_the_complete_retaliation_batch() {
    let mut w = World(vec![fighter(1, 16.0, 100); 5]);
    let d = 4;
    let mut e = Encounter::new(&[0, 1, 2, 3], d, &mut w);
    e.update(0.0, &mut w);
    assert_eq!(w.0[d].health, 96, "all four group members attack individually");
    assert_eq!(w.0[0].health, 99, "first retaliation resolves");
    assert_eq!([w.0[1].health, w.0[2].health, w.0[3].health], [100, 100, 100], "the rest remain queued");
    assert_eq!(w.0[d].fleeing_from, None, "no flee between retaliations");
    e.update(0.0, &mut w);
    assert_eq!(w.0[1].health, 99);
    assert_eq!(w.0[d].fleeing_from, None);
    e.update(0.0, &mut w);
    assert_eq!(w.0[2].health, 99);
    assert_eq!(w.0[d].fleeing_from, None);
    e.update(0.0, &mut w);
    assert_eq!(w.0[3].health, 99);
    assert_eq!(w.0[d].fleeing_from, Some(3), "flee starts after the queue is empty, from the last answered");
    assert!(e.is_complete());
}

#[test]
fn retaliation_waits_for_the_complete_group_attack_phase() {
    let mut w = World(vec![fighter(1, 16.0, 100); 3]);
    w.0[1].x = 100.0; // distant
    let mut e = Encounter::new(&[0, 1], 2, &mut w);
    e.update(0.0, &mut w);
    assert_eq!(w.0[2].health, 99, "near group member attacks");
    assert_eq!(w.0[0].health, 100, "retaliation waits for the distant member");
    assert!(w.0[0].held, "committed attacker stays available");
    e.cancel_offense(&mut w);
    e.update(0.0, &mut w);
    assert_eq!(w.0[0].health, 99, "retaliation begins after the attack phase ends");
}

#[test]
fn a_completed_attacker_can_move_while_the_group_finishes() {
    let mut w = World(vec![fighter(1, 16.0, 100); 3]);
    w.0[1].x = 100.0;
    let mut e = Encounter::new(&[0, 1], 2, &mut w);
    e.update(0.0, &mut w);
    assert_eq!(w.0[2].health, 99);
    assert!(e.release_after_completed_attack(0, &mut w));
    assert!(!w.0[0].held, "boss movement is released");
    assert!(!e.release_after_completed_attack(1, &mut w), "the follower has not struck");
    w.0[1].x = 0.0;
    e.update(0.0, &mut w);
    assert_eq!(w.0[2].health, 98, "follower attack remains active");
    assert_eq!(w.0[0].health, 99, "boss still receives queued retaliation");
}

#[test]
fn a_new_encounter_stops_both_flights() {
    let mut w = World(vec![fighter(1, 16.0, 100); 2]);
    w.0[0].fleeing_from = Some(1);
    w.0[1].fleeing_from = Some(0);
    let _ = Encounter::new(&[0], 1, &mut w);
    assert_eq!(w.0[0].fleeing_from, None, "attacker stops fleeing to approach");
    assert_eq!(w.0[0].approaching, Some(1));
    assert_eq!(w.0[1].fleeing_from, None, "defender stops fleeing for the new round");
    assert!(w.0[1].held, "defender waits for the group attack");
}

#[test]
fn blocked_retaliation_still_resolves_exactly_once() {
    let mut w = World(vec![fighter(1, 32.0, 100), fighter(1, 16.0, 100)]);
    w.0[0].x = 32.0;
    let mut e = Encounter::new(&[0], 1, &mut w);
    e.update(0.0, &mut w);
    assert_eq!(w.0[1].health, 99, "ranged attack resolves");
    assert_eq!(w.0[0].health, 100, "retaliation initially approaches");
    e.update(2.0, &mut w);
    assert_eq!(w.0[0].health, 99, "a timed-out path cannot discard retaliation");
    assert!(e.is_complete());
}

#[test]
fn weapon_range_overrides_the_unarmed_sixteen() {
    assert_eq!(attack_range(CharClass::Mercenary, None), 16.0);
    assert_eq!(attack_range(CharClass::Mercenary, Some(32)), 32.0);
    assert_eq!(attack_range(CharClass::Mutant, Some(32)), 16.0);
    assert_eq!(attack_range(CharClass::Dog, None), 24.0);
    assert_eq!(attack_range(CharClass::Trader, Some(15)), 24.0);
    assert_eq!(Reach::from_range(16.0), Reach::Close);
    assert_eq!(Reach::from_range(22.0), Reach::Medium);
    assert_eq!(Reach::from_range(29.0), Reach::Long);
    assert_eq!(Reach::from_range(32.0), Reach::Ranged);
}

#[test]
fn a_longer_reach_defender_intercepts_first() {
    let mut w = World(vec![fighter(1, 15.0, 1), fighter(1, 28.0, 100)]);
    let mut e = Encounter::new(&[0], 1, &mut w);
    e.update(0.0, &mut w);
    assert!(w.0[0].health <= 0, "longer reach lands the first strike");
    assert_eq!(w.0[1].health, 100, "a dead attacker cannot answer");
    assert!(e.is_complete());
}

#[test]
fn a_reach_intercept_replaces_rather_than_adds_retaliation() {
    let mut w = World(vec![fighter(1, 15.0, 100), fighter(1, 28.0, 100)]);
    let mut e = Encounter::new(&[0], 1, &mut w);
    e.update(0.0, &mut w);
    assert_eq!(w.0[0].health, 99, "defender intercepts exactly once");
    assert_eq!(w.0[1].health, 99, "surviving attacker answers the intercept");
    assert!(e.is_complete(), "no second retaliation is queued");
    assert_eq!(w.0[1].fleeing_from, Some(0));
}

#[test]
fn the_same_reach_class_keeps_the_initiative() {
    let mut w = World(vec![fighter(1, 15.0, 100), fighter(100, 16.0, 1)]);
    let mut e = Encounter::new(&[0], 1, &mut w);
    e.update(0.0, &mut w);
    assert!(w.0[1].health <= 0, "initiator wins a tied reach class");
    assert_eq!(w.0[0].health, 100, "a dead defender cannot retaliate");
    assert!(e.is_complete());
}

#[test]
fn an_attacker_who_never_arrives_gives_up_after_ten_seconds() {
    let mut w = World(vec![fighter(1, 16.0, 100); 2]);
    w.0[0].x = 200.0;
    let mut e = Encounter::new(&[0], 1, &mut w);
    for _ in 0..9 {
        e.update(1.0, &mut w);
        assert!(!e.is_complete());
    }
    e.update(1.0, &mut w);
    assert!(e.is_complete());
    assert_eq!((w.0[0].health, w.0[1].health), (100, 100));
    assert_eq!(w.0[1].fleeing_from, None, "nothing happened, nothing to flee from");
}

#[test]
fn nobody_to_attack_is_complete_at_once() {
    let mut w = World(vec![fighter(1, 16.0, 100), fighter(1, 16.0, 0)]);
    assert!(Encounter::new(&[1], 1, &mut w).is_complete(), "the defender is not its own attacker");
    let mut w = World(vec![fighter(1, 16.0, 100), fighter(1, 16.0, 0)]);
    assert!(Encounter::new(&[1], 0, &mut w).is_complete(), "a dead attacker takes no part");
}
