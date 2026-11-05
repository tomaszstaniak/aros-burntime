//! v0.9.1's `ClassicAiState` against a small world of its own.
//!
//! Each test pins one thing the source does, cited where it is not obvious.
//! The world here records what the AI asked of it; the real effects are the
//! game's and are checked on runs.

use burngfx::classic_ai::*;
use burngfx::gamdat::Danger;
use burngfx::settings::GameSettings;
use burngfx::spawn::Rng;

#[derive(Default)]
struct World {
    day: u32,
    here: usize,
    previous: Option<usize>,
    me: usize,
    cities: Vec<usize>,
    owners: Vec<(usize, usize)>,
    roads: Vec<(usize, usize, u8)>,
    /// What each place makes, as produced item names.
    products: Vec<(usize, Vec<String>)>,
    dangers: Vec<(usize, Danger)>,
    group: usize,
    hireable: usize,
    human_camps: usize,
    ground: Vec<String>,
    // what was asked
    hired: Vec<Option<String>>,
    camped: Vec<(usize, Camper, Option<String>)>,
    travelled: Vec<usize>,
    refreshed: u32,
    watered: u32,
}

impl World {
    fn at(here: usize) -> World {
        World { day: 1, here, group: 1, ..World::default() }
    }
    fn road(mut self, a: usize, b: usize, days: u8) -> World {
        self.roads.push((a, b, days));
        self
    }
    fn makes(mut self, l: usize, what: &[&str]) -> World {
        self.products.push((l, what.iter().map(|s| s.to_string()).collect()));
        self
    }
}

impl AiWorld for World {
    fn day(&self) -> u32 { self.day }
    fn here(&self) -> usize { self.here }
    fn previous(&self) -> Option<usize> { self.previous }
    fn me(&self) -> usize { self.me }
    fn is_city(&self, l: usize) -> bool { self.cities.contains(&l) }
    fn owner(&self, l: usize) -> Option<usize> {
        self.owners.iter().find(|(x, _)| *x == l).map(|(_, p)| *p)
    }
    fn ways(&self, l: usize) -> Vec<(usize, u8)> {
        self.roads
            .iter()
            .filter_map(|&(a, b, d)| if a == l { Some((b, d)) } else if b == l { Some((a, d)) } else { None })
            .collect()
    }
    fn products(&self, l: usize) -> Vec<String> {
        self.products.iter().find(|(x, _)| *x == l).map(|(_, p)| p.clone()).unwrap_or_default()
    }
    fn danger(&self, l: usize) -> Danger {
        self.dangers.iter().find(|(x, _)| *x == l).map(|(_, d)| *d).unwrap_or(Danger::None)
    }
    fn produce_of(&self, tool: &str) -> Option<String> {
        Some(match tool {
            "item_knife" => "item_maggots",
            "item_rat_trap" => "item_rats",
            "item_snake_trap" => "item_snake",
            "item_trap" => "item_meat",
            _ => return None,
        }.to_string())
    }
    fn camp_count(&self) -> usize {
        self.owners.iter().filter(|(_, p)| *p == self.me).count()
    }
    fn max_human_camp_count(&self) -> usize { self.human_camps }
    fn group_count(&self) -> usize { self.group }
    fn can_hire(&self) -> bool { self.hireable > 0 }
    fn clear_ground(&mut self) -> Vec<String> { std::mem::take(&mut self.ground) }
    fn hire(&mut self, weapon: Option<&str>) {
        self.hireable -= 1;
        self.group += 1;
        self.hired.push(weapon.map(String::from));
    }
    fn join_camp(&mut self, who: Camper, trap: Option<&str>, _rng: &mut Rng) {
        self.group -= 1;
        self.owners.push((self.here, self.me));
        self.camped.push((self.here, who, trap.map(String::from)));
    }
    fn travel(&mut self, to: usize) {
        self.previous = Some(self.here);
        self.travelled.push(to);
    }
    fn refresh(&mut self) { self.refreshed += 1 }
    fn water_goal(&mut self) { self.watered += 1 }
}

