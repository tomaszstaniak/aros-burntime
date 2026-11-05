//! A trader, and what they have to sell.
//!
//! Ported from v0.9.1's `Trader.cs` and `GameCreation.LoadTrader`.
//!
//! **Two behaviours, and only one of them is on a counter.** `Turn` calls
//! `NextSellLocation` and then `RefreshItems`, and the `counter` field is
//! incremented and cleared *only* inside the first: the trader moves every
//! second turn, while the stock is churned every turn. Reading one counter
//! as driving both would make a saved trader behave differently after a
//! load, which is exactly the kind of thing a save has to get right.
//!
//! # The numbers, and three the C# spells differently than it reads
//!
//! `Platform.Math.Random` is .NET's `System.Random`, where `Next(a, b)`
//! yields `a..b-1`. So:
//!
//! * `RandomizeInventory` does `for (int count = Random.Next(3, 6); count >= 0; count--)`,
//!   which is 3, 4 or 5 counting down through zero — **four to six items**, not three to six.
//! * `RefreshItems` swaps `Random.Next(1, 3)` — **one or two**.
//! * and targets `Random.Next(2, 6)` — **two to five**, though the constant
//!   is named `MAX_STOCK_ITEMS = 6`.
//!
//! There is a fourth, which looks like a slip rather than a choice:
//! `HomeArea.Neighbors[Random.Next(Neighbors.Count - 1)]` can never pick
//! the last neighbour. [`Trader::next_sell_location`] follows it and says
//! so, the same way [`crate::settings::Rate`] does for its own off-by-one.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::game::ItemBag;
use crate::spawn::Rng;

/// `Trader.Items.MaxCount` for a trader: six like anyone else, plus six
/// more because `LoadNPCs` gives a trader `MaxCount += 6`. The stock itself
/// is capped lower — see [`MAX_STOCK`].
pub const TRADER_PACK: usize = 12;

/// `_maxStockItemCount`. `RefreshItems` will not add past it.
pub const MAX_STOCK: usize = 7;

/// What each trader restocks from, read from `trader.txt`.
///
/// Keyed by the character's `NameId` — 179 is Mad Marty, 180 Chuck — which
/// `LoadNPCs` also copies into `TraderId`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StockLists {
    lists: Vec<(u16, Vec<String>)>,
    /// v1.2's `:weight` per entry, one per name (1 where none is written).
    rates: Vec<Vec<i32>>,
}

impl StockLists {
    /// ```text
    /// [trader]
    /// ' Mad Marty
    /// 179=item_meat item_knife item_loaded_rifle
    /// ```
    ///
    /// v1.2 adds a weight after the last colon, `item_meat:5`
    /// (`TraderAssortmentSetting.Parse`): one that is not a positive number
    /// is weight 1, the name before the colon kept. v0.9.1's files have no
    /// colons, so this reads them as before.
    pub fn parse(text: &str) -> StockLists {
        let mut lists = Vec::new();
        let mut rates = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('\'') || line.starts_with('[') {
                continue;
            }
            let Some((id, items)) = line.split_once('=') else { continue };
            let Ok(id) = id.trim().parse::<u16>() else { continue };
            let mut names = Vec::new();
            let mut weights = Vec::new();
            for entry in items.split_whitespace() {
                let (name, rate) = match entry.rfind(':') {
                    Some(at) if at > 0 => {
                        let rate = entry[at + 1..].parse::<i32>().ok().filter(|r| *r > 0).unwrap_or(1);
                        (&entry[..at], rate)
                    }
                    _ => (entry, 1),
                };
                names.push(name.to_string());
                weights.push(rate);
            }
            if !names.is_empty() {
                lists.push((id, names));
                rates.push(weights);
            }
        }
        StockLists { lists, rates }
    }

    /// The weights of [`StockLists::of`], entry for entry.
    pub fn rates_of(&self, name_id: u16) -> &[i32] {
        self.lists
            .iter()
            .position(|(id, _)| *id == name_id)
            .map_or(&[][..], |i| self.rates[i].as_slice())
    }

    pub fn len(&self) -> usize {
        self.lists.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lists.is_empty()
    }

    /// What that trader may restock, by name. Empty for a trader with no
    /// entry, which `AddRefreshItem` would simply never be called for.
    pub fn of(&self, name_id: u16) -> &[String] {
        self.lists
            .iter()
            .find(|(id, _)| *id == name_id)
            .map_or(&[][..], |(_, items)| items.as_slice())
    }
}

