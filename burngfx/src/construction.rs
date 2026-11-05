//! Making one item out of several.
//!
//! `construction.txt`, one file for both modes and both releases:
//!
//! ```text
//! dialog=503
//!
//! [item_hand_pump]
//! items=item_broken_pump item_rags item_hose
//! main_items=item_broken_pump
//! dialog=508
//! class=technician
//! ```
//!
//! The section name is what comes **out**. `main_items` are the items that
//! offer the recipe when they are right-clicked, `items` is everything it
//! costs — the main item included — and `class` says who can do it.
//!
//! Ported from v0.9.1's `Logic/Data/Constructions.cs`, whose classic path is
//! byte-identical at `v1.1`. Two lines of the **data** differ, and both are
//! reproduced here as the pack's own file supplies them (a v0.9.1 game reads
//! v0.9.1's file, a v1.2 game v1.2's):
//!
//! * the two-way radio's main item is written `dialog_items=` at v0.9.1, a
//!   key nothing reads, so the recipe is indexed under no main item and
//!   cannot be reached. `v1.1` writes `main_items=` and the radio can be
//!   built. Nothing here special-cases it: read v0.9.1's file and the radio
//!   is unbuildable, exactly as the release is.
//! * the protective suit's `items` are reordered at `v1.1`, which changes
//!   only which three of the four the dialog line names.
//!
//! v1.2 adds two keys (`Logic/Data/Constructions.cs`):
//!
//! * `tools=`, items that must be to hand like the parts but are **not
//!   consumed** (`CanConstruct` checks them, `Construct` takes only `Items`).
//!   No recipe in v1.2's file has any.
//! * `requires_function=`, which offers the recipe only when the result's
//!   type in the preset's item table has that function (`IsAvailable`). The
//!   mine detector's `metal_detection` exists only in Extended's table, so it
//!   is built there and nowhere else. A v0.9.1 file has neither key, and
//!   every recipe stays available.
//!
//! **Where the parts may come from.** `CanConstruct` takes two collections,
//! and the second is not another person's pack: every call site resolves
//! `classic.InventoryRoom == null ? classic.PickItems : classic.InventoryRoom.Items`
//! (`InventoryScene.cs:71,293,337`) — the pile of the room you are standing
//! in, or, on the location map, the loose items within `PICKUP_DISTANCE` of
//! the selected character. Opening the inventory from the world map leaves it
//! null, and then only what the builder carries counts.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    /// What is made — the section name.
    pub result: String,
    /// Everything it costs, main item included, in the file's order. The
    /// order is not a rule; it decides which of them the dialog line names.
    pub items: Vec<String>,
    /// Right-clicking one of these offers the recipe.
    pub main_items: Vec<String>,
    /// Which `MEN_5NN.TXT` conversation is shown.
    pub dialog: u32,
    /// Who may build it, by character class.
    pub classes: Vec<String>,
    /// v1.2 `tools`: needed to hand, never consumed.
    pub tools: Vec<String>,
    /// v1.2 `requires_function`, as `ParseFunction` reads it: `None` for no
    /// key and for a name that is not one of `ItemFunction`'s.
    pub requires_function: Option<String>,
    /// `IsAvailable` against the game's item table, set by
    /// [`Constructions::for_items`]; true until then.
    pub available: bool,
}

impl Recipe {
    /// Is every item present, in the builder's pack or in the pile beside
    /// them? `CanConstruct`, which **tests presence and does not count** —
    /// `Contains` matches by type, so a recipe asking twice for one thing
    /// would be content with one. No recipe does.
    /// v1.2 asks the same of the `tools`.
    pub fn can_build(&self, carried: &[String], nearby: &[String]) -> bool {
        self.items
            .iter()
            .chain(self.tools.iter())
            .all(|need| carried.iter().any(|i| i == need) || nearby.iter().any(|i| i == need))
    }

    /// Whether `item` goes into this recipe as a part or a tool — what the
    /// item tooltip counts (`ItemGridTooltip`: `Items.Contains || Tools.Contains`).
    pub fn uses(&self, item: &str) -> bool {
        self.items.iter().chain(self.tools.iter()).any(|i| i == item)
    }

