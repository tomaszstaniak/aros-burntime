//! What a camp makes, and what has to be true before it makes anything.
//!
//! The finding this file is built around: **a location produces nothing
//! without an owner, and the only thing that gives it one is somebody camped
//! in it.** `GetFoodProductionRate` returns an empty rate when `Player is
//! null`; `Location.Player` is written in exactly two places, `JoinCamp` and
//! `LeaveCamp`. Arriving somewhere does not claim it, and neither does
//! leaving a knife on the floor.

use burngfx::game::{production_turn, Camp, Camps, Engine, Storage, MAX_STOCK_FOOD};
use burngfx::gamdat::{self, Gamdat};
use burngfx::items::ItemTypes;
use burngfx::production::Productions;
use std::path::PathBuf;

fn testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(rel)
}

fn productions() -> Option<Productions> {
    Some(Productions::parse(&std::fs::read_to_string(testdata("production.txt")).ok()?))
}

fn types() -> Option<ItemTypes> {
    Some(ItemTypes::parse(&std::fs::read_to_string(testdata("items_original.txt")).ok()?))
}

fn save() -> Option<Gamdat> {
    let bytes = std::fs::read(testdata("burntime.pak")).ok()?;
    let pak = burngfx::pak::ClassicPak::open(&bytes).ok()?;
    gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()
}

/// A location's rooms and its ground, as much as production needs.
struct Room {
    items: Vec<usize>,
    capacity: usize,
}

struct Place {
    rooms: Vec<Room>,
    ground: Vec<usize>,
    tools: usize,
}

impl Place {
    fn with_rooms(rooms: &[usize], capacity: usize, tools: usize) -> Place {
        Place {
            rooms: rooms.iter().map(|&n| Room { items: vec![9; n], capacity }).collect(),
            ground: Vec::new(),
            tools,
        }
    }

    fn holding(&self, kind: usize) -> usize {
        self.rooms.iter().map(|r| r.items.iter().filter(|&&k| k == kind).count()).sum()
    }
}

impl Storage for Place {
    fn count_in_rooms(&self, _: usize, kind: usize) -> usize {
        self.holding(kind)
    }

    fn tools(&self, _: usize, _: usize) -> usize {
        self.tools
    }

    fn any_room_has_space(&self, _: usize) -> bool {
        self.rooms.iter().any(|r| r.items.len() < r.capacity)
    }

    fn store(&mut self, _: usize, kind: usize) -> bool {
        // `StoreItem` without `randomRoom`: the first room with space.
        if let Some(room) = self.rooms.iter_mut().find(|r| r.items.len() < r.capacity) {
            room.items.push(kind);
            return true;
        }
        self.ground.push(kind);
        true
    }

    /// No pump in these tests; the spring's own rate is the subject.
    fn pump_at(&self, _: usize) -> burngfx::game::Pump {
        burngfx::game::Pump::None
    }
}

/// A knife, as an item rather than as a production index.
const KNIFE_ITEM: usize = 200;

/// Somebody camped, with a pack of their own.
fn resident(needs: CharacterNeeds) -> Companion {
    Companion { needs, ..Companion::new(0) }
}

/// The knife, which is what every non-city location starts set to.
const KNIFE: usize = 0;
/// Maggots are food 3, so a surplus of one is an item every three days.
const MAGGOT_FOOD: i32 = 3;
const MAGGOTS: usize = 100;

/// The only item these tests need to know about.
struct Larder;

