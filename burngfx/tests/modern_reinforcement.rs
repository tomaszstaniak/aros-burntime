//! `modern_ai::reinforcement` (v1.2's `ReinforcementPlanning`) and the
//! travel candidate (`decision::add_travel`), on places made by hand with
//! Extended's tables.

use burngfx::gamdat::Danger;
use burngfx::items::ItemTypes;
use burngfx::modern_ai::camps::*;
use burngfx::modern_ai::decision::{Action, Decision, Traveller};
use burngfx::modern_ai::group::Items;
use burngfx::modern_ai::policy::{AiConfig, AiPolicy};
use burngfx::modern_ai::reinforcement::*;
use burngfx::modern_ai::supplies::GroupSupplies;
use burngfx::production::Productions;
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
    fn trap(&self, installed: i32) -> SiteProduction {
        let id = self.productions.iter().position(|p| p.tool == "item_trap").unwrap();
        let p = self.productions.get(id).unwrap().clone();
        SiteProduction { id, produce: self.types.index_of(&p.produce).unwrap(), production: p, installed }
    }
}

fn policy(garrison: i32) -> AiPolicy {
    let section = |n: u8| format!("[{n}]\nrecruit_experience = 0 40\nslump_material_grants = 0\ncritical_garrison_target = {garrison}\nthreat_radius = 1\n");
    AiConfig::parse(&format!("{}{}{}", section(0), section(1), section(2))).unwrap().policy(1).clone()
}

fn place(owner: Option<usize>, water: i32, guards: usize, productions: Vec<SiteProduction>, roads: Vec<(usize, i32)>) -> Site {
    Site {
        city: false,
        owner,
        danger: Danger::None,
        water,
        source_reserve: 0,
        has_products: true,
        current: if productions.is_empty() { None } else { Some(0) },
        productions,
        guards: (0..guards).map(|_| Guard { owner: owner.unwrap_or(9), alive: true, resting: false, protected: true }).collect(),
        stored: vec![],
        elsewhere: vec![],
        guard_items: vec![],
        stock: 0,
        roads,
        bonus: 0,
        source_room: true,
        pump: false,
        threatened: false,
    }
}

#[test]
fn a_threat_is_an_enemy_camp_or_leader_within_the_radius() {
    // 0 - 1 - 2, with 2 the enemy's.
    let sites = Sites { cache: None, sites: vec![place(Some(1), 3, 1, vec![], vec![(1, 1)]), place(None, 0, 0, vec![], vec![(0, 1), (2, 1)]), place(Some(2), 3, 1, vec![], vec![(1, 1)])] };
    assert!(!is_threatened(&sites, 1, 1, &[], 0));
    assert!(is_threatened(&sites, 1, 2, &[], 0));
    assert!(is_threatened(&sites, 1, 1, &[1], 0), "a rival leader next door");
    let mut marked = sites.clone();
    mark_threatened(&mut marked, 1, 1, &[]);
    assert_eq!(marked.sites.iter().map(|s| s.threatened).collect::<Vec<_>>(), vec![false, true, true]);
}

#[test]
fn a_camp_takes_another_guard_only_if_it_feeds_him() {
    let t = T::new();
    let i = t.items();
    let p = policy(3);
    // Two traps make 7; water 4. One guard: a second leaves 2 water and 5
    // food — the wanted surplus (two, once this camp is well established).
    let rich = Sites { cache: None, sites: vec![place(Some(1), 4, 1, vec![t.trap(2)], vec![])] };
    assert_eq!(rich.wanted_supply_surplus(i, 1), 2);
    assert!(can_support_additional_guard(i, &rich, 1, &p, &rich.sites[0], 1));
    assert!(wants_additional_guard(i, &rich, 1, &p, 0, 3), "two at most while it holds under four camps");
    // Water 3: a second guard leaves one, under the surplus.
    let dry = Sites { cache: None, sites: vec![place(Some(1), 3, 1, vec![t.trap(2)], vec![])] };
    assert!(!can_support_additional_guard(i, &dry, 1, &p, &dry.sites[0], 1));
    // At the policy's garrison: no more, whatever the camp makes.
    assert!(!can_support_additional_guard(i, &rich, 1, &policy(1), &rich.sites[0], 1));
    // Under four camps the target is two: a camp with two guards wants none.
    let full = Sites { cache: None, sites: vec![place(Some(1), 5, 2, vec![t.trap(2)], vec![])] };
    assert!(!wants_additional_guard(i, &full, 1, &p, 0, 3));
}

