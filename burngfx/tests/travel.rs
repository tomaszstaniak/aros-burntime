//! Travelling between locations, and what it costs.
//!
//! The cost comes from the 1993 save's road graph — four neighbours a
//! location, each with its own number of days — and not from the distance
//! between them on the map, the number of steps walked, or the act of opening
//! a location. Those are three different numbers and only one of them is the
//! game's.

use burngfx::game::{
    advance_day, depart, road_days, travel_a_day, CharacterNeeds, ItemBag, Journey, Springs,
    TravelError, MAX_FOOD, PACK_CAPACITY, START_HEALTH,
};
use burngfx::gamdat::{self, Gamdat};
use std::path::PathBuf;

/// A world with no camps and nothing to produce: these tests are about the
/// character's own day, not the locations'.
struct Nowhere;

impl burngfx::game::Storage for Nowhere {
    fn count_in_rooms(&self, _: usize, _: usize) -> usize {
        0
    }
    fn tools(&self, _: usize, _: usize) -> usize {
        0
    }
    fn any_room_has_space(&self, _: usize) -> bool {
        false
    }
    fn store(&mut self, _: usize, _: usize) -> bool {
        false
    }

    /// No pump in these tests; the spring's own rate is the subject.
    fn pump_at(&self, _: usize) -> burngfx::game::Pump {
        burngfx::game::Pump::None
    }
}

/// A `WorldDay` with nothing in it but the springs.
fn world<'a>(
    springs: &'a mut burngfx::game::Springs,
    camps: &'a mut burngfx::game::Camps,
) -> burngfx::game::WorldDay<'a, Nowhere> {
    // Leaked once per call and never mutated: a test binary that needs an
    // empty production table more than once is not worth threading a
    // lifetime through.
    static NONE: std::sync::OnceLock<burngfx::production::Productions> =
        std::sync::OnceLock::new();
    burngfx::game::WorldDay {
        springs,
        camps,
        storage: Box::leak(Box::new(Nowhere)),
        productions: NONE.get_or_init(burngfx::production::Productions::default),
        engine: burngfx::game::Engine::V091,
        deaths: Vec::new(),
            respawns: Box::leak(Box::new(burngfx::game::Respawns::new())),
            returned: Vec::new(),
    }
}


struct Barren;

impl burngfx::game::ItemFacts for Barren {
    fn food(&self, _: usize) -> i32 {
        0
    }
    fn water(&self, _: usize) -> i32 {
        0
    }
    fn full(&self, _: usize) -> Option<usize> {
        None
    }
    fn empty(&self, _: usize) -> Option<usize> {
        None
    }
    fn kind_named(&self, _: &str) -> Option<usize> {
        None
    }
    fn name_of(&self, _: usize) -> Option<&str> {
        None
    }
}

fn save() -> Option<Gamdat> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/burntime.pak");
    let bytes = std::fs::read(&path).ok()?;
    let pak = burngfx::pak::ClassicPak::open(&bytes).ok()?;
    gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()
}

fn fed() -> CharacterNeeds {
    CharacterNeeds {
        food: MAX_FOOD,
        max_food: MAX_FOOD,
        water: 5,
        max_water: 5,
        health: START_HEALTH,
    }
}

// -- the graph --------------------------------------------------------------

/// The four players a new game starts with, and where they stand.
#[test]
fn the_save_says_where_the_players_begin() {
    let Some(gam) = save() else { return };

    assert_eq!(gam.players.len(), gamdat::PLAYER_COUNT);
    for p in &gam.players {
        assert!(p.location < gamdat::LOCATION_COUNT, "location {} is off the map", p.location);
        assert_eq!(p.travel_days, 0, "a new game has nobody on the road");
        assert_eq!(p.travel_to, None);
    }

    // Four different towns, and four flags four frames apart — the sprite ids
    // `GameCreation` reads as `syst.raw?FlagId` to `FlagId + 3`. Neither is
    // load-bearing on its own; together they say the record is being read at
    // the right offset rather than out of the middle of something else.
    let mut where_they_are: Vec<usize> = gam.players.iter().map(|p| p.location).collect();
    where_they_are.sort_unstable();
    where_they_are.dedup();
    assert_eq!(where_they_are.len(), gamdat::PLAYER_COUNT, "two players in one town");

    let mut flags: Vec<u16> = gam.players.iter().map(|p| p.flag_id).collect();
    flags.sort_unstable();
    assert_eq!(flags, [0, 4, 8, 12]);
}