impl burngfx::game::ItemFacts for Larder {
    fn food(&self, kind: usize) -> i32 {
        if kind == MAGGOTS {
            MAGGOT_FOOD
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
        (name == "item_maggots").then_some(MAGGOTS)
    }

    fn name_of(&self, kind: usize) -> Option<&str> {
        match kind {
            MAGGOTS => Some("item_maggots"),
            KNIFE_ITEM => Some("item_knife"),
            _ => None,
        }
    }
}

// -- the data ----------------------------------------------------------------

#[test]
fn the_shipped_productions_read_as_four_tools() {
    let Some(p) = productions() else { return };
    assert_eq!(p.len(), 4);

    let knife = p.get(0).unwrap();
    assert_eq!(knife.tool, "item_knife");
    assert_eq!(knife.produce, "item_maggots");
    assert_eq!(knife.max_combination, 1);
    assert_eq!(knife.per_day, [1, 2]);
    assert_eq!(knife.per_day_two, [1, 3]);
    assert!(knife.worked_by("item_axe") && knife.worked_by("item_pitchfork"));
    assert!(!knife.worked_by("item_rat_trap"));

    // A production with no `amount2` uses `amount` for a crowd as well.
    let rats = p.get(1).unwrap();
    assert_eq!(rats.tool, "item_rat_trap");
    assert_eq!(rats.per_day, [0, 3, 4]);
    assert_eq!(rats.per_day_two, rats.per_day, "amount2 is empty, so it is amount");
    assert_eq!(rats.max_combination, 2);

    assert_eq!(p.worked_by("item_pitchfork"), Some(0));
    assert_eq!(p.worked_by("item_snake_trap"), Some(2));
    assert_eq!(p.worked_by("item_bottle"), None);
}

/// Everything each production makes is a real item, and food.
#[test]
fn every_production_makes_something_edible() {
    let (Some(p), Some(types)) = (productions(), types()) else { return };
    for production in p.iter() {
        let kind = types.index_of(&production.produce).unwrap_or_else(|| {
            panic!("{} produces {}, which is not an item", production.tool, production.produce)
        });
        assert!(types.food_of(kind) > 0, "{} is not food", production.produce);
        assert!(types.index_of(&production.tool).is_some(), "{} is not an item", production.tool);
    }
}

/// The 1993 save already says what each location is set to make, and who
/// holds it — nobody, at the start.
#[test]
fn the_save_says_what_every_camp_is_set_to_make() {
    let Some(gam) = save() else { return };
    let camps = Camps::from_gamdat(&gam);
    assert_eq!(camps.len(), gam.locations.len());

    let (mut cities, mut producing, mut barren) = (0, 0, 0);
    for (i, loc) in gam.locations.iter().enumerate() {
        let camp = camps.get(i).unwrap();
        assert_eq!(camp.owner, None, "location {i} starts owned by somebody");
        assert!(camp.residents.is_empty());

        if loc.is_city {
            cities += 1;
            assert_eq!(camp.production, None, "city {i} is set to produce something");
        } else if camp.production.is_some() {
            // Where a location produces at all it starts on production 0, the
            // knife: `Producing` is 51 in the file, and 51 is index zero.
            assert_eq!(camp.production, Some(KNIFE), "location {i}");
            producing += 1;
        } else {
            // Three places outside the cities produce nothing and offer
            // nothing — 22, 25 and 27. Not a parsing accident: a location
            // with no production has no available list either.
            assert!(
                loc.available_products.iter().all(|p| p.is_none()),
                "location {i} offers productions but is set to none"
            );
            barren += 1;
        }

        // The available list is four slots, an index or nothing in each.
        for (slot, available) in loc.available_products.iter().enumerate() {
            assert!(available.is_none() || *available == Some(slot));
        }
    }
    assert_eq!(cities, 5);
    assert_eq!(barren, 3, "three places outside the cities make nothing");
    assert!(producing > 25, "only {producing} locations are set to produce");

    // Somewhere has more than one production to choose between, or the
    // switching rule would be unreachable.
    let choices = gam
        .locations
        .iter()
        .map(|l| l.available_products.iter().flatten().count())
        .max()
        .unwrap_or(0);
    assert!(choices >= 2, "no location offers a choice of production");
}

// -- what has to be true -----------------------------------------------------

/// No owner, no production — however many knives are lying about.
#[test]
fn a_location_nobody_holds_makes_nothing() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 4);
    let mut camp = Camp { owner: None, residents: Vec::new(), production: Some(KNIFE), progress: 0.0, stationed: Vec::new(), manual_production: false };

    for _ in 0..20 {
        let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
        assert_eq!(out.item, None);
        assert_eq!(out.food_per_day, 0);
    }
    assert_eq!(place.holding(MAGGOTS), 0);
    assert_eq!(camp.progress, 0.0, "progress accrued without an owner");
}

/// An owner but nothing selected is the same nothing.
#[test]
fn a_camp_with_no_production_selected_makes_nothing() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 4);
    let mut camp = Camp { owner: Some(0), residents: Vec::new(), production: None, progress: 0.0, stationed: Vec::new(), manual_production: false };

    let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
    assert_eq!(out.item, None);
    assert_eq!(out.food_per_day, 0);
}

