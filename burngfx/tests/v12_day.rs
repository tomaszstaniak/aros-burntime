//! v1.2's employed day (`GameRules.TurnOriginalEmployed`,
//! `Character.TurnExtendedEmployed`) for a travelling group.

use burngfx::game::{CharacterNeeds, Companion, ItemBag, ItemFacts, Party};
use burngfx::rules::{Survival, WaterOutput};
use burngfx::v12_day::*;

/// 0 maggots (food 3), 1 snake (food 7), 2 bottle (water 2 → 3), 3 empty
/// bottle, 4 canteen (water 5 → 3).
struct Table;
impl ItemFacts for Table {
    fn food(&self, k: usize) -> i32 { [3, 7, 0, 0, 0][k] }
    fn water(&self, k: usize) -> i32 { [0, 0, 2, 0, 5][k] }
    fn full(&self, _: usize) -> Option<usize> { None }
    fn empty(&self, k: usize) -> Option<usize> { if k == 2 || k == 4 { Some(3) } else { None } }
    fn kind_named(&self, _: &str) -> Option<usize> { None }
    fn name_of(&self, k: usize) -> Option<&str> { ["maggots", "snake", "bottle", "empty", "canteen"].get(k).copied() }
}

fn needs(food: i32, water: i32, health: i32) -> CharacterNeeds {
    CharacterNeeds { food, max_food: 9, water, max_water: 5, health }
}

const CLASSIC: Upkeep = Upkeep { survival: Survival::DosDailyConsumption, stabilization: 0, doctor: false, threshold: None, travelling: false };
const EXTENDED: Upkeep = Upkeep { survival: Survival::RemasterSupplyPool, stabilization: 1, doctor: false, threshold: None, travelling: false };

/// Classic spends first and refills only below nought: one level of each
/// left is simply spent, and nothing in the pack is touched.
#[test]
fn classic_spends_before_it_refills() {
    let mut leader = needs(1, 1, 60);
    let mut pack = ItemBag::with(6, &[1, 2]);
    let mut party = Party::new();
    let out = party_day(&mut leader, &mut pack, &mut party, &Table, CLASSIC);
    assert!(!out.leader_died);
    assert_eq!((leader.food, leader.water, leader.health), (0, 0, 60), "under 70: no healing");
    assert_eq!(pack.len(), 2);
}

/// At nought: the cheapest water and the smallest food, into this one man,
/// and no damage.
#[test]
fn classic_refills_one_man_from_the_cheapest() {
    let mut leader = needs(0, 0, 80);
    let mut pack = ItemBag::with(6, &[1, 4, 2, 0]);
    let mut party = Party::new();
    party_day(&mut leader, &mut pack, &mut party, &Table, CLASSIC);
    // Heal first: 80 → 82. Water -1 → the bottle (2, cheaper than the
    // canteen's 5): 1. Food -1 → the maggots (3, smaller than the snake): 2.
    assert_eq!((leader.health, leader.water, leader.food), (82, 1, 2));
    assert_eq!(pack.iter().collect::<Vec<_>>(), vec![1, 4, 3], "the bottle emptied in place, the maggots eaten");
}

#[test]
fn classic_goes_without_for_twenty_five_each() {
    let mut leader = needs(0, 0, 45);
    let mut pack = ItemBag::with(6, &[]);
    let mut party = Party::new();
    let out = party_day(&mut leader, &mut pack, &mut party, &Table, CLASSIC);
    assert!(out.leader_died, "45 - 25 - 25");
}

/// Extended: an empty level fills the whole group from the smallest food
/// (spread a level at a time) and the largest water.
#[test]
fn extended_fills_the_group_from_the_pool() {
    let mut leader = needs(0, 0, 100);
    let mut pack = ItemBag::with(6, &[1, 0, 2, 4]);
    let mut party = Party::new();
    party.companions.push(Companion { needs: needs(0, 3, 100), ..Companion::new(1) });
    party_day(&mut leader, &mut pack, &mut party, &Table, EXTENDED);
    // The leader's turn: maggots (3) spread over both at nought → 2 and 1;
    // the canteen (5) over the leader at nought first; then his own day.
    assert!(pack.iter().all(|k| k != 0), "the maggots were eaten");
    assert!(pack.iter().any(|k| k == 3), "a container emptied");
    assert_eq!(leader.health, 100);
}

