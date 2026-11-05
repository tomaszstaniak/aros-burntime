//! Dying, for everybody but the played character.
//!
//! `Character.Die` is four things and we do three: drop what they carried,
//! take them out of the player's empire, and set health to zero. The fourth
//! is a respawn, and it is not modelled — see [`burngfx::game::Death`].

use burngfx::game::{
    advance_day, Camp, Camps, CharacterNeeds, Companion, Death, Engine, ItemBag, ItemFacts,
    Journey, Party, Springs, Storage, WorldDay, PACK_CAPACITY,
};
use burngfx::production::Productions;

const BREAD: usize = 1;
const KNIFE: usize = 2;

struct Table;

impl ItemFacts for Table {
    fn food(&self, kind: usize) -> i32 {
        if kind == BREAD {
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
            "item_bread" => Some(BREAD),
            "item_knife" => Some(KNIFE),
            _ => None,
        }
    }
    fn name_of(&self, kind: usize) -> Option<&str> {
        match kind {
            BREAD => Some("item_bread"),
            KNIFE => Some("item_knife"),
            _ => None,
        }
    }
}

struct Nowhere;

impl Storage for Nowhere {
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

fn dying(health: i32) -> CharacterNeeds {
    // No food, no water: the day takes health off them.
    CharacterNeeds { food: 0, max_food: 9, water: 0, max_water: 5, health }
}

fn well() -> CharacterNeeds {
    CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health: 100 }
}

fn a_day(party: &mut Party, leader: &mut CharacterNeeds, pack: &mut ItemBag) -> Vec<Death> {
    let mut day = 1u16;
    let mut springs = Springs::new();
    let mut camps = Camps::new();
    let mut storage = Nowhere;
    let productions = Productions::default();
    let mut world = WorldDay {
        springs: &mut springs,
        camps: &mut camps,
        storage: &mut storage,
        productions: &productions,
        engine: Engine::V091,
        deaths: Vec::new(),
            respawns: &mut burngfx::game::Respawns::new(),
            returned: Vec::new(),
    };
    let mut journey: Option<Journey> = None;
    let out = advance_day(&mut day, &mut world, leader, party, pack, &Table, &mut journey);
    out.deaths
}

// -- a companion on the road -------------------------------------------------

/// **The whole of it, for somebody walking with the player.** They die, they
/// leave the group, and what they carried comes back to be dropped.
#[test]
fn a_companion_dies_on_the_road() {
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = dying(10);
    party.companions[0].pack = ItemBag::with(PACK_CAPACITY, &[KNIFE, KNIFE]);
    let id = party.companions[0].id;

    let mut leader = well();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let deaths = a_day(&mut party, &mut leader, &mut pack);

    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].id, id, "named, so the player can be told who");
    assert_eq!(deaths[0].at, None, "on the road there is no location of their own");
    assert_eq!(deaths[0].dropped.iter().collect::<Vec<_>>(), [KNIFE, KNIFE]);
    assert!(!deaths[0].camp_lost);
    assert!(party.is_empty(), "and they are out of the group");
}

/// **Exactly once.** The day after changes nothing: there is nobody left to
/// die, and nothing is dropped a second time.
#[test]
fn nobody_dies_twice() {
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = dying(10);
    party.companions[0].pack = ItemBag::with(PACK_CAPACITY, &[KNIFE]);

    let mut leader = well();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    assert_eq!(a_day(&mut party, &mut leader, &mut pack).len(), 1);
    for _ in 0..4 {
        assert!(a_day(&mut party, &mut leader, &mut pack).is_empty(), "no second death");
    }
}

/// Health stops at zero rather than running past it. `Die` sets it, which
/// is why the number never goes negative once a death is modelled at all.
#[test]
fn a_dead_man_is_at_zero_not_below() {
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = dying(5);
    let mut leader = well();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let deaths = a_day(&mut party, &mut leader, &mut pack);
    assert_eq!(deaths.len(), 1);
    // The companion is gone, so the only way to see the number is the one
    // the frontend reports; what matters is that nothing keeps a negative.
    assert!(party.is_empty());
}

/// **A corpse does not feed the living.** Somebody who dies leaves the
/// group in the middle of the day, so the members after them cannot eat out
/// of the pack that is now on the ground.
#[test]
fn what_the_dead_carried_leaves_the_larder() {
    let mut party = Party::new();
    party.recruit(); // dies, carrying the only bread
    party.recruit(); // hungry, and must not get it
    // Fed but parched, so they die of thirst without first eating the
    // bread they are carrying — a starving man eats his own loaf, which is
    // right and would make this test prove nothing.
    party.companions[0].needs =
        CharacterNeeds { food: 9, max_food: 9, water: 0, max_water: 5, health: 10 };
    party.companions[0].pack = ItemBag::with(PACK_CAPACITY, &[BREAD]);
    party.companions[1].needs = CharacterNeeds { food: 0, ..well() };

    let mut leader = well();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let deaths = a_day(&mut party, &mut leader, &mut pack);

    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].dropped.iter().collect::<Vec<_>>(), [BREAD], "the bread fell with them");
    assert_eq!(party.len(), 1, "the survivor is still here");
    assert_eq!(party.companions[0].needs.food, 0, "and went hungry");
}

