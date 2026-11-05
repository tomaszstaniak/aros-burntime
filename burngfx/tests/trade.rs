//! Barter with a trader.

use burngfx::game::{ItemBag, PACK_CAPACITY};
use burngfx::items::ItemTypes;
use burngfx::trade::{check, exchange, value_of, Prices, Refusal};

/// Four items, with the original's own numbers for the three that have
/// them: a snake is stored as 21 and worth 21/4, meat 27/4, a knife 45/4.
/// The rag is worth nothing at all, which is a case in its own right.
fn types() -> ItemTypes {
    ItemTypes::parse(
        "[item_snake]\nclass=food\nburngfx=2\nfood=7\n\
         [item_meat]\nclass=food\nburngfx=3\nfood=4\n\
         [item_knife]\nclass=tool\nburngfx=10\n\
         [item_scrap]\nclass=junk\nburngfx=31\n\
         [item_rag]\nclass=junk\nburngfx=99\n",
    )
}

fn prices() -> Prices {
    // The classic table's own numbers: a snake is stored as 21, meat 27, a
    // knife 45. Scrap at 3 is the smallest value the table holds, and it is
    // here because the interesting boundary needs a three-quarter step.
    Prices::parse("' sprite = value\n2 = 21\n3 = 27\n10 = 45\n31 = 3\n")
}

fn kind(types: &ItemTypes, name: &str) -> usize {
    types.index_of(name).unwrap_or_else(|| panic!("{name} is in the test table"))
}

// -- what things are worth ---------------------------------------------------

/// **The sum is taken in fractions and truncated once.** Two snakes at 5.25
/// are ten, not five and five. Rounding each item first would change every
/// price in the game.
#[test]
fn value_is_summed_before_it_is_truncated() {
    let (t, p) = (types(), prices());
    let snake = kind(&t, "item_snake");

    assert_eq!(value_of(&[snake], &t, &p), 5, "21/4 is 5.25");
    assert_eq!(value_of(&[snake, snake], &t, &p), 10, "and two are 10.5, not 5 + 5");
    assert_eq!(value_of(&[snake, snake, snake, snake], &t, &p), 21, "four make the whole 21");
}

#[test]
fn an_item_with_no_price_is_worth_nothing() {
    let (t, p) = (types(), prices());
    assert_eq!(value_of(&[kind(&t, "item_rag")], &t, &p), 0);
    assert_eq!(value_of(&[], &t, &p), 0);
}

/// The prices are data, not a field on an item: an empty table makes
/// everything worthless rather than failing.
#[test]
fn no_price_table_at_all() {
    let t = types();
    let p = Prices::default();
    assert!(p.is_empty());
    assert_eq!(value_of(&[kind(&t, "item_snake")], &t, &p), 0);
}

// -- the deal ----------------------------------------------------------------

struct Deal {
    types: ItemTypes,
    prices: Prices,
    mine: ItemBag,
    theirs: ItemBag,
}

fn a_deal(mine: &[&str], theirs: &[&str]) -> Deal {
    let types = types();
    let to_kinds = |names: &[&str]| -> Vec<usize> {
        names.iter().map(|n| kind(&types, n)).collect()
    };
    let mine = ItemBag::with(PACK_CAPACITY, &to_kinds(mine));
    let theirs = ItemBag::with(7, &to_kinds(theirs));
    Deal { types, prices: prices(), mine, theirs }
}

impl Deal {
    fn check(&self, from_me: &[usize], from_them: &[usize]) -> Result<i32, Refusal> {
        let picks: Vec<(usize, usize)> = from_me.iter().map(|&s| (0, s)).collect();
        check(&[&self.mine], &self.theirs, &picks, from_them, &self.types, &self.prices)
            .map(|s| s.offered)
    }