/// The doctor's stabilization in Extended: below fifty, fed and watered.
#[test]
fn extended_doctor_stabilizes_below_fifty() {
    let mut leader = needs(5, 3, 40);
    let mut pack = ItemBag::with(6, &[]);
    let mut party = Party::new();
    party_day(&mut leader, &mut pack, &mut party, &Table, Upkeep { doctor: true, ..EXTENDED });
    assert_eq!(leader.health, 41);
    let mut leader = needs(5, 3, 50);
    party_day(&mut leader, &mut pack, &mut party, &Table, Upkeep { doctor: true, ..EXTENDED });
    assert_eq!(leader.health, 54, "fifty with a doctor: +4");
}

/// The Amiga opponent's own threshold, 50.
#[test]
fn a_players_own_threshold() {
    assert_eq!(healing_threshold(false, None), 70);
    assert_eq!(healing_threshold(false, Some(50)), 50);
    assert_eq!(healing_threshold(true, Some(60)), 50);
    let mut leader = needs(5, 3, 55);
    let mut pack = ItemBag::with(6, &[]);
    party_day(&mut leader, &mut pack, &mut Party::new(), &Table, Upkeep { threshold: Some(50), ..CLASSIC });
    assert_eq!(leader.health, 57);
}

#[test]
fn a_companion_who_dies_leaves_the_group_at_once() {
    let mut leader = needs(5, 5, 100);
    let mut pack = ItemBag::with(6, &[]);
    let mut party = Party::new();
    party.companions.push(Companion { needs: needs(0, 0, 30), pack: ItemBag::with(6, &[1]), ..Companion::new(1) });
    party.companions.push(Companion { needs: needs(0, 5, 100), ..Companion::new(2) });
    let out = party_day(&mut leader, &mut pack, &mut party, &Table, CLASSIC);
    // Companion 1: water -1, nothing to drink → 5; food: his own snake.
    // Companion 2 finds nothing: the snake went to the first.
    assert_eq!(party.companions.len(), 2, "{:?}", out.deaths);
    let mut party2 = Party::new();
    party2.companions.push(Companion { needs: needs(0, 0, 20), ..Companion::new(1) });
    let out = party_day(&mut leader, &mut pack, &mut party2, &Table, CLASSIC);
    assert_eq!(out.deaths.len(), 1);
    assert!(party2.companions.is_empty());
}

#[test]
fn water_output_and_boss_experience() {
    assert_eq!(water_output(WaterOutput::AmigaMinimum, 2, false, true), 6);
    assert_eq!(water_output(WaterOutput::AmigaMinimum, 8, false, true), 12);
    assert_eq!(water_output(WaterOutput::AmigaMinimum, 2, true, false), 3);
    assert_eq!(water_output(WaterOutput::RemasterFixed, 2, true, false), 4);
    assert_eq!(water_output(WaterOutput::RemasterFixed, 2, true, true), 7);
    assert_eq!(water_output(WaterOutput::DosProportional, 2, false, true), 4);
    assert_eq!(camp_boss_experience(37, 2), 43);
    assert_eq!(camp_boss_experience(90, 5), 99);
}

// -- the location's day ------------------------------------------------------

use burngfx::game::{Camp, Pump, Storage, WaterSource};
use burngfx::production::Productions;
use burngfx::rules::FoodProduction;

/// Rooms as lists of kinds; tools counted by name.
struct Rooms {
    rooms: Vec<Vec<usize>>,
    tools: usize,
    stored: Vec<usize>,
}

impl Storage for Rooms {
    fn count_in_rooms(&self, _: usize, kind: usize) -> usize {
        self.rooms.iter().flatten().filter(|&&k| k == kind).count()
    }
    fn tools(&self, _: usize, _: usize) -> usize { self.tools }
    fn any_room_has_space(&self, _: usize) -> bool { self.rooms.iter().any(|r| r.len() < 4) }
    fn store(&mut self, _: usize, kind: usize) -> bool {
        self.stored.push(kind);
        if let Some(r) = self.rooms.iter_mut().find(|r| r.len() < 4) { r.push(kind) }
        true
    }
    fn pump_at(&self, _: usize) -> Pump { Pump::None }
    fn room_items(&self, _: usize) -> Vec<(usize, usize, usize)> {
        self.rooms.iter().enumerate().flat_map(|(r, b)| b.iter().enumerate().map(move |(s, &k)| (r, s, k))).collect()
    }
    fn take_from_room(&mut self, _: usize, room: usize, slot: usize) -> Option<usize> {
        let b = self.rooms.get_mut(room)?;
        (slot < b.len()).then(|| b.remove(slot))
    }
    fn replace_in_room(&mut self, _: usize, room: usize, slot: usize, kind: usize) { self.rooms[room][slot] = kind }
    fn valid_productions(&self, _: usize) -> Vec<usize> { vec![0] }
}

