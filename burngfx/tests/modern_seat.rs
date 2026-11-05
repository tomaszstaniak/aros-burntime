//! `modern_ai::seat` and `modern_ai::resolve` on the game's own world: the
//! real map from `gam.dat`, real camps, rooms, ground and people. A fight
//! for a camp, and the last chance, are checked by what they leave changed
//! in the world — owners, residents, packs, the reserve, the ground, the
//! road — not by a returned decision alone.

use burngfx::combat::Kit;
use burngfx::game::{Camps, CharacterNeeds, Companion, ItemBag, Respawns, RoomItems, Springs};
use burngfx::gamdat::{self, Gamdat};
use burngfx::hire::People;
use burngfx::items::ItemTypes;
use burngfx::modern_ai::policy::{AiConfig, AiPolicy};
use burngfx::modern_ai::resolve::{self, Result};
use burngfx::modern_ai::seat::Seat;
use burngfx::modern_ai::state::ModernAi;
use burngfx::modern_ai::upkeep;
use burngfx::production::Productions;
use burngfx::rival::Rival;
use burngfx::rules::{FoodProduction, RuleBook, Survival, WaterOutput};
use burngfx::spawn::{Dropped, Rng};
use burngfx::trade::Prices;
use burngfx::v12_day::V12World;

fn read(f: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/v1.2/rules/extended/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn gam() -> Gamdat {
    let bytes = std::fs::read(format!("{}/testdata/burntime.pak", env!("CARGO_MANIFEST_DIR"))).expect("testdata pack");
    let pak = burngfx::pak::ClassicPak::open(&bytes).expect("pak");
    gamdat::parse(pak.get("burn_gfx/gam.dat").expect("gam.dat")).expect("parse")
}

fn policy(die_when_trapped: bool) -> AiPolicy {
    let s = |n: u8| {
        format!(
            "[{n}]\nrecruit_experience = 0 -1\nslump_material_grants = 0\ntravel_group_size = 2\nattack_group_size = 3\n\
             minimum_attack_ratio = 1.05\ncombat_loot_limit = 1\ncontested_camp_memory_turns = 16\nattack_cooldown_turns = 2\n\
             failed_attack_memory_turns = 20\nprogressing_attack_retry_turns = 3\nthreat_radius = 1\ndie_when_trapped = {die_when_trapped}\n"
        )
    };
    AiConfig::parse(&format!("{}{}{}", s(0), s(1), s(2))).unwrap().policy(1).clone()
}

fn v12(n: usize) -> V12World {
    V12World {
        survival: Survival::RemasterSupplyPool,
        stabilization: 1,
        water_output: WaterOutput::AmigaMinimum,
        food_production: FoodProduction::DosFeedThenStore,
        thresholds: [None; 4],
        doctors: vec![],
        dangers: vec![],
        gas_immune: vec![],
        radiation_immune: vec![],
        faces: vec![],
        technicians: vec![],
        technician_bonus: 0,
        cities: vec![false; n],
    }
}

/// Everything a seat borrows, owned by the test.
struct World {
    gam: Gamdat,
    types: ItemTypes,
    prices: Prices,
    productions: Productions,
    book: RuleBook,
    constructions: burngfx::construction::Constructions,
    v12: V12World,
    room_ids: Vec<Vec<u8>>,
    people: People,
    camps: Camps,
    rooms: RoomItems,
    ground: Vec<Dropped>,
    springs: Springs,
    respawns: Respawns,
    traders: burngfx::trader::Traders,
    stock_lists: burngfx::trader::StockLists,
    rival: Rival,
    ai: ModernAi,
}

impl World {
    fn new() -> World {
        let gam = gam();
        let types = ItemTypes::parse_v12(&read("items.txt")).unwrap();
        let settings = burngfx::settings::GameSettings::parse("[0]\nnpc_respawn = 10\n", 0);
        let people = People::from_records(&gam.characters, &types, &settings);
        let n = gam.locations.len();
        World {
            v12: v12(n),
            room_ids: vec![vec![4, 0]; n],
            people,
            productions: Productions::parse(&read("production.txt")),
            book: RuleBook::parse(&read("game.txt")),
            constructions: burngfx::construction::Constructions::parse(&std::fs::read_to_string(format!("{}/testdata/construction.txt", env!("CARGO_MANIFEST_DIR"))).unwrap()),
            prices: Prices::parse("3 = 27\n"),
            camps: Camps::default(),
            rooms: RoomItems::default(),
            ground: vec![],
            springs: Springs::new(),
            respawns: Respawns::default(),
            traders: burngfx::trader::Traders::default(),
            stock_lists: burngfx::trader::StockLists::default(),
            rival: Rival::new(0, 0),
            ai: ModernAi::new(1, 5),
            gam,
            types,
        }
    }
    fn k(&self, n: &str) -> usize {
        self.types.index_of(n).unwrap_or_else(|| panic!("{n}"))
    }
    fn seat(&mut self) -> Seat<'_> {
        Seat {
            rival: &mut self.rival,
            ai: &mut self.ai,
            day: 10,
            difficulty: 1,
            gam: &self.gam,
            people: &mut self.people,
            camps: &mut self.camps,
            rooms: &mut self.rooms,
            ground: &mut self.ground,
            springs: &mut self.springs,
            types: &self.types,
            prices: &self.prices,
            productions: &self.productions,
            book: &self.book,
            constructions: &self.constructions,
            v12: &self.v12,
            room_ids: &self.room_ids,
            humans: &[],
            others: vec![],
            human_here: None,
            respawns: &mut self.respawns,
            traders: &mut self.traders,
            stock_lists: &self.stock_lists,
            npc_respawn: 10,
            camp_cache: None,
            needs_cache: None,
            buried: vec![],
            trade_city_cache: None,
            fights: vec![],
        }
    }
    /// Two places that are not cities, a road apart.
    fn pair(&self) -> (usize, usize) {
        for (a, l) in self.gam.locations.iter().enumerate() {
            if l.is_city {
                continue;
            }
            if let Some(r) = l.roads.iter().find(|r| r.days > 0 && !self.gam.locations[r.neighbour].is_city) {
                return (a, r.neighbour);
            }
        }
        panic!("no pair");
    }
    fn needs(health: i32) -> CharacterNeeds {
        CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health }
    }
    /// Our group at `at`, come from `from`: the leader and two followers,
    /// each with what is named.
    fn group(&mut self, at: usize, from: usize, leader: &[&str], followers: &[&[&str]]) {
        let mut r = Rival::new(0, at);
        r.previous = Some(from);
        r.needs = World::needs(100);
        r.experience = 20;
        r.pack = ItemBag::with(6, &leader.iter().map(|n| self.k(n)).collect::<Vec<_>>());
        for f in followers {
            let id = r.party.recruit();
            let pack = ItemBag::with(6, &f.iter().map(|n| self.k(n)).collect::<Vec<_>>());
            let c = r.party.companions.iter_mut().find(|c| c.id == id).unwrap();
            *c = Companion { needs: World::needs(100), experience: 20, pack, ..Companion::new(id) };
        }
        self.rival = r;
    }
    fn camp(&mut self, l: usize, owner: usize, health: i32, items: &[&str]) {
        let pack = ItemBag::with(6, &items.iter().map(|n| self.k(n)).collect::<Vec<_>>());
        let id = 500 + l;
        self.camps.join(l, owner, Companion { needs: World::needs(health), experience: 20, pack, ..Companion::new(id) });
    }
}

