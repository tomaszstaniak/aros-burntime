//! `modern_ai::hiring`: v1.2's `Recruitment.AddCandidates` as a whole — a
//! world, and the decision it leads to — and the hire carried out
//! (`AiDecision.Execute(Recruit)` → `HireNpc`). Extended's v1.2 tables.

use burngfx::combat::Kit;
use burngfx::game::{CharacterNeeds, ItemBag};
use burngfx::gamdat::{CharClass, Danger};
use burngfx::items::ItemTypes;
use burngfx::modern_ai::camps::*;
use burngfx::modern_ai::decision::{Action, Decision};
use burngfx::modern_ai::group::Items;
use burngfx::modern_ai::hiring::*;
use burngfx::modern_ai::needs::{Needs, Plans, World};
use burngfx::modern_ai::policy::{AiConfig, AiPolicy};
use burngfx::modern_ai::recruitment::Candidate;
use burngfx::modern_ai::state::ModernAi;
use burngfx::modern_ai::trading::Barter;
use burngfx::production::Productions;
use burngfx::rival::Rival;
use burngfx::spawn::Rng;
use burngfx::trade::Prices;

fn read(f: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/v1.2/rules/extended/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

struct T {
    types: ItemTypes,
    productions: Productions,
    prices: Prices,
}

impl T {
    fn new() -> T {
        T { types: ItemTypes::parse_v12(&read("items.txt")).unwrap(), productions: Productions::parse(&read("production.txt")), prices: Prices::parse("3 = 27\n") }
    }
    fn items(&self) -> Items<'_> {
        Items { types: &self.types, prices: &self.prices }
    }
    fn k(&self, n: &str) -> usize {
        self.types.index_of(n).unwrap_or_else(|| panic!("{n}"))
    }
    fn trap(&self, installed: i32) -> SiteProduction {
        let id = self.productions.iter().position(|p| p.tool == "item_trap").unwrap();
        let p = self.productions.get(id).unwrap().clone();
        SiteProduction { id, produce: self.k(&p.produce), production: p, installed }
    }
}

fn policy(generated: bool) -> AiPolicy {
    let s = |n: u8| {
        format!(
            "[{n}]\nrecruit_experience = 0 -1\nslump_material_grants = 0\ntravel_group_size = 2\nattack_group_size = 2\n\
             critical_garrison_target = 1\nallow_generated_recruit_payment_in_cities = {generated}\n"
        )
    };
    AiConfig::parse(&format!("{}{}{}", s(0), s(1), s(2))).unwrap().policy(1).clone()
}

fn city(roads: Vec<(usize, i32)>) -> Site {
    Site {
        city: true,
        owner: None,
        danger: Danger::None,
        water: 0,
        source_reserve: 0,
        has_products: false,
        productions: vec![],
        current: None,
        guards: vec![],
        stored: vec![],
        elsewhere: vec![],
        guard_items: vec![],
        stock: 0,
        roads,
        bonus: 0,
        source_room: false,
        pump: false,
        threatened: false,
    }
}

fn camp(t: &T, owner: Option<usize>, tools: i32, roads: Vec<(usize, i32)>) -> Site {
    Site {
        city: false,
        owner,
        water: 4,
        has_products: true,
        productions: vec![t.trap(tools)],
        current: Some(0),
        guards: owner.map(|o| vec![Guard { owner: o, alive: true, resting: false, protected: true }]).unwrap_or_default(),
        source_room: true,
        ..city(roads)
    }
}

fn person(record: usize, asks: &[usize]) -> Candidate {
    Candidate { record, name: format!("p{record}"), class: CharClass::Mercenary, experience: 10, health: 100, hire_items: asks.to_vec(), pack: ItemBag::new(6), available: true }
}

fn leader(pack: ItemBag) -> Rival {
    let mut r = Rival::new(1, 0);
    r.needs = CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health: 100 };
    r.pack = pack;
    r
}

struct Case {
    sites: Sites,
    people: Vec<Vec<Candidate>>,
    r: Rival,
    ai: ModernAi,
    policy: AiPolicy,
    here: usize,
}

fn decide(t: &T, c: &Case) -> Vec<Decision> {
    let demand = vec![0; t.types.len()];
    let w = World { items: t.items(), productions: &t.productions, sites: &c.sites, player: 1, policy: &c.policy, day: 5, here: c.here, equipment_demand: &demand, traders: &[] };
    let needs = Needs::new(&c.r, &c.ai, &w, Plans::default());
    let kit = Kit::default();
    let p = Planner::new(&w, &needs, &c.r, &kit, &c.ai, Barter { factor: 100, benefit: 1.0 }, None, &c.people, &[], &[]);
    let mut out = vec![];
    p.add_candidates(c.ai.target, None, false, &mut out);
    out
}

fn summary(d: &[Decision]) -> Vec<(Action, f32, Option<usize>)> {
    d.iter().map(|d| (d.action, d.score, d.target)).collect()
}

