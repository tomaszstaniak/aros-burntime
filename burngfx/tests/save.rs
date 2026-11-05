//! Writing a game down and reading it back.

use burngfx::game::{
    Camp, CharacterNeeds, Companion, Engine, ItemBag, Journey, Mode, Place, Point, WaterSource,
    PACK_CAPACITY,
};
use burngfx::save::{limits, parse, write, Save, SaveError, MAGIC, VERSION};
use burngfx::spawn::Dropped;

const PACK: u64 = 0x37eb_2df3_73a5_4487;

/// A game with something of every kind in it, so a round trip has something
/// to lose.
fn a_game() -> Save {
    Save {
        seed: 1993,
        // Version 23: a creature (standing index 7) running from the leader.
        motion: burngfx::gait::Motion { leader: (-3, 511), cooldown: 12 },
        flights: vec![burngfx::gait::Flight { who: 1_000_007, from: 0, to: Point::new(181, 40), left: 31 }],
        paths: Vec::new(),
        faces: Vec::new(),
        identity: burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original },
        pack: PACK,
        world: 0x2504_b15f_a379_bc79,
        difficulty: 1,
        populated: Vec::new(),
        day: 3,
        // Travelling, so the player is still counted at the place they left
        // and is out on the map. The parser enforces both.
        at_location: 0,
        place: Place::World,
        position: Point::new(152, 300),
        return_position: Some(Point::new(19, 292)),
        stood: None,
        needs: CharacterNeeds { food: 7, max_food: 9, water: 3, max_water: 5, health: 100 },
        experience: 37,
        carried: ItemBag::with(PACK_CAPACITY, &[3, 11, 12]),
        // A knife drawn out of the second slot, and nothing worn.
        kit: burngfx::combat::Kit { weapon: Some(1), protection: None },
        exposure: Default::default(),
        fight_mode: true,
        single_mode: false,
        time_left_ms: 240_000,
        round_ms: 240_000,
        previous_location: None,
        info_mode: false,
        talked: Vec::new(),
        contested: burngfx::combat::Contested::default(),
        journey: Some(Journey { from: 0, to: 12, total: 2, remaining: 1 }),
        // One machine player, mid-decision: walking a road it chose, with an
        // interval half spent and one person still with it.
        rivals: vec![burngfx::rival::Rival {
            player: 1,
            at: 5,
            journey: Some(Journey { from: 5, to: 9, total: 3, remaining: 2 }),
            headed: Some(9),
            mode: burngfx::rival::Mode::Expanding,
            wait: 2,
            experience: 37,
            needs: CharacterNeeds { food: 6, max_food: 9, water: 2, max_water: 5, health: 88 },
            pack: ItemBag::with(PACK_CAPACITY, &[3]),
            // A man hired out of the world, walking with it.
            party: burngfx::game::Party::restored(
                vec![Companion { id: 7, origin: Some(198), ..Companion::new(7) }],
                8,
            ),
            dead: false,
            classic: None,
            previous: None,
            amiga: None,
            modern: None,
            weapon_kind: None,
        }],
        decided: None,
        goals: None,
        known: Vec::new(),
        // v1.2's `visitedLocations`, a seat and its places, in order.
        visited: vec![(0, vec![3, 30])],
        cheats: false,
        companions: vec![Companion {
            id: 1,
            origin: None,
            needs: CharacterNeeds { food: 7, max_food: 9, water: 0, max_water: 5, health: 100 },
            experience: 41,
            pack: ItemBag::with(PACK_CAPACITY, &[7]),
            exposure: Default::default(),
            kit: Default::default(),
            at: Point::new(130, 97),
            move_to: Point::new(135, 104),
            draws: 3,
            // Version 23: part way through a pixel, following a click.
            stride: burngfx::gait::Stride { at: Point::new(130, 97), fx: 300, fy: -41 },
            fellower: burngfx::mind::Fellower {
                radius: 17,
                leader_dest: Some(Point::new(200, 120)),
                delay: 450,
                manual: false,
                manual_delay: 0,
                manual_stop: 0,
                ..Default::default()
            },
        }],
        next_id: 3,
        hired: vec![(235, 0)],
        fallen: vec![251],
        // v1.2: one who came back as somebody else.
        identities: vec![(61, 281, 27, 96)],
        entries: Vec::new(),
        lying: Vec::new(),
        // One creature that has been fought, has walked, and is part way
        // through deciding what to do next.
        standing: vec![burngfx::hire::Standing {
            record: 198,
            at: Point::new(120, 96),
            move_to: Point::new(160, 40),
            food: 4,
            water: 1,
            health: 12,
            mind: burngfx::mind::Mind {
                rest: 0,
                first: true,
                wait_after_attack: 54,
                chasing: 2,
                charging: true,
                target: 2,
                draws: 17,
            },
            // Version 23: how he walks — part of a pixel, a pause begun.
            bearing: burngfx::gait::Bearing {
                stride: burngfx::gait::Stride { at: Point::new(120, 96), fx: -500, fy: 12 },
                pause: 170,
                near: true,
                controlled: false,
                tame: false,
            },
            // Dismissed at Sana with a knife he drew.
            location: 1,
            experience: 44,
            pack: ItemBag::with(PACK_CAPACITY, &[3]),
            kit: burngfx::combat::Kit { weapon: Some(0), protection: None },
        }],
        respawns: vec![burngfx::game::Pending {
            record: 251,
            location: 7,
            days: 4,
            food: 0,
            water: 0,
            revive_health: 100,
        }],
        camps: vec![(
            12,
            Camp {
                owner: Some(0),
                residents: vec![Companion {
                    id: 2,
                    origin: None,
                    needs: CharacterNeeds {
                        food: 4,
                        max_food: 9,
                        water: 2,
                        max_water: 5,
                        health: 61,
                    },
                    experience: 29,
                    pack: ItemBag::with(PACK_CAPACITY, &[11, 12]),
                    exposure: Default::default(),
                    kit: Default::default(),
                    at: Point::new(140, 98),
                    move_to: Point::new(145, 105),
                    draws: 6,
                    stride: Default::default(),
                    fellower: Default::default(),
                }],
                production: Some(2),
                progress: 1.5,
                // A leader who came back here: written and read like the rest.
                stationed: vec![1],
                manual_production: false,
            },
        )],
        rooms: vec![
            (5, 3, ItemBag::with(8, &[11, 12])),
            (5, 4, ItemBag::new(8)),
        ],
        ground: vec![Dropped { location: 5, at: Point::new(120, 96), kind: 11, rounds: Some(1), icon: burngfx::spawn::PILE_UNDRAWN }],
        springs: vec![(5, WaterSource { reserve: 3, refresh: 2, capacity: 5 })],
        traders: vec![
            burngfx::trader::Trader {
                record: 244,
                name_id: 179,
                at: 7,
                home: 0,
                counter: 1,
                stock: ItemBag::with(12, &[11, 12, 3]),
            },
            burngfx::trader::Trader {
                record: 251,
                name_id: 180,
                at: 1,
                home: 1,
                counter: 0,
                stock: ItemBag::new(12),
            },
        ],
        seat: 0,
        humans: Vec::new(),
        colours: None,
        bench: None,
        finished: Vec::new(),
        selected: Default::default(),
    }
}

/// The same game with the journey finished: standing inside the place it
/// was heading for.
fn a_settled_game() -> Save {
    let mut game = a_game();
    game.journey = None;
    game.at_location = 12;
    game.place = Place::Location(12);
    game
}

/// A game as a load gives it back: for a v1.2 game, without what v1.2 does
/// not save (`Save::forget_unserialized`); a legacy game as it is.
fn loaded(game: &Save) -> Save {
    let mut g = game.clone();
    g.forget_unserialized();
    g
}

fn read(text: &str) -> Result<Save, SaveError> {
    parse(text, PACK, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original }, None)
}

// -- the round trip ----------------------------------------------------------

/// **The whole point.** Everything that goes in comes back, field for field.
#[test]
fn a_game_survives_being_written_down() {
    let before = a_game();
    let after = read(&write(&before)).expect("reads back");
    assert_eq!(after, before);
}

/// And again, so that writing is stable rather than merely reversible: a
/// save written from a loaded game is the same file.
#[test]
fn writing_a_loaded_game_gives_the_same_file() {
    let once = write(&a_game());
    let twice = write(&read(&once).expect("reads back"));
    assert_eq!(once, twice);
}

/// An empty world is still a world, and the smallest save must round trip
/// too — that is where a parser that requires a section quietly fails.
#[test]
fn a_bare_game_survives_too() {
    let mut bare = a_game();
    bare.return_position = None;
    bare.journey = None;
    bare.companions.clear();
    bare.hired.clear();
    bare.fallen.clear();
    bare.respawns.clear();
    bare.camps.clear();
    bare.rooms.clear();
    bare.ground.clear();
    bare.springs.clear();
    bare.traders.clear();
    bare.next_id = 1;
    assert_eq!(read(&write(&bare)).expect("reads back"), bare);
}

/// And so does one that is not travelling, which takes a different branch
/// of every check.
#[test]
fn a_settled_game_survives_too() {
    let before = a_settled_game();
    assert_eq!(read(&write(&before)).expect("reads back"), before);
}

/// The camp's people come back attached to their camp, not in a heap.
#[test]
fn a_camp_keeps_its_people() {
    let after = read(&write(&a_game())).expect("reads back");
    let (at, camp) = &after.camps[0];
    assert_eq!(*at, 12);
    assert_eq!(camp.residents.len(), 1);
    assert_eq!(camp.residents[0].id, 2);
    assert_eq!(camp.residents[0].pack.len(), 2);
    assert_eq!(camp.progress, 1.5, "progress is not rounded away");
}

/// **A journey survives; a walk does not.** There is no field for a walk,
/// and that is the design rather than an omission.
#[test]
fn a_journey_survives_a_load() {
    let after = read(&write(&a_game())).expect("reads back");
    let j = after.journey.expect("still travelling");
    assert_eq!((j.from, j.to, j.total, j.remaining), (0, 12, 2, 1));
}

/// An empty room is not the same as a room nobody has opened, so an empty
/// bag has to survive as an empty bag.
#[test]
fn an_emptied_room_stays_emptied() {
    let after = read(&write(&a_game())).expect("reads back");
    let empty = after.rooms.iter().find(|(l, r, _)| (*l, *r) == (5, 4)).expect("the empty room");
    assert!(empty.2.is_empty());
    assert_eq!(empty.2.capacity(), 8, "and keeps its capacity");
}

// -- being dead --------------------------------------------------------------

/// **Dead is not a field.** It is health, so the two cannot disagree.
#[test]
fn dead_is_derived_from_health() {
    let mut game = a_game();
    assert!(!game.dead());
    game.needs.health = 0;
    let after = read(&write(&game)).expect("reads back");
    assert!(after.dead());
    assert_eq!(after.needs.health, 0);
}

// -- refusals ----------------------------------------------------------------

#[test]
fn something_that_is_not_a_save() {
    assert_eq!(read(""), Err(SaveError::NotASave));
    assert_eq!(read("hello\n"), Err(SaveError::NotASave));
    assert_eq!(read("burntime-rs-savex 1\nend\n"), Err(SaveError::NotASave));
}

/// `LoadGame`'s rule: a different version is refused, never migrated.
#[test]
fn a_save_from_another_version() {
    let text = write(&a_game()).replace(
        &format!("{MAGIC} {VERSION}"),
        &format!("{MAGIC} {}", VERSION + 1),
    );
    assert_eq!(read(&text), Err(SaveError::WrongVersion { found: VERSION + 1, want: VERSION }));
}

/// **The consequence of fingerprinting the whole pack**, stated as a test so
/// that it is a decision and not a surprise: the same rules and the same
/// item ids are not enough if the pack differs.
#[test]
fn a_save_made_against_another_pack() {
    let text = write(&a_game());
    let other = 0x1111_2222_3333_4444;
    assert_eq!(
        parse(&text, other, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original }, None),
        Err(SaveError::WrongPack { found: PACK, want: other })
    );
}

