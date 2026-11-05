//! Barter with a trader.
//!
//! **There is no money in Burntime.** Both sides put items on the table and
//! the deal stands when the player's side is worth at least the trader's.
//! `TraderScene.CheckTrade`, whole:
//!
//! ```csharp
//! int player = exchangeBottom.Grid.GetTradeValue();
//! int trader = exchangeTop.Grid.GetTradeValue();
//! result = (player >= trader && exchangeBottom.Grid.Count > 0) ? Ok : Ng;
//! if (exchangeTop.Count - exchangeBottom.Count > inventory.FreeSlots)       result = Ng;
//! if (exchangeBottom.Count - exchangeTop.Count > inventoryTrader.FreeSlots) result = Ng;
//! ```
//!
//! Three things fall out of it that are easy to get wrong:
//!
//! * **The player must offer something.** `Count > 0` on their side, so a
//!   trader who is giving something away for nothing still cannot.
//! * **Overpaying is allowed and there is no change.** `>=`, not `==`.
//! * **Room is checked both ways, on the net flow.** Taking three and
//!   giving one needs two free slots, not three.
//!
//! And the exchange reaches **the whole group**, not the player's own pack:
//! `foreach (Character chr in inventory.ActiveCharacter.GetGroup())`.
//!
//! # Where we differ, on purpose
//!
//! Upstream's accept button re-reads the verdict the panel last computed —
//! `if (exchangeTop.ExchangeResult == ExchangeResult.Ng) return;`.
//! [`exchange`] works out the verdict again from the model instead, the way
//! [`crate::hire`] does, so that what a panel drew a moment ago cannot
//! authorise a swap that is no longer legal.

use alloc::vec::Vec;

use crate::game::ItemBag;
use crate::items::ItemTypes;

/// What each item is worth, by its `burngfx` sprite id.
///
/// **The numbers are not in `items.txt`.** `ItemTypesData` builds a classic
/// item's `TradeValue` as `ConstValues.GetValue(burngfx) / 4.0f`, from a
/// table indexed by the DOS sprite id — which is exactly
/// [`crate::items::ItemType::sprite`]. So this is a separate input rather
/// than another field on an item, and the game supplies it the way it
/// supplies the production rules.
///
/// The stored number is the original's own: the fourth of the four columns
/// its comments show (`5,10,15,21,snake` is stored as 21). Dividing by four
/// lands back on the first, which is what the remaster uses and therefore
/// what [`Prices::of`] returns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prices {
    by_sprite: Vec<i32>,
}

impl Prices {
    /// One `sprite = value` per line; `'` starts a comment, as everywhere
    /// else in this data.
    pub fn parse(text: &str) -> Prices {
        let mut by_sprite: Vec<i32> = Vec::new();
        for line in text.lines() {
            let line = match line.find('\'') {
                Some(at) => &line[..at],
                None => line,
            };
            let line = line.trim();
            if line.is_empty() || line.starts_with('[') {
                continue;
            }
            let Some((sprite, value)) = line.split_once('=') else { continue };
            let (Ok(sprite), Ok(value)) =
                (sprite.trim().parse::<usize>(), value.trim().parse::<i32>())
            else {
                continue;
            };
            if by_sprite.len() <= sprite {
                by_sprite.resize(sprite + 1, 0);
            }
            by_sprite[sprite] = value;
        }
        Prices { by_sprite }
    }

    pub fn is_empty(&self) -> bool {
        self.by_sprite.iter().all(|&v| v == 0)
    }

    /// What one item of that sprite is worth. **A quarter of the stored
    /// number, and a fraction**: a snake is 21/4 = 5.25, not 5. The
    /// fraction is not decoration — see [`value_of`].
    pub fn of(&self, sprite: Option<u8>) -> f32 {
        let Some(sprite) = sprite else { return 0.0 };
        self.by_sprite.get(sprite as usize).copied().unwrap_or(0) as f32 / 4.0
    }
}

/// What a heap of items is worth.
///
/// **The sum is taken in fractions and truncated once, at the end** —
/// `GetTradeValue` is `float v = 0; foreach … v += item.TradeValue; return
/// (int)v;`. Two snakes at 5.25 are worth 10, not 5 + 5. Rounding each item
/// first would quietly change every price in the game.
pub fn value_of(kinds: &[usize], types: &ItemTypes, prices: &Prices) -> i32 {
    let mut total = 0.0f32;
    for &kind in kinds {
        total += worth_of(kind, types, prices);
    }
    total as i32
}

