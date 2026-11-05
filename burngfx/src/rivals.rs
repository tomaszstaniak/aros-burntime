//! Winning, losing, and holding ground against somebody else.
//!
//! Ported from v0.9.1: `VictoryCondition`, `Character.JoinCamp` /
//! `LeaveCamp` / `Dismiss`, `Player.CanTravel` and the player loop in
//! `World.Turn`. The opponent's *behaviour* is not here — see
//! [`crate::rival`] — because that release's opponent does not play by these
//! rules, and this module is only the rules.
//!
//! # The victory condition, word for word
//!
//! ```csharp
//! for (int i = 0; i < world.World.Locations.Count; i++) {
//!     Location l = world.World.Locations[i];
//!     bool neighborOfCity = false;
//!     for (int j = 0; j < l.Neighbors.Count; j++)
//!         if (l.Neighbors[j].IsCity) { neighborOfCity = true; break; }
//!     if (neighborOfCity)
//!         if (l.Player != player) return false;
//! }
//! return true;
//! ```
//!
//! **The quantifier is the whole rule, so read it twice.** The loop is over
//! *every* location in the world, and the test is "has at least one city
//! among its neighbours". So what must be held is the union of the
//! neighbourhoods of **all** cities — not the ring around one city, and not
//! one location per city. [`holdings_wanted`] returns that set and
//! `the_ring_is_around_every_city_not_one` pins the difference.
//!
//! Three more things fall out of it, each easy to get wrong:
//!
//! * **A city is a location too.** Nothing here excludes one, so a city that
//!   neighbours another city must itself be held. `OnMenuMakeCamp` has no
//!   city check either — only the opponent refuses to camp in cities — so
//!   that is reachable, not theoretical.
//! * **A location with no city next to it never matters**, however large.
//! * **Holding is all that counts.** Nobody has to die, and no city has to be
//!   entered.
//!
//! # Holding, taking, losing
//!
//! * `JoinCamp` sets `Location.Player`; that assignment *is* ownership.
//! * `LeaveCamp` and `Dismiss` clear it **only for the last one out** —
//!   `IsLastInCamp`.
//! * Taking somebody else's place is therefore not an action of its own: you
//!   remove their last camper, by whatever means the world already has, and
//!   then leave one of yours. [`Takeover`] says what that costs.
//! * `Dismiss` calls `Location.ClearItemsAfterTakeover()` in exactly one
//!   case — the last camper of a camp that is **not the active player's** —
//!   so the emptying belongs to that event and to no other. A camp given up
//!   by its own owner keeps what is in its rooms.
//! * `Player.CanTravel`: standing in a location somebody else holds, the only
//!   road out is back the way you came. See [`can_travel`].
//! * `World.Turn`: a player whose character is dead loses every camper and
//!   every companion, and is dead for good. See [`Defeat`].

use alloc::vec::Vec;

/// What the world is, as far as these rules are concerned: which locations
/// are cities, and which roads lead where.
///
/// A trait rather than `gamdat`, so the rules can be tested on a world of
/// four locations written out by hand.
pub trait Map {
    fn count(&self) -> usize;
    fn is_city(&self, location: usize) -> bool;
    /// Everywhere a road leads from here. Roads are two-way in the 1993
    /// save, but nothing here assumes it.
    fn neighbours(&self, location: usize) -> Vec<usize>;
}

/// Every location that must be held to win: those with at least one city
/// among their neighbours, cities included.
pub fn holdings_wanted(map: &impl Map) -> Vec<usize> {
    let mut wanted = Vec::new();
    for location in 0..map.count() {
        if map.neighbours(location).iter().any(|&n| map.is_city(n)) {
            wanted.push(location);
        }
    }
    wanted
}

/// Has this player won? `VictoryCondition.Process`, with `owner_of` standing
/// in for `Location.Player`.
///
/// **An empty world is a win**, exactly as upstream: the loop finds nothing
/// to fail on and returns true. That is not a case the game reaches, and it
/// is left as it is rather than quietly guarded — see
/// `a_world_with_no_cities_is_won_by_everybody`.
pub fn has_won(map: &impl Map, player: usize, owner_of: impl Fn(usize) -> Option<usize>) -> bool {
    holdings_wanted(map).into_iter().all(|l| owner_of(l) == Some(player))
}