/// Items for the camp tests: 0 maggots (the produce), 1 knife (the tool).
struct Camping;
impl ItemFacts for Camping {
    fn food(&self, k: usize) -> i32 { if k == 0 { 3 } else { 0 } }
    fn water(&self, _: usize) -> i32 { 0 }
    fn full(&self, _: usize) -> Option<usize> { None }
    fn empty(&self, _: usize) -> Option<usize> { None }
    fn kind_named(&self, n: &str) -> Option<usize> { ["item_maggots", "item_knife"].iter().position(|x| *x == n) }
    fn name_of(&self, k: usize) -> Option<&str> { ["item_maggots", "item_knife"].get(k).copied() }
}

fn knife() -> Productions {
    Productions::parse("[item_knife]\nmaxcombination=1\namount=1 2\namount2=1 3\nproduce=item_maggots\n")
}

fn world(survival: Survival) -> V12World {
    V12World {
        survival,
        stabilization: if survival == Survival::RemasterSupplyPool { 1 } else { 0 },
        water_output: WaterOutput::AmigaMinimum,
        food_production: FoodProduction::DosFeedThenStore,
        thresholds: [None; 4],
        doctors: Vec::new(),
        dangers: Vec::new(),
        gas_immune: Vec::new(),
        radiation_immune: Vec::new(),
        faces: Vec::new(),
        technicians: Vec::new(),
        technician_bonus: 0,
        cities: Vec::new(),
    }
}

fn camp_of(food: i32, water: i32, health: i32) -> Camp {
    Camp {
        owner: Some(0),
        residents: vec![Companion { needs: needs(food, water, health), ..Companion::new(3) }],
        production: Some(0),
        ..Camp::default()
    }
}

/// Classic: the day's production feeds the camper first; his water comes
/// out of the reserve, a level at a time, only once he has none.
#[test]
fn a_classic_camp_feeds_its_man_then_the_reserve_waters_him() {
    let mut camp = camp_of(0, 0, 100);
    let mut spring = WaterSource::classic(5, 2, 10);
    let mut rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    let dead = location_turn(0, &mut camp, Some(&mut spring), &mut rooms, &knife(), &Camping, &world(Survival::DosDailyConsumption));
    assert!(dead.is_empty());
    let n = camp.residents[0].needs;
    // Fed 1 of the 2 a day, then spent: food 0. Water -1 → the reserve.
    assert_eq!((n.food, n.water, n.health), (0, 0, 100));
    assert_eq!(spring.reserve, 4);
    assert!((camp.progress - 1.0).abs() < 1e-6, "the other point towards the next maggots");
}

/// The rest of the day's food is stored by value: three days of one spare
/// point make maggots (food 3).
#[test]
fn production_stores_what_the_camp_does_not_eat() {
    let mut camp = camp_of(9, 5, 100);
    let mut rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    for _ in 0..2 {
        location_turn(0, &mut camp, None, &mut rooms, &knife(), &Camping, &world(Survival::DosDailyConsumption));
    }
    // Day 1: he is full, so both points are spare (2), and he spends one.
    // Day 2: he is fed one back, the other makes 3 — maggots, nought left.
    assert_eq!(rooms.stored, vec![0]);
    assert!(camp.progress.abs() < 1e-6, "{}", camp.progress);
}

/// Extended: a camper drinks his fill from the spring (`group.Drink(null,
/// Reserve)`), then spends a level.
#[test]
fn an_extended_camper_drinks_from_the_spring() {
    let mut camp = camp_of(5, 0, 100);
    let mut spring = WaterSource::classic(10, 2, 10);
    let mut rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    location_turn(0, &mut camp, Some(&mut spring), &mut rooms, &knife(), &Camping, &world(Survival::RemasterSupplyPool));
    assert_eq!(camp.residents[0].needs.water, 4, "filled to five, then a level spent");
    assert_eq!(spring.reserve, 5);
}

