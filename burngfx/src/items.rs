//! What an item *is*: the type table out of `items_original.txt`.
//!
//! A type is a kind of thing — maggots, a bottle, a snake. It is not a thing:
//! two snakes are one type and two instances, and each instance takes a slot
//! of its own. The instances live in [`crate::game::ItemBag`]; this module
//! only says what kinds there are.
//!
//! ```text
//! [item_snake]
//! class=food
//! burngfx=2       the frame in gst.raw that draws it
//! food=7
//! ```
//!
//! `ItemTypesData` builds three things from that `burngfx` number, and they
//! are all offsets into the same string table:
//!
//! | | |
//! |---|---|
//! | sprite | `gst.raw?<burngfx>` |
//! | name   | `burn?<50 + burngfx>` |
//! | description | `burn?<110 + burngfx>` |
//!
//! An item with no `burngfx` line is one the remaster added; it has a PNG of
//! its own and no frame in the 1993 archive, so [`ItemType::sprite`] is
//! `None` and it cannot be drawn from the classic data at all.
//!
//! **`heal` is not what eating does.** It is what a doctor charges — the value
//! is read in `DoctorScene` and nowhere else. A snake is `food=7 heal=28`, and
//! eating one restores no health whatever. The two numbers are two uses
//! competing for the same object, which is rather the point of it.
//!
//! **`class` is a generation filter, not a rule about the item.** The settings
//! lines read `random_items_room = food water bottle ... -nodrop -special`,
//! and `-nodrop` means "never spawn one of these at random". It says nothing
//! about what the player may do with one: `nodrop` items are the ones you can
//! only get by making them — cooked meat, a filled canteen — and once you have
//! one you may put it down like anything else. Nothing in the original refuses
//! that; the only condition on moving an item is whether the destination has
//! room. See [`crate::game::move_item`].

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::gamdat::Danger;
use crate::text::TextRef;

/// The offsets `ItemTypesData` adds to an item's `burngfx` number.
const NAME_AT: u32 = 50;
const DESCRIPTION_AT: u32 = 110;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemType {
    /// The section name, `item_snake`. Unique, and how the rules refer to one.
    pub name: String,
    /// `food`, `water`, `bottle`, `nodrop`... — which random spawns may
    /// produce it. Not a restriction on the item; see the module docs.
    ///
    /// **A list, not a word.** `class=special rare` is two classes, and the
    /// spawn filters test them one at a time: `-rare` excludes that item
    /// while `special` would have included it.
    pub class: Vec<String>,
    /// The frame in `gst.raw`, when this item is one the 1993 game had.
    pub sprite: Option<u8>,
    /// `food` — how many points of hunger eating it fills. Zero means it is
    /// not food, and `InventoryScene` tests exactly that before offering to
    /// eat one.
    pub food: i32,
    /// `heal` — **not** what eating it does.
    ///
    /// It is what a doctor will heal in exchange for it: `DoctorScene` is the
    /// only place the value is read. A snake is `food=7 heal=28`, and eating
    /// one restores no health at all. Kept here so the number is not mistaken
    /// for a second effect of eating.
    pub heal: i32,
    /// `water` — how many points of thirst drinking it fills. A full bottle
    /// is 2.
    pub water: i32,
    /// `full` — what this becomes when it is filled at a spring. Only an
    /// empty container has one.
    pub full: Option<String>,
    /// `empty` — what this becomes when it is drunk. A full bottle names the
    /// empty one; a snake names nothing and is simply gone.
    pub empty: Option<String>,
    /// `damage` — what it hits for, in place of a bare hand's ten. A knife
    /// is 25, an axe 33, a pitchfork 38, a loaded rifle 55.
    pub damage: i32,
    /// `defense` — what it takes off an incoming blow. **No item in the
    /// classic table has one**, so protection is zero in Original; the
    /// field exists because the rule reads it and Extended may not agree.
    pub defense: i32,
    /// `ammo` — how many shots are left. A loaded rifle has two.
    pub ammo: i32,
    /// `protection` — what it keeps off, and how much of it.
    ///
    /// The line reads `protection=protection@gas??100
    /// protection@radiation??100`: a resource reference per danger, whose
    /// number is a percentage. `DangerProtectionProcessor` turns it into a
    /// rate with `value * 0.01f`, so 100 is "all of it" and the paper helmet's
    /// 20 is a fifth. Only three items in the tables carry the line, and one
    /// of them — the paper helmet — is Extended's alone.
    pub protection: Vec<(Danger, u32)>,
    /// `fluff`: a text reference (`@newburn?33`) the construction dialog
    /// shows instead of the default line when nothing can be made from this.
    /// Only the rope has one.
    pub fluff: Option<String>,
    /// `value` — what it is worth, **when the table says so itself**.
    ///
    /// No classic item has the line: their worth is `ConstValues` by sprite,
    /// see [`crate::trade::Prices`]. The items the remaster added to Extended
    /// have no sprite and carry it here instead — a dish is `value=24` — and
    /// `ItemTypesData` lets the line override the sprite's number for any
    /// item that has both. See [`crate::trade::worth_of`].
    pub value: Option<i32>,
    /// v1.2 `damage`: **four rolls for each of four experience tiers**, lowest
    /// tier first, or one value. Empty for the v0.9.1 tables, whose `damage`
    /// is the one number in [`ItemType::damage`] — and for a v1.2 item that
    /// is not a weapon. For v1.2, [`ItemType::damage`] holds `DamageValue`:
    /// the table's average, truncated (`(int)DamageValues.Average()`).
    pub damage_table: Vec<i32>,
    /// v1.2 `weapon_priority`: the order a weapon is picked in. Absent means
    /// its `DamageValue` (`ItemType.WeaponPriority`).
    pub weapon_priority: Option<i32>,
    /// v1.2 `attack_range`: how far this weapon reaches, in map pixels.
    pub attack_range: i32,
    /// v1.2 `loads`: what this can be loaded into (ammunition).
    pub loads: Vec<String>,
    /// v1.2 `load_name`.
    pub load_name: Option<String>,
    /// v1.2 `function`: named uses (`resting_sustenance`, …), as written.
    pub functions: Vec<String>,
    /// v1.2 `trader_world_group` / `trader_world_limit`: Extended's cap on
    /// how many of a group all traders together may stock.
    pub trader_world_group: Option<String>,
    pub trader_world_limit: i32,
}

