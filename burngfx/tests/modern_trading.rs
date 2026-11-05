//! `modern_ai::trading` (v1.2's `Trading.Exchange` and `Trading.Priorities`)
//! with `modern_ai::needs`, on Extended's v1.2 item table: what is bought,
//! what is paid, the state after, and refusals that change nothing.

use burngfx::combat::Kit;
use burngfx::game::{CharacterNeeds, ItemBag};
use burngfx::gamdat::Danger;
use burngfx::items::ItemTypes;
use burngfx::modern_ai::camps::{Site, Sites};
use burngfx::modern_ai::group::Items;
use burngfx::modern_ai::needs::{Needs, Plans, World};
use burngfx::modern_ai::policy::{AiConfig, AiPolicy};
use burngfx::modern_ai::state::ModernAi;
use burngfx::modern_ai::trading::*;
use burngfx::production::Productions;
use burngfx::rival::Rival;
use burngfx::trade::Prices;

fn read(f: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/v1.2/rules/extended/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

struct T {
    types: ItemTypes,
    productions: Productions,
    prices: Prices,
    sites: Sites,
    policy: AiPolicy,
    demand: Vec<i32>,
}

/// Quarters of a price, by sprite: full canteen 10, empty canteen 8, meat
/// 6.75, tires 6, fur 5, iron bars 4, gold 20, rifle 30, jacket 7.
const PRICES: &str = "3 = 27\n6 = 40\n7 = 32\n13 = 120\n45 = 16\n49 = 80\n52 = 20\n53 = 28\n54 = 24\n";

impl T {
    fn new() -> T {
        let types = ItemTypes::parse_v12(&read("items.txt")).unwrap();
        let section = |n: u8| format!("[{n}]\nrecruit_experience = 0 40\nslump_material_grants = 0\nattack_group_size = 0\narmour_limit = 10\nfirearm_limit = -1\n");
        let policy = AiConfig::parse(&format!("{}{}{}", section(0), section(1), section(2))).unwrap().policy(1).clone();
        let city = Site {
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
            roads: vec![],
            bonus: 0,
            source_room: false,
            pump: false,
            threatened: false,
        };
        let demand = vec![0; types.len()];
        T { productions: Productions::parse(&read("production.txt")), prices: Prices::parse(PRICES), sites: Sites { cache: None, sites: vec![city] }, policy, demand, types }
    }
    fn items(&self) -> Items<'_> {
        Items { types: &self.types, prices: &self.prices }
    }
    fn world(&self) -> World<'_> {
        World { items: self.items(), productions: &self.productions, sites: &self.sites, player: 1, policy: &self.policy, day: 5, here: 0, equipment_demand: &self.demand, traders: &[] }
    }
    fn k(&self, n: &str) -> usize {
        self.types.index_of(n).unwrap_or_else(|| panic!("{n}"))
    }
    fn bag(&self, names: &[&str], capacity: usize) -> ItemBag {
        ItemBag::with(capacity, &names.iter().map(|n| self.k(n)).collect::<Vec<_>>())
    }
    fn names(&self, bag: &ItemBag) -> Vec<String> {
        bag.iter().map(|k| self.types.name_of(k).unwrap().to_string()).collect()
    }
}

fn leader(food: i32, water: i32, pack: ItemBag) -> Rival {
    let mut r = Rival::new(1, 0);
    r.needs = CharacterNeeds { food, max_food: 9, water, max_water: 5, health: 100 };
    r.pack = pack;
    r
}

fn trade(t: &T, r: &mut Rival, kit: &mut Kit, ai: &mut ModernAi, stock: &mut ItemBag, benefit: f32, plans: Plans) -> Vec<Exchange> {
    let w = t.world();
    trade_with_trader(r, kit, ai, stock, &w, Barter { factor: 100, benefit }, |r, ai| Needs::new(r, ai, &w, plans), |_, _, _| {})
}

#[test]
fn a_thirsty_leader_buys_a_canteen_and_pays_just_enough() {
    let t = T::new();
    let mut r = leader(9, 1, t.bag(&["item_iron_bars", "item_fur", "item_tires"], 6));
    let (mut kit, mut ai) = (Kit::default(), ModernAi::new(1, 5));
    let mut stock = t.bag(&["item_full_canteen"], 6);
    let done = trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.0, Plans::default());
    // Cheapest first: bars, fur, tires make fifteen; the fur is handed back,
    // and bars and tires make ten, the canteen's price. A canteen is reserve
    // goods: it goes into the reserve.
    assert_eq!(done.len(), 1);
    assert_eq!((done[0].gave.clone(), done[0].got.clone()), (vec![t.k("item_iron_bars"), t.k("item_tires")], vec![t.k("item_full_canteen")]));
    assert!(done[0].strategic);
    assert_eq!(t.names(&r.pack), vec!["item_fur".to_string()]);
    assert_eq!(ai.reserve_count(t.k("item_full_canteen")), 1);
    assert_eq!(t.names(&stock), vec!["item_iron_bars".to_string(), "item_tires".into()], "the trader keeps what was given, in the order given");
}