#[test]
fn a_save_made_under_other_rules() {
    let text = write(&a_game());
    assert_eq!(parse(&text, PACK, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Extended }, None), Err(SaveError::WrongRules));
    assert_eq!(parse(&text, PACK, burngfx::rules::Identity::Legacy { engine: Engine::Current, mode: Mode::Original }, None), Err(SaveError::WrongRules));
}

/// **The commonest real failure.** A file cut short at any point is refused,
/// and refused as truncation rather than as nonsense.
#[test]
fn a_file_cut_short_at_every_point() {
    let text = write(&a_game());
    let lines: Vec<&str> = text.lines().collect();
    // Every prefix short of the whole thing, including one that stops in the
    // middle of the camps and one that stops just before `end`.
    for cut in 1..lines.len() {
        let partial = lines[..cut].join("\n");
        assert!(
            read(&partial).is_err(),
            "a save cut after {cut} of {} lines was accepted",
            lines.len()
        );
    }
    // The last line is `end`, and only the whole file reads.
    assert!(read(&lines.join("\n")).is_ok());
}

/// Losing only the final `end` is the case a parser is most likely to
/// accept by accident, because every record before it was well formed.
#[test]
fn a_file_missing_only_its_last_line() {
    let text = write(&a_game());
    let without_end = text.trim_end().trim_end_matches("end").to_string();
    assert_eq!(read(&without_end), Err(SaveError::Truncated));
}

#[test]
fn two_people_with_the_same_identity() {
    let mut game = a_game();
    game.companions.push(Companion::new(1));
    game.next_id = 9;
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("companion id".into())));
}

/// A companion in a camp and one walking with you are the same kind of
/// thing, so their identities share one space.
#[test]
fn an_identity_shared_between_the_group_and_a_camp() {
    let mut game = a_game();
    game.camps[0].1.residents[0].id = 1;
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("companion id".into())));
}

/// The next identity must be past every one in use, or somebody dismissed
/// would have their number handed to the next person hired.
#[test]
fn a_next_id_that_would_repeat_itself() {
    let mut game = a_game();
    game.next_id = 2; // resident 2 already holds it
    assert!(matches!(read(&write(&game)), Err(SaveError::BadValue(_))));
}

/// A leader who came back stands in one place: the one he came back to.
#[test]
fn a_leader_stationed_in_two_places() {
    let mut game = a_game();
    game.camps.push((13, burngfx::game::Camp { stationed: vec![1], ..Default::default() }));
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("stationed".into())));
}

#[test]
fn a_room_listed_twice() {
    let mut game = a_game();
    game.rooms.push((5, 3, ItemBag::new(8)));
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("room".into())));
}

#[test]
fn a_resident_of_a_camp_that_is_not_there() {
    let text = write(&a_game()).replace("resident 12 2", "resident 31 2");
    assert!(matches!(read(&text), Err(SaveError::BadValue(_))));
}

#[test]
fn a_line_nobody_wrote() {
    let text = write(&a_game()).replace("day 3", "dayz 3");
    assert_eq!(read(&text), Err(SaveError::BadLine("dayz".into())));
}

#[test]
fn a_number_that_is_not_one() {
    let text = write(&a_game()).replace("day 3", "day three");
    assert!(matches!(read(&text), Err(SaveError::BadValue(_))));
}

// -- the parts must agree ----------------------------------------------------

/// **A valid index is not enough.** Each of these is well formed and still
/// describes a world the game could not have been in.
#[test]
fn a_journey_that_disagrees_with_where_you_are() {
    let mut game = a_game();
    game.at_location = 30; // the journey says it left location 0
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

#[test]
fn travelling_from_indoors() {
    let mut game = a_game();
    game.place = Place::Location(0);
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

/// A journey with no days left would have arrived; one longer than its road
/// never existed.
#[test]
fn a_journey_that_should_have_ended() {
    let mut game = a_game();
    game.journey = Some(Journey { from: 0, to: 12, total: 2, remaining: 0 });
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));

    game.journey = Some(Journey { from: 0, to: 12, total: 2, remaining: 3 });
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

#[test]
fn inside_a_place_you_are_not_at() {
    let mut game = a_settled_game();
    game.place = Place::Location(30);
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

/// People imply an owner, because leaving somebody is what claims a place.
#[test]
fn a_camp_with_people_and_no_owner() {
    let mut game = a_game();
    game.camps[0].1.owner = None;
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

/// The reverse is legal: the 1993 save gives locations an owner with nobody
/// camped there, and a new game starts from it.
#[test]
fn an_owner_with_nobody_camped_is_fine() {
    let mut game = a_game();
    game.camps[0].1.residents.clear();
    read(&write(&game)).expect("a legal starting world");
}

/// Hiring the same character twice would make two of them on the next load.
#[test]
fn one_character_hired_twice() {
    let mut game = a_game();
    game.hired.push((235, 1));
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("hired character".into())));
}

/// A trader's stock and their place in the walk come back as they were.
#[test]
fn a_traders_stock_and_walk_survive() {
    let after = read(&write(&a_game())).expect("reads back");
    assert_eq!(after.traders.len(), 2);
    let marty = &after.traders[0];
    assert_eq!(marty.record, 244);
    assert_eq!((marty.at, marty.home, marty.counter), (7, 0, 1), "out selling, mid-cycle");
    assert_eq!(marty.stock.iter().collect::<Vec<_>>(), [11, 12, 3]);
    assert!(after.traders[1].stock.is_empty(), "and an empty stock stays empty");
}

/// **A version 1 save cannot describe a world with traders**, so it is
/// refused rather than loaded without them.
#[test]
fn a_save_from_before_the_traders() {
    let text = write(&a_game()).replace(
        &format!("{MAGIC} {VERSION}"),
        &format!("{MAGIC} 1"),
    );
    assert_eq!(read(&text), Err(SaveError::WrongVersion { found: 1, want: VERSION }));
}

#[test]
fn one_trader_listed_twice() {
    let mut game = a_game();
    game.traders.push(game.traders[0].clone());
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("trader".into())));
}

