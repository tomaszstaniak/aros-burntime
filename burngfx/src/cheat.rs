//! v1.2's `/petko`: the cheat dialog the world map opens when the word is
//! typed on it (`MapScene.OnKeyPress`, `ShowCheatDialog`, `OnDialogHidden`).
//! It is not behind `[debug] enable_cheats`: any v1.2 game reaches it.
//!
//! Three answers — Travel, Refill, Items — and any of them marks the game
//! (`ClassicGame.CheatsEnabled`), which the statistics then say
//! (`newburn?52`, "- Cheats active -"). The mark is a plain auto-property of
//! a `[Serializable]` state object, so `BinaryFormatter` writes its backing
//! field into the save and a load brings it back; nothing ever clears it.
//!
//! Sources: `Scenes/MapScene.cs` (`TryGetInputAction`, `OnKeyPress`,
//! `ShowCheatDialog`, `OnDialogHidden`, `RefillPlayer`, `SpawnCheatItems`,
//! `OnFastTravel`, `TravelToLocation`), `States/ClassicGame.cs`
//! (`CheatsEnabled`), `Scenes/StatisticsScene.cs` (`{cheats}`).

use alloc::string::String;
use alloc::vec::Vec;

use crate::game::CharacterNeeds;

/// The word, as `OnKeyPress` compares it.
pub const WORD: &str = "/petko";

/// `newburn?52`: the statistics' line for a game in which the dialog was
/// answered.
pub const NEWBURN_ACTIVE: usize = 52;

/// The dialog's text and its three answers (`ShowCheatDialog`), literal
/// English in v1.2 as well: none of them is a text reference.
pub const TITLE: &str = "Cheats";
pub const CHOICES: [&str; 3] = ["Travel", "Refill", "Items"];

/// What `MapScene._cheatCommand` holds: the word typed so far, or nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Command {
    typed: String,
}

/// What a typed character did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typed {
    /// Not the command's: the key goes on as it would have (`base.OnKeyPress`).
    Passed,
    /// Taken by the command, which goes on or starts again.
    Taken,
    /// The word is complete: show the dialog.
    Open,
}

impl Command {
    /// `_cheatCommand.Length > 0`: while something is typed, **every key is
    /// the command's** — `TryGetInputAction` answers no action for any of
    /// them, so a bound letter (`e`, `o`, `t`) does not act.
    pub fn is_typing(&self) -> bool {
        !self.typed.is_empty()
    }

    /// What has been typed, for a log.
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// Escape while typing (`TryGetInputAction`): the command is dropped,
    /// and the key does nothing else.
    pub fn escape(&mut self) {
        self.typed.clear();
    }

    /// `OnKeyPress(char)`, one typed character. Lower-cased first; `/`
    /// always starts again; backspace takes one back; anything else is added
    /// and the command kept only while it still begins the word.
    pub fn key(&mut self, c: char) -> Typed {
        // `char.ToLowerInvariant`: one character for one.
        let c = {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => c,
            }
        };
        if c == '/' {
            self.typed.clear();
            self.typed.push('/');
            return Typed::Taken;
        }
        if self.typed.is_empty() {
            return Typed::Passed;
        }
        if c == '\u{8}' {
            self.typed.pop();
            return Typed::Taken;
        }
        self.typed.push(c);
        if self.typed == WORD {
            self.typed.clear();
            return Typed::Open;
        }
        if !WORD.starts_with(self.typed.as_str()) {
            self.typed.clear();
        }
        Typed::Taken
    }
}

/// An answer of the dialog, by its line (`_dialog.ResultChoice`). Leaving
/// it by Back answers nothing (`ResultChoice < 0`) and marks nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// `OnFastTravel`: the map's next click on a place goes there at once.
    Travel,
    /// `RefillPlayer`.
    Refill,
    /// `SpawnCheatItems`.
    Items,
}

impl Choice {
    pub fn of_line(line: usize) -> Option<Choice> {
        match line {
            0 => Some(Choice::Travel),
            1 => Some(Choice::Refill),
            2 => Some(Choice::Items),
            _ => None,
        }
    }
}

/// `RefillPlayer`: **the leader only** (`view.Player.Character`) — food 9,
/// water 5, health 100. His group is not touched.
pub fn refill(leader: &mut CharacterNeeds) {
    leader.food = 9;
    leader.water = 5;
    leader.health = 100;
}

/// `SpawnCheatItems`'s list: `Settings["debug"].GetStrings` of
/// `insert_items_1`, `_2` and `_3`, in that order, from `settings.txt`.
/// Each is dropped where the leader stands in his location.
///
/// `ConfigFile`'s reading: a section's and a key's names in any case; a
/// comment from `'`, `;` or `//`; quotes around the value taken off; the
/// value split at spaces and commas. A missing key is no items.
pub fn items(settings: &str) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["insert_items_1", "insert_items_2", "insert_items_3"] {
        if let Some(v) = setting(settings, "debug", key) {
            out.extend(v.split([' ', ',', '\n']).filter(|w| !w.is_empty()).map(String::from));
        }
    }
    out
}

/// One `ConfigFile` value: `[section]`, `key = value`.
fn setting<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut current = "";
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = match [line.find('\''), line.find(';'), line.find("//")].into_iter().flatten().min() {
            Some(end) => &line[..end],
            None => line,
        };
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name;
            continue;
        }
        if !current.eq_ignore_ascii_case(section) {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                let v = v.trim();
                let v = v.strip_prefix('"').unwrap_or(v);
                return Some(v.strip_suffix('"').unwrap_or(v));
            }
        }
    }
    None
}

/// `OnFastTravel`'s cursor, `syst.raw?4-7`: not progressive, at the default
/// five frames a second, as the travel and info cursors are.
pub fn fast_travel_cursor_frame(ms: u32) -> u8 {
    4 + ((ms * 5 / 1000) % 4) as u8
}

/// The statistics' `{cheats}` (`StatisticsScene.OnActivateScene`): the
/// "active" line once the game is marked, nothing before.
pub fn statistics_line(raw: &str, marked: bool, active: &str) -> String {
    raw.replace("{cheats}", if marked { active } else { "" })
}
