//! `modern_ai::attack` (v1.2's `AttackPlanning` and the attack plan's
//! state), `modern_ai::deferrals`, and recruitment's attack branch fed with
//! the computed size. Extended's v1.2 tables.

use burngfx::combat::Kit;
use burngfx::game::{CharacterNeeds, ItemBag};
use burngfx::gamdat::{CharClass, Danger};
use burngfx::items::ItemTypes;
use burngfx::modern_ai::attack::*;
use burngfx::modern_ai::camps::*;
use burngfx::modern_ai::decision::{Action, Decision};
use burngfx::modern_ai::encounter::{Fighter, Rules};
use burngfx::modern_ai::group::{self, Items};
use burngfx::modern_ai::hiring::{attack_size, Planner};
use burngfx::modern_ai::needs::{Needs, Plans, World};
use burngfx::modern_ai::policy::{AiConfig, AiPolicy};
use burngfx::modern_ai::reason::Names;
use burngfx::modern_ai::recruitment::Candidate;
use burngfx::modern_ai::state::ModernAi;
use burngfx::modern_ai::trading::Barter;
use burngfx::production::Productions;
use burngfx::rival::Rival;
use burngfx::rules::RuleBook;
use burngfx::trade::Prices;

fn read(f: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/v1.2/rules/extended/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

struct T {
    types: ItemTypes,
    productions: Productions,
    prices: Prices,
    book: RuleBook,
    policy: AiPolicy,
}

impl T {
    fn new() -> T {
        let s = |n: u8| {
            format!(
                "[{n}]\nrecruit_experience = 0 -1\nslump_material_grants = 0\ntravel_group_size = 2\nattack_group_size = 3\n\
                 minimum_attack_ratio = 1.05\ncritical_garrison_target = 1\nallow_generated_recruit_payment_in_cities = true\n\
                 max_human_camp_defenders = -1\nattack_plan_turns = 40\nattack_plan_retry_delay = 10\n"
            )
        };
        T {
            types: ItemTypes::parse_v12(&read("items.txt")).unwrap(),
            productions: Productions::parse(&read("production.txt")),
            prices: Prices::parse("3 = 27\n"),
            book: RuleBook::parse(&read("game.txt")),
            policy: AiConfig::parse(&format!("{}{}{}", s(0), s(1), s(2))).unwrap().policy(1).clone(),
        }
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

fn camp(t: &T, owner: Option<usize>, roads: Vec<(usize, i32)>) -> Site {
    Site {
        city: false,
        owner,
        water: 3,
        has_products: true,
        productions: vec![t.trap(2)],
        current: Some(0),
        guards: owner.map(|o| vec![Guard { owner: o, alive: true, resting: false, protected: true }]).unwrap_or_default(),
        source_room: true,
        ..city(roads)
    }
}

/// 0 a city; 1 our camp; 2 the enemy's (player 2). A day between each.
fn map(t: &T) -> Sites {
    Sites { cache: None, sites: vec![city(vec![(1, 1), (2, 1)]), camp(t, Some(1), vec![(0, 1), (2, 1)]), camp(t, Some(2), vec![(1, 1), (0, 1)])] }
}

fn fighter(t: &T, class: CharClass, items: &[&str]) -> Fighter {
    Fighter { class, experience: 20, health: 100, pack: ItemBag::with(6, &items.iter().map(|n| t.k(n)).collect::<Vec<_>>()), kit: Kit::default(), human: false }
}

struct Titles;
impl Names for Titles {
    fn place(&self, l: usize) -> String {
        ["Ruins", "Our Camp", "Their Camp"][l].to_string()
    }
    fn person(&self, r: usize) -> String {
        format!("Mr {r}")
    }
}

/// The group, as the rival and as fighters.
fn group_of(t: &T, armed: bool) -> (Rival, Vec<Fighter>) {
    let mut r = Rival::new(1, 0);
    r.needs = CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health: 100 };
    let id = r.party.recruit();
    r.party.companions.iter_mut().find(|c| c.id == id).unwrap().needs = r.needs;
    let items: &[&str] = if armed { &["item_axe"] } else { &[] };
    if armed {
        r.pack = ItemBag::with(6, &[t.k("item_axe")]);
        r.party.companions[0].pack = ItemBag::with(6, &[t.k("item_axe")]);
    }
    (r, vec![fighter(t, CharClass::Boss, items), fighter(t, CharClass::Mercenary, items)])
}

fn world<'a>(t: &'a T, sites: &'a Sites, here: usize, day: i32, demand: &'a [i32]) -> World<'a> {
    World { items: t.items(), productions: &t.productions, sites, player: 1, policy: &t.policy, day, here, equipment_demand: demand, traders: &[] }
}

