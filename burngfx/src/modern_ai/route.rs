//! `RouteFinder`: the cheapest road to a place, by days and, for trade, by
//! exposure. At tag `v1.2`, `AI/PlayerBehavior/Travel/RouteFinder.cs`.
//!
//! Dijkstra over the locations' ways, ordered by (effective cost, days,
//! the order a place was reached): equal costs come out in the order the
//! roads were looked at, so a route never depends on a hash. A hostile camp
//! (not a city, owned by another) is never crossed, only entered as the
//! target; standing in one, the only road out is back to where the party
//! came from.

use alloc::collections::BinaryHeap;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Reverse;

/// What the router needs of the world.
pub trait RouteMap {
    /// Places in the world.
    fn locations(&self) -> usize;
    /// `Neighbors` with `WayLengths`, in their order.
    fn ways(&self, location: usize) -> Vec<(usize, i32)>;
    fn is_city(&self, location: usize) -> bool;
    fn owner(&self, location: usize) -> Option<usize>;
}

/// `IsHostile`.
pub fn is_hostile(map: &impl RouteMap, location: usize, player: usize) -> bool {
    !map.is_city(location) && map.owner(location).is_some_and(|o| o != player)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub next_step: usize,
    pub days: i32,
    /// The places after the start, ending at the target.
    pub path: Vec<usize>,
    pub effective_cost: i32,
}

/// `Find`: by days alone. `previous` is `player.PreviousLocation`.
pub fn find(map: &impl RouteMap, player: usize, previous: Option<usize>, start: usize, target: usize) -> Option<Route> {
    find_with(map, player, previous, start, target, |_| 0)
}

/// `FindSupportedTradeRoute`: ten days more for each place on the way that
/// is not the player's own, so a detour through friendly land wins over an
/// exposed corridor of the same length.
pub fn find_supported_trade_route(
    map: &impl RouteMap,
    player: usize,
    previous: Option<usize>,
    start: usize,
    target: usize,
) -> Option<Route> {
    find_with(map, player, previous, start, target, |l| {
        if l != target && map.owner(l) != Some(player) { 10 } else { 0 }
    })
}

pub fn find_with(
    map: &impl RouteMap,
    player: usize,
    previous: Option<usize>,
    start: usize,
    target: usize,
    penalty: impl Fn(usize) -> i32,
) -> Option<Route> {
    if start == target {
        return Some(Route { next_step: target, days: 0, path: Vec::new(), effective_cost: 0 });
    }
    let n = map.locations().max(start + 1).max(target + 1);
    let retreat_only = is_hostile(map, start, player);
    let retreat = if retreat_only { previous } else { None };
    let mut distance: Vec<Option<i32>> = vec![None; n];
    let mut cost: Vec<Option<i32>> = vec![None; n];
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut visited = vec![false; n];
    let mut frontier = BinaryHeap::new();
    let mut sequence = 0u32;
    distance[start] = Some(0);
    cost[start] = Some(0);
    frontier.push(Reverse((0i32, 0i32, sequence, start)));
    sequence += 1;

    while let Some(Reverse((c, d, _, current))) = frontier.pop() {
        if visited[current] || Some(d) != distance[current] || Some(c) != cost[current] {
            continue;
        }
        if current == target {
            break;
        }
        visited[current] = true;
        for (neighbour, length) in map.ways(current) {
            if neighbour >= n {
                continue;
            }
            let forced = current == start && retreat_only && Some(neighbour) == retreat;
            if current == start && retreat_only && !forced {
                continue;
            }
            let hostile = is_hostile(map, neighbour, player);
            if length <= 0 || hostile && neighbour != target && !forced {
                continue;
            }
            // Back into a second hostile camp is a legal move, but no road
            // goes on from there.
            if forced && hostile && neighbour != target {
                continue;
            }
            let (Some(dc), Some(cc)) = (distance[current], cost[current]) else { continue };
            let candidate = dc + length;
            let candidate_cost = cc + length + penalty(neighbour);
            let better = match cost[neighbour] {
                None => true,
                Some(known) => candidate_cost < known || candidate_cost == known && Some(candidate) < distance[neighbour].or(Some(i32::MAX)),
            };
            if better {
                distance[neighbour] = Some(candidate);
                cost[neighbour] = Some(candidate_cost);
                parent[neighbour] = Some(current);
                frontier.push(Reverse((candidate_cost, candidate, sequence, neighbour)));
                sequence += 1;
            }
        }
    }

    let days = distance[target]?;
    parent[target]?;
    let mut path = vec![target];
    let mut step = target;
    while let Some(p) = parent[step] {
        if p == start {
            break;
        }
        step = p;
        path.push(step);
    }
    path.reverse();
    Some(Route { next_step: path[0], days, path, effective_cost: cost[target].unwrap_or(days) })
}
