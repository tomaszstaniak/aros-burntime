//! `modern_ai::expansion` (v1.2's `ExpansionPlanning`): the territorial
//! plan, its scores and tie-breaks, and its candidates. Extended's tables.

use burngfx::combat::Kit;
use burngfx::game::{CharacterNeeds, ItemBag};
use burngfx::gamdat::{CharClass, Danger};
use burngfx::items::ItemTypes;
use burngfx::modern_ai::attack::Field;
use burngfx::modern_ai::camps::*;
use burngfx::modern_ai::decision::{Action, Decision};
use burngfx::modern_ai::encounter::{Fighter, Rules};
use burngfx::modern_ai::expansion::*;
use burngfx::modern_ai::group::{self, Items};
use burngfx::modern_ai::needs::World;
use burngfx::modern_ai::policy::{AiConfig, AiPolicy};
use burngfx::modern_ai::reason::Names;
use burngfx::modern_ai::state::ModernAi;
use burngfx::production::Productions;
use burngfx::rival::Rival;
use burngfx::rules::RuleBook;
use burngfx::spawn::Rng;
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
                 minimum_attack_ratio = 1.05\nneutral_target_score = 770\nhostile_target_score = 560\ncritical_garrison_target = 1\n\
                 allow_generated_recruit_payment_in_cities = true\nmax_human_camp_defenders = -1\nproactive_conflict_day = -1\n\
                 attack_plan_turns = 40\nattack_plan_retry_delay = 10\n"
            )
        };
        T {
            types: ItemTypes::parse_v12(&read("items.txt")).unwrap(),
            productions: Productions::parse(&read("production.txt")),
            prices: Prices::parse("0 = 8\n3 = 27\n"),
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
    fn production(&self, tool: &str, installed: i32) -> SiteProduction {
        let id = self.productions.iter().position(|p| p.tool == tool).unwrap();
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

fn place(t: &T, tool: &str, owner: Option<usize>, roads: Vec<(usize, i32)>) -> Site {
    Site {
        city: false,
        owner,
        water: 3,
        has_products: true,
        productions: vec![t.production(tool, if owner.is_some() { 1 } else { 0 })],
        current: owner.map(|_| 0),
        guards: owner.map(|o| vec![Guard { owner: o, alive: true, resting: false, protected: true }]).unwrap_or_default(),
        source_room: true,
        ..city(roads)
    }
}

struct Titles;
impl Names for Titles {
    fn place(&self, l: usize) -> String {
        format!("Place {l}")
    }
    fn person(&self, r: usize) -> String {
        format!("Mr {r}")
    }
}

fn leader(t: &T, followers: usize, items: &[&str]) -> (Rival, Vec<Fighter>) {
    let mut r = Rival::new(1, 0);
    r.needs = CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health: 100 };
    r.pack = ItemBag::with(6, &items.iter().map(|n| t.k(n)).collect::<Vec<_>>());
    let mut party = vec![Fighter { class: CharClass::Boss, experience: 20, health: 100, pack: r.pack.clone(), kit: Kit::default(), human: false }];
    for _ in 0..followers {
        let id = r.party.recruit();
        r.party.companions.iter_mut().find(|c| c.id == id).unwrap().needs = r.needs;
        party.push(Fighter { class: CharClass::Mercenary, experience: 20, health: 100, pack: ItemBag::new(6), kit: Kit::default(), human: false });
    }
    (r, party)
}

struct Run<'a> {
    t: &'a T,
    sites: Sites,
    here: usize,
    day: i32,
    r: Rival,
    party: Vec<Fighter>,
}

impl Run<'_> {
    fn plan(&self, ai: &mut ModernAi, seed: u64, reststop_ok: bool) -> (Plan, Vec<Decision>) {
        let demand = vec![0; self.t.types.len()];
        let w = World { items: self.t.items(), productions: &self.t.productions, sites: &self.sites, player: 1, policy: &self.t.policy, day: self.day, here: self.here, equipment_demand: &demand, traders: &[] };
        let g = group::supplies(&self.r, &self.t.types);
        let rules = Rules { book: &self.t.book, types: &self.t.types, difficulty: 1, hazard: Danger::None };
        let f = Field { w: &w, rules, party: &self.party, group: &g, previous: None, humans: &[], defenders_here: &[], travelling: false };
        let kit = Kit::default();
        let th = Theatre {
            f: &f,
            r: &self.r,
            kit: &kit,
            people: &[],
            camp_rooms: &[],
            rivals_at: &[],
            dead_players: &[],
            travelling_to: None,
            neutral_expansion_allowed: true,
            critical_supplies: false,
        };
        let mut rng = Rng::seeded(seed);
        let plan = th.create_plan(ai, &self.t.policy, false, &mut rng, &mut || reststop_ok);
        let mut out = vec![];
        th.add_immediate_claim(&plan, &mut out);
        th.add_candidates(ai, &self.t.policy, &plan, &mut out);
        (plan, out)
    }
}

