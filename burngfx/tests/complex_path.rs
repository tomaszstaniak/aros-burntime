//! `burngfx::complex_path`: v1.2's `ComplexPath` (`PathFinding/ComplexPath.cs`)
//! on walk grids drawn as text (`#` a closed cell, `.` an open one; a cell is
//! 8 pixels) and on a real location's mask.

use burngfx::complex_path::{ComplexPath, STEP_SECONDS};
use burngfx::gait::{self, Stride, Walked};
use burngfx::game::{self, Point};
use burngfx::zei::WalkGrid;

fn grid(rows: &[&str]) -> WalkGrid {
    let (w, h) = (rows[0].len(), rows.len());
    let mut bytes = vec![w as u8, 0, h as u8, 0];
    for r in rows {
        bytes.extend(r.bytes().map(|b| (b == b'.') as u8));
    }
    WalkGrid::from_packed(&bytes).unwrap()
}

/// A cell's centre.
fn centre(cx: i32, cy: i32) -> Point {
    Point::new(cx * 8 + 4, cy * 8 + 4)
}

/// `ToGridPoint`: `(p + 3) / 8`.
fn cell_of(p: Point) -> (i32, i32) {
    ((p.x + 3) / 8, (p.y + 3) / 8)
}

const OPEN_8: &[&str] = &["........"; 8];

/// Walk at fifty pixels a second until he stops; the cells he passes, each
/// once, in order.
fn walk(g: &WalkGrid, from: Point, to: Point, limit: usize) -> (Point, Vec<(i32, i32)>) {
    let mut path = ComplexPath::standing(from);
    path.set_move_to(to);
    let mut stride = Stride::default();
    let mut at = from;
    let mut cells = vec![cell_of(at)];
    for _ in 0..limit {
        let (now, how) = path.process(g, at, &mut stride, gait::CONTROLLED, STEP_SECONDS);
        at = now;
        if cells.last() != Some(&cell_of(at)) {
            cells.push(cell_of(at));
        }
        if how == Walked::Arrived {
            return (at, cells);
        }
    }
    panic!("no arrival in {limit} steps; at {at:?}, cells {cells:?}");
}

/// The first steps of a search on an open 8 × 8 mask from cell (2, 2) to
/// cell (6, 6), worked by hand from `ComplexPath.cs`:
///
/// * 50 cycles a second over steps of 1/18 s: 2, then 3 expansions
///   (`CycleCounter.Process`, `:127-135`; `RES/game/classic/settings.txt:11`).
/// * Step 1. The start is taken (`:291-296`); the open list, empty, is
///   sorted; its eight neighbours are appended in `Rect`'s order (`:342-376`),
///   each cost 1 — `Vector2.Length` is an `int` — and Manhattan to go:
///   (1,1) 11, (2,1) 10, (3,1) 9, (1,2) 10, (3,2) 8, (1,3) 9, (2,3) 8,
///   (3,3) 7. The second expansion takes `openList[0]`, **(1,1)**, not the
///   best — the list was sorted before they were added (`:334-337`). It is
///   no nearer (10 > 8 to go): no waypoint, and he stands. Its new
///   neighbours all score 12 or more, the cap for an 8-cell mask
///   (`(int)(8 * 1.5f)`, `:228,368`), and are never listed.
/// * Step 2. The sort left (3,3) first: taken, 6 to go < 8, so the route is
///   [(3,3)] (`:298-313`). Then (3,2), no nearer. Then, after a sort that
///   now holds (3,3)'s neighbours, (4,4) at score 6: the route is
///   [(3,3), (4,4)], and he walks 2 7/9 pixels toward (3,3)'s centre.
#[test]
fn the_search_runs_a_few_nodes_a_step_and_goes_diagonally() {
    let g = grid(OPEN_8);
    let mut path = ComplexPath::standing(centre(2, 2));
    path.set_move_to(centre(6, 6));
    let mut stride = Stride::default();
    let budget = gait::CONTROLLED;

    let (at, how) = path.process(&g, centre(2, 2), &mut stride, budget, STEP_SECONDS);
    assert_eq!((at, how), (centre(2, 2), Walked::Going), "step 1: no route yet, he stands");
    assert!(path.way_points().is_empty());
    assert!(!path.finished());

    let (at, _) = path.process(&g, at, &mut stride, budget, STEP_SECONDS);
    assert_eq!(path.way_points(), [centre(3, 3), centre(4, 4)], "step 2");
    // 2 7/9 px along the diagonal: (1.96, 1.96), shown as (22, 22).
    assert_eq!(at, Point::new(22, 22));

    // The whole walk: the diagonal, where the port's old 4-way search
    // (`game::path_to`) climbs a staircase of eight cells.
    let (end, cells) = walk(&g, centre(2, 2), centre(6, 6), 200);
    assert_eq!(cells, [(2, 2), (3, 3), (4, 4), (5, 5), (6, 6)]);
    assert_eq!(end, centre(6, 6));
    let bfs = game::path_to(&g, centre(2, 2), centre(6, 6)).unwrap();
    assert_eq!(bfs.len(), 8);
}

