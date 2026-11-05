//! Movement and the world/location transitions, with no window anywhere.
//!
//! The maps here are built by hand, so none of this needs game data and all of
//! it can run in CI.

use burngfx::game::{self, GameState, LocationData, Place, Point, WALK_SPEED};
use burngfx::map::{self, Map};

/// A .NET `BinaryWriter` string: 7-bit-encoded byte length, then the bytes.
fn dotnet_string(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut len = s.len();
    while len >= 0x80 {
        out.push((len as u8) | 0x80);
        len >>= 7;
    }
    out.push(len as u8);
    out.extend_from_slice(s.as_bytes());
    out
}

/// A map of `w` x `h` tiles with the given entrance rectangles.
fn make_map(w: i32, h: i32, entrances: &[[i32; 4]]) -> Map {
    let mut bytes = Vec::new();
    bytes.extend(dotnet_string("Burntime Map"));
    bytes.extend(dotnet_string("0.2"));
    for v in [w, h, 32, 1] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend(dotnet_string("classic"));
    for _ in 0..w * h {
        bytes.extend_from_slice(&[1, 1, 0]);
    }
    bytes.push(entrances.len() as u8);
    for e in entrances {
        for v in e {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    map::parse(&bytes).expect("hand-built map should parse")
}

#[test]
fn entering_remembers_where_we_came_from_and_leaving_restores_it() {
    let mut state = GameState::new(Point::new(100, 80));
    assert_eq!(state.place, Place::World);
    assert!(!state.is_inside());

    state.enter(4, Point::new(240, 160));
    assert_eq!(state.place, Place::Location(4));
    assert_eq!(state.position, Point::new(240, 160));
    assert_eq!(state.return_position, Some(Point::new(100, 80)));
    assert!(state.is_inside());

    // Walking around inside must not disturb where we came from.
    state.position = Point::new(300, 200);
    assert_eq!(state.return_position, Some(Point::new(100, 80)));

    state.leave();
    assert_eq!(state.place, Place::World);
    assert_eq!(state.position, Point::new(100, 80), "back where we left the desert");
    assert_eq!(state.return_position, None);
}

/// Leaving the world is not a thing; it must not clear the position.
#[test]
fn leaving_the_world_does_nothing() {
    let mut state = GameState::new(Point::new(10, 20));
    state.leave();
    assert_eq!(state.place, Place::World);
    assert_eq!(state.position, Point::new(10, 20));
}

/// The return position is the *world* position, not the previous location's.
#[test]
fn a_second_location_does_not_overwrite_the_world_position() {
    let mut state = GameState::new(Point::new(50, 50));
    state.enter(1, Point::new(200, 200));
    state.position = Point::new(210, 210);
    state.enter(2, Point::new(300, 300));
    assert_eq!(state.return_position, Some(Point::new(50, 50)));

    state.leave();
    assert_eq!(state.position, Point::new(50, 50));
}

#[test]
fn day_does_not_move_on_its_own() {
    let mut state = GameState::new(Point::default());
    state.enter(0, Point::new(1, 1));
    state.leave();
    assert_eq!(state.day, 0, "time has no rules yet, so it must not invent any");
}

#[test]
fn walking_approaches_the_target_and_stops_on_it() {
    let bounds = (480, 320);
    let target = Point::new(100, 100);
    let mut at = Point::new(0, 0);

    let mut steps = 0;
    while at != target {
        let next = game::step_towards(at, target, WALK_SPEED, bounds);
        assert_ne!(next, at, "a walk in progress must make progress");
        assert!(
            next.distance_squared(target) < at.distance_squared(target),
            "each step gets closer"
        );
        at = next;
        steps += 1;
        assert!(steps < 1000, "should have arrived long ago");
    }
    assert_eq!(at, target, "arrives exactly, without overshooting");
}

#[test]
fn walking_holds_a_straight_line_and_never_leaves_the_map() {
    let bounds = (480, 320);
    // A 2:1 target, so the short axis must move at half the rate.
    let after = game::step_towards(Point::new(0, 0), Point::new(400, 200), 10, bounds);
    assert_eq!(after, Point::new(10, 5));

    // Already there: no movement, no jitter.
    let still = Point::new(7, 9);
    assert_eq!(game::step_towards(still, still, WALK_SPEED, bounds), still);

    // A target off the map is clamped rather than followed.
    let out = game::step_towards(Point::new(470, 310), Point::new(9999, 9999), 50, bounds);
    assert_eq!(out, Point::new(479, 319));
}

#[test]
fn a_doorway_opens_only_when_you_are_standing_in_it() {
    let map = make_map(10, 8, &[[100, 100, 20, 16], [200, 40, 10, 10]]);

    assert_eq!(game::entrance_under(&map, Point::new(110, 108)), Some(0));
    assert_eq!(game::entrance_under(&map, Point::new(100, 100)), Some(0), "top left corner");
    assert_eq!(game::entrance_under(&map, Point::new(119, 115)), Some(0), "bottom right");
    assert_eq!(game::entrance_under(&map, Point::new(205, 45)), Some(1));

    // One pixel outside is outside.
    assert_eq!(game::entrance_under(&map, Point::new(120, 108)), None);
    assert_eq!(game::entrance_under(&map, Point::new(110, 116)), None);
    // Clicking a door from across the map must not open it.
    assert_eq!(game::entrance_under(&map, Point::new(10, 10)), None);
}

#[test]
fn a_missing_entry_point_falls_back_without_becoming_data() {
    let map = make_map(15, 10, &[]);
    assert_eq!(map.pixel_size(), (480, 320));

    // No GAMDAT: the centre stands in, and the absence is still visible.
    let unknown = LocationData::default();
    assert_eq!(unknown.entry_point, None);
    assert_eq!(game::spawn_point(&unknown, &map), Point::new(240, 160));

    // A map that does declare one is used as declared.
    let known = LocationData { entry_point: Some(Point::new(252, 229)) };
    assert_eq!(game::spawn_point(&known, &map), Point::new(252, 229));
}

/// The whole slice, with no window: desert, Nob Hill, a door, and back.
#[test]
fn the_round_trip_through_a_location() {
    let world = make_map(15, 12, &[[200, 150, 16, 12]]);
    let location = make_map(15, 10, &[[300, 200, 20, 20]]);
    let data = LocationData::default(); // no GAMDAT entry point

    let mut state = GameState::new(Point::new(64, 64));

    // Standing on the location's rectangle on the world map takes us in.
    state.position = Point::new(205, 155);
    let index = game::entrance_under(&world, state.position).expect("on the location");
    state.enter(index, game::spawn_point(&data, &location));
    assert_eq!(state.place, Place::Location(0));
    assert_eq!(state.position, Point::new(240, 160), "centre, for want of an entry point");

    // Walk to the door. Nothing opens until we are actually in it.
    let door = Point::new(310, 210);
    let bounds = location.pixel_size();
    let mut guard = 0;
    while game::entrance_under(&location, state.position).is_none() {
        state.position = game::step_towards(state.position, door, WALK_SPEED, bounds);
        guard += 1;
        assert!(guard < 1000, "never reached the door");
    }
    assert_eq!(game::entrance_under(&location, state.position), Some(0));

    // Back to the desert, exactly where we left it.
    state.leave();
    assert_eq!(state.place, Place::World);
    assert_eq!(state.position, Point::new(205, 155));
    assert_eq!(state.day, 0);
}

// -- water ------------------------------------------------------------------

use burngfx::game::{CharacterNeeds, Springs, WaterSource};

/// A character as the game actually starts one drinks nothing, because
/// StartWater and MaxWater are both 5. That is why the demonstration begins
/// parched rather than at the game's own opening state.
#[test]
fn a_freshly_started_character_is_not_thirsty() {
    let mut spring = WaterSource::new(5, 6);
    let mut who = CharacterNeeds::at_game_start();
    assert_eq!(who.water, 5);
    assert_eq!(spring.drink(&mut who), 0);
    assert_eq!(spring.reserve, 5, "nothing taken from the spring");
    assert_eq!(CharacterNeeds::default(), CharacterNeeds::at_game_start());
}

/// Nob Hill's numbers, straight out of MAT_005.txt.
fn nob_hill_spring() -> WaterSource {
    WaterSource::new(5, 6)
}

#[test]
fn a_spring_starts_with_one_turns_yield() {
    let spring = nob_hill_spring();
    assert_eq!(spring.refresh, 5);
    assert_eq!(spring.reserve, 5, "LocationCreator sets Reserve = Water");
    assert_eq!(spring.capacity, 6);
}

#[test]
fn drinking_fills_the_character_and_empties_the_spring_by_the_same_amount() {
    let mut spring = nob_hill_spring();
    let mut who = CharacterNeeds::parched_for_demonstration();
    assert_eq!((who.water, who.max_water), (0, 5));

    assert_eq!(spring.drink(&mut who), 5);
    assert_eq!(who.water, 5, "drank to the brim");
    assert_eq!(spring.reserve, 0, "and the spring is dry");
}

#[test]
fn you_cannot_drink_more_than_you_can_hold() {
    let mut spring = WaterSource::new(20, 20);
    let mut who = CharacterNeeds::parched_for_demonstration();

    assert_eq!(spring.drink(&mut who), 5);
    assert_eq!(who.water, 5);
    assert_eq!(spring.reserve, 15, "the rest stays in the spring");

    // Drinking again when full takes nothing.
    assert_eq!(spring.drink(&mut who), 0);
    assert_eq!(who.water, 5);
    assert_eq!(spring.reserve, 15);
}

#[test]
fn a_dry_spring_gives_what_it_has_and_no_more() {
    let mut spring = WaterSource { refresh: 5, reserve: 2, capacity: 6 };
    let mut who = CharacterNeeds::parched_for_demonstration();

    assert_eq!(spring.drink(&mut who), 2);
    assert_eq!(who.water, 2, "thirsty, but that was all of it");
    assert_eq!(spring.reserve, 0);

    // An empty spring is not an error.
    assert_eq!(spring.drink(&mut who), 0);
    assert_eq!(who.water, 2);
    assert_eq!(spring.reserve, 0);
}

/// The cap is applied at the end of a turn, not when the spring fills, so it
/// can briefly hold more than its capacity. That is what the original does.
#[test]
fn a_turn_refills_the_spring_and_the_cap_applies_at_the_end() {
    let mut spring = nob_hill_spring();
    let mut who = CharacterNeeds::parched_for_demonstration();

    spring.drink(&mut who);
    assert_eq!(spring.reserve, 0);

    spring.begin_turn(burngfx::game::Engine::V091, burngfx::game::Pump::None);
    assert_eq!(spring.reserve, 5);
    spring.end_turn();
    assert_eq!(spring.reserve, 5, "still under capacity");

    // A full spring overflows during the turn and is trimmed at the end.
    spring.begin_turn(burngfx::game::Engine::V091, burngfx::game::Pump::None);
    assert_eq!(spring.reserve, 10, "briefly over capacity, as in the original");
    spring.end_turn();
    assert_eq!(spring.reserve, 6, "trimmed to capacity");
}

/// The whole point of the mechanic: after walking into the shack the world is
/// not what it was.
#[test]
fn entering_the_shack_changes_the_world() {
    let mut state = GameState::new(Point::new(50, 50));
    state.needs = CharacterNeeds::parched_for_demonstration();
    let mut spring = nob_hill_spring();

    let before = (state.needs.water, spring.reserve);
    assert_eq!(before, (0, 5));

    state.enter(4, Point::new(240, 160));
    let drunk = spring.drink(&mut state.needs);

    assert_eq!(drunk, 5);
    assert_eq!((state.needs.water, spring.reserve), (5, 0));

    // Leaving does not put the water back.
    state.leave();
    assert_eq!(state.needs.water, 5);
    assert_eq!(spring.reserve, 0);
}

// -- collision ---------------------------------------------------------------

/// A wall down the middle of the map, from x = 100 to x = 120.
fn wall(p: Point) -> bool {
    !(100..120).contains(&p.x)
}

#[test]
fn walking_stops_at_a_wall_instead_of_through_it() {
    let bounds = (480, 320);
    let mut at = Point::new(50, 50);
    let target = Point::new(300, 50);

    for _ in 0..1000 {
        let next = game::step_towards_passable(at, target, WALK_SPEED, bounds, wall);
        if next == at {
            break;
        }
        assert!(wall(next), "stepped into the wall at {next:?}");
        at = next;
    }
    assert!(at.x < 100, "should have stopped before the wall, stopped at {at:?}");
    assert!(at.x > 90, "should have got most of the way there, stopped at {at:?}");
}

/// A character walking diagonally into a wall should slide along it rather
/// than stick, or the first obstacle would end every journey.
#[test]
fn walking_slides_along_a_wall_it_cannot_pass() {
    let bounds = (480, 320);
    // Right up against the wall, heading down and to the right.
    let at = Point::new(99, 50);
    let next = game::step_towards_passable(at, Point::new(300, 260), WALK_SPEED, bounds, wall);

    assert_eq!(next.x, at.x, "the blocked axis does not move");
    assert!(next.y > at.y, "the free axis still does");
}

/// Standing somewhere legal with every neighbour blocked means standing still.
/// (Standing somewhere *illegal* is the other case, below, and moves.)
#[test]
fn an_impossible_step_leaves_the_character_where_they_are() {
    let bounds = (480, 320);
    let at = Point::new(50, 50);
    let only_here = move |p: Point| p == at;

    assert_eq!(
        game::step_towards_passable(at, Point::new(300, 300), WALK_SPEED, bounds, only_here),
        at
    );

    // Standing on the target is not a blocked step, it is no step.
    assert_eq!(game::step_towards_passable(at, at, WALK_SPEED, bounds, only_here), at);
}

/// A character already standing somewhere blocked must be able to leave.
/// Every step out of a wall thicker than one step is still inside the wall, so
/// without an escape hatch a data change would strand them permanently.
#[test]
fn a_character_stuck_inside_a_wall_can_still_leave() {
    let bounds = (480, 320);
    let at = Point::new(110, 50); // inside the wall
    let out = game::step_towards_passable(at, Point::new(10, 50), WALK_SPEED, bounds, wall);
    assert!(out.x < at.x, "should be able to walk out, went to {out:?}");
}

// -- reaching a door ---------------------------------------------------------

/// A doorway tile belongs to the building and is not walkable, so "standing in
/// the rectangle" can never happen. The original uses a distance instead.
#[test]
fn a_door_opens_from_close_by_not_only_from_inside() {
    let map = make_map(15, 10, &[[100, 100, 10, 10]]);
    assert_eq!(game::ENTRY_REACH, 15);

    // Inside is still inside.
    assert_eq!(game::entrance_in_reach(&map, Point::new(105, 105), 15), Some(0));
    assert_eq!(game::entrance_under(&map, Point::new(105, 105)), Some(0));

    // Just outside the rectangle, well within reach.
    assert_eq!(game::entrance_in_reach(&map, Point::new(95, 105), 15), Some(0));
    assert_eq!(game::entrance_under(&map, Point::new(95, 105)), None, "not inside");

    // Exactly at the limit, and one pixel past it.
    assert_eq!(game::entrance_in_reach(&map, Point::new(85, 105), 15), Some(0));
    assert_eq!(game::entrance_in_reach(&map, Point::new(84, 105), 15), None);

    // Diagonally the corner distance is what counts, not the axes separately:
    // 10 and 10 away is 14.1, inside the reach; 11 and 11 is 15.6, outside it.
    assert_eq!(game::entrance_in_reach(&map, Point::new(90, 90), 15), Some(0));
    assert_eq!(game::entrance_in_reach(&map, Point::new(89, 89), 15), None);

    // From across the map, nothing.
    assert_eq!(game::entrance_in_reach(&map, Point::new(10, 10), 15), None);
}

/// Standing between two doors picks the nearer one rather than the first.
#[test]
fn the_nearest_door_wins() {
    let map = make_map(15, 10, &[[100, 100, 10, 10], [130, 100, 10, 10]]);
    assert_eq!(game::entrance_in_reach(&map, Point::new(115, 105), 15), Some(0), "5 vs 15");
    assert_eq!(game::entrance_in_reach(&map, Point::new(125, 105), 15), Some(1), "16 vs 5");
}

/// The stuck-recovery step prefers a way out, and is only a last resort.
#[test]
fn a_stuck_character_takes_a_way_out_when_there_is_one() {
    let bounds = (480, 320);
    // Blocked everywhere except a corridor along y = 40.
    let corridor = |p: Point| p.y == 40;

    // Standing in the wall at (100, 50), heading for the corridor.
    let at = Point::new(100, 50);
    let out = game::step_towards_passable(at, Point::new(100, 40), WALK_SPEED, bounds, corridor);
    assert!(out.y < at.y, "should move towards the corridor, went to {out:?}");

    // With no way out at all it still moves, because standing in a wall
    // forever is worse — but that is an attempt, not a guarantee of escape.
    let nothing = |_: Point| false;
    let stuck = Point::new(100, 50);
    let anyway = game::step_towards_passable(stuck, Point::new(300, 50), WALK_SPEED, bounds, nothing);
    assert_ne!(anyway, stuck, "a stuck character is not frozen");
}

// -- the door you asked for, and springs that outlast a visit ----------------

/// Two doors within reach: the one that was chosen opens, not the nearer one.
#[test]
fn the_door_you_asked_for_opens_even_when_another_is_closer() {
    // Door 0 spans x 100..109, door 1 spans 130..139. At x = 121 the gaps are
    // 12 to door 0 and 9 to door 1: both within reach, and door 1 is nearer.
    let map = make_map(15, 10, &[[100, 100, 10, 10], [130, 100, 10, 10]]);
    let between = Point::new(121, 105);

    // Both are in reach, and door 1 is the nearer of the two.
    assert!(game::entrance_reached(&map, 0, between, game::ENTRY_REACH));
    assert!(game::entrance_reached(&map, 1, between, game::ENTRY_REACH));
    assert_eq!(game::entrance_in_reach(&map, between, game::ENTRY_REACH), Some(1), "nearest");

    // Asking about door 0 must still say yes. Comparing against the nearest
    // would have said no, and the door the player chose would not open.
    assert!(
        game::entrance_reached(&map, 0, between, game::ENTRY_REACH),
        "the chosen door must not lose to a closer one"
    );

    // Out of range is still out of range, whichever door is nearest.
    let far = Point::new(60, 105);
    assert!(!game::entrance_reached(&map, 0, far, game::ENTRY_REACH));
    assert!(!game::entrance_reached(&map, 1, far, game::ENTRY_REACH));

    // A door that does not exist is never reached.
    assert!(!game::entrance_reached(&map, 9, between, game::ENTRY_REACH));
}

/// Leaving a location and coming back must not refill its spring.
#[test]
fn a_spring_outlasts_the_visit_that_emptied_it() {
    let mut springs = Springs::new();
    assert!(springs.is_empty());

    // First visit to Nob Hill: drink it dry.
    let mut who = CharacterNeeds::parched_for_demonstration();
    let drunk = springs.get_or_insert(4, || WaterSource::new(5, 6)).drink(&mut who);
    assert_eq!(drunk, 5);
    assert_eq!(springs.get(4).unwrap().reserve, 0);
    assert_eq!(springs.len(), 1);

    // Another location keeps its own water.
    let elsewhere = springs.get_or_insert(9, || WaterSource::new(8, 8));
    assert_eq!(elsewhere.reserve, 8);
    assert_eq!(springs.len(), 2);

    // Walk out to the desert and back: the maker must not run again.
    let again = springs.get_or_insert(4, || panic!("the spring was rebuilt on return"));
    assert_eq!(again.reserve, 0, "still dry");

    // And there is nothing left to drink.
    let mut thirsty = CharacterNeeds::parched_for_demonstration();
    assert_eq!(springs.get_or_insert(4, || WaterSource::new(5, 6)).drink(&mut thirsty), 0);
    assert_eq!(thirsty.water, 0);
    assert_eq!(springs.len(), 2, "no location was added twice");
}

// -- modes and releases ------------------------------------------------------

use burngfx::game::{Engine, Mode, Pump};

/// The two axes, and the place where they cross.
///
/// A *mode* is the remaster's own choice of rules; a *release* is which
/// version of the remaster we are reproducing. They are not the same question
/// and they do not select the same files:
///
/// | | v0.9.1 | current |
/// |---|---|---|
/// | items | by mode | by mode |
/// | settings | one file, both modes | by mode |
///
/// v0.9.1 calls `new GameSettings("gamesettings.txt")` with nothing said
/// about the mode. Current upstream picks between two files. So "the original
/// settings" names nothing until the release is named too — and this project
/// spent a while reading v0.9.1's code while feeding it current upstream's
/// numbers, which is exactly what that ambiguity buys you.
#[test]
fn a_mode_and_a_release_choose_different_things() {
    assert_eq!(Mode::default(), Mode::Original, "the 1993 game is the point");
    assert_eq!(Engine::default(), Engine::V091, "and v0.9.1 is what it is read from");
    let state = GameState::new(Point::default());
    assert_eq!((state.mode, state.engine), (Mode::Original, Engine::V091));

    // The item table is the mode's, and the same in both releases.
    assert_eq!(Mode::Original.items_file(), "items_original.txt");
    // The odd one out: the extended list is the file with no suffix.
    assert_eq!(Mode::Extended.items_file(), "items.txt");

    // The settings file is the release's, and only current upstream lets the
    // mode into it.
    assert_eq!(Engine::V091.settings_file(Mode::Original), "gamesettings.txt");
    assert_eq!(
        Engine::V091.settings_file(Mode::Extended),
        "gamesettings.txt",
        "v0.9.1 has one settings file and does not ask which mode"
    );
    assert_eq!(Engine::Current.settings_file(Mode::Original), "gamesettings_original.txt");
    assert_eq!(Engine::Current.settings_file(Mode::Extended), "gamesettings_extended.txt");

    assert_eq!((Mode::Original.name(), Mode::Extended.name()), ("original", "extended"));
    assert_eq!((Engine::V091.name(), Engine::Current.name()), ("v091", "current"));
}

/// Two upstream *releases* compute the pump boost differently. This is not a
/// difference between game modes: `WaterSource.GetBoost` never consults the
/// mode, so both formulas below are what their release does in every mode.
#[test]
fn the_two_releases_compute_the_pump_boost_differently() {
    use Engine::{Current, V091 as UpTo091};

    // No pump, no argument.
    for engine in [UpTo091, Current] {
        assert_eq!(game::pump_boost(engine, 8, Pump::None), 0);
    }

    // A yield of 1 is the special case both share.
    assert_eq!(game::pump_boost(UpTo091, 1, Pump::Industrial), 5);
    assert_eq!(game::pump_boost(Current, 1, Pump::Industrial), 5);

    // Between 2 and 9 they part company: v0.9.1 halves, current floors.
    assert_eq!(game::pump_boost(UpTo091, 6, Pump::Industrial), 3);
    assert_eq!(game::pump_boost(Current, 6, Pump::Industrial), 5);
    assert_eq!(game::pump_boost(UpTo091, 6, Pump::Hand), 1);
    assert_eq!(game::pump_boost(Current, 6, Pump::Hand), 2);

    // Above that the floor stops mattering and they agree again.
    assert_eq!(game::pump_boost(UpTo091, 20, Pump::Industrial), 10);
    assert_eq!(game::pump_boost(Current, 20, Pump::Industrial), 10);
}

#[test]
fn a_half_fired_gun_keeps_its_rounds_when_it_moves() {
    use burngfx::game::{move_item, ItemBag};
    let mut a = ItemBag::with(6, &[3, 5]);
    a.set_rounds_left(1, Some(2));
    let mut b = ItemBag::new(6);
    move_item(&mut a, 1, &mut b).unwrap();
    assert_eq!(b.rounds_left(0), Some(2));
    move_item(&mut b, 0, &mut a).unwrap();
    assert_eq!(a.rounds_left(1), Some(2));
}

#[test]
fn what_is_set_aside_goes_back_to_its_owner_or_down_never_elsewhere() {
    use burngfx::game::{return_set_aside, ItemBag};
    let mut leader = ItemBag::with(6, &[1, 2, 3, 4, 5, 6]); // full: bought into
    let mut mate = ItemBag::with(6, &[7]);
    let mut aside = ItemBag::with(6, &[9, 8]);
    aside.set_rounds_left(0, Some(1));
    let mut from = vec![0, 1];
    let left = {
        let mut packs: Vec<&mut ItemBag> = vec![&mut leader, &mut mate];
        return_set_aside(&mut aside, &mut from, &mut packs)
    };
    // The leader's rifle cannot go back and does not go to the mate.
    assert_eq!(left, vec![(9, Some(1))]);
    assert_eq!(mate.iter().collect::<Vec<_>>(), vec![7, 8]);
    assert_eq!(leader.len(), 6);
    assert!(aside.is_empty());
}

/// Set aside from the second man's page while the leader's page is in front:
/// it goes back to the second man, not to whoever is in front.
#[test]
fn set_aside_from_another_page_goes_back_to_that_man() {
    use burngfx::game::{return_set_aside, ItemBag};
    let mut leader = ItemBag::with(6, &[1]);
    let mut mate = ItemBag::with(6, &[]);
    let mut aside = ItemBag::with(6, &[9]);
    let mut from = vec![1];
    let left = {
        let mut packs: Vec<&mut ItemBag> = vec![&mut leader, &mut mate];
        return_set_aside(&mut aside, &mut from, &mut packs)
    };
    assert!(left.is_empty());
    assert_eq!(mate.iter().collect::<Vec<_>>(), vec![9]);
    assert_eq!(leader.len(), 1);
}

/// Where the group stands on entering a place. By road (`Player.Travel`,
/// `EnterLocation`) every member is put at the entry; by `/petko`'s Travel
/// (`MapScene.TravelToLocation` with `_debugNoTravel`, MapScene.cs:
/// 1195-1207) only the leader is (`player.Character.Position =
/// destination.EntryPoint`): the companions keep the coordinates they had
/// in the place before.
#[test]
fn petko_travel_moves_only_the_leader() {
    let mut party = game::Party::default();
    party.recruit();
    party.recruit();
    party.companions[0].at = Point::new(63, 147);
    party.companions[1].at = Point::new(148, 148);
    let entry = Point::new(200, 120);
    game::arrive(&mut party, entry, true, |_| false);
    assert_eq!(party.companions[0].at, Point::new(63, 147));
    assert_eq!(party.companions[1].at, Point::new(148, 148));
    game::arrive(&mut party, entry, false, |_| false);
    assert!(party.companions.iter().all(|c| c.at == entry && c.move_to == entry));
}
