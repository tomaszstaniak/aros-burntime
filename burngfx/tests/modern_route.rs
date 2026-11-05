//! `modern_ai::route` and `modern_ai::supplies` (v1.2's `RouteFinder` and
//! `TravelSupplies`) on small maps made for each case.

use burngfx::modern_ai::route::*;
use burngfx::modern_ai::supplies::*;

/// Places 0..n, two-way roads, cities, owners.
struct Map {
    roads: Vec<(usize, usize, i32)>,
    n: usize,
    cities: Vec<usize>,
    owners: Vec<(usize, usize)>,
}

impl RouteMap for Map {
    fn locations(&self) -> usize {
        self.n
    }
    fn ways(&self, l: usize) -> Vec<(usize, i32)> {
        self.roads
            .iter()
            .filter_map(|&(a, b, d)| if a == l { Some((b, d)) } else if b == l { Some((a, d)) } else { None })
            .collect()
    }
    fn is_city(&self, l: usize) -> bool {
        self.cities.contains(&l)
    }
    fn owner(&self, l: usize) -> Option<usize> {
        self.owners.iter().find(|(x, _)| *x == l).map(|(_, o)| *o)
    }
}

const ME: usize = 0;
const THEM: usize = 1;

#[test]
fn the_fewest_days_and_its_first_step() {
    // 0-1-2-3 at 1 day each, and 0-3 directly at 4.
    let m = Map { n: 4, roads: vec![(0, 1, 1), (1, 2, 1), (2, 3, 1), (0, 3, 4)], cities: vec![], owners: vec![] };
    let r = find(&m, ME, None, 0, 3).unwrap();
    assert_eq!((r.next_step, r.days, r.path.clone()), (1, 3, vec![1, 2, 3]));
    assert_eq!(find(&m, ME, None, 2, 2).unwrap().days, 0, "already there");
}

#[test]
fn equal_costs_keep_the_order_of_the_roads() {
    // Two two-day roads to 3: through 1 and through 2, listed in that order.
    let m = Map { n: 4, roads: vec![(0, 1, 1), (0, 2, 1), (1, 3, 1), (2, 3, 1)], cities: vec![], owners: vec![] };
    for _ in 0..3 {
        assert_eq!(find(&m, ME, None, 0, 3).unwrap().path, vec![1, 3]);
    }
    let swapped = Map { roads: vec![(0, 2, 1), (0, 1, 1), (2, 3, 1), (1, 3, 1)], ..m };
    assert_eq!(find(&swapped, ME, None, 0, 3).unwrap().path, vec![2, 3], "the first road looked at");
}

#[test]
fn hostile_camps_are_entered_never_crossed() {
    // 0-1-2, with 1 another's camp: 2 cannot be reached; 1 itself can.
    let m = Map { n: 3, roads: vec![(0, 1, 1), (1, 2, 1)], cities: vec![], owners: vec![(1, THEM)] };
    assert_eq!(find(&m, ME, None, 0, 2), None, "no road");
    assert_eq!(find(&m, ME, None, 0, 1).unwrap().days, 1, "the target may be hostile");
    // A city never is, whoever holds it; one's own camp neither.
    let city = Map { cities: vec![1], ..Map { n: 3, roads: m.roads.clone(), cities: vec![], owners: vec![(1, THEM)] } };
    assert_eq!(find(&city, ME, None, 0, 2).unwrap().days, 2);
    let mine = Map { n: 3, roads: m.roads.clone(), cities: vec![], owners: vec![(1, ME)] };
    assert_eq!(find(&mine, ME, None, 0, 2).unwrap().days, 2);
}