#[test]
fn a_weak_camp_is_taken_and_a_guard_left_in_it() {
    let mut w = World::new();
    let (theirs, ours) = w.pair();
    w.camp(ours, 0, 100, &[]);
    // One guard of theirs, nearly dead, with a knife.
    w.camp(theirs, 1, 5, &["item_knife"]);
    w.group(theirs, ours, &["item_axe"], &[&["item_axe"], &["item_axe"]]);
    w.ai.target = Some(theirs);
    let policy = policy(false);
    let got = resolve::resolve(&mut w.seat(), &policy, true, false, &mut Rng::seeded(4));
    // `Resolve`: defenders gone; the weakest follower (the first of equals)
    // stays as the guard; the place is ours; the target and its failure
    // memory are gone; the capture remembered and the pause set.
    assert!(matches!(got, Result::Captured { guard: Some(0), .. }), "{got:?}");
    let camp = w.camps.get(theirs).unwrap();
    assert_eq!((camp.owner, camp.residents.len()), (Some(0), 1));
    assert_eq!(w.rival.party.len(), 1, "one follower left in the group");
    assert_eq!((w.ai.target, w.ai.contested, w.ai.post_capture_pause), (None, Some((theirs, 26)), 2));
    // Their guard's knife: dropped where he fell, taken as loot into the
    // reserve (`combat_loot_limit` 1; a knife is reserve goods) — and then,
    // as `JoinCamp` installs a tool from the reserve (`GetBestTrap` counts
    // the knife among them), into the camp's rooms when the place makes
    // maggots; else it stays in the reserve.
    let knife = w.k("item_knife");
    let makes_maggots = w.gam.locations[theirs].available_products.iter().flatten().any(|&p| w.productions.get(p).is_some_and(|p| p.tool == "item_knife"));
    let in_rooms = (0..2).filter_map(|r| w.rooms.get(theirs, r)).any(|b| b.iter().any(|k| k == knife));
    assert_eq!((in_rooms, w.ai.reserve_count(knife)), if makes_maggots { (true, 0) } else { (false, 1) });
    assert!(!w.ground.iter().any(|d| d.location == theirs && d.kind == knife), "nothing left lying");
    // The defender's death is settled: due back in ten days.
    assert!(w.ai.retries.is_empty(), "captured: the retry record is cleared");
}

#[test]
fn a_failed_attack_falls_back_the_way_it_came() {
    let mut w = World::new();
    let (theirs, ours) = w.pair();
    w.camp(ours, 0, 100, &[]);
    // A guard of theirs who will not fall: a thousand health and an axe.
    w.camp(theirs, 1, 1000, &["item_axe"]);
    w.group(theirs, ours, &["item_axe"], &[&[], &[]]);
    w.ai.target = Some(theirs);
    let policy = policy(false);
    let got = resolve::resolve(&mut w.seat(), &policy, true, false, &mut Rng::seeded(4));
    // `Resolve`: not defeated; the target dropped; the failure remembered
    // for the camp; and the group on the road back to our camp — the only
    // road out of theirs.
    match got {
        Result::Failed { retreat, .. } => assert_eq!(retreat, Some(ours)),
        other => panic!("{other:?}"),
    }
    assert_eq!(w.rival.journey.map(|j| j.to), Some(ours));
    assert_eq!(w.ai.target, None);
    assert_eq!(w.ai.failed_attack.as_ref().map(|f| f.camp), Some(theirs));
    assert_eq!(w.camps.get(theirs).and_then(|c| c.owner), Some(1), "still theirs");
    assert!(w.rival.needs.health < 100 || w.rival.party.companions.iter().any(|c| c.needs.health < 100), "wounds written back");
}

/// A city, every place that is not a city held by player 1: trapped.
fn trapped(w: &mut World) -> usize {
    let city = (0..w.gam.locations.len()).find(|&l| w.gam.locations[l].is_city && w.gam.locations[l].roads.iter().any(|r| !w.gam.locations[r.neighbour].is_city)).unwrap();
    for l in 0..w.gam.locations.len() {
        if !w.gam.locations[l].is_city {
            w.camp(l, 1, 50, &[]);
        }
    }
    let mut r = Rival::new(0, city);
    r.needs = World::needs(100);
    w.rival = r;
    city
}

#[test]
fn trapped_on_easy_the_leader_dies() {
    let mut w = World::new();
    let city = trapped(&mut w);
    w.rival.pack = ItemBag::with(6, &[w.k("item_knife")]);
    // `LastChanceCombat`: no escape, hostile camps around, die_when_trapped.
    let got = resolve::last_chance(&mut w.seat(), &policy(true), true, &mut Rng::seeded(1));
    assert_eq!(got, Some(Result::Died { rounds: 0 }));
    assert_eq!(w.rival.needs.health, 0);
    // `Health = 0` runs `Die` there and then: his knife on the ground at
    // his `Position`, the entry (`Character.cs:197-200, 454-461`).
    assert!(w.rival.pack.is_empty());
    assert_eq!(w.ground.iter().map(|d| (d.location, d.at, d.kind)).collect::<Vec<_>>(),
               vec![(city, w.gam.locations[city].entry_point, w.k("item_knife"))]);
}

#[test]
fn trapped_otherwise_it_marches_on_the_weakest_camp() {
    let mut w = World::new();
    let city = trapped(&mut w);
    let got = resolve::last_chance(&mut w.seat(), &policy(false), true, &mut Rng::seeded(1));
    // The target chosen is one of the camps blocking the way, and the group
    // is on the road toward it.
    let target = w.ai.last_chance_target.expect("a target");
    assert!(matches!(got, Some(Result::Failed { .. })));
    assert_eq!(w.camps.get(target).and_then(|c| c.owner), Some(1));
    assert!(w.rival.journey.is_some_and(|j| j.from == city), "{:?}", w.rival.journey);
    assert!(w.rival.needs.health > 0);
}

#[test]
fn with_a_way_out_there_is_no_last_chance() {
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(b, 0, 100, &[]);
    w.group(a, b, &[], &[]);
    assert_eq!(resolve::last_chance(&mut w.seat(), &policy(true), true, &mut Rng::seeded(1)), None);
    assert_eq!(w.rival.needs.health, 100);
}

