//! `modern_ai::encounter` (v1.2's `StrategicEncounter` and the table rolls
//! it uses) against the preset tables at tag `v1.2`.

use burngfx::combat::Kit;
use burngfx::game::ItemBag;
use burngfx::gamdat::{CharClass, Danger};
use burngfx::items::ItemTypes;
use burngfx::modern_ai::encounter::*;
use burngfx::rules::RuleBook;
use burngfx::spawn::Rng;

fn testdata(rel: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/v1.2/rules/{rel}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn tables(items: &str, game: &str) -> (ItemTypes, RuleBook) {
    (ItemTypes::parse_v12(&testdata(&format!("{items}/items.txt"))).unwrap(), RuleBook::parse(&testdata(&format!("{game}/game.txt"))))
}

fn fighter(t: &ItemTypes, items: &[&str], health: i32) -> Fighter {
    let kinds: Vec<usize> = items.iter().map(|n| t.index_of(n).unwrap()).collect();
    Fighter { class: CharClass::Mercenary, experience: 20, health, pack: ItemBag::with(6, &kinds), kit: Kit::default(), human: false }
}

#[test]
fn strength_is_the_weapons_average_and_a_tenth_of_health() {
    let (t, b) = tables("extended", "classic");
    let r = Rules { book: &b, types: &t, difficulty: 1, hazard: Danger::None };
    let bare = r.strength(&fighter(&t, &[], 100), true);
    let axe = r.strength(&fighter(&t, &["item_axe"], 100), true);
    assert!(axe > bare, "{axe} > {bare}");
    assert_eq!(r.strength(&fighter(&t, &[], 50), true), bare - 5.0);
    assert_eq!(r.strength(&fighter(&t, &[], 50), false), bare, "not detailed: a flat ten");
    // Armour stretches health under Extended's rule only.
    let jacket = fighter(&t, &["item_leather_jacket"], 100);
    assert_eq!(r.strength(&jacket, true), bare, "Classic: armour does not count");
    let (te, be) = tables("extended", "extended");
    let re = Rules { book: &be, types: &te, difficulty: 1, hazard: Danger::None };
    assert!(re.strength(&fighter(&te, &["item_leather_jacket"], 100), true) > re.strength(&fighter(&te, &[], 100), true));
}

fn party(t: &ItemTypes, n: usize, weapon: &[&str], health: i32) -> Vec<Fighter> {
    (0..n).map(|_| fighter(t, weapon, health)).collect()
}

#[test]
fn a_strong_party_takes_a_weak_camp_and_the_same_seed_fights_the_same() {
    let (t, b) = tables("extended", "extended");
    let r = Rules { book: &b, types: &t, difficulty: 1, hazard: Danger::None };
    let run = |seed| {
        let mut a = party(&t, 3, &["item_axe"], 100);
        let mut d = party(&t, 1, &["item_knife"], 30);
        let o = fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false], owner: None, lifted: &[] }, false, &mut Rng::seeded(seed));
        (o, a.iter().map(|f| f.health).collect::<Vec<_>>(), d[0].health)
    };
    let (o, attackers, defender) = run(5);
    assert!(!o.attacker_withdrew && defender <= 0, "{o:?} {defender}");
    assert_eq!(run(5), (o, attackers, defender));
}

#[test]
fn the_attacker_withdraws_before_risking_the_leader() {
    let (t, b) = tables("extended", "classic");
    let r = Rules { book: &b, types: &t, difficulty: 1, hazard: Danger::None };
    // A leader at the safety health goes nowhere.
    let mut a = party(&t, 2, &["item_axe"], 100);
    a[0].health = ATTACKING_BOSS_SAFETY_HEALTH;
    let mut d = party(&t, 1, &[], 100);
    let o = fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false], owner: None, lifted: &[] }, false, &mut Rng::seeded(1));
    assert!(o.attacker_withdrew && o.rounds == 0 && d[0].health == 100);
    // Nor a leader one axe blow from it.
    let max_axe = r.maximum(&fighter(&t, &["item_axe"], 100));
    let mut a = party(&t, 2, &[], 100);
    a[0].health = ATTACKING_BOSS_SAFETY_HEALTH + max_axe - 1;
    let mut d = party(&t, 1, &["item_axe"], 100);
    let o = fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false], owner: None, lifted: &[] }, false, &mut Rng::seeded(1));
    assert!(o.attacker_withdrew && d[0].health == 100, "{o:?}");
    // A leader alone does not attack; to the death he does.
    let mut a = party(&t, 1, &["item_axe"], 100);
    let mut d = party(&t, 1, &[], 10);
    assert!(fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false], owner: None, lifted: &[] }, false, &mut Rng::seeded(1)).attacker_withdrew);
    let o = fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false], owner: None, lifted: &[] }, true, &mut Rng::seeded(1));
    assert!(!o.attacker_withdrew && d[0].dead());
}