// -- the rate ----------------------------------------------------------------

/// Tools raise the yield up to the combination limit and no further; a second
/// person changes which table is read.
#[test]
fn the_rate_is_the_table_the_tools_and_the_people() {
    let Some(p) = productions() else { return };
    let knife = p.get(KNIFE).unwrap();

    // One person: `amount`. No tools is still one a day.
    assert_eq!(knife.rate(0, 1, MAGGOT_FOOD).food_per_day, 1);
    assert_eq!(knife.rate(1, 1, MAGGOT_FOOD).food_per_day, 2);
    assert_eq!(knife.rate(5, 1, MAGGOT_FOOD).food_per_day, 2, "maxcombination is 1");

    // Two or more: `amount2`.
    assert_eq!(knife.rate(1, 2, MAGGOT_FOOD).food_per_day, 3);
    assert_eq!(knife.rate(0, 2, MAGGOT_FOOD).food_per_day, 1);

    // A trap needs a trap: with none it yields nothing at all.
    let rats = p.get(1).unwrap();
    assert_eq!(rats.rate(0, 1, 5).food_per_day, 0);
    assert_eq!(rats.rate(2, 1, 5).food_per_day, 4);
}

/// The people eat first, and only what is left over becomes items.
#[test]
fn the_camp_eats_before_anything_is_dropped() {
    let Some(p) = productions() else { return };
    let knife = p.get(KNIFE).unwrap();

    // Three a day, two mouths: a surplus of one, so one maggot every three
    // days — the item's own food value divided by the surplus.
    let plenty = knife.rate(1, 2, MAGGOT_FOOD);
    assert_eq!(plenty.food_per_day, 3);
    assert_eq!(plenty.item_drop_interval, 3.0);
    assert!(!plenty.camp_starving);

    // Exactly enough: nothing dropped, and not starving either.
    let exact = knife.rate(0, 1, MAGGOT_FOOD);
    assert_eq!(exact.food_per_day, 1);
    assert_eq!(exact.item_drop_interval, 0.0);
    assert!(!exact.camp_starving);

    // Not enough.
    let hungry = knife.rate(0, 3, MAGGOT_FOOD);
    assert_eq!(hungry.food_per_day, 1);
    assert!(hungry.camp_starving);
    assert_eq!(hungry.item_drop_interval, 0.0);
}

// -- days passing ------------------------------------------------------------

/// The criterion, in one test: an owned camp with a tool, days passing, a
/// product in the right place, the stock limit, and it starting again after
/// something is taken.
#[test]
fn an_owned_camp_fills_up_stops_at_six_and_starts_again() {
    let Some(p) = productions() else { return };
    // One room, and one knife. Nobody camped, which is scaffolding: the
    // original cannot own a location with an empty camp. With no mouths the
    // whole two a day is surplus, so a maggot every day and a half.
    let mut place = Place::with_rooms(&[0], 32, 1);
    let mut camp = Camp { owner: Some(0), residents: Vec::new(), production: Some(KNIFE), progress: 0.0, stationed: Vec::new(), manual_production: false };

    let mut made = 0;
    let mut days = 0;
    while made < MAX_STOCK_FOOD {
        let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
        days += 1;
        if out.item.is_some() {
            made += 1;
        }
        assert!(days < 40, "nothing produced in {days} days");
    }
    assert_eq!(place.holding(MAGGOTS), MAX_STOCK_FOOD);
    // Two a day against nothing eaten is a surplus of two: 3 / 2 days each.
    assert_eq!(days, 9);

    // Full: the day does nothing at all, progress included.
    let before = camp.progress;
    for _ in 0..10 {
        let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
        assert_eq!(out.item, None, "produced past the limit");
    }
    assert_eq!(camp.progress, before, "progress accrued while the store was full");
    assert_eq!(place.holding(MAGGOTS), MAX_STOCK_FOOD);

    // Take one away and it starts again — from where it stopped.
    place.rooms[0].items.pop();
    let mut resumed = 0;
    for _ in 0..3 {
        if production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091).item.is_some() {
            resumed += 1;
        }
    }
    assert_eq!(resumed, 1, "the camp did not start again");
    assert_eq!(place.holding(MAGGOTS), MAX_STOCK_FOOD);
}

