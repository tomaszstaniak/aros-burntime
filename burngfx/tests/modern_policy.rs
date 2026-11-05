//! `modern_ai::policy`. The parser's rules on a small `ai.txt` of the
//! test's own; v1.2's real file in one integration test, which needs it in
//! `testdata` (see `testdata/README.md`) and fails when it is not there.

use burngfx::modern_ai::policy::*;

/// The keys of upstream's file, with values chosen to tell the
/// difficulties and the special values apart.
fn section(n: u8, advance: i32, defenders: i32, conflict_day: i32, experience: &str, flag: &str, grants: i32, loot: i32) -> String {
    format!(
        "[{n}]\ncamp_max_advance = {advance}\ntravel_group_size = 2\nattack_group_size = {}\nminimum_attack_ratio = 1.{n}5\n\
         neutral_target_score = 700\nhostile_target_score = 500\nexpansion_economy_score = 900\n\
         max_human_camp_defenders = {defenders}\ntreat_lone_knife_guard_as_undefended = {flag}\ncritical_garrison_target = 1\n\
         attack_cooldown_turns = 2\nretaliation_turns = 20\ncontested_camp_memory_turns = 16\nfailed_attack_memory_turns = 20\n\
         progressing_attack_retry_turns = 3\nattack_plan_turns = 40\nattack_plan_retry_delay = 10\nstrategic_hostile_target_bonus = 0\n\
         proactive_conflict_day = {conflict_day}\nproactive_conflict_bonus = 0\nuse_detailed_combat_estimate = {flag}\n\
         allow_generated_recruit_payment_in_cities = yes\nrecruit_experience = {experience}\nthreat_radius = 1\narmour_limit = 5\n\
         firearm_limit = -1\nequipment_upgrade_chance = 25\nfirearm_purchase_chance = 10\nrecovery_water_days = 2\n\
         trade_benefit = 1.25\nslump_material_grants = {grants}\ncombat_loot_limit = {loot}\ndie_when_trapped = off\n",
        n + 2
    )
}

fn file(players: &str) -> String {
    format!(
        "[players]\n' a comment\ndifficulties = {players}\n\n{}{}{}",
        section(0, 2, 0, -1, "0 40", "true", 2, -3),
        section(1, 5, 1, -1, "20 60", "1", 4, 1),
        section(2, 99999, -1, 35, "40 -1", "on", 8, 3)
    )
}

#[test]
fn each_difficulty_has_its_own_section() {
    let c = AiConfig::parse(&file("game game game game")).unwrap();
    assert_eq!((c.policy(0).camp_max_advance, c.policy(1).camp_max_advance, c.policy(2).camp_max_advance), (2, 5, 99999));
    assert_eq!((c.policy(0).attack_group_size, c.policy(2).attack_group_size), (2, 4));
    assert_eq!((c.policy(0).minimum_attack_ratio, c.policy(2).minimum_attack_ratio), (1.05, 1.25));
    assert_eq!(c.policy(1).trade_benefit, 1.25);
    assert_eq!(c.policy(9).camp_max_advance, 99999, "past hard is hard");
}

#[test]
fn booleans_and_special_values() {
    let c = AiConfig::parse(&file("game game game game")).unwrap();
    // true, 1 and on are true; off is false.
    assert!(c.policy(0).treat_lone_knife_guard_as_undefended && c.policy(1).use_detailed_combat_estimate && c.policy(2).use_detailed_combat_estimate);
    assert!(c.policy(0).allow_generated_recruit_payment_in_cities && !c.policy(0).die_when_trapped);
    // -1 is unbounded where upstream says so, and kept where it does not.
    assert_eq!(c.policy(2).max_human_camp_defenders_to_attack, i32::MAX);
    assert_eq!(c.policy(0).max_human_camp_defenders_to_attack, 0);
    assert_eq!((c.policy(0).proactive_conflict_day, c.policy(2).proactive_conflict_day), (i32::MAX, 35));
    assert_eq!((c.policy(2).minimum_recruit_experience, c.policy(2).maximum_recruit_experience), (40, i32::MAX));
    assert_eq!(c.policy(0).firearm_limit, -1);
    assert_eq!(c.policy(0).combat_loot_limit, 0, "a negative loot limit is none");
}

#[test]
fn seats_take_the_games_difficulty_or_their_own() {
    let c = AiConfig::parse(&file("game game game game")).unwrap();
    assert_eq!((c.difficulty_for(0, 1), c.difficulty_for(3, 2), c.difficulty_for(9, 0)), (1, 2, 0));
    let mine = AiConfig::parse(&file("hard easy inherit 1")).unwrap();
    assert_eq!((mine.difficulty_for(0, 0), mine.difficulty_for(1, 2), mine.difficulty_for(2, 2), mine.difficulty_for(3, 0)), (2, 0, 2, 1));
    assert_eq!(parse_difficulty("brutal", 0), Err(PolicyError::Difficulty("brutal".into())));
    assert_eq!(AiConfig::parse(&file("game brutal")), Err(PolicyError::Difficulty("brutal".into())));
}

#[test]
fn a_broken_file_is_refused() {
    assert_eq!(AiConfig::parse(&file("game").replace("recruit_experience = 20 60", "recruit_experience = 20")), Err(PolicyError::RecruitExperience(1)));
    assert_eq!(AiConfig::parse(&file("game").replace("slump_material_grants = 8", "slump_material_grants = -1")), Err(PolicyError::NegativeSlumpGrants(2)));
    assert_eq!(AiConfig::parse("[players]\n"), Err(PolicyError::MissingSection(0)));
}

/// Integration: v1.2's own `ai.txt` (`testdata/v1.2/rules/classic/ai.txt`,
/// see `testdata/README.md`). Fails, naming the file, when it is absent.
#[test]
fn the_v12_ai_txt() {
    let path = format!("{}/testdata/v1.2/rules/classic/ai.txt", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e} — take it from the remaster at tag v1.2, see testdata/README.md"));
    let c = AiConfig::parse(&text).unwrap();
    let (easy, normal, hard) = (c.policy(0), c.policy(1), c.policy(2));
    assert_eq!((easy.camp_max_advance, normal.camp_max_advance, hard.camp_max_advance), (2, 5, 99999));
    assert_eq!((easy.minimum_attack_ratio, hard.minimum_attack_ratio), (1.35, 0.75));
    assert_eq!((normal.trade_benefit, hard.trade_benefit), (1.2, 1.5));
    assert_eq!((easy.armour_limit, normal.armour_limit, hard.armour_limit), (5, 10, 25));
    assert_eq!(hard.max_human_camp_defenders_to_attack, i32::MAX);
    assert_eq!(c.difficulty_for(0, 1), 1, "upstream's seats follow the game");
}
