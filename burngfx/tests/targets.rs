//! `burngfx::targets`: `MapViewOverlayNearbyAction`'s candidates, default
//! target, lock and Tab cycle.

use burngfx::targets::{candidates, Candidate, Key, Targets, CHARACTER_RELEASE_RANGE};

fn c(key: Key, distance: f32) -> Candidate {
    Candidate { key, distance }
}

#[test]
fn candidates_are_in_range_and_nearest_first() {
    let doors = [c(Key::Door(2), 12.0)];
    let items = [c(Key::Item(1), 19.9), c(Key::Item(2), 20.0)];
    let people = [c(Key::Person(5), 34.0), c(Key::Person(6), 35.0), c(Key::Person(7), 3.0)];
    let got: Vec<Key> = candidates(&doors, &items, &people).iter().map(|c| c.key).collect();
    assert_eq!(got, [Key::Person(7), Key::Door(2), Key::Item(1), Key::Person(5)]);
}

#[test]
fn the_nearest_is_the_target_until_tab_locks_another() {
    let near = [c(Key::Person(7), 3.0), c(Key::Door(2), 12.0), c(Key::Person(5), 30.0)];
    let mut t = Targets::default();
    let none = |_| None;
    assert_eq!(t.current(1, &near, none), Some(Key::Person(7)));
    // Tab from the nearest: the next in the cycle; again; round to the first.
    assert_eq!(t.cycle(1, &near, true, none), Some(Key::Door(2)));
    assert_eq!(t.cycle(1, &near, true, none), Some(Key::Person(5)));
    assert_eq!(t.cycle(1, &near, true, none), Some(Key::Person(7)));
    // Shift+Tab goes back.
    assert_eq!(t.cycle(1, &near, false, none), Some(Key::Person(5)));
    // The lock holds while it is a candidate, whatever is nearer.
    assert_eq!(t.current(1, &near, none), Some(Key::Person(5)));
}

#[test]
fn a_locked_person_is_kept_out_to_fifty_and_the_lock_is_the_selected_ones() {
    let near = [c(Key::Person(7), 3.0), c(Key::Person(5), 30.0)];
    let mut t = Targets::default();
    assert_eq!(t.cycle(1, &near, true, |_| None), Some(Key::Person(5)));
    // He walks to 45: no longer a candidate (35), still the target (50).
    let moved = [c(Key::Person(7), 3.0)];
    assert_eq!(t.current(1, &moved, |k| (k == Key::Person(5)).then_some(45.0)), Some(Key::Person(5)));
    // At 50 he is let go, and the nearest is the target again.
    assert_eq!(t.current(1, &moved, |k| (k == Key::Person(5)).then_some(CHARACTER_RELEASE_RANGE)), Some(Key::Person(7)));
    // A lock made for one selected character is dropped for another.
    assert_eq!(t.cycle(1, &near, true, |_| None), Some(Key::Person(5)));
    assert_eq!(t.current(2, &near, |_| None), Some(Key::Person(7)));
}

#[test]
fn the_cycle_keeps_its_order_and_adds_newcomers_at_the_end() {
    let mut t = Targets::default();
    let first = [c(Key::Person(1), 5.0), c(Key::Person(2), 10.0)];
    assert_eq!(t.cycle(9, &first, true, |_| None), Some(Key::Person(2)));
    // Person 3 comes nearest of all; the cycle is 1, 2, 3 — he is added at
    // its end, not by distance — so from 2 it goes to 3, then round to 1.
    let later = [c(Key::Person(3), 1.0), c(Key::Person(1), 5.0), c(Key::Person(2), 10.0)];
    assert_eq!(t.cycle(9, &later, true, |_| None), Some(Key::Person(3)));
    assert_eq!(t.cycle(9, &later, true, |_| None), Some(Key::Person(1)));
    // Nothing near: Tab does nothing.
    assert_eq!(t.cycle(9, &[], true, |_| None), None);
}
