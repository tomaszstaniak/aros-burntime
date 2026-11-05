//! **A leader who came back is one of the place's people.**
//!
//! v0.9.1: `CharacterRespawn.Turn` revives a dead player's leader and
//! `Location.EnterLocation` puts him into `Location.Characters`; nothing ever
//! takes him out again, and `Location.CampNPC` is every character there with
//! `Player != null`, alive or not. So from his first return he counts in
//! `GetFoodProductionRate` and stands in the way of `IsLastInCamp`.

use burngfx::game::{Camp, Camps, Companion, Engine, ItemFacts, Springs};

struct Store;

impl burngfx::game::Storage for Store {
    fn count_in_rooms(&self, _: usize, _: usize) -> usize {
        0
    }
    fn tools(&self, _: usize, _: usize) -> usize {
        0
    }
    fn any_room_has_space(&self, _: usize) -> bool {
        true
    }
    fn store(&mut self, _: usize, _: usize) -> bool {
        true
    }
    fn pump_at(&self, _: usize) -> burngfx::game::Pump {
        burngfx::game::Pump::None
    }
}

const MAGGOTS: usize = 1;

struct Table;

impl ItemFacts for Table {
    fn food(&self, kind: usize) -> i32 {
        if kind == MAGGOTS {
            3
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
        (kind == MAGGOTS).then_some("item_maggots")
    }
}

/// The classic knife: one a day without a tool for one mouth, one for two.
fn productions() -> burngfx::production::Productions {
    burngfx::production::Productions::parse(
        "[item_knife]\nmaxcombination=1\namount=1 2\namount2=1 3\nproduce=item_maggots\n",
    )
}

fn camper(id: usize) -> Companion {
    Companion {
        needs: burngfx::game::CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health: 100 },
        ..Companion::new(id)
    }
}

fn camp(stationed: Vec<usize>) -> Camp {
    Camp { owner: Some(0), residents: vec![camper(7)], production: Some(0), progress: 0.0, stationed, manual_production: false }
}

/// **A mouth in the rate.** One man with no tool is fed exactly (one a day
/// for one mouth); with a leader come back beside him the same camp counts
/// two mouths against what two are given, and is starving.
#[test]
fn a_leader_come_back_is_a_mouth_in_the_production_rate() {
    let productions = productions();
    let alone = burngfx::game::production_turn(&mut camp(Vec::new()), 1, &productions, &Table, &mut Store, Engine::V091);
    assert!(!alone.starving, "one man, one a day: {alone:?}");

    let with_him = burngfx::game::production_turn(&mut camp(vec![2]), 1, &productions, &Table, &mut Store, Engine::V091);
    assert_eq!(with_him.food_per_day, 1, "the two-mouth table's own number");
    assert!(with_him.starving, "and two mouths to it: {with_him:?}");
}

/// **The last ordinary camper is not the last.** `Dismiss` and `LeaveCamp`
/// free the place only when `IsLastInCamp`, and a leader who came back is
/// still there.
#[test]
fn the_last_camper_leaving_does_not_free_a_place_a_leader_came_back_to() {
    let mut camps = Camps::new();
    camps.set(1, camp(vec![2]));
    let (_, freed) = camps.dismiss(1, 0).expect("he was there");
    assert_eq!(freed, None, "not the last one");
    let left = camps.get(1).expect("the entry stays");
    assert_eq!(left.owner, Some(0), "still the player's");
    assert_eq!(left.head_count(), 1, "the leader who came back");

    // Without him, the same dismissal frees it.
    let mut camps = Camps::new();
    camps.set(1, camp(Vec::new()));
    let (_, freed) = camps.dismiss(1, 0).expect("he was there");
    assert_eq!(freed, Some(0));
    assert_eq!(camps.get(1).and_then(|c| c.owner), None);
}

/// **A camper starving to death as the last one there** leaves the place
/// held, and says the camp was not lost.
#[test]
fn a_camper_starving_last_beside_a_leader_come_back_keeps_the_place() {
    let productions = productions();
    let mut camps = Camps::new();
    let mut c = camp(vec![2]);
    c.production = None;
    c.residents[0].needs.food = 0;
    c.residents[0].needs.water = 0;
    c.residents[0].needs.health = 25;
    camps.set(1, c);
    let mut springs = Springs::new();
    let mut store = Store;
    let mut world = burngfx::game::WorldDay {
        springs: &mut springs,
        camps: &mut camps,
        storage: &mut store,
        productions: &productions,
        engine: Engine::V091,
        deaths: Vec::new(),
        respawns: Box::leak(Box::new(burngfx::game::Respawns::new())),
        returned: Vec::new(),
    };
    world.turn(&Table);
    let deaths = core::mem::take(&mut world.deaths);
    assert_eq!(deaths.len(), 1, "he starved");
    assert!(!deaths[0].camp_lost, "and the place was not lost with him");
    let left = camps.get(1).expect("the entry stays");
    assert!(left.residents.is_empty());
    assert_eq!(left.owner, Some(0), "held by nobody but the man who came back");
}

/// Coming back twice is still one man in one place.
#[test]
fn a_leader_is_stationed_once() {
    let mut camps = Camps::new();
    camps.station(4, 1);
    camps.station(4, 1);
    assert_eq!(camps.get(4).map(|c| c.stationed.clone()), Some(vec![1]));
    assert_eq!(camps.get(4).and_then(|c| c.owner), None, "standing somewhere holds nothing");
}

/// Nought days is never: v1.2 Classic's dogs, mutants and traders stay dead.
#[test]
fn a_return_in_nought_days_is_no_return() {
    let mut r = burngfx::game::Respawns::new();
    r.schedule(burngfx::game::Pending { record: 142, location: 3, days: 0, food: 0, water: 0, revive_health: 31 });
    assert_eq!(r.len(), 0);
    r.schedule(burngfx::game::Pending { record: 142, location: 3, days: 30, food: 0, water: 0, revive_health: 31 });
    assert_eq!(r.len(), 1);
}