/// The remainder is kept, not thrown away: an interval of one and a half
/// gives two items in three days, not one.
#[test]
fn progress_keeps_its_remainder() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 1);
    let mut camp = Camp { owner: Some(0), residents: Vec::new(), production: Some(KNIFE), progress: 0.0, stationed: Vec::new(), manual_production: false };

    let made: usize = (0..3)
        .filter(|_| {
            production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091).item.is_some()
        })
        .count();
    assert_eq!(made, 2, "the half day was dropped");
}

/// A full location: v0.9.1 puts the item outside, current upstream stops
/// making progress at all. Both are the release's own answer.
#[test]
fn the_two_releases_differ_when_there_is_no_room_indoors() {
    let Some(p) = productions() else { return };

    // A single room with one slot, already holding something that is not the
    // produced kind — so the stock test passes and the room is still full.
    let full = || Place { rooms: vec![Room { items: vec![9], capacity: 1 }], ground: Vec::new(), tools: 1 };

    let mut older = full();
    let mut camp = Camp { owner: Some(0), residents: Vec::new(), production: Some(KNIFE), progress: 0.0, stationed: Vec::new(), manual_production: false };
    let mut dropped = 0;
    for _ in 0..6 {
        if production_turn(&mut camp, 0, &p, &Larder, &mut older, Engine::V091).item.is_some() {
            dropped += 1;
        }
    }
    assert!(dropped > 0, "v0.9.1 should keep producing and put it outside");
    assert_eq!(older.ground.len(), dropped, "and outside is where it went");

    let mut newer = full();
    let mut camp = Camp { owner: Some(0), residents: Vec::new(), production: Some(KNIFE), progress: 0.0, stationed: Vec::new(), manual_production: false };
    for _ in 0..6 {
        assert_eq!(
            production_turn(&mut camp, 0, &p, &Larder, &mut newer, Engine::Current).item,
            None,
            "current upstream should not produce into a full location"
        );
    }
    assert_eq!(camp.progress, 0.0, "and should not bank the days either");
    assert!(newer.ground.is_empty());
}

// -- with somebody living there ---------------------------------------------

use burngfx::game::{camp_turn, CharacterNeeds, MAX_FOOD};

/// A camp that makes exactly what it eats: the person stays fed, and nothing
/// is ever left over to pick up.
///
/// One person and no tools is a knife's `amount[0]`, which is one a day
/// against one mouth. Not starving, and not producing — the two are different
/// answers and the rate says so.
#[test]
fn a_camp_that_feeds_itself_leaves_nothing_behind() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 0);
    let mut camp = Camp {
        owner: Some(0),
        residents: vec![resident(CharacterNeeds { food: 4, ..CharacterNeeds::at_game_start() })],
        production: Some(KNIFE),
        progress: 0.0,
        stationed: Vec::new(),
        manual_production: false,
    };

    // A spring to drink from, or they die of thirst in five days and the
    // test is about food. The camped drink from the location's own source,
    // which a traveller does not.
    let mut spring = burngfx::game::WaterSource::classic(9, 9, 9);

    for _ in 0..10 {
        let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
        assert_eq!(out.food_per_day, 1);
        assert!(!out.starving);
        assert_eq!(out.item, None, "a self-sufficient camp produced something");

        let residents = &mut camp.residents;
        let left = camp_turn(residents, out.food_per_day, Some(&mut spring), &Larder);
        assert_eq!(left, 0, "the day's food was not eaten");
        spring.reserve = 9;
    }

    // Fed every day: one point in, one point out, so they hold their own.
    assert_eq!(camp.residents[0].needs.food, 4, "the resident did not stay level");
    assert_eq!(place.holding(MAGGOTS), 0);
}