#[test]
fn joining_a_camp_moves_the_man_and_his_belongings() {
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(b, 0, 100, &[]);
    // Follower with a knife (a weapon: kept) and rats (unloaded into the
    // rooms: `UnloadGarrisonBelongings`); a trap in the leader's pack is
    // installed when the camp has none (`JoinCamp`).
    w.group(a, b, &["item_trap"], &[&["item_knife", "item_rats"]]);
    let mut seat = w.seat();
    assert!(seat.join_camp(0, false));
    drop(seat);
    let camp = w.camps.get(a).unwrap();
    assert_eq!((camp.owner, camp.residents.len()), (Some(0), 1));
    let guard = &camp.residents[0];
    assert_eq!(guard.pack.iter().collect::<Vec<_>>(), vec![w.k("item_knife")]);
    let stored: Vec<usize> = (0..2).filter_map(|r| w.rooms.get(a, r)).flat_map(|b| b.iter().collect::<Vec<_>>()).collect();
    let rats = w.k("item_rats");
    let trap = w.k("item_trap");
    let sites_has_trap = w.gam.locations[a].available_products.iter().flatten().any(|&p| w.productions.get(p).is_some_and(|p| p.tool == "item_trap"));
    assert!(stored.contains(&rats), "{stored:?}");
    assert_eq!(stored.contains(&trap), sites_has_trap, "the trap only where it works");
    assert_eq!(w.rival.party.len(), 0);
    let _ = Kit::default();
}

// --- the turn (`ClassicAiState.Turn`, `AiTurnController.RunTurn`) ---------

struct Titles;
impl burngfx::modern_ai::reason::Names for Titles {
    fn place(&self, l: usize) -> String {
        format!("place {l}")
    }
    fn person(&self, r: usize) -> String {
        format!("person {r}")
    }
}

fn run(w: &mut World, seed: u64) -> burngfx::modern_ai::controller::Report {
    use burngfx::modern_ai::controller::{turn, Game};
    let game = Game { seed, extended: true, names: &Titles, clock: None };
    turn(&mut w.seat(), &policy(false), &game, &mut burngfx::spawn::Rng::game(seed))
}

/// `RunTurn`: after each action the next decision is chosen on the world
/// that action left — three surplus followers dismissed one by one, each
/// time from a smaller group, until `travel_group_size` (2) is reached and
/// something else is chosen.
#[test]
fn each_decision_sees_the_world_the_last_action_left() {
    use burngfx::modern_ai::decision::{Action, Outcome};
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(a, 0, 100, &[]);
    w.group(a, b, &["item_trap", "item_knife"], &[&["item_axe"], &["item_axe"], &["item_knife"], &[]]);
    let r = run(&mut w, 7);
    let kinds: Vec<(Action, Outcome)> = r.actions.iter().map(|(d, o)| (d.action, *o)).collect();
    // `MaintainCampNetwork` leaves one guard at the camp first (5 -> 4 in
    // the group); then three dismissals bring it to the leader and one.
    assert_eq!(w.camps.get(a).unwrap().residents.len(), 1);
    assert_eq!(&kinds[..3], &[(Action::DismissFollower, Outcome::ContinuePlanning); 3]);
    assert_ne!(kinds[3].0, Action::DismissFollower, "{kinds:?}");
    assert_eq!(w.rival.party.len(), 1);
    assert!(!r.cut_short);
}

/// `maximumStrategicActions`: a group of fifteen dismisses ten and the turn
/// ends there, the rest left for the next.
#[test]
fn a_turn_takes_at_most_ten_actions() {
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(a, 0, 100, &[]);
    let many: Vec<&[&str]> = vec![&["item_knife"]; 14];
    w.group(a, b, &["item_knife"], &many);
    let r = run(&mut w, 7);
    assert_eq!(r.actions.len(), 10);
    assert!(r.cut_short);
    assert_eq!(w.rival.party.len(), 4, "14 - 10");
}

/// One generator a turn, drawn from (seed, day, seat): the same world and
/// seed give the same turn.
#[test]
fn the_same_seed_gives_the_same_turn() {
    let go = |seed| {
        let mut w = World::new();
        let (a, b) = w.pair();
        w.camp(b, 0, 100, &[]);
        w.group(a, b, &["item_trap", "item_knife"], &[&["item_axe"]]);
        let r = run(&mut w, seed);
        (r, w.rival.journey, w.rival.party.len(), w.ai.clone())
    };
    let (r1, j1, p1, ai1) = go(7);
    let (r2, j2, p2, ai2) = go(7);
    assert_eq!(r1, r2);
    assert_eq!((j1, p1), (j2, p2));
    assert_eq!(ai1, ai2);
}

/// `StationaryRelocationTier`: alone at a camp with nothing to do the
/// leader waits; the third wait in one place asks for an exploratory move
/// (none here: no owned neighbour), the sixth strategic wait for a
/// purposeful one, which goes for logistics — and the counters restart.
#[test]
fn waiting_without_progress_ends_in_a_relocation() {
    use burngfx::modern_ai::decision::Action;
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(a, 0, 100, &[]);
    w.group(a, b, &[], &[]);
    let mut actions = vec![];
    let mut last = String::new();
    for _ in 0..6 {
        let r = run(&mut w, 7);
        actions.push(r.actions.last().unwrap().0.action);
        last = r.actions.last().unwrap().0.reason.text(&Titles);
        w.rival.journey = None;
        w.rival.at = a;
    }
    assert_eq!(actions, [Action::Wait, Action::Wait, Action::Wait, Action::Wait, Action::Wait, Action::Travel]);
    assert_eq!((w.ai.stall.no_progress_waits, w.ai.stall.no_progress_location), (0, None));
    assert_eq!(w.ai.stall.previous_exploratory_camp, Some(a));
    // The relocation's reason carries the one it replaced.
    assert!(last.starts_with("seek useful logistics at place "), "{last}");
    assert!(last.ends_with("; non-progress fallback for: expansion blocked: no portable production tool and no affordable or collectible route"), "{last}");
}

