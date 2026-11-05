//! v1.2's rule presets: `rules/<set>/game.txt` read into named rules.
//!
//! From v1.2 (`27412d2`) the remaster keeps no rules in code paths of its
//! own. A game is a **rule set** — Classic or Extended from the start screen,
//! Dos or Amiga from the command line (`Logic/Generation/GameProfiles.cs`) —
//! and each rule set is a `game.txt` whose `[rules]` section names one choice
//! per area (`combat = dos_unarmoured`, `survival = remaster_supply_pool`, …)
//! plus one section of numbers per difficulty. `GameRules` then switches on
//! each choice where it applies. This module is that file, read; the
//! behaviour behind each choice lives with the area it belongs to.
//!
//! The AI is a separate choice (`AiProfile`), made on the start screen
//! independently of the rules.
//!
//! **Missing and unknown choices** fall back exactly as `GameSettings.
//! ParseRule` does — to a fixed default per key, with a warning — and the
//! warnings are kept in [`RuleBook::warnings`] so a test can insist the
//! shipped presets produce none, instead of the fallback passing unseen.
//!
//! The v0.9.1 rules this project reproduced first are not a preset here: v1.2
//! ships no rule set that is v0.9.1. They stay behind `Engine::V091` as a
//! tested reference.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// `RuleSet`: which preset a game is played by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleSet {
    /// Command line only in v1.2.
    Dos,
    /// Command line only in v1.2.
    Amiga,
    /// "Extended rules" — the start screen's default, what this project
    /// called New.
    Extended,
    /// "Classic rules" — "a faithful blend of the original DOS and Amiga
    /// implementations" (`lang/en/tooltip.txt`).
    Classic,
}

/// The four files a preset is made of (`GameDefinitions.Get`). Classic
/// borrows Dos's items and trader lists and Amiga's productions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresetFiles {
    pub game: &'static str,
    pub items: &'static str,
    pub trader: &'static str,
    pub production: &'static str,
}

impl RuleSet {
    pub const ALL: [RuleSet; 4] = [RuleSet::Dos, RuleSet::Amiga, RuleSet::Extended, RuleSet::Classic];

    pub fn name(self) -> &'static str {
        match self {
            RuleSet::Dos => "dos",
            RuleSet::Amiga => "amiga",
            RuleSet::Extended => "extended",
            RuleSet::Classic => "classic",
        }
    }

    /// `GameDefinitions.ParseRules`, without its silent fallback: a name we
    /// do not know is `None`, and the caller says so.
    pub fn parse(name: &str) -> Option<RuleSet> {
        RuleSet::ALL.into_iter().find(|r| r.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Paths relative to `resources/game/classic/`.
    pub fn files(self) -> PresetFiles {
        match self {
            RuleSet::Dos => PresetFiles {
                game: "rules/dos/game.txt",
                items: "rules/dos/items.txt",
                trader: "rules/dos/trader.txt",
                production: "rules/dos/production.txt",
            },
            RuleSet::Amiga => PresetFiles {
                game: "rules/amiga/game.txt",
                items: "rules/amiga/items.txt",
                trader: "rules/amiga/trader.txt",
                production: "rules/amiga/production.txt",
            },
            RuleSet::Classic => PresetFiles {
                game: "rules/classic/game.txt",
                items: "rules/dos/items.txt",
                trader: "rules/dos/trader.txt",
                production: "rules/amiga/production.txt",
            },
            RuleSet::Extended => PresetFiles {
                game: "rules/extended/game.txt",
                items: "rules/extended/items.txt",
                trader: "rules/extended/trader.txt",
                production: "rules/extended/production.txt",
            },
        }
    }

    /// The pack entries of this preset's four files, as `(dir, kind)` for
    /// [`crate::pack::names::v12`]: game, items, trader, production.
    pub fn pack_entries(self) -> [(&'static str, &'static str); 4] {
        let f = self.files();
        [f.game, f.items, f.trader, f.production].map(|path| {
            // "rules/<dir>/<kind>.txt"
            let rest = path.strip_prefix("rules/").unwrap_or(path);
            let (dir, file) = rest.split_once('/').unwrap_or(("", rest));
            (dir, file.strip_suffix(".txt").unwrap_or(file))
        })
    }

    /// Which presets the start screen offers (`MenuScene`'s game mode).
    pub fn on_start_screen(self) -> bool {
        matches!(self, RuleSet::Classic | RuleSet::Extended)
    }
}

/// `AiProfile`: who plays the computer's seats. Independent of the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiProfile {
    /// "Remaster AI players", the start screen's default.
    Modern,
    /// The seats are killed at creation (`GameCreation.cs`).
    None,
    /// Command line only in v1.2.
    Dos,
    /// The recovered Amiga opponent.
    Amiga,
}