/// Who has won, asked over every player the way `ClassicGame.CheckWinner`
/// asks it: the first in order wins a tie.
pub fn winner(
    map: &impl Map,
    players: &[usize],
    owner_of: impl Fn(usize) -> Option<usize> + Copy,
) -> Option<usize> {
    players.iter().copied().find(|&p| has_won(map, p, owner_of))
}

/// The same, with a scenario's shortened goal: only the first `goals` of the
/// locations the rule wants are counted.
///
/// **This is a scenario, not a rule.** The condition itself is
/// [`holdings_wanted`] and is not touched; a short game is one where fewer of
/// those places are asked for, so that a demonstration can be played to the
/// end without thirty days of walking. `None` is the real game. The order is
/// the world's own — location index — so a shortened goal is the same set
/// every time it is played.
pub fn winner_within(
    map: &impl Map,
    goals: Option<usize>,
    players: &[usize],
    owner_of: impl Fn(usize) -> Option<usize> + Copy,
) -> Option<usize> {
    let wanted = holdings_wanted(map);
    let wanted = match goals {
        Some(n) => &wanted[..n.min(wanted.len())],
        None => &wanted[..],
    };
    // An empty goal is nobody's win rather than everybody's: a scenario that
    // asks for nothing is a scenario that was not set up, and announcing a
    // victory for it would be worse than saying nothing.
    if wanted.is_empty() {
        return None;
    }
    players
        .iter()
        .copied()
        .find(|&p| wanted.iter().all(|&l| owner_of(l) == Some(p)))
}

/// **What the player is allowed to be told.**
///
/// `StatisticsScene.Update` is the source's own answer, and it counts only
/// the active player: how many cities the world has, how many of them they
/// control, how many locations they hold, and who is with them. There is no
/// line about anybody else — and `MapScene.OnClickEntrance` in info mode
/// refuses to open a place that is not the player's ("only show if current
/// location or owned by player"). So a progress display that named the
/// opponent's holdings would be telling the player something the game they
/// are playing does not tell them.
///
/// **And the screen does not count the way the victory does.** Read them
/// side by side:
///
/// ```csharp
/// // VictoryCondition.Process: every location with a city among its neighbours
/// if (neighborOfCity) if (l.Player != player) return false;
///
/// // StatisticsScene.Update: a city is "controlled" when its non-city
/// // neighbours are yours
/// foreach (Location n in l.Neighbors)
///     if (!n.IsCity && n.Player != world.ActivePlayerObj) { controlled = false; break; }
/// ```
///
/// The screen skips city neighbours and the win check does not, so **every
/// city can read as controlled while the game is not won** — wherever two
/// cities are neighbours. That is the source's own inconsistency and it is
/// left as it is: the display is a report, the condition is the rule. See
/// `the_screen_counts_differently_from_the_condition`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Standing {
    /// Every city in the world.
    pub cities: usize,
    /// Those whose non-city neighbours are all this player's.
    pub controlled: usize,
    /// Locations this player holds.
    pub held: usize,
    /// Of the places the victory condition actually wants, how many are
    /// this player's. **Not in the original's statistics**, which has no
    /// such line; it is the honest progress figure and is reported as what
    /// it is rather than dressed up as the game's own.
    pub wanted_held: usize,
    pub wanted: usize,
}

/// The player's own standing, counted the way `StatisticsScene` counts it,
/// with the condition's own figure alongside.
pub fn standing_of(
    map: &impl Map,
    player: usize,
    owner_of: impl Fn(usize) -> Option<usize> + Copy,
) -> Standing {
    let mut out = Standing::default();
    for location in 0..map.count() {
        if owner_of(location) == Some(player) {
            out.held += 1;
        }
        if !map.is_city(location) {
            continue;
        }
        out.cities += 1;
        let controlled = map
            .neighbours(location)
            .into_iter()
            .filter(|&n| !map.is_city(n))
            .all(|n| owner_of(n) == Some(player));
        if controlled {
            out.controlled += 1;
        }
    }
    let wanted = holdings_wanted(map);
    out.wanted = wanted.len();
    out.wanted_held = wanted.into_iter().filter(|&l| owner_of(l) == Some(player)).count();
    out
}