/// The walk is home, out, home — reaching two moves them and clears it in
/// the same breath, so a saved counter is only ever zero or one.
#[test]
fn a_traders_counter_past_its_cycle() {
    let mut game = a_game();
    game.traders[0].counter = 2;
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

#[test]
fn a_trader_over_their_stock_cap() {
    let mut game = a_game();
    game.traders[0].stock = ItemBag::with(12, &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

/// The schedule survives, because nothing else says a death was not the
/// end.
#[test]
fn a_respawn_schedule_survives() {
    let after = read(&write(&a_game())).expect("reads back");
    assert_eq!(after.respawns.len(), 1);
    assert_eq!(after.respawns[0].record, 251);
    assert_eq!(after.respawns[0].location, 7);
    assert_eq!(after.respawns[0].days, 4);
}

#[test]
fn a_respawn_for_somebody_alive() {
    let mut game = a_game();
    game.respawns[0].record = 235; // hired, not dead
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

#[test]
fn a_respawn_already_due() {
    let mut game = a_game();
    game.respawns[0].days = 0;
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

/// **Who died comes back as dead.** Nothing else in a save could say so:
/// the companion is gone from the party and the record is neither hired nor
/// standing about, so without this a load would put them back alive.
#[test]
fn the_fallen_survive_a_save() {
    let after = read(&write(&a_game())).expect("reads back");
    assert_eq!(after.fallen, [251]);
}

#[test]
fn one_character_dead_twice() {
    let mut game = a_game();
    game.fallen.push(251);
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("fallen character".into())));
}

/// Hired and dead at once is not a world.
#[test]
fn a_character_both_hired_and_dead() {
    let mut game = a_game();
    game.fallen.push(235); // 235 is hired above
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

/// A companion's origin comes back, which is what a respawn would need.
#[test]
fn a_companions_origin_survives() {
    let mut game = a_game();
    game.companions[0].origin = Some(244);
    game.camps[0].1.residents[0].origin = None;
    let after = read(&write(&game)).expect("reads back");
    assert_eq!(after.companions[0].origin, Some(244));
    assert_eq!(after.camps[0].1.residents[0].origin, None, "and so does having none");
}

// -- limits ------------------------------------------------------------------

/// A file no real save could be is refused before it is read.
#[test]
fn a_file_too_large_to_be_a_save() {
    let huge = "x".repeat(limits::BYTES + 1);
    assert_eq!(read(&huge), Err(SaveError::TooBig("file".into())));
}

/// A count from a corrupt file cannot make us allocate.
#[test]
fn counts_beyond_what_a_world_can_hold() {
    let text = write(&a_game()).replace("ground 5 120 96 11", "ground 999999 120 96 11");
    assert_eq!(read(&text), Err(SaveError::TooBig("location".into())));

    let text = write(&a_game()).replace("carried 6 3 ", "carried 999999 3 ");
    assert_eq!(read(&text), Err(SaveError::TooBig("bag".into())));
}

/// A bag may not be handed more than it can hold.
#[test]
fn a_bag_holding_more_than_its_capacity() {
    // Two slots, and three items named for them.
    let text = write(&a_game()).replace("carried 6 3 ", "carried 2 3 ");
    assert_eq!(read(&text), Err(SaveError::TooBig("bag contents".into())));
}

// -- the generator -----------------------------------------------------------

/// **The audit, pinned.**
///
/// A save carries a seed, not a generator position, and that is only sound
/// because the one generator that runs after the game starts is rebuilt
/// from `seed ^ day` every day and carries nothing between them. If that
/// ever changes — a generator threaded through the turn, an item dropped
/// from a running sequence — this test fails, and it should, because saves
/// would begin to diverge silently from that moment.
#[test]
fn the_generator_carries_no_state() {
    use burngfx::spawn::Rng;
    let (seed, day) = (1993u64, 4u64);

    let mut fresh = Rng::seeded(seed ^ day);
    let first: Vec<u64> = (0..8).map(|_| fresh.next_u64()).collect();

    // A generator built the same way on another machine, or after a load,
    // draws the same numbers — there is no position to have lost.
    let mut again = Rng::seeded(seed ^ day);
    let second: Vec<u64> = (0..8).map(|_| again.next_u64()).collect();
    assert_eq!(first, second);

    // And a different day is a different sequence, so the day is doing the
    // work a carried position would otherwise have to.
    let mut tomorrow = Rng::seeded(seed ^ (day + 1));
    let third: Vec<u64> = (0..8).map(|_| tomorrow.next_u64()).collect();
    assert_ne!(first, third);
}

// -- the differential test ---------------------------------------------------

use burngfx::game::{advance_day, ItemFacts, Party, Springs, Storage, WorldDay};
use burngfx::production::Productions;

const SNAKE: usize = 1;
const MAGGOTS: usize = 2;
const KNIFE: usize = 3;

struct Table;

impl ItemFacts for Table {
    fn food(&self, kind: usize) -> i32 {
        match kind {
            SNAKE => 7,
            MAGGOTS => 3,
            _ => 0,
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
            "item_snake" => Some(SNAKE),
            "item_maggots" => Some(MAGGOTS),
            "item_knife" => Some(KNIFE),
            _ => None,
        }
    }
    fn name_of(&self, kind: usize) -> Option<&str> {
        match kind {
            SNAKE => Some("item_snake"),
            MAGGOTS => Some("item_maggots"),
            KNIFE => Some("item_knife"),
            _ => None,
        }
    }
}

/// A world just big enough to have a camp that works: two rooms, a knife in
/// one of them, and somewhere to put what gets made.
struct World {
    rooms: burngfx::game::RoomItems,
    ground: Vec<Dropped>,
}

const CAMP: usize = 12;
const ROOM_CAPACITY: usize = 4;

impl Storage for World {
    fn count_in_rooms(&self, location: usize, kind: usize) -> usize {
        (0..2)
            .filter_map(|r| self.rooms.get(location, r))
            .map(|b| b.iter().filter(|&k| k == kind).count())
            .sum()
    }
    fn tools(&self, location: usize, _production: usize) -> usize {
        (0..2)
            .filter_map(|r| self.rooms.get(location, r))
            .map(|b| b.iter().filter(|&k| k == KNIFE).count())
            .sum()
    }
    fn any_room_has_space(&self, location: usize) -> bool {
        (0..2).any(|r| self.rooms.get(location, r).is_none_or(|b| b.len() < ROOM_CAPACITY))
    }
    fn store(&mut self, location: usize, kind: usize) -> bool {
        for r in 0..2 {
            let bag =
                self.rooms.get_or_insert(location, r, || ItemBag::new(ROOM_CAPACITY));
            if bag.len() < ROOM_CAPACITY {
                return bag.push(kind);
            }
        }
        self.ground.push(Dropped { location, at: Point::new(1, 1), kind, rounds: None, icon: burngfx::spawn::PILE_UNDRAWN });
        true
    }

    /// No pump in these tests; the spring's own rate is the subject.
    fn pump_at(&self, _: usize) -> burngfx::game::Pump {
        burngfx::game::Pump::None
    }
}

/// Everything a day can touch, in one place, so a run can be compared with
/// another run rather than inspected field by field.
struct Running {
    day: u16,
    /// Where the player counts as being. It stays at the place they left
    /// until the journey ends, which is the rule the parser enforces.
    at_location: usize,
    needs: CharacterNeeds,
    party: Party,
    carried: ItemBag,
    journey: Option<Journey>,
    springs: Springs,
    camps: burngfx::game::Camps,
    world: World,
}

fn a_running_game() -> Running {
    let mut rooms = burngfx::game::RoomItems::new();
    rooms.set(CAMP, 0, ItemBag::with(ROOM_CAPACITY, &[KNIFE]));

    // Two identities handed out by the party, the way the game hands them
    // out: one stays with the player, one is left in the camp. Doing it any
    // other way makes a world the parser is right to refuse.
    let mut party = Party::new();
    party.recruit();
    party.recruit();
    let camped = party.companions.pop().expect("the second");

    let mut camps = burngfx::game::Camps::new();
    camps.join(CAMP, 0, camped);
    if let Some(camp) = camps.at(CAMP) {
        camp.production = Some(0);
    }

    let mut springs = Springs::new();
    springs.set(5, WaterSource { reserve: 1, refresh: 2, capacity: 5 });

    Running {
        day: 1,
        at_location: 0,
        needs: CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health: 100 },
        party,
        carried: ItemBag::with(PACK_CAPACITY, &[SNAKE, KNIFE]),
        journey: Some(Journey { from: 0, to: CAMP, total: 3, remaining: 3 }),
        springs,
        camps,
        world: World { rooms, ground: Vec::new() },
    }
}

fn productions() -> Productions {
    Productions::parse(
        "[item_knife]\nmaxcombination=2\namount=0 4 6\nproduce=item_maggots\n",
    )
}

impl Running {
    fn a_day(&mut self, productions: &Productions) {
        let mut world = WorldDay {
            springs: &mut self.springs,
            camps: &mut self.camps,
            storage: &mut self.world,
            productions,
            engine: Engine::V091,
            deaths: Vec::new(),
            respawns: &mut burngfx::game::Respawns::new(),
            returned: Vec::new(),
        };
        let out = advance_day(
            &mut self.day,
            &mut world,
            &mut self.needs,
            &mut self.party,
            &mut self.carried,
            &Table,
            &mut self.journey,
        );
        // The frontend does exactly this on arrival, and the save's own
        // consistency check is what would catch it being forgotten.
        if let Some(reached) = out.arrived_at {
            self.at_location = reached;
        }
    }

    /// The whole of it, as a save — which is also how two runs are compared.
    fn snapshot(&self) -> Save {
        Save {
            seed: 1993,
                motion: Default::default(),
            flights: Vec::new(),
            paths: Vec::new(),
            faces: Vec::new(),
            identity: burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original },
            pack: PACK,
            world: 0,
            difficulty: 0,
            populated: Vec::new(),
            day: self.day,
            at_location: self.at_location,
            place: Place::World,
            position: Point::new(19, 292),
            return_position: None,
            stood: None,
            needs: self.needs,
            experience: 37,
            carried: self.carried.clone(),
            kit: Default::default(),
            exposure: Default::default(),
            fight_mode: false,
            single_mode: false,
            time_left_ms: 240_000,
            round_ms: 240_000,
            previous_location: None,
            info_mode: false,
            talked: Vec::new(),
            contested: Default::default(),
            standing: Vec::new(),
            journey: self.journey,
            rivals: Vec::new(),
            decided: None,
            goals: None,
            known: Vec::new(),
            visited: Vec::new(),
            cheats: false,
            companions: self.party.companions.clone(),
            next_id: self.party.next_id(),
            hired: vec![(235, 0)],
            fallen: Vec::new(),
            identities: Vec::new(),
            entries: Vec::new(),
            lying: Vec::new(),
            respawns: Vec::new(),
            traders: Vec::new(),
            camps: self.camps.iter().map(|(l, c)| (l, c.clone())).collect(),
            rooms: self.world.rooms.iter().map(|(l, r, b)| (l, r, b.clone())).collect(),
            ground: self.world.ground.clone(),
            springs: self.springs.iter().map(|(l, s)| (l, *s)).collect(),
            seat: 0,
            humans: Vec::new(),
            colours: None,
            bench: None,
            finished: Vec::new(),
            selected: Default::default(),
        }
    }

    /// Put a parsed save back. **One move, not field by field** — that is
    /// what makes a failed load leave the old game standing.
    fn restore(save: &Save) -> Running {
        let mut rooms = burngfx::game::RoomItems::new();
        for (l, r, bag) in &save.rooms {
            rooms.set(*l, *r, bag.clone());
        }
        let mut camps = burngfx::game::Camps::new();
        for (l, camp) in &save.camps {
            camps.set(*l, camp.clone());
        }
        let mut springs = Springs::new();
        for (l, s) in &save.springs {
            springs.set(*l, *s);
        }
        Running {
            day: save.day,
            at_location: save.at_location,
            needs: save.needs,
            party: Party::restored(save.companions.clone(), save.next_id),
            carried: save.carried.clone(),
            journey: save.journey,
            springs,
            camps,
            world: World { rooms, ground: save.ground.clone() },
        }
    }
}

/// **The criterion.** Save in the middle of a journey, with a companion and
/// a working camp; read it back; finish the journey. The world that comes
/// out is the world that would have come out without the interruption —
/// compared as whole state, not as a log, because a field missed by both
/// the save and the log would be invisible in one.
#[test]
fn a_saved_game_and_an_uninterrupted_one_end_the_same() {
    let productions = productions();

    // Straight through.
    let mut straight = a_running_game();
    for _ in 0..5 {
        straight.a_day(&productions);
    }

    // Interrupted: one day, written down, read back, then the rest.
    let mut broken = a_running_game();
    broken.a_day(&productions);
    let text = write(&broken.snapshot());
    let reloaded = read(&text).expect("the save reads back");
    let mut broken = Running::restore(&reloaded);
    for _ in 0..4 {
        broken.a_day(&productions);
    }

    let a = straight.snapshot();
    let b = broken.snapshot();

    // Named first, so a failure says which part of the world drifted rather
    // than dumping two whole games.
    assert_eq!(b.day, a.day, "day");
    assert_eq!(b.needs, a.needs, "the player's needs");
    assert_eq!(b.journey, a.journey, "the journey");
    assert_eq!(b.companions, a.companions, "who is walking with you");
    assert_eq!(b.carried, a.carried, "what you carry");
    assert_eq!(b.camps, a.camps, "camps, their people and their progress");
    assert_eq!(b.rooms, a.rooms, "what is in the rooms");
    assert_eq!(b.ground, a.ground, "what is on the ground");
    assert_eq!(b.springs, a.springs, "the springs");
    assert_eq!(b.traders, a.traders, "the traders, their stock and their walk");
    assert_eq!(b, a, "and the whole of it");
}

/// The interruption has to be worth something: if nothing changed over
/// those days, the test above would pass on an empty world.
#[test]
fn the_days_in_that_test_actually_change_the_world() {
    let productions = productions();
    let mut game = a_running_game();
    let before = game.snapshot();
    for _ in 0..5 {
        game.a_day(&productions);
    }
    let after = game.snapshot();

    assert_ne!(after.day, before.day);
    assert_ne!(after.journey, before.journey, "the journey moved on");
    assert_ne!(after.needs, before.needs, "the player ate and drank");
    assert_ne!(after.camps, before.camps, "the camp made progress");
    assert_ne!(after.rooms, before.rooms, "and something was produced");
}

// -- traders across a save ---------------------------------------------------

/// **A trade, written down, read back, and several days more.**
///
/// This is the model's half of the durability question: it cannot say
/// whether the game's own save command collects the right things, but it
/// can say whether the format and the trader rules survive a round trip —
/// so a failure in the whole run can be attributed to one side or the
/// other instead of to "something".
#[test]
fn a_trade_and_the_days_after_it_survive_a_save() {
    use burngfx::trade::Prices;
    use burngfx::trader::{StockLists, Trader, Traders};

    // Three kinds, priced the way the classic table prices them.
    let types = burngfx::items::ItemTypes::parse(
        "[item_snake]\nclass=food\nburngfx=2\nfood=7\n\
         [item_meat]\nclass=food\nburngfx=3\nfood=4\n\
         [item_knife]\nclass=tool\nburngfx=10\n",
    );
    let prices = Prices::parse("2 = 21\n3 = 27\n10 = 45\n");
    let lists = StockLists::parse("[trader]\n179=item_meat item_knife item_snake\n");
    let kind = |name: &str| types.index_of(name);
    let neighbours = |_home: usize| vec![4usize, 5, 6];
    let (snake, knife) = (kind("item_snake").unwrap(), kind("item_knife").unwrap());

    // A world with one trader, opened for business.
    let record = 244;
    let build = || {
        let mut traders = Traders::default();
        let mut t = Trader::new(record, 179, 3);
        t.randomize_inventory(&lists, &kind, &mut burngfx::trader::rng_for(1993, 0, record));
        traders.restore(t.clone());
        (traders, t)
    };
    let (_, opened) = build();
    assert!(!opened.stock.is_empty(), "the trader has something to sell");

    // The exchange: a knife of the player's for whatever is in their first
    // slot. Done once, and its result is the state both runs start from.
    let after_trade = |mut trader: Trader| -> (ItemBag, ItemBag, Trader) {
        let mut mine = ItemBag::with(PACK_CAPACITY, &[knife, knife, snake]);
        let mut friend = ItemBag::new(PACK_CAPACITY);
        {
            let mut bags = [&mut mine, &mut friend];
            burngfx::trade::exchange(
                &mut bags,
                &mut trader.stock,
                &[(0, 0), (0, 1)],
                &[0],
                &types,
                &prices,
            )
            .expect("two knives cover anything in that list");
        }
        (mine, friend, trader)
    };

    let (mine, friend, traded) = after_trade(opened.clone());
    assert_eq!(mine.len(), 2, "two knives out, one thing in");

    // Straight through: five more days of the trader's own turns.
    let mut straight = traded.clone();
    for day in 1..6u16 {
        let mut rng = burngfx::trader::rng_for(1993, day, record);
        straight.turn(&lists, &kind, &neighbours(3), &mut rng);
    }

    // Interrupted: two days, written down, read back, then three more.
    let mut broken = traded.clone();
    for day in 1..3u16 {
        let mut rng = burngfx::trader::rng_for(1993, day, record);
        broken.turn(&lists, &kind, &neighbours(3), &mut rng);
    }
    let mut save = a_game();
    save.traders = vec![broken.clone()];
    save.carried = mine.clone();
    save.companions = vec![Companion {
        id: 1,
        origin: None,
        needs: CharacterNeeds { food: 5, max_food: 9, water: 3, max_water: 5, health: 100 },
        experience: 41,
        pack: friend.clone(),
        exposure: Default::default(),
        kit: Default::default(),
        at: Point::new(150, 99),
        move_to: Point::new(155, 106),
        draws: 9,
        stride: Default::default(),
        fellower: Default::default(),
    }];
    save.camps[0].1.residents[0].id = 2;
    save.next_id = 3;

    let text = write(&save);
    let read_back = read(&text).expect("the save reads back");
    let mut broken = read_back.traders[0].clone();
    for day in 3..6u16 {
        let mut rng = burngfx::trader::rng_for(1993, day, record);
        broken.turn(&lists, &kind, &neighbours(3), &mut rng);
    }

    assert_eq!(broken.at, straight.at, "where the trader ended up");
    assert_eq!(broken.counter, straight.counter, "how far through the walk");
    assert_eq!(
        broken.stock.iter().collect::<Vec<_>>(),
        straight.stock.iter().collect::<Vec<_>>(),
        "and what is in the stock, item for item"
    );

    // Both inventories came back too — the player's and the companion's.
    assert_eq!(read_back.carried, mine, "what the player carries");
    assert_eq!(read_back.companions[0].pack, friend, "and what the companion carries");

    // The days did something, so this is not comparing two still traders.
    assert!(
        straight.at != traded.at || straight.stock.iter().collect::<Vec<_>>()
            != traded.stock.iter().collect::<Vec<_>>(),
        "five days moved the trader or churned the stock"
    );
}

// -- what a fight leaves behind ----------------------------------------------

/// **A half-fired rifle is still half-fired after a load.** The count lives
/// with the item rather than with its type, and it is the only way the
/// original's two-round rifle can be told from a fresh one at all.
#[test]
fn rounds_spent_stay_spent() {
    let mut game = a_game();
    game.carried.set_rounds_left(0, Some(1));
    // And a weapon somebody has swung, which the original leaves at minus
    // one. Odd, and saved as it is rather than tidied on the way out.
    game.carried.set_rounds_left(2, Some(-1));

    let back = read(&write(&game)).expect("reads back");

    assert_eq!(back.carried.rounds_left(0), Some(1));
    assert_eq!(back.carried.rounds_left(1), None, "untouched, and still saying so");
    assert_eq!(back.carried.rounds_left(2), Some(-1));
    assert_eq!(back, game);
}

/// A drawn weapon survives, for the leader and for a companion. Which of
/// two weapons a character holds depends on the order they drew them in, so
/// losing it would quietly re-arm everybody on load.
#[test]
fn a_drawn_weapon_survives() {
    let mut game = a_game();
    game.companions[0].kit = burngfx::combat::Kit { weapon: Some(0), protection: None };

    let back = read(&write(&game)).expect("reads back");

    assert_eq!(back.kit, burngfx::combat::Kit { weapon: Some(1), protection: None });
    assert_eq!(back.companions[0].kit.weapon, Some(0));
}

/// A weapon drawn from a slot with nothing in it is not a save.
#[test]
fn a_weapon_drawn_from_nowhere() {
    let mut game = a_game();
    game.kit.weapon = Some(5);
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));

    let mut game = a_game();
    game.companions[0].kit.protection = Some(3);
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

/// **A dog the player wounded is still wounded, still where it walked to,
/// and still thinking what it was thinking.** Everybody standing in the
/// world is rebuilt from `gam.dat`, which knows nothing of this morning, so
/// every exception has to be written down — the mind included, because
/// `wait_after_attack` is what stops a load from being worth a free bite.
#[test]
fn a_creature_survives_as_it_was() {
    let game = a_game();
    let back = read(&write(&game)).expect("reads back");
    assert_eq!(back.standing, game.standing);
    assert_eq!(back.standing[0].mind.wait_after_attack, 54);
    assert!(back.standing[0].mind.charging);
}

// -- the rules the session is played under ------------------------------------

/// **The contested answers travel with the file.** They decide what a blow
/// does, so a resumed fight has to be fought under the same ones.
#[test]
fn the_contested_rules_survive() {
    let mut game = a_game();
    game.contested =
        burngfx::combat::Contested { melee_spends_a_round: false, best_armour_wins: true };

    let text = write(&game);
    assert!(
        text.contains("contested melee-spends-a-round=no best-armour-wins=yes"),
        "written by name, because this is the line somebody will read"
    );

    let back = read(&text).expect("reads back");
    assert_eq!(back.contested, game.contested);
}

/// **With nothing asked for, the file's answers are adopted.** That is the
/// ordinary case: a save is loaded by a run that was told nothing, and the
/// fight continues as it was being fought.
#[test]
fn a_load_restores_the_policy_it_was_saved_under() {
    let mut game = a_game();
    game.contested.melee_spends_a_round = false;

    let back = parse(&write(&game), PACK, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original }, None).expect("adopted");

    assert!(!back.contested.melee_spends_a_round);
}

