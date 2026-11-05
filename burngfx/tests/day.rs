//! A day passing: what it costs, what it refills, and in what order.
//!
//! The order is the substance here. `Character.Turn` eats before it counts
//! deprivation, and counts deprivation before it takes the day's ration — so
//! the day you run out is the day it hurts, and eating in time prevents it.
//! Move either step and the numbers still go down; they go down wrongly.

use burngfx::game::{
    advance_day, character_day, find_food, CharacterNeeds, ItemBag, Springs, WaterSource,
    DEPRIVATION, MAX_FOOD, MAX_HEALTH, PACK_CAPACITY, REGEN_FLOOR_WITHOUT_DOCTOR,
    REGEN_WITHOUT_DOCTOR, START_HEALTH,
};
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


const SNAKE: usize = 3;
const MAGGOTS: usize = 1;
const BOTTLE: usize = 5;
const FULL_BOTTLE: usize = 6;

/// Three foods and a bottle, said in four numbers each. The real table
/// answers the same questions for fifty items; this one is small enough to
/// read beside the assertions.
struct Items;

impl burngfx::game::ItemFacts for Items {
    fn food(&self, kind: usize) -> i32 {
        match kind {
            SNAKE => 7,
            MAGGOTS => 3,
            _ => 0,
        }
    }

    fn water(&self, kind: usize) -> i32 {
        match kind {
            FULL_BOTTLE => 2,
            _ => 0,
        }
    }

    fn full(&self, kind: usize) -> Option<usize> {
        (kind == BOTTLE).then_some(FULL_BOTTLE)
    }

    fn empty(&self, kind: usize) -> Option<usize> {
        (kind == FULL_BOTTLE).then_some(BOTTLE)
    }

    fn kind_named(&self, _: &str) -> Option<usize> {
        None
    }
    fn name_of(&self, _: usize) -> Option<&str> {
        None
    }
}

/// A world with nothing edible or drinkable in it.
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

fn fed() -> CharacterNeeds {
    CharacterNeeds {
        food: MAX_FOOD,
        max_food: MAX_FOOD,
        water: 5,
        max_water: 5,
        health: START_HEALTH,
    }
}

// -- the shape of one day ---------------------------------------------------

/// A well-fed character simply spends one of each and loses nothing.
#[test]
fn a_day_costs_one_food_and_one_water() {
    let mut needs = fed();
    let mut pack = ItemBag::new(PACK_CAPACITY);

    let out = character_day(&mut needs, &mut pack, &Barren);

    assert_eq!(needs.food, MAX_FOOD - 1);
    assert_eq!(needs.water, 4);
    assert_eq!(needs.health, START_HEALTH, "already at the cap, so no healing to do");
    assert_eq!(out.deprivation, 0);
    assert_eq!(out.ate, None);
    assert!(!out.died);
}

/// Running out is punished on the day it happens, not the day after.
///
/// The character below has one point of each left. The day takes it, and
/// because deprivation is measured *before* the ration is spent, that day
/// costs no health — the next one does.
#[test]
fn the_day_you_run_out_is_free_and_the_next_one_is_not() {
    let mut needs = CharacterNeeds { food: 1, water: 1, ..fed() };
    let mut pack = ItemBag::new(PACK_CAPACITY);

    let first = character_day(&mut needs, &mut pack, &Barren);
    assert_eq!((needs.food, needs.water), (0, 0), "spent the last of both");
    assert_eq!(first.deprivation, 0, "they were not empty when the day was counted");
    assert_eq!(needs.health, START_HEALTH);

    let second = character_day(&mut needs, &mut pack, &Barren);
    assert_eq!(second.deprivation, 2 * DEPRIVATION, "empty of both");
    assert_eq!(needs.health, START_HEALTH - 50);
    assert_eq!((needs.food, needs.water), (0, 0), "and cannot go below nothing");
}

/// Going without food and going without water are counted separately.
#[test]
fn each_empty_need_costs_its_own_twenty_five() {
    for (food, water, expected) in [(0, 5, DEPRIVATION), (5, 0, DEPRIVATION), (0, 0, 2 * DEPRIVATION)] {
        let mut needs = CharacterNeeds { food, water, ..fed() };
        let mut pack = ItemBag::new(PACK_CAPACITY);
        let out = character_day(&mut needs, &mut pack, &Barren);
        assert_eq!(out.deprivation, expected, "food {food}, water {water}");
    }
}