/// Everybody in the world who trades.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Traders {
    traders: Vec<Trader>,
    /// v1.2's city shops (`Location.LocalTrader`): the location and the
    /// index of its trader here. They stand in their city and never walk.
    shops: Vec<(usize, usize)>,
}

/// `SaveGame.FirstTraderId`: the first city's shopkeeper is this record,
/// the next city's the next.
pub const FIRST_SHOP_RECORD: usize = 21;

impl Traders {
    /// `LoadTrader`, as far as it concerns us: every placed character of
    /// class `Trader` is one, and the location their record names is their
    /// `HomeArea`.
    ///
    /// The five records whose `LocationId` is out of range are *not* here.
    /// `LoadNPCs` does build a trader from each, but one with no home area,
    /// and `NextSellLocation` returns immediately for those — so they stand
    /// nowhere and never move. Nothing yet gives the player a way to meet
    /// one, and inventing a place for them would be inventing a rule.
    pub fn from_records(records: &[crate::gamdat::CharacterRecord]) -> Traders {
        let mut traders = Vec::new();
        for (record, r) in records.iter().enumerate() {
            if r.class != crate::gamdat::CharClass::Trader {
                continue;
            }
            if let crate::gamdat::Placed::At(home) = r.placed {
                traders.push(Trader::new(record, r.name_id, home));
            }
        }
        Traders { traders, shops: Vec::new() }
    }

    /// v1.2's `LoadTrader`: every city's shop, the k-th city's trader being
    /// record `21 + k` — the five records whose location is out of range,
    /// which `LoadNPCs` makes traders with no home area.
    pub fn with_city_shops(records: &[crate::gamdat::CharacterRecord], gam: &crate::gamdat::Gamdat) -> Traders {
        let mut t = Traders::from_records(records);
        for (k, (location, _)) in gam.locations.iter().enumerate().filter(|(_, l)| l.is_city).enumerate() {
            let record = FIRST_SHOP_RECORD + k;
            let Some(r) = records.get(record) else { continue };
            if r.class != crate::gamdat::CharClass::Trader {
                continue;
            }
            t.traders.push(Trader::new(record, r.name_id, location));
            t.shops.push((location, t.traders.len() - 1));
        }
        t
    }

    /// The shop of a city, by index into this list.
    pub fn shop_of(&self, location: usize) -> Option<usize> {
        self.shops.iter().find(|(l, _)| *l == location).map(|(_, i)| *i)
    }

    fn is_shop(&self, index: usize) -> bool {
        self.shops.iter().any(|(_, i)| *i == index)
    }

    pub fn len(&self) -> usize {
        self.traders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.traders.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Trader> + '_ {
        self.traders.iter()
    }

    pub fn get(&self, index: usize) -> Option<&Trader> {
        self.traders.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Trader> {
        self.traders.get_mut(index)
    }

    /// Whoever is standing in a location, by index into this list.
    pub fn at(&self, location: usize) -> impl Iterator<Item = usize> + '_ {
        self.traders
            .iter()
            .enumerate()
            .filter(move |(_, t)| t.at == location)
            .map(|(i, _)| i)
    }

    /// Replace one, for restoring a save. `false` when no trader has that
    /// record, which is a save that does not match this world.
    pub fn restore(&mut self, trader: Trader) -> bool {
        match self.traders.iter_mut().find(|t| t.record == trader.record) {
            Some(existing) => {
                *existing = trader;
                true
            }
            None => false,
        }
    }

