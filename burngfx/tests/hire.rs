//! Taking one person on.

use burngfx::game::{CharacterNeeds, ItemBag, ItemFacts, Party, MAX_FOOD, MAX_WATER, PACK_CAPACITY};
use burngfx::gamdat::{CharClass, CharacterRecord, Placed};
use burngfx::hire::{hire, offer, Boss, People, Person, Refusal, MAX_GROUP};

// -- a small item table ------------------------------------------------------

const SNAKE: usize = 1;
const MEAT: usize = 2;
const BOTTLE: usize = 3;
const KNIFE: usize = 7;
const EMPTY_BOTTLE: usize = 8;

struct Table;

impl ItemFacts for Table {
    fn food(&self, kind: usize) -> i32 {
        match kind {
            SNAKE => 7,
            MEAT => 4,
            _ => 0,
        }
    }
    fn water(&self, kind: usize) -> i32 {
        match kind {
            BOTTLE => 3,
            _ => 0,
        }
    }
    fn full(&self, kind: usize) -> Option<usize> {
        (kind == EMPTY_BOTTLE).then_some(BOTTLE)
    }
    fn empty(&self, kind: usize) -> Option<usize> {
        (kind == BOTTLE).then_some(EMPTY_BOTTLE)
    }
    fn kind_named(&self, name: &str) -> Option<usize> {
        match name {
            "item_snake" => Some(SNAKE),
            "item_meat" => Some(MEAT),
            "item_water_bottle" => Some(BOTTLE),
            "item_knife" => Some(KNIFE),
            _ => None,
        }
    }
    fn name_of(&self, kind: usize) -> Option<&str> {
        match kind {
            SNAKE => Some("item_snake"),
            MEAT => Some("item_meat"),
            BOTTLE => Some("item_water_bottle"),
            KNIFE => Some("item_knife"),
            EMPTY_BOTTLE => Some("item_empty_bottle"),
            _ => None,
        }
    }
}

// -- people to talk to -------------------------------------------------------

/// A record placed at location 3, with whatever needs testing changed after.
fn record(class: CharClass, experience: u8, hire_item_id: u16) -> CharacterRecord {
    CharacterRecord {
        placed: Placed::At(3),
        class,
        name_id: 42,
        face_id: 1,
        text_id: 1,
        sprite_id: 1,
        at: burngfx::game::Point { x: 0, y: 0 },
        water: 1,
        food: 1,
        health: 80,
        experience,
        hire_item_id,
        employer: 0,
    }
}

fn one(class: CharClass, experience: u8, hire_item_id: u16) -> People {
    People::from_records(&[record(class, experience, hire_item_id)], &Table, &settings())
}

fn pack(kinds: &[usize]) -> ItemBag {
    ItemBag::with(PACK_CAPACITY, kinds)
}

/// Difficulty 0 in the classic `gam.dat`, which makes the floor two.
const DIFFICULTY: i32 = 0;

// -- what the panel is told --------------------------------------------------

/// With no special hire item, the three staples are accepted, in the order
/// `Hire` searches them.
#[test]
fn the_offer_names_what_is_accepted() {
    let people = one(CharClass::Mercenary, 10, 0);
    let got = offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }).expect("an offer");
    assert_eq!(got.accepts, [SNAKE, MEAT, BOTTLE]);
    assert_eq!(got.pay_with, MEAT);
    assert_eq!(got.pay_from_slot, 0);
}

/// **The order is `HireItems`', not the pack's.** Meat sits in slot 0 and a
/// snake in slot 1, and the snake goes, because the search walks the
/// accepted types and asks the pack for each.
#[test]
fn the_accepted_order_decides_which_item_goes() {
    let people = one(CharClass::Mercenary, 10, 0);
    let got = offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT, SNAKE]), experience: 100, player: 0 }).expect("an offer");
    assert_eq!(got.pay_with, SNAKE);
    assert_eq!(got.pay_from_slot, 1);
}

/// A special hire item of 4 or above replaces the staples.
#[test]
fn a_special_price_is_the_only_price() {
    let people = one(CharClass::Mercenary, 10, KNIFE as u16);
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[SNAKE, MEAT, BOTTLE]), experience: 100, player: 0 }),
        Err(Refusal::NothingTheyAccept)
    );
    let got = offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[KNIFE]), experience: 100, player: 0 }).expect("an offer");
    assert_eq!(got.accepts, [KNIFE]);
}

/// Offering changes nothing, however often it is asked.
#[test]
fn asking_costs_nothing() {
    let people = one(CharClass::Mercenary, 10, 0);
    let party = Party::new();
    let mut carried = pack(&[MEAT]);
    for _ in 0..3 {
        // The real pack, not a copy of it: the point is that asking three
        // times leaves it as it was.
        let boss = Boss { pack: &mut carried, experience: 100, player: 0 };
        offer(&people, 0, &party, &boss).expect("an offer");
    }
    assert_eq!(carried, pack(&[MEAT]));
    assert!(party.is_empty());
    assert!(!people.get(0).unwrap().hired);
}

// -- the refusals ------------------------------------------------------------

#[test]
fn creatures_and_traders_are_not_offered_work() {
    for class in [CharClass::Dog, CharClass::Mutant, CharClass::Trader] {
        let people = one(class, 10, 0);
        assert_eq!(
            offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }),
            Err(Refusal::NotOpenToWork),
            "{class:?}"
        );
    }
}