/// Why a v1.2 item table was refused.
///
/// `ItemTypesData` throws on a damage vector that is not empty, one value or
/// sixteen, or that has a negative entry: the game does not start. This
/// port refuses the table the same way — **never a weapon silently worth
/// nothing**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemError {
    /// `Invalid damage vector for {item}: expected one value or four tiers of
    /// four rolls.` The section name is empty for the unarmed line.
    BadDamage { item: String, values: String },
}

/// Read `protection=protection@gas??100 protection@radiation??100`.
///
/// `DangerProtectionProcessor` takes the **file** of the reference as the
/// danger's name and the value as a percentage: `new DangerProtection(
/// id.File.ToLower(), value * 0.01f)`. Anything else in the line is a danger
/// this port has no name for, and is dropped rather than guessed at.
fn parse_protection(value: &str) -> Vec<(Danger, u32)> {
    let mut out = Vec::new();
    for word in value.split_whitespace() {
        let Some(rest) = word.strip_prefix("protection@") else { continue };
        let Some((kind, percent)) = rest.split_once("??") else { continue };
        let danger = match kind.to_ascii_lowercase().as_str() {
            "gas" => Danger::Gas,
            "radiation" => Danger::Radiation,
            _ => continue,
        };
        let Ok(percent) = percent.trim().parse::<u32>() else { continue };
        out.push((danger, percent));
    }
    out
}

impl ItemType {
    /// How much of that danger this item keeps off, in percent.
    pub fn protection_against(&self, danger: Danger) -> u32 {
        self.protection.iter().find(|(d, _)| *d == danger).map(|(_, p)| *p).unwrap_or(0)
    }

    /// Does this item carry that class? `ItemType.IsClass`, case-insensitive
    /// as the original is.
    pub fn is_class(&self, name: &str) -> bool {
        self.class.iter().any(|c| c.eq_ignore_ascii_case(name))
    }

    /// The line of `BURN.TXT` that names it.
    pub fn name_ref(&self) -> Option<TextRef<'static>> {
        Some(TextRef { database: "burn", index: (NAME_AT + self.sprite? as u32) as usize })
    }

    /// The line that describes it.
    pub fn description_ref(&self) -> Option<TextRef<'static>> {
        Some(TextRef { database: "burn", index: (DESCRIPTION_AT + self.sprite? as u32) as usize })
    }
}

/// Every item the rules know about, in file order.
#[derive(Debug, Default)]
pub struct ItemTypes {
    types: Vec<ItemType>,
    /// v1.2: the nameless first section's `damage`, what a bare hand does
    /// (`ItemTypes.UnarmedDamage`). Empty for the v0.9.1 tables.
    unarmed: Vec<i32>,
}