/// Roads are two-way and cost the same in both directions, and everywhere is
/// reachable from everywhere. A graph that failed either would make travel
/// mean something else entirely.
#[test]
fn the_road_graph_is_symmetric_and_joined_up() {
    let Some(gam) = save() else { return };

    let mut roads = 0;
    for (from, loc) in gam.locations.iter().enumerate() {
        assert!(loc.roads.len() <= 4, "location {from} has {} roads", loc.roads.len());
        for road in &loc.roads {
            assert!(road.days > 0, "a road from {from} to {} costs nothing", road.neighbour);
            assert_eq!(
                road_days(&gam, road.neighbour, from),
                Some(road.days),
                "the road from {from} to {} is not the same coming back",
                road.neighbour
            );
            roads += 1;
        }
    }
    assert!(roads > 60, "only {roads} roads in the whole world");

    // Every location reachable from where the first player starts.
    let start = gam.players[0].location;
    let mut seen = vec![false; gam.locations.len()];
    let mut queue = alloc_queue(start);
    seen[start] = true;
    while let Some(at) = queue.pop() {
        for road in &gam.locations[at].roads {
            if !seen[road.neighbour] {
                seen[road.neighbour] = true;
                queue.push(road.neighbour);
            }
        }
    }
    let stranded: Vec<usize> = (0..seen.len()).filter(|&i| !seen[i]).collect();
    assert!(stranded.is_empty(), "no road reaches {stranded:?}");
}

fn alloc_queue(start: usize) -> Vec<usize> {
    vec![start]
}

// -- setting out ------------------------------------------------------------

/// You can only go where a road goes, and only one road at a time.
///
/// `GetDirectTravelDays` looks at the current location's own neighbours and
/// nothing further, so a place two roads away is not a destination at all —
/// the player takes the legs one by one. This is not a simplification here;
/// it is what the original does.
#[test]
fn only_a_neighbour_is_a_destination() {
    let Some(gam) = save() else { return };
    let from = gam.players[0].location;
    let neighbour = gam.locations[from].roads[0].neighbour;

    let journey = depart(&gam, None, from, neighbour).unwrap();
    assert_eq!(journey.total, road_days(&gam, from, neighbour).unwrap());
    assert_eq!(journey.remaining, journey.total);
    assert_eq!((journey.from, journey.to), (from, neighbour));

    // Somewhere with no road from here.
    let far = (0..gam.locations.len())
        .find(|&i| i != from && road_days(&gam, from, i).is_none())
        .expect("not everywhere is a neighbour");
    assert_eq!(depart(&gam, None, from, far), Err(TravelError::NoRoad));

    assert_eq!(depart(&gam, None, from, from), Err(TravelError::Arrived));

    // And not while already on a road: the edge is committed.
    assert_eq!(
        depart(&gam, Some(journey), from, neighbour),
        Err(TravelError::AlreadyTravelling)
    );
}

/// A journey ends on the day that finishes it, not on the day after.
#[test]
fn the_last_day_of_the_road_is_the_day_you_arrive() {
    let three_days = Journey { from: 1, to: 2, total: 3, remaining: 3 };

    let after_one = travel_a_day(three_days).unwrap();
    assert_eq!(after_one.remaining, 2);
    let after_two = travel_a_day(after_one).unwrap();
    assert_eq!(after_two.remaining, 1);
    assert_eq!(travel_a_day(after_two), None, "arrived");

    // A one-day road arrives on its first day.
    assert_eq!(travel_a_day(Journey { from: 1, to: 2, total: 1, remaining: 1 }), None);
}

// -- the road costs what a day costs ----------------------------------------