/// The protection a fighter wears in a fight on the real map. Upstream puts
/// it on only when a blow lands (`TableCombat.Roll`: `defender.Protection =
/// PreferredProtection(defender)`; `SelectLoadout` is the inventory
/// window's): the gas mask where the place is gassed, the jacket elsewhere.
/// A guard of theirs who will not fall hits back, so both sides are struck;
/// whoever was struck wears the preferred piece, written back from the
/// fight, and whoever was not has put nothing on.
#[test]
fn in_a_fight_the_mask_is_worn_in_gas_and_the_jacket_elsewhere() {
    use burngfx::gamdat::Danger;
    let worn = |gassed: bool| {
        let mut w = World::new();
        let n = w.gam.locations.len();
        let theirs = (0..n)
            .find(|&l| {
                let loc = &w.gam.locations[l];
                !loc.is_city && loc.danger == if gassed { Danger::Gas } else { Danger::None }
                    && loc.roads.iter().any(|r| r.days > 0 && !w.gam.locations[r.neighbour].is_city)
            })
            .expect("a place");
        let ours = w.gam.locations[theirs].roads.iter().find(|r| r.days > 0 && !w.gam.locations[r.neighbour].is_city).unwrap().neighbour;
        let kit = ["item_axe", "item_leather_jacket", "item_gas_mask"];
        w.camp(ours, 0, 100, &[]);
        w.camp(theirs, 1, 1000, &kit);
        w.group(theirs, ours, &kit, &[&kit, &kit]);
        w.ai.target = Some(theirs);
        let got = resolve::resolve(&mut w.seat(), &policy(false), true, false, &mut Rng::seeded(4));
        assert!(matches!(got, Result::Failed { .. }), "{got:?}");
        let name = |pack: &ItemBag, slot: Option<usize>| slot.and_then(|s| pack.get(s)).and_then(|k| w.types.name_of(k)).map(str::to_string);
        // (struck, worn) for the leader, each follower, and their guard.
        let mut out = vec![(w.rival.needs.health < 100, name(&w.rival.pack, w.ai.leader_kit.protection))];
        out.extend(w.rival.party.companions.iter().map(|c| (c.needs.health < 100, name(&c.pack, c.kit.protection))));
        out.extend(w.camps.get(theirs).unwrap().residents.iter().map(|r| (r.needs.health < 1000, name(&r.pack, r.kit.protection))));
        out
    };
    for (gassed, piece) in [(true, "item_gas_mask"), (false, "item_leather_jacket")] {
        let got = worn(gassed);
        assert!(got.iter().filter(|x| x.0).count() >= 2, "both sides struck: {got:?}");
        for (struck, worn) in &got {
            assert_eq!(worn.as_deref(), struck.then_some(piece), "{got:?}");
        }
    }
}

/// `burial::bury`, the one rule both machine seats use: the seat's own
/// group, the other machine's group here, then the camp, each from the
/// back; things on the ground where each person stood (`DropPosition =
/// Position`), a machine leader's at the entry point where `Travel` put him; the camp with
/// no camper left is nobody's; each person due back; the leaders' packs on
/// the ground, the other machine's first.
#[test]
fn the_fallen_are_buried_in_one_order_by_one_rule() {
    use burngfx::burial::{bury, Here, Source};
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(a, 1, 0, &["item_knife"]);
    w.group(a, b, &["item_axe"], &[&["item_rats"], &[]]);
    w.rival.party.companions[0].needs.health = 0;
    w.rival.party.companions[0].origin = Some(7);
    w.rival.party.companions[0].at = burngfx::game::Point::new(101, 102);
    w.camps.at(a).unwrap().residents[0].at = burngfx::game::Point::new(301, 302);
    w.rival.needs.health = 0;
    let mut other = Rival::new(2, a);
    other.pack = ItemBag::with(6, &[w.k("item_meat")]);
    let id = other.party.recruit();
    let c = other.party.companions.iter_mut().find(|c| c.id == id).unwrap();
    c.pack = ItemBag::with(6, &[w.k("item_bottle")]);
    c.needs.health = 0;
    c.at = burngfx::game::Point::new(201, 202);
    other.needs.health = 0;
    let entry = w.gam.locations[a].entry_point;
    let mut others = vec![&mut other];
    let got = bury(Here {
        location: a,
        entry,
        rival: &mut w.rival,
        humans: Vec::new(),
        human_leaders: Vec::new(),
        others: &mut others,
        camps: &mut w.camps,
        ground: &mut w.ground,
        people: &mut w.people,
        respawns: &mut w.respawns,
        npc_respawn: 10,
        order: Vec::new(),
    });
    let sources: Vec<Source> = got.people.iter().map(|f| f.source).collect();
    assert_eq!(sources, [Source::Party(0), Source::Party(2), Source::Camp(Some(1))]);
    let kinds: Vec<usize> = w.ground.iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [w.k("item_rats"), w.k("item_bottle"), w.k("item_knife"), w.k("item_meat"), w.k("item_axe")]);
    let p = burngfx::game::Point::new;
    let at: Vec<burngfx::game::Point> = w.ground.iter().map(|d| d.at).collect();
    assert_eq!(at, [p(101, 102), p(201, 202), p(301, 302), entry, entry]);
    assert!(w.ground.iter().all(|d| d.location == a));
    assert_eq!(got.leaders.iter().map(|l| l.0).collect::<Vec<_>>(), [2, 0], "the other machine's leader first");
    assert_eq!(w.camps.get(a).and_then(|c| c.owner), None, "no camper left: nobody's");
    assert_eq!(w.rival.party.len(), 1);
    assert!(w.rival.pack.is_empty());
    assert!(w.respawns.iter().any(|p| p.record == 7 && p.location == a && p.days == 10));
}


/// `Player.Travel`: the members of a machine group stand at the
/// destination's entry point plus `entryOffsets[i]`, the leader on it.
#[test]
fn a_group_set_on_the_road_stands_at_its_entry_positions() {
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(b, 0, 100, &[]);
    w.group(a, b, &[], &[&[], &[]]);
    assert!(w.seat().travel(b));
    let e = w.gam.locations[b].entry_point;
    let at: Vec<(i32, i32)> = w.rival.party.companions.iter().map(|c| (c.at.x - e.x, c.at.y - e.y)).collect();
    assert_eq!(at, [(0, -8), (8, 0)]);
}

/// `AiTelemetry` changes nothing: eight turns of the same game with the
/// sink off and on leave the same world and the same opponent, and the
/// sink did hear something.
#[test]
fn reporting_changes_nothing() {
    use burngfx::modern_ai::telemetry::Telemetry;
    let play = |on: bool| {
        let mut w = World::new();
        let (a, b) = w.pair();
        w.camp(a, 0, 100, &[]);
        w.group(a, b, &["item_trap", "item_knife"], &[&["item_axe"], &["item_axe"], &["item_knife"], &[]]);
        if on {
            w.ai.telemetry = Telemetry::on();
        }
        let mut reports = Vec::new();
        let mut decisions = Vec::new();
        for _ in 0..8 {
            let r = run(&mut w, 11);
            decisions.push(r.actions.iter().map(|(d, o)| (d.action, d.target, *o)).collect::<Vec<_>>());
            reports.extend(w.ai.telemetry.take());
            w.rival.journey = None;
        }
        let state = alloc_state(&w);
        (state, decisions, reports)
    };
    let (quiet, quiet_decisions, none) = play(false);
    let (loud, loud_decisions, lines) = play(true);
    assert!(none.is_empty());
    assert!(!lines.is_empty(), "the sink heard nothing");
    assert_eq!(quiet_decisions, loud_decisions);
    assert_eq!(quiet, loud);
}

/// Everything a turn can change, written down.
fn alloc_state(w: &World) -> String {
    let mut ai = String::new();
    w.ai.write(1, &mut ai);
    format!("{ai}\n{:?}\n{:?}\n{:?}\n{:?}", w.rival, w.camps, w.rooms, w.ground)
}

