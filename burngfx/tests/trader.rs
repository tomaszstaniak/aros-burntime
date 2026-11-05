//! A trader's stock, and the two behaviours in their turn.

use burngfx::game::ItemBag;
use burngfx::spawn::Rng;
use burngfx::trader::{rng_for, StockLists, Trader, MAX_STOCK, TRADER_PACK};

const LIST: &str = "\
[trader]
' Mad Marty
179=item_meat item_knife item_rifle item_trap item_pump item_hose item_overall
' Chuck
180=item_meat item_batteries
' Lonely
181=item_meat
";

/// Item kinds, by the order they appear above.
fn kind_of(name: &str) -> Option<usize> {
    [
        "item_meat", "item_knife", "item_rifle", "item_trap", "item_pump", "item_hose",
        "item_overall", "item_batteries",
    ]
    .iter()
    .position(|n| *n == name)
}

fn lists() -> StockLists {
    StockLists::parse(LIST)
}

// -- the stock lists ---------------------------------------------------------

#[test]
fn a_stock_list_is_keyed_by_name_id() {
    let l = lists();
    assert_eq!(l.len(), 3);
    assert_eq!(l.of(179).len(), 7);
    assert_eq!(l.of(180), ["item_meat", "item_batteries"]);
    assert!(l.of(999).is_empty(), "a trader with no entry restocks from nothing");
}

#[test]
fn comments_and_the_section_line_are_not_lists() {
    let l = StockLists::parse("[trader]\n' a name\n\n179=item_meat\n");
    assert_eq!(l.len(), 1);
}

// -- what they start with ----------------------------------------------------

/// `RandomizeInventory`: four to six, never more, never fewer.
///
/// The count comes from `Random.Next(3, 6)` counting down through zero,
/// which is three to five *plus one*. Getting this wrong by one is the
/// easiest mistake in the file and the reason it is asserted over a spread
/// of seeds rather than one.
#[test]
fn a_trader_opens_with_four_to_six_things() {
    let l = lists();
    let mut seen = [false; 7];
    for seed in 0..200u64 {
        let mut t = Trader::new(0, 179, 3);
        t.randomize_inventory(&l, &kind_of, &mut Rng::seeded(seed));
        let n = t.stock.len();
        assert!((4..=6).contains(&n), "seed {seed} gave {n}");
        seen[n] = true;
    }
    assert!(seen[4] && seen[5] && seen[6], "and all three counts do happen");
}

/// **Nothing is stocked twice.** `GetNextItem` only offers kinds the trader
/// is not already holding, so a short list caps the stock rather than
/// repeating anything.
#[test]
fn a_short_list_caps_the_stock_instead_of_repeating() {
    let l = lists();
    let mut t = Trader::new(0, 180, 3); // two kinds to choose from
    t.randomize_inventory(&l, &kind_of, &mut Rng::seeded(7));
    assert_eq!(t.stock.len(), 2);
    let kinds: Vec<usize> = t.stock.iter().collect();
    assert_ne!(kinds[0], kinds[1]);

    let mut alone = Trader::new(0, 181, 3); // one kind
    alone.randomize_inventory(&l, &kind_of, &mut Rng::seeded(7));
    assert_eq!(alone.stock.len(), 1);
}

#[test]
fn a_trader_nobody_listed_opens_empty() {
    let mut t = Trader::new(0, 999, 3);
    t.randomize_inventory(&lists(), &kind_of, &mut Rng::seeded(1));
    assert!(t.stock.is_empty());
}

// -- the churn ---------------------------------------------------------------

/// `RefreshItems` runs **every** turn and keeps the stock inside its range.
#[test]
fn the_stock_churns_every_turn_and_stays_in_range() {
    let l = lists();
    let mut t = Trader::new(0, 179, 3);
    let mut rng = Rng::seeded(99);
    t.randomize_inventory(&l, &kind_of, &mut rng);

    for turn in 0..60 {
        t.refresh_items(&l, &kind_of, &mut rng);
        assert!(t.stock.len() <= MAX_STOCK, "turn {turn} went over the cap");
        assert!(t.stock.capacity() == TRADER_PACK);
        let kinds: Vec<usize> = t.stock.iter().collect();
        for (i, k) in kinds.iter().enumerate() {
            assert!(!kinds[..i].contains(k), "turn {turn} stocked one kind twice");
        }
    }
}