fn field<'a>(t: &'a T, w: &'a World<'a>, party: &'a [Fighter], g: &'a burngfx::modern_ai::supplies::GroupSupplies, defenders: &'a [Fighter]) -> Field<'a> {
    let rules = Rules { book: &t.book, types: &t.types, difficulty: 1, hazard: Danger::None };
    Field { w, rules, party, group: g, previous: None, humans: &[], defenders_here: defenders, travelling: false }
}

#[test]
fn the_frontier_is_reached_through_cities_and_barren_land_only() {
    let t = T::new();
    let mut sites = map(&t);
    let demand = vec![0; t.types.len()];
    let (r, party) = group_of(&t, true);
    let g = group::supplies(&r, &t.types);
    {
        let w = world(&t, &sites, 0, 5, &demand);
        assert!(is_territorial_frontier_target(&field(&t, &w, &party, &g, &[]), 2));
    }
    // Our camp gone, and a free trap site between the city and theirs: not
    // the frontier — that site should be taken first.
    sites.sites[1].owner = None;
    sites.sites[1].guards.clear();
    sites.sites[0].roads = vec![(1, 1)];
    let w = world(&t, &sites, 0, 5, &demand);
    assert!(!is_territorial_frontier_target(&field(&t, &w, &party, &g, &[]), 2));
}

#[test]
fn the_size_wanted_is_the_expected_defence_and_one() {
    let t = T::new();
    let sites = map(&t);
    let demand = vec![0; t.types.len()];
    // Day 300, the owner there since day 0: fortification 0.05 + 0.35 +
    // 0.25 + 0.025 (one camp) = 0.675 >= 0.55, water 3 and two fed at full
    // tools: two expected (`Estimate`), and one more (`RequiredAttackGroupSize`).
    let (r, unarmed) = group_of(&t, false);
    let g = group::supplies(&r, &t.types);
    let w = world(&t, &sites, 0, 300, &demand);
    let mut ai = ModernAi::new(1, 5);
    burngfx::modern_ai::intel::update_knowledge(&mut ai, 0, [(1, Some(1)), (2, Some(2))]);
    let f = field(&t, &w, &unarmed, &g, &[]);
    assert_eq!(estimate(&mut ai, &f, 2).expected_defenders, 2);
    assert_eq!(required_attack_group_size(&mut ai, &f, 2, &t.policy), 3, "unarmed: not suitable, so the full three");
    // Armed and strong enough already: the two there are (`available`).
    let (_, armed) = group_of(&t, true);
    let f = field(&t, &w, &armed, &g, &[]);
    assert!(is_suitable(&mut ai, &f, 2, &t.policy), "{} against {}", attacker_strength(&f), estimate(&mut ai, &f, 2).estimated_strength);
    assert_eq!(required_attack_group_size(&mut ai, &f, 2, &t.policy), 2);
}

fn hire_decisions(t: &T, people: Vec<Vec<Candidate>>) -> Vec<Decision> {
    let sites = map(t);
    let demand = vec![0; t.types.len()];
    let (r, unarmed) = group_of(t, false);
    let g = group::supplies(&r, &t.types);
    let w = world(t, &sites, 0, 300, &demand);
    let mut ai = ModernAi::new(1, 5);
    burngfx::modern_ai::intel::update_knowledge(&mut ai, 0, [(1, Some(1)), (2, Some(2))]);
    ai.target = Some(2);
    let size = {
        let f = field(t, &w, &unarmed, &g, &[]);
        let target = ai.target;
        attack_size(&mut ai, &f, target, true, &t.policy)
    };
    assert_eq!(size, Some(3));
    let needs = Needs::new(&r, &ai, &w, Plans::default());
    let kit = Kit::default();
    let p = Planner::new(&w, &needs, &r, &kit, &ai, Barter { factor: 100, benefit: 1.0 }, None, &people, &[], &[]);
    let mut out = vec![];
    p.add_candidates(ai.target, size, false, &mut out);
    out
}