// -- eating without being asked ---------------------------------------------

/// A character out of food eats from their own pack, and only then.
#[test]
fn a_character_with_nothing_left_eats_what_they_carry() {
    let mut needs = CharacterNeeds { food: 0, ..fed() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[SNAKE]);

    let out = character_day(&mut needs, &mut pack, &Items);

    assert_eq!(out.ate, Some(SNAKE));
    assert!(pack.is_empty(), "and the snake is gone");
    // Ate seven, then spent one on the day. No deprivation: the meal came
    // first, so the character was not empty when it was counted.
    assert_eq!(needs.food, 6);
    assert_eq!(out.deprivation, 0);
    assert_eq!(needs.health, START_HEALTH);
}

/// One point left is not nothing: the pack is not touched.
#[test]
fn a_character_with_one_point_left_keeps_the_snake_for_tomorrow() {
    let mut needs = CharacterNeeds { food: 1, ..fed() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[SNAKE]);

    let out = character_day(&mut needs, &mut pack, &Items);

    assert_eq!(out.ate, None);
    assert_eq!(pack.len(), 1, "still there");
    assert_eq!(needs.food, 0, "and now they are out");
}

/// The biggest meal is chosen, and the first of equals — `Group.FindFood`
/// compares strictly greater, so an earlier item of the same worth wins.
#[test]
fn the_largest_meal_is_the_one_eaten() {
    let bag = ItemBag::with(PACK_CAPACITY, &[MAGGOTS, SNAKE, MAGGOTS]);
    assert_eq!(find_food(&bag, &Items), Some(1), "the snake, not the maggots");

    let two_snakes = ItemBag::with(PACK_CAPACITY, &[MAGGOTS, SNAKE, SNAKE]);
    assert_eq!(find_food(&two_snakes, &Items), Some(1), "the first of equals");

    let nothing_edible = ItemBag::with(PACK_CAPACITY, &[9, 9]);
    assert_eq!(find_food(&nothing_edible, &Items), None);
    assert_eq!(find_food(&ItemBag::new(PACK_CAPACITY), &Items), None);
}

// -- healing and dying ------------------------------------------------------

/// Two points a night, and only from seventy up. Below the floor a character
/// does not recover at all, which is what makes one bad day compound.
#[test]
fn healing_stops_below_the_floor() {
    for (start, expected) in [
        (START_HEALTH, START_HEALTH),
        (99, MAX_HEALTH),
        (REGEN_FLOOR_WITHOUT_DOCTOR, REGEN_FLOOR_WITHOUT_DOCTOR + REGEN_WITHOUT_DOCTOR),
        (REGEN_FLOOR_WITHOUT_DOCTOR - 1, REGEN_FLOOR_WITHOUT_DOCTOR - 1),
        (10, 10),
    ] {
        let mut needs = CharacterNeeds { health: start, ..fed() };
        let mut pack = ItemBag::new(PACK_CAPACITY);
        character_day(&mut needs, &mut pack, &Barren);
        assert_eq!(needs.health, expected, "starting from {start}");
    }
}

/// Nothing to eat or drink: fifty a day from a hundred, so two days.
#[test]
fn a_character_with_nothing_starves_in_two_days() {
    let mut needs = CharacterNeeds { food: 0, water: 0, ..fed() };
    let mut pack = ItemBag::new(PACK_CAPACITY);

    // 100 -> 50 -> ... and no healing at all below seventy.
    let mut days = 0;
    loop {
        let out = character_day(&mut needs, &mut pack, &Barren);
        days += 1;
        if out.died {
            break;
        }
        assert!(days < 10, "still alive after {days} days with nothing");
    }
    assert_eq!(days, 2, "fifty a day from a hundred");
    assert!(needs.health <= 0);
}

/// A character kept watered but not fed lasts longer, and stops healing on the
/// way down — which is the shape of the thing, not just a smaller number.
#[test]
fn losing_one_need_is_survivable_for_a_while() {
    let mut needs = CharacterNeeds { food: 0, water: 5, ..fed() };
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let mut health = Vec::new();

    for _ in 0..5 {
        needs.water = 5; // drinking every day
        let out = character_day(&mut needs, &mut pack, &Barren);
        health.push(needs.health);
        if out.died {
            break;
        }
    }

    // 100 -25 = 75; then +2 -25 = 52; then no healing, 27, 2, dead.
    assert_eq!(health, [75, 52, 27, 2, -23]);
}