/// Nobody's place makes nothing, and a camp whose last man dies is free.
#[test]
fn the_last_camper_dying_frees_the_place() {
    let mut camp = camp_of(0, 0, 40);
    let mut rooms = Rooms { rooms: vec![vec![]], tools: 0, stored: Vec::new() };
    // No tools and the knife's `amount=1 …`: one a day still feeds him; no
    // water anywhere: 40 - 25 < 25… he lives one day at 15.
    let dead = location_turn(0, &mut camp, None, &mut rooms, &knife(), &Camping, &world(Survival::DosDailyConsumption));
    assert!(dead.is_empty());
    let dead = location_turn(0, &mut camp, None, &mut rooms, &knife(), &Camping, &world(Survival::DosDailyConsumption));
    assert_eq!(dead.len(), 1);
    assert!(dead[0].camp_lost);
    assert_eq!(camp.owner, None);
}

/// A player's own choice of production stands while it makes anything.
#[test]
fn a_chosen_production_is_kept_while_it_produces() {
    let mut camp = Camp { manual_production: true, ..camp_of(9, 5, 100) };
    let mut rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    location_turn(0, &mut camp, None, &mut rooms, &knife(), &Camping, &world(Survival::DosDailyConsumption));
    assert!(camp.manual_production);
    let mut idle = Camp { manual_production: true, production: None, ..camp_of(9, 5, 100) };
    location_turn(0, &mut idle, None, &mut rooms, &knife(), &Camping, &world(Survival::DosDailyConsumption));
    assert!(!idle.manual_production, "nothing made: chosen automatically again");
    assert_eq!(idle.production, Some(0));
}

fn with_technician(bonus: i32) -> V12World {
    V12World { technicians: vec![50], technician_bonus: bonus, ..world(Survival::DosDailyConsumption) }
}

fn technician_camp(health: i32) -> Camp {
    Camp {
        owner: Some(0),
        residents: vec![Companion { needs: needs(9, 5, health), origin: Some(50), ..Companion::new(3) }],
        production: Some(0),
        ..Camp::default()
    }
}

/// `GetFoodProductionRate`'s technician bonus: a living one of the owner's,
/// camped, no city, on a production that makes anything; never below
/// nought. v1.2's presets all set it to nought.
#[test]
fn the_technicians_bonus() {
    let rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    let rate = |camp: &Camp, v: &V12World, p: &Productions| food_rate(0, camp, &rooms, p, &Camping, v, 0);
    let k = knife();
    assert_eq!(rate(&technician_camp(100), &with_technician(2), &k), 4, "2 a day with one knife, and the bonus");
    assert_eq!(rate(&technician_camp(100), &with_technician(0), &k), 2, "the presets' nought");
    assert_eq!(rate(&camp_of(9, 5, 100), &with_technician(2), &k), 2, "no technician");
    assert_eq!(rate(&technician_camp(0), &with_technician(2), &k), 2, "a dead one counts for nothing");
    assert_eq!(rate(&technician_camp(100), &V12World { cities: vec![true], ..with_technician(2) }, &k), 2, "not in a city");
    assert_eq!(rate(&technician_camp(100), &with_technician(-3), &k), 2, "never below nought");
    let idle = Productions::parse("[item_knife]\nmaxcombination=1\namount=0 0\namount2=0 0\nproduce=item_maggots\n");
    assert_eq!(rate(&technician_camp(100), &with_technician(2), &idle), 0, "nothing made, no bonus");
    let nobody = Camp { owner: None, ..technician_camp(100) };
    assert_eq!(rate(&nobody, &with_technician(2), &k), 0, "nobody's place makes nothing");
}

/// The same figure feeds the camp's day: with the bonus, the fed camper's
/// three spare points make maggots the first day.
#[test]
fn the_bonus_reaches_the_camps_day() {
    let mut camp = Camp { residents: vec![Companion { needs: needs(8, 5, 100), origin: Some(50), ..Companion::new(3) }], ..technician_camp(100) };
    let mut rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    location_turn(0, &mut camp, None, &mut rooms, &knife(), &Camping, &with_technician(2));
    assert_eq!(rooms.stored, vec![0], "4 a day: one fed to him, three stored as maggots");
    let mut camp = Camp { residents: vec![Companion { needs: needs(8, 5, 100), origin: Some(50), ..Companion::new(3) }], ..technician_camp(100) };
    let mut rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    location_turn(0, &mut camp, None, &mut rooms, &knife(), &Camping, &with_technician(0));
    assert!(rooms.stored.is_empty(), "without it, 2 a day: one fed, one towards the next");
}

