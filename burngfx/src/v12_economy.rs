//! v1.2's player economy by the preset's named rules: where everybody
//! starts, what the human starts with, who may be hired and with what, the
//! barter, and the doctor's price.
//!
//! All at tag `v1.2`: `GameRules.InitializeHumanPlayer`, `InitializeRecruit`,
//! `StartLocationPlacement`, `RuleFormulas` (`CanRecruit`, `AcceptTrade`,
//! `DoctorResult`).
//!
//! **The service rules give the same number.** `CalculateRestaurantValue`
//! and `CalculatePubValue` take the eat or drink value under
//! `remaster_nutrition` and the trade value under `dos_trade_value`, but
//! `ItemTypesData` sets all three from the same `ConstValues` entry (or the
//! same `value=` line). Both are `(int)` of the sum — [`crate::trade::value_of`].

use alloc::string::String;
use alloc::vec::Vec;

use crate::gamdat::{Gamdat, ItemOwner};
use crate::items::ItemTypes;
use crate::rules::{PlayerSetup, RecruitSupplies, Recruitment, RuleBook, StartLocations};
use crate::spawn::Rng;

/// `StartLocationPlacement.Apply`, every player in seat order. Locations
/// are counted from nought here (the file counts from one).
///
/// `amiga_regions`: each player draws one of the regions still free, then
/// one location of it. `dos_rotating_groups`: one group drawn for the first
/// player, the next for each next, one location of each drawn.
pub fn start_locations(book: &RuleBook, difficulty: u16, rng: &mut Rng) -> [Option<usize>; 4] {
    let mut out = [None; 4];
    let level = &book.difficulty[(difficulty as usize).min(2)];
    let pick = |list: &[i32], rng: &mut Rng| -> Option<usize> {
        if list.is_empty() {
            return None;
        }
        let l = list[rng.below(list.len() as u32) as usize];
        (l > 0).then(|| (l - 1) as usize)
    };
    match book.rules.start_locations {
        StartLocations::AmigaRegions => {
            let mut free: Vec<usize> = (1..=level.start_regions.max(0) as usize).collect();
            for slot in out.iter_mut() {
                if free.is_empty() {
                    break;
                }
                let group = free.remove(rng.below(free.len() as u32) as usize);
                *slot = level.start_locations.get(group - 1).and_then(|list| pick(list, rng));
            }
        }
        StartLocations::DosRotatingGroups => {
            let Some(section) = book.section("original_start_locations") else { return out };
            let groups = section.int("groups");
            if groups <= 0 {
                return out;
            }
            let mut group = rng.below(groups as u32) as i32;
            for slot in out.iter_mut() {
                let mut key = String::from("group_");
                key.push_str(&alloc::format!("{}", group + 1));
                *slot = pick(&section.ints(&key), rng);
                group = (group + 1) % groups;
            }
        }
    }
    out
}

/// `World.Update`'s pass over the players' faces (`Logic/World.cs:66-83`):
/// a machine player's character is made with face 0, so on the first update
/// every one whose face is already taken — by a human, or by a machine
/// player before it — draws `Next(0, MAX_FACE_ID)` (0 to 4) until it has one
/// nobody has. Seats in order; `humans` are the human seats and their faces.
/// Returns every other seat of the four with its face.
pub fn machine_faces(humans: &[(usize, i32)], rng: &mut Rng) -> Vec<(usize, i32)> {
    let mut used: Vec<i32> = Vec::new();
    let mut out = Vec::new();
    for seat in 0..4 {
        if let Some(&(_, face)) = humans.iter().find(|h| h.0 == seat) {
            used.push(face);
            continue;
        }
        let mut face = 0;
        while used.contains(&face) {
            face = rng.range(0, 5);
        }
        used.push(face);
        out.push((seat, face));
    }
    out
}

/// What a human's character starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub health: i32,
    pub experience: i32,
    pub food: i32,
    pub water: i32,
    /// Item kinds, in order.
    pub items: Vec<usize>,
    /// Names that are not in the item table (`remaster_settings` only).
    pub missing: Vec<String>,
}

/// The item type of a `gam.dat` sprite (`ItemTypes[int]`).
pub fn kind_of_sprite(types: &ItemTypes, sprite: u8) -> Option<usize> {
    (0..types.len()).find(|&k| types.sprite_of(k) == Some(sprite))
}

/// The items `gam.dat` gives a character record, in the table's order.
pub fn items_of_record(gam: &Gamdat, types: &ItemTypes, record: usize) -> Vec<usize> {
    gam.items
        .iter()
        .filter(|i| i.owner() == ItemOwner::Character(record))
        .filter_map(|i| kind_of_sprite(types, i.sprite_id))
        .collect()
}