/// Four companions plus the leader is five, and five is the cap.
#[test]
fn a_full_group_takes_nobody_else() {
    let people = one(CharClass::Mercenary, 10, 0);
    let mut party = Party::new();
    for _ in 0..MAX_GROUP - 2 {
        party.recruit();
    }
    assert_eq!(party.len() + 1, MAX_GROUP - 1, "room for exactly one more");
    offer(&people, 0, &party, &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }).expect("still room");

    party.recruit();
    assert_eq!(party.len() + 1, MAX_GROUP);
    assert_eq!(
        offer(&people, 0, &party, &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }),
        Err(Refusal::GroupFull)
    );
}

/// `boss.Experience >= Parent.Experience * 0.66f`, and the threshold is
/// inclusive: at 100 theirs, 66 is enough and 65 is not.
#[test]
fn experience_is_two_thirds_of_theirs() {
    let people = one(CharClass::Mercenary, 100, 0);
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 65, player: 0 }),
        Err(Refusal::TooInexperienced)
    );
    offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 66, player: 0 }).expect("exactly two thirds");
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 0, player: 0 }),
        Err(Refusal::TooInexperienced)
    );
}

/// **The `0.66f` is kept as a float on purpose.**
///
/// Over every pair of `u8` experience values, `boss >= npc * 0.66f32` and
/// the exact `boss * 100 >= npc * 66` disagree in exactly one place: 150
/// against 99, where the float product rounds to 99.00001 and the exact
/// arithmetic lands on 99. Following the original there costs nothing and
/// means the rule is not quietly a different rule.
#[test]
fn the_one_place_the_float_matters() {
    let people = one(CharClass::Mercenary, 150, 0);
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 99, player: 0 }),
        Err(Refusal::TooInexperienced),
        "99 * 100 >= 150 * 66 would say yes; the original's float says no"
    );
    offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }).expect("one more is enough");
}

/// Somebody with no experience at all asks nothing of the leader.
#[test]
fn a_novice_asks_no_experience() {
    let people = one(CharClass::Mercenary, 0, 0);
    offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 0, player: 0 }).expect("nothing is asked");
}

#[test]
fn an_empty_pack_hires_nobody() {
    let people = one(CharClass::Mercenary, 10, 0);
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[]), experience: 100, player: 0 }),
        Err(Refusal::NothingTheyAccept)
    );
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[KNIFE]), experience: 100, player: 0 }),
        Err(Refusal::NothingTheyAccept)
    );
}

#[test]
fn another_players_employee_will_not_talk() {
    let mut r = record(CharClass::Mercenary, 10, 0);
    r.employer = 2; // counted from one, so this is player 1
    let people = People::from_records(&[r], &Table, &settings());
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }),
        Err(Refusal::WorksForSomeoneElse)
    );
    // **And not for the player who already employs them either.** Upstream
    // does not have to say so: a character of yours gets a different greeting
    // because it is yours, and hiring somebody you already employ means
    // nothing there. Here a camper stays standing so that he can be fought
    // for, which made him look like somebody to hire — and an opponent hired
    // its own man out of a camp while leaving him holding it, so that one
    // record was a resident of two places at once.
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 1 }),
        Err(Refusal::WorksForSomeoneElse),
        "already works for you"
    );
}

#[test]
fn nobody_is_standing_there() {
    let people = People::default();
    assert_eq!(
        offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }),
        Err(Refusal::NoSuchPerson)
    );
}

/// Records that name no location produce nobody to talk to.
#[test]
fn only_placed_records_become_people() {
    let mut in_group = record(CharClass::Mercenary, 10, 0);
    in_group.placed = Placed::InGroupOf;
    let mut homeless = record(CharClass::Trader, 10, 0);
    homeless.placed = Placed::HomelessTrader;
    let people =
        People::from_records(&[in_group, record(CharClass::Doctor, 10, 0), homeless], &Table, &settings());
    assert_eq!(people.len(), 1);
    assert_eq!(people.get(0).unwrap().record, 1, "the file's own index is kept");
    assert_eq!(people.at(3).collect::<Vec<_>>(), [0]);
    assert_eq!(people.at(4).count(), 0);
}

// -- what hiring actually does -----------------------------------------------

/// The whole of it: the payment goes, the person joins, and they are no
/// longer standing there.
#[test]
fn hiring_moves_the_payment_and_the_person() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let mut party = Party::new();
    let mut carried = pack(&[KNIFE, MEAT]);

    let id = hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table)
        .expect("hired");

    assert_eq!(carried, pack(&[KNIFE]), "the meat is gone and nothing else");
    assert_eq!(party.len(), 1);
    assert_eq!(party.companions[0].id, id);
    assert!(people.get(0).unwrap().hired);
    assert_eq!(people.at(3).count(), 0, "no longer standing in the location");
}

/// A hire that is refused moves nothing at all.
#[test]
fn a_refused_hire_changes_nothing() {
    let mut people = one(CharClass::Mercenary, 100, 0);
    let mut party = Party::new();
    let mut carried = pack(&[MEAT]);

    assert_eq!(
        hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 10, player: 0 }, DIFFICULTY, &Table),
        Err(Refusal::TooInexperienced)
    );
    assert_eq!(carried, pack(&[MEAT]));
    assert!(party.is_empty());
    assert!(!people.get(0).unwrap().hired);
}