#[test]
fn standing_in_a_hostile_camp_the_only_road_is_back() {
    // 1 is theirs; we came from 0; 2 is free; 3 is theirs too.
    let m = Map { n: 4, roads: vec![(0, 1, 1), (1, 2, 1), (1, 3, 1)], cities: vec![], owners: vec![(1, THEM), (3, THEM)] };
    let back = find(&m, ME, Some(0), 1, 0).unwrap();
    assert_eq!((back.next_step, back.days), (0, 1));
    assert_eq!(find(&m, ME, Some(0), 1, 2), None, "not forward, though it is a day away");
    assert_eq!(find(&m, ME, None, 1, 0), None, "nowhere to retreat to");
    // Back into another hostile camp: legal as the target, never onward.
    let m2 = Map { n: 4, roads: vec![(1, 3, 1), (3, 2, 1)], cities: vec![], owners: vec![(1, THEM), (3, THEM)] };
    assert_eq!(find(&m2, ME, Some(3), 1, 3).unwrap().days, 1);
    assert_eq!(find(&m2, ME, Some(3), 1, 2), None);
}

#[test]
fn trade_routes_pay_ten_for_exposure() {
    // To city 4: 0-1-4 is two days through free 1; 0-2-3-4 is three through
    // our own 2 and 3. The plain router takes the short one; the trade
    // router the long one (2 + 10 > 3).
    let m = Map {
        n: 5,
        roads: vec![(0, 1, 1), (1, 4, 1), (0, 2, 1), (2, 3, 1), (3, 4, 1)],
        cities: vec![4],
        owners: vec![(2, ME), (3, ME)],
    };
    assert_eq!(find(&m, ME, None, 0, 4).unwrap().path, vec![1, 4]);
    let t = find_supported_trade_route(&m, ME, None, 0, 4).unwrap();
    assert_eq!((t.path.clone(), t.days, t.effective_cost), (vec![2, 3, 4], 3, 3));
    // Once the detour is worse than ten days, exposure is paid.
    let far = Map { roads: vec![(0, 1, 1), (1, 4, 1), (0, 2, 5), (2, 3, 5), (3, 4, 5)], ..m };
    let t = find_supported_trade_route(&far, ME, None, 0, 4).unwrap();
    assert_eq!((t.path, t.days, t.effective_cost), (vec![1, 4], 2, 12));
}

fn group(food: &[i32], water: &[i32], food_carried: i32, water_carried: i32) -> GroupSupplies {
    GroupSupplies {
        food: food.to_vec(),
        water: water.to_vec(),
        max_food: vec![9; food.len()],
        max_water: vec![5; food.len()],
        health: vec![100; food.len()],
        food_carried,
        water_carried,
    }
}

fn days(d: i32) -> Route {
    Route { next_step: 1, days: d, path: vec![1], effective_cost: d }
}

#[test]
fn supplies_count_the_whole_group() {
    // The leader has plenty; the follower has one day: three days are not
    // made unless what is carried makes up the follower's two.
    assert!(!has_route_supplies(&group(&[9, 1], &[5, 5], 1, 0), &days(3), false, 0, 0));
    assert!(has_route_supplies(&group(&[9, 1], &[5, 5], 2, 0), &days(3), false, 0, 0));
    assert!(!has_route_supplies(&group(&[9, 1], &[5, 5], 2, 0), &days(3), false, 1, 0), "a payment set aside is not food");
    // An attack wants a day less of food, and as much water again.
    assert!(!has_route_supplies(&group(&[5, 5], &[5, 5], 0, 0), &days(3), true, 0, 0));
    assert!(has_route_supplies(&group(&[5, 5], &[6, 6], 0, 0), &days(3), true, 0, 0));
    assert_eq!(lowest_after_distribution(&[3, 1, 1], 3), 2);
    let mut r = [3, 1, 1];
    distribute_to_lowest(&mut r, 3);
    assert_eq!(r, [3, 3, 2], "one at a time to the first of the lowest");
}

