//! `modern_ai::camps`: v1.2's `CampEconomy` and `EconomicReturn`, on places
//! made by hand with Extended's production and item tables.

use burngfx::gamdat::Danger;
use burngfx::items::ItemTypes;
use burngfx::modern_ai::camps::*;
use burngfx::modern_ai::group::Items;
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
        // Meat (sprite 3) at 27/4, rats (1) at 8/4, snake (2) at 21/4.
        T { types: ItemTypes::parse_v12(&read("items.txt")).unwrap(), productions: Productions::parse(&read("production.txt")), prices: Prices::parse("1 = 8\n2 = 21\n3 = 27\n") }
    }
    fn items(&self) -> Items<'_> {
        Items { types: &self.types, prices: &self.prices }
    }
    fn k(&self, n: &str) -> usize {
        self.types.index_of(n).unwrap()
    }
    /// A production by its tool's name, with `installed` tools.
    fn prod(&self, tool: &str, installed: i32) -> SiteProduction {
        let id = self.productions.iter().position(|p| p.tool == tool).unwrap();
        let p = self.productions.get(id).unwrap().clone();
        SiteProduction { id, produce: self.k(&p.produce), production: p, installed }
    }
}

fn site(water: i32, owner: Option<usize>, productions: Vec<SiteProduction>, current: Option<usize>, guards: Vec<Guard>) -> Site {
    Site {
        city: false,
        owner,
        danger: Danger::None,
        water,
        source_reserve: 0,
        has_products: true,
        productions,
        current,
        guards,
        stored: vec![],
        elsewhere: vec![],
        guard_items: vec![],
        stock: 0,
        roads: vec![],
        bonus: 0,
        source_room: true,
        pump: false,
        threatened: false,
    }
}

fn guard(owner: usize) -> Guard {
    Guard { owner, alive: true, resting: false, protected: false }
}

#[test]
fn surplus_is_what_the_guards_leave() {
    let t = T::new();
    let i = t.items();
    // Two traps, two guards: amount2 is missing, so amount: 7 a day.
    let s = site(3, Some(0), vec![t.prod("item_trap", 2)], Some(0), vec![guard(0), guard(0)]);
    assert_eq!((s.food_per_day(i), s.food_surplus(i), s.water_surplus()), (7, 5, 1));
    // A guard with a bible eats and drinks nothing; a dead one neither.
    let s2 = site(3, Some(0), vec![t.prod("item_trap", 2)], Some(0), vec![guard(0), Guard { resting: true, ..guard(0) }, Guard { alive: false, ..guard(0) }]);
    assert_eq!((s2.food_surplus(i), s2.water_surplus(), s2.living_guards(0)), (6, 2, 2));
    assert!(s2.well_established(i) && !s.well_established(i), "two to spare of each");
    // Nobody's place makes nothing.
    assert_eq!(site(3, None, vec![t.prod("item_trap", 2)], Some(0), vec![]).food_per_day(i), 0);
}

#[test]
fn water_for_a_visiting_group() {
    let t = T::new();
    let i = t.items();
    let mut s = site(2, Some(0), vec![], None, vec![guard(0)]);
    assert!(!s.can_provision_group_water(i, 2), "one to spare, two visiting");
    assert!(s.needs_reusable_water_reserve(2) && s.travel_water_bottleneck(i, 2));
    s.source_reserve = 6;
    assert!(s.can_provision_group_water(i, 2), "three days' reserve for two");
    s.source_reserve = 0;
    s.stored = vec![t.k("item_bottle")];
    assert!(s.can_provision_group_water(i, 2), "a container to fill from what is left");
    s.guards.push(guard(0));
    assert!(!s.can_provision_group_water(i, 2), "nothing left to fill it from");
}

#[test]
fn returns_weigh_the_surplus_by_the_water() {
    let t = T::new();
    let i = t.items();
    let s = site(3, Some(0), vec![t.prod("item_rat_trap", 1), t.prod("item_trap", 1)], Some(1), vec![guard(0)]);
    // One trap, one guard: 5 - 1 = 4 surplus, 4/9 of a meat at 6.75.
    let r = current(i, &s, 0);
    assert_eq!((r.food_per_day, r.surplus_food_per_day), (5, 4));
    assert!((r.sustainable_value_per_day - 4.0 / 9.0 * 6.75).abs() < 1e-4);
    let dry = Site { water: 1, ..s.clone() };
    assert!((current(i, &dry, 0).sustainable_value_per_day - r.sustainable_value_per_day * 0.5).abs() < 1e-4);
    // A second trap: 7 - 1 = 6.
    let gain = marginal_camp_improvement(i, &s, 0);
    assert!((gain - 2.0 / 9.0 * 6.75).abs() < 1e-4, "{gain}");
    // The potential picks the trap at full tools over the rats.
    assert_eq!(potential(i, &s, 1).production, Some(1));
    // A pump only counts where one is needed.
    assert_eq!(marginal_water_improvement(i, &s, 0, false), 0.0);
}

#[test]
fn what_a_place_is_worth_to_hold() {
    let t = T::new();
    let i = t.items();
    let mut sites = Sites::default();
    sites.sites.push(site(3, None, vec![t.prod("item_trap", 0)], None, vec![]));
    let mut corridor = site(1, None, vec![], None, vec![]);
    corridor.has_products = false;
    corridor.roads = vec![(0, 1), (2, 1)];
    sites.sites.push(corridor);
    let mut city = site(0, None, vec![], None, vec![]);
    city.city = true;
    sites.sites.push(city);
    // Meat 360, water 75, established 300, no route value, and the economy:
    // full tools for one, 6 surplus, 6/9 meat a day at 6.75, × 35.
    assert_eq!(sites.territorial_value(i, 0), 360 + 75 + 300 + (6.0f32 / 9.0 * 6.75 * 35.0) as i32);
    assert_eq!(sites.territorial_value(i, 1), 25 + 120, "no food: a corridor's worth");
    assert!(sites.opens_city_access(1) && !sites.opens_city_access(0));
    assert!(sites.get(0).unwrap().can_sustain_camp(i) && !sites.get(1).unwrap().can_sustain_camp(i));
    assert_eq!(sites.wanted_supply_surplus(i, 0), 1);
}
