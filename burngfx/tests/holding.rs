//! **Twenty days of holding two camps.**
//!
//! The question this file exists for is not "can an opponent take a second
//! place" — it took one — but "is the man it left there able to live". A flag
//! on a map that a starving man holds is not a holding; it is a countdown.
//!
//! It is asked here rather than in the running game for a reason worth
//! writing down. A scripted game cannot watch twenty days of an opponent
//! without the *player* surviving them too, and a player without a
//! well-supplied camp starves in about twelve: nine days of food in the pack,
//! and what the world lays out is twelve edible things across thirty-seven
//! locations. Driving the player's own upkeep through the panels — leave a
//! tool in a camp, come back for what it made — is a real strategy and was
//! tried at length; as a coordinate script it cost hours and still missed.
//! The world's day is the same code either way, so it is run directly.

use burngfx::game::{
    Camp, Camps, Companion, Engine, ItemBag, ItemFacts, Springs, WaterSource, PACK_CAPACITY,
};

/// A store with the tools that were left in each camp, and room for what the
/// camps make. Only what `production_turn` asks of it.
struct Store {
    /// Tools lying in each location, by location.
    tools: Vec<(usize, usize)>,
    /// What has been made and not taken away, by location.
    made: Vec<(usize, usize)>,
    capacity: usize,
}

impl burngfx::game::Storage for Store {
    fn count_in_rooms(&self, location: usize, _kind: usize) -> usize {
        self.made.iter().filter(|(l, _)| *l == location).count()
    }
    fn tools(&self, location: usize, _production: usize) -> usize {
        self.tools.iter().find(|(l, _)| *l == location).map_or(0, |(_, n)| *n)
    }
    fn any_room_has_space(&self, location: usize) -> bool {
        self.count_in_rooms(location, 0) < self.capacity
    }
    fn store(&mut self, location: usize, kind: usize) -> bool {
        if !self.any_room_has_space(location) {
            return false;
        }
        self.made.push((location, kind));
        true
    }

    /// No pump in these tests; the spring's own rate is the subject.
    fn pump_at(&self, _: usize) -> burngfx::game::Pump {
        burngfx::game::Pump::None
    }
}

const MAGGOTS: usize = 1;

/// One production: a knife makes maggots, and a maggot is three days' food.
struct Table;

impl ItemFacts for Table {
    fn food(&self, kind: usize) -> i32 {
        if kind == MAGGOTS {
            3
        } else {
            0
        }
    }
    fn water(&self, _kind: usize) -> i32 {
        0
    }
    fn full(&self, _kind: usize) -> Option<usize> {
        None
    }
    fn empty(&self, _kind: usize) -> Option<usize> {
        None
    }
    fn kind_named(&self, name: &str) -> Option<usize> {
        (name == "item_maggots").then_some(MAGGOTS)
    }
    fn name_of(&self, kind: usize) -> Option<&str> {
        (kind == MAGGOTS).then_some("item_maggots")
    }
}

fn productions() -> burngfx::production::Productions {
    burngfx::production::Productions::parse(
        // The classic table's own keys and the knife's own numbers: one a
        // day with no tool at all, two with one knife. That first number is
        // the whole of why a camper without a tool does not simply die.
        "[item_knife]\nmaxcombination=1\namount=1 2\namount2=1 3\nproduce=item_maggots\n",
    )
}

fn camp_with(owner: usize, id: usize) -> Camp {
    Camp {
        owner: Some(owner),
        residents: vec![Companion { needs: fresh(), ..Companion::new(id) }],
        production: Some(0),
        progress: 0.0,
        stationed: Vec::new(),
        manual_production: false,
    }
}

fn fresh() -> burngfx::game::CharacterNeeds {
    burngfx::game::CharacterNeeds {
        food: 9,
        max_food: 9,
        water: 5,
        max_water: 5,
        health: 100,
    }
}

/// Run `days` days of the world over these camps, and give back each camp's
/// resident at the end.
fn twenty_days(camps: &mut Camps, store: &mut Store, days: usize, springs: &mut Springs) {
    let productions = productions();
    for _ in 0..days {
        let mut world = burngfx::game::WorldDay {
            springs,
            camps,
            storage: store,
            productions: &productions,
            engine: Engine::V091,
            deaths: Vec::new(),
            respawns: Box::leak(Box::new(burngfx::game::Respawns::new())),
            returned: Vec::new(),
        };
        world.turn(&Table);
    }
}