    fn do_it(&mut self, from_me: &[usize], from_them: &[usize]) -> Result<i32, Refusal> {
        let picks: Vec<(usize, usize)> = from_me.iter().map(|&s| (0, s)).collect();
        let mut mine = core::mem::replace(&mut self.mine, ItemBag::new(0));
        let out = {
            let mut packs = [&mut mine];
            exchange(
                &mut packs,
                &mut self.theirs,
                &picks,
                from_them,
                &self.types,
                &self.prices,
            )
        };
        self.mine = mine;
        out.map(|s| s.offered)
    }

    fn holding(&self) -> Vec<&str> {
        self.mine.iter().filter_map(|k| self.types.name_of(k)).collect()
    }

    fn they_hold(&self) -> Vec<&str> {
        self.theirs.iter().filter_map(|k| self.types.name_of(k)).collect()
    }
}

/// A knife is 11.25 and a snake 5.25, so two snakes do not buy one.
#[test]
fn the_price_has_to_be_met() {
    let deal = a_deal(&["item_snake", "item_snake"], &["item_knife"]);
    assert_eq!(
        deal.check(&[0, 1], &[0]),
        Err(Refusal::NotEnough { offered: 10, asked: 11 })
    );
}

/// Three do. **And there is no change** — `>=`, not `==`.
#[test]
fn overpaying_is_allowed_and_buys_nothing_back() {
    let mut deal = a_deal(&["item_snake", "item_snake", "item_snake"], &["item_knife"]);
    assert_eq!(deal.do_it(&[0, 1, 2], &[0]), Ok(15));
    assert_eq!(deal.holding(), ["item_knife"], "all three snakes went");
    assert_eq!(deal.they_hold(), ["item_snake", "item_snake", "item_snake"]);
}

/// **You cannot take for nothing**, even what the trader is not asking for.
#[test]
fn a_gift_still_has_to_be_paid_for() {
    let deal = a_deal(&["item_snake"], &["item_rag"]);
    assert_eq!(deal.check(&[], &[0]), Err(Refusal::YouOfferNothing));
    // The rag is worth nothing, so a single snake more than covers it —
    // once something is actually offered.
    assert_eq!(deal.check(&[0], &[0]), Ok(5));
}

/// Nor can you give something away for nothing: the player's side must
/// still be non-empty, but the trader's may be.
#[test]
fn giving_something_away_is_a_deal() {
    let mut deal = a_deal(&["item_snake"], &[]);
    assert_eq!(deal.do_it(&[0], &[]), Ok(5));
    assert!(deal.holding().is_empty());
    assert_eq!(deal.they_hold(), ["item_snake"]);
}

// -- room --------------------------------------------------------------------

/// **Room is checked on the net flow.** A full pack can still take three
/// things if it is giving three away.
#[test]
fn a_full_pack_can_still_trade_evenly() {
    let mut deal = a_deal(
        &["item_snake", "item_snake", "item_snake", "item_knife", "item_knife", "item_knife"],
        &["item_meat", "item_meat", "item_meat"],
    );
    assert_eq!(deal.mine.len(), PACK_CAPACITY, "the pack is full to start with");
    assert_eq!(deal.do_it(&[3, 4, 5], &[0, 1, 2]), Ok(33), "three knives for three meats");
    assert_eq!(deal.holding().iter().filter(|n| **n == "item_meat").count(), 3);
}

/// But taking more than it gives needs the difference in free slots.
#[test]
fn taking_more_than_you_give_needs_the_room() {
    // Two knives are 22, three meats 20, so the price is met and only the
    // room is in the way: two out, three in, and a pack with nothing spare.
    let mut deal = a_deal(
        &["item_snake", "item_snake", "item_snake", "item_snake", "item_knife", "item_knife"],
        &["item_meat", "item_meat", "item_meat"],
    );
    assert_eq!(deal.mine.len(), PACK_CAPACITY);
    assert_eq!(
        deal.do_it(&[4, 5], &[0, 1, 2]),
        Err(Refusal::NoRoomInYourPacks),
        "two out, three in, and no free slot"
    );
    assert_eq!(deal.holding().len(), PACK_CAPACITY, "and nothing moved");
    assert_eq!(deal.they_hold().len(), 3);
}