// -- springs ----------------------------------------------------------------

/// A spring fills whether or not anyone is there, and is capped afterwards.
#[test]
fn an_unvisited_spring_fills_up_and_stops_at_its_capacity() {
    let mut springs = Springs::new();
    springs.get_or_insert(7, || WaterSource::classic(0, 5, 6));

    let level = |s: &Springs| s.get(7).unwrap().reserve;
    assert_eq!(level(&springs), 0, "Muarab opens empty");

    springs.advance_day(burngfx::game::Engine::V091, |_| burngfx::game::Pump::None);
    assert_eq!(level(&springs), 5);
    springs.advance_day(burngfx::game::Engine::V091, |_| burngfx::game::Pump::None);
    assert_eq!(level(&springs), 6, "capped, not eleven");
    for _ in 0..10 {
        springs.advance_day(burngfx::game::Engine::V091, |_| burngfx::game::Pump::None);
    }
    assert_eq!(level(&springs), 6, "and it stays there");
}

/// Drinking and refilling, over several days, with the character leaving in
/// between. The spring is not a property of the visit.
#[test]
fn a_spring_refills_between_visits() {
    let mut springs = Springs::new();
    springs.get_or_insert(4, || WaterSource::classic(0, 5, 6));
    let mut needs = CharacterNeeds { water: 0, ..fed() };

    // Day one, and the spring has nothing to give.
    assert_eq!(springs.get_or_insert(4, || unreachable!()).drink(&mut needs), 0);

    springs.advance_day(burngfx::game::Engine::V091, |_| burngfx::game::Pump::None);
    // Come back: five have collected, and a parched character takes them all.
    assert_eq!(springs.get_or_insert(4, || unreachable!()).drink(&mut needs), 5);
    assert_eq!(needs.water, 5);
    assert_eq!(springs.get(4).unwrap().reserve, 0);

    // Away for three days, then back to a full one — six, not fifteen.
    for _ in 0..3 {
        springs.advance_day(burngfx::game::Engine::V091, |_| burngfx::game::Pump::None);
    }
    assert_eq!(springs.get(4).unwrap().reserve, 6);
}

/// The 1993 save's own numbers, so the starting state is the game's and not
/// a guess. Muarab opens dry.
#[test]
fn the_springs_come_from_the_save_with_the_levels_it_records() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/burntime.pak");
    let Ok(bytes) = std::fs::read(&path) else { return };
    let pak = burngfx::pak::ClassicPak::open(&bytes).unwrap();
    let Some(raw) = pak.get("burn_gfx/gam.dat") else { return };
    let gam = burngfx::gamdat::parse(raw).unwrap();

    let springs = Springs::from_gamdat(&gam);
    assert!(springs.len() > 10, "only {} springs in the world", springs.len());

    // Muarab: reserve 0, five a day, holding six. The reserve is not the
    // refresh rate, and building it that way would open the game with a free
    // drink nobody earned.
    let muarab = springs.get(4).expect("Muarab has a spring");
    assert_eq!(
        (muarab.reserve, muarab.refresh, muarab.capacity),
        (
            gam.locations[4].water_reserve,
            gam.locations[4].water_refresh,
            gam.locations[4].water_capacity
        )
    );
    assert_eq!(muarab.reserve, 0, "it opens dry");

    // Every spring the save records is here, and no location without one.
    for (i, loc) in gam.locations.iter().enumerate() {
        assert_eq!(springs.get(i).is_some(), loc.has_water(), "location {i}");
    }
}

// -- several days, end to end -----------------------------------------------