fn settings(min: i32, max: i32, advance: i32) -> GameSettings {
    GameSettings { ai_camp_interval: (min, max), ai_camp_max_advance: advance, ..GameSettings::default() }
}

fn fresh(s: &GameSettings) -> ClassicAi {
    ClassicAi::new(s, &mut Rng::seeded(1))
}

/// `InitInstance`: waiting, with an interval already drawn from
/// `[MinInterval, MaxInterval)`.
#[test]
fn it_starts_waiting_on_a_drawn_interval() {
    let s = settings(3, 6, 1);
    for seed in 0..50 {
        let ai = ClassicAi::new(&s, &mut Rng::seeded(seed));
        assert_eq!(ai.mode, Mode::WaitInterval);
        assert!((3..6).contains(&ai.wait), "{}", ai.wait);
    }
}

/// `FirstTurn` on day 1: three knives, a rat trap and a snake trap. Day 1 is
/// not divisible by anything in the schedule, so that is all.
#[test]
fn day_one_fills_the_pool_for_free() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    let mut w = World::at(0);
    turn(&mut ai, &mut w, &s, &mut Rng::seeded(2));
    assert_eq!(
        ai.pool.items,
        vec![("item_knife".into(), 3), ("item_rat_trap".into(), 1), ("item_snake_trap".into(), 1)]
    );
}

/// `UpdateItems`: the knife twice on a day divisible by three, and the rest
/// on their own days — the "protection suit" on the fiftieth, though no such
/// item exists.
#[test]
fn the_schedule_adds_items_on_its_days() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    let mut w = World::at(0);
    w.day = 150; // divisible by 3, 10, 15, 25, 50
    turn(&mut ai, &mut w, &s, &mut Rng::seeded(2));
    let count = |id: &str| ai.pool.items.iter().find(|(t, _)| t == id).map_or(0, |(_, n)| *n);
    assert_eq!(count("item_knife"), 2);
    assert_eq!(count("item_axe"), 1);
    assert_eq!(count("item_pitchfork"), 1);
    assert_eq!(count("item_gas_mask"), 1);
    assert_eq!(count("item_protection_suit"), 1);
    assert_eq!(count("item_loaded_rifle"), 0);
}

/// Everything on the ground where it stands goes into the pool — or nowhere,
/// if the filter does not take it. Either way it is gone from the world.
#[test]
fn the_ground_is_swept_into_the_pool() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    let mut w = World::at(0);
    w.day = 2;
    w.ground = vec!["item_axe".into(), "item_meat".into()];
    let done = turn(&mut ai, &mut w, &s, &mut Rng::seeded(2));
    assert_eq!(done.took, ["item_axe", "item_meat"]);
    assert!(w.ground.is_empty());
    assert!(ai.pool.items.iter().any(|(t, _)| t == "item_axe"));
    assert!(ai.pool.items.iter().all(|(t, _)| t != "item_meat"));
}

/// Alone, it hires; having hired, it looks for a camp **in the same turn**;
/// able to camp here, it hires again for the camper — `GetNpcForCamp` hires
/// before it gives up a companion — and then waits a drawn interval.
#[test]
fn hire_and_camp_the_same_day() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    let mut w = World::at(5).makes(5, &["item_maggots"]);
    w.hireable = 2;
    w.human_camps = 0; // 0 + 1 > 0: room for one camp
    turn(&mut ai, &mut w, &s, &mut Rng::seeded(3));

    assert_eq!(w.hired.len(), 2, "one to travel with, one to leave behind");
    assert!(w.hired.iter().all(|weapon| weapon.as_deref() == Some("item_knife")));
    assert_eq!(w.camped, [(5, Camper::JustHired, Some("item_knife".to_string()))]);
    assert_eq!(ai.mode, Mode::WaitInterval);
    assert!((3..6).contains(&ai.wait));
    assert_eq!(w.refreshed, 1, "its own camp now: topped up");
    assert_eq!(w.watered, 1);
}