    /// Open every trader's stock, once, at the start of a game.
    pub fn open_for_business(
        &mut self,
        lists: &StockLists,
        kind_of: &impl Fn(&str) -> Option<usize>,
        seed: u64,
    ) {
        for trader in &mut self.traders {
            let mut rng = rng_for(seed, 0, trader.record);
            trader.randomize_inventory(lists, kind_of, &mut rng);
        }
    }

    /// A day for every trader. Each draws from a stream of their own, so
    /// the order they are visited in does not change what any of them does.
    pub fn turn(
        &mut self,
        lists: &StockLists,
        kind_of: &impl Fn(&str) -> Option<usize>,
        neighbours_of: &impl Fn(usize) -> Vec<usize>,
        seed: u64,
        day: u16,
    ) {
        for trader in &mut self.traders {
            let neighbours = neighbours_of(trader.home);
            let mut rng = rng_for(seed, day, trader.record);
            trader.turn(lists, kind_of, &neighbours, &mut rng);
        }
    }
}

/// v1.2's `trader_refresh` for a day's turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// `dos_individual` (`OriginalTraderRefresh.Dos`), then the walk.
    Dos { meat: usize },
    /// `remaster_random` (`TurnExtendedTrader`): the walk, then the weighted
    /// churn. `one_firearm` is Extended's rule that a trader holding a
    /// firearm is offered no other.
    Remaster { one_firearm: bool },
}

impl Traders {
    /// `RandomizeInventory` for every trader, as v1.2 opens Extended's
    /// (`trader_inventory = remaster_random`), in the order `LoadTrader`
    /// meets them, every draw from the game's generator.
    pub fn open_for_business_v12(
        &mut self,
        lists: &StockLists,
        types: &crate::items::ItemTypes,
        world: &impl Fn(&str) -> usize,
        one_firearm: bool,
        rng: &mut Rng,
    ) {
        for i in 0..self.traders.len() {
            let others = self.group_counts_except(i, types);
            let t = &mut self.traders[i];
            t.stock = ItemBag::new(TRADER_PACK);
            // `for (count = Next(3, 6); count >= 0; count--)`.
            let mut count = rng.range(3, 6);
            loop {
                let Some(kind) = t.next_item_v12(lists, types, &|g| world(g) + others(g), one_firearm, rng) else { break };
                t.stock.push(kind);
                if count == 0 {
                    break;
                }
                count -= 1;
            }
        }
    }

    /// v1.2's day for every trader (`World.TurnTraders`), in order, every
    /// draw from the game's generator. `world(group)` counts the items of a
    /// world-limited group everywhere but in the traders' stock, which is
    /// counted here as it changes.
    #[allow(clippy::too_many_arguments)]
    pub fn turn_v12(
        &mut self,
        refresh: Refresh,
        lists: &StockLists,
        types: &crate::items::ItemTypes,
        world: &impl Fn(&str) -> usize,
        neighbours_of: &impl Fn(usize) -> Vec<usize>,
        day: u16,
        rng: &mut Rng,
    ) {
        for i in 0..self.traders.len() {
            let neighbours = neighbours_of(self.traders[i].home);
            let shop = self.is_shop(i);
            match refresh {
                Refresh::Dos { meat } => {
                    let t = &mut self.traders[i];
                    let assortment: Vec<usize> =
                        lists.of(t.name_id).iter().filter_map(|n| types.index_of(n)).collect();
                    t.dos_refresh(&assortment, day, &|k| crate::amiga_ai::title_id(types.sprite_of(k)), meat, rng);
                    if !shop {
                        t.next_sell_location(&neighbours, rng);
                    }
                }
                Refresh::Remaster { one_firearm } => {
                    let others = self.group_counts_except(i, types);
                    let t = &mut self.traders[i];
                    if !shop {
                        t.next_sell_location(&neighbours, rng);
                    }
                    t.refresh_items_v12(lists, types, &|g| world(g) + others(g), one_firearm, rng);
                }
            }
        }
    }