impl ItemTypes {
    /// **A placeholder type for each name the table lacks**, appended after
    /// the real ones so no real item's number moves.
    ///
    /// `ItemTypes[string]` in the source makes one on demand — the file's
    /// unnamed first section under the asked-for id: no picture, no class, no
    /// values — and the computer player asks for three names by string that
    /// are not always there (`crate::classic_ai::MAY_BE_PLACEHOLDERS`). Made
    /// up front here, in a fixed order, so a save made with them loads to the
    /// same numbers.
    pub fn with_placeholders(&mut self, names: &[&str]) {
        for name in names {
            if self.index_of(name).is_none() {
                self.types.push(ItemType { name: name.to_string(), ..ItemType::default() });
            }
        }
    }

    /// Read `items_original.txt` or `items.txt`.
    ///
    /// Sections that are not `[item_*]` are skipped rather than refused: the
    /// file opens with a bare `image=` line that belongs to no section.
    pub fn parse(text: &str) -> ItemTypes {
        let mut types: Vec<ItemType> = Vec::new();
        // Whether the section being read is an item. Without this, a section
        // that is not one hands its lines to whichever item came before it —
        // and a stray `burngfx` would silently redraw that item.
        let mut in_item = false;

        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix('[') {
                let name = rest.trim_end_matches(']').trim();
                in_item = name.starts_with("item_");
                if in_item {
                    types.push(ItemType { name: name.to_string(), ..ItemType::default() });
                }
                continue;
            }

            if !in_item {
                continue;
            }
            let Some(current) = types.last_mut() else { continue };
            let Some((key, value)) = line.split_once('=') else { continue };
            match key.trim() {
                "class" => {
                    current.class =
                        value.split_whitespace().map(|c| c.to_string()).collect()
                }
                // Anything that is not a number leaves the item without a
                // sprite, which is the same as not having the line at all.
                "burngfx" => current.sprite = value.trim().parse().ok(),
                "food" => current.food = value.trim().parse().unwrap_or(0),
                "heal" => current.heal = value.trim().parse().unwrap_or(0),
                "water" => current.water = value.trim().parse().unwrap_or(0),
                "full" => current.full = Some(value.trim().to_string()),
                "empty" => current.empty = Some(value.trim().to_string()),
                "damage" => current.damage = value.trim().parse().unwrap_or(0),
                "defense" => current.defense = value.trim().parse().unwrap_or(0),
                "ammo" => current.ammo = value.trim().parse().unwrap_or(0),
                "protection" => current.protection = parse_protection(value),
                "fluff" if !value.trim().is_empty() => current.fluff = Some(value.trim().to_string()),
                // `section.GetInt("value")`: a line that is not a number
                // reads as zero there, which is a worth, not an absence.
                "value" => current.value = Some(value.trim().parse().unwrap_or(0)),
                _ => {}
            }
        }