/// City 0 (here) — camp 2 of ours, a day away, two traps and a guard (six
/// food a day to spare: a standing follower is wanted) — city 3, a day
/// from both.
fn standing(t: &T) -> Case {
    Case {
        sites: Sites { cache: None, sites: vec![city(vec![(2, 1), (3, 1)]), city(vec![]), camp(t, Some(1), 2, vec![(0, 1), (3, 1)]), city(vec![(0, 1), (2, 1)])] },
        people: vec![vec![person(40, &[])], vec![], vec![], vec![person(41, &[])]],
        r: leader(ItemBag::new(6)),
        ai: ModernAi::new(1, 5),
        policy: policy(true),
        here: 0,
    }
}

#[test]
fn a_follower_is_hired_here_when_one_is_free() {
    let t = T::new();
    // `AddCandidates`: needsFollower (1 < 2, surplus 6 >= 4); canRecruit;
    // FindSafeLocalRecruit (nobody critical); in a city: Recruit at 990.
    let d = decide(&t, &standing(&t));
    assert_eq!(summary(&d), vec![(Action::Recruit, 990.0, Some(0))]);
    assert_eq!(d[0].recruit, Some(40));
}

#[test]
fn nobody_here_means_the_road_to_a_city_with_somebody() {
    let t = T::new();
    let mut c = standing(&t);
    c.people[0].clear();
    // `!canRecruit`: FindBestCampForCityPreparation finds no lot worth it
    // (nothing carried, nothing stored), so FindNearestRecruitmentCity: city 3,
    // one day, back to camp 2 in one. `AddTravelCandidate`: 970 - 1.
    assert_eq!(summary(&decide(&t, &c)), vec![(Action::Travel, 969.0, Some(3))]);
}

#[test]
fn a_recruit_who_cannot_be_paid_is_no_recruit() {
    let t = T::new();
    let mut c = standing(&t);
    let meat = t.k("item_meat");
    c.people[0] = vec![person(40, &[meat])];
    // Meat asked, none carried, generated payment refused by the policy:
    // `CreateRecruitmentPlan` unfunded, so not `CanRecruit`; city 3's
    // recruit asks nothing.
    c.policy = policy(false);
    assert_eq!(summary(&decide(&t, &c)), vec![(Action::Travel, 969.0, Some(3))]);
    // Carrying the meat: hired here, the meat his pay.
    c.r = leader(ItemBag::with(6, &[meat]));
    let d = decide(&t, &c);
    assert_eq!((summary(&d), d[0].recruit), (vec![(Action::Recruit, 990.0, Some(0))], Some(40)));
}

#[test]
fn an_unreachable_settlement_waits() {
    let t = T::new();
    let mut c = standing(&t);
    // Place 1 is free trap land with no road to it: the settler cannot be
    // taken there (no route: settlementReady false), nor met there or on the
    // way; with a recruit here, `AddCandidates` waits at 2110 — and the
    // staging camp has no road to the target either.
    c.sites.sites[1] = camp(&t, None, 0, vec![]);
    c.ai.target = Some(1);
    c.ai.target_was_neutral = true;
    assert_eq!(summary(&decide(&t, &c)), vec![(Action::Wait, 2110.0, Some(0))]);
}

/// A lone leader with no camp at free place 0, between free trap land at 1
/// and at 2, two days each; a man at each asks nothing (the first is free).
fn first_camp(t: &T, roads: Vec<(usize, i32)>, danger: Danger) -> Case {
    let mut here = camp(t, None, 0, roads);
    here.productions.clear();
    here.current = None;
    let mut one = camp(t, None, 0, vec![(0, 2)]);
    one.danger = danger;
    let two = camp(t, None, 0, vec![(0, 2)]);
    Case {
        sites: Sites { cache: None, sites: vec![here, one, two] },
        people: vec![vec![], vec![person(51, &[])], vec![person(52, &[])]],
        r: leader(ItemBag::with(6, &[t.k("item_trap")])),
        ai: ModernAi::new(1, 5),
        policy: policy(true),
        here: 0,
    }
}

#[test]
fn the_first_camp_waypoint_and_its_tie() {
    let t = T::new();
    // `FindFirstCampWaypoint`: both qualify (a trap carried feeds two:
    // CanRemainAtNewCamp), equal in every key: the first neighbour.
    // `AddTravelCandidate`: 2095 - 2.
    let c = first_camp(&t, vec![(1, 2), (2, 2)], Danger::None);
    assert_eq!(summary(&decide(&t, &c)), vec![(Action::Travel, 2093.0, Some(1))]);
    let c = first_camp(&t, vec![(2, 2), (1, 2)], Danger::None);
    assert_eq!(summary(&decide(&t, &c)), vec![(Action::Travel, 2093.0, Some(2))], "the other order, the other one");
}