/// It does move things: a stock that never changed would pass the range
/// check above and be wrong.
#[test]
fn the_churn_actually_changes_the_stock() {
    let l = lists();
    let mut t = Trader::new(0, 179, 3);
    let mut rng = Rng::seeded(5);
    t.randomize_inventory(&l, &kind_of, &mut rng);

    let before: Vec<usize> = t.stock.iter().collect();
    let mut ever_differed = false;
    for _ in 0..10 {
        t.refresh_items(&l, &kind_of, &mut rng);
        if t.stock.iter().collect::<Vec<_>>() != before {
            ever_differed = true;
        }
    }
    assert!(ever_differed);
}

/// An empty trader restocks rather than staying empty.
#[test]
fn an_empty_trader_fills_up_again() {
    let l = lists();
    let mut t = Trader::new(0, 179, 3);
    t.stock = ItemBag::new(TRADER_PACK);
    t.refresh_items(&l, &kind_of, &mut Rng::seeded(3));
    assert!(!t.stock.is_empty());
}

// -- the walk ----------------------------------------------------------------

/// **The counter is movement's alone**, and it moves on every second turn.
#[test]
fn a_trader_moves_every_second_turn() {
    let mut t = Trader::new(0, 179, 3);
    let neighbours = [4, 5, 6];
    let mut rng = Rng::seeded(1);

    t.next_sell_location(&neighbours, &mut rng);
    assert_eq!(t.at, 3, "the first turn only counts");
    assert_eq!(t.counter, 1);

    t.next_sell_location(&neighbours, &mut rng);
    assert_ne!(t.at, 3, "the second moves them out");
    assert_eq!(t.counter, 0, "and starts the count again");

    t.next_sell_location(&neighbours, &mut rng);
    let away = t.at;
    t.next_sell_location(&neighbours, &mut rng);
    assert_eq!(t.at, 3, "and the turn after that they go home");
    assert_ne!(away, 3);
}

/// **Two neighbours: only the first is ever visited.** This is the
/// smallest list where the original's `Count - 1` actually discards a real
/// choice, and it is the case that shows the behaviour rather than the
/// guard.
#[test]
fn with_two_neighbours_only_the_first_is_visited() {
    let mut seen = [false; 8];
    for seed in 0..200u64 {
        let mut t = Trader::new(0, 179, 3);
        let mut rng = Rng::seeded(seed);
        t.next_sell_location(&[4, 5], &mut rng);
        t.next_sell_location(&[4, 5], &mut rng);
        seen[t.at] = true;
    }
    assert!(seen[4], "the first neighbour is reachable");
    assert!(!seen[5], "the second never is, because Next(Count - 1) is Next(1)");
    assert!(!seen[3], "and they do leave home");
}

/// **The last neighbour is never chosen.** `Random.Next(Count - 1)` yields
/// `0..Count-2`, which reads as a slip rather than a rule — but it is the
/// behaviour, and a test is where that belongs rather than a comment alone.
#[test]
fn the_last_neighbour_is_never_visited() {
    let neighbours = [4, 5, 6];
    let mut seen = [false; 8];
    for seed in 0..200u64 {
        let mut t = Trader::new(0, 179, 3);
        let mut rng = Rng::seeded(seed);
        t.next_sell_location(&neighbours, &mut rng);
        t.next_sell_location(&neighbours, &mut rng);
        seen[t.at] = true;
    }
    assert!(seen[4] && seen[5], "the first two do come up");
    assert!(!seen[6], "and the last one never does");
}

/// **One neighbour is not a special case.** `System.Random.Next(0)` returns
/// zero, so `Neighbors[Next(Count - 1)]` picks the only neighbour there is.
/// Keeping the trader at home here would be a deviation dressed as a guard,
/// and an earlier version of this did exactly that.
#[test]
fn one_neighbour_is_visited_like_any_other() {
    let mut t = Trader::new(0, 179, 3);
    let mut rng = Rng::seeded(1);
    t.next_sell_location(&[4], &mut rng);
    assert_eq!(t.at, 3, "the first turn only counts");
    t.next_sell_location(&[4], &mut rng);
    assert_eq!(t.at, 4, "the second takes them to the only neighbour");
    t.next_sell_location(&[4], &mut rng);
    t.next_sell_location(&[4], &mut rng);
    assert_eq!(t.at, 3, "and then home again");
}

