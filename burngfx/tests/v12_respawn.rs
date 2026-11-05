//! v1.2's `CharacterRespawn` (tag `v1.2`): the location cycle, the name
//! swap that lets an ordinary person come back, Extended's cities, and the
//! creatures' timers.

use burngfx::game::{respawn_body, respawn_face, Pending, Respawns, SpawnCycle};
use burngfx::gamdat::CharClass;
use burngfx::rules::SpawnMethod;

fn pending(record: usize, location: usize, days: i32) -> Pending {
    Pending { record, location, days, food: 0, water: 0, revive_health: 100 }
}

/// Records below 100 are mercenaries, 100s dogs, 200s mutants, 300s traders.
fn class_of(record: usize) -> CharClass {
    match record / 100 {
        0 => CharClass::Mercenary,
        1 => CharClass::Dog,
        2 => CharClass::Mutant,
        _ => CharClass::Trader,
    }
}

fn revive(class: CharClass) -> i32 {
    match class {
        CharClass::Dog | CharClass::Mutant => 31,
        CharClass::Trader => 100,
        _ => 95,
    }
}

fn cycle(npc: i32, city_threshold: i32) -> SpawnCycle {
    SpawnCycle { method: SpawnMethod::AmigaLocationCycle, npc, trader: 50, dog: 5, mutant: 7, city_threshold }
}

#[test]
fn one_person_alone_never_comes_back() {
    let mut r = Respawns::new();
    r.schedule(pending(1, 27, 8));
    for day in 1..40u16 {
        assert!(r.advance_day_v12(day, 37, cycle(8, 0), &class_of, &revive, &mut Vec::new()).is_empty());
    }
    assert_eq!(r.waiting_for(1), Some(8), "an ordinary person does not count down under a cycle");
}

#[test]
fn on_the_locations_day_the_first_comes_back_with_the_others_name() {
    let mut r = Respawns::new();
    r.schedule(pending(61, 27, 8));
    r.schedule(pending(62, 27, 8));
    // (day + 27) % 8 == 0 first on day 5.
    assert!(r.advance_day_v12(4, 37, cycle(8, 0), &class_of, &revive, &mut Vec::new()).is_empty());
    let back = r.advance_day_v12(5, 37, cycle(8, 0), &class_of, &revive, &mut Vec::new());
    assert_eq!(back.len(), 1);
    assert_eq!((back[0].record, back[0].location), (61, 27));
    assert_eq!((back[0].needs.health, back[0].needs.food, back[0].needs.water), (95, 8, 4));
    let renamed = back[0].renamed.expect("a name swap");
    assert_eq!((renamed.donor, renamed.location_turn), (62, 32));
    assert_eq!(r.waiting_for(62), Some(8), "the donor waits on");
    assert!(r.advance_day_v12(13, 37, cycle(8, 0), &class_of, &revive, &mut Vec::new()).is_empty(), "no donor left");
}

#[test]
fn extended_sends_them_to_the_emptiest_city_below_the_threshold() {
    let mut r = Respawns::new();
    r.schedule(pending(1, 3, 4));
    r.schedule(pending(2, 3, 4));
    let mut cities = vec![(0, 12), (8, 9), (13, 4), (18, 4)];
    // (1 + 3) % 4 == 0.
    let back = r.advance_day_v12(1, 37, cycle(4, 12), &class_of, &revive, &mut cities);
    assert_eq!(back[0].location, 13, "the first of the emptiest");
    assert_eq!(cities[2], (13, 5));
    let mut full = vec![(0, 12), (8, 13)];
    let mut r = Respawns::new();
    r.schedule(pending(1, 3, 4));
    r.schedule(pending(2, 3, 4));
    let back = r.advance_day_v12(1, 37, cycle(4, 12), &class_of, &revive, &mut full);
    assert_eq!(back[0].location, 3, "no city below the threshold: where he died");
}

#[test]
fn creatures_count_down_and_a_return_restarts_their_kind_there() {
    let mut r = Respawns::new();
    r.schedule(pending(101, 5, 2));
    r.schedule(pending(102, 5, 3));
    r.schedule(pending(201, 5, 2));
    assert!(r.advance_day_v12(1, 37, cycle(8, 0), &class_of, &revive, &mut Vec::new()).is_empty());
    let back = r.advance_day_v12(2, 37, cycle(8, 0), &class_of, &revive, &mut Vec::new());
    let records: Vec<usize> = back.iter().map(|b| b.record).collect();
    assert_eq!(records, vec![101, 201]);
    assert_eq!(back[0].needs.health, 31);
    assert!(back.iter().all(|b| b.renamed.is_none()));
    assert_eq!(r.waiting_for(102), Some(5), "the other dog there starts the dog's full interval again");
}

#[test]
fn under_timer_everybody_counts_down() {
    let mut r = Respawns::new();
    r.schedule(pending(1, 27, 2));
    r.schedule(pending(2, 27, 9));
    let c = SpawnCycle { method: SpawnMethod::Timer, ..cycle(8, 0) };
    assert!(r.advance_day_v12(1, 37, c, &class_of, &revive, &mut Vec::new()).is_empty());
    let back = r.advance_day_v12(2, 37, c, &class_of, &revive, &mut Vec::new());
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].renamed.map(|n| n.donor), Some(2));
}

#[test]
fn faces_and_bodies() {
    assert_eq!(respawn_face(0), 7);
    assert_eq!(respawn_face(2), 9);
    assert_eq!(respawn_face(3), 11, "face 10 is the gas mask, skipped");
    assert_eq!(respawn_face(20), 28);
    assert_eq!(respawn_body(CharClass::Doctor, 0), Some(16));
    assert_eq!(respawn_body(CharClass::Mercenary, 1), Some(80));
    assert_eq!(respawn_body(CharClass::Technician, 2), Some(144));
    assert_eq!(respawn_body(CharClass::Dog, 0), None);
}