/// The point of the whole thing: days on the road are days, and the stores go
/// down while nobody is anywhere.
#[test]
fn walking_a_road_spends_the_days_it_costs() {
    let Some(gam) = save() else { return };
    let from = gam.players[0].location;
    let (to, cost) = gam.locations[from]
        .roads
        .iter()
        .map(|r| (r.neighbour, r.days))
        .max_by_key(|&(_, d)| d)
        .expect("the first player has a road out");
    assert!(cost >= 2, "the longest road from {from} is only {cost} day(s)");

    let mut day = 1u16;
    let mut springs = Springs::from_gamdat(&gam);
    let mut camps = burngfx::game::Camps::new();
    let mut needs = fed();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let mut journey = Some(depart(&gam, None, from, to).unwrap());

    let (food, water) = (needs.food, needs.water);
    let mut arrived = None;
    for _ in 0..cost {
        let out = advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Barren, &mut journey);
        arrived = arrived.or(out.arrived_at);
    }

    assert_eq!(arrived, Some(to), "arrived on the last day of the road");
    assert_eq!(journey, None, "and is no longer travelling");
    assert_eq!(day, 1 + cost as u16, "one day per day of road");
    assert_eq!(needs.food, food - cost as i32, "a day's food each day");
    assert_eq!(needs.water, water - cost as i32);
}

/// Standing still costs the same as walking, so the road is not free and not
/// double-charged either. The difference between the two is where you end up.
#[test]
fn a_day_on_the_road_costs_what_a_day_at_home_costs() {
    let Some(gam) = save() else { return };
    let from = gam.players[0].location;
    let to = gam.locations[from].roads[0].neighbour;

    let mut walking = fed();
    let mut resting = fed();
    let mut day = 1u16;
    let mut springs = Springs::from_gamdat(&gam);
    let mut camps = burngfx::game::Camps::new();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let mut journey = Some(depart(&gam, None, from, to).unwrap());
    let mut nowhere = None;

    advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut walking, &mut burngfx::game::Party::new(), &mut pack, &Barren, &mut journey);
    advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut resting, &mut burngfx::game::Party::new(), &mut pack, &Barren, &mut nowhere);

    assert_eq!(walking, resting, "the road itself costs nothing extra");
    assert_eq!(nowhere, None, "and standing still starts no journey");
}

/// Springs fill while the player is out on a road, because locations turn
/// whether or not anybody is in them.
#[test]
fn the_world_carries_on_while_you_walk() {
    let Some(gam) = save() else { return };
    let from = gam.players[0].location;
    let to = gam.locations[from].roads[0].neighbour;

    let mut day = 1u16;
    let mut springs = Springs::from_gamdat(&gam);
    let mut camps = burngfx::game::Camps::new();
    let before: Vec<i32> = (0..gam.locations.len())
        .filter_map(|i| springs.get(i).map(|s| s.reserve))
        .collect();

    let mut needs = fed();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let mut journey = Some(depart(&gam, None, from, to).unwrap());
    advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Barren, &mut journey);

    let after: Vec<i32> = (0..gam.locations.len())
        .filter_map(|i| springs.get(i).map(|s| s.reserve))
        .collect();
    assert!(
        after.iter().zip(&before).any(|(a, b)| a > b),
        "no spring in the world gained anything while the player walked"
    );
}

/// Asking whether a journey is possible must not start one.
///
/// The interface shows the road's cost before the player commits, and it gets
/// that number from the same call the departure uses. If asking had a side
/// effect the panel would promise something the rules then refused, or worse,
/// set out on its own.
#[test]
fn asking_about_a_road_is_a_question_and_not_an_act() {
    let Some(gam) = save() else { return };
    let from = gam.players[0].location;
    let to = gam.locations[from].roads[0].neighbour;

    let first = depart(&gam, None, from, to);
    let second = depart(&gam, None, from, to);
    assert_eq!(first, second, "the same question, twice, and the same answer");

    // Asking ten times leaves everything exactly as it was: the journey is
    // whatever the caller stores, and nothing here stores it.
    for _ in 0..10 {
        assert_eq!(depart(&gam, None, from, to), first);
        assert_eq!(depart(&gam, None, from, 999), Err(TravelError::NoRoad));
    }

    // And the answer it gives is the number the graph holds, so a panel
    // drawing `total` is drawing the cost that will be charged.
    assert_eq!(first.unwrap().total, road_days(&gam, from, to).unwrap());
}