/// The cycle the whole of this is for: eat a snake, let days pass, watch the
/// stores fall and the spring fill, and see going without take hold.
///
/// One call to `advance_day` is one day, whatever else is going on — the
/// point of putting it in one place.
#[test]
fn a_snake_buys_a_week_and_the_spring_keeps_up() {
    let mut day = 1u16;
    let mut springs = Springs::new();
    let mut camps = burngfx::game::Camps::new();
    springs.get_or_insert(4, || WaterSource::classic(0, 5, 6));
    let mut needs = CharacterNeeds { food: 0, water: 0, ..fed() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[SNAKE]);

    // Day one: nothing to drink yet, and the snake is eaten unasked.
    let first = advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Items, &mut None);
    assert_eq!(day, 2);
    assert_eq!(first.ate, Some(SNAKE));
    assert_eq!(needs.food, 6, "seven eaten, one spent");
    assert_eq!(first.deprivation, DEPRIVATION, "still no water");
    assert_eq!(needs.health, 75);
    assert_eq!(springs.get(4).unwrap().reserve, 5, "and the spring filled");

    // Drink the spring dry, then live off the snake for a week.
    springs.get_or_insert(4, || unreachable!()).drink(&mut needs);
    assert_eq!(needs.water, 5);

    let mut days_fed = 0;
    while needs.food > 0 {
        needs.water = 5; // a daily drink, so only food is being tested
        advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Items, &mut None);
        days_fed += 1;
    }
    assert_eq!(days_fed, 6, "six more days out of one snake");
    assert_eq!(day, 8);
    assert!(pack.is_empty(), "and nothing left to eat");

    // The spring, meanwhile, has been filling the whole time and is capped.
    assert_eq!(springs.get(4).unwrap().reserve, 6);
}

// -- carrying water ---------------------------------------------------------

use burngfx::game::{drink_from, fill_container, find_water, FillError, MAX_WATER};

/// Filling is all or nothing, and it costs the spring the whole container.
///
/// `InventoryScene` tests `Reserve >= WaterValue` and does nothing at all
/// below it: a spring with one point does not half-fill a bottle, and does
/// not lose the point either. Nothing in the original fills partially.
#[test]
fn a_bottle_is_filled_whole_or_not_at_all() {
    let mut spring = WaterSource::classic(1, 5, 6);
    let mut pack = ItemBag::with(PACK_CAPACITY, &[BOTTLE]);

    // One point in the spring, two needed.
    assert_eq!(
        fill_container(&mut spring, &mut pack, 0, &Items),
        Err(FillError::NotEnoughWater)
    );
    assert_eq!(spring.reserve, 1, "and the point is still there");
    assert_eq!(pack.get(0), Some(BOTTLE), "still empty");

    // Enough, and it costs exactly the container's worth.
    spring.reserve = 5;
    assert_eq!(fill_container(&mut spring, &mut pack, 0, &Items), Ok(FULL_BOTTLE));
    assert_eq!(spring.reserve, 3);
    assert_eq!(pack.get(0), Some(FULL_BOTTLE), "and it kept its slot");
}

/// Only an empty container can be filled, and only a real slot.
#[test]
fn nothing_else_can_be_filled() {
    let mut spring = WaterSource::classic(6, 5, 6);
    let mut pack = ItemBag::with(PACK_CAPACITY, &[SNAKE, FULL_BOTTLE]);

    assert_eq!(
        fill_container(&mut spring, &mut pack, 0, &Items),
        Err(FillError::NotAContainer),
        "a snake is not a bottle"
    );
    assert_eq!(
        fill_container(&mut spring, &mut pack, 1, &Items),
        Err(FillError::NotAContainer),
        "and a full one is already full"
    );
    assert_eq!(fill_container(&mut spring, &mut pack, 9, &Items), Err(FillError::NoSuchSlot));
    assert_eq!(spring.reserve, 6, "no refusal costs the spring anything");
}

/// Drinking empties the container in place, and only if somebody drank.
#[test]
fn drinking_leaves_an_empty_bottle_in_the_same_slot() {
    let mut needs = CharacterNeeds { water: 0, ..fed() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[SNAKE, FULL_BOTTLE]);

    assert_eq!(drink_from(&mut pack, 1, &mut needs, &Items), Some(2));
    assert_eq!(needs.water, 2);
    assert_eq!(pack.get(1), Some(BOTTLE), "empty, and where it was");
    assert_eq!(pack.get(0), Some(SNAKE), "nothing else moved");

    // Nothing to drink in a snake.
    assert_eq!(drink_from(&mut pack, 0, &mut needs, &Items), None);
    assert_eq!(drink_from(&mut pack, 1, &mut needs, &Items), None, "nor in an empty bottle");
}