// -- `World.Turn`'s loop over a group, and the doctor --------------------------

/// `for (j = 0; j < Party.Count; j++) Party[j].Turn()` (`UP/Logic/World.cs:
/// 128-132`) over a list `Die` → `Dismiss` shortens as it goes: A dies of
/// hunger and is gone, B shifts into his index, and the loop moves past
/// him — B has no day: his water stays nought, his health 70, his bottle
/// full (rules audit 2.4 (a), D3).
#[test]
fn after_a_companion_dies_the_next_has_no_day() {
    let mut leader = needs(5, 5, 100);
    let mut pack = ItemBag::new(6);
    let mut party = Party::new();
    party.companions.push(Companion { needs: needs(0, 5, 20), ..Companion::new(1) });
    party.companions.push(Companion { needs: needs(5, 0, 70), pack: ItemBag::with(6, &[2]), ..Companion::new(2) });
    let out = party_day_with(&mut leader, &mut pack, &mut party, &Table, CLASSIC, None, Some(&[]));
    assert_eq!(out.deaths.len(), 1, "A dies of hunger");
    let b = &party.companions[0];
    assert_eq!((b.needs.water, b.needs.health), (0, 70));
    assert_eq!(b.pack.iter().collect::<Vec<_>>(), vec![2], "his bottle untouched");
}

/// The leader's death dismisses nobody (`Die`: only `Player.Character !=
/// this`), so the companions still have their day (`World.cs:97-132`): one
/// with no food eats the snake in his pack (rules audit 2.4 (b), D4). What
/// the leader carried is on the ground at once (`Items.MoveTo`), so his
/// maggots — the group's smallest food — are not the ones eaten.
#[test]
fn when_the_leader_dies_the_companions_still_have_their_day() {
    let mut leader = needs(0, 0, 20);
    let mut pack = ItemBag::with(6, &[0]);
    let mut party = Party::new();
    party.companions.push(Companion { needs: needs(0, 5, 100), pack: ItemBag::with(6, &[1]), ..Companion::new(1) });
    let out = party_day_with(&mut leader, &mut pack, &mut party, &Table, CLASSIC, None, Some(&[]));
    assert!(out.leader_died, "no water: 20 - 25");
    assert_eq!(out.leader_dropped.map(|b| b.iter().collect::<Vec<_>>()), Some(vec![0]));
    assert!(pack.is_empty());
    let c = &party.companions[0];
    assert_eq!(c.needs.food, 6, "the snake (7), less the day's level");
    assert!(c.pack.is_empty());
}

/// `HasLocalDoctor` is asked for each member at his turn
/// (`Character.cs:804-807`): with the doctor alive the leader at 60 heals
/// four; the doctor then dies, the man after him has no day, and the next,
/// at 60, heals nothing — Classic's 70 without a doctor (rules audit 2.4
/// (d), D6).
#[test]
fn a_doctor_who_dies_heals_nobody_after_him() {
    let doctors = [77];
    let mut leader = needs(5, 5, 60);
    let mut pack = ItemBag::new(6);
    let mut party = Party::new();
    party.companions.push(Companion { needs: needs(0, 0, 10), origin: Some(77), ..Companion::new(1) });
    party.companions.push(Companion { needs: needs(5, 5, 60), ..Companion::new(2) });
    party.companions.push(Companion { needs: needs(5, 5, 60), ..Companion::new(3) });
    let out = party_day_with(&mut leader, &mut pack, &mut party, &Table, CLASSIC, None, Some(&doctors));
    assert_eq!(out.deaths.len(), 1);
    assert_eq!(leader.health, 64);
    assert_eq!(party.companions[0].needs.health, 60, "skipped");
    assert_eq!(party.companions[1].needs.health, 60, "no doctor left, under 70");
}

/// In a camp the doctor is asked the same way: a dying doctor camped there
/// first leaves the camper after him without the +4. (A camper's death does
/// not shorten `Location.Characters`, so nobody is skipped there.)
#[test]
fn in_a_camp_a_doctor_who_died_heals_nobody_after_him() {
    let mut camp = Camp {
        owner: Some(0),
        residents: vec![
            Companion { needs: needs(0, 0, 10), origin: Some(77), ..Companion::new(3) },
            Companion { needs: needs(5, 5, 60), ..Companion::new(4) },
        ],
        production: None,
        ..Camp::default()
    };
    let mut rooms = Rooms { rooms: vec![vec![]], tools: 0, stored: Vec::new() };
    let v12 = V12World { doctors: vec![77], ..world(Survival::DosDailyConsumption) };
    let dead = location_turn(0, &mut camp, None, &mut rooms, &knife(), &Camping, &v12);
    assert_eq!(dead.len(), 1);
    assert_eq!(camp.residents[0].needs.health, 60);
}