/// A camp with a tool feeds its person **and** leaves something over.
///
/// The person is fed from `NPCFoodProduction` before anything else — that is
/// a different mechanism from taking the mouths off the surplus, and this is
/// the test that tells them apart: the resident's food goes up while items
/// appear.
#[test]
fn a_camp_with_a_tool_feeds_its_people_and_stocks_the_room() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 1);
    let mut camp = Camp {
        owner: Some(0),
        // Hungry, so that being fed is visible.
        residents: vec![resident(CharacterNeeds { food: 2, ..CharacterNeeds::at_game_start() })],
        production: Some(KNIFE),
        progress: 0.0,
        stationed: Vec::new(),
        manual_production: false,
    };
    let mut spring = burngfx::game::WaterSource::classic(9, 9, 9);

    let mut made = 0;
    for _ in 0..9 {
        let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
        assert_eq!(out.food_per_day, 2, "one knife, one person");
        assert!(!out.starving);
        if out.item.is_some() {
            made += 1;
        }
        let residents = &mut camp.residents;
        let left = camp_turn(residents, out.food_per_day, Some(&mut spring), &Larder);
        assert_eq!(left, 1, "one to spare");
        spring.reserve = 9;
    }

    // Two a day, one mouth: a surplus of one, so a maggot every three days.
    assert_eq!(made, 3);
    assert_eq!(place.holding(MAGGOTS), 3);
    // And the resident ate one a day out of the two, so they are no worse off.
    assert_eq!(camp.residents[0].needs.food, 2);
}

/// A camp that cannot feed itself is starving, and its person runs down.
#[test]
fn a_camp_that_cannot_feed_itself_starves() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 0);
    let mut camp = Camp {
        owner: Some(0),
        // Three mouths against a knife's one a day with no tools.
        residents: (0..3).map(|_| resident(CharacterNeeds::at_game_start())).collect(),
        production: Some(KNIFE),
        progress: 0.0,
        stationed: Vec::new(),
        manual_production: false,
    };

    let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
    assert_eq!(out.food_per_day, 1);
    assert!(out.starving, "three people on one a day is not starving?");
    assert_eq!(out.item, None);

    let residents = &mut camp.residents;
    camp_turn(residents, out.food_per_day, None, &Larder);
    // Everyone loses the day's point. The first was offered the day's single
    // ration and was already full, so it did them no good — being fed and
    // having room to eat are not the same thing.
    for who in &camp.residents {
        assert_eq!(who.needs.food, MAX_FOOD - 1);
    }
}

// -- claiming a place and giving it up ---------------------------------------

use burngfx::game::{Companion, Party};

/// The whole of ownership, in one test.
///
/// A location is claimed by leaving somebody in it and given up by taking the
/// last of them away — `JoinCamp` and `LeaveCamp`, the only two writers of
/// `Location.Player` in the tree. Everything else follows: an unclaimed place
/// produces nothing however many tools are lying in it.
#[test]
fn a_place_is_claimed_by_camping_in_it_and_given_up_by_leaving() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 1);
    let mut camps = Camps::new();
    camps.join(7, 0, Companion::new(0));
    // Undo that: this test starts from nobody holding it.
    if let Some(camp) = camps.at(7) {
        camp.owner = None;
        camp.residents.clear();
        camp.production = Some(KNIFE);
    }

    let mut party = Party::new();
    party.recruit();

    // Nobody there: a knife in the room buys nothing.
    for _ in 0..6 {
        let camp = camps.at(7).unwrap();
        assert_eq!(
            production_turn(camp, 7, &p, &Larder, &mut place, Engine::V091).item,
            None,
            "an unclaimed place produced"
        );
    }
    assert_eq!(place.holding(MAGGOTS), 0);

    // Leave somebody: the place is yours, and it starts working.
    assert!(party.leave_one(&mut camps, 7, 0));
    assert!(party.is_empty());
    assert_eq!(camps.get(7).unwrap().owner, Some(0));
    assert_eq!(camps.get(7).unwrap().residents.len(), 1);

    let mut made = 0;
    for _ in 0..6 {
        let camp = camps.at(7).unwrap();
        if production_turn(camp, 7, &p, &Larder, &mut place, Engine::V091).item.is_some() {
            made += 1;
        }
    }
    assert_eq!(made, 2, "two a day against one mouth is a maggot every three days");

    // Take them back: nobody holds it, and it stops.
    assert!(party.take_one_back(&mut camps, 7, 0));
    assert_eq!(party.len(), 1);
    assert_eq!(camps.get(7).unwrap().owner, None, "the last one out kept the place");
    assert!(camps.get(7).unwrap().residents.is_empty());

    let before = place.holding(MAGGOTS);
    for _ in 0..12 {
        let camp = camps.at(7).unwrap();
        assert_eq!(
            production_turn(camp, 7, &p, &Larder, &mut place, Engine::V091).item,
            None,
            "a place nobody holds went on producing"
        );
    }
    assert_eq!(place.holding(MAGGOTS), before, "and made nothing");

    // Nothing to take from an empty camp, and nobody to leave twice.
    assert!(!party.take_one_back(&mut camps, 7, 0));
    assert!(party.leave_one(&mut camps, 7, 0));
    assert!(!party.leave_one(&mut camps, 7, 0), "left somebody who was not there");
}

