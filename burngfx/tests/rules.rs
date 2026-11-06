//! v1.2's presets read from `rules/<set>/game.txt` (`testdata/v1.2/`, taken
//! with `git show v1.2:resources/game/classic/rules/…`). Each file test skips
//! when the file is not supplied, like the rest of the data-backed tests.

use burngfx::rules::*;
use std::path::PathBuf;

fn read(rel: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/v1.2").join(rel);
    std::fs::read_to_string(path).ok()
}

fn book(set: RuleSet) -> Option<RuleBook> {
    read(set.files().game).map(|t| RuleBook::parse(&t))
}

/// **Every shipped preset names every rule, and names it rightly.** A
/// fallback would pass silently in the remaster (a log line); here it fails.
#[test]
fn the_shipped_presets_need_no_fallback() {
    for set in RuleSet::ALL {
        let Some(book) = book(set) else { return };
        assert!(book.warnings.is_empty(), "{}: {:?}", set.name(), book.warnings);
    }
}

/// The two rule presets offered by the start screen.
#[test]
fn classic_and_extended_differ_where_the_audit_says() {
    let (Some(c), Some(e)) = (book(RuleSet::Classic), book(RuleSet::Extended)) else { return };
    assert_eq!(c.rules.combat, Combat::DosUnarmoured);
    assert_eq!(e.rules.combat, Combat::RemasterArmour);
    assert_eq!(c.rules.player_setup, PlayerSetup::DosGamdat);
    assert_eq!(e.rules.player_setup, PlayerSetup::RemasterSettings);
    assert_eq!(c.rules.survival, Survival::DosDailyConsumption);
    assert_eq!(e.rules.survival, Survival::RemasterSupplyPool);
    assert_eq!(c.rules.water_output, WaterOutput::AmigaMinimum);
    assert_eq!(e.rules.water_output, WaterOutput::RemasterFixed);
    assert_eq!(c.rules.trader_refresh, TraderRefresh::DosIndividual);
    assert_eq!(e.rules.trader_refresh, TraderRefresh::RemasterRandom);
    assert_eq!((c.rules.doctor_stabilization, e.rules.doctor_stabilization), (0, 1));
    assert_eq!((c.rules.dropped_food_decay_interval, e.rules.dropped_food_decay_interval), (0, 10));
    // Shared.
    for b in [&c, &e] {
        assert_eq!(b.rules.recruitment, Recruitment::DosXpTimes15);
        assert_eq!(b.rules.boss_experience, BossExperience::DosCamps);
        assert_eq!(b.rules.spawn_method, SpawnMethod::AmigaLocationCycle);
        assert_eq!(b.rules.experience_tier_width, 25);
        assert_eq!(b.rules.start_experience, 37);
        assert_eq!(b.rules.health_cap, 95);
        assert!(b.is_fight_class("fighter") && b.is_fight_class("trader"));
        assert!(!b.is_fight_class("doctor"));
        assert_eq!([0, 1, 2].map(|l| b.barter_factor(l)), [100, 90, 80]);
    }
}

#[test]
fn difficulty_sections_carry_the_creature_tables_and_the_starts() {
    let (Some(c), Some(e)) = (book(RuleSet::Classic), book(RuleSet::Extended)) else { return };
    for d in &c.difficulty {
        assert_eq!(d.trader_attack, vec![9, 11, 13, 15]);
        assert_eq!(d.mutant_attack, vec![2, 3, 4, 6]);
        assert_eq!(d.dog_attack, vec![2, 3, 4, 6]);
        assert_eq!((d.trader_health, d.mutant_health, d.dog_health), (97, 31, 31));
        assert_eq!(d.npc_spawn, 8);
        assert_eq!(d.start_locations, vec![vec![26, 34, 19, 20], vec![31, 28, 33, 23], vec![11, 12, 17, 10], vec![5, 3, 9, 2]]);
        assert_eq!(d.mutant_drop_chance, 0.0);
    }
    assert_eq!(e.difficulty[0].trader_attack, vec![8, 16, 25, 40]);
    assert_eq!(e.difficulty[0].start_regions, 5);
    assert_eq!(e.difficulty[0].start_locations[0], vec![2, 4]);
    assert!((e.difficulty[0].mutant_drop_chance - 0.2).abs() < 1e-6);
    assert_eq!(e.difficulty[0].npc_spawn, 4);
    assert_eq!(e.difficulty[0].city_spawn_threshold, 12);
}