/// **The same person cannot be hired twice.** The second attempt is refused
/// by the model, not by the panel, and takes no second payment.
#[test]
fn hiring_twice_is_refused_and_costs_nothing() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let mut party = Party::new();
    let mut carried = pack(&[MEAT, SNAKE]);

    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table).expect("hired");
    assert_eq!(carried, pack(&[MEAT]), "the snake went; it is first in HireItems order");

    assert_eq!(
        hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table),
        Err(Refusal::WorksForSomeoneElse)
    );
    assert_eq!(carried, pack(&[MEAT]), "the meat is still there");
    assert_eq!(party.len(), 1, "and there is still only one of them");
}

/// The panel may say yes and the model still say no: nothing between the two
/// is trusted.
#[test]
fn the_offer_is_re_checked_when_it_is_taken() {
    let people = one(CharClass::Mercenary, 10, 0);
    let party = Party::new();
    let mut carried = pack(&[MEAT]);
    let shown = {
        let boss = Boss { pack: &mut carried, experience: 100, player: 0 };
        offer(&people, 0, &party, &boss).expect("an offer")
    };
    assert_eq!(shown.pay_with, MEAT);

    // The meat is eaten before the player presses yes.
    let mut people = people;
    let mut party = party;
    let mut carried = pack(&[]);
    assert_eq!(
        hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table),
        Err(Refusal::NothingTheyAccept)
    );
    assert!(party.is_empty());
}

// -- what the payment is for -------------------------------------------------

/// The payment is eaten, capped at the maximum.
#[test]
fn the_payment_feeds_them() {
    let mut r = record(CharClass::Mercenary, 10, 0);
    r.food = 3;
    r.water = 3;
    let mut people = People::from_records(&[r], &Table, &settings());
    let mut party = Party::new();
    let mut carried = pack(&[SNAKE]); // seven food, which overflows

    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table).expect("hired");
    let joined = &party.companions[0];
    assert_eq!(joined.needs.food, MAX_FOOD, "3 + 7 capped at the maximum");
    assert_eq!(joined.needs.water, 3, "a snake is not a drink");
}

/// **Food and water are two separate uses of the same item.** A full bottle
/// waters them and does not feed them.
#[test]
fn the_payment_waters_them_separately() {
    let mut r = record(CharClass::Mercenary, 10, 0);
    r.food = 4;
    r.water = 1;
    let mut people = People::from_records(&[r], &Table, &settings());
    let mut party = Party::new();
    let mut carried = pack(&[BOTTLE]);

    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table).expect("hired");
    let joined = &party.companions[0];
    assert_eq!(joined.needs.food, 4, "untouched");
    assert_eq!(joined.needs.water, 4, "1 + 3");
}

/// The floor is `2 - difficulty` and applies to **both** food and water,
/// whether or not the payment was edible.
#[test]
fn nobody_joins_on_nothing() {
    for (difficulty, floor) in [(0, 2), (1, 1), (2, 0)] {
        let mut r = record(CharClass::Mercenary, 10, KNIFE as u16);
        r.food = 0;
        r.water = 0;
        let mut people = People::from_records(&[r], &Table, &settings());
        let mut party = Party::new();
        let mut carried = pack(&[KNIFE]); // neither food nor drink

        hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, difficulty, &Table)
            .expect("hired");
        let joined = &party.companions[0];
        assert_eq!(joined.needs.food, floor, "difficulty {difficulty}");
        assert_eq!(joined.needs.water, floor, "difficulty {difficulty}");
    }
}

/// The floor never takes anything away from someone who has more.
#[test]
fn the_floor_is_a_floor_and_not_a_reset() {
    let mut r = record(CharClass::Mercenary, 10, KNIFE as u16);
    r.food = 5;
    r.water = 4;
    let mut people = People::from_records(&[r], &Table, &settings());
    let mut party = Party::new();
    let mut carried = pack(&[KNIFE]);

    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, 0, &Table).expect("hired");
    assert_eq!(party.companions[0].needs.food, 5);
    assert_eq!(party.companions[0].needs.water, 4);
}

// -- what they bring with them -----------------------------------------------

/// They keep their own numbers, not a fresh character's.
#[test]
fn they_join_as_themselves() {
    let mut r = record(CharClass::Doctor, 10, 0);
    r.health = 61;
    r.food = 2;
    r.water = 2;
    let mut people = People::from_records(&[r], &Table, &settings());
    let mut party = Party::new();
    let mut carried = pack(&[MEAT]);

    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table).expect("hired");
    let joined = &party.companions[0];
    assert_eq!(joined.needs.health, 61, "not the start-of-game health");
    assert_ne!(joined.needs, CharacterNeeds::at_game_start());
    assert_eq!(joined.needs.max_food, MAX_FOOD);
    assert_eq!(joined.needs.max_water, MAX_WATER);
}