/// A camper who falls in the controller's fight drops his things where he
/// stood (`DropPosition = Position`), not at the entry; what is not taken
/// as loot stays there.
#[test]
fn a_fallen_guard_drops_where_he_stood() {
    let mut w = World::new();
    let (theirs, ours) = w.pair();
    w.camp(ours, 0, 100, &[]);
    w.camp(theirs, 1, 5, &["item_axe", "item_leather_jacket", "item_gas_mask", "item_tires"]);
    let e = w.gam.locations[theirs].entry_point;
    let stood = burngfx::game::Point::new(e.x + 20, e.y);
    w.camps.at(theirs).unwrap().residents[0].at = stood;
    w.group(theirs, ours, &["item_axe"], &[&["item_axe"], &["item_axe"]]);
    w.ai.target = Some(theirs);
    let got = resolve::resolve(&mut w.seat(), &policy(false), true, false, &mut Rng::seeded(4));
    assert!(matches!(got, Result::Captured { .. }), "{got:?}");
    let tires = w.k("item_tires");
    let here: Vec<&burngfx::spawn::Dropped> = w.ground.iter().filter(|d| d.location == theirs).collect();
    assert!(here.iter().any(|d| d.kind == tires && d.at == stood), "{here:?}");
    assert!(here.iter().all(|d| d.at == stood), "nothing at the entry: {here:?}");
}

/// A camp making meat with `traps` traps on its room floor and one guard
/// carrying `carried` traps; the reserve already holding `reserved` traps.
/// Returns how many traps each of reserve, room and guard has afterwards,
/// and the excess the camp computed.
fn reclaim(inventory: bool, traps: usize, carried: usize, reserved: u8) -> (u8, usize, usize, i32) {
    let mut w = World::new();
    if inventory {
        let text = read("production.txt").replace("[item_trap]\nallow_inventory=false", "[item_trap]\nallow_inventory=true");
        assert_ne!(text, read("production.txt"));
        w.productions = Productions::parse(&text);
    }
    let meat = w.productions.iter().position(|p| p.produce == "item_meat").unwrap();
    let l = (0..w.gam.locations.len())
        .find(|&l| !w.gam.locations[l].is_city && w.gam.locations[l].available_products.iter().flatten().any(|&p| p == meat))
        .unwrap();
    let trap = w.k("item_trap");
    w.camp(l, 0, 100, &vec!["item_trap"; carried]);
    w.camps.at(l).unwrap().production = Some(meat);
    w.rooms.set(l, 0, ItemBag::with(6, &vec![trap; traps]));
    w.group(l, l, &[], &[]);
    if reserved > 0 {
        w.ai.reserve.push((trap, reserved));
    }
    let policy = policy(false);
    let mut seat = w.seat();
    let snap = seat.snap(&policy, true);
    let excess = burngfx::modern_ai::production::redundant_tools(seat.items(), &snap.v.sites, 0, l);
    let excess = excess.first().map_or(0, |e| e.1);
    upkeep::collect_redundant_tools(&mut seat, &snap, l);
    let count = |b: &ItemBag| b.iter().filter(|&k| k == trap).count();
    let reserve = w.ai.reserve.iter().find(|(k, _)| *k == trap).map_or(0, |r| r.1);
    let guard = w.camps.get(l).unwrap().residents.iter().map(|c| count(&c.pack)).sum();
    (reserve, count(w.rooms.get(l, 0).unwrap()), guard, excess)
}

#[test]
fn redundant_tools_come_off_the_floor_into_the_reserve() {
    // The rooms' traps go, as many as the excess; the guard's pack does not
    // count where the production says so, and v1.2 ships it saying so.
    let (reserve, room, guard, excess) = reclaim(false, 4, 1, 0);
    assert_eq!(excess, 2);
    assert_eq!((reserve, room, guard), (2, 2, 1));
}

#[test]
fn a_full_reserve_leaves_the_tool_where_it_is() {
    // One place left in the reserve: the first candidate goes into it, the
    // second is refused and stays on the floor — and the refusal does not
    // reach past the first `excess` candidates for another one.
    let (reserve, room, guard, _) = reclaim(false, 4, 1, burngfx::modern_ai::state::RESERVE_CAP - 1);
    assert_eq!((reserve, room, guard), (burngfx::modern_ai::state::RESERVE_CAP, 3, 1));
    let (reserve, room, guard, _) = reclaim(false, 4, 1, burngfx::modern_ai::state::RESERVE_CAP);
    assert_eq!((reserve, room, guard), (burngfx::modern_ai::state::RESERVE_CAP, 4, 1));
}

#[test]
fn rooms_and_guards_are_one_list_when_packs_count() {
    // With packs counted, the rooms come first and the guards after them,
    // the first `excess` of the two together (`Concat`, `Take`).
    // The camp wants two: one trap on the floor and three carried is two
    // too many, and they are the floor's and then the guard's first.
    let (reserve, room, guard, excess) = reclaim(true, 1, 3, 0);
    assert_eq!(excess, 2);
    assert_eq!((reserve, room, guard), (2, 0, 2));
    // Without packs counted the same camp has nothing to give back.
    let (reserve, room, guard, excess) = reclaim(false, 1, 3, 0);
    assert_eq!((reserve, room, guard, excess), (0, 1, 3, 0));
}

#[test]
fn a_last_chance_march_leaves_with_the_camp_water() {
    // Our group stands in our own camp with an empty bottle; the room holds
    // a full one. Trapped, it marches on the hostile camp next door — and
    // swaps the empty bottle for the full one before it goes, as every
    // other departure does (`PrepareWaterForDeparture`).
    let mut w = World::new();
    let (ours, theirs) = w.pair();
    w.camp(ours, 0, 100, &[]);
    w.camp(theirs, 1, 50, &[]);
    let (full, empty) = (w.k("item_water_bottle"), w.k("item_bottle"));
    w.rooms.set(ours, 0, ItemBag::with(6, &[full]));
    w.group(ours, theirs, &["item_bottle"], &[]);
    w.ai.last_chance_target = Some(theirs);
    let got = resolve::last_chance(&mut w.seat(), &policy(false), true, &mut Rng::seeded(1));
    assert!(matches!(got, Some(Result::Failed { .. })), "{got:?}");
    assert!(w.rival.journey.is_some_and(|j| j.from == ours), "{:?}", w.rival.journey);
    assert_eq!(w.rival.pack.iter().collect::<Vec<_>>(), vec![full]);
    assert_eq!(w.rooms.get(ours, 0).unwrap().iter().collect::<Vec<_>>(), vec![empty]);
}

/// A group with nothing to rest on, and an own camp whose guard carries a
/// bible and whose room holds `in_room`.
fn reststop_bible(in_room: &[&str]) -> (World, usize) {
    let mut w = World::new();
    let (ours, other) = w.pair();
    w.camp(ours, 0, 100, &["item_bible"]);
    let room: Vec<usize> = in_room.iter().map(|n| w.k(n)).collect();
    w.rooms.set(ours, 0, ItemBag::with(6, &room));
    w.group(other, ours, &[], &[&[]]);
    (w, ours)
}