/// A full character does not open the bottle. A nearly full one does, and
/// wastes the rest — the same rule as food.
#[test]
fn a_bottle_is_used_up_only_if_somebody_drank() {
    let mut full = CharacterNeeds { water: MAX_WATER, ..fed() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[FULL_BOTTLE]);
    assert_eq!(drink_from(&mut pack, 0, &mut full, &Items), Some(0), "nobody drank");
    assert_eq!(pack.get(0), Some(FULL_BOTTLE), "so it is still full");
    assert_eq!(full.water, MAX_WATER);

    let mut nearly = CharacterNeeds { water: MAX_WATER - 1, ..fed() };
    assert_eq!(drink_from(&mut pack, 0, &mut nearly, &Items), Some(1), "one of the two");
    assert_eq!(nearly.water, MAX_WATER);
    assert_eq!(pack.get(0), Some(BOTTLE), "and the other is spilt");
}

/// `Group.FindWater` picks the largest, first of equals — like FindFood.
#[test]
fn the_fullest_container_is_the_one_drunk() {
    let bag = ItemBag::with(PACK_CAPACITY, &[BOTTLE, FULL_BOTTLE, SNAKE, FULL_BOTTLE]);
    assert_eq!(find_water(&bag, &Items), Some(1));
    assert_eq!(find_water(&ItemBag::with(PACK_CAPACITY, &[SNAKE, BOTTLE]), &Items), None);
}

/// The criterion for this piece of work, in one test.
///
/// Fill at the spring, leave, let a day pass, and the character drinks what
/// they carried — leaving an empty container in the pack. No water appears
/// from nowhere and no item is lost.
#[test]
fn a_filled_bottle_is_what_gets_the_character_through_the_night() {
    let mut day = 1u16;
    let mut springs = Springs::new();
    let mut camps = burngfx::game::Camps::new();
    springs.get_or_insert(4, || WaterSource::classic(6, 5, 6));
    let mut needs = CharacterNeeds { water: 1, food: MAX_FOOD, ..fed() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[BOTTLE, SNAKE]);

    // At the spring: fill the bottle.
    let spring = springs.get_or_insert(4, || unreachable!());
    assert_eq!(fill_container(spring, &mut pack, 0, &Items), Ok(FULL_BOTTLE));
    assert_eq!(spring.reserve, 4);

    // Walk out and let the day end. The last point of water is spent, so the
    // character is dry when the next day begins.
    let first = advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Items, &mut None);
    assert_eq!(first.drank, None, "not empty yet, so the bottle stays shut");
    assert_eq!(needs.water, 0);
    assert_eq!(pack.get(0), Some(FULL_BOTTLE));

    // The next day they wake with nothing, and drink what they carried.
    let second = advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Items, &mut None);
    assert_eq!(second.drank, Some(FULL_BOTTLE));
    assert_eq!(second.deprivation, 0, "the drink came before the reckoning");
    assert_eq!(needs.health, START_HEALTH, "and cost them no health");
    assert_eq!(needs.water, 1, "drank two, spent one");
    assert_eq!(pack.get(0), Some(BOTTLE), "an empty bottle, in its slot");
    assert_eq!(pack.get(1), Some(SNAKE), "and the snake untouched");
    assert_eq!(pack.len(), 2, "nothing lost, nothing gained");

    // And the spring has been filling all the while, capped at six.
    assert_eq!(springs.get(4).unwrap().reserve, 6);
}

/// Without the bottle, the same two days cost the character health. This is
/// the comparison that says the bottle did the work.
#[test]
fn the_same_two_days_without_a_bottle_cost_health() {
    let mut day = 1u16;
    let mut springs = Springs::new();
    let mut camps = burngfx::game::Camps::new();
    springs.get_or_insert(4, || WaterSource::classic(6, 5, 6));
    let mut needs = CharacterNeeds { water: 1, food: MAX_FOOD, ..fed() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[SNAKE]);

    advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Items, &mut None);
    let second = advance_day(&mut day, &mut world(&mut springs, &mut camps), &mut needs, &mut burngfx::game::Party::new(), &mut pack, &Items, &mut None);

    assert_eq!(second.drank, None, "nothing to drink");
    assert_eq!(second.deprivation, DEPRIVATION);
    assert_eq!(needs.health, START_HEALTH - DEPRIVATION);
}

// -- how long the stores last -----------------------------------------------

use burngfx::game::days_of_supplies;