/// **Their pack comes with them**, and it is theirs rather than the
/// leader's.
#[test]
fn their_pack_comes_with_them() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    people.get_mut(0).unwrap().pack = pack(&[KNIFE, EMPTY_BOTTLE]);
    let mut party = Party::new();
    let mut carried = pack(&[MEAT]);

    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table).expect("hired");
    assert_eq!(party.companions[0].pack, pack(&[KNIFE, EMPTY_BOTTLE]));
    assert_eq!(carried, pack(&[]), "the leader's pack is only lighter");
    assert_eq!(party.companions[0].pack.capacity(), PACK_CAPACITY);
}

/// Two hires are two people with two identities.
#[test]
fn each_hire_is_a_different_person() {
    let people = People::from_records(
        &[record(CharClass::Mercenary, 10, 0), record(CharClass::Doctor, 10, 0)],
        &Table,
        &settings(),
    );
    let mut people = people;
    let mut party = Party::new();
    let mut carried = pack(&[MEAT, SNAKE]);

    let first = hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table)
        .expect("hired");
    let second = hire(&mut people, 1, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 }, DIFFICULTY, &Table)
        .expect("hired");
    assert_ne!(first, second);
    assert_eq!(party.len(), 2);
    assert!(carried.is_empty());
}

// -- against the shipped data ------------------------------------------------

fn shipped() -> Option<burngfx::gamdat::Gamdat> {
    let bytes = std::fs::read("testdata/burntime.pak").ok()?;
    let pak = burngfx::pak::ClassicPak::open(&bytes).ok()?;
    burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()
}

/// The world holds people who can actually be talked into working, and they
/// are a minority of what is standing about.
#[test]
fn the_shipped_world_has_people_worth_asking() {
    let Some(g) = shipped() else { return };
    let people = People::from_records(&g.characters, &Table, &settings());
    assert_eq!(people.len(), 300);

    let askable: Vec<&Person> =
        (0..people.len()).filter_map(|i| people.get(i)).filter(|p| p.class.can_be_hired()).collect();
    // 300 placed, of which 110 are creatures; the rest are traders and the
    // four professions.
    assert!(!askable.is_empty());
    assert!(askable.len() < 300 - 110, "traders are not askable either");
    for p in &askable {
        assert!(!p.accepts.is_empty(), "everybody names a price");
    }
}

/// **The played character's experience comes from the settings, not from
/// their record.** At difficulty 0 that is 37, which is what decides who in
/// the world will work for a player on their first day.
#[test]
fn the_player_starts_with_the_settings_experience() {
    let Ok(text) = std::fs::read_to_string("testdata/gamesettings_original.txt") else { return };
    let settings = burngfx::settings::GameSettings::parse(&text, 0);
    assert_eq!(settings.start_experience, 37);

    // 37 buys anybody up to 56: 56 * 0.66 = 36.96, 57 * 0.66 = 37.62.
    let people = one(CharClass::Mercenary, 56, 0);
    offer(&people, 0, &Party::new(), &Boss { pack: &mut pack(&[MEAT]), experience: 37, player: 0 })
        .expect("just within reach");
    let people = one(CharClass::Mercenary, 57, 0);
    assert_eq!(
        offer(&people, 0, &Party::new(),
              &Boss { pack: &mut pack(&[MEAT]), experience: 37, player: 0 }),
        Err(Refusal::TooInexperienced)
    );
}

// -- standing next to somebody -----------------------------------------------

/// A point already walkable is its own approach.
#[test]
fn an_approach_to_open_ground_is_that_ground() {
    let mut walk = burngfx::zei::WalkGrid::new(8, 8);
    for ty in 0..8 {
        for tx in 0..8 {
            walk.fill_tile(tx, ty, true);
        }
    }
    let target = burngfx::game::Point::new(20, 20);
    assert_eq!(burngfx::game::approach(&walk, target, 32), Some(target));
}

/// **Somebody standing on ground we call solid is still reachable.** The
/// answer is a point beside them, not nothing — which is the difference
/// between a click that walks over and one that reports no way there.
#[test]
fn an_approach_to_solid_ground_is_beside_it() {
    let mut walk = burngfx::zei::WalkGrid::new(8, 8);
    for ty in 0..8 {
        for tx in 0..8 {
            walk.fill_tile(tx, ty, true);
        }
    }
    // Close the tile they stand on. A tile is several mask cells across,
    // so this closes the whole square around the point.
    walk.fill_tile(0, 0, false);
    let target = burngfx::game::Point::new(20, 20);
    let beside = burngfx::game::approach(&walk, target, 32).expect("somewhere to stand");
    assert_ne!(beside, target);
    assert!(walk.walkable_at(beside.x, beside.y));
    assert!((beside.x - target.x).abs() <= 32 && (beside.y - target.y).abs() <= 32);
}

/// Nowhere to stand is answered as such, rather than by walking somewhere
/// far away.
#[test]
fn there_is_not_always_somewhere_to_stand() {
    let walk = burngfx::zei::WalkGrid::new(8, 8);
    assert_eq!(burngfx::game::approach(&walk, burngfx::game::Point::new(20, 20), 32), None);
}

// -- after a death -----------------------------------------------------------