/// **How far along a scenario's shortened goal is**, as `(held, asked for)`.
///
/// Reported on a line of its own and never folded into the victory
/// condition's own count: the condition is "every location beside a city",
/// sixteen of them in the classic world, and a demonstration that asks for
/// two of them is a scenario. A display that wrote the scenario's number
/// under the words "to win" would be telling the player a different rule
/// from the one the game plays by — and, because the two counts are taken
/// over different sets, could say "3 of 2".
pub fn scenario_progress(
    map: &impl Map,
    goals: usize,
    player: usize,
    owner_of: impl Fn(usize) -> Option<usize> + Copy,
) -> (usize, usize) {
    let wanted = holdings_wanted(map);
    let asked = &wanted[..goals.min(wanted.len())];
    (asked.iter().filter(|&&l| owner_of(l) == Some(player)).count(), asked.len())
}

/// How a game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// This player holds every location that touches a city.
    Won(usize),
    /// This player's own character died: `World.Turn` dismisses everything
    /// they had and marks them dead.
    Lost(usize),
}

/// What a player loses the moment their character dies — `World.Turn`, in
/// its order: every camper of theirs is dismissed, then every companion.
///
/// The dismissals are the world's existing ones, so this only says **what**
/// must happen; the caller does it with the machinery it already has.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Defeat {
    /// Locations whose camps are theirs, and so are emptied.
    pub camps: Vec<usize>,
    /// How many were walking with them.
    pub companions: usize,
}

/// Work out what a dead player's defeat costs, without doing any of it.
///
/// `owned` is what that player holds — the caller has the world and can say;
/// asking for it rather than a whole map keeps this a rule and not a search.
pub fn defeat(owned: &[usize], companions: usize) -> Defeat {
    Defeat { camps: owned.to_vec(), companions }
}

/// **Can this road be taken?** `Player.CanTravel`: from a location held by
/// somebody else, the only permitted destination is the previous one.
///
/// `here` is who holds the location being left, not where the player is
/// going: leaving is what is restricted, and arriving is not.
pub fn can_travel(
    here: Option<usize>,
    player: usize,
    previous: Option<usize>,
    destination: usize,
) -> bool {
    match here {
        Some(owner) if owner != player => previous == Some(destination),
        _ => true,
    }
}

/// **`Location.ControllingPlayer`** (v1.2): a city every one of whose
/// neighbours the same player holds is controlled by him; no city, a city
/// with no neighbours, and one with a neighbour free or held by somebody else
/// are controlled by nobody. The world map draws a controlled city's name in
/// its controller's colour.
pub fn controlling_player(is_city: bool, neighbours: &[usize], owner: impl Fn(usize) -> Option<usize>) -> Option<usize> {
    let (&first, rest) = neighbours.split_first().filter(|_| is_city)?;
    let controller = owner(first)?;
    rest.iter().all(|&n| owner(n) == Some(controller)).then_some(controller)
}

/// **May this player leave somebody camped here?**
///
/// `OnMenuMakeCamp`: with the location already somebody else's, the menu does
/// not camp at all — it opens a conversation, `ConversationType.Capture`. So
/// a held place is not taken by walking in and pitching a tent beside its
/// owner; their camper has to go first, and the only thing in this game that
/// makes a camper go is a fight.
///
/// Camping where you already hold is allowed — that is how a camp grows.
pub fn may_camp(camp_owner: Option<usize>, player: usize) -> bool {
    match camp_owner {
        None => true,
        Some(owner) => owner == player,
    }
}

