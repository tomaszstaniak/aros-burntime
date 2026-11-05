//! `modern_ai::production` and the production policy
//! (`v12_day::auto_select_food_production`), on Extended's v1.2 tables.

use burngfx::combat::Kit;
use burngfx::construction::Constructions;
use burngfx::game::{Camp, Companion, ItemBag, Pump, Storage};
use burngfx::gamdat::Danger;
use burngfx::items::ItemTypes;
use burngfx::modern_ai::camps::*;
use burngfx::modern_ai::group::Items;
use burngfx::modern_ai::production::*;
use burngfx::modern_ai::state::ModernAi;
use burngfx::production::Productions;
use burngfx::rules::{FoodProduction, Survival, WaterOutput};
use burngfx::trade::Prices;
use burngfx::v12_day::{auto_select_food_production, V12World};

fn read(f: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

struct T {
    types: ItemTypes,
    productions: Productions,
    prices: Prices,
}

impl T {
    fn new() -> T {
        T {
            types: ItemTypes::parse_v12(&read("v1.2/rules/extended/items.txt")).unwrap(),
            productions: Productions::parse(&read("v1.2/rules/extended/production.txt")),
            prices: Prices::parse("1 = 8\n2 = 21\n3 = 27\n"),
        }
    }
    fn items(&self) -> Items<'_> {
        Items { types: &self.types, prices: &self.prices }
    }
    fn k(&self, n: &str) -> usize {
        self.types.index_of(n).unwrap_or_else(|| panic!("{n}"))
    }
    fn pid(&self, tool: &str) -> usize {
        self.productions.iter().position(|p| p.tool == tool).unwrap()
    }
    fn prod(&self, tool: &str, installed: i32) -> SiteProduction {
        let id = self.pid(tool);
        let p = self.productions.get(id).unwrap().clone();
        SiteProduction { id, produce: self.k(&p.produce), production: p, installed }
    }
}

/// One place's rooms: tools by production id.
struct Rooms {
    tools: Vec<(usize, usize)>,
    valid: Vec<usize>,
}

impl Storage for Rooms {
    fn count_in_rooms(&self, _: usize, _: usize) -> usize {
        0
    }
    fn tools(&self, _: usize, production: usize) -> usize {
        self.tools.iter().find(|(p, _)| *p == production).map_or(0, |(_, n)| *n)
    }
    fn any_room_has_space(&self, _: usize) -> bool {
        true
    }
    fn store(&mut self, _: usize, _: usize) -> bool {
        true
    }
    fn pump_at(&self, _: usize) -> Pump {
        Pump::None
    }
    fn valid_productions(&self, _: usize) -> Vec<usize> {
        self.valid.clone()
    }
}

fn v12() -> V12World {
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
        cities: vec![],
    }
}

#[test]
fn the_policy_switches_a_camp_to_what_feeds_it_best() {
    let t = T::new();
    let (knife, trap) = (t.pid("item_knife"), t.pid("item_trap"));
    let camp = || Camp { owner: Some(1), residents: vec![Companion::new(1)], production: Some(knife), ..Camp::default() };
    // A trap has been put in the rooms: the knife's maggots give way to meat.
    let mut c = camp();
    let rooms = Rooms { tools: vec![(knife, 1), (trap, 1)], valid: vec![knife, trap] };
    let rate = auto_select_food_production(0, &mut c, &rooms, &t.productions, &t.types, &v12(), false);
    assert_eq!((c.production, rate), (Some(trap), 5));
    // A player's own choice does not hold against the opponent's policy,
    // and the policy does not clear the mark.
    let mut c = Camp { manual_production: true, ..camp() };
    auto_select_food_production(0, &mut c, &rooms, &t.productions, &t.types, &v12(), false);
    assert_eq!((c.production, c.manual_production), (Some(trap), true));
    // No trap in the rooms: the knife stays; nobody's place is not touched.
    let mut c = camp();
    let bare = Rooms { tools: vec![(knife, 1)], valid: vec![knife, trap] };
    auto_select_food_production(0, &mut c, &bare, &t.productions, &t.types, &v12(), false);
    assert_eq!(c.production, Some(knife));
    let mut free = Camp { owner: None, ..camp() };
    assert_eq!(auto_select_food_production(0, &mut free, &rooms, &t.productions, &t.types, &v12(), false), 0);
    assert_eq!(free.production, Some(knife));
}