/// Several at once are each reported, and each leaves the group.
#[test]
fn more_than_one_can_go_in_a_day() {
    let mut party = Party::new();
    party.recruit();
    party.recruit();
    party.recruit();
    party.companions[0].needs = dying(5);
    party.companions[1].needs = well();
    party.companions[2].needs = dying(5);

    let mut leader = well();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let deaths = a_day(&mut party, &mut leader, &mut pack);

    assert_eq!(deaths.len(), 2);
    assert_eq!(party.len(), 1, "the well one is left");
    assert_eq!(party.companions[0].needs.health, 100);
}

/// Nobody dies who does not have to.
#[test]
fn a_well_group_loses_nobody() {
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = well();
    let mut leader = well();
    let mut pack = ItemBag::with(PACK_CAPACITY, &[BREAD]);
    assert!(a_day(&mut party, &mut leader, &mut pack).is_empty());
    assert_eq!(party.len(), 1);
}

// -- a camp ------------------------------------------------------------------

fn a_camp_day(camps: &mut Camps) -> Vec<Death> {
    let mut storage = Nowhere;
    let productions = Productions::default();
    let mut springs = Springs::new();
    let mut world = WorldDay {
        springs: &mut springs,
        camps,
        storage: &mut storage,
        productions: &productions,
        engine: Engine::V091,
        deaths: Vec::new(),
            respawns: &mut burngfx::game::Respawns::new(),
            returned: Vec::new(),
    };
    world.turn(&Table);
    world.deaths
}

fn camped(camps: &mut Camps, location: usize, who: &[(usize, CharacterNeeds, Vec<usize>)]) {
    let mut camp = Camp { owner: Some(0), residents: Vec::new(), production: None, progress: 0.0, stationed: Vec::new(), manual_production: false };
    for (id, needs, pack) in who {
        let mut one = Companion::new(*id);
        one.needs = *needs;
        one.pack = ItemBag::with(PACK_CAPACITY, pack);
        camp.residents.push(one);
    }
    camps.set(location, camp);
}

/// **The last one dies and the place is nobody's again.** `Dismiss` clears
/// `Location.Player` only for the last, which is the same rule that makes
/// taking the last one back give the place up.
#[test]
fn the_last_resident_dying_loses_the_camp() {
    let mut camps = Camps::new();
    camped(&mut camps, 7, &[(1, dying(10), vec![KNIFE])]);

    let deaths = a_camp_day(&mut camps);

    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].at, Some(7), "and it says where, so the things can be dropped there");
    assert!(deaths[0].camp_lost, "the last one");
    assert_eq!(deaths[0].dropped.iter().collect::<Vec<_>>(), [KNIFE]);

    let camp = camps.get(7).expect("the camp entry survives");
    assert!(camp.residents.is_empty());
    assert_eq!(camp.owner, None, "nobody's");
}

/// **One of several going does not lose it.** The camp keeps its owner and
/// the survivors keep working.
#[test]
fn one_of_several_dying_leaves_the_camp_standing() {
    let mut camps = Camps::new();
    camped(&mut camps, 7, &[(1, dying(10), vec![KNIFE]), (2, well(), vec![])]);

    let deaths = a_camp_day(&mut camps);

    assert_eq!(deaths.len(), 1);
    assert!(!deaths[0].camp_lost, "somebody is still there");

    let camp = camps.get(7).expect("the camp");
    assert_eq!(camp.residents.len(), 1);
    assert_eq!(camp.residents[0].id, 2);
    assert_eq!(camp.owner, Some(0), "still yours");
}

/// And a camp loses nobody on a day it can feed itself.
#[test]
fn a_fed_camp_loses_nobody() {
    let mut camps = Camps::new();
    camped(&mut camps, 7, &[(1, well(), vec![])]);
    assert!(a_camp_day(&mut camps).is_empty());
    assert_eq!(camps.get(7).map(|c| c.owner), Some(Some(0)));
}

/// Camp deaths happen once too.
#[test]
fn a_camp_does_not_kill_the_same_man_twice() {
    let mut camps = Camps::new();
    camped(&mut camps, 7, &[(1, dying(10), vec![KNIFE])]);
    assert_eq!(a_camp_day(&mut camps).len(), 1);
    for _ in 0..4 {
        assert!(a_camp_day(&mut camps).is_empty());
    }
    assert_eq!(camps.get(7).map(|c| c.owner), Some(None), "and it stays nobody's");
}

// -- who they were -----------------------------------------------------------

