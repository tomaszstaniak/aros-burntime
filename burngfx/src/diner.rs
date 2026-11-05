//! The pub and the restaurant: things on the table for a drink or a meal,
//! shared out by the group.
//!
//! Ported from v0.9.1's `PubScene` and `RestaurantScene`. The two are one
//! scene written twice, and differ in exactly one thing: the pub calls
//! `Group.Drink` and the restaurant `Group.Eat`.
//!
//! # What the source actually says
//!
//! * **There is no price, and no menu.** What goes on the table is the
//!   payment; what it is worth is what is served. `OnButtonEat` is
//!   `eatLastAmount = grid.GetEatValue(); … group.Eat(leader, value);
//!   grid.Clear();`.
//! * **Worth is barter worth, not food.** `ItemTypesData` sets `EatValue`
//!   and `DrinkValue` in the same lines, from the same number, as
//!   `TradeValue` — see [`crate::trade::worth_of`]. A knife buys ten days of
//!   water in a pub; a snake's `food=7` does not enter into it. The sum is
//!   taken in fractions and truncated once.
//! * **Anything may go on the table.** Unlike the doctor, neither scene's
//!   `OnLeftClickItemInventory` looks at the item.
//! * **The table holds four.** `grid.Grid = new Vector2(4, 1)`.
//! * **Served to the group, within fifty of the leader.** The leader passed
//!   in is `SelectedCharacter`, which switches `Group.RangeFilterValue` on:
//!   the same level-at-a-time sharing and the same filter as eating from the
//!   inventory — [`crate::game::group_eat_near`] and
//!   [`crate::game::group_drink_near`].
//! * **Only packs within fifty can pay.** The inventory window builds its
//!   pages from `group.IsInRange(leader, member)`, so a companion standing
//!   off has no page and nothing of theirs can reach the table.
//! * **What the group cannot take is lost.** `Group.Eat`'s return is thrown
//!   away and the items are cleared regardless — a full group pays and gets
//!   nothing, and the surplus of a part-full one is not given back.
//! * **An item with an empty form goes whole.** A full canteen is removed,
//!   not emptied: the table takes the item, not its contents.
//!
//! # Where we differ, and why
//!
//! * **The panel is a preview.** Upstream moves an item out of the pack as it
//!   is placed and back — to the group, the leader's pack first, not to its
//!   owner — only on Exit. [`check`] marks slots and changes nothing;
//!   [`serve`] re-checks and then moves everything or nothing. The same shape
//!   as [`crate::doctor`] and [`crate::trade`].
//! * **Worthless things are refused.** Upstream answers a table worth
//!   nothing with "Does this look like a soup kitchen?" (`burn?536`) — and
//!   takes the items anyway, because `OnButtonEat` clears the grid on every
//!   path. The words are a refusal and the code is a loss; we keep the words
//!   and not the loss: [`Refusal::WorthNothing`], nothing moves. A table
//!   worth something to a full group is *not* refused, because there the
//!   source is consistent with itself: it serves, and the group has no room.
//!   [`Standing::wasted`] says so before anything is spent.
//! * **v1.2 games take, and serve anything.** v1.2's `ServiceScene`
//!   (`Scenes/ServiceScene.cs`, which replaced the two scenes) is followed
//!   there instead of the preview: the caller moves each thing out of the
//!   pack onto a real table as it is placed, gives the table back to the
//!   group on EXIT with [`crate::game::move_to_group`], and serves it with
//!   [`serve_table`] — worth nothing or not, empty or not — which clears it.
//!   The legacy game keeps the preview and the refusal.
//! * **The picture follows the door.** `PubScene` chooses `bar.pac` or
//!   `pub1.pac` in its constructor, from whatever `InventoryBackground` held
//!   when the scene object was built, so the choice can stick to the first
//!   pub visited. The `== 14` it tests is plainly meant per door; see
//!   [`Service::background`]. Suspected remaster bug; it changes a picture,
//!   not a rule.

use alloc::vec::Vec;

use crate::game::{self, CharacterNeeds, Companion, ItemBag, Point};
use crate::items::ItemTypes;
use crate::trade::Prices;

/// `grid.Grid = new Vector2(4, 1)`.
pub const TABLE: usize = 4;

/// The pictures behind the two scenes, by `.pac` stem.
pub const BACKGROUNDS: [&str; 4] = ["bar", "pub1", "wirt", "koch"];