#[test]
fn the_reststop_bible_comes_from_a_guard_when_no_room_has_one() {
    let (mut w, ours) = reststop_bible(&[]);
    let bible = w.k("item_bible");
    assert!(burngfx::modern_ai::controller::prepare_reststop_settlement(&mut w.seat()));
    let carried = w.rival.pack.iter().chain(w.rival.party.companions.iter().flat_map(|c| c.pack.iter())).filter(|&k| k == bible).count();
    assert_eq!(carried, 1);
    assert!(!w.camps.get(ours).unwrap().residents[0].pack.iter().any(|k| k == bible));
}

#[test]
fn the_reststop_bible_comes_from_a_room_before_a_guard() {
    let (mut w, ours) = reststop_bible(&["item_bible"]);
    let bible = w.k("item_bible");
    assert!(burngfx::modern_ai::controller::prepare_reststop_settlement(&mut w.seat()));
    assert!(w.rooms.get(ours, 0).unwrap().iter().all(|k| k != bible));
    assert!(w.camps.get(ours).unwrap().residents[0].pack.iter().any(|k| k == bible));
}

#[test]
fn after_a_trade_the_turn_plans_on_the_needs_it_left() {
    // `RunTurn` takes the needs once and every decision of the turn reads
    // them (`AiTurnContext.Needs`). A thirsty leader buys a canteen; after
    // the exchange the turn's needs are taken again (`RefreshNeeds`), so
    // what follows plans on a group that has water, not on the one that
    // walked in. Fails with the refresh removed.
    let mut w = World::new();
    w.prices = Prices::parse("3 = 27\n6 = 40\n7 = 32\n13 = 120\n45 = 16\n49 = 80\n52 = 20\n53 = 28\n54 = 24\n");
    w.traders = burngfx::trader::Traders::with_city_shops(&w.gam.characters, &w.gam);
    let city = (0..w.gam.locations.len()).find(|&l| w.gam.locations[l].is_city && w.traders.shop_of(l).is_some()).unwrap();
    let shop = w.traders.shop_of(city).unwrap();
    let canteen = w.k("item_full_canteen");
    w.traders.get_mut(shop).unwrap().stock = ItemBag::with(burngfx::trader::TRADER_PACK, &[canteen]);
    w.group(city, city, &["item_fur", "item_tires", "item_fur", "item_tires"], &[]);
    w.rival.needs.water = 1;
    let mut policy = policy(false);
    policy.trade_benefit = 1.0;
    let mut seat = w.seat();
    let before = seat.fresh_needs(&policy, true);
    let cache = std::rc::Rc::new(std::cell::RefCell::new(Some(before.clone())));
    seat.needs_cache = Some(cache.clone());
    assert!(upkeep::trade_with_trader(&mut seat, &policy, true, shop));
    let after = seat.fresh_needs(&policy, true);
    drop(seat);
    assert_eq!(w.ai.reserve_count(canteen), 1);
    assert_ne!(before, after, "the canteen changes what the group needs");
    assert_eq!(cache.borrow().as_ref(), Some(&after));
}

#[test]
fn the_turn_keeps_its_first_trade_city() {
    // `AiTurnContext.BestTradeCity` is asked twice in a turn — in the
    // morning's upkeep and again when the plan is made — and the second
    // time gets the first answer, whatever changed in between. Our port
    // keeps it in `trade_city_cache`; with nothing kept it is worked out.
    let mut w = World::new();
    w.prices = Prices::parse("3 = 27\n6 = 40\n7 = 32\n13 = 120\n45 = 16\n49 = 80\n52 = 20\n53 = 28\n54 = 24\n");
    w.traders = burngfx::trader::Traders::with_city_shops(&w.gam.characters, &w.gam);
    let (a, b) = w.pair();
    w.camp(a, 0, 100, &[]);
    w.group(a, b, &["item_fur", "item_tires", "item_fur", "item_tires"], &[]);
    let policy = policy(false);
    let mut seat = w.seat();
    let snap = seat.snap(&policy, true);
    let ask = |seat: &Seat| upkeep::with_context(seat, &snap, &policy, |cx| burngfx::modern_ai::controller::cached_trade_city(seat, cx));
    let fresh = upkeep::with_context(&seat, &snap, &policy, |cx| burngfx::modern_ai::trade_plan::best_trade_city(cx, seat.ai, seat.rival.previous));
    assert!(fresh.is_some());
    assert_eq!(ask(&seat), fresh);
    let other = (0..seat.gam.locations.len()).find(|&l| seat.gam.locations[l].is_city && Some(l) != fresh).unwrap();
    seat.trade_city_cache = Some(Some(other));
    assert_eq!(ask(&seat), Some(other));
    seat.trade_city_cache = Some(None);
    assert_eq!(ask(&seat), None);
}

/// v1.2's `AiState.HireNpc` (`UP/AI/PlayerBehavior/Core/AiState.cs:1366`)
/// hires through `Character.Hire`, which ends in `InitializeRecruit`
/// (`Character.cs:429-430`, `GameRules.cs:101-117`): under `dos_fixed` the
/// recruit has food 9 and water 5, under `remaster_difficulty` an AI boss's
/// recruit has 5 and 5 — whatever his record held.
fn modern_recruit(rule: burngfx::rules::RecruitSupplies) -> CharacterNeeds {
    let mut w = World::new();
    w.book.rules.recruit_supplies = rule;
    let (who, at) = (0..w.people.len())
        .filter_map(|i| w.people.get(i).map(|p| (i, p)))
        .find(|(_, p)| p.class.can_be_hired() && p.employer.is_none())
        .map(|(i, p)| (i, p.location))
        .unwrap();
    let record = w.people.get(who).unwrap().record;
    let p = w.people.get_mut(who).unwrap();
    p.needs.food = 2;
    p.needs.water = 1;
    w.rival = Rival::new(0, at);
    assert!(w.seat().hire(record));
    w.rival.party.companions[0].needs
}

#[test]
fn a_modern_recruit_gets_initialize_recruit_supplies() {
    let c = modern_recruit(burngfx::rules::RecruitSupplies::DosFixed);
    assert_eq!((c.food, c.water), (9, 5));
    let e = modern_recruit(burngfx::rules::RecruitSupplies::RemasterDifficulty);
    assert_eq!((e.food, e.water), (5, 5));
}

