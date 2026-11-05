//! v1.2's economy by named rules. The cases marked "upstream" are
//! `RuleFormulasTests.cs` and `OriginalStockTests.cs` at tag `v1.2`.

use burngfx::rules::{RecruitSupplies, Recruitment, RuleBook};
use burngfx::spawn::Rng;
use burngfx::trader::{StockLists, Trader};
use burngfx::v12_economy::*;

#[test]
fn recruitment_upstream() {
    let dos = Recruitment::DosXpTimes15;
    assert!(can_recruit(dos, 40, 60));
    assert!(!can_recruit(dos, 40, 61));
    assert!(can_recruit(dos, 39, 58));
    assert!(!can_recruit(dos, 39, 59));
    assert!(!can_recruit(dos, 33, 50), "Extended no longer rounds two-thirds down to 66 percent");
    assert!(can_recruit(dos, 34, 50));
    assert!(can_recruit(dos, 66, 99));
    assert!(!can_recruit(dos, 65, 99));
    assert!(can_recruit(Recruitment::AmigaXpPlus4, 40, 44));
    assert!(!can_recruit(Recruitment::AmigaXpPlus4, 40, 45));
}

#[test]
fn doctor_upstream() {
    assert_eq!(doctor_result(90, 1, 4.0, 95), 94);
    assert_eq!(doctor_result(91, 1, 4.0, 95), 95);
    assert_eq!(doctor_result(99, 0, 4.0, 95), 99);
    assert_eq!(doctor_result(80, 12, 1.0, 95), 92);
    assert_eq!(doctor_result(95, 12, 1.0, 95), 95);
    for factor in [1.0, 0.75] {
        for health in [94, 95, 96, 100] {
            assert_eq!(doctor_result(health, 0, factor, 95), health);
            assert_eq!(doctor_result(health, 12, factor, 95), health.max(95));
        }
    }
}

#[test]
fn barter_upstream() {
    assert!(accept_trade(&[6.75], &[6.0], 95), "reserve quote preserves quarter values");
    assert!(!accept_trade(&[6.75], &[6.0, 0.5], 95), "checks the entire basket");
    assert!(!accept_trade(&[], &[], 100), "rejects empty offers");
    assert!(!accept_trade(&[100.0], &[100.0], 95), "default profile factor is neutral");
    // Classic's factors are 100, 90, 80: 6.75 for 6 stands on easy and
    // normal, not on hard (5.4).
    assert!(accept_trade(&[6.75], &[6.0], 100));
    assert!(accept_trade(&[6.75], &[6.0], 90));
    assert!(!accept_trade(&[6.75], &[6.0], 80));
    // Equal sums: only the full factor takes them.
    assert!(accept_trade(&[5.25, 5.25], &[10.5], 100));
    assert!(!accept_trade(&[5.25, 5.25], &[10.5], 90));
}

#[test]
fn recruit_supplies_by_rule() {
    let mut rng = Rng::seeded(1);
    assert_eq!(recruit_supplies(RecruitSupplies::DosFixed, 2, 0, 9, 3, &mut rng), (9, 5));
    for _ in 0..50 {
        let (food, water) = recruit_supplies(RecruitSupplies::RemasterDifficulty, 0, 1, 9, 0, &mut rng);
        assert_eq!(food, 2, "floor 2 - difficulty");
        assert!((3..=5).contains(&water));
        let (food, water) = recruit_supplies(RecruitSupplies::RemasterDifficulty, 1, 4, 9, 7, &mut rng);
        assert_eq!(food, 9, "the payment fed, up to his maximum");
        assert!((1..=4).contains(&water));
        let (food, water) = recruit_supplies(RecruitSupplies::RemasterDifficulty, 2, 0, 9, 0, &mut rng);
        assert_eq!(food, 0);
        assert!((0..=2).contains(&water));
    }
}

fn book(text: &str) -> RuleBook {
    RuleBook::parse(text)
}

#[test]
fn regions_are_drawn_without_repeating() {
    let b = book(
        "[rules]\nstart_locations = amiga_regions\n[0]\nstart_regions = 4\n\
         start_locations_1 = 26 34\nstart_locations_2 = 31 28\nstart_locations_3 = 11 12\nstart_locations_4 = 5 3\n",
    );
    for seed in 0..30 {
        let got = start_locations(&b, 0, &mut Rng::seeded(seed));
        let mut regions: Vec<usize> = got
            .iter()
            .map(|l| match l.unwrap() + 1 {
                26 | 34 => 1,
                31 | 28 => 2,
                11 | 12 => 3,
                5 | 3 => 4,
                other => panic!("{other}"),
            })
            .collect();
        regions.sort();
        assert_eq!(regions, vec![1, 2, 3, 4]);
    }
}