/// Several people camped: the place stays yours until the last one goes.
#[test]
fn a_place_stays_yours_while_anybody_is_still_there() {
    let mut camps = Camps::new();
    camps.join(3, 0, Companion::new(0));
    let mut party = Party::new();
    party.recruit();
    party.recruit();

    assert!(party.leave_one(&mut camps, 3, 0));
    assert!(party.leave_one(&mut camps, 3, 0));
    assert_eq!(camps.get(3).unwrap().residents.len(), 3);

    for expected in [2, 1] {
        assert!(party.take_one_back(&mut camps, 3, 0));
        assert_eq!(camps.get(3).unwrap().residents.len(), expected);
        assert_eq!(camps.get(3).unwrap().owner, Some(0), "given up too early");
    }
    assert!(party.take_one_back(&mut camps, 3, 0));
    assert_eq!(camps.get(3).unwrap().owner, None);
}

/// The decisive one: the tool is in a companion's pack and nowhere else.
///
/// `GetFoodProductionRate` adds `trapsOnNPCs` to `trapsInRooms`, so a knife
/// carried by the person camped there counts exactly as one lying on the
/// floor — and taking that person away takes the knife with them. Nothing is
/// copied and nothing is left behind: the pack belongs to the character, and
/// the character is what moves.
#[test]
fn a_tool_in_a_companions_pack_counts_and_leaves_with_them() {
    let Some(p) = productions() else { return };
    // The floor holds nothing at all — no tool, and the rooms know it.
    let mut place = Place::with_rooms(&[0], 32, 0);
    let mut camps = Camps::new();
    let mut party = Party::new();

    let id = party.recruit();
    party.companions[0].pack.push(KNIFE_ITEM);
    assert_eq!(place.tools(4, KNIFE), 0, "the floor must hold no tool for this test");

    // Left behind with the knife: the camp works.
    assert!(party.leave_one(&mut camps, 4, 0));
    {
        let camp = camps.at(4).unwrap();
        camp.production = Some(KNIFE);
        assert_eq!(camp.residents[0].id, id, "somebody else was left behind");
        assert_eq!(camp.residents[0].pack.get(0), Some(KNIFE_ITEM), "the knife did not travel");
    }

    let mut made = 0;
    for _ in 0..6 {
        let camp = camps.at(4).unwrap();
        if production_turn(camp, 4, &p, &Larder, &mut place, Engine::V091).item.is_some() {
            made += 1;
        }
    }
    // Two a day against one mouth: a maggot every three days.
    assert_eq!(made, 2, "the carried knife did not count");

    // Somebody camped with no knife makes only what one person needs, and
    // that is the comparison that proves the knife was doing the work.
    let mut bare_camps = Camps::new();
    let mut bare_party = Party::new();
    bare_party.recruit();
    assert!(bare_party.leave_one(&mut bare_camps, 5, 0));
    bare_camps.at(5).unwrap().production = Some(KNIFE);
    let mut bare_place = Place::with_rooms(&[0], 32, 0);
    for _ in 0..6 {
        let camp = bare_camps.at(5).unwrap();
        assert_eq!(
            production_turn(camp, 5, &p, &Larder, &mut bare_place, Engine::V091).item,
            None,
            "a camp with no tool at all produced something"
        );
    }

    // Take them back: the knife goes too, and the camp stops.
    assert!(party.take_one_back(&mut camps, 4, 0));
    assert_eq!(party.companions[0].id, id, "a different person came back");
    assert_eq!(party.companions[0].pack.get(0), Some(KNIFE_ITEM), "the knife stayed behind");
    assert!(camps.get(4).unwrap().residents.is_empty());
    assert_eq!(camps.get(4).unwrap().owner, None);

    let before = place.holding(MAGGOTS);
    for _ in 0..9 {
        let camp = camps.at(4).unwrap();
        assert_eq!(
            production_turn(camp, 4, &p, &Larder, &mut place, Engine::V091).item,
            None,
            "the camp went on without anybody in it"
        );
    }
    assert_eq!(place.holding(MAGGOTS), before, "and made nothing");

    // Nothing duplicated: one knife in the world, in one pack.
    let in_packs: usize = party.companions.iter().map(|c| c.pack.len()).sum();
    let in_camp: usize = camps
        .get(4)
        .map(|c| c.residents.iter().map(|r| r.pack.len()).sum())
        .unwrap_or(0);
    assert_eq!(in_packs + in_camp, 1, "the knife was copied or lost");
}

