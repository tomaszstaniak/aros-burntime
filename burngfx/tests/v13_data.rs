//! v1.3's Extended tables (`docs/compliance-v1.3.md` M4, `e31128a`
//! "rebalance trader stock"), read the way the game reads them. They come
//! from the player's files (`testdata/v1.3`, see its README); a missing one
//! fails the test, or skips it with `BURNTIME_ALLOW_MISSING_TESTDATA`
//! (`common`).

mod common;

use burngfx::items::ItemTypes;
use burngfx::trader::StockLists;

fn read(version: &str, rel: &str) -> Option<String> {
    common::read_text(&format!("{version}/{rel}"))
}

/// The new `value=` lines, and the radios without a world limit.
#[test]
fn extended_item_values_and_limits_are_v13s() {
    let (Some(v12), Some(v13)) = (read("v1.2", "rules/extended/items.txt"), read("v1.3", "rules/extended/items.txt")) else { return };
    let (old, new) = (ItemTypes::parse_v12(&v12).unwrap(), ItemTypes::parse_v12(&v13).unwrap());
    let get = |t: &ItemTypes, n: &str| t.get(t.index_of(n).unwrap_or_else(|| panic!("{n}"))).unwrap().clone();
    for (name, value) in [("item_defective_two_way_radio", 105), ("item_bible", 144), ("item_leather_jacket", 96),
                          ("item_steel_helmet", 120), ("item_pants", 60)] {
        assert_eq!(get(&old, name).value, None, "{name} had no value in v1.2");
        assert_eq!(get(&new, name).value, Some(value), "{name}");
    }
    for name in ["item_two_way_radio", "item_defective_two_way_radio"] {
        assert_eq!(get(&old, name).trader_world_limit, 4, "{name}");
        assert_eq!(get(&new, name).trader_world_limit, 0, "{name}: no limit since v1.3");
    }
    // The other limits are untouched: the bible keeps its 2.
    assert_eq!(get(&new, "item_bible").trader_world_limit, 2);
    assert_eq!(old.len(), new.len(), "no item added or removed");
}

/// The traders' lists grew; every name in them is an item of the table.
#[test]
fn extended_trader_lists_are_v13s() {
    let (Some(items), Some(text)) = (read("v1.3", "rules/extended/items.txt"), read("v1.3", "rules/extended/trader.txt")) else { return };
    let types = ItemTypes::parse_v12(&items).unwrap();
    let lists = StockLists::parse(&text);
    // Mad Marty (179): clothing added, with its weights.
    let marty = lists.of(179);
    let at = |n: &str| marty.iter().position(|x| x == n).unwrap_or_else(|| panic!("{n}"));
    assert_eq!(lists.rates_of(179)[at("item_sweater")], 2);
    assert_eq!(lists.rates_of(179)[at("item_leather_jacket")], 1);
    // Jack the Nepper (181): two ammunition now, gloves added.
    let jack = lists.of(181);
    assert_eq!(lists.rates_of(181)[jack.iter().position(|x| x == "item_ammunition").unwrap()], 2);
    assert!(jack.iter().any(|x| x == "item_gloves"));
    // Kaiser (192) sells a defective radio now.
    assert!(lists.of(192).iter().any(|x| x == "item_defective_two_way_radio"));
    for id in 179..=201u16 {
        for name in lists.of(id) {
            assert!(types.index_of(name).is_some(), "trader {id}: {name}");
        }
    }
}
