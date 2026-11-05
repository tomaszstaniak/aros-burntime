//! `AiPolicy`: `ai.txt`, one section per difficulty (`[0]` easy, `[1]`
//! normal, `[2]` hard) and the seats' difficulties under `[players]`.
//! Classic and Extended read the same file; `armour_limit` only matters
//! under Extended rules.

use alloc::string::String;
use alloc::vec::Vec;

use crate::rules::{sections, Section};

/// Difficulties `ai.txt` defines.
pub const DIFFICULTIES: usize = 3;

/// What makes `ai.txt` unusable; upstream throws on each.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyError {
    MissingSection(u8),
    RecruitExperience(u8),
    NegativeSlumpGrants(u8),
    Difficulty(String),
}

/// One difficulty's numbers. `-1` in the file for the unbounded ones becomes
/// `i32::MAX`, as upstream's `int.MaxValue`.
#[derive(Debug, Clone, PartialEq)]
pub struct AiPolicy {
    pub camp_max_advance: i32,
    pub group_size: i32,
    pub attack_group_size: i32,
    pub minimum_attack_ratio: f32,
    pub neutral_target_score: f32,
    pub hostile_target_score: f32,
    pub expansion_economy_score: f32,
    pub max_human_camp_defenders_to_attack: i32,
    pub treat_lone_knife_guard_as_undefended: bool,
    pub critical_garrison_target: i32,
    pub attack_cooldown_turns: i32,
    pub retaliation_turns: i32,
    pub contested_camp_memory_turns: i32,
    pub failed_attack_memory_turns: i32,
    pub progressing_attack_retry_turns: i32,
    pub attack_plan_turns: i32,
    pub attack_plan_retry_delay: i32,
    pub strategic_hostile_target_bonus: f32,
    pub proactive_conflict_day: i32,
    pub proactive_conflict_bonus: f32,
    pub use_detailed_combat_estimate: bool,
    pub allow_generated_recruit_payment_in_cities: bool,
    pub minimum_recruit_experience: i32,
    pub maximum_recruit_experience: i32,
    pub threat_radius: i32,
    pub armour_limit: i32,
    pub firearm_limit: i32,
    pub equipment_upgrade_chance: i32,
    pub firearm_purchase_chance: i32,
    pub recovery_water_days: i32,
    pub trade_benefit: f32,
    pub slump_material_grant_limit: i32,
    pub combat_loot_limit: i32,
    pub die_when_trapped: bool,
}

/// `ConfigSection.GetBool` with no default: 1, yes, true, on.
fn boolean(s: &Section, key: &str) -> bool {
    let v = s.string(key).trim().to_ascii_lowercase();
    matches!(v.as_str(), "1" | "yes" | "true" | "on")
}

fn unbounded(v: i32) -> i32 {
    if v < 0 { i32::MAX } else { v }
}

impl AiPolicy {
    /// `LoadPolicy`.
    pub fn from_section(s: &Section, difficulty: u8) -> Result<AiPolicy, PolicyError> {
        let experience = s.ints("recruit_experience");
        if experience.len() != 2 {
            return Err(PolicyError::RecruitExperience(difficulty));
        }
        let grants = s.int("slump_material_grants");
        if grants < 0 {
            return Err(PolicyError::NegativeSlumpGrants(difficulty));
        }
        Ok(AiPolicy {
            camp_max_advance: s.int("camp_max_advance"),
            group_size: s.int("travel_group_size"),
            attack_group_size: s.int("attack_group_size"),
            minimum_attack_ratio: s.float("minimum_attack_ratio"),
            neutral_target_score: s.float("neutral_target_score"),
            hostile_target_score: s.float("hostile_target_score"),
            expansion_economy_score: s.float("expansion_economy_score"),
            max_human_camp_defenders_to_attack: unbounded(s.int("max_human_camp_defenders")),
            treat_lone_knife_guard_as_undefended: boolean(s, "treat_lone_knife_guard_as_undefended"),
            critical_garrison_target: s.int("critical_garrison_target"),
            attack_cooldown_turns: s.int("attack_cooldown_turns"),
            retaliation_turns: s.int("retaliation_turns"),
            contested_camp_memory_turns: s.int("contested_camp_memory_turns"),
            failed_attack_memory_turns: s.int("failed_attack_memory_turns"),
            progressing_attack_retry_turns: s.int("progressing_attack_retry_turns"),
            attack_plan_turns: s.int("attack_plan_turns"),
            attack_plan_retry_delay: s.int("attack_plan_retry_delay"),
            strategic_hostile_target_bonus: s.float("strategic_hostile_target_bonus"),
            proactive_conflict_day: unbounded(s.int("proactive_conflict_day")),
            proactive_conflict_bonus: s.float("proactive_conflict_bonus"),
            use_detailed_combat_estimate: boolean(s, "use_detailed_combat_estimate"),
            allow_generated_recruit_payment_in_cities: boolean(s, "allow_generated_recruit_payment_in_cities"),
            minimum_recruit_experience: experience[0],
            maximum_recruit_experience: unbounded(experience[1]),
            threat_radius: s.int("threat_radius"),
            armour_limit: s.int("armour_limit"),
            firearm_limit: s.int("firearm_limit"),
            equipment_upgrade_chance: s.int("equipment_upgrade_chance"),
            firearm_purchase_chance: s.int("firearm_purchase_chance"),
            recovery_water_days: s.int("recovery_water_days"),
            trade_benefit: s.float("trade_benefit"),
            slump_material_grant_limit: grants,
            combat_loot_limit: s.int("combat_loot_limit").max(0),
            die_when_trapped: boolean(s, "die_when_trapped"),
        })
    }
}

/// `ai.txt` read whole: the three policies and the seats' words.
#[derive(Debug, Clone, PartialEq)]
pub struct AiConfig {
    pub policies: [AiPolicy; DIFFICULTIES],
    seats: Vec<String>,
}

/// `ParseDifficulty`: game/inherit take the game's, else easy/normal/hard or
/// their digits.
pub fn parse_difficulty(value: &str, game: u8) -> Result<u8, PolicyError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "game" | "inherit" => Ok(game.min(DIFFICULTIES as u8 - 1)),
        "easy" | "0" => Ok(0),
        "normal" | "1" => Ok(1),
        "hard" | "2" => Ok(2),
        _ => Err(PolicyError::Difficulty(String::from(value))),
    }
}

impl AiConfig {
    pub fn parse(text: &str) -> Result<AiConfig, PolicyError> {
        let all = sections(text);
        let find = |name: &str| all.iter().find(|s| s.name == name);
        let policy = |d: u8| -> Result<AiPolicy, PolicyError> {
            let name = [b'0' + d];
            let s = find(core::str::from_utf8(&name).unwrap_or("0")).ok_or(PolicyError::MissingSection(d))?;
            AiPolicy::from_section(s, d)
        };
        let seats = find("players").map(|s| s.words("difficulties")).unwrap_or_default();
        for word in &seats {
            parse_difficulty(word, 0)?;
        }
        Ok(AiConfig { policies: [policy(0)?, policy(1)?, policy(2)?], seats })
    }

    /// `SettingsForPlayer`: the seat's own word, or the game's difficulty
    /// for a seat the file does not name.
    pub fn difficulty_for(&self, seat: usize, game: u8) -> u8 {
        self.seats
            .get(seat)
            .and_then(|w| parse_difficulty(w, game).ok())
            .unwrap_or(game.min(DIFFICULTIES as u8 - 1))
    }

    pub fn policy(&self, difficulty: u8) -> &AiPolicy {
        &self.policies[(difficulty as usize).min(DIFFICULTIES - 1)]
    }
}