/// **Ours draws nothing when there is one candidate. Whether the original
/// does is not established.**
///
/// `System.Random.Next(0)` returns zero, which is why the single neighbour
/// is chosen at all — but *returning* zero and *consuming* a draw are
/// separate questions, and the second depends on which implementation a
/// seeded `Random` uses. Not drawing keeps the rest of that trader's day on
/// the numbers it would otherwise have had; drawing would shift the stock
/// churn that follows. So this is a real behavioural difference if the
/// original draws, and it is recorded as ours rather than as fidelity.
///
/// What would settle it: running a seeded `System.Random` and comparing the
/// sequence across a `Next(0)`. Nothing here can.
#[test]
fn one_neighbour_costs_us_no_random_numbers() {
    let mut untouched = Rng::seeded(42);
    let expected: Vec<u64> = {
        let mut r = Rng::seeded(42);
        (0..4).map(|_| r.next_u64()).collect()
    };
    let mut t = Trader::new(0, 179, 3);
    t.next_sell_location(&[4], &mut untouched);
    t.next_sell_location(&[4], &mut untouched);
    assert_eq!(t.at, 4);
    let after: Vec<u64> = (0..4).map(|_| untouched.next_u64()).collect();
    assert_eq!(after, expected, "the generator was not drawn from");
}

/// **An empty list is the one place the original has no answer**:
/// `Next(-1)` throws. Staying home is ours, and a choice rather than a
/// reading — the counter still turns over so nobody is wedged mid-cycle.
#[test]
fn no_neighbours_at_all_is_our_choice() {
    let mut t = Trader::new(0, 179, 3);
    let mut rng = Rng::seeded(1);
    t.next_sell_location(&[], &mut rng);
    assert_eq!(t.counter, 1);
    t.next_sell_location(&[], &mut rng);
    assert_eq!(t.at, 3, "nowhere to go");
    assert_eq!(t.counter, 0, "and the cycle still turned over");
    t.next_sell_location(&[], &mut rng);
    t.next_sell_location(&[], &mut rng);
    assert_eq!(t.at, 3);
}

/// A trader already away from home goes home, however many neighbours
/// there are — that branch draws no random number at all.
#[test]
fn coming_home_needs_no_neighbours() {
    let mut t = Trader::new(0, 179, 3);
    t.at = 9;
    let mut rng = Rng::seeded(1);
    t.next_sell_location(&[], &mut rng);
    t.next_sell_location(&[], &mut rng);
    assert_eq!(t.at, 3, "home is where they return to, with no list to consult");
}

/// A whole turn does both things, in the original's order.
#[test]
fn a_turn_moves_and_churns() {
    let l = lists();
    let mut t = Trader::new(0, 179, 3);
    let mut rng = Rng::seeded(11);
    t.randomize_inventory(&l, &kind_of, &mut rng);

    let where_they_were = t.at;
    t.turn(&l, &kind_of, &[4, 5, 6], &mut rng);
    assert_eq!(t.at, where_they_were, "first turn: counted, not moved");
    t.turn(&l, &kind_of, &[4, 5, 6], &mut rng);
    assert_ne!(t.at, where_they_were, "second turn: moved");
    assert!(!t.stock.is_empty(), "and the stock is still being kept up");
}

// -- reproducibility, which is what a save rests on --------------------------

/// **The same seed gives the same trader**, turn for turn. A save carries
/// the stock and the counter; the generator is rebuilt, so this is what
/// makes a loaded game behave like one that was never interrupted.
#[test]
fn the_same_seed_gives_the_same_trader() {
    let l = lists();
    let run = |seed: u64| {
        let mut t = Trader::new(0, 179, 3);
        let mut rng = Rng::seeded(seed);
        t.randomize_inventory(&l, &kind_of, &mut rng);
        let mut trail = Vec::new();
        for _ in 0..8 {
            t.turn(&l, &kind_of, &[4, 5, 6], &mut rng);
            trail.push((t.at, t.counter, t.stock.iter().collect::<Vec<_>>()));
        }
        trail
    };
    assert_eq!(run(1234), run(1234));
    assert_ne!(run(1234), run(1235), "and a different seed is a different trader");
}