    /// One trader's v1.2 turn (`Trader.Turn` → `RuleBook.TurnTrader`), every
    /// draw from the game's generator: under `dos_individual` the refresh
    /// then the walk, under `remaster_random` the walk then the churn. A city
    /// shop has no home area to walk from (`NextSellLocation` returns before
    /// its counter). The caller does not turn a dead one (`if (IsDead)
    /// return;`). Returns where he walked to, when he walked.
    #[allow(clippy::too_many_arguments)]
    pub fn turn_one_v12(
        &mut self,
        i: usize,
        refresh: Refresh,
        lists: &StockLists,
        types: &crate::items::ItemTypes,
        world: &impl Fn(&str) -> usize,
        neighbours_of: &impl Fn(usize) -> Vec<usize>,
        day: u16,
        rng: &mut Rng,
    ) -> Option<usize> {
        let shop = self.is_shop(i);
        let neighbours = neighbours_of(self.traders.get(i)?.home);
        let before = self.traders[i].at;
        match refresh {
            Refresh::Dos { meat } => {
                let t = &mut self.traders[i];
                let assortment: Vec<usize> = lists.of(t.name_id).iter().filter_map(|n| types.index_of(n)).collect();
                t.dos_refresh(&assortment, day, &|k| crate::amiga_ai::title_id(types.sprite_of(k)), meat, rng);
                if !shop {
                    t.next_sell_location(&neighbours, rng);
                }
            }
            Refresh::Remaster { one_firearm } => {
                let others = self.group_counts_except(i, types);
                let t = &mut self.traders[i];
                if !shop {
                    t.next_sell_location(&neighbours, rng);
                }
                t.refresh_items_v12(lists, types, &|g| world(g) + others(g), one_firearm, rng);
            }
        }
        let after = self.traders[i].at;
        (after != before).then_some(after)
    }

    /// Which trader, by index here, a `gam.dat` record is.
    pub fn index_of(&self, record: usize) -> Option<usize> {
        self.traders.iter().position(|t| t.record == record)
    }

    /// v1.2's city shops (`World.Traders`), in city order, by index here.
    pub fn shops(&self) -> Vec<usize> {
        self.shops.iter().map(|&(_, i)| i).collect()
    }

    /// How many of each world-limited group the other traders hold.
    fn group_counts_except(&self, me: usize, types: &crate::items::ItemTypes) -> impl Fn(&str) -> usize {
        let mut held: Vec<(String, usize)> = Vec::new();
        for (j, t) in self.traders.iter().enumerate() {
            if j == me {
                continue;
            }
            for k in t.stock.iter() {
                let g = world_group(types, k);
                match held.iter_mut().find(|(n, _)| *n == g) {
                    Some(e) => e.1 += 1,
                    None => held.push((g, 1)),
                }
            }
        }
        move |g: &str| held.iter().find(|(n, _)| n == g).map_or(0, |e| e.1)
    }
}

/// `TraderWorldGroup`, or the item's own id when it has none.
pub fn world_group(types: &crate::items::ItemTypes, kind: usize) -> String {
    types
        .get(kind)
        .map(|t| t.trader_world_group.clone().unwrap_or_else(|| t.name.clone()))
        .unwrap_or_default()
}

/// `AiItemPool.IsFirearm`.
pub fn is_firearm(name: &str) -> bool {
    matches!(
        name,
        "item_loaded_rifle" | "item_loaded_rifle_1" | "item_loaded_pistol" | "item_unloaded_rifle" | "item_unloaded_pistol"
    )
}