#[test]
fn the_first_settlement_is_the_best_near_one() {
    let t = T::new();
    // A lone leader with a trap in the city 0; free trap land at 1 (a day)
    // and at 2 (three days), alike. `SelectFirstSettlementTarget`: the same
    // NeutralTargetScore, less a hundred a day: 1.
    let (r, party) = leader(&t, 0, &["item_trap"]);
    let run = Run { t: &t, sites: Sites { cache: None, sites: vec![city(vec![(1, 1), (2, 3)]), place(&t, "item_trap", None, vec![(0, 1)]), place(&t, "item_trap", None, vec![(0, 3)])] }, here: 0, day: 5, r, party };
    let mut ai = ModernAi::new(1, 5);
    let (plan, out) = run.plan(&mut ai, 1, true);
    assert_eq!((plan.target, plan.preparing_attack, ai.target, ai.target_was_neutral), (Some(1), false, Some(1), true));
    // NeutralTargetScore = 770 + TerritorialValue (no camp yet: nothing
    // connects) + no Reststop bonus.
    assert_eq!(plan.target_score, 770.0 + run.sites.territorial_value(t.items(), 1) as f32);
    // Alone, a neutral target is no journey (`AddCandidates`): nothing yet.
    assert!(out.is_empty());
}

#[test]
fn a_maggot_camp_only_when_nothing_better_is_reachable() {
    let t = T::new();
    // Knife (maggots) land a day away; trap land unreachable: the maggots,
    // as the last resort (`hasPreferred` false). With the trap land
    // reachable at three days: the trap land, though farther.
    let (r, party) = leader(&t, 0, &["item_trap", "item_knife"]);
    let maggots = place(&t, "item_knife", None, vec![(0, 1)]);
    let lonely = Run { t: &t, sites: Sites { cache: None, sites: vec![city(vec![(1, 1)]), maggots.clone(), place(&t, "item_trap", None, vec![])] }, here: 0, day: 5, r: r.clone(), party: party.clone() };
    assert_eq!(lonely.plan(&mut ModernAi::new(1, 5), 1, true).0.target, Some(1));
    let both = Run { t: &t, sites: Sites { cache: None, sites: vec![city(vec![(1, 1), (2, 3)]), maggots, place(&t, "item_trap", None, vec![(0, 3)])] }, here: 0, day: 5, r, party };
    assert_eq!(both.plan(&mut ModernAi::new(1, 5), 1, true).0.target, Some(2));
}

#[test]
fn reststop_needs_the_bible_brought_along() {
    let t = T::new();
    let (r, party) = leader(&t, 0, &["item_trap"]);
    // Reststop (25) the only free place, a day away.
    let mut sites = vec![city(vec![(25, 1)])];
    sites.extend((1..25).map(|_| city(vec![])));
    sites.push(place(&t, "item_trap", None, vec![(0, 1)]));
    let run = Run { t: &t, sites: Sites { cache: None, sites }, here: 0, day: 5, r, party };
    // `SetSettlementTarget` fails when `PrepareReststopSettlement` does.
    let mut ai = ModernAi::new(1, 5);
    assert_eq!((run.plan(&mut ai, 1, false).0.target, ai.target), (None, None));
    let mut ai = ModernAi::new(1, 5);
    assert_eq!((run.plan(&mut ai, 1, true).0.target, ai.target), (Some(25), Some(25)));
}