/// **With something asked for that disagrees, the load is refused.** Two
/// authorities, and picking one of them quietly is exactly the failure this
/// is here to prevent.
#[test]
fn a_load_never_overrides_the_policy_quietly() {
    let game = a_game(); // v0.9.1's answers
    let other =
        burngfx::combat::Contested { melee_spends_a_round: false, best_armour_wins: false };

    let refused = parse(&write(&game), PACK, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original }, Some(other));

    assert_eq!(
        refused,
        Err(SaveError::ContestedRules {
            found: burngfx::combat::Contested::default(),
            want: other
        })
    );

    // Asking for what the file already says is not a disagreement.
    let agreed = parse(
        &write(&game),
        PACK,
        burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original },
        Some(burngfx::combat::Contested::default()),
    );
    assert!(agreed.is_ok());
}

/// A rule this build has never heard of is a file whose fights it cannot
/// reproduce, so it is refused rather than read past.
#[test]
fn an_unknown_contested_rule_is_refused() {
    let text = write(&a_game())
        .replace("contested melee-spends-a-round=yes", "contested nose-length=yes");
    assert_eq!(read(&text), Err(SaveError::BadValue("contested rule".into())));

    let text = write(&a_game()).replace("melee-spends-a-round=yes", "melee-spends-a-round=maybe");
    assert_eq!(read(&text), Err(SaveError::BadValue("contested answer".into())));
}


/// **Single mode survives a save**, written as a word. Upstream's
/// `Player.singleMode` is a plain field of a serialised class; a game resumed
/// with the group suddenly following again would not be the game that was
/// saved.
#[test]
fn single_mode_survives() {
    let mut game = a_game();
    game.single_mode = true;

    let text = write(&game);
    assert!(text.contains(" single\n"), "written by name on the kit line");

    let back = read(&text).expect("reads back");
    assert!(back.single_mode);
    assert_eq!(back, game);

    let text = write(&game).replace(" single\n", " alone\n");
    assert_eq!(read(&text), Err(SaveError::BadValue("single mode".into())));
}

// -- the difficulty, and reading a header on its own ---------------------------

/// **The difficulty is part of the game, not of the command line.** It used
/// to come from `gam.dat` and could not be anything else; the start screen
/// chooses it now, and a game resumed at another difficulty would generate,
/// hire and fight by other numbers.
#[test]
fn the_difficulty_survives_a_round_trip() {
    let game = a_game();
    assert_eq!(game.difficulty, 1);
    let back = read(&write(&game)).expect("a save of ours");
    assert_eq!(back.difficulty, 1);
    assert_eq!(back, game);
}

/// A file with no difficulty line is not a save of this version, and is
/// refused for the same reason every other missing line is.
#[test]
fn a_save_without_a_difficulty_is_truncated() {
    let text = write(&a_game()).replace("difficulty 1\n", "");
    assert_eq!(read(&text), Err(SaveError::Truncated));
}

/// **The header can be read without restoring anything.** The start screen
/// needs it: what is in the file decides which tables to build before the
/// load itself can check them.
#[test]
fn a_header_says_what_the_file_was_made_under() {
    let text = write(&a_game());
    let head = burngfx::save::header(&text).expect("our own save");
    assert_eq!(head.version, VERSION);
    assert_eq!(head.identity, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Original });
    assert_eq!(head.pack, PACK);
    assert_eq!(head.difficulty, 1);
    assert_eq!(head.day, 3);
}

/// It reports rather than judges: a save from another version still has a
/// readable header, so the screen can say what it is instead of showing
/// nothing. Refusing is `parse`'s job.
#[test]
fn a_header_from_another_version_is_still_read() {
    let text = write(&a_game()).replacen(
        &format!("{MAGIC} {VERSION}"),
        &format!("{MAGIC} {}", VERSION + 1),
        1,
    );
    let head = burngfx::save::header(&text).expect("the header is still a header");
    assert_eq!(head.version, VERSION + 1);
    assert_eq!(read(&text), Err(SaveError::WrongVersion { found: VERSION + 1, want: VERSION }));
}

#[test]
fn something_that_is_not_a_save_has_no_header() {
    assert_eq!(burngfx::save::header("hello\n"), Err(SaveError::NotASave));
    assert_eq!(burngfx::save::header(""), Err(SaveError::NotASave));
}

/// **A header is a hint; the body is the game.** A file whose header is
/// perfectly well formed — another mode, a version this program knows — and
/// whose body is cut short is refused, and the refusal is about the body.
///
/// This is what stands behind the start screen reading a header to decide
/// which tables to build: deciding is all it does. Nothing is restored until
/// the whole file has parsed.
#[test]
fn a_good_header_over_a_broken_body_is_still_refused() {
    let whole = write(&a_game()).replacen("mode original", "mode extended", 1);
    let head = burngfx::save::header(&whole).expect("the header is readable");
    assert_eq!(head.identity, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Extended }, "and says extended");

    // Cut everything after the header away, leaving a file that begins
    // exactly like a good one.
    let body_at = whole.find("position ").expect("a save has a position");
    let cut = &whole[..body_at];
    assert_eq!(burngfx::save::header(cut).map(|h| h.identity), Ok(burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Extended }));
    assert_eq!(
        parse(cut, PACK, burngfx::rules::Identity::Legacy { engine: Engine::V091, mode: Mode::Extended }, None),
        Err(SaveError::Truncated),
        "a header that reads is not a game that loads"
    );

    // The same file, whole, refused for the other reason: a session playing
    // Original will not take up an Extended game.
    assert_eq!(read(&whole), Err(SaveError::WrongRules));
}

// -- machine players, and how a game ends -------------------------------------

/// **A decision in flight survives.** Where it stands, the road it committed
/// to, the mode it is in and the turns left of its interval: a game resumed
/// without them would hand the opponent a free re-think.
#[test]
fn a_machine_player_comes_back_mid_decision() {
    let before = a_game();
    let after = read(&write(&before)).expect("reads back");
    assert_eq!(after.rivals, before.rivals);

    let rival = &after.rivals[0];
    assert_eq!(rival.player, 1);
    assert_eq!(rival.mode, burngfx::rival::Mode::Expanding);
    assert_eq!(rival.wait, 2);
    assert_eq!(rival.headed, Some(9));
    assert_eq!(rival.journey.map(|j| (j.to, j.remaining)), Some((9, 2)));
    assert_eq!(rival.party.len(), 1, "and the man walking with it");
    assert_eq!(rival.party.companions[0].origin, Some(198), "who he is");
    assert_eq!(rival.party.next_id(), 8, "and the next number it would hand out");
    assert_eq!(rival.experience, 37);
    assert_eq!(rival.needs.health, 88);
    assert_eq!(rival.pack.iter().collect::<Vec<_>>(), [3]);
}

/// A machine player with nothing in flight — standing still, waiting, no
/// road, nobody with it — round trips too. That is the shape a new game
/// starts in, and the one a lazy writer would lose.
#[test]
fn a_machine_player_standing_still_survives_as_well() {
    let mut game = a_game();
    game.rivals = vec![burngfx::rival::Rival::new(2, 11)];
    let after = read(&write(&game)).expect("reads back");
    assert_eq!(after.rivals, game.rivals);
    assert_eq!(after.rivals[0].journey, None);
    assert_eq!(after.rivals[0].headed, None);
    assert_eq!(after.rivals[0].mode, burngfx::rival::Mode::Waiting);
}

/// A game with no machine players at all writes no rival lines and reads
/// back as empty, rather than as a rival with everything at zero.
#[test]
fn a_game_with_no_machine_players_has_none() {
    let mut game = a_game();
    game.rivals.clear();
    let text = write(&game);
    assert!(!text.contains("rival "), "nothing written");
    assert!(read(&text).expect("reads back").rivals.is_empty());
}