#[test]
fn the_fight_has_its_own_round_limit() {
    let (t, b) = tables("extended", "classic");
    let r = Rules { book: &b, types: &t, difficulty: 1, hazard: Danger::None };
    let mut a = party(&t, 2, &[], 1_000_000);
    let mut d = party(&t, 2, &[], 1_000_000);
    let o = fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false, false], owner: None, lifted: &[] }, true, &mut Rng::seeded(3));
    assert_eq!((o.rounds, o.attacker_withdrew), (MAX_ROUNDS, false));
    assert!(d.iter().all(|f| !f.dead()));
}

#[test]
fn a_visiting_owner_takes_his_party_away_at_sixty_five() {
    let (t, b) = tables("extended", "classic");
    let r = Rules { book: &b, types: &t, difficulty: 1, hazard: Danger::None };
    // The camp's guard, and the owner with one follower, visiting.
    let mut d = party(&t, 3, &[], 100);
    d[1].health = DEFENDING_BOSS_SAFETY_HEALTH;
    let mut a = party(&t, 3, &["item_axe"], 100);
    let o = fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false, true, true], owner: Some(1), lifted: &[] }, false, &mut Rng::seeded(2));
    assert!(o.defending_party_disengaged);
    assert_eq!((d[1].health, d[2].health), (DEFENDING_BOSS_SAFETY_HEALTH, 100), "the party left untouched");
}

/// In a gassed place the defender puts on what keeps the gas off — the mask,
/// which stops no blow — and not his jacket; elsewhere the jacket, which
/// takes a fifth off. The blow is the same roll both times.
#[test]
fn in_a_dangerous_place_the_armour_worn_is_the_protection() {
    let (t, b) = tables("extended", "extended");
    let blow = |hazard| {
        let r = Rules { book: &b, types: &t, difficulty: 1, hazard };
        let mut attacker = fighter(&t, &["item_axe"], 100);
        let mut defender = fighter(&t, &["item_leather_jacket", "item_gas_mask"], 100);
        let d = r.roll(&mut attacker, &mut defender, &mut Rng::seeded(11));
        (d, defender.kit.protection.and_then(|s| defender.pack.get(s)).and_then(|k| t.name_of(k)).map(str::to_string))
    };
    let (gassed, worn_gassed) = blow(Danger::Gas);
    let (clear, worn_clear) = blow(Danger::None);
    assert_eq!(worn_gassed.as_deref(), Some("item_gas_mask"));
    assert_eq!(worn_clear.as_deref(), Some("item_leather_jacket"));
    assert_eq!(clear, burngfx::table_combat::apply_armour(gassed, 20), "{gassed} through the jacket");
    assert!(clear < gassed);
}

/// F22: `LivingDefenders().OrderBy(character => character.Health)`
/// (`StrategicEncounter.cs:51`) orders by the getter, which reads 0 for a
/// defender alive below one point (`Character.cs:351-354`). Of two defenders
/// the port holds at 1, the lifted one is struck first though listed second.
#[test]
fn the_lowest_health_target_is_the_getters_nought_below_one() {
    let (t, b) = tables("extended", "classic");
    let r = Rules { book: &b, types: &t, difficulty: 1, hazard: Danger::None };
    for seed in 1..6 {
        let mut a = party(&t, 3, &["item_axe"], 100);
        let mut d = party(&t, 2, &[], 1);
        let o = fight(&r, &mut a, Defence { fighters: &mut d, visiting: &[false, false], owner: None, lifted: &[false, true] }, true, &mut Rng::seeded(seed));
        assert_eq!(o.fallen.first(), Some(&(false, 1)), "seed {seed}: {o:?}");
        // Unlifted, equal health: the first.
        let mut d = party(&t, 2, &[], 1);
        let o = fight(&r, &mut a.clone(), Defence { fighters: &mut d, visiting: &[false, false], owner: None, lifted: &[] }, true, &mut Rng::seeded(seed));
        assert_eq!(o.fallen.first(), Some(&(false, 0)), "seed {seed}: {o:?}");
    }
}
