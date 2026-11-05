//! What a camp makes, and how fast.
//!
//! `production.txt`, one file for both game modes:
//!
//! ```text
//! [item_knife]
//! maxcombination=1
//! amount=1 2          ' by tool count: 0 tools -> 1 a day, 1 tool -> 2
//! amount2=1 3         ' the same with two or more people in the camp
//! produce=item_maggots
//! alternatives=item_axe item_pitchfork
//! ```
//!
//! The section name is the **tool**, and `alternatives` names others that do
//! the same job. Four of them in the classic data: a knife makes maggots, a
//! rat trap makes rats, a snake trap makes snakes, a trap makes meat.
//!
//! **Two numbers come out of a rate, and they are not the same thing.**
//! `FoodPerDay` feeds the people camped there — `Character.Turn` spends it
//! before anyone touches the stores. Only the *surplus* over that becomes
//! items, one every `FoodValue / surplus` days. A camp whose production
//! exactly matches its mouths produces nothing you can pick up, and one that
//! falls short is starving.
//!
//! Ported from v0.9.1's `Production.GetRate`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One production: a tool, what it makes, and how much.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Production {
    /// The item that does the work — `item_knife`, `item_rat_trap`.
    pub tool: String,
    /// Other items that count as the same tool.
    pub alternatives: Vec<String>,
    /// What it makes, as an item name.
    pub produce: String,
    /// How many tools can be combined. More than this add nothing.
    pub max_combination: usize,
    /// Food a day, by tool count. Index 0 is "no tools at all".
    pub per_day: Vec<i32>,
    /// The same with two or more people camped there. Empty in the file
    /// means "the same as one person", and the original copies it over.
    pub per_day_two: Vec<i32>,
    /// v1.2 `allow_inventory`: whether tools carried by the camp's people
    /// count (`GetProductionToolCount`). `GetBool` reads a missing line as
    /// false, and every production v1.2 ships says false or nothing — only
    /// tools installed in the rooms count. v0.9.1 counted the packs too
    /// (`trapsOnNPCs`); its camps do not read this.
    pub allow_inventory: bool,
}

/// What a production yields today.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rate {
    /// Days between items. Zero means none are dropped at all — either the
    /// camp eats everything, or there is nothing to eat.
    pub item_drop_interval: f32,
    /// Food a day, before the camp eats.
    pub food_per_day: i32,
    /// The camp makes less than it needs.
    pub camp_starving: bool,
}

impl Production {
    /// Does this item do this production's work?
    pub fn worked_by(&self, item: &str) -> bool {
        self.tool == item || self.alternatives.iter().any(|a| a == item)
    }

    /// `Production.GetRate(toolCount, npcCount)`.
    ///
    /// The surplus is `food_per_day - npcCount`, and the interval is the
    /// produced item's own food value divided by it — a snake worth 7 with a
    /// surplus of 2 appears every three and a half days. Below zero the camp
    /// starves; exactly zero yields nothing and is not starving.
    pub fn rate(&self, tools: usize, camp: usize, produce_food_value: i32) -> Rate {
        self.rate_with(tools, camp, produce_food_value, 0)
    }

    /// v1.2's `GetRate(toolCount, npcCount, maintenanceBonus)`: the bonus
    /// (a technician's, `technician_food_bonus`) is added to a production
    /// that makes anything at all, and never below nought.
    pub fn rate_with(&self, tools: usize, camp: usize, produce_food_value: i32, bonus: i32) -> Rate {
        let count = tools.min(self.max_combination);
        let table = if camp >= 2 { &self.per_day_two } else { &self.per_day };
        let base = table.get(count).or_else(|| table.last()).copied().unwrap_or(0);
        let food_per_day = if base > 0 { base + bonus.max(0) } else { base };

        let mut rate = Rate { food_per_day, ..Rate::default() };
        let remaining = food_per_day - camp as i32;
        if remaining > 0 {
            rate.item_drop_interval = produce_food_value as f32 / remaining as f32;
        } else if remaining < 0 {
            rate.camp_starving = true;
        }
        rate
    }
}

/// Everything `production.txt` describes.
#[derive(Debug, Default)]
pub struct Productions {
    list: Vec<Production>,
}

impl Productions {
    pub fn parse(text: &str) -> Productions {
        let mut list: Vec<Production> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix('[') {
                let name = rest.trim_end_matches(']').trim();
                if !name.is_empty() {
                    list.push(Production {
                        tool: name.to_string(),
                        alternatives: Vec::new(),
                        produce: String::new(),
                        max_combination: 0,
                        per_day: Vec::new(),
                        per_day_two: Vec::new(),
                        allow_inventory: false,
                    });
                }
                continue;
            }
            let Some(current) = list.last_mut() else { continue };
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
                "maxcombination" => current.max_combination = value.parse().unwrap_or(0),
                "produce" => current.produce = value.to_string(),
                "allow_inventory" => current.allow_inventory = value.eq_ignore_ascii_case("true"),
                "amount" => {
                    current.per_day = value.split_whitespace().filter_map(|n| n.parse().ok()).collect()
                }
                "amount2" => {
                    current.per_day_two =
                        value.split_whitespace().filter_map(|n| n.parse().ok()).collect()
                }
                "alternatives" => {
                    current.alternatives = value.split_whitespace().map(|a| a.to_string()).collect()
                }
                _ => {}
            }
        }

        // "If ProductionPerDay2Person.Length == 0 then it is ProductionPerDay."
        for production in &mut list {
            if production.per_day_two.is_empty() {
                production.per_day_two = production.per_day.clone();
            }
        }

        Productions { list }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Production> {
        self.list.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Production> {
        self.list.iter()
    }

    /// Which production an item is the tool for, if any.
    pub fn worked_by(&self, item: &str) -> Option<usize> {
        self.list.iter().position(|p| p.worked_by(item))
    }
}
