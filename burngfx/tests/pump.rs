//! An installed pump, and what it does to a spring's day.
//!
//! `WaterSource.Water`'s getter is `water + GetBoost()`, and `BeginTurn` is
//! `Reserve += Water`. So the boost is not an extra step taken once a day: it
//! is part of every reading of the spring, and the reserve is where it lands.
//!
//! **This is the one rule of the three in this slice that the two tags do not
//! agree on.** v0.9.1 special-cases a base yield of exactly 1; `v1.1` makes
//! the same number a floor. The difference is not cosmetic: at v0.9.1 a hand
//! pump on a spring yielding 2 or 3 adds nothing whatever, and an industrial
//! pump gives a spring yielding 1 five times what it gives one yielding 2.

use burngfx::game::{pump_boost, Engine, Pump, Springs, WaterSource};

#[test]
fn an_industrial_pump_beats_a_hand_pump_and_they_do_not_add_up() {
    // `else if`: the industrial one wins outright wherever both are found.
    assert_eq!(pump_boost(Engine::V091, 8, Pump::Industrial), 4);
    assert_eq!(pump_boost(Engine::V091, 8, Pump::Hand), 2);
    assert_eq!(pump_boost(Engine::V091, 8, Pump::None), 0);
}

/// v0.9.1's cliff, kept because it is what the release does.
#[test]
fn at_v091_a_hand_pump_on_a_middling_spring_is_useless() {
    for water in [2, 3] {
        assert_eq!(pump_boost(Engine::V091, water, Pump::Hand), 0, "water {water}");
    }
    assert_eq!(pump_boost(Engine::V091, 1, Pump::Hand), 2);

    // And the same cliff the other way round for the industrial pump: a
    // spring yielding 1 gets five, one yielding 2 gets one.
    assert_eq!(pump_boost(Engine::V091, 1, Pump::Industrial), 5);
    assert_eq!(pump_boost(Engine::V091, 2, Pump::Industrial), 1);
}

#[test]
fn v11_makes_the_same_numbers_a_floor_instead() {
    for water in [1, 2, 3, 4] {
        assert_eq!(pump_boost(Engine::Current, water, Pump::Hand), 2, "water {water}");
        assert_eq!(pump_boost(Engine::Current, water, Pump::Industrial), 5, "water {water}");
    }
    // Above the floor the two agree again.
    assert_eq!(pump_boost(Engine::Current, 12, Pump::Industrial), 6);
    assert_eq!(pump_boost(Engine::V091, 12, Pump::Industrial), 6);
}

/// Nob Hill: 5 a turn, holding at most 6. The cap still lands at the end of
/// the turn, so a pumped spring overflows and is trimmed exactly as before.
#[test]
fn the_boost_reaches_the_reserve_and_the_cap_still_applies() {
    let mut spring = WaterSource::classic(0, 5, 6);
    assert_eq!(spring.yield_with(Engine::V091, Pump::None), 5);
    assert_eq!(spring.yield_with(Engine::V091, Pump::Industrial), 7);

    spring.begin_turn(Engine::V091, Pump::Industrial);
    assert_eq!(spring.reserve, 7, "briefly over capacity, as without a pump");
    spring.end_turn();
    assert_eq!(spring.reserve, 6);
}

/// A pump is installed in a place, so the world asks per location.
#[test]
fn the_world_asks_each_location_for_its_own_pump() {
    let mut springs = Springs::new();
    springs.set(4, WaterSource::classic(0, 4, 40));
    springs.set(9, WaterSource::classic(0, 4, 40));

    springs.advance_day(Engine::V091, |location| {
        if location == 4 {
            Pump::Industrial
        } else {
            Pump::None
        }
    });

    assert_eq!(springs.at(4).unwrap().reserve, 6, "4 a turn and half again");
    assert_eq!(springs.at(9).unwrap().reserve, 4, "no pump, no change");
}