/// `InitializeHumanPlayer`. `dos_gamdat`: the seat's own record — its
/// health, experience, food and water and the items it holds.
/// `remaster_settings`: 100, `start_experience`, 9 and 5, and the
/// difficulty's `start_items`.
pub fn human_start(book: &RuleBook, difficulty: u16, seat: usize, gam: &Gamdat, types: &ItemTypes) -> Start {
    match book.rules.player_setup {
        PlayerSetup::RemasterSettings => {
            let mut items = Vec::new();
            let mut missing = Vec::new();
            for name in &book.difficulty[(difficulty as usize).min(2)].start_items {
                match types.index_of(name) {
                    Some(k) => items.push(k),
                    None => missing.push(name.clone()),
                }
            }
            Start { health: 100, experience: book.rules.start_experience, food: 9, water: 5, items, missing }
        }
        PlayerSetup::DosGamdat => {
            let c = gam.characters.get(seat);
            Start {
                health: c.map_or(100, |c| c.health as i32),
                experience: c.map_or(0, |c| c.experience as i32),
                food: c.map_or(9, |c| c.food as i32),
                water: c.map_or(5, |c| c.water as i32),
                items: items_of_record(gam, types, seat),
                missing: Vec::new(),
            }
        }
    }
}

/// `RuleFormulas.CanRecruit`.
pub fn can_recruit(rule: Recruitment, boss: i32, recruit: i32) -> bool {
    match rule {
        Recruitment::DosXpTimes15 => 3 * boss / 2 >= recruit,
        Recruitment::AmigaXpPlus4 => boss >= recruit - 4,
    }
}

/// `InitializeRecruit` for a human's hire: the recruit's food and water.
/// `dos_fixed`: nine and five. `remaster_difficulty`: an edible payment
/// fed to him (up to his maximum), food at least `2 - difficulty`, and
/// water drawn from 3..=5, 1..=4 or 0..=2 by difficulty.
pub fn recruit_supplies(
    rule: RecruitSupplies,
    difficulty: u16,
    food: i32,
    max_food: i32,
    payment_food: i32,
    rng: &mut Rng,
) -> (i32, i32) {
    match rule {
        RecruitSupplies::DosFixed => (9, 5),
        RecruitSupplies::RemasterDifficulty => {
            let mut food = food;
            if payment_food != 0 {
                food = max_food.min(food + payment_food);
            }
            let floor = 2 - difficulty as i32;
            if food < floor {
                food = floor;
            }
            let (low, high) = match difficulty {
                0 => (3, 5),
                1 => (1, 4),
                _ => (0, 2),
            };
            // `Next(min, max + 1)`.
            (food, rng.range(low, high + 1))
        }
    }
}

/// `InitializeRecruit` for an AI boss's hire (`GameRules.cs:101-117`):
/// nine and five under `dos_fixed`, five and five under
/// `remaster_difficulty` (`boss.Type == PlayerType.Ai`). Nothing drawn.
pub fn ai_recruit_supplies(rule: RecruitSupplies) -> (i32, i32) {
    match rule {
        RecruitSupplies::DosFixed => (9, 5),
        RecruitSupplies::RemasterDifficulty => (5, 5),
    }
}

/// `OriginalPaymentValue`: the sum of quarter values, times four, rounded
/// away from nought.
fn payment_value(values: &[f32]) -> i32 {
    let sum: f32 = values.iter().sum();
    let x = (sum * 4.0) as f64;
    let r = if x >= 0.0 { (x + 0.5) as i64 } else { -((-x + 0.5) as i64) };
    r as i32
}

/// `RuleFormulas.AcceptTrade`: something offered, and its value times the
/// difficulty's barter factor at least what is asked. In quarters, not
/// truncated — v0.9.1 compared whole sums.
pub fn accept_trade(offered: &[f32], wanted: &[f32], barter_factor: i32) -> bool {
    accept_trade_with(offered, wanted, barter_factor, 1.0)
}

/// `AcceptTrade(offered, wanted, difficultyFactor, profileFactor)`: the
/// Modern opponent's `trade_benefit` multiplies what it offers.
pub fn accept_trade_with(offered: &[f32], wanted: &[f32], barter_factor: i32, profile: f32) -> bool {
    !offered.is_empty() && effective_trade_value(offered, barter_factor, profile) >= payment_value(wanted) as f32 / 4.0
}

/// `EffectiveTradeFactor`.
pub fn effective_trade_factor(barter_factor: i32, profile: f32) -> f32 {
    barter_factor as f32 / 100.0 * profile
}

/// `EffectiveTradeValue`: the offer in quarters, times the factor.
pub fn effective_trade_value(values: &[f32], barter_factor: i32, profile: f32) -> f32 {
    payment_value(values) as f32 / 4.0 * effective_trade_factor(barter_factor, profile)
}

/// `RuleFormulas.DoctorResult`: health plus the medicine times
/// `healing_factor`, capped at `health_cap`, never lower than before.
pub fn doctor_result(health: i32, points: i32, factor: f32, cap: i32) -> i32 {
    health.max(cap.min(health + (points as f32 * factor) as i32))
}