    /// `construction.Classes[(int)charClass]`.
    pub fn allows(&self, class: &str) -> bool {
        self.classes.iter().any(|c| c.eq_ignore_ascii_case(class))
    }
}

/// What right-clicking an item offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// Which recipe, as an index into [`Constructions`].
    pub recipe: usize,
    /// Whether the Yes line is there at all. `FindConstruction` keeps the
    /// first recipe whose main item is present even when the parts are not,
    /// so that the answer is the right sentence rather than the default one.
    pub can_build: bool,
    /// The conversation to show.
    pub dialog: u32,
}

/// Everything `construction.txt` describes.
#[derive(Debug, Default)]
pub struct Constructions {
    list: Vec<Recipe>,
    /// The header `dialog=`, shown when nothing matches and the item has no
    /// fluff text of its own.
    pub default_dialog: u32,
}

impl Constructions {
    pub fn parse(text: &str) -> Constructions {
        let mut list: Vec<Recipe> = Vec::new();
        let mut default_dialog = 0;

        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix('[') {
                let name = rest.trim_end_matches(']').trim();
                if !name.is_empty() {
                    list.push(Recipe {
                        result: name.to_string(),
                        items: Vec::new(),
                        main_items: Vec::new(),
                        dialog: 0,
                        classes: Vec::new(),
                        tools: Vec::new(),
                        requires_function: None,
                        available: true,
                    });
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            let words = || value.split_whitespace().map(|w| w.to_string()).collect::<Vec<_>>();
            match (key.trim(), list.last_mut()) {
                // Before the first section, `dialog=` is the fallback one.
                ("dialog", None) => default_dialog = value.parse().unwrap_or(0),
                ("dialog", Some(current)) => current.dialog = value.parse().unwrap_or(0),
                ("items", Some(current)) => current.items = words(),
                ("main_items", Some(current)) => current.main_items = words(),
                ("class", Some(current)) => current.classes = words(),
                ("tools", Some(current)) => current.tools = words(),
                ("requires_function", Some(current)) => current.requires_function = parse_function(value),
                _ => {}
            }
        }

        Constructions { list, default_dialog }
    }

    /// `IsAvailable` for every recipe against the game's item table: one
    /// with a required function is offered only when the result's type is in
    /// the table and has that function. The table decides, so the same file
    /// serves Classic and Extended.
    pub fn for_items(mut self, types: &crate::items::ItemTypes) -> Constructions {
        for recipe in &mut self.list {
            recipe.available = match &recipe.requires_function {
                None => true,
                Some(function) => types
                    .index_of(&recipe.result)
                    .and_then(|k| types.get(k))
                    .is_some_and(|t| t.functions.iter().any(|f| f == function)),
            };
        }
        self
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Recipe> {
        self.list.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Recipe> {
        self.list.iter()
    }

    /// `SelectInspector` (`Constructions.cs:102-115`, v1.2): who judges
    /// `main_item`'s recipes. The current character when no recipe has it
    /// as a main item or when he may build an available one; otherwise the
    /// first of `candidates` (the group, leader first) who may; failing
    /// that, the current one again. `None` is the current character, `Some`
    /// the index into `candidates`.
    pub fn select_inspector(&self, current: &str, candidates: &[&str], main_item: &str) -> Option<usize> {
        let matching: Vec<&Recipe> = self.list.iter().filter(|r| r.main_items.iter().any(|m| m == main_item)).collect();
        let can_inspect = |class: &str| matching.iter().any(|r| r.available && r.allows(class));
        if matching.is_empty() || can_inspect(current) {
            return None;
        }
        candidates.iter().position(|c| can_inspect(c))
    }

    /// [`Self::select_inspector`]'s class.
    pub fn inspector_class<'c>(&self, current: &'c str, candidates: &[&'c str], main_item: &str) -> &'c str {
        self.select_inspector(current, candidates, main_item).map_or(current, |i| candidates[i])
    }

    /// `FindConstruction`: what right-clicking `main_item` offers this
    /// builder, given what they carry and what lies beside them.
    ///
    /// The first recipe that can be built wins. Failing that, the first whose
    /// main item is present is kept anyway, with `can_build` false, so the
    /// builder says what is missing instead of the blank answer.
    pub fn find(
        &self,
        class: &str,
        main_item: &str,
        carried: &[String],
        nearby: &[String],
    ) -> Option<Offer> {
        let mut fallback = None;
        for (index, recipe) in self.list.iter().enumerate() {
            if !recipe.available || !recipe.main_items.iter().any(|m| m == main_item) || !recipe.allows(class) {
                continue;
            }
            if recipe.can_build(carried, nearby) {
                return Some(Offer { recipe: index, can_build: true, dialog: recipe.dialog });
            }
            if fallback.is_none() {
                fallback = Some(Offer { recipe: index, can_build: false, dialog: recipe.dialog });
            }
        }
        fallback
    }
}

/// `ParseFunction`: `metal_detection` or `metal-detection` names
/// `ItemFunction.MetalDetection`; anything else is `None`, which gates
/// nothing.
fn parse_function(value: &str) -> Option<String> {
    const FUNCTIONS: [&str; 4] = ["remote_intel", "resting_sustenance", "creature_deterrent", "metal_detection"];
    let name = value.trim().replace('-', "_").to_ascii_lowercase();
    FUNCTIONS.contains(&name.as_str()).then_some(name)
}

/// What the builder says to an item right-clicked for building:
/// `Constructions.GetConstruction`.
///
/// The right click that reaches this always opens a conversation — with no
/// recipe too. Then the item's own `fluff` text is said if it has one (only
/// the rope does), else section 3 of the header's `dialog=` file, "I have no
/// idea what to make out of this."
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// A recipe, with or without the Yes line.
    Recipe(Offer),
    /// The item's `fluff`, a text reference.
    Fluff(String),
    /// The header's `dialog=`.
    Default(u32),
}

impl Constructions {
    pub fn answer(
        &self,
        class: &str,
        main_item: &str,
        fluff: Option<&str>,
        carried: &[String],
        nearby: &[String],
    ) -> Answer {
        match self.find(class, main_item, carried, nearby) {
            Some(offer) => Answer::Recipe(offer),
            None => match fluff {
                Some(text) => Answer::Fluff(text.to_string()),
                None => Answer::Default(self.default_dialog),
            },
        }
    }
}

/// Where the parts came from, and so where the result goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum From {
    /// The builder's own pack.
    Carried,
    /// The room's pile, or the ground beside them.
    Nearby,
}

/// What building costs and what it yields: `Constructions.Construct`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    /// One entry per item of the recipe, saying which pile it is taken from.
    /// The builder's pack is emptied first; the pile only covers what the
    /// builder lacks.
    pub take: Vec<(String, From)>,
    /// What is made.
    pub result: String,
    /// Where it is put: the builder's pack if the main item was theirs,
    /// otherwise the pile the main item came from.
    pub into: From,
}

impl Constructions {
    /// Work out the whole exchange without performing it, so that the caller
    /// moves the items and this stays a rule.
    ///
    /// `Construct` decides where the result goes by asking whether the
    /// builder holds the **main item**, and it asks before taking anything.
    pub fn build(&self, offer: &Offer, main_item: &str, carried: &[String]) -> Option<Build> {
        let recipe = self.get(offer.recipe)?;
        let into =
            if carried.iter().any(|i| i == main_item) { From::Carried } else { From::Nearby };

        // A pack the taking is simulated against: two of a thing must not be
        // paid for with one, and `Find` takes the first that matches.
        let mut pack: Vec<&String> = carried.iter().collect();
        let mut take = Vec::new();
        for need in &recipe.items {
            let taken = match pack.iter().position(|i| *i == need) {
                Some(at) => {
                    pack.remove(at);
                    From::Carried
                }
                None => From::Nearby,
            };
            take.push((need.clone(), taken));
        }

        Some(Build { take, result: recipe.result.clone(), into })
    }
}