/// The forecast counts what is in the pack, which is the whole reason it
/// exists: `min(food, water)` said one day whether or not the character was
/// carrying water, and a bottle visibly changes the journey.
#[test]
fn the_forecast_counts_the_pack_and_not_only_the_belly() {
    let low = CharacterNeeds { food: 5, water: 1, ..fed() };

    let nothing = ItemBag::new(PACK_CAPACITY);
    assert_eq!(days_of_supplies(&low, &nothing, &Items, 60), 1, "one day, then dry");

    // A full bottle is two more points of water, drunk on the morning the
    // character wakes with none — so two more days on the road.
    let with_water = ItemBag::with(PACK_CAPACITY, &[FULL_BOTTLE]);
    assert_eq!(days_of_supplies(&low, &with_water, &Items, 60), 3);

    // An empty one changes nothing, and neither does a bottle they cannot
    // reach a spring with.
    let empty = ItemBag::with(PACK_CAPACITY, &[BOTTLE]);
    assert_eq!(days_of_supplies(&low, &empty, &Items, 60), 1);
}

/// Whichever runs out first is the answer, and a pack full of food does not
/// mend a shortage of water.
///
/// This test was written expecting eight days from a snake and was wrong:
/// the character in it is carrying five days of water, and food they cannot
/// drink does not extend that. Running the real rules said so; a formula
/// would have agreed with whatever it was told.
#[test]
fn the_forecast_is_bounded_by_whichever_runs_out_first() {
    let hungry = CharacterNeeds { food: 1, water: 5, ..fed() };
    let nothing = ItemBag::new(PACK_CAPACITY);
    assert_eq!(days_of_supplies(&hungry, &nothing, &Items, 60), 1, "out of food tomorrow");

    // A snake covers the hunger, and then the water is what runs out.
    let with_food = ItemBag::with(PACK_CAPACITY, &[SNAKE]);
    assert_eq!(days_of_supplies(&hungry, &with_food, &Items, 60), 5, "five days of water");

    // Water as well, and now the snake is what decides it: nine days of
    // water against eight of food, and eight is the answer.
    let with_both = ItemBag::with(PACK_CAPACITY, &[SNAKE, FULL_BOTTLE, FULL_BOTTLE]);
    assert_eq!(days_of_supplies(&hungry, &with_both, &Items, 60), 8);

    // Out of both, and carrying nothing: not a single day.
    let empty_handed = CharacterNeeds { food: 0, water: 0, ..fed() };
    assert_eq!(days_of_supplies(&empty_handed, &nothing, &Items, 60), 0);
}

/// It is a forecast, not a rehearsal: asking must not spend anything.
#[test]
fn asking_how_long_the_stores_last_does_not_consume_them() {
    let needs = CharacterNeeds { food: 4, water: 3, ..fed() };
    let pack = ItemBag::with(PACK_CAPACITY, &[SNAKE, FULL_BOTTLE]);

    let first = days_of_supplies(&needs, &pack, &Items, 60);
    for _ in 0..5 {
        assert_eq!(days_of_supplies(&needs, &pack, &Items, 60), first);
    }
    assert_eq!(needs, CharacterNeeds { food: 4, water: 3, ..fed() }, "the character moved");
    assert_eq!(pack.len(), 2, "the pack was emptied");
}

/// The cap is a cap, not an answer: a character who could walk for a month is
/// reported as reaching the limit rather than looping to the end of time.
#[test]
fn the_forecast_stops_at_the_limit_it_is_given() {
    let full = fed();
    // Food and water both, or the water would decide it at five days.
    let larder = ItemBag::with(
        PACK_CAPACITY,
        &[SNAKE, SNAKE, FULL_BOTTLE, FULL_BOTTLE, FULL_BOTTLE, FULL_BOTTLE],
    );
    assert_eq!(days_of_supplies(&full, &larder, &Items, 5), 5, "capped");
    assert!(days_of_supplies(&full, &larder, &Items, 60) > 5, "and uncapped, further");
}

/// And it agrees with what actually happens: walk the days it promises and
/// the character is still whole; walk one more and they are not.
#[test]
fn the_forecast_matches_the_days_that_follow() {
    let start = CharacterNeeds { food: 3, water: 2, ..fed() };
    let carried = ItemBag::with(PACK_CAPACITY, &[FULL_BOTTLE]);
    let promised = days_of_supplies(&start, &carried, &Items, 60);

    let mut needs = start;
    let mut pack = carried.clone();
    for day in 0..promised {
        let out = character_day(&mut needs, &mut pack, &Items);
        assert_eq!(out.deprivation, 0, "went without on day {day}, sooner than promised");
    }
    assert_eq!(needs.health, START_HEALTH, "and lost nothing on the way");

    let out = character_day(&mut needs, &mut pack, &Items);
    assert!(out.deprivation > 0, "the day after the forecast should be the hard one");
}