#[test]
fn recruitment_gathers_the_attack_its_planning_asks_for() {
    let t = T::new();
    let man = Candidate { record: 40, name: "Lee".into(), class: CharClass::Mercenary, experience: 10, health: 100, hire_items: vec![], pack: ItemBag::new(6), available: true };
    // Two of three: no staging camp (ours can feed one guard, not the two
    // to stage), nothing to recall here, a safe recruit in this city:
    // `AddRecruitOrCityCandidate` at 1080.
    let d = hire_decisions(&t, vec![vec![man], vec![], vec![]]);
    assert_eq!(d.iter().map(|d| (d.action, d.score, d.target, d.recruit)).collect::<Vec<_>>(), vec![(Action::Recruit, 1080.0, Some(0), Some(40))]);
    assert_eq!(d[0].reason.text(&Titles), "recruit toward 3 attackers for Their Camp");
    // Nobody here and no other city: `AddFailedCityAttackRecruitmentCancellation`.
    let d = hire_decisions(&t, vec![vec![], vec![], vec![]]);
    assert_eq!(d.iter().map(|d| (d.action, d.score, d.target)).collect::<Vec<_>>(), vec![(Action::CancelAttackPlan, 1200.0, Some(2))]);
    assert_eq!(d[0].reason.text(&Titles), "cancel attack on Their Camp: no safe, affordable recruit is available at Ruins");
}

#[test]
fn in_a_hostile_camp_attack_or_fall_back() {
    let t = T::new();
    let sites = map(&t);
    let demand = vec![0; t.types.len()];
    let w = world(&t, &sites, 2, 5, &demand);
    let guard = [fighter(&t, CharClass::Mercenary, &[])];
    // Armed, one bare-handed guard: attack.
    let (r, armed) = group_of(&t, true);
    let g = group::supplies(&r, &t.types);
    let mut ai = ModernAi::new(1, 5);
    let mut out = vec![];
    assert!(try_add_immediate_response(&mut ai, &field(&t, &w, &armed, &g, &guard), &t.policy, &mut out));
    assert_eq!((out[0].action, out[0].score, out[0].reason.text(&Titles)), (Action::AttackHostile, 1200.0, "hostile camp blocks the current route".into()));
    // Unarmed, with a plan on it: the plan is put aside, and the group goes
    // back to our camp, a supplied day away.
    let (r, unarmed) = group_of(&t, false);
    let g = group::supplies(&r, &t.types);
    let mut ai = ModernAi::new(1, 5);
    ai.target = Some(2);
    let mut out = vec![];
    // From inside their camp the only road is back the way the group came
    // (`RouteFinder.Find`, retreat only): it came from our camp.
    let came_from_ours = Field { previous: Some(1), ..field(&t, &w, &unarmed, &g, &guard) };
    try_add_immediate_response(&mut ai, &came_from_ours, &t.policy, &mut out);
    assert_eq!((out[0].action, out[0].score, out[0].target, out[0].next_step), (Action::Travel, 1250.0, Some(1), Some(1)));
    assert_eq!(out[0].reason.text(&Titles), "retreat toward the nearest reachable safe location");
    assert_eq!((ai.target, ai.deferrals.len(), ai.deferrals[0].retry_day), (None, 1, 15));
    // What was seen of the defence is remembered.
    assert_eq!(burngfx::modern_ai::intel::observation(&ai, 2).map(|o| (o.defenders, o.observed_day)), Some((1, 5)));
    // Not knowing where it came from, it has no road out: it waits.
    let mut ai = ModernAi::new(1, 5);
    let mut out = vec![];
    try_add_immediate_response(&mut ai, &field(&t, &w, &unarmed, &g, &guard), &t.policy, &mut out);
    assert_eq!((out[0].action, out[0].reason.text(&Titles)), (Action::Wait, "will not initiate an unsuitable attack without a retreat route".into()));
}