/// **Neither of the two wrong states.** A record whose person died must not
/// stay "hired" with nobody to show for it, and must not go back on the map
/// ready to be hired again.
#[test]
fn a_dead_record_is_neither_hired_nor_hireable() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let mut party = Party::new();
    let mut carried = pack(&[MEAT]);
    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 },
         DIFFICULTY, &Table)
        .expect("hired");
    assert!(people.get(0).unwrap().hired);

    let record = people.get(0).unwrap().record;
    assert!(people.died(record));

    let person = people.get(0).unwrap();
    assert!(!person.hired, "they belong to nobody now");
    assert_eq!(person.employer, None);
    assert!(person.dead);

    assert_eq!(people.at(3).count(), 0, "and there is nobody to talk to");
    assert_eq!(
        offer(&people, 0, &Party::new(),
              &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }),
        Err(Refusal::NoSuchPerson),
        "the dead are not refusing; they are gone"
    );
}

/// The dead are listed for a save, because nothing else could say so.
#[test]
fn the_fallen_are_named_for_a_save() {
    let mut people = People::from_records(
        &[record(CharClass::Mercenary, 10, 0), record(CharClass::Doctor, 10, 0)],
        &Table,
        &settings(),
    );
    assert_eq!(people.fallen().count(), 0);
    let record = people.get(1).unwrap().record;
    people.died(record);
    assert_eq!(people.fallen().collect::<Vec<_>>(), [record]);
}

/// A record this world does not have is refused rather than silently
/// ignored — the same shape as restoring a trader.
#[test]
fn a_death_naming_nobody() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    assert!(!people.died(9999));
}

/// **A returned record can be hired again**, and is one person rather than
/// two: `returned` puts the same record back, belonging to nobody, with an
/// empty pack because `Die` emptied it.
#[test]
fn somebody_who_came_back_can_be_hired_again() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let mut party = Party::new();
    let mut carried = pack(&[MEAT, SNAKE]);
    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 },
         DIFFICULTY, &Table)
        .expect("hired once");
    let record = people.get(0).unwrap().record;
    // What a death does: the day takes them out of the group, and the
    // record is marked. Doing only the second half is not a death.
    party.companions.clear();
    people.died(record);

    // Their time is up: health restored, hunger and thirst as they died
    // with, and put back at a location.
    let needs = burngfx::game::CharacterNeeds {
        food: 3,
        max_food: MAX_FOOD,
        water: 0,
        max_water: MAX_WATER,
        health: 100,
    };
    let entry = burngfx::game::Point::new(252, 229);
    assert!(people.returned(record, 5, entry, needs));

    let person = people.get(0).unwrap();
    assert!(!person.dead);
    assert!(!person.hired, "belonging to nobody");
    assert_eq!(person.location, 5, "and standing where they came back");
    assert_eq!(person.at, entry, "at the location's entry point");
    assert_eq!(person.needs, needs, "mended, not made new");
    assert!(person.pack.is_empty(), "the pack stayed where Die left it");
    assert_eq!(people.at(5).count(), 1, "there to be talked to");
    assert_eq!(people.fallen().count(), 0);

    // And taken on again — the same record, not a second person.
    let id = hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 },
                  DIFFICULTY, &Table)
        .expect("hired again");
    assert_eq!(party.len(), 1, "one of them, not two");
    assert_eq!(party.companions[0].origin, Some(record), "and known to be the same person");
    assert!(id > 0);
}

// -- a day for the free ------------------------------------------------------

/// **A free person forages; they never starve.** `Character.Turn` sends
/// anybody with no player to `TurnNonPlayer`, which refills an empty
/// stomach from one to eight and an empty canteen from one to four, spends
/// one of each, and never touches health.
#[test]
fn nobody_free_ever_starves() {
    let mut people = People::from_records(
        &[record(CharClass::Mercenary, 10, 0), record(CharClass::Doctor, 10, 0)],
        &Table,
        &settings(),
    );
    let health: Vec<i32> = (0..people.len())
        .map(|i| people.get(i).expect("there").needs.health)
        .collect();
    for day in 1..200u16 {
        people.turn(1993, day, None);
        for (i, &was) in health.iter().enumerate() {
            let p = people.get(i).expect("still there");
            assert_eq!(p.needs.health, was, "health is never touched: day {day}");
            assert!(p.needs.food >= 0 && p.needs.food < p.needs.max_food);
            assert!(p.needs.water >= 0 && p.needs.water < p.needs.max_water);
        }
    }
}

/// The refill happens only when they are empty, and costs one afterwards —
/// so somebody with plenty simply spends.
#[test]
fn a_full_belly_is_just_spent_from() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    people.get_mut(0).unwrap().needs.food = 5;
    people.get_mut(0).unwrap().needs.water = 3;
    people.turn(1993, 1, None);
    let p = people.get(0).unwrap();
    assert_eq!(p.needs.food, 4);
    assert_eq!(p.needs.water, 2);
}

/// The hired and the dead take no such turn: a companion's day belongs to
/// the group, and a corpse has none.
#[test]
fn the_hired_and_the_dead_take_no_free_turn() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let mut party = Party::new();
    let mut carried = pack(&[MEAT]);
    hire(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 },
         DIFFICULTY, &Table)
        .expect("hired");
    let before = people.get(0).unwrap().needs;
    people.turn(1993, 1, None);
    assert_eq!(people.get(0).unwrap().needs, before, "hired: the group feeds them");

    let record = people.get(0).unwrap().record;
    people.died(record);
    let before = people.get(0).unwrap().needs;
    people.turn(1993, 2, None);
    assert_eq!(people.get(0).unwrap().needs, before, "dead: no day at all");
}