/// What one item is worth, as a fraction.
///
/// `ItemTypesData` sets `TradeValue`, `EatValue` and `DrinkValue` in the same
/// three lines and from the same number, so this one answer is the barter
/// price, the restaurant's meal and the pub's drink alike:
///
/// * a classic item: `ConstValues.GetValue(burngfx) / 4.0f` — [`Prices::of`];
/// * **unless its section has `value=`**, which then wins, also divided by
///   four;
/// * an item with no `burngfx` at all: `value / 4.0f`, or nothing.
///
/// Only Extended's table has the line, on the items the remaster added. Until
/// this read it those were worth nothing to a trader in Extended — a dish
/// sold for nothing — while the source prices it at six.
pub fn worth_of(kind: usize, types: &ItemTypes, prices: &Prices) -> f32 {
    match types.get(kind).and_then(|t| t.value) {
        Some(value) => value as f32 / 4.0,
        None => prices.of(types.sprite_of(kind)),
    }
}

/// Why a deal does not stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The player put nothing on the table. `Count > 0` on their side, so
    /// even a gift has to be paid for.
    YouOfferNothing,
    /// The player's side is worth less than the trader's.
    NotEnough { offered: i32, asked: i32 },
    /// The group has nowhere to put what it would receive.
    NoRoomInYourPacks,
    /// The trader has nowhere to put what they would receive.
    NoRoomForTheTrader,
    /// A slot named on either side does not hold anything.
    NoSuchSlot,
    /// The same slot was named twice. **One item is one item**: counting it
    /// twice towards the price and then removing two different things
    /// afterwards would make an item out of nothing.
    SlotNamedTwice,
}

/// One side's picks: which bag, and which slot in it.
///
/// The player's side names a bag because the exchange reaches the whole
/// group — index 0 is the leader's pack, then one per companion.
pub type Pick = (usize, usize);

/// What a panel needs to show about a deal in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    pub offered: i32,
    pub asked: i32,
    /// Free slots the group would have left, after the swap.
    pub your_room: usize,
    pub their_room: usize,
}

/// Whose barter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Barter {
    /// v0.9.1's `CheckTrade`: whole sums, `>=`.
    Legacy,
    /// v1.2's `AcceptTrade`: quarter values, the offer times the
    /// difficulty's `barter_factor` per cent.
    V12 { factor: i32 },
}

/// Would this deal stand? Changes nothing.
pub fn check(
    packs: &[&ItemBag],
    theirs: &ItemBag,
    from_you: &[Pick],
    from_them: &[usize],
    types: &ItemTypes,
    prices: &Prices,
) -> Result<Standing, Refusal> {
    check_with(packs, theirs, from_you, from_them, types, prices, Barter::Legacy)
}

/// [`check`], under the given barter.
pub fn check_with(
    packs: &[&ItemBag],
    theirs: &ItemBag,
    from_you: &[Pick],
    from_them: &[usize],
    types: &ItemTypes,
    prices: &Prices,
    barter: Barter,
) -> Result<Standing, Refusal> {
    let mut yours = Vec::new();
    for (i, &(bag, slot)) in from_you.iter().enumerate() {
        if from_you[..i].contains(&(bag, slot)) {
            return Err(Refusal::SlotNamedTwice);
        }
        let kind = packs.get(bag).and_then(|b| b.get(slot)).ok_or(Refusal::NoSuchSlot)?;
        yours.push(kind);
    }
    let mut wanted = Vec::new();
    for (i, &slot) in from_them.iter().enumerate() {
        if from_them[..i].contains(&slot) {
            return Err(Refusal::SlotNamedTwice);
        }
        wanted.push(theirs.get(slot).ok_or(Refusal::NoSuchSlot)?);
    }

    if yours.is_empty() {
        return Err(Refusal::YouOfferNothing);
    }
    let offered = value_of(&yours, types, prices);
    let asked = value_of(&wanted, types, prices);
    let stands = match barter {
        Barter::Legacy => offered >= asked,
        Barter::V12 { factor } => {
            let worth = |k: &[usize]| k.iter().map(|&x| worth_of(x, types, prices)).collect::<Vec<f32>>();
            crate::v12_economy::accept_trade(&worth(&yours), &worth(&wanted), factor)
        }
    };
    if !stands {
        return Err(Refusal::NotEnough { offered, asked });
    }

    // Room, on the net flow and in that direction only: giving away more
    // than you take never needs a slot.
    let your_free: usize = packs.iter().map(|b| b.capacity() - b.len()).sum();
    let their_free = theirs.capacity() - theirs.len();
    if wanted.len().saturating_sub(yours.len()) > your_free {
        return Err(Refusal::NoRoomInYourPacks);
    }
    if yours.len().saturating_sub(wanted.len()) > their_free {
        return Err(Refusal::NoRoomForTheTrader);
    }

    Ok(Standing {
        offered,
        asked,
        your_room: your_free + yours.len() - wanted.len(),
        their_room: their_free + wanted.len() - yours.len(),
    })
}