#[test]
fn a_plan_runs_out_and_is_put_aside() {
    let t = T::new();
    let sites = map(&t);
    let demand = vec![0; t.types.len()];
    let (mut r, armed) = group_of(&t, true);
    // No water at all: the attack's road (one day, two of water each with
    // the fight) is not supplied, so the plan is never ready.
    r.needs.water = 0;
    r.party.companions[0].needs.water = 0;
    let g = group::supplies(&r, &t.types);
    let route = burngfx::modern_ai::route::find(&sites, 1, None, 0, 2).unwrap();
    let mut ai = ModernAi::new(1, 5);
    burngfx::modern_ai::intel::update_knowledge(&mut ai, 0, [(1, Some(1)), (2, Some(2))]);
    let w300 = world(&t, &sites, 0, 300, &demand);
    start_attack_plan(&mut ai, &field(&t, &w300, &armed, &g, &[]), 2, &t.policy);
    assert_eq!(ai.attack_plan_until, Some(340));
    // Day 340: still in time, kept.
    let w = world(&t, &sites, 0, 340, &demand);
    assert_eq!(validate_persistent_target(&mut ai, &field(&t, &w, &armed, &g, &[]), 2, &route, &t.policy), Some(2));
    // Day 341 (`IsAttackPlanExpired`: day > until): put aside for ten days
    // (`attack_plan_retry_delay`), the target dropped.
    let w = world(&t, &sites, 0, 341, &demand);
    assert_eq!(validate_persistent_target(&mut ai, &field(&t, &w, &armed, &g, &[]), 2, &route, &t.policy), None);
    assert_eq!((ai.target, ai.deferrals[0].retry_day), (None, 351));
    // Watered, the same plan is ready on day 341 and its deadline goes.
    let (r, armed) = group_of(&t, true);
    let g = group::supplies(&r, &t.types);
    let mut ai = ModernAi::new(1, 5);
    burngfx::modern_ai::intel::update_knowledge(&mut ai, 0, [(1, Some(1)), (2, Some(2))]);
    let w300 = world(&t, &sites, 0, 300, &demand);
    start_attack_plan(&mut ai, &field(&t, &w300, &armed, &g, &[]), 2, &t.policy);
    assert_eq!(validate_persistent_target(&mut ai, &field(&t, &w, &armed, &g, &[]), 2, &route, &t.policy), Some(2));
    assert_eq!(ai.attack_plan_until, None);
}

#[test]
fn a_deferral_lasts_until_its_day_or_a_change() {
    use burngfx::modern_ai::deferrals::*;
    let mut ai = ModernAi::new(1, 5);
    let now = Progress { supplies: 3, group: 2, strength: 40.0 };
    defer_for_turns(&mut ai, 10, 2, Some(2), 10, &now);
    assert!(is_deferred(&mut ai, 19, 2, Some(2), &now));
    assert!(!is_deferred(&mut ai, 20, 2, Some(2), &now), "the retry day");
    assert!(ai.deferrals.is_empty(), "and forgotten");
    defer_for_turns(&mut ai, 10, 2, Some(2), 10, &now);
    assert!(!is_deferred(&mut ai, 11, 2, Some(3), &now), "another owner now");
    // Supplies that rose do not wake a route-supplies blocker; a bigger group
    // wakes an attackers' one.
    let route = burngfx::modern_ai::reason::Reason::AttackWaiting { target: 2, why: burngfx::modern_ai::reason::Waiting::RouteSupplies }.text(&Titles);
    try_defer(&mut ai, 10, 2, Some(2), &route, &now);
    assert!(is_deferred(&mut ai, 12, 2, Some(2), &Progress { supplies: 9, ..now }));
    let attackers = burngfx::modern_ai::reason::Reason::AttackWaiting { target: 2, why: burngfx::modern_ai::reason::Waiting::Attackers { size: 3 } }.text(&Titles);
    try_defer(&mut ai, 10, 2, Some(2), &attackers, &now);
    assert!(!is_deferred(&mut ai, 12, 2, Some(2), &Progress { group: 3, ..now }));
    // Sixteen at most: the oldest goes.
    for target in 0..17 {
        defer_for_turns(&mut ai, 10, target, None, 10, &now);
    }
    assert_eq!(ai.deferrals.len(), 16);
    assert!(!ai.deferrals.iter().any(|d| d.target == 0));
}