/// Which of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    /// `RoomType.Pub`, room ids 14 and 23: a drink.
    Pub,
    /// `RoomType.Restaurant`, room ids 22 and 24: a meal.
    Restaurant,
}

impl Service {
    pub fn of_room(kind: crate::matmap::RoomType) -> Option<Service> {
        match kind {
            crate::matmap::RoomType::Pub => Some(Service::Pub),
            crate::matmap::RoomType::Restaurant => Some(Service::Restaurant),
            _ => None,
        }
    }

    /// The picture for a door with this room id. `bar.pac` for 14 and
    /// `pub1.pac` otherwise; `wirt.pac` for 22 and `koch.pac` otherwise.
    pub fn background(self, room_id: u8) -> &'static str {
        match (self, room_id) {
            (Service::Pub, 14) => "bar",
            (Service::Pub, _) => "pub1",
            (Service::Restaurant, 22) => "wirt",
            (Service::Restaurant, _) => "koch",
        }
    }

    /// The first of the three lines the scene says: `burn?550` for the pub,
    /// `burn?530` for the restaurant. `+3` after serving, `+6` for a table
    /// worth nothing, `+9` for an empty one.
    pub fn text_base(self) -> usize {
        match self {
            Service::Pub => 550,
            Service::Restaurant => 530,
        }
    }
}

/// Why nothing is served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NothingOnTheTable,
    TooMuchOnTheTable,
    /// A slot that holds nothing, or a pack that is not there.
    NoSuchSlot,
    SlotNamedTwice,
    /// A companion fifty or more from the leader: they have no page in the
    /// window, so nothing of theirs can be on the table.
    OutOfReach { companion: usize, distance: i32 },
    /// Worth nothing. See the module docs for why this refuses.
    WorthNothing,
}

/// One member's part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    /// 0 is the leader, then one per companion in group order.
    pub who: usize,
    pub from: i32,
    pub to: i32,
    /// `Some(distance)` when they are too far off to be served at all.
    pub out_of_reach: Option<i32>,
}

/// What the table would do. Shown before anything is spent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    /// What it is worth, and so how many points are served.
    pub worth: i32,
    /// Points somebody took.
    pub taken: i32,
    /// Points nobody near enough had room for. Lost, as upstream loses them.
    pub wasted: i32,
    pub shares: Vec<Share>,
}

/// One pick: which pack — 0 the leader's, then one per companion — and
/// which slot in it.
pub type Pick = (usize, usize);

/// What the things on the table are worth.
pub fn worth(kinds: &[usize], types: &ItemTypes, prices: &Prices) -> i32 {
    crate::trade::value_of(kinds, types, prices)
}

fn distance(a: Point, b: Point) -> i32 {
    let d2 = ((a.x - b.x) as i64).pow(2) + ((a.y - b.y) as i64).pow(2);
    let mut r = 0i64;
    while (r + 1) * (r + 1) <= d2 {
        r += 1;
    }
    r as i32
}

fn level(service: Service, needs: &CharacterNeeds) -> i32 {
    match service {
        Service::Pub => needs.water,
        Service::Restaurant => needs.food,
    }
}

/// Would this be served, and to whom? Changes nothing.
#[allow(clippy::too_many_arguments)]
pub fn check(
    service: Service,
    leader: &CharacterNeeds,
    leader_at: Point,
    carried: &ItemBag,
    companions: &[Companion],
    picks: &[Pick],
    types: &ItemTypes,
    prices: &Prices,
) -> Result<Standing, Refusal> {
    if picks.is_empty() {
        return Err(Refusal::NothingOnTheTable);
    }
    if picks.len() > TABLE {
        return Err(Refusal::TooMuchOnTheTable);
    }
    let mut kinds = Vec::new();
    for (i, &(bag, slot)) in picks.iter().enumerate() {
        if picks[..i].contains(&(bag, slot)) {
            return Err(Refusal::SlotNamedTwice);
        }
        let pack = match bag.checked_sub(1) {
            None => carried,
            Some(n) => {
                let who = companions.get(n).ok_or(Refusal::NoSuchSlot)?;
                if !game::in_group_range(leader_at, who.at) {
                    return Err(Refusal::OutOfReach {
                        companion: bag,
                        distance: distance(leader_at, who.at),
                    });
                }
                &who.pack
            }
        };
        kinds.push(pack.get(slot).ok_or(Refusal::NoSuchSlot)?);
    }

    let worth = worth(&kinds, types, prices);
    if worth <= 0 {
        return Err(Refusal::WorthNothing);
    }

    let mut fed = *leader;
    let mut group: Vec<Companion> = companions.to_vec();
    let left = share(service, &mut fed, leader_at, &mut group, worth);

    let mut shares = Vec::with_capacity(companions.len() + 1);
    shares.push(Share { who: 0, from: level(service, leader), to: level(service, &fed), out_of_reach: None });
    for (n, (was, now)) in companions.iter().zip(&group).enumerate() {
        let out_of_reach = (!game::in_group_range(leader_at, was.at)).then(|| distance(leader_at, was.at));
        shares.push(Share {
            who: n + 1,
            from: level(service, &was.needs),
            to: level(service, &now.needs),
            out_of_reach,
        });
    }
    Ok(Standing { worth, taken: worth - left, wasted: left, shares })
}