/// v1.2's `Die` happens at the blow (`Character.Health`'s setter,
/// `Character.cs:197-200`, then `Die`, `:454-478`): what the fallen carried
/// reaches the ground, and their returns are scheduled, in the order the
/// blows brought them down — not in group order once the fight is over
/// (follow-up F10). The camper first, then the other machine's leader, then
/// our companion.
#[test]
fn the_fallen_reach_the_ground_in_the_order_of_the_blows() {
    use burngfx::burial::{bury, Here, Key};
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(a, 1, 0, &["item_knife"]);
    w.group(a, b, &["item_axe"], &[&["item_rats"]]);
    w.rival.party.companions[0].needs.health = 0;
    w.rival.party.companions[0].origin = Some(7);
    let ours = w.rival.party.companions[0].id;
    w.camps.at(a).unwrap().residents[0].origin = Some(8);
    let camper = w.camps.get(a).unwrap().residents[0].id;
    let mut other = Rival::new(2, a);
    other.pack = ItemBag::with(6, &[w.k("item_meat")]);
    other.needs.health = 0;
    let entry = w.gam.locations[a].entry_point;
    let mut others = vec![&mut other];
    bury(Here {
        location: a,
        entry,
        rival: &mut w.rival,
        humans: Vec::new(),
        human_leaders: Vec::new(),
        others: &mut others,
        camps: &mut w.camps,
        ground: &mut w.ground,
        people: &mut w.people,
        respawns: &mut w.respawns,
        npc_respawn: 10,
        order: vec![Key::Camper { id: camper }, Key::Leader(2), Key::Member { player: 0, id: ours }],
    });
    let kinds: Vec<usize> = w.ground.iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [w.k("item_knife"), w.k("item_meat"), w.k("item_rats")]);
    let returns: Vec<usize> = w.respawns.iter().map(|p| p.record).collect();
    assert_eq!(returns, [8, 7]);
}

// --- v1.2's load step (conflict C1) ---------------------------------------
//
// `ClassicGame.InitAfterLoad` → `ClassicAiState.InitAfterLoad`
// (`AI/PlayerBehavior/Core/AiState.cs:171-178`): what v1.2 serializes is
// kept, the rest is at its CLR default, the intent and the deadline are
// worked out again, the guards unload. Each test's expected values are
// v1.2's after-load values, not the port's before-save ones.