/// **A decided game stays decided.** Announcing a victory twice, once before
/// the save and once after, is exactly what a load must not do.
#[test]
fn a_decision_survives_the_save() {
    for decision in [
        burngfx::rivals::Decision::Won(0),
        burngfx::rivals::Decision::Lost(1),
        burngfx::rivals::Decision::Won(3),
    ] {
        let mut game = a_game();
        game.decided = Some(decision);
        let after = read(&write(&game)).expect("reads back");
        assert_eq!(after.decided, Some(decision));
    }

    // And a game still being played says so.
    assert_eq!(read(&write(&a_game())).expect("reads back").decided, None);
}

/// A rival line that stops in the middle is a truncated file, not a rival
/// with defaults.
#[test]
fn half_a_rival_line_is_refused() {
    let text = write(&a_game());
    let cut = text
        .lines()
        .map(|l| if l.starts_with("rival ") { "rival 1 5 6 9" } else { l })
        .collect::<Vec<_>>()
        .join("\n");
    // A line that stops in the middle is a line we cannot read, and the
    // parser says which word it wanted — `BadLine`, not a rival with
    // defaults quietly filled in.
    assert!(
        matches!(read(&cut), Err(SaveError::BadLine(_))),
        "got {:?}",
        read(&cut)
    );
}

/// **Two players' first companions are both number one.** An identity is
/// handed out by a party, so it is unique within that party and says nothing
/// about anybody else's. The game that found this refused to load the moment
/// the player's camper and the opponent's camper met in one file.
#[test]
fn each_player_numbers_its_own_people() {
    let mut game = a_game();
    // The opponent's man walking with it, given the number the player's own
    // first companion has.
    game.rivals[0].party =
        burngfx::game::Party::restored(vec![Companion::new(game.companions[0].id)], 9);
    let after = read(&write(&game)).expect("two parties, two number ones");
    assert_eq!(after.rivals[0].party.companions[0].id, game.companions[0].id);
    assert_eq!(after.companions[0].id, game.companions[0].id);
}

/// Within one party they still may not collide, and a camper belongs to the
/// party that owns the camp.
#[test]
fn one_players_camper_and_companion_still_share_a_space() {
    let mut game = a_game();
    let camp = 33;
    game.rivals[0].party = burngfx::game::Party::restored(vec![Companion::new(4)], 9);
    game.camps.push((
        camp,
        burngfx::game::Camp {
            owner: Some(game.rivals[0].player),
            residents: vec![Companion::new(4)],
            production: None,
            progress: 0.0,
            stationed: Vec::new(),
        manual_production: false,
        },
    ));
    assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("companion id".into())));
}

/// **A machine player's counter clears its campers too.** Its party comes
/// back holding nobody, but the man it left in a camp still carries a number
/// it gave out — and the next one it hires must not be given that number
/// again.
#[test]
fn a_machine_players_counter_clears_the_man_it_left_behind() {
    let mut game = a_game();
    game.rivals[0].party = burngfx::game::Party::restored(Vec::new(), 1);
    game.camps.push((
        34,
        burngfx::game::Camp {
            owner: Some(game.rivals[0].player),
            residents: vec![Companion::new(6)],
            production: None,
            progress: 0.0,
            stationed: Vec::new(),
        manual_production: false,
        },
    ));
    let after = read(&write(&game)).expect("reads back");
    assert!(after.rivals[0].party.next_id() > 6, "past the man in the camp");
}

/// **One man is written down twice, and a file where the two halves disagree
/// about where he is is refused.** The camp's resident is the man — who he
/// is, what he carries, what he needs — and the standing line is the figure
/// in the place. The one column they share is his position, because that is
/// what you walk up to and fight; the writer keeps it in step, and a load
/// that found them apart would be settling a contradiction by the order it
/// read the lines in. Their needs are two ledgers and are not compared.
#[test]
fn a_camper_and_the_figure_in_his_camp_must_agree() {
    let mut game = a_game();
    let record = 77;
    game.camps.push((
        35,
        burngfx::game::Camp {
            owner: Some(0),
            residents: vec![Companion {
                origin: Some(record),
                at: burngfx::game::Point::new(100, 100),
                move_to: burngfx::game::Point::new(100, 100),
                ..Companion::new(41)
            }],
            production: None,
            progress: 0.0,
            stationed: Vec::new(),
        manual_production: false,
        },
    ));
    game.next_id = 42;
    game.standing.push(burngfx::hire::Standing {
        record,
        at: burngfx::game::Point::new(100, 100),
        move_to: burngfx::game::Point::new(100, 100),
        food: Companion::new(41).needs.food,
        water: Companion::new(41).needs.water,
        health: Companion::new(41).needs.health,
        mind: Default::default(),
        bearing: Default::default(),
        location: 5,
        experience: 0,
        pack: ItemBag::new(PACK_CAPACITY),
        kit: Default::default(),
    });
    assert!(read(&write(&game)).is_ok(), "in step, and so a world");

    // Now move the figure and leave the camper where he was.
    game.standing.last_mut().expect("just pushed").at = burngfx::game::Point::new(500, 20);
    assert_eq!(
        read(&write(&game)),
        Err(SaveError::Inconsistent(
            "a camper and the figure standing in his camp disagree".into()
        ))
    );
}

// -- one man is in one place --------------------------------------------------

/// A camp holding one man, for the tests below.
fn camp_of(owner: usize, record: usize, id: usize) -> burngfx::game::Camp {
    burngfx::game::Camp {
        owner: Some(owner),
        residents: vec![Companion { origin: Some(record), ..Companion::new(id) }],
        production: None,
        progress: 0.0,
        stationed: Vec::new(),
        manual_production: false,
    }
}

/// **The same man cannot hold two camps.** This is the state that actually
/// occurred: an opponent hired its own camper back, and the save had one
/// record as a resident of two places, owning both. The hiring path is shut,
/// and this refuses the state however it arose.
#[test]
fn one_record_cannot_be_camped_in_two_places() {
    let mut game = a_game();
    let record = 201;
    game.camps.push((30, camp_of(1, record, 11)));
    game.camps.push((31, camp_of(1, record, 12)));
    game.rivals[0].party = burngfx::game::Party::restored(Vec::new(), 13);
    assert_eq!(
        read(&write(&game)),
        Err(SaveError::Duplicate("character in two places".into()))
    );
}

/// **Nor walk with two parties.** The player's group and an opponent's are
/// different parties, and their identity numbers are allowed to collide — the
/// record underneath them is not.
#[test]
fn one_record_cannot_walk_with_two_parties() {
    let mut game = a_game();
    let record = 202;
    game.companions[0].origin = Some(record);
    game.rivals[0].party = burngfx::game::Party::restored(
        vec![Companion { origin: Some(record), ..Companion::new(7) }],
        8,
    );
    assert_eq!(
        read(&write(&game)),
        Err(SaveError::Duplicate("character in two places".into()))
    );
}

/// **Nor walk with a party and hold a camp at the same time.** Being left in
/// a camp is what takes somebody out of the group; a file where both are true
/// describes two of him.
#[test]
fn one_record_cannot_both_walk_and_camp() {
    let mut game = a_game();
    let record = 203;
    game.companions[0].origin = Some(record);
    game.camps.push((32, camp_of(0, record, 99)));
    game.next_id = 100;
    assert_eq!(
        read(&write(&game)),
        Err(SaveError::Duplicate("character in two places".into()))
    );
}

/// A companion with no record claims nobody, so two of them are not two of
/// the same man — a scenario hands people over without a `gam.dat` entry.
#[test]
fn people_with_no_record_claim_nobody() {
    let mut game = a_game();
    game.camps.push((33, {
        let mut camp = camp_of(0, 0, 51);
        camp.residents[0].origin = None;
        camp
    }));
    game.camps.push((34, {
        let mut camp = camp_of(0, 0, 52);
        camp.residents[0].origin = None;
        camp
    }));
    game.next_id = 53;
    assert!(read(&write(&game)).is_ok(), "nobody is claimed twice");
}

/// **The turn survives a save**: `World.Time` and the two pieces of player
/// state v0.9.1 serialises with it. A game taken up mid-turn has exactly the
/// time left that it was saved with.
#[test]
fn the_turn_survives() {
    let mut game = a_game();
    game.time_left_ms = 123_456;
    game.round_ms = 240_000;
    game.previous_location = Some(4);
    game.info_mode = true;
    game.talked = vec![248, 12];
    let text = write(&game);
    assert!(text.contains("talked 248 12\n"));
    assert!(text.contains("time 123456 240000\n"));
    assert!(text.contains("previous 4\n"));
    assert!(text.contains("mapmode info\n"));
    let back = read(&text).expect("reads back");
    assert_eq!(back, game);

    let bad = text.replace("time 123456 240000", "time 300000 240000");
    assert_eq!(read(&bad), Err(SaveError::BadValue("time".into())));
    let missing = text.replace("time 123456 240000\n", "");
    assert_eq!(read(&missing), Err(SaveError::Truncated));
}

/// **A part-fired gun stays part-fired all the way round**: put down, saved,
/// loaded in a new game state, picked up — one round left, not a full gun.
#[test]
fn a_part_fired_gun_on_the_ground_survives_a_save_and_is_picked_up_as_it_was() {
    use burngfx::game::{pick_up, put_down, ItemBag, Point};
    let mut game = a_game();
    let mut pack = ItemBag::with(6, &[3, 11]);
    pack.set_rounds_left(1, Some(1));
    let d = put_down(&mut pack, 1, 5, Point::new(120, 96)).unwrap();
    game.ground = vec![d];
    let back = read(&write(&game)).expect("reads back");
    let mut ground = back.ground.clone();
    let mut hands = ItemBag::with(6, &[3]);
    assert_eq!(pick_up(&mut ground, 0, &mut hands), Ok(11));
    assert_eq!(hands.rounds_left(1), Some(1));
    assert!(ground.is_empty());
}

/// Version 15: what a danger has spent and not yet made a whole point of,
/// for the player and each companion. Without it a save made standing in
/// radiation lost up to a point that an unbroken run kept.
#[test]
fn a_danger_half_paid_survives() {
    use burngfx::danger::{Exposure, UNIT};
    let mut game = a_game();
    game.exposure = Exposure { carried: 1_234_567, lifted: false };
    assert!(!game.companions.is_empty(), "the fixture has a companion");
    game.companions[0].exposure = Exposure { carried: 42, lifted: false };

    let text = write(&game);
    assert!(text.contains("exposure 1234567 42\n"));
    assert_eq!(read(&text).expect("reads back"), game);

    // A whole point or more is never carried, so it is not a save of ours.
    let too_much = text.replace("exposure 1234567 42", &format!("exposure {UNIT} 42"));
    assert_eq!(read(&too_much), Err(SaveError::BadValue("exposure".into())));
    // One number short: somebody in the group is missing theirs.
    let short = text.replace("exposure 1234567 42", "exposure 1234567");
    assert!(matches!(read(&short), Err(SaveError::Inconsistent(_))));
}

/// Version 16: a seat played by Classic's computer player keeps its mode,
/// countdown, last heading and pool — types in the order they went in, with
/// their counts — and every machine player keeps `previousLocation`.
#[test]
fn classics_state_survives() {
    let mut game = a_game();
    let mut ai = burngfx::classic_ai::ClassicAi {
        mode: burngfx::classic_ai::Mode::WaitInterval,
        wait: -3,
        headed: Some(12),
        pool: burngfx::classic_ai::Pool::default(),
    };
    for id in ["item_knife", "item_knife", "item_protection_suit", "item_rat_trap"] {
        ai.pool.insert(id);
    }
    game.rivals[0].classic = Some(ai);
    game.rivals[0].previous = Some(4);
    let text = write(&game);
    assert!(text.contains(" previous 4 classic wait -3 12 3 item_knife 2 item_protection_suit 1 item_rat_trap 1\n"));
    assert_eq!(read(&text).expect("reads back"), game);

    let bad = text.replace("item_protection_suit 1", "item_meat 1");
    assert_eq!(read(&bad), Err(SaveError::BadValue("pool item".into())), "the pool takes only its own types");
}