/// The generator for one trader on one day.
///
/// **This is the decision that keeps a save honest.** The production
/// generator carries no state because it is rebuilt from `seed ^ day` every
/// day; a trader could not borrow that, because several traders act on the
/// same day and would all draw the same numbers. So each gets a stream of
/// their own, mixed from the seed, the day and **their record index in
/// `gam.dat`** — an identity that does not move when the packer's profile
/// changes, unlike a position in any list we happen to build.
///
/// What that buys: `(seed, day)` plus each trader's own stock and counter
/// is the whole of what a save needs. There is no sequence position to
/// lose, no dependence on the order traders are visited within a day, and
/// no dependence on how many locations were packed.
///
/// What it costs: our traders do not draw the same numbers the remaster's
/// do. They could not anyway — `Platform.Math.Random` is seeded from
/// `DateTime.Now.Ticks` and is not reproducible between two runs of the
/// remaster itself, let alone between it and this.
pub fn rng_for(seed: u64, day: u16, record: usize) -> Rng {
    // Odd constants from the usual mixing folklore, used only to keep the
    // three inputs from cancelling: xorshift64* tolerates a poor seed but
    // `day ^ record` would collide for obvious pairs.
    let mixed = seed
        ^ (day as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (record as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    Rng::seeded(mixed)
}

/// One trader's own state: what they carry, where they are, and how far
/// through their two-turn walk they have got.
#[derive(Clone, Debug, PartialEq)]
pub struct Trader {
    /// Their index in the character table, which is their identity.
    pub record: usize,
    /// `TraderId`, the key into [`StockLists`].
    pub name_id: u16,
    /// Where they are now.
    pub at: usize,
    /// `HomeArea` — the city they belong to and return to.
    pub home: usize,
    pub stock: ItemBag,
    /// `counter`. **Movement only.** It reaches two on every second turn,
    /// moves the trader and resets. Nothing else reads it.
    pub counter: u32,
}

impl Trader {
    pub fn new(record: usize, name_id: u16, home: usize) -> Trader {
        Trader {
            record,
            name_id,
            at: home,
            home,
            stock: ItemBag::new(TRADER_PACK),
            counter: 0,
        }
    }

    /// `RandomizeInventory`: clear the stock and fill it with four to six
    /// distinct kinds from the refresh list.
    ///
    /// Distinct because `GetNextItem` only ever offers kinds the trader is
    /// not already holding, so a short list caps the stock below the count
    /// asked for rather than repeating anything.
    pub fn randomize_inventory(&mut self, lists: &StockLists, kind_of: &impl Fn(&str) -> Option<usize>, rng: &mut Rng) {
        self.stock = ItemBag::new(TRADER_PACK);
        let mut count = 3 + rng.below(3); // Next(3, 6): 3, 4 or 5
        while let Some(kind) = self.next_item(lists, kind_of, rng) {
            if !self.stock.push(kind) || count == 0 {
                break;
            }
            count -= 1;
        }
    }

    /// `RefreshItems`: churn one or two items a turn, steering the stock
    /// towards two to five and never past [`MAX_STOCK`].
    pub fn refresh_items(&mut self, lists: &StockLists, kind_of: &impl Fn(&str) -> Option<usize>, rng: &mut Rng) {
        let swap = (self.stock.len() as u32).min(1 + rng.below(2)); // Next(1, 3)
        let mut remove = swap;
        let mut add = swap;

        let target = 2 + rng.below(4); // Next(2, 6): 2..5
        // `projectedCount` is `Count + add - remove`, and the two are equal
        // here, so it is simply what is in stock now.
        let projected = self.stock.len() as u32;
        if target > projected {
            add += target - projected;
        } else {
            remove += projected - target;
        }
        // "keep add/remove within 2 difference" says the comment; the code
        // keeps them within one.
        if add > remove {
            add = add.min(remove + 1);
        } else {
            remove = remove.min(add + 1);
        }

        for _ in 0..remove {
            if self.stock.is_empty() {
                break;
            }
            let slot = rng.below(self.stock.len() as u32) as usize;
            self.stock.remove(slot);
        }
        for _ in 0..add {
            if self.stock.len() >= MAX_STOCK {
                break;
            }
            let Some(kind) = self.next_item(lists, kind_of, rng) else { break };
            self.stock.push(kind);
        }
    }

    /// `GetNextItem`: a kind from the refresh list that is not in stock
    /// already, chosen at random. `None` when there is none left.
    fn next_item(&self, lists: &StockLists, kind_of: &impl Fn(&str) -> Option<usize>, rng: &mut Rng) -> Option<usize> {
        let mut choices: Vec<usize> = Vec::new();
        for name in lists.of(self.name_id) {
            let Some(kind) = kind_of(name) else { continue };
            if !self.stock.iter().any(|k| k == kind) {
                choices.push(kind);
            }
        }
        match choices.len() {
            0 => None,
            // "only one item type, skip randomizer" — and skipping it
            // matters, because drawing a number changes what comes next.
            1 => Some(choices[0]),
            n => Some(choices[rng.below(n as u32) as usize]),
        }
    }

    /// `NextSellLocation`: every second turn, walk between home and one of
    /// its neighbours.
    ///
    /// **With two neighbours or more, the last is never chosen.**
    /// `Random.Next(Count - 1)` yields `0..Count-2`, which looks like a slip
    /// rather than a rule, but it is the behaviour and following it costs
    /// nothing.
    ///
    /// **One neighbour is not a special case.** `System.Random.Next(0)`
    /// returns 0, so `Neighbors[Next(Count - 1)]` picks the only neighbour
    /// there is. An earlier version of this kept the trader at home for a
    /// list of one, which was a deviation dressed up as a guard.
    ///
    /// **`Next(0)` still spends a number** on .NET's seeded generator (its
    /// `(int)(Sample() * 0)`; `tests/net_random.rs` replays such calls), so
    /// on a v1.2 game's generator a list of one draws as upstream's does. A
    /// legacy stream's `below(0)` draws nothing, as it never did.
    ///
    /// An empty list is the one place the original has no answer:
    /// `Next(-1)` throws. Staying home is ours, and it is a choice rather
    /// than a reading.
    pub fn next_sell_location(&mut self, neighbours: &[usize], rng: &mut Rng) {
        self.counter += 1;
        if self.counter <= 1 {
            return;
        }
        self.counter = 0;
        if self.at == self.home {
            let Some(last) = neighbours.len().checked_sub(1) else { return };
            self.at = neighbours[rng.below(last as u32) as usize];
        } else {
            self.at = self.home;
        }
    }

    /// `OriginalTraderRefresh.Dos`: the first food (titles `0x33..=0x36`)
    /// goes, then the first thing outside the first four of the assortment;
    /// of the first six, one held is dropped one time in eight, one not
    /// held comes in when its title matches the day modulo four; and meat
    /// every second day. `AddTraderStock` ignores the carrying limit.
    pub fn dos_refresh(&mut self, assortment: &[usize], day: u16, title: &impl Fn(usize) -> i32, meat: usize, rng: &mut Rng) {
        let first = |stock: &ItemBag, f: &dyn Fn(usize) -> bool| (0..stock.len()).find(|&s| stock.get(s).is_some_and(f));
        if let Some(s) = first(&self.stock, &|k| (0x33..=0x36).contains(&title(k))) {
            self.stock.remove(s);
        }
        let core: Vec<i32> = assortment.iter().take(4).map(|&k| title(k)).collect();
        if let Some(s) = first(&self.stock, &|k| !core.contains(&title(k))) {
            self.stock.remove(s);
        }
        for &kind in assortment.iter().take(6) {
            let t = title(kind);
            match first(&self.stock, &|k| title(k) == t) {
                Some(s) => {
                    if rng.below(8) == 0 {
                        self.stock.remove(s);
                    }
                }
                None => {
                    if t % 4 == day as i32 % 4 {
                        self.add_stock(kind);
                    }
                }
            }
        }
        if day % 2 == 0 {
            self.add_stock(meat);
        }
    }

    /// `AddTraderStock`: past any limit. The bag grows when it must.
    fn add_stock(&mut self, kind: usize) {
        if self.stock.is_full() {
            let mut bigger = ItemBag::new(self.stock.capacity() + TRADER_PACK);
            for (i, k) in self.stock.iter().enumerate() {
                bigger.push(k);
                bigger.set_rounds_left(i, self.stock.rounds_left(i));
            }
            self.stock = bigger;
        }
        self.stock.push(kind);
    }

    /// v1.2's `GetNextItem`: of the kinds not held, those a world limit
    /// still allows and — with `one_firearm` — no second firearm, chosen by
    /// weight (`SelectWeightedIndex` over `Next(total)`); one alone without
    /// a draw.
    pub fn next_item_v12(
        &self,
        lists: &StockLists,
        types: &crate::items::ItemTypes,
        world: &impl Fn(&str) -> usize,
        one_firearm: bool,
        rng: &mut Rng,
    ) -> Option<usize> {
        let holds_firearm = self.stock.iter().any(|k| types.name_of(k).is_some_and(is_firearm));
        let mut choices: Vec<(usize, i32)> = Vec::new();
        for (name, &rate) in lists.of(self.name_id).iter().zip(lists.rates_of(self.name_id)) {
            let Some(kind) = types.index_of(name) else { continue };
            if self.stock.iter().any(|k| k == kind) {
                continue;
            }
            if one_firearm && is_firearm(name) && holds_firearm {
                continue;
            }
            let limit = types.get(kind).map_or(0, |t| t.trader_world_limit);
            if limit > 0 {
                let mine = self.stock.iter().filter(|&k| world_group(types, k) == world_group(types, kind)).count();
                if world(&world_group(types, kind)) + mine >= limit as usize {
                    continue;
                }
            }
            choices.push((kind, rate.max(1)));
        }
        match choices.len() {
            0 => None,
            1 => Some(choices[0].0),
            _ => {
                let total: i32 = choices.iter().map(|c| c.1).sum();
                let mut pick = rng.below(total as u32) as i32;
                for &(kind, rate) in &choices {
                    pick -= rate;
                    if pick < 0 {
                        return Some(kind);
                    }
                }
                choices.last().map(|c| c.0)
            }
        }
    }

    /// v1.2's `RefreshItems`: as v0.9.1's, but only what no world limit
    /// holds may be taken out (`CanRemoveFromStock`), and what comes in is
    /// [`Trader::next_item_v12`].
    pub fn refresh_items_v12(
        &mut self,
        lists: &StockLists,
        types: &crate::items::ItemTypes,
        world: &impl Fn(&str) -> usize,
        one_firearm: bool,
        rng: &mut Rng,
    ) {
        // `Min(count, Next(1, 3))`, then `Next(2, 6)`.
        let swap = (self.stock.len() as u32).min(rng.range(1, 3) as u32);
        let mut remove = swap;
        let mut add = swap;
        let target = rng.range(2, 6) as u32;
        let projected = self.stock.len() as u32;
        if target > projected {
            add += target - projected;
        } else {
            remove += projected - target;
        }
        if add > remove {
            add = add.min(remove + 1);
        } else {
            remove = remove.min(add + 1);
        }
        for _ in 0..remove {
            if self.stock.is_empty() {
                break;
            }
            let removable: Vec<usize> = (0..self.stock.len())
                .filter(|&s| self.stock.get(s).and_then(|k| types.get(k)).is_some_and(|t| t.trader_world_limit <= 0))
                .collect();
            if removable.is_empty() {
                break;
            }
            let slot = removable[rng.below(removable.len() as u32) as usize];
            self.stock.remove(slot);
        }
        for _ in 0..add {
            if self.stock.len() >= MAX_STOCK {
                break;
            }
            let Some(kind) = self.next_item_v12(lists, types, world, one_firearm, rng) else { break };
            self.stock.push(kind);
        }
    }

    /// `Turn`, in its order: move, then churn the stock. A trader ignores
    /// food and water entirely — the comment in the original says so — and
    /// their health is restored, which nothing here models yet.
    pub fn turn(&mut self, lists: &StockLists, kind_of: &impl Fn(&str) -> Option<usize>, neighbours: &[usize], rng: &mut Rng) {
        self.next_sell_location(neighbours, rng);
        self.refresh_items(lists, kind_of, rng);
    }
}