/// A goal on a closed cell: the reverse search over closed cells finds an
/// open one (`:211-216`), the route is searched again to it (`:269-299`),
/// and he stops there — `Path.Goal` becomes it (`FinishCalculation`,
/// `:442-448`). Worked by hand: from (5,4) the reverse search lists the
/// block's eight cells, takes (4,3) first (appended first, `:342`), lists
/// its open neighbours (3,2), (4,2), (5,2), (3,3), (3,4) at cost 2 with 0 to
/// go (`:364-370`); the next take is (5,3), the head of the list sorted
/// before they came; then the sort — by cost, *highest first*
/// (`NodeComparerReverse`, `:31-37`) — puts (3,2) at the head, it is taken
/// with 0 to go, and nothing cheaper than 2 can follow
/// (`first.TotalCost < nearestNode.TotalCost`, `:298-299`). Not the open
/// cell straight above the goal, (5,2), nor the one beside the block's
/// corner nearest the walker.
#[test]
fn a_closed_goal_is_walked_to_the_nearest_open_cell() {
    let g = grid(&[
        "........", //
        "........",
        "........",
        "....###.",
        "....###.",
        "....###.",
        "........",
        "........",
    ]);
    let (end, _) = walk(&g, centre(1, 1), centre(5, 4), 400);
    let near = cell_of(end);
    assert!(g.cell(near.0 as usize, near.1 as usize), "stops on an open cell: {end:?}");
    assert_eq!(near, (3, 2));
}

/// **`UpdateMovingTarget`** (`ComplexPath.cs:425-442`): while the route to
/// the old target is still being walked, a moved target is not taken up;
/// once the route is used up — or within 8 pixels of its end — it is; and
/// `forceRepath` takes it at once.
#[test]
fn a_moving_target_is_taken_up_only_near_the_end_of_the_route() {
    let g = grid(OPEN_8);
    let budget = gait::CONTROLLED;
    let mut path = ComplexPath::standing(centre(0, 0));
    let mut stride = Stride::default();
    path.update_moving_target(centre(7, 0), true);
    let mut at = centre(0, 0);
    for _ in 0..3 {
        at = path.process(&g, at, &mut stride, budget, STEP_SECONDS).0;
    }
    // Three steps in: the search is not finished, so a new target waits.
    assert!(!path.finished());
    path.update_moving_target(centre(7, 7), false);
    assert_eq!(path.move_to(), centre(7, 7));
    at = path.process(&g, at, &mut stride, budget, STEP_SECONDS).0;
    assert_eq!(path.goal(), centre(7, 0), "still the old route");
    // Forced: the new one, searched from the next step.
    path.update_moving_target(centre(7, 7), true);
    path.process(&g, at, &mut stride, budget, STEP_SECONDS);
    assert_eq!(path.goal(), centre(7, 7));

    // Not forced, but the route is done and he is within 8 pixels of it.
    let mut path = ComplexPath::standing(centre(0, 0));
    let mut stride = Stride::default();
    path.set_move_to(centre(2, 0));
    let mut at = centre(0, 0);
    for _ in 0..40 {
        at = path.process(&g, at, &mut stride, budget, STEP_SECONDS).0;
    }
    assert!(path.finished());
    path.update_moving_target(centre(2, 3), false);
    path.process(&g, at, &mut stride, budget, STEP_SECONDS);
    assert_eq!(path.goal(), centre(2, 3));
}

fn real_grid(name: &str) -> Option<WalkGrid> {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/burntime.pack")).ok()?;
    let pack = burngfx::pack::Pack::open(&bytes).ok()?;
    WalkGrid::from_packed(pack.get(&format!("walk/{name}"))?)
}