/// Do the deal. **Re-checks it first, and then either everything moves or
/// nothing does.**
///
/// Both sides are taken out before either is filled, which is what makes
/// the room arithmetic above the right arithmetic: an item leaving frees
/// its slot for one arriving.
pub fn exchange(
    packs: &mut [&mut ItemBag],
    theirs: &mut ItemBag,
    from_you: &[Pick],
    from_them: &[usize],
    types: &ItemTypes,
    prices: &Prices,
) -> Result<Standing, Refusal> {
    exchange_with(packs, theirs, from_you, from_them, types, prices, Barter::Legacy)
}

/// [`exchange`], under the given barter.
pub fn exchange_with(
    packs: &mut [&mut ItemBag],
    theirs: &mut ItemBag,
    from_you: &[Pick],
    from_them: &[usize],
    types: &ItemTypes,
    prices: &Prices,
    barter: Barter,
) -> Result<Standing, Refusal> {
    let standing = {
        let borrowed: Vec<&ItemBag> = packs.iter().map(|b| &**b).collect();
        check_with(&borrowed, theirs, from_you, from_them, types, prices, barter)?
    };

    // What changes hands, in the order it was clicked. v1.2's `OnButtonAccept`
    // moves the two exchange grids, which are lists in click order
    // (`Scenes/TraderScene.cs:298-306`, `ItemList.Move`), so a v1.2 game
    // fills the packs and the stock in that order. The legacy game keeps
    // the order this port has always had: highest slot first.
    // A v1.2 game also moves each item's rounds with it: Accept moves the
    // `Item` objects, whose `ammo` is their own (`Logic/Item/Item.cs:
    // 103-137`). The legacy game's deal never carried them.
    let v12 = matches!(barter, Barter::V12 { .. });
    let in_click_order = v12;
    let clicked_given: Vec<(usize, Option<i32>)> =
        from_you.iter().filter_map(|&(bag, slot)| Some((packs[bag].get(slot)?, packs[bag].rounds_left(slot)))).collect();
    let clicked_taken: Vec<(usize, Option<i32>)> =
        from_them.iter().filter_map(|&slot| Some((theirs.get(slot)?, theirs.rounds_left(slot)))).collect();

    // Slots shift as things are removed, so take the highest first — and
    // gather what was there before anything moves.
    let mut yours: Vec<Pick> = from_you.to_vec();
    yours.sort_unstable_by(|a, b| b.cmp(a));
    let mut given = Vec::new();
    for &(bag, slot) in &yours {
        if let Some(kind) = packs[bag].remove(slot) {
            given.push((kind, None));
        }
    }
    let mut wanted: Vec<usize> = from_them.to_vec();
    wanted.sort_unstable_by(|a, b| b.cmp(a));
    let mut taken = Vec::new();
    for &slot in &wanted {
        if let Some(kind) = theirs.remove(slot) {
            taken.push((kind, None));
        }
    }
    if in_click_order {
        given = clicked_given;
        taken = clicked_taken;
    }

    // `MoveItems` fills the group's packs in order, so the leader's first.
    for (kind, rounds) in taken {
        for bag in packs.iter_mut() {
            if bag.push(kind) {
                let last = bag.len() - 1;
                bag.set_rounds_left(last, rounds.filter(|_| v12));
                break;
            }
        }
    }
    for (kind, rounds) in given {
        if theirs.push(kind) {
            let last = theirs.len() - 1;
            theirs.set_rounds_left(last, rounds.filter(|_| v12));
        }
    }

    Ok(standing)
}