/// The same the other way: the trader's seven slots are checked too.
#[test]
fn the_trader_needs_room_as_well() {
    let mut deal = a_deal(
        &["item_snake", "item_snake", "item_snake"],
        &["item_meat", "item_meat", "item_meat", "item_meat", "item_meat", "item_meat",
          "item_meat"],
    );
    assert_eq!(deal.theirs.len(), 7, "their stock is full");
    assert_eq!(
        deal.do_it(&[0, 1, 2], &[0]),
        Err(Refusal::NoRoomForTheTrader),
        "three out, one in, and they have no free slot"
    );
    assert_eq!(deal.holding().len(), 3, "and nothing moved");
}

// -- the exchange itself -----------------------------------------------------

/// **A refused deal moves nothing.**
#[test]
fn a_refused_deal_costs_nothing() {
    let mut deal = a_deal(&["item_snake"], &["item_knife"]);
    let before: (Vec<String>, Vec<String>) = (
        deal.holding().iter().map(|s| s.to_string()).collect(),
        deal.they_hold().iter().map(|s| s.to_string()).collect(),
    );
    assert!(deal.do_it(&[0], &[0]).is_err());
    let after: (Vec<String>, Vec<String>) = (
        deal.holding().iter().map(|s| s.to_string()).collect(),
        deal.they_hold().iter().map(|s| s.to_string()).collect(),
    );
    assert_eq!(after, before);
}

/// Asking costs nothing however often it is asked.
#[test]
fn checking_changes_nothing() {
    // A snake is 5.25 and meat 6.75, so it takes the knife: 11.25 covers it.
    let deal = a_deal(&["item_snake", "item_knife"], &["item_meat"]);
    for _ in 0..3 {
        deal.check(&[1], &[0]).expect("a knife covers meat");
    }
    assert_eq!(deal.holding(), ["item_snake", "item_knife"]);
    assert_eq!(deal.they_hold(), ["item_meat"]);
}

/// A slot that holds nothing is refused rather than silently skipped.
#[test]
fn a_slot_with_nothing_in_it() {
    let deal = a_deal(&["item_snake"], &["item_meat"]);
    assert_eq!(deal.check(&[7], &[0]), Err(Refusal::NoSuchSlot));
    assert_eq!(deal.check(&[0], &[7]), Err(Refusal::NoSuchSlot));
}

/// **Several slots at once, and the higher ones do not shift the lower.**
/// Removing by index is where an exchange most easily takes the wrong item.
#[test]
fn picking_several_slots_takes_the_ones_named() {
    let mut deal = a_deal(
        &["item_snake", "item_meat", "item_knife", "item_rag"],
        &["item_meat"],
    );
    // Slots 0 and 2: the snake and the knife. The meat and the rag stay.
    assert_eq!(deal.do_it(&[0, 2], &[0]), Ok(16));
    assert_eq!(deal.holding(), ["item_meat", "item_rag", "item_meat"]);
    assert_eq!(deal.they_hold(), ["item_knife", "item_snake"]);
}

// -- the whole group ---------------------------------------------------------

/// **The exchange reaches every pack in the group**, not the leader's
/// alone: `foreach (Character chr in ActiveCharacter.GetGroup())`.
#[test]
fn a_companion_can_pay_and_be_paid() {
    let types = types();
    let prices = prices();
    let (snake, knife) = (kind(&types, "item_snake"), kind(&types, "item_knife"));

    let mut leader = ItemBag::with(PACK_CAPACITY, &[snake, snake, snake, snake, snake, snake]);
    let mut friend = ItemBag::with(PACK_CAPACITY, &[snake, snake, snake]);
    let mut theirs = ItemBag::with(7, &[knife]);

    // The leader's pack is full, so what comes back has to reach the
    // companion — and what pays for it comes out of the companion's pack.
    let out = {
        let mut packs = [&mut leader, &mut friend];
        exchange(&mut packs, &mut theirs, &[(1, 0), (1, 1), (1, 2)], &[0], &types, &prices)
    };
    assert_eq!(out.map(|s| s.offered), Ok(15));
    assert_eq!(leader.len(), PACK_CAPACITY, "the leader's pack is untouched and still full");
    assert_eq!(friend.iter().collect::<Vec<_>>(), [knife], "the knife went where there was room");
    assert_eq!(theirs.len(), 3, "and the three snakes are theirs");
}