#[test]
fn amiga_and_dos_have_their_own_choices() {
    let (Some(a), Some(d)) = (book(RuleSet::Amiga), book(RuleSet::Dos)) else { return };
    assert_eq!(a.rules.recruitment, Recruitment::AmigaXpPlus4);
    assert_eq!(a.rules.boss_experience, BossExperience::AmigaEconomy);
    assert_eq!(a.rules.trader_refresh, TraderRefresh::AmigaGlobal);
    assert_eq!(a.rules.food_production, FoodProduction::AmigaStoreAll);
    assert_eq!(d.rules.start_locations, StartLocations::DosRotatingGroups);
    assert_eq!(d.rules.spawn_method, SpawnMethod::DosPlayerCycle);
    assert_eq!(d.rules.water_output, WaterOutput::DosProportional);
    assert_eq!([0, 1, 2].map(|l| d.barter_factor(l)), [100, 80, 60]);
    let groups = d.section("original_start_locations").expect("dos has its groups");
    assert_eq!(groups.int("groups"), 4);
    assert_eq!(groups.ints("group_3"), vec![11, 12, 4]);
}

/// `ParseRule`'s fallbacks: missing and unknown both give the default, and
/// both are reported.
#[test]
fn a_missing_or_unknown_rule_falls_back_and_says_so() {
    let book = RuleBook::parse("[rules]\ncombat = remaster_laser\n[0]\n[1]\n[2]\n");
    assert_eq!(book.rules.combat, Combat::DosUnarmoured);
    assert!(book.warnings.contains(&Warning::Unknown { key: "combat", value: "remaster_laser".into() }));
    assert!(book.warnings.contains(&Warning::Missing { key: "survival" }));
    assert_eq!(book.rules.survival, Survival::DosDailyConsumption);
    assert_eq!(book.rules.start_locations, StartLocations::AmigaRegions);
    assert_eq!(book.rules.spawn_method, SpawnMethod::Timer);

    let none = RuleBook::parse("");
    assert!(none.warnings.contains(&Warning::NoDifficulty(2)));
}

#[test]
fn barter_level_is_clamped() {
    let book = RuleBook::parse("[0]\nbarter_factor = 100\n[1]\nbarter_factor = 90\n[2]\nbarter_factor = 80\n");
    assert_eq!(book.barter_factor(-3), 100);
    assert_eq!(book.barter_factor(9), 80);
}

#[test]
fn comments_and_the_nameless_first_section() {
    let s = sections("image=gfx/dummy.png\n' a comment\ndamage=2 3 ' trailing\n[x] ' name\nk = v\n");
    assert_eq!(s[0].name, "");
    assert_eq!(s[0].ints("damage"), vec![2, 3]);
    assert_eq!(s[1].name, "x");
    assert_eq!(s[1].string("k"), "v");
}

/// Only Classic and Extended are on the start screen; Modern, Amiga and
/// None are playable, and DOS is not offered as if it were.
#[test]
fn what_the_start_screen_offers() {
    assert!(RuleSet::Classic.on_start_screen() && RuleSet::Extended.on_start_screen());
    assert!(!RuleSet::Dos.on_start_screen() && !RuleSet::Amiga.on_start_screen());
    assert_eq!(RuleSet::parse("Extended"), Some(RuleSet::Extended));
    assert_eq!(RuleSet::parse("v091"), None);
    assert!(AiProfile::Modern.implemented() && AiProfile::None.implemented() && AiProfile::Amiga.implemented());
    assert!(!AiProfile::Dos.implemented());
    assert_eq!(RuleSet::Classic.files().items, "rules/dos/items.txt");
    assert_eq!(RuleSet::Classic.files().production, "rules/amiga/production.txt");
}