#[test]
fn weights_are_read_after_the_last_colon() {
    let lists = StockLists::parse("[trader]\n179=item_meat:5 item_knife item_rope:x item_axe:0\n");
    assert_eq!(lists.of(179), ["item_meat", "item_knife", "item_rope", "item_axe"]);
    assert_eq!(lists.rates_of(179), [5, 1, 1, 1]);
}

/// Kinds are their own titles here.
fn trader_with(stock: &[usize]) -> Trader {
    let mut t = Trader::new(4, 179, 0);
    t.stock = burngfx::game::ItemBag::new(64);
    for &k in stock {
        t.stock.push(k);
    }
    t
}

fn stock(t: &Trader) -> Vec<usize> {
    t.stock.iter().collect()
}

#[test]
fn dos_refresh_upstream() {
    let title = |k: usize| k as i32;
    let core: Vec<usize> = (0x3d..0x41).collect();
    // "retains first food leftovers and uses type membership": food2, junk,
    // food1, core — the first food and the first non-core go. The draws
    // here are all non-zero (Rng below 8 may be 0, so check membership).
    let mut t = trader_with(&[0x36, 0x48, 0x36, 0x3d]);
    t.dos_refresh(&core, 3, &title, 0x36, &mut Rng::seeded(11));
    let s = stock(&t);
    assert!(!s.contains(&0x48), "first non-core removed");
    assert_eq!(s.iter().filter(|&&k| k == 0x36).count(), 1, "later food remains");

    // "removes at most one non-core item".
    let mut t = trader_with(&[0x48, 0x49]);
    t.dos_refresh(&[], 1, &title, 0x36, &mut Rng::seeded(1));
    assert_eq!(stock(&t), vec![0x49]);

    // "all six assortment entries and duplicate meat": three meats, day 2.
    let types = [0x3e, 0x3f, 0x40, 0x41, 0x42, 0x46];
    let mut t = trader_with(&[0x36, 0x36, 0x36]);
    t.dos_refresh(&types, 2, &title, 0x36, &mut Rng::seeded(1));
    // 62, 66, 70 have title % 4 == 2; then meat on an even day.
    assert_eq!(stock(&t), vec![0x36, 0x3e, 0x42, 0x46, 0x36]);
}

#[test]
fn dos_refresh_drops_a_held_item_one_time_in_eight() {
    let title = |k: usize| k as i32;
    let mut dropped = 0;
    for seed in 0..400 {
        let mut t = trader_with(&[0x3d]);
        t.dos_refresh(&[0x3d], 1, &title, 0x36, &mut Rng::seeded(seed));
        if stock(&t).is_empty() {
            dropped += 1;
        }
    }
    assert!((20..=80).contains(&dropped), "{dropped} of 400");
}