/// `TryClassify` against every sentence the port's reasons write: the
/// blockers upstream finds in them, and none in the rest.
#[test]
fn which_reasons_put_a_target_aside() {
    use burngfx::modern_ai::deferrals::classify;
    use burngfx::modern_ai::reason::{Reason, Waiting};
    use burngfx::modern_ai::state::Blocker;
    let w = |why| Reason::AttackWaiting { target: 2, why };
    let blocking = [
        (w(Waiting::RouteSupplies), Blocker::RouteSupplies),
        (w(Waiting::Attackers { size: 3 }), Blocker::Attackers),
        (w(Waiting::CombatReadiness), Blocker::CombatReadiness),
        (Reason::DelayRecruitment { target: 1 }, Blocker::SettlementSupplies),
    ];
    for (r, b) in &blocking {
        assert_eq!(classify(&r.text(&Titles)), Some(*b), "{}", r.text(&Titles));
    }
    // "assemble 3 attackers" and "recruit toward 3 attackers" say attackers
    // but not "waiting for"; the other waits name no blocker.
    for r in [
        w(Waiting::Weapon),
        w(Waiting::NotFrontier),
        w(Waiting::NoRoute),
        w(Waiting::CampWater),
        w(Waiting::LegalAdvance),
        Reason::RecruitAttackers { size: 3, target: 2 },
        Reason::RecallToAssemble { size: 3, target: 2 },
        Reason::DeferAttackNoAssembly { target: 2, size: 3 },
        Reason::CollectRecruitmentSupplies(Box::new(Reason::RecruitAttackers { size: 3, target: 2 })),
        Reason::ProvisionBeforeSettler { target: 1 },
        Reason::HostileCampBlocksRoute,
    ] {
        assert_eq!(classify(&r.text(&Titles)), None, "{}", r.text(&Titles));
    }
}

/// `RecordAttack`: a blow on one of ours marks the place contested; from a
/// human it also starts retaliation, reported in upstream's words; our own
/// blows and a creature's (no player) do nothing.
#[test]
fn a_blow_on_our_people_is_remembered() {
    use burngfx::modern_ai::attack::record_attack;
    use burngfx::modern_ai::telemetry::{resolve, Telemetry};
    let t = T::new();
    let mut ai = ModernAi::new(1, 5);
    ai.telemetry = Telemetry::on();
    record_attack(&mut ai, 1, 30, None, false, 2, &t.policy);
    record_attack(&mut ai, 1, 30, Some(1), false, 2, &t.policy);
    assert_eq!((ai.contested, ai.retaliating), (None, None));
    record_attack(&mut ai, 1, 30, Some(2), false, 2, &t.policy);
    assert_eq!((ai.contested, ai.retaliating), (Some((2, 30 + t.policy.contested_camp_memory_turns)), None));
    record_attack(&mut ai, 1, 31, Some(0), true, 2, &t.policy);
    assert_eq!(ai.retaliating, Some((0, 31 + t.policy.retaliation_turns)));
    let lines: Vec<String> = ai.telemetry.take().iter().map(|l| resolve(l, &Titles)).collect();
    assert_eq!(lines, ["will retaliate against Mr 0 after the attack at Their Camp"]);
}