#[test]
fn a_gassed_waypoint_wants_protection_for_everybody() {
    let t = T::new();
    // Place 1 is gassed and listed first: without a mask carried and one in
    // the reserve (`CanCreateCamp`), it is passed over for 2.
    let c = first_camp(&t, vec![(1, 2), (2, 2)], Danger::Gas);
    assert_eq!(summary(&decide(&t, &c)), vec![(Action::Travel, 2093.0, Some(2))]);
    // A mask carried and one in the reserve: 1 again.
    let mut c = first_camp(&t, vec![(1, 2), (2, 2)], Danger::Gas);
    c.r = leader(ItemBag::with(6, &[t.k("item_trap"), t.k("item_gas_mask")]));
    c.ai.reserve_add(t.k("item_gas_mask"));
    assert_eq!(summary(&decide(&t, &c)), vec![(Action::Travel, 2093.0, Some(1))]);
}

fn world<'a>(t: &'a T, c: &'a Case, demand: &'a [i32]) -> World<'a> {
    World { items: t.items(), productions: &t.productions, sites: &c.sites, player: 1, policy: &c.policy, day: 5, here: c.here, equipment_demand: demand, traders: &[] }
}

#[test]
fn a_stale_plan_takes_no_payment() {
    let t = T::new();
    let meat = t.k("item_meat");
    let mut c = standing(&t);
    c.people[0] = vec![person(40, &[meat])];
    c.r = leader(ItemBag::with(6, &[meat]));
    let demand = vec![0; t.types.len()];
    let w = world(&t, &c, &demand);
    let mut kit = Kit::default();
    let mut rooms: Vec<Vec<ItemBag>> = vec![];
    // Planned for 41, who is not here: refused, the meat kept.
    let mut r = c.r.clone();
    let got = execute_recruit(&mut r, &mut kit, &mut rooms, &w, &c.ai, &c.people[0], Some(41), &mut Rng::seeded(1), |_, _| true);
    assert_eq!((got, r.pack.len()), (Hired::Refused, 1));
    // The world will not take him (the group is full there, say): the same.
    let got = execute_recruit(&mut r, &mut kit, &mut rooms, &w, &c.ai, &c.people[0], Some(40), &mut Rng::seeded(1), |_, _| false);
    assert_eq!((got, r.pack.len()), (Hired::Refused, 1));
    // The meat eaten since the plan: nothing to pay with, nobody hired.
    let mut hungry = c.r.clone();
    hungry.pack = ItemBag::new(6);
    let got = execute_recruit(&mut hungry, &mut kit, &mut rooms, &w, &c.ai, &c.people[0], Some(40), &mut Rng::seeded(1), |_, _| panic!("not hired"));
    assert_eq!(got, Hired::Refused);
    // As planned: paid with the meat, and hired.
    let mut seen = None;
    let got = execute_recruit(&mut r, &mut kit, &mut rooms, &w, &c.ai, &c.people[0], Some(40), &mut Rng::seeded(1), |_, rec| {
        seen = Some(rec);
        true
    });
    assert_eq!((got, r.pack.len(), seen), (Hired::Hired { record: 40 }, 0, Some(40)));
}


struct Titles;
impl burngfx::modern_ai::reason::Names for Titles {
    fn place(&self, l: usize) -> String {
        ["Ruins", "Trap Land", "Our Camp", "Second City"][l].to_string()
    }
    fn person(&self, r: usize) -> String {
        format!("Mr {r}")
    }
}

/// The reasons are upstream's sentences, places and people named.
#[test]
fn reasons_read_as_upstream_writes_them() {
    use burngfx::modern_ai::reason::Reason;
    let t = T::new();
    let d = decide(&t, &standing(&t));
    assert_eq!(d[0].reason.text(&Titles), "city opportunity: recruit toward 2 people");
    let mut c = standing(&t);
    c.people[0].clear();
    assert_eq!(decide(&t, &c)[0].reason.text(&Titles), "leader needs a recruit before claiming camps");
    let d = decide(&t, &first_camp(&t, vec![(1, 2), (2, 2)], Danger::None));
    assert_eq!(d[0].reason.text(&Titles), "advance to viable first-camp waypoint Trap Land");
    let mut c = standing(&t);
    c.sites.sites[1] = camp(&t, None, 0, vec![]);
    c.ai.target = Some(1);
    c.ai.target_was_neutral = true;
    assert_eq!(decide(&t, &c)[0].reason.text(&Titles), "delay recruitment until the projected two-person route to Trap Land is supplied");
    let nested = Reason::CollectRecruitmentSupplies(Box::new(Reason::RecruitAttackers { size: 3, target: 2 }));
    assert_eq!(nested.text(&Titles), "collect recruitment-trip supplies before recruit toward 3 attackers for Our Camp");
    assert_eq!(Reason::PickUpAtRouteStop { recruit: 7, stop: 3, target: 1 }.text(&Titles), "pick up Mr 7 at route stop Second City before settling Trap Land");
}