// -- choosing what a camp makes ---------------------------------------------

use burngfx::game::{
    may_change_production, production_slot, set_production, step_production,
};

/// Who may change a place's production, and where.
///
/// `MapScene.OnClickEntrance` opens the info screen for a place you hold, or
/// for one that is not a city, that you are standing in, and that nobody
/// holds. So a wild place can be set before it is claimed — and somebody
/// else's camp never can.
#[test]
fn only_your_own_camp_or_a_wild_place_you_stand_in() {
    let mine = Camp { owner: Some(0), ..Camp::default() };
    let theirs = Camp { owner: Some(1), ..Camp::default() };
    let nobodys = Camp::default();

    // Yours, wherever you are and whatever it is.
    assert!(may_change_production(&mine, 0, false, false));
    assert!(may_change_production(&mine, 0, true, false), "a city you hold is still yours");

    // Somebody else's, never.
    assert!(!may_change_production(&theirs, 0, false, true));
    assert!(!may_change_production(&theirs, 0, false, false));

    // Unclaimed: only if you are standing there and it is not a city.
    assert!(may_change_production(&nobodys, 0, false, true));
    assert!(!may_change_production(&nobodys, 0, false, false), "not from a distance");
    assert!(!may_change_production(&nobodys, 0, true, true), "not an unclaimed city");
}

/// Stepping through the list, including the two things about it that are
/// easy to get wrong.
#[test]
fn the_choice_steps_forward_and_never_back_to_nothing() {
    let three = [Some(0), Some(1), Some(2), None];

    // From nothing, forward reaches the first slot.
    assert_eq!(step_production(None, &three, true), Some(0));
    assert_eq!(step_production(None, &three, false), None, "nothing is before nothing");

    assert_eq!(step_production(Some(0), &three, true), Some(1));
    assert_eq!(step_production(Some(1), &three, true), Some(2));
    // The fourth slot is empty, so forward stops.
    assert_eq!(step_production(Some(2), &three, true), Some(2));

    assert_eq!(step_production(Some(2), &three, false), Some(1));
    assert_eq!(step_production(Some(1), &three, false), Some(0));
    // **And never back to nothing**: `OnProductionRight` tests `> 0`.
    assert_eq!(step_production(Some(0), &three, false), Some(0));

    // A place that offers nothing offers nothing.
    let none = [None, None, None, None];
    assert_eq!(step_production(None, &none, true), None);

    // A gap stops forward movement dead, because the test is on the very
    // next slot rather than the next filled one. No location in the save has
    // one — checked — but this is what the rule does.
    let gapped = [Some(0), None, Some(2), None];
    assert_eq!(step_production(Some(0), &gapped, true), Some(0), "a hole is a wall");
}

/// The current production is found by its slot, which is how the screen
/// knows where to start.
#[test]
fn the_current_choice_is_a_slot_in_the_available_list() {
    let available = [Some(0), Some(1), None, None];
    assert_eq!(production_slot(&Camp { production: Some(1), ..Camp::default() }, &available), Some(1));
    assert_eq!(production_slot(&Camp { production: None, ..Camp::default() }, &available), None);
    // Set to something this place does not offer: no slot, which is what the
    // original's loop leaves `productionID` at.
    assert_eq!(production_slot(&Camp { production: Some(3), ..Camp::default() }, &available), None);
}