/// With nobody to hire, the camper is the **first** companion.
#[test]
fn with_nobody_to_hire_the_first_companion_stays() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    ai.mode = Mode::LookForNextCamp;
    let mut w = World::at(5).makes(5, &["item_maggots"]);
    w.day = 2;
    w.group = 3;
    ai.pool.insert("item_knife");
    turn(&mut ai, &mut w, &s, &mut Rng::seeded(3));
    assert_eq!(w.camped, [(5, Camper::First, Some("item_knife".to_string()))]);
}

/// The trap is the best one for what the place makes: trap before snake trap
/// before rat trap before knife, and only one whose product is on offer.
#[test]
fn the_trap_matches_the_place() {
    let mut pool = Pool::default();
    for id in ["item_knife", "item_rat_trap", "item_snake_trap"] {
        pool.insert(id);
    }
    let produce = |t: &str| -> Option<String> {
        Some(match t {
            "item_knife" => "item_maggots",
            "item_rat_trap" => "item_rats",
            "item_snake_trap" => "item_snake",
            "item_trap" => "item_meat",
            _ => return None,
        }.to_string())
    };
    let offer = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(pool.best_trap(&offer(&["item_maggots", "item_rats"]), &produce), Some("item_rat_trap"));
    assert_eq!(pool.best_trap(&offer(&["item_maggots"]), &produce), Some("item_knife"));
    assert_eq!(pool.best_trap(&offer(&["item_meat"]), &produce), None, "no trap in the pool");
}

/// Not a city, nobody's, a trap for it — and in a danger, the right
/// protection in the pool. The placeholder "protection suit" is what opens
/// radiation, and gas as well.
#[test]
fn where_it_may_camp() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    ai.pool.insert("item_knife");
    let mut w = World::at(0).makes(1, &["item_maggots"]).makes(2, &["item_maggots"]).makes(3, &["item_maggots"]);
    w.cities = vec![0];
    w.dangers = vec![(2, Danger::Radiation), (3, Danger::Gas)];
    w.owners = vec![(4, 1)];
    assert!(!can_create_camp(&ai, &w, 0), "a city");
    assert!(can_create_camp(&ai, &w, 1));
    assert!(!can_create_camp(&ai, &w, 2), "radiation, no suit");
    assert!(!can_create_camp(&ai, &w, 3), "gas, no mask");
    assert!(!can_create_camp(&ai, &w, 4), "somebody's");
    ai.pool.insert("item_protection_suit");
    assert!(can_create_camp(&ai, &w, 2));
    assert!(can_create_camp(&ai, &w, 3), "the placeholder counts as a gas mask too");
}

/// At its limit — the best human's camps plus `ai_camp_max_advance` — it
/// neither camps nor goes looking, turn after turn.
#[test]
fn at_the_limit_it_stays_put() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    ai.mode = Mode::LookForNextCamp;
    let mut w = World::at(5).makes(5, &["item_maggots"]).road(5, 6, 1);
    w.day = 2;
    w.group = 2;
    w.owners = vec![(7, 0)];
    ai.pool.insert("item_knife");
    w.human_camps = 0; // 0 + 1 <= 1
    turn(&mut ai, &mut w, &s, &mut Rng::seeded(4));
    assert!(w.camped.is_empty() && w.travelled.is_empty());
    assert_eq!(ai.mode, Mode::None);
}

/// Nowhere to camp here: set out for the nearest place it could, and a place
/// next door wins over anything further off.
#[test]
fn it_heads_for_the_nearest_camp_site() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    ai.mode = Mode::LookForNextCamp;
    ai.pool.insert("item_knife");
    let mut w = World::at(0).road(0, 1, 3).road(0, 2, 1).road(2, 3, 1).makes(1, &["item_maggots"]).makes(3, &["item_maggots"]);
    w.cities = vec![0, 2];
    w.day = 2;
    w.group = 2;
    turn(&mut ai, &mut w, &s, &mut Rng::seeded(4));
    assert_eq!(w.travelled, [1], "the three-day neighbour, before the two-day way round");
    assert_eq!(ai.headed, Some(1));
}

