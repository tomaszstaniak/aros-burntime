//! `gamesettings_original.txt`: the numbers a new game is built from.
//!
//! Three sections, one per difficulty, and a comment character that is a
//! single quote:
//!
//! ```text
//! [0] ' difficulty easy
//! random_items_room       = food water bottle ... -nodrop -special
//! random_items_room_rate  = 1 3
//! ```
//!
//! Only the item-generation lines are read here; the rest of the file is
//! respawn timers and combat numbers that nothing uses yet.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Which item types a spawn may produce.
///
/// `GetTypesWithClass` walks every item type and keeps it if any `include`
/// entry matches one of its classes or its name, unless an `exclude` entry
/// matches first. Exclusion wins, and both sides accept a bare item name as
/// well as a class — `-item_rope` and `-rare` are both legal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Filter {
    /// Read one settings line: words, with a leading `-` meaning "not this".
    pub fn parse(line: &str) -> Filter {
        let mut filter = Filter::default();
        for word in line.split_whitespace() {
            match word.strip_prefix('-') {
                Some(rest) if !rest.is_empty() => filter.exclude.push(rest.to_string()),
                _ => filter.include.push(word.to_string()),
            }
        }
        filter
    }

    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}

/// How many items a spawn produces.
///
/// **The original's own arithmetic is `Next() % (max - min) + min`**, which
/// yields `min` to `max - 1` — so `rate = 1 3` gives one item or two, never
/// three, and `rate = 2 5` gives two, three or four. That is very probably a
/// slip: the settings file reads as a range and every other reading of "1 3"
/// in it means one to three.
///
/// [`Rate::count`] produces the **inclusive** range the file appears to
/// describe, and [`Rate::count_as_upstream`] produces what the C# does. The
/// difference is one item in a room and is recorded rather than chosen
/// silently — we cannot match the DOS sequence either way, but which rule we
/// are following should not be a matter of reading the source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rate {
    pub min: u32,
    pub max: u32,
}

impl Rate {
    /// `GetItemGeneration`: the first number, then the second, or the first
    /// again when there is only one.
    pub fn parse(line: &str) -> Rate {
        let mut numbers = line.split_whitespace().filter_map(|n| n.parse::<u32>().ok());
        let min = numbers.next().unwrap_or(0);
        let max = numbers.next().unwrap_or(min);
        Rate { min, max }
    }

    /// A count in `min..=max`.
    pub fn count(&self, rng: &mut crate::spawn::Rng) -> u32 {
        if self.max <= self.min {
            return self.min;
        }
        self.min + rng.below(self.max - self.min + 1)
    }

    /// A count in `min..max`, which is what `RandomCount` computes:
    /// `Next() % (Maximum - Minimum) + Minimum`, and no draw at all when the
    /// two are equal.
    pub fn count_as_upstream(&self, rng: &mut crate::spawn::Rng) -> u32 {
        if self.max <= self.min {
            return self.min;
        }
        self.min + rng.next_mod(self.max - self.min)
    }
}

/// One spawn rule: what may be produced, and how much of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spawn {
    pub filter: Filter,
    pub rate: Rate,
}