/// Two humans: the first has played today and is on the bench, the second
/// is playing, and the machine player sits in seat 2.
fn a_two_human_game() -> Save {
    let mut game = a_game();
    game.rivals[0].player = 2;
    game.seat = 1;
    game.finished = vec![0];
    game.humans = vec![
        burngfx::save::Human { seat: 0, name: "Max".into(), face: 3, color: burngfx::player_colors::PlayerColor::Green, out: false, gone: false },
        burngfx::save::Human { seat: 1, name: " Ma x ".into(), face: 0, color: burngfx::player_colors::PlayerColor::Red, out: false, gone: false },
    ];
    game.bench = Some(burngfx::save::Bench {
        seat: 0,
        at_location: 7,
        place: Place::Location(7),
        position: Point::new(40, 50),
        return_position: Some(Point::new(200, 210)),
        stood: None,
        needs: CharacterNeeds { food: 4, max_food: 9, water: 1, max_water: 5, health: 70 },
        experience: 41,
        carried: ItemBag::with(PACK_CAPACITY, &[12, 12]),
        kit: burngfx::combat::Kit::default(),
        exposure: burngfx::danger::Exposure { carried: 5, lifted: false },
        fight_mode: false,
        single_mode: true,
        previous_location: Some(3),
        info_mode: true,
        talked: vec![44, 45],
        journey: None,
        companions: vec![Companion { id: 30, origin: Some(120), ..Companion::new(30) }],
        next_id: 31,
        selected: burngfx::save::Selection::Companion(30),
    });
    game
}

#[test]
fn two_humans_survive_whole() {
    let game = a_two_human_game();
    let text = write(&game);
    assert!(text.contains("\nseat 1\n"));
    assert!(text.contains("\nhuman 1 red 0 alive 6  Ma x \n"), "the name as typed, spaces and all");
    assert!(text.contains("\nfinished 0\n"));
    assert!(text.contains("\nbench at 7\n"));
    assert!(text.contains("\nbench companion 30 120 "));
    let back = read(&text).expect("reads back");
    assert_eq!(back, game);
    assert_eq!(write(&back), text);
}

#[test]
fn the_bench_says_only_what_a_player_is() {
    let text = write(&a_two_human_game());
    let bad = text.replace("bench at 7\n", "bench at 7\nbench spring 5 1 1 1\n");
    assert!(matches!(read(&bad), Err(SaveError::BadLine(_))));
}

#[test]
fn two_humans_without_a_bench_is_half_a_game() {
    let mut game = a_two_human_game();
    game.bench = None;
    assert_eq!(read(&write(&game)), Err(SaveError::Truncated));
}

#[test]
fn the_one_playing_has_not_finished() {
    let mut game = a_two_human_game();
    game.finished = vec![0, 1];
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

#[test]
fn a_human_seat_is_not_the_machines() {
    let mut game = a_two_human_game();
    game.rivals[0].player = 0;
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}

#[test]
fn the_benched_man_is_one_man() {
    let mut game = a_two_human_game();
    let record = game.companions.first().and_then(|c| c.origin);
    if let (Some(b), Some(r)) = (game.bench.as_mut(), record) {
        b.companions[0].origin = Some(r);
        assert_eq!(read(&write(&game)), Err(SaveError::Duplicate("character in two places".into())));
    }
}

#[test]
fn a_selection_survives_and_must_be_the_players() {
    use burngfx::save::Selection;
    let mut game = a_two_human_game();
    let first = game.companions[0].id;
    game.selected = Selection::Companion(first);
    let text = write(&game);
    assert!(text.contains(&format!("\nselect companion {first}\n")));
    assert!(text.contains("\nbench select companion 30\n"));
    assert_eq!(read(&text).expect("reads back"), game);
    game.selected = Selection::Companion(999);
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
    game.selected = Selection::Camper(12345);
    assert!(matches!(read(&write(&game)), Err(SaveError::Inconsistent(_))));
}


// -- identity ----------------------------------------------------------------

/// A v1.2 game writes `engine v12`, its preset and its opponent, and reads
/// back as itself.
#[test]
fn a_v12_game_says_which_preset_and_opponent() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::None };
    let text = write(&game);
    assert!(text.contains("\nengine v12\npreset classic\nai none\n"), "{}", &text[..200]);
    assert!(!text.contains("\nmode "), "no legacy mode line");
    assert_eq!(burngfx::save::header(&text).unwrap().identity, game.identity);
    assert_eq!(parse(&text, PACK, game.identity, None), Ok(loaded(&game)));
    // Under any other identity it is not the game asked for.
    let extended = Identity::V12 { preset: RuleSet::Extended, ai: AiProfile::None };
    assert_eq!(parse(&text, PACK, extended, None), Err(SaveError::WrongRules));
    let legacy = Identity::Legacy { engine: Engine::V091, mode: Mode::Original };
    assert_eq!(parse(&text, PACK, legacy, None), Err(SaveError::WrongRules));
}

/// **No save carries the generator** (save version 25): v1.2's is the
/// process's static `Random` (`Burntime.Platform/Math/Math.cs:9-13`),
/// which `BinaryFormatter` never sees. A file of 22 to 24 still carries one:
/// its line is checked and dropped, and without it such a file is cut short.
#[test]
fn a_v12_save_carries_no_generator() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Extended, ai: AiProfile::Modern };
    let text = write(&game);
    assert!(!text.contains("\nrng "), "no rng line");
    let mut want = game.clone();
    want.forget_unserialized();
    assert_eq!(parse(&text, PACK, game.identity, None), Ok(want.clone()));
    // A version 25 file has no such line to give.
    let rng = {
        let (table, inext, inextp) = burngfx::net_random::NetRandom::new(7).state();
        let mut l = format!("rng {inext} {inextp}");
        for v in table {
            l.push_str(&format!(" {v}"));
        }
        l
    };
    let with = text.replacen("\nend", &format!("\n{rng}\nend"), 1);
    assert!(parse(&with, PACK, game.identity, None).is_err());
    // Version 24 wrote it: read, checked, dropped.
    let old = with.replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 24", 1);
    assert_eq!(parse(&old, PACK, game.identity, None), Ok(want));
    let cut = text.replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 24", 1);
    assert_eq!(parse(&cut, PACK, game.identity, None), Err(SaveError::Truncated));
    let short = old.replacen("\nrng ", "\nrng 56 ", 1);
    assert!(parse(&short, PACK, game.identity, None).is_err());
}

/// A Modern opponent's stalled-wait reason is a sentence, and files before
/// version 25 wrote it as one word (`%` and the blanks as `%XX`). Such a
/// file still reads — spaces, a `%`, a lone `-` and all — and the record is
/// then forgotten, as v1.2 forgets the watchdog at load (`AiState.cs:82-100`).
#[test]
fn an_older_modern_reason_with_spaces_is_read_and_forgotten() {
    use burngfx::modern_ai::state::*;
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Extended, ai: AiProfile::Modern };
    game.rivals[0].modern = Some(ModernAi::new(1, 5));
    let mut want = game.clone();
    want.forget_unserialized();
    let rng = "rng 0 21".to_string() + &" 1".repeat(56);
    for word in ["%attack%20plan%20for%20Sana", "100%25%20sure", "-", "%2541", "trade", "%"] {
        let text = write(&game)
            .replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 24", 1)
            .replacen("\nend", &format!("\n{rng}\nmodern 1 stall 12 {word} 2 0 none 0 none\nmodern 1 journey 9 {word}\nend"), 1);
        let read = parse(&text, PACK, game.identity, None);
        if word == "%" {
            assert!(read.is_ok(), "an empty escaped reason");
        } else {
            assert_eq!(read, Ok(want.clone()), "{word:?}");
        }
    }
    let broken = write(&game)
        .replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 24", 1)
        .replacen("\nend", &format!("\n{rng}\nmodern 1 journey 9 %%zz\nend"), 1);
    assert!(parse(&broken, PACK, game.identity, None).is_err());
}

/// A v1.2 game's machine faces are lines of their own, read back as they
/// were; a legacy file naming one is refused.
#[test]
fn a_v12_game_keeps_its_machine_faces() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::None };
    game.faces = vec![(1, 4), (2, 2), (3, 1)];
    let text = write(&game);
    assert!(text.contains("\nface 1 4\nface 2 2\nface 3 1\n"));
    assert_eq!(parse(&text, PACK, game.identity, None), Ok(loaded(&game)));
    assert!(parse(&text.replacen("face 2 2", "face 2 5", 1), PACK, game.identity, None).is_err());
    let legacy = write(&a_game()).replacen("\nend", "\nface 2 2\nend", 1);
    assert!(matches!(parse(&legacy, PACK, a_game().identity, None), Err(SaveError::Inconsistent(_))));
}

/// A version 21 v1.2 save had no generator, and needs none: no save does.
/// A legacy file naming one is refused.
#[test]
fn a_version_21_v12_save_reads_without_a_generator() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::Amiga };
    let text = write(&game).replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 21", 1);
    let mut want = game.clone();
    want.forget_unserialized();
    assert_eq!(parse(&text, PACK, game.identity, None), Ok(want));
    let legacy = write(&a_game())
        .replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 24", 1)
        .replacen("\nend", "\nrng 0 21 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\nend", 1);
    assert!(matches!(parse(&legacy, PACK, a_game().identity, None), Err(SaveError::Inconsistent(_))));
}

/// The legacy lines are exactly what they were, and a version 20 file is
/// still read — as the legacy game it has to be.
#[test]
fn a_legacy_save_is_unchanged_and_version_20_still_reads() {
    let game = a_game();
    let text = write(&game);
    assert!(text.contains("\nmode original\nengine v091\n"));
    let old = text.replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 20", 1);
    assert_eq!(parse(&old, PACK, game.identity, None), Ok(game.clone()));
    assert_eq!(burngfx::save::header(&old).unwrap().identity, game.identity);
}

/// Mixed or partial identities are refused, never read as the nearest one.
#[test]
fn an_identity_that_is_neither_is_refused() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let legacy = write(&a_game());
    let v12 = {
        let mut g = a_game();
        g.identity = Identity::V12 { preset: RuleSet::Extended, ai: AiProfile::Amiga };
        write(&g)
    };
    let cases = [
        legacy.replacen("engine v091\n", "engine v091\npreset classic\n", 1),
        legacy.replacen("mode original\n", "", 1),
        v12.replacen("ai amiga\n", "", 1),
        v12.replacen("preset extended", "preset v091", 1),
        v12.replacen("ai amiga", "ai clever", 1),
        v12.replacen("engine v12\n", "mode original\nengine v12\n", 1),
        v12.replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 20", 1),
    ];
    for (i, text) in cases.iter().enumerate() {
        assert_eq!(burngfx::save::header(text).map(|h| h.identity), Err(SaveError::WrongRules), "case {i}");
    }
}

/// v1.2's Amiga opponent's state is written on a line of its own and read
/// back onto its seat — and only in a v1.2 game.
#[test]
fn an_amiga_seat_is_saved_and_read_back() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::Amiga };
    game.rivals[0].amiga = Some(burngfx::amiga_ai::AmigaAi {
        difficulty: 2,
        mode: burngfx::amiga_ai::Mode::WaitInterval,
        headed: Some(4),
        wait: 1,
        last_arrival_day: 7,
        maintenance_pending: false,
    });
    let text = write(&game);
    assert!(text.contains("\namiga 1 2 wait 4 1 7 done\n"), "{text}");
    assert_eq!(parse(&text, PACK, game.identity, None), Ok(loaded(&game)));
    // In a legacy game such a line is a file that does not describe a world.
    let legacy = text
        .replacen("engine v12\npreset classic\nai amiga\n", "mode original\nengine v091\n", 1);
    assert!(parse(&legacy, PACK, Identity::Legacy { engine: Engine::V091, mode: Mode::Original }, None).is_err());
}