// -- the stream a save rests on ----------------------------------------------

/// **Each trader draws from a stream of their own**, mixed from the seed,
/// the day and their record index. Several traders act on the same day, so
/// they cannot share the production generator's `seed ^ day` — they would
/// all behave identically.
#[test]
fn two_traders_on_one_day_do_not_move_alike() {
    let mut streams = Vec::new();
    for record in 0..6usize {
        let mut rng = rng_for(1993, 4, record);
        streams.push((0..6).map(|_| rng.next_u64()).collect::<Vec<_>>());
    }
    for (i, a) in streams.iter().enumerate() {
        for b in &streams[..i] {
            assert_ne!(a, b, "two traders drew the same numbers on the same day");
        }
    }
}

/// And one trader's days differ from each other.
#[test]
fn one_trader_differs_from_day_to_day() {
    let mut days = Vec::new();
    for day in 1..8u16 {
        let mut rng = rng_for(1993, day, 244);
        days.push((0..6).map(|_| rng.next_u64()).collect::<Vec<_>>());
    }
    for (i, a) in days.iter().enumerate() {
        for b in &days[..i] {
            assert_ne!(a, b);
        }
    }
}

/// **Nothing here depends on which maps were packed.** The identity is the
/// record index in `gam.dat`, which is the same in every profile, and the
/// stream is a pure function of the three numbers. A trader's day is
/// therefore reproducible from a save without carrying a generator
/// position — which is the whole reason for doing it this way.
#[test]
fn the_stream_is_a_function_of_seed_day_and_record() {
    let once: Vec<u64> = {
        let mut r = rng_for(1993, 5, 244);
        (0..8).map(|_| r.next_u64()).collect()
    };
    let again: Vec<u64> = {
        let mut r = rng_for(1993, 5, 244);
        (0..8).map(|_| r.next_u64()).collect()
    };
    assert_eq!(once, again);

    // Changing any one of the three changes it.
    for (seed, day, record) in [(1994u64, 5u16, 244usize), (1993, 6, 244), (1993, 5, 245)] {
        let mut r = rng_for(seed, day, record);
        let other: Vec<u64> = (0..8).map(|_| r.next_u64()).collect();
        assert_ne!(once, other, "({seed}, {day}, {record}) collided with the original");
    }
}

/// A whole day replayed from a save: same stock, same place, same counter.
#[test]
fn a_traders_day_replays_from_the_three_numbers() {
    let l = lists();
    let day_of = |start: &Trader, day: u16| {
        let mut t = start.clone();
        let mut rng = rng_for(1993, day, t.record);
        t.turn(&l, &kind_of, &[4, 5, 6], &mut rng);
        t
    };
    let mut opening = Trader::new(244, 179, 3);
    opening.randomize_inventory(&l, &kind_of, &mut rng_for(1993, 0, 244));

    // Two runs from the same starting state and the same day agree, which
    // is what a load has to reproduce.
    assert_eq!(day_of(&opening, 3), day_of(&opening, 3));
    assert_ne!(day_of(&opening, 3), day_of(&opening, 4));
}

// -- the shipped list --------------------------------------------------------

#[test]
fn the_shipped_lists_read_as_traders() {
    let Ok(text) = std::fs::read_to_string("testdata/trader.txt") else { return };
    let l = StockLists::parse(&text);
    assert!(l.len() >= 8, "the classic data has a trader per city");
    // 179 is the first name id the file uses.
    assert!(!l.of(179).is_empty());
    for name in l.of(179) {
        assert!(name.starts_with("item_"));
    }
}

// -- the world's traders -----------------------------------------------------

fn shipped() -> Option<burngfx::gamdat::Gamdat> {
    let bytes = std::fs::read("testdata/burntime.pak").ok()?;
    let pak = burngfx::pak::ClassicPak::open(&bytes).ok()?;
    burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()
}