/// **`Location.ClearItemsAfterTakeover`: what is put back** in a place whose
/// last camper was taken from another player, once every room is emptied.
///
/// The source's list, tier by tier — `difficulty` is the start screen's (0
/// easy, 1 normal, 2 hard), and each lower level adds to the ones above it:
///
/// * every level: one food; a fifth of a chance of something `rare` or
///   `special` that is not `nodrop`; half a chance of a `bottle`; "nought to
///   two" `material` or `useless` things;
/// * normal and easy: half a chance of a `trap`; another fifth of a chance
///   of `rare`/`special`, `nodrop` allowed; "nought to two" more;
/// * easy: "one to two" food; half a chance of a `trap`; a fifth of a chance
///   of `rare`/`special`/`protection_parts`; "one to two" `material` or
///   `useless`.
///
/// **`GenerateClass`'s own slips are kept**, because they decide what a
/// takeover yields:
///
/// * the counted form returns nothing at all when `min` is zero
///   (`if (min <= 0 || max <= 0) return Array.Empty`) — so every "nought to
///   two" is none, at every level;
/// * the count is `Random.Next(min, max)`, whose top is exclusive — so "one
///   to two" is always one;
/// * the pick is `Random.Next(types.Length - 1)` — the last matching type is
///   never drawn.
///
/// The numbers come from `rng`, the port's own stream, not `System.Random`;
/// see the randomness row of the departures.
pub fn takeover_restock(
    types: &crate::items::ItemTypes,
    difficulty: u16,
    rng: &mut crate::spawn::Rng,
) -> Vec<usize> {
    let filter = |include: &[&str], exclude: &[&str]| crate::settings::Filter {
        include: include.iter().map(|s| (*s).into()).collect(),
        exclude: exclude.iter().map(|s| (*s).into()).collect(),
    };
    let pick = |kinds: &[usize], rng: &mut crate::spawn::Rng| -> usize {
        kinds[rng.below(kinds.len().saturating_sub(1) as u32) as usize]
    };
    // `GenerateClass(include, exclude, chance)`.
    let one = |include: &[&str], exclude: &[&str], percent: u32, rng: &mut crate::spawn::Rng| {
        let kinds = crate::spawn::candidates(types, &filter(include, exclude));
        if kinds.is_empty() {
            return None;
        }
        // `Random.NextSingle() <= chance`.
        if rng.below(100) >= percent {
            return None;
        }
        Some(pick(&kinds, rng))
    };
    // `GenerateClass(include, exclude, min, max)`.
    let some = |include: &[&str], exclude: &[&str], min: u32, max: u32, rng: &mut crate::spawn::Rng| {
        if min == 0 || max == 0 {
            return Vec::new();
        }
        let kinds = crate::spawn::candidates(types, &filter(include, exclude));
        if kinds.is_empty() {
            return Vec::new();
        }
        let count = min + rng.below(max.saturating_sub(min));
        (0..count).map(|_| pick(&kinds, rng)).collect::<Vec<_>>()
    };
    let mut out = Vec::new();
    if difficulty <= 2 {
        out.extend(one(&["food"], &[], 100, rng));
        out.extend(one(&["rare", "special"], &["nodrop"], 20, rng));
        out.extend(one(&["bottle"], &[], 50, rng));
        out.extend(some(&["material", "useless"], &[], 0, 2, rng));
    }
    if difficulty <= 1 {
        out.extend(one(&["trap"], &[], 50, rng));
        out.extend(one(&["rare", "special"], &[], 20, rng));
        out.extend(some(&["material", "useless"], &[], 0, 2, rng));
    }
    if difficulty == 0 {
        out.extend(some(&["food"], &[], 1, 2, rng));
        out.extend(one(&["trap"], &[], 50, rng));
        out.extend(one(&["rare", "special", "protection_parts"], &[], 20, rng));
        out.extend(some(&["material", "useless"], &[], 1, 2, rng));
    }
    out
}

/// **Does losing this camper empty the place, and does it count as a
/// takeover?**
///
/// `Dismiss`, in its own order: if this was the last one camped there, and
/// the camp was not the active player's, the rooms are cleared; either way
/// the location becomes nobody's.
///
/// `by` is who is doing it — `World.ActivePlayerObj` in the original. A camp
/// given up by its own owner is not a takeover and keeps what it holds.
pub fn last_camper_leaves(camp_owner: Option<usize>, by: usize) -> (bool, bool) {
    let lost = camp_owner.is_some();
    let cleared = matches!(camp_owner, Some(owner) if owner != by);
    (lost, cleared)
}