/// How many days the man in this camp lasts, and what he looks like the day
/// before. Measured rather than assumed: the question is exactly "how long",
/// and a test that asserted a guess would be answering itself.
fn lasts(camps: &mut Camps, store: &mut Store, springs: &mut Springs, location: usize)
    -> (usize, burngfx::game::CharacterNeeds)
{
    let mut last = fresh();
    for day in 1..=40 {
        twenty_days(camps, store, 1, springs);
        match camps.get(location).and_then(|c| c.residents.first()) {
            Some(who) => last = who.needs,
            None => return (day, last),
        }
    }
    (usize::MAX, last)
}

/// **A tool is the difference between subsisting and putting something by —
/// not between living and dying.**
///
/// This was measured because it was assumed wrongly first. The classic
/// table's knife reads `amount=1 2`: one a day *with no tool at all*, two
/// with one knife. One a day is exactly one man's keep, so a camp that is set
/// to make something feeds the man in it for ever, whether or not anybody
/// left him a knife — he simply never builds a reserve, and lives at food
/// nought with nothing to fetch.
///
/// The knife doubles that: he eats his one and the other is stored, so after
/// twenty days the place has six maggots standing in it and his own keep
/// never dipped.
///
/// So the second camp the opponent founds is not a countdown, and saying it
/// was would have been an invention. What it *is* is a place that produces
/// nothing anybody can come back for.
#[test]
fn a_tool_is_the_difference_between_subsisting_and_storing() {
    let mut springs = Springs::new();
    springs.set(1, WaterSource::classic(5, 5, 5));
    let mut camps = Camps::new();
    camps.set(1, camp_with(1, 11));
    let mut store = Store { tools: vec![(1, 1)], made: Vec::new(), capacity: 8 };
    twenty_days(&mut camps, &mut store, 20, &mut springs);

    let with_tool = camps.get(1).expect("still held").residents.first().expect("still alive");
    assert_eq!(with_tool.needs.health, 100, "twenty days and not a scratch");
    assert_eq!(with_tool.needs.food, 8, "fed out of the day's yield, every day");
    assert_eq!(store.made.len(), 6, "and six maggots put by: {:?}", store.made);

    // The same camp, the same spring, no knife.
    let mut springs = Springs::new();
    springs.set(2, WaterSource::classic(5, 5, 5));
    let mut camps = Camps::new();
    camps.set(2, camp_with(1, 12));
    let mut store = Store { tools: Vec::new(), made: Vec::new(), capacity: 8 };
    let (died_on, last) = lasts(&mut camps, &mut store, &mut springs, 2);

    assert_eq!(died_on, usize::MAX, "he did not starve: {last:?}");
    assert_eq!(last.health, 100);
    assert!(store.made.is_empty(), "and nothing was left over: {:?}", store.made);
    assert_eq!(camps.get(2).and_then(|c| c.owner), Some(1), "the place is still theirs");
}

/// **And a camp with nothing and nobody to bring anything runs down.** With
/// the spring taken away as well, the day has nothing to give: this is the
/// shape of an opponent leaving a man somewhere it cannot supply.
#[test]
fn a_dry_camp_with_no_tool_kills_the_man_in_it() {
    let mut springs = Springs::new();
    let mut camps = Camps::new();
    camps.set(3, Camp { production: None, ..camp_with(1, 13) });
    let mut store = Store { tools: Vec::new(), made: Vec::new(), capacity: 8 };

    twenty_days(&mut camps, &mut store, 20, &mut springs);

    // The camp is empty and the location is nobody's: `Dismiss` took the last
    // one out when he died.
    let camp = camps.get(3).expect("the entry stays");
    assert!(camp.residents.is_empty(), "he did not survive it");
    assert_eq!(camp.owner, None, "and the place is nobody's again");
}

/// Somebody sent with food of his own lasts as long as it does, and no
/// longer: nothing about being camped feeds a man who has nothing.
#[test]
fn a_pack_is_worth_exactly_what_is_in_it() {
    let mut springs = Springs::new();
    springs.set(4, WaterSource::classic(5, 5, 5));
    let mut camps = Camps::new();
    let mut camp = Camp { production: None, ..camp_with(1, 14) };
    camp.residents[0].pack = ItemBag::with(PACK_CAPACITY, &[MAGGOTS, MAGGOTS]);
    camps.set(4, camp);
    let mut store = Store { tools: Vec::new(), made: Vec::new(), capacity: 8 };

    // Nine days of his own food, then two maggots at three days each: fifteen
    // days before he is going without at all.
    twenty_days(&mut camps, &mut store, 14, &mut springs);
    assert!(
        camps.get(4).expect("held").residents.first().is_some_and(|w| w.needs.health > 0),
        "fourteen days is inside what he was given"
    );

    twenty_days(&mut camps, &mut store, 10, &mut springs);
    assert!(
        camps.get(4).expect("the entry stays").residents.is_empty(),
        "and twenty-four is past it"
    );
}