fn testdata(rel: &str) -> String {
    format!("{}/testdata/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn extended_types() -> burngfx::items::ItemTypes {
    burngfx::items::ItemTypes::parse_v12(&std::fs::read_to_string(testdata("v1.2/rules/extended/items.txt")).unwrap())
        .unwrap()
}

/// `GetNextItem` by weight: over many draws, meat (5) comes up about five
/// times as often as a weight-1 entry, and never anything already held.
#[test]
fn next_item_follows_the_weights() {
    let types = extended_types();
    let lists = StockLists::parse("[trader]\n179=item_meat:5 item_knife:1 item_rope:1\n");
    let t = Trader::new(4, 179, 0);
    let (mut meat, mut knife) = (0, 0);
    for seed in 0..700 {
        let k = t.next_item_v12(&lists, &types, &|_| 0, true, &mut Rng::seeded(seed)).unwrap();
        if types.name_of(k) == Some("item_meat") {
            meat += 1;
        } else if types.name_of(k) == Some("item_knife") {
            knife += 1;
        }
    }
    assert!(meat > 3 * knife, "{meat} / {knife}");
    let mut held = Trader::new(4, 179, 0);
    held.stock.push(types.index_of("item_meat").unwrap());
    held.stock.push(types.index_of("item_knife").unwrap());
    let only = held.next_item_v12(&lists, &types, &|_| 0, true, &mut Rng::seeded(1)).unwrap();
    assert_eq!(types.name_of(only), Some("item_rope"), "one choice left: no draw needed");
}

/// `IsBelowTraderWorldLimit` and Extended's one firearm.
#[test]
fn world_limits_and_one_firearm() {
    let types = extended_types();
    let limited = (0..types.len()).find(|&k| types.get(k).is_some_and(|t| t.trader_world_limit > 0)).unwrap();
    let name = types.name_of(limited).unwrap().to_string();
    let group = burngfx::trader::world_group(&types, limited);
    let limit = types.get(limited).unwrap().trader_world_limit as usize;
    let lists = StockLists::parse(&format!("[trader]\n179={name}\n"));
    let t = Trader::new(4, 179, 0);
    assert!(t.next_item_v12(&lists, &types, &|g| if g == group { limit - 1 } else { 0 }, true, &mut Rng::seeded(1)).is_some());
    assert!(t.next_item_v12(&lists, &types, &|g| if g == group { limit } else { 0 }, true, &mut Rng::seeded(1)).is_none());

    let lists = StockLists::parse("[trader]\n179=item_loaded_rifle item_loaded_pistol\n");
    let mut armed = Trader::new(4, 179, 0);
    armed.stock.push(types.index_of("item_loaded_rifle").unwrap());
    assert!(armed.next_item_v12(&lists, &types, &|_| 0, true, &mut Rng::seeded(1)).is_none(), "Extended: one firearm");
    assert!(armed.next_item_v12(&lists, &types, &|_| 0, false, &mut Rng::seeded(1)).is_some());
}

fn real_gamdat() -> Option<burngfx::gamdat::Gamdat> {
    let bytes = std::fs::read(testdata("burntime.pak")).ok()?;
    let pak = burngfx::pak::ClassicPak::open(&bytes).ok()?;
    burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()
}

/// The five cities' shops are records 21..=25, one per city in order.
#[test]
fn city_shops_are_the_five_homeless_traders() {
    let Some(g) = real_gamdat() else { return };
    let t = burngfx::trader::Traders::with_city_shops(&g.characters, &g);
    let cities: Vec<usize> = g.locations.iter().enumerate().filter(|(_, l)| l.is_city).map(|(i, _)| i).collect();
    assert_eq!(cities.len(), 5);
    for (k, &c) in cities.iter().enumerate() {
        let i = t.shop_of(c).expect("a shop");
        assert_eq!(t.get(i).unwrap().record, 21 + k);
        assert_eq!(t.get(i).unwrap().at, c);
    }
    assert_eq!(t.len(), 22, "17 wandering traders and 5 shops");
}

/// Classic's world from `gam.dat`: the human's own start, the traders'
/// stock, and the rooms used as upstream indexes them.
#[test]
fn classic_from_gamdat() {
    let Some(g) = real_gamdat() else { return };
    let types = burngfx::items::ItemTypes::parse_v12(&std::fs::read_to_string(testdata("v1.2/rules/dos/items.txt")).unwrap()).unwrap();
    let book = RuleBook::parse(&std::fs::read_to_string(testdata("v1.2/rules/classic/game.txt")).unwrap());
    let start = human_start(&book, 0, 0, &g, &types);
    assert_eq!((start.health, start.food, start.water, start.experience), (95, 9, 5, 37));
    let names: Vec<&str> = start.items.iter().map(|&k| types.name_of(k).unwrap()).collect();
    assert_eq!(names, ["item_meat", "item_bottle", "item_full_canteen", "item_knife"]);

    let ext = RuleBook::parse(&std::fs::read_to_string(testdata("v1.2/rules/extended/game.txt")).unwrap());
    let start = human_start(&ext, 0, 0, &g, &extended_types());
    assert_eq!((start.health, start.food, start.water, start.experience), (100, 9, 5, 37));

    let (_, held) = burngfx::spawn::stock_from_gamdat(&g, &types, |_| burngfx::spawn::Place::default());
    let of = |r: usize| held.iter().filter(|(rec, _)| *rec == r).map(|&(_, k)| types.name_of(k).unwrap()).collect::<Vec<_>>();
    assert_eq!(of(6), ["item_snake", "item_snake_trap", "item_knife", "item_wire", "item_ammunition", "item_axe"]);
    assert_eq!(of(54), ["item_knife"]);
    assert!(held.iter().all(|(r, _)| *r >= 4), "the player slots are their own");
}

/// `World.Update`'s faces (`Logic/World.cs:66-83`): a machine player starts
/// with face 0; each whose face is taken draws `Next(0, 5)` until it has
/// one nobody has. So with one human on face 0 the three machine players
/// have three different faces of 1 to 4, each of the four equally likely;
/// with a human on face 3, the first machine seat keeps 0.
#[test]
fn machine_players_draw_faces_nobody_has() {
    use burngfx::spawn::Rng;
    use burngfx::v12_economy::machine_faces;
    let mut rng = Rng::game(1993);
    let mut seen = [0u32; 5];
    for _ in 0..4000 {
        let faces = machine_faces(&[(0, 0)], &mut rng);
        assert_eq!(faces.iter().map(|f| f.0).collect::<Vec<_>>(), [1, 2, 3]);
        let mut ids: Vec<i32> = faces.iter().map(|f| f.1).collect();
        for &f in &ids {
            assert!((1..5).contains(&f));
            seen[f as usize] += 1;
        }
        ids.dedup();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3);
    }
    // 12,000 faces over four values: 3,000 each, a standard error near 47.
    for n in &seen[1..] {
        assert!((2800..3200).contains(n), "{seen:?}");
    }
    let faces = machine_faces(&[(1, 3)], &mut rng);
    assert_eq!(faces[0], (0, 0), "a free face is kept, without a draw");
}