fn share(service: Service, leader: &mut CharacterNeeds, leader_at: Point,
         companions: &mut [Companion], worth: i32) -> i32 {
    match service {
        Service::Pub => game::group_drink_near(leader, leader_at, companions, worth),
        Service::Restaurant => game::group_eat_near(leader, leader_at, companions, worth),
    }
}

/// Serve it. **Re-checks first, then everything moves or nothing does.**
///
/// The items go whether or not the group had room — see [`Standing::wasted`].
#[allow(clippy::too_many_arguments)]
pub fn serve(
    service: Service,
    leader: &mut CharacterNeeds,
    leader_at: Point,
    carried: &mut ItemBag,
    companions: &mut [Companion],
    picks: &[Pick],
    types: &ItemTypes,
    prices: &Prices,
) -> Result<Standing, Refusal> {
    let standing = check(service, leader, leader_at, carried, companions, picks, types, prices)?;

    // Highest slot first within a pack, so one removal does not renumber the
    // next.
    let mut order: Vec<Pick> = picks.to_vec();
    order.sort_unstable_by(|a, b| b.cmp(a));
    for (bag, slot) in order {
        match bag.checked_sub(1) {
            None => carried.remove(slot),
            Some(n) => companions[n].pack.remove(slot),
        };
    }
    share(service, leader, leader_at, companions, standing.worth);
    Ok(standing)
}

/// v1.2's `ServiceScene.CommitOffer` for the pub and the restaurant
/// (`Scenes/ServiceScene.cs:224-233`), then `offer.Clear()` (`:318-326`).
///
/// `table` holds what was taken out of the packs as it was placed. Whatever
/// it is worth — nothing included, an empty table included — is served to
/// the group a level at a time and the table is emptied: nothing is
/// refused and nothing comes back. `center` is the selected character, the
/// one the range is measured from (`serviceLeader = SelectedCharacter`,
/// `:137`); `None` is `Group.IgnoreRangeFilter` set, and then nobody is out
/// of reach.
#[allow(clippy::too_many_arguments)]
pub fn serve_table(
    service: Service,
    leader: &mut CharacterNeeds,
    leader_at: Point,
    companions: &mut [Companion],
    center: Option<Point>,
    table: &mut ItemBag,
    types: &ItemTypes,
    prices: &Prices,
) -> Standing {
    let kinds: Vec<usize> = table.iter().collect();
    let worth = worth(&kinds, types, prices).max(0);
    while table.remove(0).is_some() {}

    let far = |at: Point| center.filter(|&c| !game::in_group_range(c, at)).map(|c| distance(c, at));
    let before: Vec<(i32, Option<i32>)> = core::iter::once((level(service, leader), far(leader_at)))
        .chain(companions.iter().map(|c| (level(service, &c.needs), far(c.at))))
        .collect();
    let left = match service {
        Service::Pub => game::group_drink_from(center, leader, leader_at, companions, worth),
        Service::Restaurant => game::group_eat_from(center, leader, leader_at, companions, worth),
    };
    let after = core::iter::once(level(service, leader)).chain(companions.iter().map(|c| level(service, &c.needs)));
    let shares = before
        .iter()
        .zip(after)
        .enumerate()
        .map(|(who, (&(from, out_of_reach), to))| Share { who, from, to, out_of_reach })
        .collect();
    Standing { worth, taken: worth - left, wasted: left, shares }
}