/// v1.2's Modern opponent: a save reads back as v1.2 loads it — what v1.2
/// serializes as it was, the rest forgotten (the parse's
/// `forget_unserialized`; `InitAfterLoad`'s recomputation is the game's, at
/// load) — and a record the file cannot hold is refused.
#[test]
fn a_modern_seat_reads_back_as_v1_2_loads_it() {
    use burngfx::modern_ai::state::*;
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Extended, ai: AiProfile::Modern };
    let mut ai = ModernAi::new(1, 5);
    ai.target = Some(12);
    ai.target_was_neutral = true;
    assert!(ai.reserve_add(30) && ai.reserve_add(7) && ai.reserve_add(7));
    ai.slump_grants_used = 2;
    ai.journey = Some(Commitment { destination: 9, reason: "trade".into() });
    ai.attack_plan_until = Some(44);
    ai.attack_recruitment = Some(AttackRecruitment { group_size: 3, until_day: 20 });
    ai.retaliating = Some((2, 31));
    ai.contested = Some((5, 17));
    ai.post_capture_pause = 1;
    ai.failed_attack = Some(FailedAttack { camp: 5, until_day: 25, group_size: 2, attacker_strength: 23.7, defender_strength: 41.1 });
    ai.last_chance_target = Some(8);
    ai.recruitment_failure_anchor = Some(11);
    let f = Force { count: 2, health: 150, strength: 0.1, boss_health: 80 };
    ai.retries.push(Retry { target: 5, owner: Some(2), attacker: f, defender: Force { count: 1, ..f }, blocked: true });
    ai.knowledge.push(Observation { camp: 5, owner: None, owner_since: 3, observed_day: 6, defenders: 2, strength: 22.0 });
    ai.deferrals.push(Deferral { target: 12, owner: None, blocker: Blocker::SettlementSupplies, retry_day: 15, progress: 3, sequence: 4 });
    ai.next_deferral = 5;
    ai.stall = Stall { stalled_target: Some(12), stalled_reason: String::new(), stalled_waits: 3, no_progress_waits: 4, no_progress_location: Some(1), strategic_no_progress_waits: 2, previous_exploratory_camp: None };
    ai.demobilized.push(41);
    ai.support = Support { initialized: true, last_trap_day: 11, trap_count: 2, saving_for_snake_trap: true, strategic: Some((4, 2, 9)), traders_seen: vec![(0, 4), (3, 9)] };
    ai.leader_kit = burngfx::combat::Kit { weapon: None, protection: Some(2) };
    ai.attack_recruitment_city = Some(3);
    ai.survival_releases = vec![7, 2];
    game.rivals[0].modern = Some(ai);

    let text = write(&game);
    let modern: Vec<&str> = text.lines().filter(|l| l.starts_with("modern ")).collect();
    assert_eq!(modern, ["modern 1 settings 1 5 2", "modern 1 target 12", "modern 1 reserve 30 1", "modern 1 reserve 7 2", "modern 1 kit none 2"]);
    let mut want = game.clone();
    want.forget_unserialized();
    assert_eq!(parse(&text, PACK, game.identity, None), Ok(want.clone()));
    let m = want.rivals[0].modern.as_ref().unwrap();
    assert_eq!((m.target, m.slump_grants_used, m.leader_kit.protection), (Some(12), 2, Some(2)));
    assert_eq!((m.journey.as_ref(), m.retries.len(), m.knowledge.len()), (None, 0, 0));

    // A version 24 file's records are read, checked and forgotten — the old
    // `attack_recruitment` with its dead city among them (LOAD-7).
    let rng = "rng 0 21".to_string() + &" 1".repeat(56);
    let old = text
        .replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 24", 1)
        .replacen("modern 1 settings 1 5 2\nmodern 1 target 12", "modern 1 settings 1 5 2 1\nmodern 1 target 12 neutral", 1)
        .replacen("\nend", &format!("\n{rng}\nmodern 1 attack_recruitment 3 20 2\nmodern 1 last_chance 8\nmodern 1 knows 5 none 3 6 2 41b00000\nend"), 1);
    assert_eq!(parse(&old, PACK, game.identity, None), Ok(want));

    // Broken records and a state with no seat are refused.
    for bad in [
        text.replacen("modern 1 reserve 7 2", "modern 1 reserve 7 6", 1),
        text.replacen("modern 1 reserve 7 2", "modern 1 reserve 30 2", 1),
        text.replacen("modern 1 target 12", "modern 1 target 12 friendly", 1),
        text.replacen("modern 1 settings 1 5 2", "modern 1 settings 3 5 2", 1),
        text.replacen("modern 1 settings 1 5 2\n", "", 1),
        text.replacen("modern 1 settings", "modern 3 settings", 1),
        old.replacen("modern 1 last_chance 8", "modern 1 last_chance 8 9", 1),
        old.replacen("attack_recruitment 3 20 2", "attack_recruitment 3 20", 1),
    ] {
        assert!(parse(&bad, PACK, game.identity, None).is_err(), "{bad}");
    }
}

/// A memory lasts to its day and no longer; the reserve stops at five.
#[test]
fn modern_memories_expire_on_their_day() {
    use burngfx::modern_ai::state::*;
    let mut ai = ModernAi::new(0, 2);
    ai.retaliating = Some((2, 10));
    ai.contested = Some((5, 12));
    ai.failed_attack = Some(FailedAttack { camp: 5, until_day: 10, group_size: 2, attacker_strength: 1.0, defender_strength: 1.0 });
    ai.attack_plan_until = Some(10);
    ai.forget_expired(10);
    assert!(ai.retaliating_against(2, 10) && ai.failed_attack.is_some() && ai.attack_plan_running(10));
    ai.forget_expired(11);
    assert!(ai.retaliating.is_none() && ai.failed_attack.is_none());
    assert!(ai.was_recently_contested(5, 11) && !ai.attack_plan_running(11));
    for _ in 0..RESERVE_CAP {
        assert!(ai.reserve_add(3));
    }
    assert!(!ai.reserve_add(3) && ai.reserve_count(3) == RESERVE_CAP);
    assert!(ai.reserve_take(3) && !ai.reserve_take(4));
    // An emptied kind keeps its place; the order is the order of arrival.
    let mut ai = ModernAi::new(0, 2);
    assert!(ai.reserve_add(9) && ai.reserve_add(2) && ai.reserve_take(9) && ai.reserve_add(4));
    assert_eq!(ai.reserve, vec![(9, 0), (2, 1), (4, 1)]);
    assert!(!ai.reserve_take(9) && ai.reserve_add(9));
    assert_eq!(ai.reserve[0], (9, 1));
}

/// The Saves page's two header lines: whose game and when, read by the page
/// and nothing else; the game they restore is the same without them.
#[test]
fn the_page_reads_the_saving_player_and_the_time() {
    let game = a_two_human_game();
    let stamp = burngfx::save::Stamp { days: 17_814, minute: 12 };
    let text = burngfx::save::write_stamped(&game, Some(stamp));
    let h = burngfx::save::header(&text).unwrap();
    assert_eq!(h.player.as_deref(), Some(" Ma x "));
    assert_eq!(h.saved, Some(stamp));
    let plain = burngfx::save::header(&write(&game)).unwrap();
    assert_eq!(plain.player.as_deref(), Some(" Ma x "));
    assert_eq!(plain.saved, None);
    let a = parse(&text, game.pack, game.identity, None).unwrap();
    let b = parse(&write(&game), game.pack, game.identity, None).unwrap();
    assert_eq!(write(&a), write(&b));
}

/// **v1.2's cheats mark goes with the game.** `ClassicGame.CheatsEnabled`
/// (`States/ClassicGame.cs:287`) is an auto-property of a `[Serializable]`
/// class with no `[NonSerialized]`, so `StateManager.Save`'s
/// `BinaryFormatter` writes its backing field and a load brings it back;
/// `MapScene.OnDialogHidden` (`MapScene.cs:286-287`) is the only writer and
/// sets it, never clears it. A game without the mark writes no line at all,
/// as before.
#[test]
fn the_cheats_mark_survives_a_save_and_a_load() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::Amiga };
    let clean = write(&game);
    assert!(!clean.contains("\ncheats\n"), "no line for a game that was not marked");
    assert_eq!(parse(&clean, PACK, game.identity, None).map(|g| g.cheats), Ok(false));

    game.cheats = true;
    let text = write(&game);
    assert!(text.contains("\ncheats\n"), "{text}");
    let back = parse(&text, PACK, game.identity, None).expect("a marked v1.2 game");
    assert!(back.cheats);
    assert_eq!(back, loaded(&game));
    assert_eq!(write(&back), text, "and written again the same");

    // A legacy game has no `/petko` dialog: such a line is no world it made.
    let mut legacy = a_game();
    legacy.cheats = true;
    assert!(matches!(read(&write(&legacy)), Err(SaveError::Inconsistent(_))));
}

/// v1.2's health is a float and a save keeps all of it (`Character.health`
/// is serialized): a camper standing in gas (`HazardRules.ApplyContinuous`
/// reaches the human's campers, `HazardRules.cs:10-11`) keeps the fraction
/// not yet a whole point across a load.
#[test]
fn a_campers_exposure_survives_a_load() {
    let mut game = a_game();
    game.camps[0].1.residents[0].exposure.carried = 12_345;
    let after = read(&write(&game)).expect("reads back");
    assert_eq!(after.camps[0].1.residents[0].exposure.carried, 12_345);
}

/// v1.2 saves `Location.Characters` as it stands, the dead still lying in
/// it included: a resumed game turns a place's people, and is clicked and
/// recruited from, in the order the unbroken one does.
#[test]
fn the_order_of_a_places_people_survives_a_load() {
    let mut game = a_game();
    game.entries = vec![(61, 3), (12, 1)];
    game.lying = vec![(251, 7)];
    let after = read(&write(&game)).expect("reads back");
    assert_eq!(after.entries, game.entries);
    assert_eq!(after.lying, game.lying);
}

/// v1.2 saves `Character.Position` (`Character.cs`, serialized): where a
/// leader on the map stands in his place comes back with a load.
#[test]
fn where_a_leader_on_the_map_stood_survives_a_load() {
    let mut game = a_game();
    game.stood = Some(Point::new(123, 45));
    let after = read(&write(&game)).expect("reads back");
    assert_eq!(after.stood, Some(Point::new(123, 45)));
}

/// A machine leader's chosen weapon (`Character.Weapon`, serialized in
/// v1.2) survives a load: the next duel starts from it (register AMI-8).
#[test]
fn a_machine_leaders_weapon_survives_a_load() {
    let mut game = a_game();
    game.rivals[0].weapon_kind = Some(3);
    let text = write(&game);
    assert!(text.contains(" weapon 3\n"));
    assert_eq!(read(&text).expect("reads back"), game);
}