/// The generation rules for one difficulty.
///
/// **Not `derive(Default)`.** Two of these fields have a meaningful absence
/// and a meaningless zero — see [`GameSettings::default`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameSettings {
    /// `random_items_start` — put at the player's own location.
    pub start: Spawn,
    /// `random_items_room` — put in the rooms of every location that is not
    /// a city.
    pub room: Spawn,
    /// `random_items_closed_room` — put in rooms that need an item to enter,
    /// which in the classic maps means the rope rooms.
    pub closed_room: Spawn,
    /// `random_items_danger_location` — extra for a location with a danger.
    pub danger: Spawn,
    /// `random_items_ground` — dropped outside.
    pub ground: Spawn,
    /// `start_experience` — what the played character knows.
    ///
    /// **It replaces whatever the save's record says.** `GameCreation` does
    /// `Players[i].Character.Experience = settings.StartExperience` after
    /// the records are read, so the four player slots' own numbers never
    /// reach the game. It matters because hiring weighs it: at difficulty 0
    /// it is 37, which buys anybody up to 56.
    pub start_experience: i32,
    /// `npc_respawn` — days until a dead person comes back.
    ///
    /// Ten at difficulty 0, twenty at the next, thirty at the hardest, and
    /// the same in every profile the classic data ships — **which is not a
    /// reason to hardcode it**. `CharacterRespawn` reads it from the
    /// settings of the game being played, and so does this.
    pub npc_respawn: i32,
    /// `trader_respawn`, `mutant_respawn`, `dog_respawn` — the same
    /// countdown for the other three classes. `CharacterRespawn.Respawn`
    /// picks by class and nothing else, so these are not a separate
    /// mechanism, only a different number.
    pub trader_respawn: i32,
    pub mutant_respawn: i32,
    pub dog_respawn: i32,
    /// `trader_health`, `mutant_health`, `dog_health` — what those classes
    /// carry, and what `Revive` puts back. **A dog does not come back at a
    /// hundred**: `Dog.Revive` overrides the base with `DogHealth`, which is
    /// 31 in every profile the classic data ships.
    pub trader_health: i32,
    pub mutant_health: i32,
    pub dog_health: i32,
    /// `trader_attack`, `mutant_attack`, `dog_attack` — their
    /// `BaseAttackValue`, in place of the 10 a person fights with bare
    /// handed. A dog's 30 is three times a man's.
    pub trader_attack: i32,
    pub mutant_attack: i32,
    pub dog_attack: i32,
    /// `start_items` — what every player is handed at the start.
    /// `SetStartInventory` clears the character's pack and adds these, so it
    /// is the whole of what they carry rather than an addition to it.
    pub start_items: Vec<String>,
    /// `ai_camp_interval` — how long a machine player waits between camps,
    /// as `(minimum, maximum)`. `GameSettings.cs` reads the two numbers into
    /// `AiSettings.MinInterval` and `MaxInterval`, and `ClassicAiState` draws
    /// `Random.Next(min, max)` after each camp it makes. Three to six turns
    /// on the easiest profile, nought to three on the hardest.
    ///
    /// **Only v0.9.1's settings file carries it.** The pair of files current
    /// upstream splits it into — `gamesettings_original.txt` and
    /// `gamesettings_extended.txt` — have no `ai_camp_*` line at all, and
    /// neither does anything else in that tree: the opponent's pace moved out
    /// of the settings when its behaviour was rebuilt. A file without the
    /// line therefore keeps v0.9.1's numbers rather than reading as nought,
    /// which would be an opponent that never expands — see
    /// [`CLASSIC_CAMP_INTERVAL`].
    pub ai_camp_interval: (i32, i32),
    /// `ai_camp_max_advance` — how many camps a machine player may hold
    /// beyond the best human player's count before it stops expanding. One
    /// on the easiest profile; on the hardest the file says 99999, which is
    /// the data's own way of writing "no limit". Absent from current
    /// upstream's files; see [`ai_camp_interval`](Self::ai_camp_interval).
    pub ai_camp_max_advance: i32,
    /// `start_locations` — where a player may begin, as location indices.
    /// `SetStartLocations` draws one per player, **every** player, human or
    /// machine, without repeats. `start_locations_ai` is in the file too and
    /// nothing reads it.
    pub start_locations: Vec<usize>,
}

/// What a settings file with no `ai_camp_*` line means.
///
/// v0.9.1's own easiest profile, because that is the release these rules are
/// pinned to and "no line" is an absence rather than a decision. A zero here
/// would be a decision — an opponent that waits no time and may never expand
/// — made by accident.
pub const CLASSIC_CAMP_INTERVAL: (i32, i32) = (3, 6);
/// The same, for the expansion allowance.
pub const CLASSIC_CAMP_MAX_ADVANCE: i32 = 1;

impl Default for GameSettings {
    /// Everything at zero, **except the opponent's pace**.
    ///
    /// A zero spawn rate produces nothing, a zero respawn is a number the
    /// file always carries, and both read as "nothing said". A zero camp
    /// interval does not: it is an opponent that waits no turns and, with a
    /// zero allowance, may never expand at all. Current upstream's settings
    /// files carry no `ai_camp_*` line, so that absence would otherwise
    /// become a decision nobody made.
    fn default() -> GameSettings {
        GameSettings {
            start: Spawn::default(),
            room: Spawn::default(),
            closed_room: Spawn::default(),
            danger: Spawn::default(),
            ground: Spawn::default(),
            start_experience: 0,
            npc_respawn: 0,
            trader_respawn: 0,
            mutant_respawn: 0,
            dog_respawn: 0,
            trader_health: 0,
            mutant_health: 0,
            dog_health: 0,
            trader_attack: 0,
            mutant_attack: 0,
            dog_attack: 0,
            start_items: Vec::new(),
            ai_camp_interval: CLASSIC_CAMP_INTERVAL,
            ai_camp_max_advance: CLASSIC_CAMP_MAX_ADVANCE,
            start_locations: Vec::new(),
        }
    }
}

impl GameSettings {
    /// How long this class waits before coming back.
    ///
    /// `CharacterRespawn.Respawn` chooses by class and by nothing else:
    /// a trader's twenty, a dog's or a mutant's thirty, and `npc_respawn`
    /// for the four professions.
    pub fn respawn_days(&self, class: crate::gamdat::CharClass) -> i32 {
        use crate::gamdat::CharClass::*;
        match class {
            Trader => self.trader_respawn,
            Dog => self.dog_respawn,
            Mutant => self.mutant_respawn,
            _ => self.npc_respawn,
        }
    }

    /// What this class is at full health — which `Revive` puts back, and
    /// which `Dog.Turn` and `Mutant.Turn` restore every morning.
    ///
    /// **A hundred for a person.** `Character.Revive` sets `health = 100`
    /// flatly, and the professions have no override.
    pub fn full_health(&self, class: crate::gamdat::CharClass) -> i32 {
        use crate::gamdat::CharClass::*;
        match class {
            Trader => self.trader_health,
            Dog => self.dog_health,
            Mutant => self.mutant_health,
            _ => crate::game::MAX_HEALTH,
        }
    }