/// Free slots are counted across the group, so a full leader with an empty
/// companion still has room.
#[test]
fn room_is_counted_across_the_group() {
    let types = types();
    let prices = prices();
    let (knife, meat) = (kind(&types, "item_knife"), kind(&types, "item_meat"));

    let mut leader = ItemBag::with(PACK_CAPACITY, &[knife; 6]);
    let mut friend = ItemBag::new(PACK_CAPACITY);
    let mut theirs = ItemBag::with(7, &[meat, meat]);

    // Two knives are 22 and two meats 13, so this is about room alone.
    let out = {
        let mut packs = [&mut leader, &mut friend];
        exchange(&mut packs, &mut theirs, &[(0, 0), (0, 1)], &[0, 1], &types, &prices)
    };
    assert_eq!(out.map(|s| s.offered), Ok(22));
    assert_eq!(friend.len() + leader.len(), 6, "two out, two in");
}

// -- against the shipped data ------------------------------------------------

/// `trader.txt` names each trader's stock by their `NameId`. Nothing reads
/// it yet; this only says what shape it is, so the day it is packed the
/// parser is already known to match.
#[test]
fn a_traders_stock_list_reads_as_names() {
    let Ok(text) = std::fs::read_to_string("testdata/trader.txt") else { return };
    let mut lists = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('\'') || line.starts_with('[') {
            continue;
        }
        let Some((id, items)) = line.split_once('=') else { continue };
        assert!(id.trim().parse::<u16>().is_ok(), "keyed by NameId: {id}");
        assert!(items.split_whitespace().all(|i| i.starts_with("item_")));
        lists += 1;
    }
    assert!(lists > 0);
}

/// **The same slot named twice must not pay twice.** One item is one item;
/// counting it once for value and removing two different things afterwards
/// would create an item out of nothing.
#[test]
fn one_item_cannot_pay_twice() {
    let mut deal = a_deal(&["item_knife", "item_snake"], &["item_meat", "item_meat"]);
    // Slot 0 twice: a single knife pretending to be two.
    let out = deal.do_it(&[0, 0], &[0, 1]);
    assert!(out.is_err(), "naming a slot twice is refused, not counted twice");
    assert_eq!(deal.holding(), ["item_knife", "item_snake"], "and nothing moved");
    assert_eq!(deal.they_hold(), ["item_meat", "item_meat"]);
}

/// The same on the trader's side.
#[test]
fn one_of_their_items_cannot_be_taken_twice() {
    let mut deal = a_deal(&["item_knife", "item_knife"], &["item_meat"]);
    let out = deal.do_it(&[0, 1], &[0, 0]);
    assert!(out.is_err());
    assert_eq!(deal.they_hold(), ["item_meat"]);
    assert_eq!(deal.holding(), ["item_knife", "item_knife"]);
}

// -- the edge of the deal ----------------------------------------------------

/// **Truncation is what decides it, and here is the case that proves it.**
///
/// Every classic price is a multiple of three quarters, so two sides can
/// differ by 0.75 and still land in the same whole number. A snake and meat
/// are 12.00 exactly; a snake, meat and scrap are 12.75. Truncated, both
/// are 12 — so `player >= trader` holds and **the deal stands although the
/// player is offering less than they are taking**. Comparing the fractions
/// would refuse it.
#[test]
fn truncation_is_what_decides_the_deal() {
    let deal = a_deal(
        &["item_snake", "item_meat", "item_knife"],
        &["item_snake", "item_meat", "item_scrap"],
    );
    // 12.00 offered against 12.75 asked, and it stands.
    assert_eq!(deal.check(&[0, 1], &[0, 1, 2]), Ok(12));
}

