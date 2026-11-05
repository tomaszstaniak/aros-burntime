//! v1.2's item tables: damage as four tiers of four rolls, and a table that
//! would make a weapon worth nothing refused rather than read as zero.

use burngfx::items::{ItemError, ItemTypes};
use burngfx::rules::RuleSet;
use std::path::PathBuf;

fn read(rel: &str) -> Option<String> {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/v1.2").join(rel)).ok()
}

#[test]
fn every_shipped_table_reads() {
    for set in RuleSet::ALL {
        let Some(text) = read(set.files().items) else { return };
        let types = ItemTypes::parse_v12(&text).unwrap_or_else(|e| panic!("{}: {e:?}", set.name()));
        assert_eq!(types.unarmed(), &[2, 3, 4, 6, 6, 8, 10, 11, 8, 10, 12, 15, 9, 11, 13, 15], "{}", set.name());
        // Every `damage` line is a full table, and none reads as nothing.
        // (`class=weapon` alone is not one: the unloaded rifle and the
        // ammunition carry the class and no damage.)
        let lines = text.lines().filter(|l| l.trim_start().starts_with("damage=")).count();
        let tables: Vec<_> = (0..types.len()).filter_map(|i| types.get(i)).filter(|t| !t.damage_table.is_empty()).collect();
        assert_eq!(tables.len() + 1, lines, "{}: the root line and one per item", set.name());
        for t in tables {
            assert_eq!(t.damage_table.len(), 16, "{} {}", set.name(), t.name);
            assert!(t.damage > 0, "{} {}", set.name(), t.name);
        }
    }
}

#[test]
fn the_knife_and_the_rifle() {
    let Some(text) = read(RuleSet::Classic.files().items) else { return };
    let types = ItemTypes::parse_v12(&text).unwrap();
    let knife = types.index_of("item_knife").unwrap();
    let k = types.get(knife).unwrap();
    assert_eq!(k.damage_table, vec![9, 11, 14, 16, 10, 13, 16, 18, 12, 14, 17, 20, 14, 16, 18, 20]);
    // (9+11+14+16+10+13+16+18+12+14+17+20+14+16+18+20) / 16 = 238 / 16 = 14.875
    assert_eq!(k.damage, 14);
    assert_eq!(k.weapon_priority, Some(25));
    assert_eq!(types.weapon_priority_of(knife), 25);
    assert_eq!(k.attack_range, 15);

    let rifle = types.get(types.index_of("item_loaded_rifle").unwrap()).unwrap();
    assert_eq!(rifle.ammo, 6);
    assert_eq!(rifle.attack_range, 32);
    assert_eq!(rifle.empty.as_deref(), Some("item_unloaded_rifle"));
    assert_eq!(rifle.load_name.as_deref(), Some("@tooltip?85"));
}

/// No `weapon_priority` line: the item's `DamageValue`.
#[test]
fn weapon_priority_defaults_to_the_average() {
    let types = ItemTypes::parse_v12("[item_club]\nclass=weapon\ndamage=4 4 4 4 8 8 8 8 8 8 8 8 8 8 8 8\n").unwrap();
    assert_eq!(types.weapon_priority_of(0), 7);
}

/// `ItemTypesData` throws; so does this, naming the item.
#[test]
fn a_table_that_is_not_one_or_sixteen_is_refused() {
    for bad in ["1 2 3 4", "1 2 3 4 5 6 7 8 9 10 11 12 13 14 15", "-1", "2 x"] {
        let text = format!("[item_club]\nclass=weapon\ndamage={bad}\n");
        assert_eq!(
            ItemTypes::parse_v12(&text).err(),
            Some(ItemError::BadDamage { item: "item_club".into(), values: bad.into() }),
            "{bad}"
        );
    }
    assert!(matches!(ItemTypes::parse_v12("damage=1 2\n[item_x]\n"), Err(ItemError::BadDamage { item, .. }) if item.is_empty()));
    // One value is a table too.
    assert_eq!(ItemTypes::parse_v12("[item_stick]\ndamage=5\n").unwrap().get(0).unwrap().damage, 5);
}

/// The v0.9.1 reader is left as it was: one number, no table.
#[test]
fn the_legacy_reader_is_unchanged() {
    let types = ItemTypes::parse("[item_knife]\nclass=weapon\ndamage=25\n");
    let k = types.get(0).unwrap();
    assert_eq!(k.damage, 25);
    assert!(k.damage_table.is_empty());
    assert!(types.unarmed().is_empty());
}