/// Every placed trader in the classic world, each at home to start with.
#[test]
fn the_world_has_traders_and_they_start_at_home() {
    let Some(g) = shipped() else { return };
    let t = burngfx::trader::Traders::from_records(&g.characters);
    assert!(t.len() >= 8, "one per city at least, got {}", t.len());
    for trader in t.iter() {
        assert_eq!(trader.at, trader.home);
        assert!(trader.stock.is_empty(), "nothing is stocked until the game opens");
        assert_eq!(trader.counter, 0);
    }
}

/// **The five homeless records are not among them.** `LoadNPCs` does build
/// a trader from each, but with no home area — `NextSellLocation` returns
/// at once for those, so they stand nowhere. Giving them a place would be
/// inventing a rule.
#[test]
fn the_homeless_traders_are_left_out() {
    let Some(g) = shipped() else { return };
    let all = g
        .characters
        .iter()
        .filter(|c| c.class == burngfx::gamdat::CharClass::Trader)
        .count();
    let placed = burngfx::trader::Traders::from_records(&g.characters).len();
    assert_eq!(all - placed, 5, "exactly the five with an out-of-range LocationId");
}

/// Opening for business fills every one of them from their own list.
#[test]
fn opening_stocks_everybody_from_their_own_list() {
    let Some(g) = shipped() else { return };
    let Ok(text) = std::fs::read_to_string("testdata/trader.txt") else { return };
    let lists = StockLists::parse(&text);
    let Ok(items) = std::fs::read_to_string("testdata/items_original.txt") else { return };
    let types = burngfx::items::ItemTypes::parse(&items);
    let kind = |name: &str| types.index_of(name);

    let mut traders = burngfx::trader::Traders::from_records(&g.characters);
    traders.open_for_business(&lists, &kind, 1993);

    let mut stocked = 0;
    for t in traders.iter() {
        if lists.of(t.name_id).is_empty() {
            continue;
        }
        assert!(!t.stock.is_empty(), "trader {} has a list and no stock", t.name_id);
        assert!(t.stock.len() <= 6);
        stocked += 1;
    }
    assert!(stocked >= 8, "most traders have a list; {stocked} were stocked");
}

/// **A day is the same whichever order the traders are visited in**, which
/// is what a stream of their own buys — and what a save then does not have
/// to record.
#[test]
fn the_order_traders_are_visited_in_does_not_matter() {
    let Some(g) = shipped() else { return };
    let Ok(text) = std::fs::read_to_string("testdata/trader.txt") else { return };
    let lists = StockLists::parse(&text);
    let Ok(items) = std::fs::read_to_string("testdata/items_original.txt") else { return };
    let types = burngfx::items::ItemTypes::parse(&items);
    let kind = |name: &str| types.index_of(name);
    let neighbours = |home: usize| -> Vec<usize> {
        g.locations.get(home).map_or(Vec::new(), |l| l.roads.iter().map(|r| r.neighbour).collect())
    };

    let mut forwards = burngfx::trader::Traders::from_records(&g.characters);
    forwards.open_for_business(&lists, &kind, 1993);
    let mut once = forwards.clone();
    // Watched day by day: a trader moves on every second turn, so after an
    // even number of them everybody who went out is home again. Checking
    // only at the end would compare two still worlds and pass.
    let mut ever_away = false;
    for day in 1..5u16 {
        once.turn(&lists, &kind, &neighbours, 1993, day);
        if once.iter().any(|t| t.at != t.home) {
            ever_away = true;
        }
    }

    // The same days again, from the same opening: each trader's own stream
    // means nothing about the loop's order can leak in.
    let mut twice = forwards.clone();
    for day in 1..5u16 {
        twice.turn(&lists, &kind, &neighbours, 1993, day);
    }
    assert_eq!(
        once.iter().map(|t| (t.at, t.counter, t.stock.len())).collect::<Vec<_>>(),
        twice.iter().map(|t| (t.at, t.counter, t.stock.len())).collect::<Vec<_>>()
    );

    // And the traders did move, so this is not comparing two still worlds.
    assert!(ever_away, "somebody went out selling at some point");
    assert!(
        once.iter().all(|t| t.at == t.home),
        "and after four turns — two moves each — everybody is home again"
    );
}