        ItemTypes { types, unarmed: Vec::new() }
    }

    /// Read a v1.2 table (`rules/<set>/items.txt`).
    ///
    /// The same lines as [`ItemTypes::parse`], and the v1.2 ones besides:
    /// `damage` as a table (validated as `ItemTypesData` does), the unarmed
    /// line before the first section, `weapon_priority`, `attack_range`,
    /// `loads`, `load_name`, `function`, `trader_world_group` and
    /// `trader_world_limit`.
    pub fn parse_v12(text: &str) -> Result<ItemTypes, ItemError> {
        let mut out = ItemTypes::parse(text);
        let sections = crate::rules::sections(text);
        let damage = |section: &crate::rules::Section| -> Result<Vec<i32>, ItemError> {
            let words: Vec<&str> = section.string("damage").split_whitespace().collect();
            let values: Vec<i32> = words.iter().filter_map(|w| w.parse().ok()).collect();
            if values.len() != words.len() || !matches!(values.len(), 0 | 1 | 16) || values.iter().any(|&v| v < 0) {
                return Err(ItemError::BadDamage {
                    item: section.name.clone(),
                    values: section.string("damage").to_string(),
                });
            }
            Ok(values)
        };
        if let Some(root) = sections.first().filter(|s| s.name.is_empty()) {
            out.unarmed = damage(root)?;
        }
        for section in sections.iter().filter(|s| s.name.starts_with("item_")) {
            let table = damage(section)?;
            let Some(t) = out.types.iter_mut().find(|t| t.name == section.name) else { continue };
            // `DamageValue = (int)DamageValues.Average()`.
            t.damage = if table.is_empty() { 0 } else { table.iter().sum::<i32>() / table.len() as i32 };
            t.damage_table = table;
            t.weapon_priority = section.has("weapon_priority").then(|| section.int("weapon_priority"));
            t.attack_range = section.int("attack_range");
            t.loads = section.words("loads");
            t.load_name = Some(section.string("load_name")).filter(|v| !v.is_empty()).map(|v| v.to_string());
            t.functions = section.words("function");
            t.trader_world_group =
                Some(section.string("trader_world_group")).filter(|v| !v.is_empty()).map(|v| v.to_string());
            t.trader_world_limit = section.int("trader_world_limit").max(0);
        }
        Ok(out)
    }

    /// v1.2 `ItemTypes.UnarmedDamage`.
    pub fn unarmed(&self) -> &[i32] {
        &self.unarmed
    }

    /// v1.2 `WeaponPriority`: the line, or the item's `DamageValue`.
    pub fn weapon_priority_of(&self, index: usize) -> i32 {
        self.types.get(index).map_or(0, |t| t.weapon_priority.unwrap_or(t.damage))
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&ItemType> {
        self.types.get(index)
    }

    /// The index of a type by its section name, which is how a scenario names
    /// the item it wants without depending on file order.
    /// `heal` — what a doctor will mend in exchange for one. **Not what
    /// eating it does**; see [`ItemType::heal`].
    pub fn heal_of(&self, index: usize) -> i32 {
        self.types.get(index).map_or(0, |t| t.heal)
    }

    /// `damage` — what it hits for. Zero means it is not a weapon.
    pub fn damage_of(&self, index: usize) -> i32 {
        self.types.get(index).map_or(0, |t| t.damage)
    }

    /// `defense` — what it takes off an incoming blow.
    pub fn defense_of(&self, index: usize) -> i32 {
        self.types.get(index).map_or(0, |t| t.defense)
    }

    /// `ammo` — shots left in it.
    pub fn ammo_of(&self, index: usize) -> i32 {
        self.types.get(index).map_or(0, |t| t.ammo)
    }

    /// The `burngfx` frame of one type, which is also how the classic data
    /// indexes its prices — see [`crate::trade::Prices`].
    pub fn sprite_of(&self, index: usize) -> Option<u8> {
        self.types.get(index).and_then(|t| t.sprite)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.types.iter().position(|t| t.name == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ItemType> {
        self.types.iter()
    }

    /// What `kind` becomes when it is filled at a spring, as an index.
    pub fn full_of(&self, kind: usize) -> Option<usize> {
        self.index_of(self.get(kind)?.full.as_deref()?)
    }

    /// What `kind` becomes when it is drunk or eaten, as an index. `None`
    /// means the item is used up and gone rather than transformed.
    pub fn empty_of(&self, kind: usize) -> Option<usize> {
        self.index_of(self.get(kind)?.empty.as_deref()?)
    }

    pub fn food_of(&self, kind: usize) -> i32 {
        self.get(kind).map_or(0, |t| t.food)
    }

    /// What that kind is called, for the one rule that names items: a
    /// production says which item is its tool.
    pub fn name_of(&self, kind: usize) -> Option<&str> {
        Some(self.get(kind)?.name.as_str())
    }

    pub fn water_of(&self, kind: usize) -> i32 {
        self.get(kind).map_or(0, |t| t.water)
    }
}

impl crate::game::ItemFacts for ItemTypes {
    fn food(&self, kind: usize) -> i32 {
        self.food_of(kind)
    }

    fn water(&self, kind: usize) -> i32 {
        self.water_of(kind)
    }

    fn full(&self, kind: usize) -> Option<usize> {
        self.full_of(kind)
    }

    fn empty(&self, kind: usize) -> Option<usize> {
        self.empty_of(kind)
    }

    fn kind_named(&self, name: &str) -> Option<usize> {
        self.index_of(name)
    }

    fn name_of(&self, kind: usize) -> Option<&str> {
        ItemTypes::name_of(self, kind)
    }
    fn has_function(&self, kind: usize, function: &str) -> bool {
        self.get(kind).is_some_and(|t| t.functions.iter().any(|f| f == function))
    }

    fn protection(&self, kind: usize, danger: Danger) -> u32 {
        self.get(kind).map_or(0, |t| t.protection_against(danger))
    }
}

impl crate::combat::Weapons for ItemTypes {
    fn damage(&self, kind: usize) -> i32 {
        self.damage_of(kind)
    }

    fn defense(&self, kind: usize) -> i32 {
        self.defense_of(kind)
    }

    fn ammo(&self, kind: usize) -> i32 {
        self.ammo_of(kind)
    }

    fn empty(&self, kind: usize) -> Option<usize> {
        self.empty_of(kind)
    }
}