/// A version 22 file has none of version 23's movement: its people and
/// companions read as standing on whole pixels, with no pause, no follower
/// state and no creature target.
#[test]
fn a_version_22_file_reads_without_the_movement() {
    let mut game = a_game();
    game.companions.clear();
    game.camps.clear();
    game.rivals.clear();
    game.motion = Default::default();
    game.flights.clear();
    for s in game.standing.iter_mut() {
        s.mind.target = 0;
        s.bearing = Default::default();
    }
    let text: String = write(&game)
        .replacen(&format!("{MAGIC} {VERSION}"), "burntime-rs-save 22", 1)
        .lines()
        .map(|l| {
            if l.starts_with("standing ") {
                let w: Vec<&str> = l.split(' ').collect();
                w[..w.len() - 6].join(" ")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert!(!text.contains("\nmotion "));
    assert_eq!(parse(&text, PACK, game.identity, None), Ok(game));
}

/// **A v1.2 save reads back as v1.2 loads it** (conflict C1, owner's
/// decision): what v1.2 serializes comes back as it was; every field it
/// leaves out of its save (`[NonSerialized]`, or a static weak table) comes
/// back at the value the CLR gives it after `Deserialize`, before
/// `InitAfterLoad` runs. One assertion per field, each with the C# field it
/// stands for.
#[test]
fn a_v12_save_reads_back_as_v1_2_loads_it() {
    use burngfx::modern_ai::state::*;
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Extended, ai: AiProfile::Modern };
    let mut ai = ModernAi::new(1, 5);
    ai.target = Some(12);
    ai.target_was_neutral = true;
    assert!(ai.reserve_add(30) && ai.reserve_add(7));
    ai.slump_grants_used = 2;
    ai.leader_kit = burngfx::combat::Kit { weapon: None, protection: Some(2) };
    ai.journey = Some(Commitment { destination: 9, reason: "trade".into() });
    ai.attack_plan_until = Some(44);
    ai.attack_recruitment = Some(AttackRecruitment { group_size: 3, until_day: 20 });
    ai.retaliating = Some((2, 31));
    ai.contested = Some((5, 17));
    ai.post_capture_pause = 1;
    ai.failed_attack = Some(FailedAttack { camp: 5, until_day: 25, group_size: 2, attacker_strength: 23.7, defender_strength: 41.1 });
    ai.last_chance_target = Some(8);
    ai.recruitment_failure_anchor = Some(11);
    let f = Force { count: 2, health: 150, strength: 0.1, boss_health: 80 };
    ai.retries.push(Retry { target: 5, owner: Some(2), attacker: f, defender: f, blocked: true });
    ai.knowledge.push(Observation { camp: 5, owner: None, owner_since: 3, observed_day: 6, defenders: 2, strength: 22.0 });
    ai.deferrals.push(Deferral { target: 12, owner: None, blocker: Blocker::SettlementSupplies, retry_day: 15, progress: 3, sequence: 4 });
    ai.next_deferral = 5;
    ai.stall = Stall { stalled_target: Some(12), stalled_waits: 3, ..Stall::default() };
    ai.demobilized.push(41);
    ai.support = Support { initialized: true, last_trap_day: 11, trap_count: 2, saving_for_snake_trap: true, strategic: Some((4, 2, 9)), traders_seen: vec![(0, 4)] };
    ai.attack_recruitment_city = Some(3);
    ai.survival_releases = vec![7];
    game.rivals[0].modern = Some(ai.clone());

    let back = parse(&write(&game), PACK, game.identity, None).unwrap();

    // Movement (`Character.cs:52-73`, `PathState.cs:17-22`, all
    // `[NonSerialized]`; `ClassicGame.cs:21-22` the bite cooldown).
    assert_eq!(back.motion, burngfx::gait::Motion::default(), "precisePosition, creatureAttackCooldownRemaining");
    assert!(back.flights.is_empty(), "fleeTimeRemaining, fleeDestination, fleeFromCharacter");
    let s = &back.standing[0];
    // `CreatureMind.cs:12-13`: serialized.
    assert_eq!((s.mind.rest, s.mind.first), (0, true), "takeSomeRest, first");
    // `CreatureMind.cs:15-19`: `[NonSerialized]`.
    assert_eq!(s.mind.wait_after_attack, 0, "waitAfterAttack");
    assert_eq!(s.mind.chasing, 0, "tryToAttack");
    assert!(!s.mind.charging && s.mind.target == 0, "attack");
    assert_eq!(s.bearing.stride, Default::default(), "PathState.precisePosition");
    assert_eq!((s.bearing.pause, s.bearing.near), (0, false), "proximityPauseRemaining, wasInTalkingDistance");
    // `FellowerMind.cs:20-37`: every field `[NonSerialized]`.
    let c = &back.companions[0];
    assert_eq!(c.fellower, Default::default(), "formationRadius, lastLeaderDestination, followStartDelay, manualFollow*");
    assert_eq!(c.stride, Default::default(), "PathState.precisePosition");

    // The Modern opponent (`AiState.cs:33-122,811-812`, the weak tables).
    let m = back.rivals[0].modern.as_ref().unwrap();
    assert_eq!((m.difficulty, m.max_advance), (1, 5), "settings");
    assert_eq!(m.slump_grants_used, 2, "slumpMaterialGrantsUsed");
    assert_eq!(m.target, Some(12), "headedLocation");
    assert_eq!(m.reserve, ai.reserve, "itemPool");
    assert_eq!(m.leader_kit, ai.leader_kit, "Character.weapon, .protection");
    assert!(!m.target_was_neutral, "strategicTargetWasNeutral (recomputed by InitAfterLoad)");
    assert_eq!(m.attack_plan_until, None, "attackPlanUntilDay (rebuilt by InitAfterLoad)");
    assert_eq!(m.post_capture_pause, 0, "postCapturePauseTurns");
    assert_eq!(m.journey, None, "committedJourneyDestination");
    assert_eq!(m.attack_recruitment, None, "attackRecruitmentDeferred");
    assert_eq!(m.retaliating, None, "retaliatingAgainst");
    assert_eq!(m.contested, None, "recentlyContestedCamp");
    assert_eq!(m.failed_attack, None, "failedAttackCamp");
    assert_eq!(m.last_chance_target, None, "lastChanceAttackTarget");
    assert_eq!(m.recruitment_failure_anchor, None, "recruitmentFailureTheaterAnchor");
    assert!(m.retries.is_empty(), "AttackRetryMemory");
    assert!(m.knowledge.is_empty(), "DefenseIntelligence");
    assert!(m.deferrals.is_empty() && m.next_deferral == 0, "TerritorialTargetDeferrals");
    assert_eq!(m.stall, Stall::default(), "consecutive*Waits, noProgressLocation, previousExploratoryCamp");
    assert!(m.demobilized.is_empty(), "demobilizedFollowers");
    assert_eq!(m.support, Support::default(), "EconomicSupport.ProgressState");
    assert_eq!(m.attack_recruitment_city, None, "AttackRecruitmentCity");
    assert!(m.survival_releases.is_empty(), "SurvivalReleaseByPlayer");
}

/// A creature some boss once took on keeps his `SimpleMind` through a save
/// (RULE-67): v1.2 serializes `Character.Mind`, and only `GameCreation`
/// makes a `CreatureMind`.
#[test]
fn a_tamed_creature_stays_tame_through_a_save() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::Amiga };
    game.standing[0].bearing.tame = true;
    let text = write(&game);
    let back = parse(&text, PACK, game.identity, None).unwrap();
    assert!(back.standing[0].bearing.tame);
    game.standing[0].bearing.tame = false;
    let back = parse(&write(&game), PACK, game.identity, None).unwrap();
    assert!(!back.standing[0].bearing.tame);
}

/// **The Amiga opponent at a load** (C1 follow-up): every field of
/// `AmigaAiState` and `OriginalAiState` is serialized — `settings`
/// (difficulty), `mode`, `headedLocation`, `wait`
/// (`AI/Profiles/Original/OriginalAiState.cs:26-30`), `lastArrivalDay` and
/// `localMaintenancePending` (`[OptionalField]`, serialized; the attribute
/// only lets older files lack them, `Amiga/AmigaAiState.cs:20-29`) — and
/// `InitAfterLoad` changes nothing but a negative `lastArrivalDay`
/// (`:54-59`; the base's is empty, `OriginalAiState.cs:73-77`). So each
/// field reads back as written, one at a time.
#[test]
fn an_amiga_seat_reads_back_as_v1_2_loads_it() {
    use burngfx::amiga_ai::{AmigaAi, Mode};
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let base = AmigaAi::new(1, 3);
    let variants: [(&str, fn(&mut AmigaAi)); 6] = [
        ("settings.Difficulty", |a| a.difficulty = 2),
        ("mode", |a| a.mode = Mode::HireNpc),
        ("headedLocation", |a| a.headed = Some(17)),
        ("wait", |a| a.wait = 5),
        ("lastArrivalDay", |a| a.last_arrival_day = 9),
        ("localMaintenancePending", |a| a.maintenance_pending = false),
    ];
    for (field, change) in variants {
        let mut game = a_game();
        game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::Amiga };
        let mut ai = base.clone();
        change(&mut ai);
        assert_ne!(ai, base, "{field}");
        game.rivals[0].amiga = Some(ai.clone());
        let back = parse(&write(&game), PACK, game.identity, None).unwrap();
        let mut got = back.rivals[0].amiga.clone().unwrap();
        got.after_load(30);
        assert_eq!(got, ai, "{field}");
    }
    // `InitAfterLoad`: a negative day is the load's day.
    let mut ai = AmigaAi { last_arrival_day: -1, ..base };
    ai.after_load(30);
    assert_eq!(ai.last_arrival_day, 30);
}

/// `ComplexPath`'s `adjustedMove` is serialized with `move`
/// (`ComplexPath.cs:418-421`; only `path` is `[NonSerialized]`): a path
/// whose search had settled on the nearest reachable place reads back
/// aiming there, to the bit; a legacy game has none.
#[test]
fn a_path_s_adjusted_destination_is_kept() {
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::Amiga };
    game.paths = vec![burngfx::gait::PathRecord { who: 2, move_to: Point::new(100, 50), adjusted: (97.25, 51.123_45) }];
    let text = write(&game);
    assert!(text.contains("\npath 2 100 50 42c28000 424c7e6a\n"), "{text}");
    let back = parse(&text, PACK, game.identity, None).unwrap();
    assert_eq!(back.paths, game.paths);
    let legacy = text.replace("engine v12\npreset classic\nai amiga\n", "mode original\nengine v091\n");
    assert!(parse(&legacy, PACK, Identity::Legacy { engine: Engine::V091, mode: Mode::Original }, None).is_err());
}

/// RULE-41 in a save: v1.2 serializes the float. Version 26 writes the
/// getter and the fraction (a lift as one [`UNIT`] more); a v1.2 file of 25
/// held the legacy meaning (one high while a fraction was carried) and is
/// read as the float it stood for.
#[test]
fn a_v12_save_keeps_the_float_health() {
    use burngfx::danger::{Exposure, UNIT};
    use burngfx::rules::{AiProfile, Identity, RuleSet};
    let mut game = a_game();
    game.identity = Identity::V12 { preset: RuleSet::Classic, ai: AiProfile::Amiga };
    game.needs.health = 48;
    game.exposure = Exposure { carried: 65 * UNIT / 100, lifted: false };
    game.companions[0].needs.health = 1;
    game.companions[0].exposure = Exposure { carried: 325 * UNIT / 1000, lifted: true };
    let text = write(&game);
    assert!(text.contains(&format!("exposure {} {}\n", 65 * UNIT / 100, 325 * UNIT / 1000 + UNIT)), "{text}");
    let back = parse(&text, PACK, game.identity, None).unwrap();
    assert_eq!((back.needs.health, back.exposure), (48, game.exposure));
    assert_eq!((back.companions[0].needs.health, back.companions[0].exposure), (1, game.companions[0].exposure));
    // Version 25: 49 with 0.35 carried was 48.65.
    let old = text
        .replacen(&format!("{MAGIC} {VERSION}"), &format!("{MAGIC} 25"), 1)
        .replace(&format!("exposure {} {}", 65 * UNIT / 100, 325 * UNIT / 1000 + UNIT), &format!("exposure {} 0", 35 * UNIT / 100))
        .replace("\nneeds 7 9 3 5 48\n", "\nneeds 7 9 3 5 49\n");
    assert!(old.contains("\nneeds 7 9 3 5 49\n"), "{old}");
    {
        let back = parse(&old, PACK, game.identity, None).unwrap();
        assert_eq!((back.needs.health, back.exposure.carried), (48, 65 * UNIT / 100));
    }
}

/// v1.3's `populated` line (M5): written only when some place was peopled,
/// read back; a save without it (every save before it) has none.
#[test]
fn populated_places_round_trip_and_default_to_none() {
    let mut game = a_game();
    game.populated = vec![37];
    let text = burngfx::save::write(&game);
    assert!(text.lines().any(|l| l == "populated 37"));
    let back = burngfx::save::parse(&text, PACK, game.identity, None).unwrap();
    assert_eq!(back.populated, vec![37]);
    let plain = burngfx::save::write(&a_game());
    assert!(!plain.contains("populated"));
    assert!(burngfx::save::parse(&plain, PACK, a_game().identity, None).unwrap().populated.is_empty());
}

/// v1.3's colours: a human's `black` and `blue`, and the `colours` line
/// with every seat's, read back; a save without the line has none.
#[test]
fn the_four_colours_read_back() {
    use burngfx::player_colors::PlayerColor::*;
    let mut game = a_two_human_game();
    game.humans[0].color = Black;
    game.humans[1].color = Blue;
    game.colours = Some([Black, Blue, Green, Red]);
    let text = write(&game);
    assert!(text.contains("\ncolours black blue green red\n"));
    let back = read(&text).expect("reads");
    assert_eq!(back.colours, Some([Black, Blue, Green, Red]));
    assert_eq!((back.humans[0].color, back.humans[1].color), (Black, Blue));
    game.colours = None;
    let old = write(&game);
    assert!(!old.contains("colours"));
    assert_eq!(read(&old).expect("reads").colours, None);
}