// -- travelling together -----------------------------------------------------

use burngfx::game::{group_find_food, party_day, Party, Whose};

/// A travelling group shares one larder, and the source says so plainly.
///
/// `Character.Turn` asks `GetGroup()`, which returns the player's whole group
/// for anyone in it; `Group.FindFood` then walks **every** member's items. So
/// a companion's snake feeds the leader, and the leader's feeds them.
///
/// A camped person is the other case: `JoinCamp` takes them out of the group,
/// so `GetGroup()` returns themselves and they eat from the location and
/// their own pack alone. Neither assumption — all shared, all separate — is
/// right on its own.
#[test]
fn a_travelling_group_eats_out_of_everybodys_pack() {
    let leader_pack = ItemBag::new(PACK_CAPACITY);
    let mut party = Party::new();
    party.recruit();
    party.companions[0].pack.push(SNAKE);

    // The best food anywhere, and it is not the leader's — they have none.
    assert_eq!(
        group_find_food(&leader_pack, &party.companions, &Items),
        Some((Whose::Companion(0), 0))
    );

    // A hungry leader eats it, out of the companion's pack.
    let mut leader = CharacterNeeds { food: 0, ..fed() };
    party.companions[0].needs = CharacterNeeds { food: 9, ..fed() };
    let mut pack = leader_pack.clone();
    let out = party_day(&mut leader, &mut pack, &mut party, &Items);

    assert_eq!(out.ate, Some(SNAKE));
    assert!(party.companions[0].pack.is_empty(), "the snake was not taken from them");
    assert!(leader.food > 0, "the leader went hungry beside a full pack");
}

/// `Group.Eat` spreads a meal a level at a time, so it evens the group out
/// rather than filling whoever asked first.
#[test]
fn a_shared_meal_is_spread_rather_than_given_to_one() {
    let mut leader = CharacterNeeds { food: 0, ..fed() };
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = CharacterNeeds { food: 0, ..fed() };

    // A snake is seven points between two empty people.
    let left = burngfx::game::group_eat(&mut leader, &mut party.companions, 7);
    assert_eq!(left, 0, "none of it was wasted");
    assert_eq!((leader.food, party.companions[0].needs.food), (4, 3), "one level at a time");
}

/// A companion keeps who they are and what they carry across a stay in a
/// camp — the original moves the character, not a copy of it.
#[test]
fn a_companion_is_the_same_person_after_a_spell_in_a_camp() {
    let mut party = Party::new();
    let id = party.recruit();
    party.companions[0].pack.push(SNAKE);
    party.companions[0].needs.food = 4;

    let mut camps = burngfx::game::Camps::new();
    assert!(party.leave_one(&mut camps, 2, 0));
    assert!(party.is_empty());

    let resident = &camps.get(2).unwrap().residents[0];
    assert_eq!(resident.id, id);
    assert_eq!(resident.pack.get(0), Some(SNAKE));
    assert_eq!(resident.needs.food, 4);

    assert!(party.take_one_back(&mut camps, 2, 0));
    assert_eq!(party.companions[0].id, id, "a different person came back");
    assert_eq!(party.companions[0].pack.get(0), Some(SNAKE), "their pack was emptied");
    assert_eq!(party.companions[0].needs.food, 4, "their appetite was reset");
}

/// The order of a group is the source's, not a convenience.
///
/// `Player.Character`'s setter inserts the leader at 0; `Hire` appends; and
/// `LeaveCamp` appends too, so somebody taken back out of a camp rejoins at
/// the **back** rather than in their old place. Both places that walk the
/// list care: `FindFood` takes the first of equals, `Eat` fills the equally
/// hungry in order.
#[test]
fn the_group_is_ordered_by_when_each_one_last_joined_it() {
    let mut party = Party::new();
    let first = party.recruit();
    let second = party.recruit();
    assert_eq!(
        party.companions.iter().map(|c| c.id).collect::<Vec<_>>(),
        [first, second],
        "hiring appends"
    );

    // Leave the first one somewhere and take them back: they come back last.
    let mut camps = burngfx::game::Camps::new();
    assert!(party.leave(&mut camps, 1, 0, 0));
    assert_eq!(party.companions.iter().map(|c| c.id).collect::<Vec<_>>(), [second]);
    assert!(party.take_one_back(&mut camps, 1, 0));
    assert_eq!(
        party.companions.iter().map(|c| c.id).collect::<Vec<_>>(),
        [second, first],
        "rejoining should put them at the back, as `Group.Add` does"
    );

    // And leaving names a person rather than taking whoever is last.
    assert!(party.leave(&mut camps, 2, 0, 0));
    assert_eq!(camps.get(2).unwrap().residents[0].id, second);
    assert!(!party.leave(&mut camps, 2, 0, 5), "left somebody who is not there");
}