/// And the step below it is refused, so the rule is not simply permissive:
/// a knife is 11.25 against the same 12.75, which is 11 against 12.
#[test]
fn the_step_below_that_boundary_is_refused() {
    let deal = a_deal(
        &["item_snake", "item_meat", "item_knife"],
        &["item_snake", "item_meat", "item_scrap"],
    );
    assert_eq!(
        deal.check(&[2], &[0, 1, 2]),
        Err(Refusal::NotEnough { offered: 11, asked: 12 })
    );
}

/// Equal is enough. A knife is 11.25 and so is a knife.
#[test]
fn paying_exactly_is_a_deal() {
    let deal = a_deal(&["item_knife"], &["item_knife"]);
    assert_eq!(deal.check(&[0], &[0]), Ok(11));
}

/// A margin smaller than a whole unit is still a margin: four snakes are
/// 21.00 against three meats at 20.25, and that one would stand with or
/// without truncation. It is here as the ordinary case beside the two
/// above, not as evidence of anything about rounding.
#[test]
fn a_margin_smaller_than_a_unit_is_still_a_margin() {
    let deal = a_deal(
        &["item_snake", "item_snake", "item_snake", "item_snake"],
        &["item_meat", "item_meat", "item_meat"],
    );
    assert_eq!(deal.check(&[0, 1, 2, 3], &[0, 1, 2]), Ok(21), "21.0 against 20.25");
}

/// **Why `f32` is safe here, and what that rests on.**
///
/// Dividing by four gives quarters, and a quarter is exact in binary, so
/// no sum of these prices drifts. Checked against the whole classic table —
/// all 58 entries, every one a multiple of three, ranging 3 to 195, so the
/// prices run 0.75 to 48.75 and the entire table summed is 760.5. Far
/// inside the range where `f32` counts quarters exactly.
///
/// **It is a property of the data, not of the arithmetic.** A table built
/// any other way could give a price that is not a quarter, and then the
/// sums would start to drift. That is the thing to re-check if the numbers
/// ever come from somewhere else, and it is why they are an input.
#[test]
fn the_classic_prices_are_exact_quarters() {
    let p = prices();
    for stored in [21, 27, 45] {
        let quarters = stored as f32 / 4.0;
        assert_eq!(quarters * 4.0, stored as f32, "{stored}/4 is exact");
    }
    // And a sum of them is exact too, up to a whole trader's stock.
    let mut total = 0.0f32;
    for _ in 0..7 {
        total += p.of(Some(10));
    }
    assert_eq!(total, 45.0 * 7.0 / 4.0);
    assert_eq!(total as i32, 78, "78.75 truncates to 78");
}

/// **A trader may end up holding two of a kind through barter.**
///
/// `GetNextItem` refuses to *stock* a kind the trader already has, but that
/// is about generating a stock list — it says nothing about what they will
/// take in a deal. Tidying the two rules into one would quietly refuse
/// perfectly ordinary trades, so this pins them apart.
#[test]
fn a_trader_takes_a_second_of_what_they_already_hold() {
    let mut deal = a_deal(&["item_knife", "item_knife"], &["item_knife"]);
    // Two knives for their one: they end up with three of the same kind.
    assert_eq!(deal.do_it(&[0, 1], &[0]), Ok(22));
    assert_eq!(deal.they_hold(), ["item_knife", "item_knife"]);
    assert_eq!(deal.holding(), ["item_knife"]);
}