/// On a real mask (`MAT_001`, 72 × 60 cells), from cell (8, 40) to
/// (45, 8): v1.2's search heads diagonally north-east round what is in the
/// way, through 55 cells, where the port's former 4-way search
/// (`game::path_to`) took 69 — east along row 40, then a staircase. The
/// same walk, position by position, is upstream's own run by .NET
/// (`real_routes_are_upstream_s_step_by_step`).
#[test]
fn a_real_route_goes_round_as_v1_2_s_does() {
    let Some(g) = real_grid("MAT_001") else { return };
    let (from, to) = (centre(8, 40), centre(45, 8));
    let (end, cells) = walk(&g, from, to, 5000);
    let want: &[(i32, i32)] = &[
        (8, 40), (9, 40), (9, 39), (10, 39), (10, 38), (11, 38), (11, 37), (12, 37), (12, 36), (13, 36),
        (14, 35), (15, 35), (15, 34), (16, 34), (16, 35), (17, 35), (18, 35), (19, 35), (20, 34), (21, 33),
        (21, 32), (22, 32), (22, 31), (23, 31), (24, 30), (25, 29), (26, 28), (26, 27), (27, 27), (28, 26),
        (29, 25), (29, 24), (30, 24), (31, 23), (32, 22), (33, 21), (34, 20), (34, 19), (35, 19), (36, 18),
        (36, 17), (37, 17), (38, 16), (39, 15), (39, 14), (40, 14), (40, 13), (41, 13), (41, 12), (42, 12),
        (43, 11), (44, 10), (44, 9), (45, 9), (45, 8),
    ];
    assert_eq!(cells, want);
    assert_eq!(end, to);
    assert!(cells.iter().all(|c| g.cell(c.0 as usize, c.1 as usize)));
    let bfs: Vec<(i32, i32)> = game::path_to(&g, from, to).unwrap().iter().map(|p| cell_of(*p)).collect();
    assert_eq!(bfs.len(), 69);
    assert_eq!(&bfs[..3], [(9, 40), (10, 40), (11, 40)]);
}

/// Real .NET: `tools/dotnet-complexpath` runs `List<T>.Sort` and upstream's
/// own `ComplexPath.cs` and writes `tests/data/dotnet_complexpath.txt`.
fn vectors() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/dotnet_complexpath.txt")).unwrap()
}

/// `dotnet_sort` leaves ties exactly where .NET 10's `List<T>.Sort` does,
/// on 78 lists of 2 to 200 keys with few distinct values, both comparers.
#[test]
fn the_sort_is_dotnet_s() {
    let mut n = 0;
    for line in vectors().lines().filter(|l| l.starts_with("sort ")) {
        let (head, ids) = line.split_once(" = ").unwrap();
        let mut w = head.split(' ').skip(1);
        let desc = w.next() == Some("desc");
        let _count = w.next();
        let mut list: Vec<(i32, usize)> = w.enumerate().map(|(i, k)| (k.parse().unwrap(), i)).collect();
        if desc {
            burngfx::complex_path::dotnet_sort(&mut list, |a, b| b.0.cmp(&a.0));
        } else {
            burngfx::complex_path::dotnet_sort(&mut list, |a, b| a.0.cmp(&b.0));
        }
        let want: Vec<usize> = ids.split(' ').map(|s| s.parse().unwrap()).collect();
        assert_eq!(list.iter().map(|e| e.1).collect::<Vec<_>>(), want, "{line}");
        n += 1;
    }
    assert_eq!(n, 78);
}

/// The walk, step by step, against upstream's `ComplexPath` run by .NET on
/// the same mask: every position each step returns.
#[test]
fn real_routes_are_upstream_s_step_by_step() {
    let Some(g) = real_grid("MAT_001") else { return };
    for line in vectors().lines().filter(|l| l.starts_with("route ")) {
        let w: Vec<&str> = line.split(' ').collect();
        let n: Vec<i32> = w[2..6].iter().map(|s| s.parse().unwrap()).collect();
        let want: Vec<Point> = w[6..]
            .iter()
            .map(|p| {
                let (x, y) = p.split_once(',').unwrap();
                Point::new(x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        let mut path = ComplexPath::standing(centre(n[0], n[1]));
        path.set_move_to(centre(n[2], n[3]));
        let mut stride = Stride::default();
        let mut at = centre(n[0], n[1]);
        let mut got = Vec::new();
        for _ in 0..want.len() {
            at = path.process(&g, at, &mut stride, gait::CONTROLLED, STEP_SECONDS).0;
            got.push(at);
        }
        let first = got.iter().zip(&want).position(|(a, b)| a != b);
        assert_eq!(first, None, "{} {:?}: step {:?}: {:?} / {:?}", w[1], &n, first, first.map(|i| got[i]), first.map(|i| want[i]));
    }
}