impl AiProfile {
    pub const ALL: [AiProfile; 4] = [AiProfile::Modern, AiProfile::None, AiProfile::Dos, AiProfile::Amiga];

    pub fn name(self) -> &'static str {
        match self {
            AiProfile::Modern => "modern",
            AiProfile::None => "none",
            AiProfile::Dos => "dos",
            AiProfile::Amiga => "amiga",
        }
    }

    pub fn parse(name: &str) -> Option<AiProfile> {
        AiProfile::ALL.into_iter().find(|a| a.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Whether this port can actually play it. **A profile that is not
    /// implemented is never offered as playable**: choosing it is refused,
    /// not quietly replaced by another opponent or by empty seats. Modern
    /// is offered since its AROS acceptance (B7 on `0.1.0-aros3`).
    pub fn implemented(self) -> bool {
        matches!(self, AiProfile::Modern | AiProfile::None | AiProfile::Amiga)
    }
}

macro_rules! named_rule {
    ($(#[$doc:meta])* $name:ident, $key:literal, $default:ident, { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const KEY: &'static str = $key;
            /// `GameSettings`'s fallback when the key is missing or unknown.
            pub const DEFAULT: $name = $name::$default;

            pub fn parse(value: &str) -> Option<$name> {
                match value {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }

            pub fn name(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }
    };
}

named_rule!(StartLocations, "start_locations", AmigaRegions,
    { DosRotatingGroups = "dos_rotating_groups", AmigaRegions = "amiga_regions" });
named_rule!(BossExperience, "boss_experience", DosCamps,
    { DosCamps = "dos_camps", AmigaEconomy = "amiga_economy" });
named_rule!(Recruitment, "recruitment", DosXpTimes15,
    { DosXpTimes15 = "dos_xp_times_1_5", AmigaXpPlus4 = "amiga_xp_plus_4" });
named_rule!(TraderRefresh, "trader_refresh", DosIndividual,
    { DosIndividual = "dos_individual", AmigaGlobal = "amiga_global", RemasterRandom = "remaster_random" });
named_rule!(WaterOutput, "water_output", AmigaMinimum,
    { DosProportional = "dos_proportional", AmigaMinimum = "amiga_minimum", RemasterFixed = "remaster_fixed" });
named_rule!(SpawnMethod, "spawn_method", Timer,
    { Timer = "timer", DosPlayerCycle = "dos_player_cycle", AmigaLocationCycle = "amiga_location_cycle" });
named_rule!(PlayerSetup, "player_setup", DosGamdat,
    { DosGamdat = "dos_gamdat", RemasterSettings = "remaster_settings" });
named_rule!(InitialItems, "initial_items", DosGamdat,
    { DosGamdat = "dos_gamdat", RemasterSpawning = "remaster_spawning" });
named_rule!(RecruitSupplies, "recruit_supplies", DosFixed,
    { DosFixed = "dos_fixed", RemasterDifficulty = "remaster_difficulty" });
named_rule!(TraderInventory, "trader_inventory", DosGamdat,
    { DosGamdat = "dos_gamdat", RemasterRandom = "remaster_random" });
named_rule!(FoodProduction, "food_production", DosFeedThenStore,
    { DosFeedThenStore = "dos_feed_then_store", AmigaStoreAll = "amiga_store_all" });
named_rule!(Survival, "survival", DosDailyConsumption,
    { DosDailyConsumption = "dos_daily_consumption", RemasterSupplyPool = "remaster_supply_pool" });
named_rule!(ServiceValue, "service_value", DosTradeValue,
    { DosTradeValue = "dos_trade_value", RemasterNutrition = "remaster_nutrition" });
named_rule!(
    /// `CombatRule`: both use the damage tables; only `remaster_armour` lets
    /// protection take a share off a blow.
    Combat, "combat", DosUnarmoured,
    { DosUnarmoured = "dos_unarmoured", RemasterArmour = "remaster_armour" });

/// Something in the file that `GameSettings` would have covered with a
/// default. Kept rather than logged, so it can be asserted on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    /// `Missing rule '{key}'; using '{fallback}'.`
    Missing { key: &'static str },
    /// `Unknown rule '{key}={value}'; using '{fallback}'.`
    Unknown { key: &'static str, value: String },
    /// A difficulty section `[0]`, `[1]` or `[2]` is absent.
    NoDifficulty(u8),
}

/// One `[section]` of the file, its lines as they were written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub name: String,
    entries: Vec<(String, String)>,
}

impl Section {
    /// `ConfigSection.GetString`: the value, or empty.
    pub fn string(&self, key: &str) -> &str {
        self.entries.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// `GetInt`: zero when absent or not a number.
    pub fn int(&self, key: &str) -> i32 {
        self.string(key).parse().unwrap_or(0)
    }

    /// `GetFloat`: zero when absent or not a number.
    pub fn float(&self, key: &str) -> f32 {
        self.string(key).parse().unwrap_or(0.0)
    }

    /// `GetInts`: every word that is a number.
    pub fn ints(&self, key: &str) -> Vec<i32> {
        self.string(key).split_whitespace().filter_map(|w| w.parse().ok()).collect()
    }

    /// `GetStrings`: the words.
    pub fn words(&self, key: &str) -> Vec<String> {
        self.string(key).split_whitespace().map(|w| w.to_string()).collect()
    }
}

/// The file as sections, in order. Lines before the first section belong to
/// a section with an empty name, as `ConfigFile` has it.
pub fn sections(text: &str) -> Vec<Section> {
    let mut out = alloc::vec![Section::default()];
    for raw in text.lines() {
        let line = strip_comment(raw);
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let name = rest.trim_end_matches(']').trim();
            out.push(Section { name: name.to_string(), entries: Vec::new() });
            continue;
        }
        if let (Some((key, value)), Some(section)) = (line.split_once('='), out.last_mut()) {
            section.entries.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    out
}

/// A rate a second as whole thousandths, the unit
/// [`crate::danger::Exposure`] counts in: exact for the three decimals the
/// shipped files use. Nought or less is none.
fn milli(per_second: f32) -> u32 {
    if per_second > 0.0 { (per_second * 1000.0 + 0.5) as u32 } else { 0 }
}

fn strip_comment(line: &str) -> &str {
    match line.find('\'') {
        Some(at) => line[..at].trim(),
        None => line.trim(),
    }
}

/// `[rules]`, resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub start_locations: StartLocations,
    pub boss_experience: BossExperience,
    pub recruitment: Recruitment,
    pub trader_refresh: TraderRefresh,
    pub water_output: WaterOutput,
    pub spawn_method: SpawnMethod,
    pub player_setup: PlayerSetup,
    pub initial_items: InitialItems,
    pub recruit_supplies: RecruitSupplies,
    pub trader_inventory: TraderInventory,
    pub food_production: FoodProduction,
    pub survival: Survival,
    pub service_value: ServiceValue,
    pub combat: Combat,
    pub start_experience: i32,
    /// `healing_factor`: the doctor's multiplier.
    pub healing_factor: f32,
    /// `health_cap`: the most a doctor heals to.
    pub health_cap: i32,
    pub doctor_stabilization: i32,
    pub technician_food_bonus: i32,
    /// `experience_tier_width`: combat experience per damage-table tier.
    pub experience_tier_width: i32,
    /// `fight_class`: who fights with full experience (`fighter`, `trader`…).
    pub fight_class: Vec<String>,
    /// `dropped_food_decay_interval`: zero is never.
    pub dropped_food_decay_interval: i32,
    /// `gas_immune_faces`, `radiation_immune_faces`: faces the danger does
    /// not touch (`IsHazardImmune`).
    pub gas_immune_faces: Vec<i32>,
    pub radiation_immune_faces: Vec<i32>,
    /// `gas_damage_per_second`, `radiation_damage_per_second`
    /// (`HazardDamage`), in thousandths of a point: what continuous hazard
    /// costs a second unprotected. A missing key is `GetFloat`'s 0.
    pub gas_damage_milli: u32,
    pub radiation_damage_milli: u32,
}

/// One difficulty's numbers (`[0]`, `[1]`, `[2]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Difficulty {
    pub barter_factor: i32,
    pub start_regions: i32,
    /// `start_locations_1` … `start_locations_N`, one list per region.
    pub start_locations: Vec<Vec<i32>>,
    pub start_items: Vec<String>,
    pub npc_spawn: i32,
    pub city_spawn_threshold: i32,
    pub trader_respawn: i32,
    pub mutant_respawn: i32,
    pub dog_respawn: i32,
    pub trader_health: i32,
    pub mutant_health: i32,
    pub dog_health: i32,
    /// Four rolls each; a creature has no weapon.
    pub trader_attack: Vec<i32>,
    pub mutant_attack: Vec<i32>,
    pub dog_attack: Vec<i32>,
    pub mutant_drop_chance: f32,
    pub mutant_drop_type: Vec<String>,
    /// The `random_items_*` lines and the rest, as written.
    pub section: Section,
}

/// A preset's `game.txt`.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleBook {
    pub rules: Rules,
    pub difficulty: [Difficulty; 3],
    /// Every section, for what is read where it is used (`region_item_*`,
    /// `original_start_locations`).
    pub sections: Vec<Section>,
    pub warnings: Vec<Warning>,
}

impl RuleBook {
    pub fn parse(text: &str) -> RuleBook {
        let sections = sections(text);
        let empty = Section::default();
        let rules_section = sections.iter().find(|s| s.name == "rules").unwrap_or(&empty);
        let mut warnings = Vec::new();

        macro_rules! rule {
            ($t:ident) => {{
                let value = rules_section.string($t::KEY);
                if value.is_empty() {
                    warnings.push(Warning::Missing { key: $t::KEY });
                    $t::DEFAULT
                } else {
                    match $t::parse(value) {
                        Some(r) => r,
                        None => {
                            warnings.push(Warning::Unknown { key: $t::KEY, value: value.to_string() });
                            $t::DEFAULT
                        }
                    }
                }
            }};
        }

        let rules = Rules {
            start_locations: rule!(StartLocations),
            boss_experience: rule!(BossExperience),
            recruitment: rule!(Recruitment),
            trader_refresh: rule!(TraderRefresh),
            water_output: rule!(WaterOutput),
            spawn_method: rule!(SpawnMethod),
            player_setup: rule!(PlayerSetup),
            initial_items: rule!(InitialItems),
            recruit_supplies: rule!(RecruitSupplies),
            trader_inventory: rule!(TraderInventory),
            food_production: rule!(FoodProduction),
            survival: rule!(Survival),
            service_value: rule!(ServiceValue),
            combat: rule!(Combat),
            start_experience: rules_section.int("start_experience"),
            healing_factor: rules_section.float("healing_factor"),
            health_cap: rules_section.int("health_cap"),
            doctor_stabilization: rules_section.int("doctor_stabilization").max(0),
            technician_food_bonus: rules_section.int("technician_food_bonus").max(0),
            experience_tier_width: rules_section.int("experience_tier_width"),
            fight_class: rules_section.words("fight_class"),
            dropped_food_decay_interval: rules_section.int("dropped_food_decay_interval").max(0),
            gas_immune_faces: rules_section.ints("gas_immune_faces"),
            radiation_immune_faces: rules_section.ints("radiation_immune_faces"),
            gas_damage_milli: milli(rules_section.float("gas_damage_per_second")),
            radiation_damage_milli: milli(rules_section.float("radiation_damage_per_second")),
        };

        let difficulty = [0u8, 1, 2].map(|level| {
            let name = match level {
                0 => "0",
                1 => "1",
                _ => "2",
            };
            let Some(s) = sections.iter().find(|s| s.name == name) else {
                warnings.push(Warning::NoDifficulty(level));
                return Difficulty::default();
            };
            let regions = s.int("start_regions");
            let mut start_locations = Vec::new();
            let mut key = String::from("start_locations_");
            for region in 1..=regions.max(0) {
                key.truncate("start_locations_".len());
                push_num(&mut key, region as u32);
                start_locations.push(s.ints(&key));
            }
            Difficulty {
                barter_factor: s.int("barter_factor"),
                start_regions: regions,
                start_locations,
                start_items: s.words("start_items"),
                // "npc_respawn is retained as a fallback for custom and older
                // rulesets."
                npc_spawn: if s.has("npc_spawn") { s.int("npc_spawn") } else { s.int("npc_respawn") },
                city_spawn_threshold: s.int("city_spawn_threshold"),
                trader_respawn: s.int("trader_respawn"),
                mutant_respawn: s.int("mutant_respawn"),
                dog_respawn: s.int("dog_respawn"),
                trader_health: s.int("trader_health"),
                mutant_health: s.int("mutant_health"),
                dog_health: s.int("dog_health"),
                trader_attack: s.ints("trader_attack"),
                mutant_attack: s.ints("mutant_attack"),
                dog_attack: s.ints("dog_attack"),
                mutant_drop_chance: s.float("mutant_drop_chance").clamp(0.0, 1.0),
                mutant_drop_type: s.words("mutant_drop_type"),
                section: s.clone(),
            }
        });

        RuleBook { rules, difficulty, sections, warnings }
    }

    /// `GetBarterFactor(level)`: the level clamped to 0..=2.
    pub fn barter_factor(&self, level: i32) -> i32 {
        self.difficulty[level.clamp(0, 2) as usize].barter_factor
    }

    /// `IsFightClass`: `Mercenary` is written `fighter` in the file.
    pub fn is_fight_class(&self, class: &str) -> bool {
        self.rules.fight_class.iter().any(|c| c.eq_ignore_ascii_case(class))
    }

    /// `IsHazardImmune`'s list for a danger.
    pub fn immune_faces(&self, danger: crate::gamdat::Danger) -> &[i32] {
        match danger {
            crate::gamdat::Danger::Gas => &self.rules.gas_immune_faces,
            crate::gamdat::Danger::Radiation => &self.rules.radiation_immune_faces,
            crate::gamdat::Danger::None => &[],
        }
    }

    /// `HazardDamage(type)`, in thousandths of a point a second.
    pub fn hazard_damage_milli(&self, danger: crate::gamdat::Danger) -> u32 {
        match danger {
            crate::gamdat::Danger::Gas => self.rules.gas_damage_milli,
            crate::gamdat::Danger::Radiation => self.rules.radiation_damage_milli,
            crate::gamdat::Danger::None => 0,
        }
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }
}

fn push_num(s: &mut String, mut v: u32) {
    let mut digits = [0u8; 10];
    let mut n = 0;
    loop {
        digits[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
        if v == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        s.push(digits[n] as char);
    }
}

/// **What a game is.** Exactly one of the two, recorded in its save and
/// taken from it on loading.
///
/// * `Legacy` — the v0.9.1 reproduction (`Engine::V091` or the borrowed
///   `Engine::Current`, with its `Mode`). Kept for regressions and
///   comparison, reachable from the command line only, and written to a save
///   in exactly the lines it always was, so older saves stay unambiguous.
/// * `V12` — a v1.2 rule preset with an independently chosen opponent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    Legacy { engine: crate::game::Engine, mode: crate::game::Mode },
    V12 { preset: RuleSet, ai: AiProfile },
}

impl Identity {
    /// The word a save's `engine` line carries for a v1.2 game.
    pub const V12_ENGINE: &'static str = "v12";

    pub fn is_legacy(self) -> bool {
        matches!(self, Identity::Legacy { .. })
    }

    /// One line for the console: `legacy v091 original`, `v1.2 classic, AI none`.
    pub fn describe(self) -> String {
        let mut s = String::new();
        match self {
            Identity::Legacy { engine, mode } => {
                s.push_str("legacy ");
                s.push_str(engine.name());
                s.push(' ');
                s.push_str(mode.name());
            }
            Identity::V12 { preset, ai } => {
                s.push_str("v1.2 ");
                s.push_str(preset.name());
                s.push_str(", AI ");
                s.push_str(ai.name());
            }
        }
        s
    }
}