/// **Switching does not reset the progress**, and that is read rather than
/// assumed: `productionState` is written in three places in the whole tree,
/// all inside `Location.Turn`, and the info screen touches none of them.
///
/// So a camp most of the way towards one thing carries that towards the next,
/// and because the interval is the new item's food value over the same
/// surplus, switching to something cheaper can drop one the very next day.
#[test]
fn switching_what_a_camp_makes_keeps_the_progress() {
    let Some(p) = productions() else { return };
    let mut place = Place::with_rooms(&[0], 32, 1);
    let mut camp = Camp {
        owner: Some(0),
        residents: vec![resident(CharacterNeeds::at_game_start())],
        production: Some(KNIFE),
        progress: 0.0,
        stationed: Vec::new(),
        manual_production: false,
    };

    // Two days of a knife: two a day, one mouth, so a maggot every three
    // days — two days in, the progress stands at two.
    for _ in 0..2 {
        production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
    }
    assert_eq!(camp.progress, 2.0);
    assert_eq!(place.holding(MAGGOTS), 0, "nothing yet");

    // Switch. The progress is untouched.
    set_production(&mut camp, Some(1));
    assert_eq!(camp.progress, 2.0, "the progress was reset");
    assert_eq!(camp.production, Some(1));

    // And switching back leaves it where it was as well.
    set_production(&mut camp, Some(KNIFE));
    assert_eq!(camp.progress, 2.0);

    // One more day and the banked progress pays out.
    let out = production_turn(&mut camp, 0, &p, &Larder, &mut place, Engine::V091);
    assert_eq!(out.item, Some(MAGGOTS), "the two banked days were thrown away");
}

/// **Somebody else's camper is not yours to take back.** `LeaveCamp` is a
/// method on one of the active player's own characters, so there is no way to
/// reach an opponent's man with it — and reaching one would empty their camp
/// without a fight and hand you the man as well.
#[test]
fn you_cannot_take_back_a_camper_who_is_not_yours() {
    let mut camps = Camps::new();
    let mut theirs = Party::new();
    theirs.recruit();
    assert!(theirs.leave_one(&mut camps, 5, 1), "player 1 leaves a man at 5");
    assert_eq!(camps.get(5).unwrap().owner, Some(1));

    let mut mine = Party::new();
    assert!(!mine.take_one_back(&mut camps, 5, 0), "not yours to take");
    assert_eq!(mine.len(), 0);
    assert_eq!(camps.get(5).unwrap().owner, Some(1), "and their camp still stands");
    assert_eq!(camps.get(5).unwrap().residents.len(), 1);

    // Their own player may, and the place becomes nobody's.
    assert!(theirs.take_one_back(&mut camps, 5, 1));
    assert_eq!(camps.get(5).unwrap().owner, None);
}

/// A man taken back out of a camp keeps his number, and the party's counter
/// stays past it — otherwise the next person hired could be given it.
#[test]
fn a_man_taken_back_does_not_have_his_number_handed_out_again() {
    let mut camps = Camps::new();
    let mut party = Party::restored(Vec::new(), 1);
    camps.set(
        9,
        burngfx::game::Camp {
            owner: Some(0),
            residents: vec![burngfx::game::Companion::new(6)],
            production: None,
            progress: 0.0,
            stationed: Vec::new(),
        manual_production: false,
        },
    );
    assert!(party.take_one_back(&mut camps, 9, 0));
    assert_eq!(party.companions[0].id, 6);
    assert!(party.next_id() > 6, "the counter cleared him");
    assert_ne!(party.recruit(), 6);
}

/// **A man taken back out of a camp is the man who was in it.** His health is
/// the camp's copy, because the camp is where a camper's numbers live — a
/// defender beaten down to five and then taken into a party must still be at
/// five. Healing somebody by moving them between two representations of
/// themselves is the failure this pins.
#[test]
fn a_camper_keeps_what_the_camp_says_he_has() {
    let mut camps = Camps::new();
    let mut party = Party::new();
    party.recruit();
    assert!(party.leave_one(&mut camps, 6, 0));

    // A fight, as far as this test is concerned: the camp's copy is what
    // takes the blows.
    let camper = &mut camps.at(6).expect("camped").residents[0];
    camper.needs.health = 5;
    camper.needs.food = 2;

    assert!(party.take_one_back(&mut camps, 6, 0));
    assert_eq!(party.companions[0].needs.health, 5, "he came back wounded");
    assert_eq!(party.companions[0].needs.food, 2);
}