/// And so may the player, from the other direction.
#[test]
fn the_player_takes_a_second_of_what_they_already_hold() {
    let mut deal = a_deal(&["item_knife", "item_meat"], &["item_meat"]);
    assert_eq!(deal.do_it(&[0], &[0]), Ok(11));
    assert_eq!(deal.holding(), ["item_meat", "item_meat"]);
}

// -- the converted table -----------------------------------------------------

/// The packer converts the remaster's `ConstValues.cs` into a price file.
/// This checks the conversion against numbers read out of that source by
/// hand, so a change to the parsing shows up as a wrong price rather than
/// as a subtly cheaper world.
#[test]
fn the_converted_prices_match_the_table() {
    let Ok(bytes) = std::fs::read("testdata/burntime.pack") else { return };
    let Ok(pack) = burngfx::pack::Pack::open(&bytes) else { return };
    let Some(raw) = pack.get("rules/prices") else { return };
    let prices = Prices::parse(core::str::from_utf8(raw).unwrap_or(""));

    // Sprite id, stored value, and what a single one is worth. Read out of
    // ConstValues.cs: `9,//2,4,6,9,maggots}` is id 0.
    for (sprite, stored) in [(0u8, 9), (2, 21), (3, 27), (10, 45), (17, 150)] {
        assert_eq!(
            prices.of(Some(sprite)) * 4.0,
            stored as f32,
            "sprite {sprite} should be stored as {stored}"
        );
    }
    // A trap at 150 is the dearest thing a camp can be given, and worth
    // 37.5 — seven snakes and change.
    assert_eq!(prices.of(Some(17)), 37.5);
    assert!(!prices.is_empty());
}

/// And the whole table is quarters, which is what makes the `f32` sums
/// exact — checked against the real file rather than the test fixture.
#[test]
fn every_shipped_price_is_a_quarter() {
    let Ok(bytes) = std::fs::read("testdata/burntime.pack") else { return };
    let Ok(pack) = burngfx::pack::Pack::open(&bytes) else { return };
    let Some(raw) = pack.get("rules/prices") else { return };
    let prices = Prices::parse(core::str::from_utf8(raw).unwrap_or(""));

    let mut priced = 0;
    for sprite in 0..=57u8 {
        let v = prices.of(Some(sprite));
        assert_eq!(v * 4.0 % 1.0, 0.0, "sprite {sprite} is not a quarter");
        if v > 0.0 {
            priced += 1;
        }
    }
    assert_eq!(priced, 58, "every entry in the table carries a price");
}

// -- what the table says about worth -----------------------------------------

/// **An item's own `value=` line wins over the sprite's price**, and an item
/// with no sprite has nothing else. `ItemTypesData` reads the `ConstValues`
/// number first and then overwrites it from the section when the line is
/// there — for `TradeValue`, `EatValue` and `DrinkValue` alike.
#[test]
fn an_items_own_value_line_is_its_price() {
    let types = ItemTypes::parse(
        // A sprite and a value: the line wins. 24/4 is 6, the sprite's 45/4
        // would be 11.
        // The dish, the advice and item_nothing have no sprite at all, which
        // is how the items Extended added are written.
        "[item_marked_knife]\nclass=tool\nburngfx=10\nvalue=24\n\
         [item_dish]\nclass=useless\nimage=gfx/dish.png\nvalue=24\n\
         [item_advice]\nclass=nodrop\nimage=gfx/advice.png\nvalue=1\n\
         [item_nothing]\nclass=useless\nimage=gfx/none.png\n",
    );
    let p = prices();
    let k = |n| kind(&types, n);

    assert_eq!(burngfx::trade::worth_of(k("item_marked_knife"), &types, &p), 6.0,
               "the line, not the sprite");
    assert_eq!(burngfx::trade::worth_of(k("item_dish"), &types, &p), 6.0);
    // A quarter each: four pieces of advice are worth one, and one is worth
    // nothing — the same truncation as everywhere else.
    assert_eq!(value_of(&[k("item_advice")], &types, &p), 0);
    assert_eq!(value_of(&[k("item_advice"); 4], &types, &p), 1);
    assert_eq!(value_of(&[k("item_nothing")], &types, &p), 0,
               "no sprite and no value line is worth nothing");
}