/// Somebody who comes back with nothing is in no danger — the next day
/// they forage.
#[test]
fn somebody_back_from_the_dead_forages_at_once() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let record = people.get(0).unwrap().record;
    people.died(record);
    let empty = burngfx::game::CharacterNeeds {
        food: 0,
        max_food: MAX_FOOD,
        water: 0,
        max_water: MAX_WATER,
        health: 100,
    };
    people.returned(record, 3, burngfx::game::Point::new(10, 10), empty);

    people.turn(1993, 5, None);
    let p = people.get(0).unwrap();
    assert_eq!(p.needs.health, 100, "still well");
    // Refilled to 1..8 then spent one, so 0..7 — and the point is that
    // nothing took health off them for being empty.
    assert!(p.needs.food < p.needs.max_food);
}

/// The class numbers a fight needs, as the classic file gives them at
/// difficulty 0.
fn settings() -> burngfx::settings::GameSettings {
    burngfx::settings::GameSettings::parse(
        "[0]\ndog_health = 31\ndog_attack = 30\ndog_respawn = 30\n\
         mutant_health = 31\nmutant_attack = 40\nmutant_respawn = 30\n\
         trader_health = 100\ntrader_attack = 60\ntrader_respawn = 20\n\
         npc_respawn = 10\n",
        0,
    )
}

/// **Somebody camped for another player is still standing there.**
///
/// `JoinCamp` does not take a character off the map — it moves them to the
/// location and leaves them there, which is why an enemy camp is a person you
/// can walk up to. So a defender keeps their place among the people standing
/// in that location, and is not hidden the way somebody walking in a group is.
#[test]
fn a_defender_stands_in_the_location_and_works_for_their_player() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let record = people.get(0).expect("somebody").record;
    let location = people.get(0).expect("somebody").location;

    assert!(people.guarding(record, 1, location, burngfx::game::Point { x: 40, y: 50 }));

    // Still standing there, so a click, `nearest_person` and a fight all
    // find them.
    assert!(
        people.at(location).any(|i| people.get(i).is_some_and(|p| p.record == record)),
        "a camper is somebody you can walk up to"
    );
    let person = people.get_by_record(record).expect("still there");
    assert_eq!(person.at, burngfx::game::Point { x: 40, y: 50 });
    assert_eq!(person.location, location);

    // And they will not work for anybody else while they do.
    assert_eq!(
        offer(&people, 0, &Party::new(),
              &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 0 }),
        Err(Refusal::WorksForSomeoneElse),
        "they are somebody else's"
    );
    // Their own player may not take them on again either: they are in a
    // camp, and being in a camp is work. See
    // `another_players_employee_will_not_talk` for what that prevents.
    assert_eq!(
        offer(&people, 0, &Party::new(),
              &Boss { pack: &mut pack(&[MEAT]), experience: 100, player: 1 }),
        Err(Refusal::WorksForSomeoneElse),
        "he is already holding the place"
    );
}

/// Guarding somebody who is not in the world answers plainly rather than
/// inventing them.
#[test]
fn nobody_can_guard_a_place_they_are_not_in() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    assert!(!people.guarding(9999, 1, 3, burngfx::game::Point { x: 0, y: 0 }));
}

/// **A trader who stays where he is keeps walking.** `move_person` is how the
/// world puts a trader in the town his round has reached, and `EnterLocation`
/// only happens when the town changes. It used to cancel the walk every time
/// it was called — and it is called for every trader on every load, so a
/// resumed game had its traders stop where they stood while an unbroken one
/// had them walk on, and every draw after that went differently.
#[test]
fn a_trader_who_stays_put_keeps_walking() {
    let mut people = one(CharClass::Trader, 10, 0);
    let target = burngfx::game::Point { x: 40, y: 20 };
    people.get_mut(0).unwrap().move_to = target;

    assert!(people.move_person(0, 3), "still in location 3");
    assert_eq!(people.get(0).unwrap().move_to, target);

    // Moving on to another town does end the walk.
    assert!(people.move_person(0, 5));
    let him = people.get(0).unwrap();
    assert_eq!(him.location, 5);
    assert_eq!(him.move_to, him.at);
}

/// **v1.2's traders forage nothing.** `Trader.Turn` does not call
/// `base.Turn` (`Logic/Character/Trader.cs:94-102`), so `TurnNonPlayer`'s
/// refill never reaches them: a trader with nothing to eat or drink still has
/// nothing after the day. (A legacy game's day refills him, as it did; the
/// port's v1.2 day did the same before this, from a stream that was also the
/// trader's stock stream.) A free mercenary beside him forages from the
/// game's generator, `Next() % (Max - 1) + 1`, then spends one.
#[test]
fn a_v12_trader_takes_no_free_persons_turn() {
    let mut people = People::from_records(
        &[record(CharClass::Trader, 10, 0), record(CharClass::Mercenary, 10, 0)],
        &Table,
        &settings(),
    );
    for i in 0..2 {
        let p = people.get_mut(i).unwrap();
        p.needs.food = 0;
        p.needs.water = 0;
    }
    let mut legacy = people.clone();
    let mut rng = burngfx::spawn::Rng::game(7);
    people.turn(1993, 1, Some(&mut rng));
    let trader = people.get(0).unwrap().needs;
    assert_eq!((trader.food, trader.water), (0, 0));
    let merc = people.get(1).unwrap().needs;
    assert!((0..MAX_FOOD - 1).contains(&merc.food) && (0..MAX_WATER - 1).contains(&merc.water));
    legacy.turn(1993, 1, None);
    let fed = legacy.get(0).unwrap().needs;
    assert!(fed.food > 0 || fed.water > 0, "a legacy day refills him: {fed:?}");
}

