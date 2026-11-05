//! A machine player that plays by the same rules as the human.

use burngfx::game::{CharacterNeeds, ItemBag, ItemFacts, Journey, PACK_CAPACITY};
use burngfx::rival::{
    can_camp, draw_interval, may_expand, next_step, supply, turn, Act, Ground, Mode, Rival,
};
use burngfx::settings::GameSettings;
use burngfx::spawn::Rng;

const MEAT: usize = 1;
const KNIFE: usize = 2;

struct Table;

impl ItemFacts for Table {
    fn food(&self, kind: usize) -> i32 {
        if kind == MEAT {
            4
        } else {
            0
        }
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
    fn kind_named(&self, name: &str) -> Option<usize> {
        match name {
            "item_meat" => Some(MEAT),
            "item_knife" => Some(KNIFE),
            _ => None,
        }
    }
    fn name_of(&self, kind: usize) -> Option<&str> {
        match kind {
            MEAT => Some("item_meat"),
            KNIFE => Some("item_knife"),
            _ => None,
        }
    }
}

/// A small world:
///
/// ```text
///   0(city) — 1 — 2 — 3
///                  \— 4(city)
/// ```
struct World {
    cities: Vec<usize>,
    roads: Vec<(usize, usize, u8)>,
    owners: Vec<(usize, usize)>,
    people: Vec<usize>,
    water: Vec<(usize, i32)>,
    /// Something edible lying in a location, by the handle the caller uses.
    food: Vec<(usize, usize)>,
    /// A food production that could be set going, by location.
    products: Vec<(usize, usize)>,
    /// This player's own places that have something to eat in them.
    larder: Vec<usize>,
    /// A pack slot holding something that would work a camp here.
    tool: Option<usize>,
    /// Something lying in a location that would pay a mercenary.
    pay: Vec<(usize, usize)>,
    /// A production tool lying in a location.
    tools: Vec<(usize, usize)>,
    /// This player's own places with wages standing in them.
    wages: Vec<usize>,
    /// A pack slot the water in a location would fill.
    fillable: Vec<(usize, usize)>,
    /// Locations where a man left behind could not keep himself.
    barren: Vec<usize>,
}

impl World {
    fn small() -> World {
        World {
            cities: vec![0, 4],
            roads: vec![(0, 1, 1), (1, 2, 1), (2, 3, 2), (2, 4, 1)],
            owners: Vec::new(),
            people: vec![0],
            water: Vec::new(),
            food: Vec::new(),
            products: Vec::new(),
            larder: Vec::new(),
            tool: None,
            pay: Vec::new(),
            tools: Vec::new(),
            wages: Vec::new(),
            fillable: Vec::new(),
            barren: Vec::new(),
        }
    }
}

/// What a group is carrying, for the tests that do not care about the detail.
fn carrying(food: usize, payment: bool) -> burngfx::rival::Carried {
    burngfx::rival::Carried { food, payment, tool: true }
}

impl Ground for World {
    fn roads(&self, location: usize) -> Vec<(usize, u8)> {
        let mut out = Vec::new();
        for &(a, b, days) in &self.roads {
            if a == location {
                out.push((b, days));
            }
            if b == location {
                out.push((a, days));
            }
        }
        out
    }
    fn is_city(&self, location: usize) -> bool {
        self.cities.contains(&location)
    }
    fn owner(&self, location: usize) -> Option<usize> {
        self.owners.iter().find(|(l, _)| *l == location).map(|(_, p)| *p)
    }
    fn hireable(&self, location: usize, _player: usize) -> Option<usize> {
        // Somebody stands in these locations, and takes whatever is in slot 0.
        self.people.contains(&location).then_some(0)
    }
    fn water_here(&self, location: usize) -> i32 {
        self.water.iter().find(|(l, _)| *l == location).map(|(_, w)| *w).unwrap_or(0)
    }
    fn food_here(&self, location: usize) -> Option<usize> {
        self.food.iter().find(|(l, _)| *l == location).map(|(_, what)| *what)
    }
    fn food_product(&self, location: usize) -> Option<usize> {
        self.products.iter().find(|(l, _)| *l == location).map(|(_, slot)| *slot)
    }
    fn my_larder(&self) -> Vec<usize> {
        self.larder.clone()
    }
    fn payment_here(&self, location: usize) -> Option<usize> {
        self.pay.iter().find(|(l, _)| *l == location).map(|(_, what)| *what)
    }
    fn tool_here(&self, location: usize) -> Option<usize> {
        self.tools.iter().find(|(l, _)| *l == location).map(|(_, what)| *what)
    }
    fn my_wages(&self) -> Vec<usize> {
        self.wages.clone()
    }
    fn can_keep(&self, location: usize) -> bool {
        !self.barren.contains(&location)
    }
    fn fillable_here(&self, location: usize) -> Option<usize> {
        self.fillable.iter().find(|(l, _)| *l == location).map(|(_, slot)| *slot)
    }
    fn my_camps(&self) -> Vec<usize> {
        self.owners.iter().filter(|(_, p)| *p == 1).map(|(l, _)| *l).collect()
    }
    fn camp_tool(&self, _location: usize) -> Option<usize> {
        self.tool
    }
}

fn settings(interval: (i32, i32), advance: i32) -> GameSettings {
    GameSettings {
        ai_camp_interval: interval,
        ai_camp_max_advance: advance,
        ..GameSettings::default()
    }
}

fn thirsty(water: i32, food: i32) -> CharacterNeeds {
    CharacterNeeds { food, max_food: 9, water, max_water: 5, health: 100 }
}

// -- the pace comes from the file, not from here ------------------------------

/// `ai_camp_interval` and `ai_camp_max_advance` are read out of the profile
/// the game is being played at.
#[test]
fn the_pace_is_read_from_the_settings() {
    let text = "\
[0] ' difficulty easy
ai_camp_interval      = 3 6 ' creates every 3 - 6 turns
ai_camp_max_advance   = 1 ' creates not more camps than human player has
[2] ' difficulty hard
ai_camp_interval      = 0 3
ai_camp_max_advance   = 99999
";
    let easy = GameSettings::parse(text, 0);
    assert_eq!(easy.ai_camp_interval, (3, 6));
    assert_eq!(easy.ai_camp_max_advance, 1);

    let hard = GameSettings::parse(text, 2);
    assert_eq!(hard.ai_camp_interval, (0, 3));
    assert_eq!(hard.ai_camp_max_advance, 99999);
}

/// **A file with no `ai_camp_*` line keeps v0.9.1's pace.**
///
/// Current upstream's two settings files have no such line — the opponent's
/// pace moved out of the settings when its behaviour was rebuilt — and
/// reading that absence as zero would be an opponent that waits no turns and
/// never expands. The absence is not a decision; this is.
#[test]
fn settings_without_the_line_keep_the_pinned_pace() {
    let without = GameSettings::parse("[0] ' difficulty easy\nstart_experience = 37\n", 0);
    assert_eq!(without.ai_camp_interval, burngfx::settings::CLASSIC_CAMP_INTERVAL);
    assert_eq!(without.ai_camp_max_advance, burngfx::settings::CLASSIC_CAMP_MAX_ADVANCE);
    assert_eq!(without.start_experience, 37, "and the rest of the file still reads");

    // A file that does carry it is read, not overridden.
    let with = GameSettings::parse("[0]\nai_camp_interval = 0 3\nai_camp_max_advance = 99999\n", 0);
    assert_eq!(with.ai_camp_interval, (0, 3));
    assert_eq!(with.ai_camp_max_advance, 99999);
}

/// `Random.Next(min, max)` is half open, so `3 6` never gives six.
#[test]
fn the_interval_is_drawn_between_the_two_numbers() {
    let s = settings((3, 6), 1);
    let mut rng = Rng::seeded(1993);
    for _ in 0..50 {
        let drawn = draw_interval(&s, &mut rng);
        assert!((3..6).contains(&drawn), "drew {drawn}");
    }
    // A degenerate line gives the minimum rather than dividing by zero.
    assert_eq!(draw_interval(&settings((4, 4), 1), &mut rng), 4);
}

/// **The expansion limit is the human's count plus the allowance.** At
/// `max_advance = 1` a machine player holding as many as the best human may
/// take one more and no further.
#[test]
fn expansion_stops_at_the_humans_count_plus_the_allowance() {
    let s = settings((3, 6), 1);
    assert!(may_expand(&s, 0, 0), "nobody holds anything yet");
    assert!(may_expand(&s, 1, 1));
    assert!(!may_expand(&s, 2, 1), "one ahead is the limit");
    // The hardest profile's 99999 is the data's way of saying "no limit".
    assert!(may_expand(&settings((0, 3), 99999), 40, 0));
}

// -- supply, and no gifts -----------------------------------------------------

/// **It drinks from a spring the world actually has**, and only when thirsty.
#[test]
fn it_drinks_where_there_is_water() {
    let mut world = World::small();
    world.water = vec![(2, 5)];
    let mut rival = Rival::new(1, 2);
    rival.needs = thirsty(1, 9);
    assert_eq!(supply(&rival, &world, carrying(1, true)), vec![Act::Drink(2)]);

    // Full, and it leaves the spring alone.
    rival.needs = thirsty(5, 9);
    assert!(supply(&rival, &world, carrying(1, true)).is_empty());
}

/// **It eats what it carries, through the same day the player's group has.**
///
/// `party_day` is the whole of it: one turn per actor, the best food in the
/// group, and nothing conjured. An opponent with an empty pack simply goes
/// without — where v0.9.1 would have put meat in its hand.
#[test]
fn it_eats_from_its_own_pack_through_the_ordinary_day() {
    let mut rival = Rival::new(1, 1);
    rival.needs = thirsty(5, 0);
    rival.pack = ItemBag::with(PACK_CAPACITY, &[KNIFE, MEAT]);

    burngfx::game::party_day(&mut rival.needs, &mut rival.pack, &mut rival.party, &Table);

    // Four from the meat, less the one the day itself costs.
    assert_eq!(rival.needs.food, 3, "the meat, not the knife");
    assert_eq!(rival.pack.iter().collect::<Vec<_>>(), [KNIFE], "and it is gone");

    // With nothing to eat, the day takes its toll instead of feeding it.
    let mut hungry = Rival::new(1, 1);
    hungry.needs = thirsty(5, 0);
    let before = hungry.needs.health;
    burngfx::game::party_day(&mut hungry.needs, &mut hungry.pack, &mut hungry.party, &Table);
    assert!(hungry.needs.health < before, "an empty pack is an empty pack");
}

/// Being in a city buys it nothing by itself: no free food, no free water, no
/// free health. That is the cheat upstream removed and this never had.
#[test]
fn a_city_is_not_a_larder() {
    let world = World::small();
    let mut rival = Rival::new(1, 0); // standing in a city
    rival.needs = thirsty(0, 0);
    assert!(supply(&rival, &world, carrying(0, true)).is_empty(), "a city with nothing in it gives nothing");
    assert_eq!(rival.needs.water, 0, "and nothing was given");
    assert_eq!(rival.needs.food, 0);
}

// -- where it goes ------------------------------------------------------------

/// A city cannot be camped, and neither can ground somebody else holds.
#[test]
fn what_can_be_camped() {
    let mut world = World::small();
    world.owners = vec![(3, 0)];
    assert!(!can_camp(0, 1, &world), "a city");
    assert!(!can_camp(3, 1, &world), "the other player's");
    assert!(can_camp(2, 1, &world), "nobody's");
    world.owners.push((2, 1));
    assert!(can_camp(2, 1, &world), "and its own is fine");
}

/// The nearest campable place, one road at a time — and the answer is the
/// **first step**, because a journey is one committed road.
#[test]
fn it_walks_towards_the_nearest_free_ground() {
    let world = World::small();
    // Standing in the city 0: 1 is campable and one road away.
    let rival = Rival::new(1, 0);
    assert_eq!(next_step(&rival, &world), Some(1));
}

/// **Somebody else's ground is a dead end.** `Player.CanTravel` lets it go
/// back only the way it came, so a search may not route through their land.
#[test]
fn it_does_not_route_through_enemy_ground() {
    let mut world = World::small();
    // The human holds 1 and 2, the only way out of the city 0.
    world.owners = vec![(1, 0), (2, 0)];
    let rival = Rival::new(1, 0);
    assert_eq!(next_step(&rival, &world), None, "nothing reachable that is free");
}

// -- the turn -----------------------------------------------------------------

/// With nobody to leave in a camp it goes hiring, and it pays from its pack.
#[test]
fn with_nobody_left_it_hires() {
    let world = World::small();
    let s = settings((3, 6), 1);
    let mut rng = Rng::seeded(7);
    let mut rival = Rival::new(1, 0); // in the city, where people stand
    rival.pack = ItemBag::with(PACK_CAPACITY, &[MEAT]);

    let acts = turn(&mut rival, &world, &s, 0, 0, carrying(9, true), &mut rng);

    assert_eq!(acts, vec![Act::Hire { at: 0, pay_with: 0 }]);
    // **The turn decides; it does not act.** The caller hires with
    // `hire::hire`, which is the player's own function, so the group is
    // still empty here.
    assert!(rival.party.is_empty(), "deciding is not doing");
    assert_eq!(rival.mode, Mode::Expanding, "and then it goes looking");
}

/// With somebody to leave and free ground under it, it camps — and that is
/// what claims the place.
#[test]
fn it_camps_where_it_stands_when_it_can() {
    let world = World::small();
    let s = settings((3, 6), 1);
    let mut rng = Rng::seeded(7);
    let mut rival = Rival::new(1, 2);
    rival.party.recruit();
    rival.party.recruit();
    rival.mode = Mode::Expanding;

    let acts = turn(&mut rival, &world, &s, 0, 0, carrying(9, true), &mut rng);

    assert_eq!(acts, vec![Act::Camp { at: 2, tool: None }]);
    // Again: the caller leaves somebody behind with `Party::leave`, so the
    // group is untouched here — and with two of them, one left behind still
    // leaves one, so it goes back to waiting rather than hiring.
    assert_eq!(rival.party.len(), 2, "deciding is not doing");
    assert_eq!(rival.mode, Mode::Waiting, "and then it waits out the interval");
    assert!((3..6).contains(&rival.wait));
}

/// On a city it cannot camp, so it sets out — one road, not a route.
#[test]
fn it_travels_one_road_at_a_time() {
    let world = World::small();
    let s = settings((3, 6), 1);
    let mut rng = Rng::seeded(7);
    let mut rival = Rival::new(1, 0);
    rival.party.recruit();
    rival.mode = Mode::Expanding;

    let acts = turn(&mut rival, &world, &s, 0, 0, carrying(9, true), &mut rng);
    assert_eq!(acts, vec![Act::Travel(1)]);
    assert_eq!(rival.headed, Some(1));
}

/// **A journey is days, and nothing strategic happens during them.** Supply
/// still does: a thirsty traveller drinks when it arrives, not a week later.
#[test]
fn nothing_strategic_happens_on_the_road() {
    let mut world = World::small();
    world.water = vec![(1, 5)];
    let s = settings((3, 6), 1);
    let mut rng = Rng::seeded(7);
    let mut rival = Rival::new(1, 1);
    rival.party.recruit();
    rival.mode = Mode::Expanding;
    rival.needs = thirsty(0, 9);
    rival.journey = Some(Journey { from: 1, to: 2, total: 2, remaining: 1 });

    let acts = turn(&mut rival, &world, &s, 0, 0, carrying(9, true), &mut rng);
    assert_eq!(acts, vec![Act::Drink(1)], "it drinks, and it does not decide");
    assert_eq!(rival.mode, Mode::Expanding, "the decision waits for the arrival");
}

/// A dead opponent does nothing at all.
#[test]
fn the_dead_take_no_turns() {
    let world = World::small();
    let s = settings((3, 6), 1);
    let mut rng = Rng::seeded(7);
    let mut rival = Rival::new(1, 1);
    rival.dead = true;
    for _ in 0..3 { rival.party.recruit(); }
    rival.mode = Mode::Expanding;
    assert!(turn(&mut rival, &world, &s, 0, 0, carrying(9, true), &mut rng).is_empty());
    assert_eq!(rival.party.len(), 3, "and nothing moved");
}

/// **The whole point, as a sequence**: from an empty group in a city, a
/// machine player hires, walks out, and plants a camp — with no gifts, on a
/// world with one spring and one person in it.
#[test]
fn it_gets_from_an_empty_group_to_a_camp_on_its_own() {
    let mut world = World::small();
    world.water = vec![(1, 5)];
    let s = settings((3, 6), 1);
    let mut rng = Rng::seeded(1993);
    let mut rival = Rival::new(1, 0);
    rival.pack = ItemBag::with(PACK_CAPACITY, &[MEAT]);

    let mut camped = None;
    for _ in 0..12 {
        let acts = turn(&mut rival, &world, &s, 0, 0, carrying(9, true), &mut rng);
        for act in acts {
            match act {
                // The caller hires with `hire::hire`; here the part that
                // matters is that it pays out of its own pack and gains
                // somebody.
                Act::Hire { pay_with, .. } => {
                    rival.pack.remove(pay_with);
                    rival.party.recruit();
                }
                // The caller moves it; here the journey is one day for
                // every road in this world.
                Act::Travel(to) => {
                    rival.at = to;
                }
                Act::Camp { at: where_, .. } => {
                    rival.party.companions.pop();
                    camped = Some(where_);
                }
                // Nothing lies about in this world and nothing grows, so it
                // never asks for either.
                Act::Take { .. } => unreachable!("nothing to pick up here"),
                Act::Produce { .. } => unreachable!("nothing grows here"),
                Act::Fill { .. } => unreachable!("no container to fill here"),
                Act::Drink(_) => rival.needs.water = rival.needs.max_water,
            }
        }
        if camped.is_some() {
            break;
        }
    }

    assert_eq!(camped, Some(1), "it hired in the city and camped on the first free ground");
    assert!(rival.pack.is_empty(), "and it paid for the hire out of its own pack");
}

// -- wages, and the second camp -----------------------------------------------

/// **The short road to wages is the one everybody has.** A mercenary takes a
/// snake, some meat or a bottle of water; the opponent starts with an empty
/// bottle, and a spring turns it into the third of those. Without this it had
/// food it could not pay anybody with — a knife sets a camp making maggots,
/// and maggots are supper, not wages.
#[test]
fn it_fills_its_bottle_where_there_is_water() {
    let mut world = World::small();
    world.water = vec![(1, 5)];
    world.fillable = vec![(1, 2)]; // slot 2 of its pack is the empty bottle
    let mut rival = Rival::new(1, 1);
    rival.needs = thirsty(5, 9);

    // Carrying nothing to pay with, it fills.
    let acts = supply(&rival, &world, carrying(9, false));
    assert!(acts.contains(&Act::Fill { at: 1, slot: 2 }), "got {acts:?}");

    // Carrying wages already, it leaves the spring alone.
    let acts = supply(&rival, &world, carrying(9, true));
    assert!(!acts.iter().any(|a| matches!(a, Act::Fill { .. })));

    // And where there is no container to fill, nothing happens.
    world.fillable.clear();
    assert!(!supply(&rival, &world, carrying(9, false))
        .iter()
        .any(|a| matches!(a, Act::Fill { .. })));
}

/// **A second man in the same camp is not a second camp.** Camping on your own
/// ground is how a camp grows, and the victory condition counts places — an
/// opponent that put every man it hired into the first camp would hold one
/// place for ever. It did: two hires, both left at Stove Vent.
#[test]
fn expanding_means_a_place_that_is_not_already_its_own() {
    let mut world = World::small();
    world.owners = vec![(1, 1)]; // it already holds where it stands
    let s = settings((3, 6), 1);
    let mut rng = burngfx::spawn::Rng::seeded(4);
    let mut rival = Rival::new(1, 1);
    rival.mode = Mode::Expanding;
    rival.party.recruit();

    let acts = turn(&mut rival, &world, &s, 1, 1, carrying(9, true), &mut rng);
    assert!(
        !acts.iter().any(|a| matches!(a, Act::Camp { .. })),
        "it camped again where it already holds: {acts:?}"
    );
    assert!(acts.iter().any(|a| matches!(a, Act::Travel(_))), "it should move on: {acts:?}");
}

/// **It does not leave a man where he cannot keep himself.** A camp that can
/// be set to make something feeds one man for ever and a spring waters him;
/// somewhere with neither is a countdown with a flag on it, and holding
/// ground that kills the man holding it is not holding ground.
#[test]
fn it_will_not_leave_a_man_somewhere_barren() {
    let mut world = World::small();
    world.barren = vec![1]; // where it is standing
    let s = settings((3, 6), 1);
    let mut rng = burngfx::spawn::Rng::seeded(11);
    let mut rival = Rival::new(1, 1);
    rival.mode = Mode::Expanding;
    rival.party.recruit();

    let acts = turn(&mut rival, &world, &s, 0, 1, carrying(9, true), &mut rng);
    assert!(
        !acts.iter().any(|a| matches!(a, Act::Camp { .. })),
        "it left a man somewhere that cannot keep him: {acts:?}"
    );

    // Somewhere that can keep him, it camps.
    world.barren.clear();
    let mut rival = Rival::new(1, 1);
    rival.mode = Mode::Expanding;
    rival.party.recruit();
    let acts = turn(&mut rival, &world, &s, 0, 1, carrying(9, true), &mut rng);
    assert!(acts.iter().any(|a| matches!(a, Act::Camp { .. })), "got {acts:?}");
}

/// `GetInts("start_locations")` per difficulty, and `start_locations_ai`
/// is not what anybody starts from.
#[test]
fn every_difficulty_has_its_own_start_locations() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/gamesettings.txt");
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let easy = burngfx::settings::GameSettings::parse(&text, 0);
    let normal = burngfx::settings::GameSettings::parse(&text, 1);
    let hard = burngfx::settings::GameSettings::parse(&text, 2);
    assert_eq!(easy.start_locations, [3, 10, 11, 17, 24, 29, 31, 32, 35]);
    assert_eq!(normal.start_locations, [9, 12, 15, 23, 25, 26, 28, 30, 34, 36]);
    assert_eq!(hard.start_locations, [2, 6, 14, 16, 19, 34]);
}