/// And the price a trader asks is that same number: an Extended item with a
/// `value=` line buys something, where before it bought nothing at all.
#[test]
fn an_item_priced_by_its_own_line_can_be_traded() {
    let types = ItemTypes::parse(
        "[item_snake]\nclass=food\nburngfx=2\nfood=7\n\
         [item_dish]\nclass=useless\nimage=gfx/dish.png\nvalue=24\n",
    );
    let p = prices();
    let (dish, snake) = (kind(&types, "item_dish"), kind(&types, "item_snake"));

    let mut mine = ItemBag::with(PACK_CAPACITY, &[dish]);
    let mut theirs = ItemBag::with(PACK_CAPACITY, &[snake]);

    // The dish is worth 6, the snake 5: enough, and it would not have been
    // when an item without a sprite was worth nothing.
    let paid = {
        let mut packs = [&mut mine];
        exchange(&mut packs, &mut theirs, &[(0, 0)], &[0], &types, &p).expect("6 covers 5")
    };
    assert_eq!((paid.offered, paid.asked), (6, 5));
    assert_eq!(mine.iter().collect::<Vec<_>>(), [snake]);
    assert_eq!(theirs.iter().collect::<Vec<_>>(), [dish]);
}

// -- v1.2: what Accept leaves where -------------------------------------------

/// **Accept moves things in the order they were clicked.** v1.2's
/// `OnButtonAccept` (`Scenes/TraderScene.cs:298-306`) removes the offers
/// from the packs and the stock, then `MoveItems(exchangeTop.Grid)` and
/// `MoveItems(exchangeBottom.Grid)`: each grid is a list in click order
/// (`GUI/ItemGridWindow.cs:494-499`), and `ItemList.Move` takes its first
/// item each time (`Logic/Item/ItemList.cs:54-64`). So what is bought lands
/// in the packs, and what is sold in the stock, in click order — not in
/// slot order.
#[test]
fn v12_accept_moves_things_in_click_order() {
    use burngfx::trade::{exchange_with, Barter};
    let (t, p) = (types(), prices());
    let (snake, meat, knife, scrap) =
        (kind(&t, "item_snake"), kind(&t, "item_meat"), kind(&t, "item_knife"), kind(&t, "item_scrap"));
    let mut leader = ItemBag::with(PACK_CAPACITY, &[snake, meat, knife]);
    let mut stock = ItemBag::with(12, &[scrap, snake, scrap]);
    // The snake clicked before the knife; the trader's snake before his
    // second scrap. Slot order the other way round would be the same; the
    // highest slot first is not.
    let mine = [(0, 0), (0, 2)];
    let theirs = [1, 2];
    exchange_with(&mut [&mut leader], &mut stock, &mine, &theirs, &t, &p, Barter::V12 { factor: 100 })
        .expect("a snake and a knife for a snake and a scrap");
    assert_eq!(leader.iter().collect::<Vec<_>>(), vec![meat, snake, scrap], "bought, in click order");
    assert_eq!(stock.iter().collect::<Vec<_>>(), vec![scrap, snake, knife], "sold, in click order");
}