/// The refill is uniform over `1..Max`: over many days of one free man
/// emptied each morning, every value of `Next() % (MaxFood - 1) + 1` comes
/// up about equally often.
#[test]
fn a_v12_refill_is_uniform() {
    let mut people = one(CharClass::Mercenary, 10, 0);
    let mut rng = burngfx::spawn::Rng::game(1993);
    let mut seen = vec![0u32; MAX_FOOD as usize];
    for day in 0..14_000u16 {
        people.get_mut(0).unwrap().needs.food = 0;
        people.turn(1993, day, Some(&mut rng));
        // Refilled to 1..=Max-1, then one spent.
        seen[people.get(0).unwrap().needs.food as usize + 1] += 1;
    }
    let n = (MAX_FOOD - 1) as f64;
    let e = 14_000.0 / n;
    let chi: f64 = seen[1..].iter().map(|&o| (o as f64 - e).powi(2) / e).sum();
    assert_eq!(seen[0], 0);
    assert!(chi < 30.0, "{seen:?} chi2 {chi:.1}");
}

/// v1.2's recruit water under `remaster_difficulty`: `Next(min, max + 1)`,
/// 3 to 5 on the easiest difficulty (`GameRules.cs:101-131`), drawn afresh
/// at every hire. Hired, dismissed and hired again many times on one day,
/// the same man comes with each of 3, 4 and 5 about a third of the time —
/// where the port's stream, made from the seed, the day and his record,
/// gave him the same water every time.
#[test]
fn a_v12_rehire_draws_its_water_again() {
    use burngfx::hire::{hire_with, Terms};
    let mut people = one(CharClass::Mercenary, 10, 0);
    let mut rng = burngfx::spawn::Rng::game(1993);
    let terms = Terms::V12 {
        recruitment: burngfx::rules::Recruitment::DosXpTimes15,
        supplies: burngfx::rules::RecruitSupplies::RemasterDifficulty,
        difficulty: 0,
    };
    let mut seen = [0u32; 6];
    for _ in 0..3000 {
        let mut party = Party::new();
        let mut carried = pack(&[MEAT]);
        let id = hire_with(&mut people, 0, &mut party, &mut Boss { pack: &mut carried, experience: 100, player: 0 },
                           0, &Table, terms, &mut rng)
            .expect("hired");
        let who = party.companions.iter().find(|c| c.id == id).unwrap().clone();
        seen[who.needs.water as usize] += 1;
        let record = who.origin.unwrap();
        assert!(people.dismissed(record, 0, &who));
    }
    assert_eq!(seen[..3], [0, 0, 0]);
    for n in &seen[3..] {
        assert!((850..1150).contains(n), "{seen:?}");
    }
}

/// **Losing what the world gave you is written down.** v1.2 Classic hands
/// some people their `gam.dat` items when the world is built. A machine
/// player that takes a man's only item leaves him looking built in every
/// other way; the save must still name him, or a load — which builds the
/// world afresh, items and all — hands the item back. (Found by B7's Classic
/// Modern run: the resumed game's last save held one standing line the
/// uninterrupted one did not, a man at Nirvana with his item back.)
#[test]
fn a_man_who_lost_what_the_world_gave_him_is_written_down() {
    let built = || {
        let mut people = one(CharClass::Mercenary, 10, 0);
        let record = people.get(0).unwrap().record;
        assert!(people.hand_out(record, MEAT));
        people
    };
    let mut people = built();
    assert_eq!(people.standing().count(), 0, "as built, with his meat");
    people.get_mut(0).unwrap().pack = pack(&[]);
    let saved: Vec<_> = people.standing().collect();
    assert_eq!(saved.len(), 1, "he no longer carries what he was built with");
    let mut loaded = built();
    for s in saved {
        assert!(loaded.restore_standing(s));
    }
    assert!(loaded.get(0).unwrap().pack.is_empty());
}

// -- the order of a location's people (v1.2 `Location.Characters`) ----------

/// Three mercenaries at location 3, records 0, 1 and 2.
fn three() -> People {
    let r = |n: u8| record(CharClass::Mercenary, n, 0);
    let mut people = People::from_records(&[r(10), r(11), r(12)], &Table, &settings());
    people.arrival = true;
    people
}

fn order(people: &People) -> Vec<usize> {
    people.at(3).map(|i| people.get(i).unwrap().record).collect()
}