/// `NearestCity` takes no one-day road to a city.
#[test]
fn a_city_one_day_away_is_not_where_it_goes_to_hire() {
    let s = settings(3, 6, 1);
    let ai = fresh(&s);
    let mut w = World::at(0).road(0, 1, 1).road(0, 2, 2);
    w.cities = vec![1, 2];
    assert_eq!(nearest_city(&ai, &w), Some(2));
}

/// …and failing a city further off, it goes where the *camp* search points.
#[test]
fn failing_a_city_it_follows_the_camp_search() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    ai.pool.insert("item_knife");
    let w = World::at(0).road(0, 1, 1).road(0, 2, 2).road(1, 3, 1).makes(3, &["item_maggots"]);
    // 2 is no city and makes nothing; 3 is a camp site behind 1.
    assert_eq!(nearest_city(&ai, &w), Some(1));
}

/// Out of somebody else's camp, the camp search goes only back the way it
/// came — but the city search does not ask.
#[test]
fn leaving_somebody_elses_camp() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    ai.pool.insert("item_knife");
    let mut w = World::at(0).road(0, 1, 2).road(0, 2, 2).makes(1, &["item_maggots"]);
    w.owners = vec![(0, 9)];
    w.cities = vec![2];
    w.previous = Some(2);
    assert_eq!(nearest_free_camp(&ai, &w), None, "1 is not the way it came");
    assert_eq!(nearest_city(&ai, &w), Some(2));
}

/// Counting down, it waits; at zero and under the limit it looks again, the
/// same turn.
#[test]
fn waiting_runs_out() {
    let s = settings(3, 6, 1);
    let mut ai = fresh(&s);
    ai.wait = 1;
    let mut w = World::at(0).road(0, 1, 2).makes(1, &["item_maggots"]);
    w.cities = vec![0];
    w.day = 2;
    w.group = 2;
    ai.pool.insert("item_knife");
    turn(&mut ai, &mut w, &s, &mut Rng::seeded(5));
    assert_eq!(ai.mode, Mode::LookForNextCamp);
    assert_eq!(w.travelled, [1]);
    assert_eq!(w.refreshed, 1, "a city: topped up");
}

// -- the effects ---------------------------------------------------------------

use burngfx::game::{CharacterNeeds, Companion, ItemBag, Point, WaterSource, PACK_CAPACITY};
use burngfx::items::ItemTypes;

fn table(file: &str) -> Option<ItemTypes> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(file);
    Some(ItemTypes::parse(&std::fs::read_to_string(path).ok()?))
}

/// `RefreshGroupReserves`: meat and a full wineskin, out of nothing, and an
/// empty wineskin filled.
#[test]
fn the_refresh_hands_over_meat_and_a_full_wineskin() {
    let Some(types) = table("items_original.txt") else { return };
    let (meat, empty, full) = (
        types.index_of("item_meat").unwrap(),
        types.index_of("item_empty_wineskin").unwrap(),
        types.index_of("item_full_wineskin").unwrap(),
    );
    let mut pack = ItemBag::new(PACK_CAPACITY);
    refresh_reserves(&mut pack, &types);
    assert_eq!(pack.iter().collect::<Vec<_>>(), [meat, full]);

    let mut pack = ItemBag::with(PACK_CAPACITY, &[empty, empty]);
    refresh_reserves(&mut pack, &types);
    assert_eq!(pack.iter().collect::<Vec<_>>(), [full, full, meat], "no second wineskin; both filled");
}