    /// `BaseAttackValue`: what this class hits for with nothing in hand.
    ///
    /// A dog's thirty is three times a bare-handed man's ten, which is why
    /// walking up to one and punching it is a poor plan.
    pub fn base_attack(&self, class: crate::gamdat::CharClass) -> i32 {
        use crate::gamdat::CharClass::*;
        match class {
            Trader => self.trader_attack,
            Dog => self.dog_attack,
            Mutant => self.mutant_attack,
            _ => crate::combat::DEFAULT_ATTACK,
        }
    }

    /// Read the section for one difficulty. `gam.dat` records which.
    ///
    /// A missing section gives empty rules rather than an error: a spawn with
    /// no candidates produces nothing, which is the same shape of answer.
    pub fn parse(text: &str, difficulty: u16) -> GameSettings {
        let mut wanted = String::from("[");
        push_num(&mut wanted, difficulty as u32);
        wanted.push(']');

        let mut settings = GameSettings::default();
        let mut inside = false;

        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                // The section marker is followed by a comment on the same
                // line: `[0] ' difficulty easy`.
                let name = line.split_whitespace().next().unwrap_or("");
                inside = name == wanted;
                continue;
            }
            if !inside || line.starts_with('\'') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), strip_comment(value));

            if let Some(stat) = class_stat(&mut settings, key) {
                *stat = value.parse().unwrap_or(0);
                continue;
            }
            if key == "start_experience" {
                settings.start_experience = value.parse().unwrap_or(0);
                continue;
            }
            if key == "start_items" {
                settings.start_items =
                    value.split_whitespace().map(|i| i.to_string()).collect();
                continue;
            }
            // The machine player's pace. Two numbers on one line, the way
            // `GetInts("ai_camp_interval")[0]` and `[1]` read them.
            if key == "ai_camp_interval" {
                let mut numbers = value.split_whitespace().filter_map(|n| n.parse().ok());
                let min = numbers.next().unwrap_or(0);
                settings.ai_camp_interval = (min, numbers.next().unwrap_or(min));
                continue;
            }
            if key == "start_locations" {
                settings.start_locations =
                    value.split_whitespace().filter_map(|n| n.parse().ok()).collect();
                continue;
            }
            if key == "ai_camp_max_advance" {
                settings.ai_camp_max_advance = value.parse().unwrap_or(0);
                continue;
            }

            let (base, is_rate) = match key.strip_suffix("_rate") {
                Some(base) => (base, true),
                None => (key, false),
            };
            let Some(spawn) = target(&mut settings, base) else { continue };
            if is_rate {
                spawn.rate = Rate::parse(value);
            } else {
                spawn.filter = Filter::parse(value);
            }
        }

        settings
    }
}

/// Which class number a settings key names.
///
/// **A zero here is not a default.** `CharacterRespawn.AfterDeserialization`
/// substitutes its own when a value is missing, and its substitutes do not
/// all match the shipped file — it puts back 40 for `DogAttack` where the
/// data says 30. Reading the file is therefore the only way to be right, and
/// a key the file does not carry stays zero rather than borrowing that
/// fallback.
fn class_stat<'a>(settings: &'a mut GameSettings, key: &str) -> Option<&'a mut i32> {
    Some(match key {
        "npc_respawn" => &mut settings.npc_respawn,
        "trader_respawn" => &mut settings.trader_respawn,
        "mutant_respawn" => &mut settings.mutant_respawn,
        "dog_respawn" => &mut settings.dog_respawn,
        "trader_health" => &mut settings.trader_health,
        "mutant_health" => &mut settings.mutant_health,
        "dog_health" => &mut settings.dog_health,
        "trader_attack" => &mut settings.trader_attack,
        "mutant_attack" => &mut settings.mutant_attack,
        "dog_attack" => &mut settings.dog_attack,
        _ => return None,
    })
}

/// Which rule a settings key names.
fn target<'a>(settings: &'a mut GameSettings, key: &str) -> Option<&'a mut Spawn> {
    Some(match key {
        "random_items_start" => &mut settings.start,
        "random_items_room" => &mut settings.room,
        "random_items_closed_room" => &mut settings.closed_room,
        "random_items_danger_location" => &mut settings.danger,
        "random_items_ground" => &mut settings.ground,
        _ => return None,
    })
}

/// A trailing `' comment` is not part of a value.
fn strip_comment(value: &str) -> &str {
    match value.find('\'') {
        Some(at) => value[..at].trim(),
        None => value.trim(),
    }
}

fn push_num(s: &mut String, mut v: u32) {
    if v == 0 {
        s.push('0');
        return;
    }
    let mut digits = [0u8; 10];
    let mut n = 0;
    while v > 0 {
        digits[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        s.push(digits[n] as char);
    }
}