/// `GameCreation.LoadNPCs` fills each list in record order; `EnterLocation`
/// on a respawn appends (`Location.cs:120-124`, `Character.cs:294-310`). So
/// a man who comes back stands last, and v1.2's Amiga recruit, the first
/// hireable in the list (`OriginalAiState.cs:110-123`), takes the one who
/// never left — rules audit 2.13.
#[test]
fn whoever_comes_back_is_last_in_his_places_list() {
    let mut people = three();
    assert_eq!(order(&people), vec![0, 1, 2]);
    people.died(0);
    assert_eq!(order(&people), vec![1, 2]);
    assert!(people.returned(0, 3, burngfx::game::Point::new(0, 0), CharacterNeeds::default()));
    assert_eq!(order(&people), vec![1, 2, 0]);
}

/// `Dismiss` of a companion sets `Location = Player.Location`: the end of
/// the list. A camper let go is already in it and stays where he is
/// (`Character.cs:433-446`). `JoinCamp` appends too.
#[test]
fn a_companion_let_go_is_last_and_a_camper_let_go_keeps_his_place() {
    let mut people = three();
    let who = burngfx::game::Companion::new(1);
    assert!(people.dismissed_companion(1, 3, &who));
    assert_eq!(order(&people), vec![0, 2, 1]);
    assert!(people.guarding(0, 1, 3, burngfx::game::Point::new(0, 0)));
    assert_eq!(order(&people), vec![2, 1, 0]);
    assert!(people.dismissed(0, 3, &who));
    assert_eq!(order(&people), vec![2, 1, 0]);
}

/// A trader who walks on leaves one list and is appended to the other
/// (`NextSellLocation`: `Location = …`); one who stays where he is keeps
/// his place.
#[test]
fn a_figure_moved_to_another_place_is_last_there() {
    let mut people = three();
    assert!(people.move_person(0, 3));
    assert_eq!(order(&people), vec![0, 1, 2]);
    assert!(people.move_person(0, 4));
    assert!(people.move_person(0, 3));
    assert_eq!(order(&people), vec![1, 2, 0]);
}

/// The dead lie in their place's list until the place is shown: no day for
/// them, but the list still holds them (`Character.cs:963-968`).
#[test]
fn the_dead_lie_in_the_list_until_the_place_is_shown() {
    let mut people = three();
    assert!(people.died_with_group(1, 3));
    assert_eq!(order(&people), vec![0, 2]);
    let roll: Vec<usize> = people.roll(3).into_iter().map(|i| people.get(i).unwrap().record).collect();
    assert_eq!(roll, vec![0, 2, 1]);
    people.laid_out(3);
    assert_eq!(people.roll(3).len(), 2);
}

/// v1.2's `Location.Turn` walks `Characters` by index while a wandering
/// trader's walk takes him out of it (`Trader.cs:131-153`,
/// `StateLinkList.cs:20-45`): the man after him has no day, and the trader,
/// appended to a place whose turn is still to come, has a second one (rules
/// audit 2.4 (c), A9).
#[test]
fn a_trader_who_walks_on_skips_the_next_man_and_turns_again_further_on() {
    let r = |n: u8, at: usize| CharacterRecord { placed: Placed::At(at), ..record(CharClass::Mercenary, n, 0) };
    let mut people = People::from_records(&[r(10, 3), r(11, 3), r(12, 3), r(13, 5)], &Table, &settings());
    people.arrival = true;
    let mut turns = [0u32; 4];
    for l in [3, 5] {
        let list = people.roll(l);
        burngfx::v12_day::turn_in_order(list, |who| {
            let record = people.get(who).unwrap().record;
            turns[record] += 1;
            // Record 0 is the trader, and walks on from 3 to 5.
            record == 0 && l == 3 && people.move_person(0, 5)
        });
    }
    assert_eq!(turns, [2, 0, 1, 1]);
}

/// `Dog.Turn` sets `Health = DogHealth` before `base.Turn` asks `IsDead`
/// (`Dog.cs:28-34`): a dog killed in front of the player whose death
/// animation had not ended when the day came is still in his place's list,
/// and that day he is alive again — and has his day.
#[test]
fn a_dog_still_lying_in_his_place_is_alive_again_on_its_turn() {
    let mut people = People::from_records(&[record(CharClass::Dog, 1, 0), record(CharClass::Mercenary, 1, 0)], &Table, &settings());
    people.arrival = true;
    assert!(people.fell(0).is_some());
    assert!(people.get(0).unwrap().lying);
    assert!(people.fell(1).is_some());
    assert!(!people.revive_lying_creature(1), "a man stays dead");
    assert!(people.revive_lying_creature(0));
    let dog = people.get(0).unwrap();
    assert!(!dog.dead && !dog.lying);
    assert_eq!(dog.needs.health, dog.full_health);
    // Laid out where the player watched, he would have been gone.
    people.laid_out(3);
    assert!(!people.revive_lying_creature(1));
}

/// A wandering trader who walks on stands at his new place's entry,
/// stopped (`Trader.NextSellLocation`: `Position = Location.EntryPoint;
/// Path.Stop(Position)`, `Trader.cs:131-153`).
#[test]
fn a_trader_who_walks_on_stands_at_the_entry() {
    let mut people = three();
    let entry = burngfx::game::Point::new(40, 50);
    people.move_person(0, 4);
    people.stand_at_entry(0, entry);
    let p = people.get(0).unwrap();
    assert_eq!((p.location, p.at, p.move_to), (4, entry, entry));
}