#[test]
fn trade_benefit_makes_the_offer_go_further() {
    let t = T::new();
    let mut r = leader(9, 1, t.bag(&["item_iron_bars", "item_fur", "item_tires"], 6));
    let (mut kit, mut ai) = (Kit::default(), ModernAi::new(2, 5));
    let mut stock = t.bag(&["item_full_canteen"], 6);
    let done = trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.5, Plans::default());
    // Bars and fur, nine, are thirteen and a half at one and a half.
    assert_eq!(done[0].gave, vec![t.k("item_iron_bars"), t.k("item_fur")]);
    assert_eq!(t.names(&r.pack), vec!["item_tires".to_string()]);
}

#[test]
fn too_little_to_offer_changes_nothing() {
    let t = T::new();
    let mut r = leader(9, 1, t.bag(&["item_iron_bars"], 6));
    let (mut kit, mut ai) = (Kit::default(), ModernAi::new(1, 5));
    let mut stock = t.bag(&["item_full_canteen"], 6);
    let (r0, stock0, ai0) = (r.clone(), stock.clone(), ai.clone());
    assert!(trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.0, Plans::default()).is_empty());
    assert_eq!((r, stock, ai), (r0, stock0, ai0));
}

#[test]
fn nothing_wanted_or_nothing_there_changes_nothing() {
    let t = T::new();
    // Gold is wanted for nothing, and fur and tires (eleven) are not worth
    // trading up to it (twenty): no plan, nothing moves.
    let mut r = leader(9, 5, t.bag(&["item_fur", "item_tires"], 6));
    let (mut kit, mut ai) = (Kit::default(), ModernAi::new(1, 5));
    let mut stock = t.bag(&["item_gold"], 6);
    let before = (r.clone(), stock.clone(), ai.clone());
    assert!(trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.0, Plans::default()).is_empty());
    assert_eq!((r, stock, ai.clone()), before);
    // An empty trader: nothing at all.
    let mut empty = ItemBag::new(6);
    let mut r = leader(9, 1, t.bag(&["item_gold"], 6));
    let r0 = r.clone();
    assert!(trade(&t, &mut r, &mut kit, &mut ai, &mut empty, 1.0, Plans::default()).is_empty());
    assert_eq!(r, r0);
}

#[test]
fn a_weapon_in_hand_and_the_settlers_pay_are_never_offered() {
    let t = T::new();
    // The gold would pay, but it is kept for the settler; the knife is his
    // weapon. Nothing else: no trade, nothing touched.
    let mut r = leader(9, 1, t.bag(&["item_gold", "item_knife"], 6));
    let mut kit = Kit { weapon: Some(1), protection: None };
    let mut ai = ModernAi::new(1, 5);
    let mut stock = t.bag(&["item_full_canteen"], 6);
    let plans = Plans { settlement_payment: Some(t.k("item_gold")), ..Plans::default() };
    let (r0, s0) = (r.clone(), stock.clone());
    assert!(trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.0, plans).is_empty());
    assert_eq!((r, stock, kit.weapon), (r0, s0, Some(1)));
    // Without the reservation the gold goes — for the canteen and, with
    // what is left of its worth, the empty one too (twenty for ten alone
    // would waste more than a third of it, which upstream refuses).
    let mut r = leader(9, 1, t.bag(&["item_gold", "item_knife"], 6));
    let done = trade(&t, &mut r, &mut kit, &mut ai, &mut t.bag(&["item_full_canteen", "item_empty_canteen"], 6), 1.0, Plans::default());
    // Equal priority, the cheaper first: the empty canteen is the target and
    // the full one fills the basket.
    assert_eq!((done[0].gave.clone(), done[0].got.clone()), (vec![t.k("item_gold")], vec![t.k("item_empty_canteen"), t.k("item_full_canteen")]));
    assert_eq!((t.names(&r.pack), kit.weapon), (vec!["item_knife".to_string()], Some(0)), "the weapon slot follows the knife");
}

#[test]
fn no_room_for_what_would_be_bought_changes_nothing() {
    let t = T::new();
    // Hungry: meat is wanted. The only offer is the reserve's jacket (above
    // the armour limit, so tradeable), which frees no slot, and the pack is
    // full of the settler's pay.
    let mut r = leader(2, 5, t.bag(&["item_gold"; 6], 6));
    let mut kit = Kit::default();
    let mut ai = ModernAi::new(1, 5);
    ai.reserve_add(t.k("item_leather_jacket"));
    let mut stock = t.bag(&["item_meat"], 6);
    let plans = Plans { settlement_payment: Some(t.k("item_gold")), ..Plans::default() };
    let (r0, s0, a0) = (r.clone(), stock.clone(), ai.clone());
    assert!(trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.0, plans).is_empty());
    assert_eq!((&r, &stock, &ai), (&r0, &s0, &a0), "the jacket stays in the reserve");
    // One slot free is still not room: the leader keeps a place each for
    // water and protection (`MissingLeaderRoleSlotsAfter`).
    let mut r = leader(2, 5, t.bag(&["item_gold"; 5], 6));
    assert!(trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.0, plans).is_empty());
    // Three free: the same trade goes through, paid from the reserve.
    let mut r = leader(2, 5, t.bag(&["item_gold"; 3], 6));
    let done = trade(&t, &mut r, &mut kit, &mut ai, &mut stock, 1.0, plans);
    assert_eq!((done[0].gave.clone(), done[0].got.clone()), (vec![t.k("item_leather_jacket")], vec![t.k("item_meat")]));
    assert_eq!((ai.reserve_count(t.k("item_leather_jacket")), r.pack.len()), (0, 4));
    assert_eq!(t.names(&stock), vec!["item_leather_jacket".to_string()]);
}