fn site(_: &T, owner: Option<usize>, water: i32, productions: Vec<SiteProduction>, roads: Vec<(usize, i32)>) -> Site {
    Site {
        city: false,
        owner,
        danger: Danger::None,
        water,
        source_reserve: 0,
        has_products: true,
        current: if productions.is_empty() { None } else { Some(0) },
        productions,
        guards: if owner.is_some() { vec![Guard { owner: owner.unwrap(), alive: true, resting: false, protected: false }] } else { vec![] },
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
fn a_second_trap_moves_where_a_first_earns_more() {
    let t = T::new();
    let i = t.items();
    // Camp 0 has both traps; camp 1, a day away, has none.
    let near = Sites { cache: None, sites: vec![site(&t, Some(1), 3, vec![t.prod("item_trap", 2)], vec![(1, 1)]), site(&t, Some(1), 3, vec![t.prod("item_trap", 0)], vec![(0, 1)])] };
    assert_eq!(desired_tool_count(i, &near, 1, 0, 0), 1, "the second one belongs there");
    // Camp 1 wants both of its own (the only other camp has traps already):
    // two missing there, none beyond what camp 0 wants.
    assert_eq!(missing_tool_count(i, &near, 1, "item_meat"), 2);
    // Too far for what it would earn: keep both.
    let far = Sites { cache: None, sites: vec![site(&t, Some(1), 3, vec![t.prod("item_trap", 2)], vec![(1, 30)]), site(&t, Some(1), 3, vec![t.prod("item_trap", 0)], vec![(0, 30)])] };
    assert_eq!(desired_tool_count(i, &far, 1, 0, 0), 2);
    // A threatened camp is not one to move production to.
    let mut threatened = near.clone();
    threatened.sites[1].threatened = true;
    assert_eq!(desired_tool_count(i, &threatened, 1, 0, 0), 2);
}

#[test]
fn what_is_worth_building() {
    let t = T::new();
    let i = t.items();
    let ai = ModernAi::new(1, 5);
    // An owned camp that could trap meat and has no trap wants two: a
    // trap, worth 90 + 2 × 200.
    let sites = Sites { cache: None, sites: vec![site(&t, Some(1), 3, vec![t.prod("item_trap", 0)], vec![])] };
    let o = useful_construction_opportunities(i, &sites, 1, &ai);
    assert_eq!(o.iter().map(|o| (o.result, o.value)).collect::<Vec<_>>(), vec![("item_trap", 490)]);
    // Little water and nothing better to do: pumps too.
    let dry = Sites { cache: None, sites: vec![site(&t, Some(1), 1, vec![t.prod("item_trap", 2)], vec![])] };
    assert!(needs_pump(i, &dry.sites[0]));
    let o: Vec<_> = useful_construction_opportunities(i, &dry, 1, &ai).into_iter().map(|o| o.result).collect();
    assert!(o.contains(&"item_hand_pump") && o.contains(&"item_industrial_pump"), "{o:?}");
    let mut pumped = dry.clone();
    pumped.sites[0].pump = true;
    assert!(!needs_pump(i, &pumped.sites[0]));
}

#[test]
fn the_reserve_takes_one_of_each_part_never_a_weapon_in_hand() {
    let t = T::new();
    let i = t.items();
    let mut ai = ModernAi::new(1, 5);
    let mut leader = ItemBag::with(6, &[t.k("item_wire"), t.k("item_wire"), t.k("item_spring")]);
    let mut kit = Kit { weapon: None, protection: Some(2) };
    let mut gunner = ItemBag::with(6, &[t.k("item_loaded_rifle"), t.k("item_ammunition")]);
    let reserved = refill_construction_reserve(
        &mut ai,
        i,
        &mut [Source { bag: &mut leader, kit: Some(&mut kit) }, Source { bag: &mut gunner, kit: None }],
    );
    assert_eq!(reserved, vec!["item_wire".to_string()], "the spring is his chosen protection; the ammunition is the gun's");
    assert_eq!((leader.len(), gunner.len(), kit.protection), (2, 2, Some(1)));
    // Once there is one, a second is not reserved.
    let again = refill_construction_reserve(&mut ai, i, &mut [Source { bag: &mut leader, kit: Some(&mut kit) }]);
    assert!(again.is_empty() && ai.reserve_count(t.k("item_wire")) == 1);
}

#[test]
fn a_trap_is_built_from_the_packs_and_the_reserve_or_not_at_all() {
    let t = T::new();
    let i = t.items();
    let recipes = Constructions::parse(&read("construction.txt"));
    let sites = Sites { cache: None, sites: vec![site(&t, Some(1), 3, vec![t.prod("item_trap", 0)], vec![])] };
    // Spring and wire carried, tin in the reserve.
    let mut ai = ModernAi::new(1, 5);
    ai.reserve_add(t.k("item_tin"));
    let mut pack = ItemBag::with(6, &[t.k("item_spring"), t.k("item_knife"), t.k("item_wire")]);
    let made = construct_portable_upgrade(&mut ai, i, &sites, 1, &recipes, &mut [Source { bag: &mut pack, kit: None }]);
    assert_eq!(made, Some(t.k("item_trap")));
    assert_eq!((pack.iter().collect::<Vec<_>>(), ai.reserve_count(t.k("item_tin"))), (vec![t.k("item_knife")], 0));
    // No tin anywhere: nothing built, nothing spent.
    let mut ai = ModernAi::new(1, 5);
    let mut pack = ItemBag::with(6, &[t.k("item_spring"), t.k("item_wire")]);
    assert_eq!(construct_portable_upgrade(&mut ai, i, &sites, 1, &recipes, &mut [Source { bag: &mut pack, kit: None }]), None);
    assert_eq!(pack.len(), 2);
    // Nothing wanted: nothing built even with every part.
    let served = Sites { cache: None, sites: vec![site(&t, Some(1), 3, vec![t.prod("item_trap", 2)], vec![])] };
    let mut pack = ItemBag::with(6, &[t.k("item_spring"), t.k("item_wire"), t.k("item_tin")]);
    let mut ai = ModernAi::new(1, 5);
    let _ = construct_portable_upgrade(&mut ai, i, &served, 1, &recipes, &mut [Source { bag: &mut pack, kit: None }]);
    // (The trap may still be wanted for a free place; with none, it is not.)
    assert!(useful_construction_opportunities(i, &served, 1, &ai).iter().all(|o| o.result != "item_trap"));
}