#[test]
fn a_human_who_strikes_first_may_be_attacked_back() {
    use burngfx::modern_ai::attack::record_attack;
    // Player 2 is human, and on easy no human camp with a defender is
    // attacked (`max_human_camp_defenders = 0`) — until that human attacks
    // our people: then, for `retaliation_turns`, it may be
    // (`IsTargetAllowed`: `IsRetaliatingAgainst`).
    let t = T::new();
    let mut policy = t.policy.clone();
    policy.max_human_camp_defenders_to_attack = 0;
    let sites = map(&t);
    let demand = vec![0; t.types.len()];
    let (r, armed) = group_of(&t, true);
    let g = group::supplies(&r, &t.types);
    let day = 30;
    let w = world(&t, &sites, 0, day, &demand);
    let rules = Rules { book: &t.book, types: &t.types, difficulty: 1, hazard: Danger::None };
    let f = Field { w: &w, rules, party: &armed, group: &g, previous: None, humans: &[2], defenders_here: &[], travelling: false };
    let mut ai = ModernAi::new(1, 5);
    assert_eq!(estimate(&mut ai, &f, 2).expected_defenders, 1);
    assert!(!is_target_allowed(&mut ai, &f, 2, &policy));
    // A machine's blow is remembered as contest, not as a reason.
    record_attack(&mut ai, 1, day, Some(2), false, 1, &policy);
    assert!(!is_target_allowed(&mut ai, &f, 2, &policy));
    record_attack(&mut ai, 1, day, Some(2), true, 1, &policy);
    assert!(is_target_allowed(&mut ai, &f, 2, &policy));
    // And no longer once the retaliation has run out.
    let later = world(&t, &sites, 0, day + policy.retaliation_turns + 1, &demand);
    let f = Field { w: &later, ..f };
    assert!(!is_target_allowed(&mut ai, &f, 2, &policy));
}

#[test]
fn the_inventory_shows_the_blow_in_hand_and_the_armour_worn() {
    // `TableCombat.EquippedPreview`. The axe's rows are 9 11 15 20 /
    // 9 12 17 22 / 10 14 19 25 / 13 17 21 25 and a tier is 25 combat
    // experience. A mercenary (a fight class) with 30 is in the second row;
    // a boss (not one) with 30 fights with 15 and is in the first. The
    // leather jacket's 20 counts only where armour does (Extended's
    // `remaster_armour`), not under Classic's `dos_unarmoured`.
    let t = T::new();
    let classic = RuleBook::parse(&std::fs::read_to_string(format!("{}/testdata/v1.2/rules/classic/game.txt", env!("CARGO_MANIFEST_DIR"))).unwrap());
    let pack = ItemBag::with(6, &[t.k("item_axe"), t.k("item_leather_jacket")]);
    let armed = |class, experience, kit| Fighter { class, experience, health: 100, pack: pack.clone(), kit, human: true };
    let worn = Kit { weapon: Some(0), protection: Some(1) };
    let extended = Rules { book: &t.book, types: &t.types, difficulty: 1, hazard: Danger::None };
    let classic = Rules { book: &classic, ..extended };
    assert_eq!(extended.equipped_preview(&armed(CharClass::Mercenary, 30, worn)), (9, 22, Some(20)));
    assert_eq!(classic.equipped_preview(&armed(CharClass::Mercenary, 30, worn)), (9, 22, None));
    assert_eq!(extended.equipped_preview(&armed(CharClass::Boss, 30, worn)), (9, 20, Some(20)));
    // Nothing in hand: the unarmed row, whatever is in the pack; nothing
    // worn: no defence shown.
    let (min, max, defense) = extended.equipped_preview(&armed(CharClass::Mercenary, 30, Kit::default()));
    let unarmed = burngfx::table_combat::preview(t.types.unarmed(), 30, 25).unwrap();
    assert_eq!(((min, max), defense), (unarmed, None));
}

#[test]
fn hazard_protection_is_the_piece_worn_or_an_immune_face() {
    use burngfx::v12_items::hazard_protection_rate;
    let t = T::new();
    let pack = ItemBag::with(6, &[t.k("item_gas_mask"), t.k("item_leather_jacket")]);
    assert_eq!(hazard_protection_rate(&pack, Some(0), Danger::Gas, 3, &[], &t.types), 100);
    assert_eq!(hazard_protection_rate(&pack, Some(0), Danger::Radiation, 3, &[], &t.types), 0);
    // The mask carried but the jacket worn: nothing against gas.
    assert_eq!(hazard_protection_rate(&pack, Some(1), Danger::Gas, 3, &[], &t.types), 0);
    // A face immune to gas, wearing nothing.
    assert_eq!(hazard_protection_rate(&pack, None, Danger::Gas, 3, &[3], &t.types), 100);
}