/// `AttackPlanTurns` as v1.2 ships it: 40 on every difficulty
/// (`resources/game/classic/ai.txt:21,59,94`).
fn shipped_attack_plan_turns(difficulty: u8) -> i32 {
    let text = std::fs::read_to_string(format!("{}/testdata/v1.2/rules/classic/ai.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    AiConfig::parse(&text).unwrap().policy(difficulty).attack_plan_turns
}

/// The whole load step on the test's world, as `aros-game`'s `after_load`
/// runs it.
fn load_step(w: &mut World, day: i32) {
    use burngfx::modern_ai::seat::{normalize_loaded_garrison_belongings, Garrisons};
    let me = w.rival.player;
    let turns = shipped_attack_plan_turns(w.ai.difficulty);
    let (gam, camps) = (&w.gam, &w.camps);
    w.ai.after_load(day, me, |l| (gam.locations[l].is_city, camps.get(l).and_then(|c| c.owner)), turns);
    normalize_loaded_garrison_belongings(Garrisons {
        me,
        ai: &mut w.ai,
        camps: &mut w.camps,
        rooms: &mut w.rooms,
        springs: &mut w.springs,
        types: &w.types,
        prices: &w.prices,
        room_ids: &w.room_ids,
    });
}

/// Every field set; after the load step the five v1.2 serializes are as
/// they were (`settings` `AiState.cs:38`, `slumpMaterialGrantsUsed`
/// `:73-74`, `headedLocation` `:37`, `itemPool` `:42`, the leader's
/// `weapon`/`protection` `Character.cs:40-41`) and every other one is
/// `ModernAi::new`'s — the CLR default of the `[NonSerialized]` field or an
/// empty weak table — but for the two `InitAfterLoad` works out again.
#[test]
fn after_load_keeps_what_v1_2_serializes() {
    use burngfx::modern_ai::state::*;
    let mut ai = ModernAi::new(2, 7);
    ai.target = Some(4);
    ai.target_was_neutral = false;
    assert!(ai.reserve_add(30) && ai.reserve_add(7) && ai.reserve_add(7));
    ai.slump_grants_used = 3;
    ai.leader_kit = Kit { weapon: Some(1), protection: Some(2) };
    ai.journey = Some(Commitment { destination: 9, reason: "trade".into() });
    ai.attack_plan_until = Some(44);
    ai.attack_recruitment = Some(AttackRecruitment { group_size: 3, until_day: 20 });
    ai.retaliating = Some((2, 31));
    ai.contested = Some((5, 17));
    ai.post_capture_pause = 2;
    ai.failed_attack = Some(FailedAttack { camp: 5, until_day: 25, group_size: 2, attacker_strength: 1.0, defender_strength: 2.0 });
    ai.last_chance_target = Some(8);
    ai.recruitment_failure_anchor = Some(11);
    let f = Force { count: 2, health: 150, strength: 0.1, boss_health: 80 };
    ai.retries.push(Retry { target: 5, owner: Some(2), attacker: f, defender: f, blocked: true });
    ai.knowledge.push(Observation { camp: 5, owner: None, owner_since: 3, observed_day: 6, defenders: 2, strength: 22.0 });
    ai.deferrals.push(Deferral { target: 12, owner: None, blocker: Blocker::Attackers, retry_day: 15, progress: 3, sequence: 4 });
    ai.next_deferral = 5;
    ai.stall = Stall { stalled_target: Some(12), stalled_waits: 3, no_progress_waits: 2, ..Stall::default() };
    ai.demobilized.push(41);
    ai.support = Support { initialized: true, last_trap_day: 11, trap_count: 2, saving_for_snake_trap: true, strategic: Some((4, 2, 9)), traders_seen: vec![(0, 4)] };
    ai.attack_recruitment_city = Some(3);
    ai.survival_releases = vec![7];
    let before = ai.clone();
    // Target 4 a free non-city: a settlement after the load.
    ai.after_load(30, 0, |_| (false, None), 40);
    let mut want = ModernAi::new(2, 7);
    want.target = Some(4);
    want.reserve = before.reserve.clone();
    want.slump_grants_used = 3;
    want.leader_kit = before.leader_kit;
    want.target_was_neutral = true;
    assert_eq!(ai, want);
}

/// `InitAfterLoad`'s two lines (`AiState.cs:173-177`) with `HasAttackPlan`
/// (`:334-336`), one case a line: (what was saved, the target's city flag
/// and owner now) → (`strategicTargetWasNeutral`, `attackPlanUntilDay`).
/// Day 30, this seat 0, `AttackPlanTurns` 40.
#[test]
fn after_load_recomputes_intent() {
    use burngfx::modern_ai::state::ModernAi;
    let turns = shipped_attack_plan_turns(1);
    assert_eq!(turns, 40);
    let case = |target: Option<usize>, was_neutral: bool, plan: Option<i32>, city: bool, owner: Option<usize>| {
        let mut ai = ModernAi::new(1, 5);
        ai.target = target;
        ai.target_was_neutral = was_neutral;
        ai.attack_plan_until = plan;
        ai.after_load(30, 0, |_| (city, owner), turns);
        (ai.target_was_neutral, ai.attack_plan_until)
    };
    // Chosen neutral, now a rival's: an attack plan with a fresh deadline.
    assert_eq!(case(Some(6), true, None, false, Some(2)), (false, Some(70)));
    // Chosen hostile, now free: a settlement, no deadline.
    assert_eq!(case(Some(6), false, Some(50), false, None), (true, None));
    // A plan marked ready (deadline gone), the target still hostile: a deadline again.
    assert_eq!(case(Some(6), false, None, false, Some(2)), (false, Some(70)));
    // A plan expiring tomorrow: extended to the full 40 days.
    assert_eq!(case(Some(6), false, Some(31), false, Some(3)), (false, Some(70)));
    // Our own camp: neither neutral nor an attack.
    assert_eq!(case(Some(6), false, Some(50), false, Some(0)), (false, None));
    // A city: no owner, so "neutral", and never an attack plan.
    assert_eq!(case(Some(6), false, Some(50), true, None), (true, None));
    // No target at all.
    assert_eq!(case(None, true, Some(50), false, None), (false, None));
}

/// `NormalizeLoadedGarrisonBelongings` (`CampManagement.cs:102-111`): a
/// guard of ours who still carries his belongings — stationed when the
/// rooms were full — unloads them at the load into rooms that now have
/// space, keeping his weapon; another seat's guard keeps everything. Not a
/// no-op for a port save: stationing unloads only where there is room.
#[test]
fn a_load_unloads_what_our_guards_still_carry() {
    let mut w = World::new();
    let (ours, theirs) = w.pair();
    w.camp(ours, 0, 100, &["item_knife", "item_meat"]);
    w.camp(theirs, 1, 100, &["item_knife", "item_meat"]);
    let (knife, meat) = (w.k("item_knife"), w.k("item_meat"));
    load_step(&mut w, 30);
    let guard = |w: &World, l: usize| w.camps.get(l).unwrap().residents[0].pack.iter().collect::<Vec<_>>();
    assert_eq!(guard(&w, ours), [knife]);
    assert!((0..2).filter_map(|r| w.rooms.get(ours, r)).any(|b| b.iter().any(|k| k == meat)), "the meat is in a room");
    assert_eq!(guard(&w, theirs), [knife, meat]);
}

/// A machine turn on day `day`, drawing from `rng` — the process's
/// generator, which goes on from turn to turn and across a load.
fn turn_on(w: &mut World, day: i32, rng: &mut Rng) -> burngfx::modern_ai::controller::Report {
    use burngfx::modern_ai::controller::{turn, Game};
    let game = Game { seed: 0, extended: true, names: &Titles, clock: None };
    let mut seat = w.seat();
    seat.day = day;
    turn(&mut seat, &policy(false), &game, rng)
}

/// The Modern state as a save writes it and a load reads it back: the
/// `modern` lines, parsed.
fn through_a_save(ai: &ModernAi) -> ModernAi {
    let mut text = String::new();
    ai.write(1, &mut text);
    let mut back = None;
    for line in text.lines() {
        let mut w = line.split_whitespace();
        assert_eq!((w.next(), w.next()), (Some("modern"), Some("1")));
        let what = w.next().unwrap();
        ModernAi::read(&mut back, what, &mut w).unwrap();
    }
    back.unwrap()
}

/// A Modern seat alone at its camp, waiting: the watchdog's counters grow
/// turn by turn (`AiState.cs:82-100`).
fn a_waiting_seat() -> World {
    let mut w = World::new();
    let (a, b) = w.pair();
    w.camp(a, 0, 100, &[]);
    w.group(a, b, &[], &[]);
    w
}

/// **The criterion** (C1): a game saved and loaded at a cut is the
/// uninterrupted game with v1.2's load step applied at the cut, in memory —
/// the same turns, the same world, the same state. Both draw from one
/// process generator that goes on across the load, as v1.2's static
/// `Random` does. And the load step is not nothing here: the uninterrupted
/// game without it plays differently (the watchdog counts from nought again).
#[test]
fn a_resumed_modern_game_is_an_uninterrupted_one_with_a_load_at_the_cut() {
    use burngfx::modern_ai::decision::Action;
    let play = |w: &mut World, days: core::ops::Range<i32>, rng: &mut Rng| -> Vec<Action> {
        let a = w.rival.at;
        days.map(|d| {
            let r = turn_on(w, d, rng);
            w.rival.journey = None;
            w.rival.at = a;
            r.actions.last().unwrap().0.action
        })
        .collect()
    };
    // Uninterrupted, v1.2's load step applied in memory after day 12.
    let mut straight = a_waiting_seat();
    let mut rng = Rng::game(15);
    let mut seen = play(&mut straight, 10..13, &mut rng);
    let cut_state = straight.ai.clone();
    load_step(&mut straight, 13);
    seen.extend(play(&mut straight, 13..19, &mut rng));
    // Saved after day 12 and loaded: the Modern lines written and read
    // back, the same load step, the same process generator going on.
    let mut resumed = a_waiting_seat();
    let mut rng2 = Rng::game(15);
    let mut seen2 = play(&mut resumed, 10..13, &mut rng2);
    assert_eq!(resumed.ai, cut_state);
    resumed.ai = through_a_save(&resumed.ai);
    load_step(&mut resumed, 13);
    seen2.extend(play(&mut resumed, 13..19, &mut rng2));
    assert_eq!(seen2, seen, "the same decisions");
    assert_eq!(resumed.ai, straight.ai, "the same state");
    assert_eq!(resumed.rival, straight.rival);
    assert_eq!(rng2, rng, "the generator at the same place");
    // Without the load step: the counters run on, and the relocation comes
    // three turns sooner — the memory v1.2 forgets did decide something.
    assert_ne!(cut_state.stall, Default::default(), "something to forget");
    let mut unbroken = a_waiting_seat();
    let mut rng3 = Rng::game(15);
    let unbroken_seen = play(&mut unbroken, 10..19, &mut rng3);
    assert_ne!(unbroken_seen, seen, "{unbroken_seen:?}");
}

/// **Two loads of one save play the same** with the same `seed` — the
/// controlled randomness two runs are compared under (upstream's tests call
/// `SetRandomSeed`).
#[test]
fn loading_one_save_twice_with_one_seed_plays_the_same() {
    let mut source = a_waiting_seat();
    let mut rng = Rng::game(3);
    for d in 10..13 {
        turn_on(&mut source, d, &mut rng);
    }
    let go = |seed: u64| {
        let mut w = a_waiting_seat();
        w.rival = source.rival.clone();
        w.ai = through_a_save(&source.ai);
        load_step(&mut w, 13);
        let mut rng = Rng::game(seed);
        let reports: Vec<_> = (13..20).map(|d| turn_on(&mut w, d, &mut rng)).collect();
        (reports, w.ai, w.rival)
    };
    let (one, two) = (go(1993), go(1993));
    assert_eq!(one, two);
}