/// `Group.Drink(null, 10)` and the rest: everybody, near or far, a level at a
/// time; health up by a hundred points in all.
#[test]
fn the_refresh_tops_everybody_up() {
    let mut leader = CharacterNeeds { food: 0, max_food: 9, water: 0, max_water: 5, health: 40 };
    let mut far = Companion::new(1);
    far.needs = CharacterNeeds { food: 0, max_food: 9, water: 0, max_water: 5, health: 40 };
    far.at = Point::new(900, 900);
    let mut group = vec![far];
    refresh_attributes(&mut leader, &mut group);
    assert_eq!((leader.water, group[0].needs.water), (5, 5));
    assert_eq!((leader.food, group[0].needs.food), (5, 5));
    assert_eq!((leader.health, group[0].needs.health), (90, 90));
}

/// `WaterGoal`: every item worth anything counts as an "empty water item",
/// so with a water room here the group's kit goes into it until it is full.
#[test]
fn the_water_goal_carries_the_kit_into_the_water_room() {
    let Some(types) = table("items_original.txt") else { return };
    let knife = types.index_of("item_knife").unwrap();
    let bottle = types.index_of("item_bottle").unwrap();
    let full_bottle = types.full_of(bottle).unwrap();
    let mut leader = CharacterNeeds { food: 5, max_food: 9, water: 5, max_water: 5, health: 100 };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[knife, bottle]);
    let mut spring = WaterSource::classic(10, 5, 10);
    let mut room = ItemBag::new(8);
    let moved = water_goal(
        &mut leader, Point::new(0, 0), &mut pack, &mut [], Some(&mut spring), Some(&mut room),
        &types, &|_| 1.0,
    );
    assert_eq!(moved, 2);
    assert!(pack.is_empty(), "knife and all");
    assert_eq!(room.iter().collect::<Vec<_>>(), [knife, full_bottle], "the bottle filled on the way");
    assert!(spring.reserve < 10);
}

/// Nothing worth nothing moves, and a full room stops it.
#[test]
fn the_water_goal_stops_at_a_full_room() {
    let Some(types) = table("items_original.txt") else { return };
    let knife = types.index_of("item_knife").unwrap();
    let mut leader = CharacterNeeds { food: 5, max_food: 9, water: 5, max_water: 5, health: 100 };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[knife, knife, knife]);
    let mut spring = WaterSource::classic(0, 0, 10);
    let mut room = ItemBag::with(8, &[knife; 7]);
    let moved = water_goal(
        &mut leader, Point::new(0, 0), &mut pack, &mut [], Some(&mut spring), Some(&mut room),
        &types, &|_| 1.0,
    );
    assert_eq!((moved, pack.len()), (1, 2));
}

/// `HireNpc`: the leader's pack is emptied whatever was in it.
#[test]
fn a_hire_empties_the_leaders_pack() {
    let mut pack = ItemBag::with(PACK_CAPACITY, &[1, 2, 3]);
    assert_eq!(ai_pays(&mut pack, &[10, 11, 12]), Some(12), "the last thing they accept");
    assert!(pack.is_empty());
}

/// Original's table has no pistol; the pool's "pistol" is a placeholder that
/// is carried and never drawn.
#[test]
fn placeholders_are_carried_not_drawn() {
    let Some(mut types) = table("items_original.txt") else { return };
    let real = types.len();
    types.with_placeholders(&MAY_BE_PLACEHOLDERS);
    let pistol = types.index_of("item_loaded_pistol").unwrap();
    assert!(pistol >= real, "appended, so no real number moves");
    assert_eq!(types.index_of("item_protective_suit").is_some(), true);
    assert!(types.index_of("item_protection_suit").unwrap() >= real);

    let mut party = burngfx::game::Party::new();
    armed_recruit(&mut party, CharacterNeeds::at_game_start(), 10, 40, Point::new(0, 0), Some((pistol, false)));
    let c = &party.companions[0];
    assert_eq!(c.pack.iter().collect::<Vec<_>>(), [pistol]);
    assert_eq!(c.kit.weapon, None);
}