/// Ties in `FindFood` go to whoever is earlier in the list, and the leader
/// is always first.
#[test]
fn a_tie_between_two_equal_meals_goes_to_the_earlier_pack() {
    let mut leader_pack = ItemBag::new(PACK_CAPACITY);
    leader_pack.push(SNAKE);
    let mut party = Party::new();
    party.recruit();
    party.companions[0].pack.push(SNAKE);

    assert_eq!(
        group_find_food(&leader_pack, &party.companions, &Items),
        Some((Whose::Leader, 0)),
        "the leader is index 0, and equals do not displace"
    );

    // Take the leader's away and it falls to the companion.
    leader_pack.remove(0);
    assert_eq!(
        group_find_food(&leader_pack, &party.companions, &Items),
        Some((Whose::Companion(0), 0))
    );
}

/// Not enough to go round: the hungriest eat first, whatever the order.
///
/// `Group.Eat` walks the levels, not the list — so a single point goes to the
/// emptiest person even when somebody earlier in the group is also hungry.
/// A test with full packs would never show the difference.
#[test]
fn a_short_meal_goes_to_the_emptiest_first() {
    // The leader has two points, the companion none. One point to share.
    let mut leader = CharacterNeeds { food: 2, ..fed() };
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = CharacterNeeds { food: 0, ..fed() };

    let left = burngfx::game::group_eat(&mut leader, &mut party.companions, 1);
    assert_eq!(left, 0);
    assert_eq!(leader.food, 2, "the leader ate although somebody was emptier");
    assert_eq!(party.companions[0].needs.food, 1);

    // Level by level from there: the next point evens them, then order tells.
    let mut leader = CharacterNeeds { food: 1, ..fed() };
    party.companions[0].needs = CharacterNeeds { food: 1, ..fed() };
    let left = burngfx::game::group_eat(&mut leader, &mut party.companions, 1);
    assert_eq!(left, 0);
    assert_eq!(
        (leader.food, party.companions[0].needs.food),
        (2, 1),
        "equally hungry, so the earlier one eats"
    );

    // And a meal too big for the group is not all swallowed.
    let mut leader = CharacterNeeds { food: MAX_FOOD - 1, ..fed() };
    party.companions[0].needs = CharacterNeeds { food: MAX_FOOD - 1, ..fed() };
    let left = burngfx::game::group_eat(&mut leader, &mut party.companions, 7);
    assert_eq!(left, 5, "two points of room between them, five wasted");
}

/// Scarcity over several days: the group empties evenly rather than one
/// person starving while another is full.
#[test]
fn a_group_running_short_goes_down_together() {
    let mut leader = CharacterNeeds { food: 1, water: 9, max_water: 9, ..fed() };
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = CharacterNeeds { food: 1, water: 9, max_water: 9, ..fed() };

    // One snake between two, and nothing else.
    let mut pack = ItemBag::with(PACK_CAPACITY, &[SNAKE]);

    // Day one spends their last point each; day two they wake empty and the
    // snake is shared out.
    party_day(&mut leader, &mut pack, &mut party, &Items);
    assert_eq!((leader.food, party.companions[0].needs.food), (0, 0));
    assert_eq!(pack.len(), 1, "eaten too early");

    party_day(&mut leader, &mut pack, &mut party, &Items);
    assert!(pack.is_empty(), "the snake was not eaten");
    // Seven points between two empty people, then a day each off the top.
    assert_eq!((leader.food, party.companions[0].needs.food), (3, 2));
    assert_eq!(leader.health, START_HEALTH, "the leader went without anyway");
    assert_eq!(party.companions[0].needs.health, START_HEALTH);
}