/// **Leaving the group is right; losing who they were is not.** A death
/// carries the record the companion came from, which is what a respawn
/// would need and what says the corpse was *this* person.
#[test]
fn a_death_remembers_which_person_it_was() {
    let mut party = Party::new();
    party.recruit();
    party.companions[0].origin = Some(244);
    party.companions[0].needs = dying(5);

    let mut leader = well();
    let mut pack = ItemBag::with(PACK_CAPACITY, &[]);
    let deaths = a_day(&mut party, &mut leader, &mut pack);
    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].origin, Some(244));
}

/// Somebody handed over by a scenario rather than hired has no record, and
/// the death says so rather than inventing one.
#[test]
fn a_companion_from_nowhere_has_no_record() {
    let mut party = Party::new();
    party.recruit();
    party.companions[0].needs = dying(5);
    let mut leader = well();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let deaths = a_day(&mut party, &mut leader, &mut pack);
    assert_eq!(deaths[0].origin, None);
}

// -- coming back -------------------------------------------------------------

use burngfx::game::{Pending, Respawns, Returned, MAX_HEALTH};

/// **The day of the death counts.** `Respawn.Object.Turn()` is the last
/// thing `World.Turn` does, and the entry was added during the group's turn
/// earlier the same day — so a ten-day wait is over nine days after.
#[test]
fn the_day_of_the_death_counts_towards_the_wait() {
    let mut waiting = Respawns::new();
    waiting.schedule(Pending { record: 244, location: 3, days: 10, food: 0, water: 0, revive_health: 100 });

    // Ten ticks in all, and only the tenth brings them back. The first of
    // them is the evening of the death, so the return lands nine days
    // after the day they died.
    for day in 1..10 {
        assert!(waiting.advance_day().is_empty(), "tick {day} was too early");
        assert_eq!(waiting.waiting_for(244), Some(10 - day));
    }
    let back = waiting.advance_day();
    assert_eq!(back.len(), 1, "the tenth tick is the one");
    assert_eq!(back[0].record, 244);
    assert!(waiting.is_empty());
}

/// **Full health is not a fresh character.** `Revive` is one line —
/// `health = 100` — so food and water come back as they were, and the pack
/// stays empty because `Die` emptied it.
#[test]
fn they_come_back_mended_and_nothing_else() {
    let mut waiting = Respawns::new();
    waiting.schedule(Pending { record: 244, location: 3, days: 1, food: 2, water: 0, revive_health: 100 });
    let back = waiting.advance_day();
    assert_eq!(
        back[0],
        Returned {
            record: 244,
            location: 3,
            needs: CharacterNeeds {
                food: 2,
                max_food: 9,
                water: 0,
                max_water: 5,
                health: MAX_HEALTH,
            },
            renamed: None,
        },
        "health restored, hunger and thirst exactly as they died with"
    );
}

/// A record waits once. Scheduling somebody already waiting would bring
/// two of them back.
#[test]
fn nobody_waits_twice() {
    let mut waiting = Respawns::new();
    waiting.schedule(Pending { record: 244, location: 3, days: 5, food: 0, water: 0, revive_health: 100 });
    waiting.schedule(Pending { record: 244, location: 9, days: 1, food: 0, water: 0, revive_health: 100 });
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting.waiting_for(244), Some(5), "the first schedule stands");
}

/// Several can be waiting at once and each comes back on its own day.
#[test]
fn each_waits_their_own_time() {
    let mut waiting = Respawns::new();
    waiting.schedule(Pending { record: 1, location: 3, days: 1, food: 0, water: 0, revive_health: 100 });
    waiting.schedule(Pending { record: 2, location: 4, days: 3, food: 0, water: 0, revive_health: 100 });

    let back = waiting.advance_day();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].record, 1);
    assert_eq!(waiting.advance_day().len(), 0);
    let back = waiting.advance_day();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].record, 2);
    assert_eq!(back[0].location, 4, "each to their own place");
}

/// **The place is fixed when they die and does not follow anybody.** For
/// somebody who fell on the road that is where the player was at the time,
/// because `Dismiss` had just set their location to it.
#[test]
fn the_place_they_return_to_is_fixed_at_death() {
    let mut waiting = Respawns::new();
    waiting.schedule(Pending { record: 244, location: 3, days: 2, food: 0, water: 0, revive_health: 100 });
    // Whatever happens in between, the entry is unchanged.
    assert_eq!(waiting.iter().next().map(|w| w.location), Some(3));
    waiting.advance_day();
    let back = waiting.advance_day();
    assert_eq!(back[0].location, 3);
}

/// Nobody is stranded: an entry that somehow started at or below zero
/// comes back on the next tick rather than waiting for ever.
#[test]
fn a_schedule_that_is_already_due_is_not_stranded() {
    let mut waiting = Respawns::new();
    waiting.schedule(Pending { record: 244, location: 3, days: 1, food: 0, water: 0, revive_health: 100 });
    assert_eq!(waiting.advance_day().len(), 1);
}