#[test]
fn stationing_here_travelling_there_or_dismissing() {
    let t = T::new();
    let i = t.items();
    let p = policy(3);
    let sites = Sites {
        cache: None,
        sites: vec![place(Some(1), 4, 1, vec![t.trap(2)], vec![(1, 2)]), place(Some(1), 4, 1, vec![t.trap(2)], vec![(0, 2)])],
    };
    let group = GroupSupplies { food: vec![9, 9], water: vec![5, 5], max_food: vec![9, 9], max_water: vec![5, 5], health: vec![100, 100], food_carried: 0, water_carried: 0 };
    let at = |here| Traveller { sites: &sites, items: i, player: 1, previous: None, here, group: &group };
    let camp = best_camp_for_reinforcement(i, &sites, 1, &p, None, 0, 3);
    assert_eq!(camp, Some(0), "the same guards: the nearer, here");
    let needs = RecruitmentNeeds { camp, target: 2, attack_staging: false };
    let mut c: Vec<Decision> = vec![];
    add_candidates(&at(0), &p, 2, None, needs, &mut c);
    assert_eq!((c[0].action, c[0].score), (Action::StationFollower, 2000.0));
    // From the other camp: travel there, 830 less two days, committed.
    let mut c = vec![];
    add_candidates(&at(1), &p, 2, None, needs, &mut c);
    assert_eq!((c[0].action, c[0].score, c[0].next_step, c[0].commit_journey), (Action::Travel, 828.0, Some(0), true));
    // Alone, the leader stations nobody; three with nowhere to put one: dismiss.
    let mut c = vec![];
    add_candidates(&at(0), &p, 1, None, needs, &mut c);
    assert!(c.is_empty());
    let mut c = vec![];
    add_candidates(&at(0), &p, 3, None, RecruitmentNeeds::default(), &mut c);
    assert_eq!(c[0].action, Action::DismissFollower);
    // An attack wanting four keeps all three.
    let mut c = vec![];
    add_candidates(&at(0), &p, 3, Some(4), RecruitmentNeeds::default(), &mut c);
    assert!(c.is_empty());
}

#[test]
fn a_journey_without_supplies_is_no_candidate() {
    let t = T::new();
    let i = t.items();
    let sites = Sites { cache: None, sites: vec![place(None, 0, 0, vec![], vec![(1, 4)]), place(Some(1), 4, 1, vec![t.trap(2)], vec![(0, 4)])] };
    let thirsty = GroupSupplies { food: vec![9], water: vec![2], max_food: vec![9], max_water: vec![5], health: vec![100], food_carried: 0, water_carried: 0 };
    let tr = Traveller { sites: &sites, items: i, player: 1, previous: None, here: 0, group: &thirsty };
    let mut c = vec![];
    burngfx::modern_ai::decision::add_travel(&tr, &mut c, Some(1), 900.0, burngfx::modern_ai::reason::Reason::Note("home".into()), Default::default(), None, false);
    assert!(c.is_empty(), "two days' water for four");
    // It may go when it would survive, as recovery allows.
    burngfx::modern_ai::decision::add_travel(&tr, &mut c, Some(1), 900.0, burngfx::modern_ai::reason::Reason::Note("home".into()), burngfx::modern_ai::decision::Risk { survivable_recovery: true, fatal_recovery: false }, None, false);
    assert_eq!(c[0].score, 896.0);
}