/// Our camp 0 (here), with a follower; free trap land at 1 and 2, each a
/// day from us, alike in every way.
fn grown(t: &T) -> Run<'_> {
    let (r, party) = leader(t, 1, &["item_trap"]);
    Run {
        t,
        sites: Sites { cache: None, sites: vec![place(t, "item_trap", Some(1), vec![(1, 1), (2, 1)]), place(t, "item_trap", None, vec![(0, 1)]), place(t, "item_trap", None, vec![(0, 1)])] },
        here: 0,
        day: 5,
        r,
        party,
    }
}

#[test]
fn equal_places_are_split_by_the_jitter_drawn_in_their_order() {
    let t = T::new();
    let run = grown(&t);
    // Both score the same before the jitter: `Random.Next(-20, 21)` each, in
    // the order of the places, the first of the best kept.
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..40 {
        let mut ai = ModernAi::new(1, 5);
        let target = run.plan(&mut ai, seed, true).0.target.unwrap();
        // Replay the two draws: the first is place 1's, the second place 2's.
        let mut rng = Rng::seeded(seed);
        let (a, b) = (rng.below(41) as i32 - 20, rng.below(41) as i32 - 20);
        assert_eq!(target, if b > a { 2 } else { 1 }, "seed {seed}: {a} vs {b}");
        seen.insert(target);
    }
    assert_eq!(seen.len(), 2, "both win sometimes");
}

#[test]
fn a_supplied_group_advances_and_an_unsupplied_one_waits() {
    let t = T::new();
    let mut run = grown(&t);
    let mut ai = ModernAi::new(1, 5);
    let (plan, out) = run.plan(&mut ai, 3, true);
    let target = plan.target.unwrap();
    // `AddCandidates`: two of them, supplied: travel at the target's score
    // less a day.
    let travel = out.iter().find(|d| d.action == Action::Travel).unwrap();
    assert_eq!((travel.target, travel.score, travel.next_step), (Some(target), plan.target_score - 1.0, Some(target)));
    // No water left for anyone, and the camp can give none for travel:
    // `HasSettlementRouteSupplies` fails, wait at 10.
    run.r.needs.water = 0;
    run.r.party.companions[0].needs.water = 0;
    for s in &mut run.sites.sites {
        s.water = 0;
    }
    let mut ai = ModernAi::new(1, 5);
    ai.target = Some(target);
    ai.target_was_neutral = true;
    let (_, out) = run.plan(&mut ai, 3, true);
    let wait: Vec<_> = out.iter().filter(|d| d.action == Action::Wait).map(|d| (d.score, d.reason.text(&Titles))).collect();
    // The target is kept (`CanWaitForSettlementFood`: our camp here makes
    // food to spare and the way leaves our land), and the plan waits.
    assert_eq!(ai.target, Some(target));
    assert_eq!(wait, vec![(10.0, format!("settlement route to Place {target} is waiting for travel reserves"))]);
    assert!(!out.iter().any(|d| d.action == Action::Travel));
}

#[test]
fn standing_on_free_land_with_a_follower_it_is_claimed() {
    let t = T::new();
    let (r, party) = leader(&t, 1, &["item_trap"]);
    // Here is free trap land; our camp is a day away.
    let run = Run { t: &t, sites: Sites { cache: None, sites: vec![place(&t, "item_trap", None, vec![(1, 1)]), place(&t, "item_trap", Some(1), vec![(0, 1)])] }, here: 0, day: 5, r, party };
    let mut ai = ModernAi::new(1, 5);
    let (plan, out) = run.plan(&mut ai, 1, true);
    assert!(plan.can_claim_current);
    // `CurrentClaimScore`: no target elsewhere, not connecting two camps:
    // 1050 + half the territorial value.
    let expected = 1050.0 + run.sites.territorial_value(t.items(), 0) as f32 * 0.5;
    let claim = out.iter().find(|d| d.action == Action::ClaimNeutral).unwrap();
    assert_eq!((claim.score, claim.target), (expected, Some(0)));
    assert_eq!(claim.reason.text(&Titles), "claim high-potential meat/snake camp at current location");
}

#[test]
fn after_a_capture_the_opponent_pauses() {
    let t = T::new();
    let run = grown(&t);
    let mut ai = ModernAi::new(1, 5);
    ai.post_capture_pause = 2;
    let (_, out) = run.plan(&mut ai, 1, true);
    assert!(out.iter().any(|d| d.action == Action::Wait && d.score == 900.0 && d.reason.text(&Titles) == "post-capture pause"));
    assert_eq!(ai.post_capture_pause, 1);
}