#[test]
fn recovery_roads_that_would_kill_are_refused() {
    let mut g = group(&[1, 3], &[0, 3], 0, 0);
    assert_eq!(expected_recovery_route_damage(&g, &days(2)), 25 + 50);
    assert!(can_survive_recovery_route(&g, &days(2)));
    g.health = vec![100, 75];
    assert!(!can_survive_recovery_route(&g, &days(2)));
    // Thirsty: going is no worse than staying when staying adds nothing.
    assert!(dehydration_travel_no_worse_than_waiting(&g, &days(2), 0));
    assert!(!dehydration_travel_no_worse_than_waiting(&g, &days(2), 4));
}

#[test]
fn a_recruit_counts_in_the_group() {
    let g = group(&[4], &[4], 0, 0);
    assert!(has_projected_recruit_route_supplies(&g, &days(3), 3, 3, 0, 0));
    assert!(!has_projected_recruit_route_supplies(&g, &days(3), 2, 3, 0, 0), "the recruit's own two days");
    // Alone to a city of minimum 3, a recruit with 2, two days on and two back.
    let g = group(&[2], &[2], 4, 4);
    // The city tops the leader up to 3; two days on leaves 1, and 4 carried:
    // five days back at most.
    assert!(has_staged_recruit_settlement_supplies(&g, &days(2), &days(2), &days(5), 3, 2, 2, 0, 0));
    assert!(!has_staged_recruit_settlement_supplies(&g, &days(2), &days(2), &days(6), 3, 2, 2, 0, 0));
    assert!(!has_staged_recruit_settlement_supplies(&group(&[2, 2], &[2, 2], 4, 4), &days(2), &days(2), &days(2), 3, 2, 2, 0, 0), "alone only");
    assert!(has_staged_recruit_city_return_supplies(&group(&[2, 2], &[2, 2], 0, 0), &days(2), &days(3), 3, 3, 3, 0, 0));
    assert!(!has_staged_recruit_city_return_supplies(&group(&[2, 1], &[2, 2], 0, 0), &days(2), &days(3), 3, 3, 3, 0, 0));
}

/// The real world's roads (`gam.dat`): every route found is a chain of
/// roads, its days are their sum, and nothing shorter exists.
#[test]
fn the_worlds_roads() {
    let bytes = std::fs::read("testdata/burntime.pak").expect("testdata pack");
    let pak = burngfx::pak::ClassicPak::open(&bytes).expect("pak");
    let gam = burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat").expect("gam.dat")).expect("parse");
    struct World<'a>(&'a burngfx::gamdat::Gamdat);
    impl RouteMap for World<'_> {
        fn locations(&self) -> usize {
            self.0.locations.len()
        }
        fn ways(&self, l: usize) -> Vec<(usize, i32)> {
            self.0.locations[l].roads.iter().map(|r| (r.neighbour, r.days as i32)).collect()
        }
        fn is_city(&self, l: usize) -> bool {
            self.0.locations[l].is_city
        }
        fn owner(&self, _: usize) -> Option<usize> {
            None
        }
    }
    let w = World(&gam);
    let n = gam.locations.len();
    // Floyd–Warshall as the reference.
    let mut best = vec![vec![i32::MAX / 4; n]; n];
    for a in 0..n {
        best[a][a] = 0;
        for (b, d) in w.ways(a) {
            best[a][b] = best[a][b].min(d);
        }
    }
    for k in 0..n {
        for a in 0..n {
            for b in 0..n {
                best[a][b] = best[a][b].min(best[a][k] + best[k][b]);
            }
        }
    }
    for a in 0..n {
        for b in 0..n {
            let Some(r) = find(&w, 0, None, a, b) else {
                assert!(best[a][b] >= i32::MAX / 4, "{a}->{b} exists");
                continue;
            };
            assert_eq!(r.days, best[a][b], "{a}->{b}");
            let mut at = a;
            let mut sum = 0;
            for &step in &r.path {
                sum += burngfx::game::road_days(&gam, at, step).expect("a road") as i32;
                at = step;
            }
            assert_eq!((at, sum), (b, r.days));
        }
    }
}