// -- the figure and the shop -------------------------------------------------

/// **A trader is held twice and only one of them moves on its own.**
///
/// `Trader::at` is the authoritative answer; a `Person`'s location follows
/// it. When it did not, the shop walked to the next town while the figure
/// stayed at home, and the trade panel said "no trader here" in the very
/// location the trader was in. This is that bug, as a test.
#[test]
fn a_traders_figure_follows_their_shop() {
    let Some(g) = shipped() else { return };
    let Ok(text) = std::fs::read_to_string("testdata/trader.txt") else { return };
    let lists = StockLists::parse(&text);
    let Ok(items) = std::fs::read_to_string("testdata/items_original.txt") else { return };
    let types = burngfx::items::ItemTypes::parse(&items);
    let kind = |name: &str| types.index_of(name);
    let neighbours = |home: usize| -> Vec<usize> {
        g.locations.get(home).map_or(Vec::new(), |l| l.roads.iter().map(|r| r.neighbour).collect())
    };

    let mut people = burngfx::hire::People::from_records(&g.characters, &types, &settings());
    let mut traders = burngfx::trader::Traders::from_records(&g.characters);
    traders.open_for_business(&lists, &kind, 1993);

    let agrees = |people: &burngfx::hire::People, traders: &burngfx::trader::Traders| {
        traders.iter().all(|t| {
            (0..people.len())
                .filter_map(|i| people.get(i))
                .find(|p| p.record == t.record)
                .is_some_and(|p| p.location == t.at)
        })
    };

    assert!(agrees(&people, &traders), "they start in the same place");

    let mut ever_moved = false;
    for day in 1..8u16 {
        traders.turn(&lists, &kind, &neighbours, 1993, day);
        // Before following, the two disagree the moment anybody moves —
        // which is what makes the call below load-bearing rather than
        // decorative.
        if traders.iter().any(|t| t.at != t.home) {
            ever_moved = true;
        }
        assert_eq!(people.follow_traders(&traders), 0, "every figure was found");
        assert!(agrees(&people, &traders), "day {day}: a shop left its body behind");
    }
    assert!(ever_moved, "somebody went out selling, so this was worth checking");
}

/// And after a load: rebuilding the people from `gam.dat` puts every figure
/// back at its record's location, so a trader saved away from home would be
/// left behind by exactly the same gap.
#[test]
fn the_figures_follow_after_a_rebuild_too() {
    let Some(g) = shipped() else { return };
    let Ok(items) = std::fs::read_to_string("testdata/items_original.txt") else { return };
    let types = burngfx::items::ItemTypes::parse(&items);

    let mut people = burngfx::hire::People::from_records(&g.characters, &types, &settings());
    let mut traders = burngfx::trader::Traders::from_records(&g.characters);

    // A trader saved away from home, the way a mid-game save has them.
    let record = traders.iter().next().map(|t| t.record).unwrap_or(0);
    let away = {
        let t = traders.get_mut(0).expect("at least one trader");
        t.at = 9;
        t.record
    };
    assert_eq!(away, record);

    let before = (0..people.len())
        .filter_map(|i| people.get(i))
        .find(|p| p.record == record)
        .map(|p| p.location);
    assert_ne!(before, Some(9), "the rebuilt figure is at its record's home");

    assert_eq!(people.follow_traders(&traders), 0);
    let after = (0..people.len())
        .filter_map(|i| people.get(i))
        .find(|p| p.record == record)
        .map(|p| p.location);
    assert_eq!(after, Some(9), "and now it is where the shop is");
}

/// The class numbers a fight needs, as the classic file gives them at
/// difficulty 0.
fn settings() -> burngfx::settings::GameSettings {
    burngfx::settings::GameSettings::parse(
        "[0]\ndog_health = 31\ndog_attack = 30\ndog_respawn = 30\n\
         mutant_health = 31\nmutant_attack = 40\nmutant_respawn = 30\n\
         trader_health = 100\ntrader_attack = 60\ntrader_respawn = 20\n\
         npc_respawn = 10\n",
        0,
    )
}