/// **What changes hands keeps its state.** v1.2's Accept moves the `Item`
/// objects themselves (`TraderScene.cs:298-306`: `Items.Remove`, then
/// `GetGroup().MoveItems(grid)`), and an item's rounds are its own
/// (`Item.ammo`, `Logic/Item/Item.cs:103-137`): a half-fired rifle sold is
/// half-fired in the stock, and bought back is half-fired again.
#[test]
fn v12_accept_carries_each_items_rounds() {
    use burngfx::trade::{exchange_with, Barter};
    let (t, p) = (types(), prices());
    let (snake, meat, knife, scrap) =
        (kind(&t, "item_snake"), kind(&t, "item_meat"), kind(&t, "item_knife"), kind(&t, "item_scrap"));
    let mut leader = ItemBag::with(PACK_CAPACITY, &[snake, knife]);
    leader.set_rounds_left(1, Some(1));
    let mut stock = ItemBag::with(12, &[meat, scrap]);
    stock.set_rounds_left(0, Some(3));
    exchange_with(&mut [&mut leader], &mut stock, &[(0, 1)], &[0], &t, &p, Barter::V12 { factor: 100 })
        .expect("a knife for meat");
    assert_eq!(leader.iter().collect::<Vec<_>>(), vec![snake, meat]);
    assert_eq!(leader.rounds_left(1), Some(3), "bought with its rounds");
    assert_eq!(stock.iter().collect::<Vec<_>>(), vec![scrap, knife]);
    assert_eq!(stock.rounds_left(1), Some(1), "sold with its rounds");
    assert_eq!(leader.rounds_left(0), None, "the snake untouched");
}

/// The legacy game keeps the order it has always had (highest slot first);
/// only a v1.2 game changes.
#[test]
fn legacy_accept_keeps_its_order() {
    let (t, p) = (types(), prices());
    let (snake, meat, knife, scrap) =
        (kind(&t, "item_snake"), kind(&t, "item_meat"), kind(&t, "item_knife"), kind(&t, "item_scrap"));
    let mut leader = ItemBag::with(PACK_CAPACITY, &[snake, meat, knife]);
    let mut stock = ItemBag::with(12, &[scrap, snake, scrap]);
    exchange(&mut [&mut leader], &mut stock, &[(0, 0), (0, 2)], &[1, 2], &t, &p).expect("the same deal");
    assert_eq!(leader.iter().collect::<Vec<_>>(), vec![meat, scrap, snake]);
    assert_eq!(stock.iter().collect::<Vec<_>>(), vec![scrap, knife, snake]);
}

/// **Reuse against recompute (audit A6).** v1.2's Accept trusts the light
/// computed at the last click (`TraderScene.cs:292-296`, `:559-602`); ours
/// works the verdict out again. With nothing changing in between — no world
/// update runs in the scene — the two cannot differ: for every table below,
/// what `check_with` said is what `exchange_with` does.
#[test]
fn the_verdict_at_accept_is_the_verdict_shown() {
    use burngfx::trade::{check_with, exchange_with, Barter};
    let (t, p) = (types(), prices());
    let all = [kind(&t, "item_snake"), kind(&t, "item_meat"), kind(&t, "item_knife"), kind(&t, "item_scrap"),
               kind(&t, "item_rag")];
    let mut seed = 12345u32;
    let mut next = |n: u32| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (seed >> 16) % n
    };
    for _ in 0..500 {
        let mine_n = next(PACK_CAPACITY as u32 + 1) as usize;
        let stock_n = next(13) as usize;
        let packed: Vec<usize> = (0..mine_n).map(|_| all[next(5) as usize]).collect();
        let stocked: Vec<usize> = (0..stock_n).map(|_| all[next(5) as usize]).collect();
        let mut picks = Vec::new();
        for s in 0..mine_n {
            if next(2) == 0 {
                picks.push((0, s));
            }
        }
        let mut wanted = Vec::new();
        for s in 0..stock_n {
            if next(2) == 0 {
                wanted.push(s);
            }
        }
        for barter in [Barter::Legacy, Barter::V12 { factor: 100 }, Barter::V12 { factor: 80 }] {
            let mut pack = ItemBag::with(PACK_CAPACITY, &packed);
            let mut stock = ItemBag::with(12, &stocked);
            let shown = check_with(&[&pack], &stock, &picks, &wanted, &t, &p, barter);
            let done = exchange_with(&mut [&mut pack], &mut stock, &picks, &wanted, &t, &p, barter);
            assert_eq!(shown, done);
        }
    }
}