/// A place nobody holds (`RefreshFoodProductionSelection`,
/// `UP/Logic/Location.cs:384-403`): its selection goes back to automatic
/// and `ProduceFood(0)` puts its progress to nought, every day (rules audit
/// 2.5, D7).
#[test]
fn a_place_nobody_holds_goes_back_to_automatic_with_no_progress() {
    let mut camp = Camp { owner: None, production: Some(0), progress: 2.0, manual_production: true, ..Camp::default() };
    let mut rooms = Rooms { rooms: vec![vec![1]], tools: 1, stored: Vec::new() };
    production_turn(0, &mut camp, &mut rooms, &knife(), &Camping, &world(Survival::DosDailyConsumption));
    assert!(!camp.manual_production);
    assert_eq!(camp.progress, 0.0);
    assert!(rooms.stored.is_empty());
}

/// Bottles left in a spring room are filled each night (`Location.cs:
/// 457-463`, `WaterSource.RefillItem`), after the people's day and before
/// the cap (rules audit 1.1). Bottle 3 empty, its full form 4 holds five.
#[test]
fn a_spring_room_fills_its_bottles_each_night_before_the_cap() {
    struct Bottles;
    impl ItemFacts for Bottles {
        fn food(&self, _: usize) -> i32 { 0 }
        fn water(&self, k: usize) -> i32 { if k == 4 { 5 } else { 0 } }
        fn full(&self, k: usize) -> Option<usize> { (k == 3).then_some(4) }
        fn empty(&self, k: usize) -> Option<usize> { (k == 4).then_some(3) }
        fn kind_named(&self, _: &str) -> Option<usize> { None }
        fn name_of(&self, _: usize) -> Option<&str> { None }
    }
    let night = |reserve: i32, capacity: i32| {
        let mut spring = WaterSource::classic(reserve, 2, capacity);
        spring.reserve += 2; // `BeginTurn`
        let mut room = ItemBag::with(6, &[3]);
        refill_spring_room(&mut spring, &mut room, &Bottles);
        spring.end_turn();
        (room.get(0), spring.reserve)
    };
    assert_eq!(night(10, 12), (Some(4), 7));
    assert_eq!(night(2, 12), (Some(3), 4), "too little: it stays empty");
    assert_eq!(night(10, 10), (Some(4), 7), "the fill comes before the cap");
}

/// **A danger's fraction at the day** (`TurnOriginalEmployed`,
/// `GameRules.cs:273-297`): `Health >= threshold` reads the getter, and
/// `Health += 2`, `Health -= 25` write through the setter, which drops the
/// fraction. 70.65 (70 and 0.65 above) reads 70 and heals (threshold 70)
/// to 72, the fraction gone; 69.65: no heal, nothing written, the fraction
/// kept; going without water, 48.65 ends on 23.
#[test]
fn the_day_reads_and_spends_a_danger_s_fraction_as_v1_2_does() {
    use burngfx::danger::Exposure;
    let carry = || Exposure { carried: 65 * burngfx::danger::UNIT / 100, lifted: false };
    let run = |health: i32, water: i32| {
        let mut leader = needs(5, water, health);
        let mut e = carry();
        party_day_exposed(&mut leader, &mut e, &mut ItemBag::with(6, &[]), &mut Party::new(), &Table, CLASSIC, None, Some(&[]));
        (leader.health, e.carried)
    };
    assert_eq!(run(70, 3), (72, 0));
    assert_eq!(run(69, 3), (69, carry().carried));
    assert_eq!(run(48, 0), (23, 0));
    // A companion carries his own.
    let mut c = Companion::new(1);
    (c.needs, c.exposure) = (needs(5, 0, 48), carry());
    let mut party = Party::new();
    party.companions.push(c);
    party_day_exposed(&mut needs(5, 3, 60), &mut Exposure::default(), &mut ItemBag::with(6, &[]), &mut party, &Table, CLASSIC, None, Some(&[]));
    assert_eq!((party.companions[0].needs.health, party.companions[0].exposure.carried), (23, 0));
}
