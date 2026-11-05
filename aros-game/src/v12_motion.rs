//! One step of everybody's feet in a v1.2 game's location: the location's
//! people (`Location.Update`), then the player's group
//! (`Player.Update`), each as v1.2's `Character.Update` does it
//! (`Logic/Character/Character.cs:956-1127`):
//!
//! 1. the speed — 50 px/s for the group and whoever the player controls, 35
//!    for a creature, 23.1 for anybody with a `SimpleMind`, 17.5 for a
//!    mutant with one, 70 while fleeing;
//! 2. the pauses — a hovered character who is not the player's stands, and
//!    so does a person whom the selected character has come within 30 px of,
//!    for 10 s;
//! 3. the mind, unless a fight owns the character's feet;
//! 4. the fight's approach, then its flight;
//! 5. the walk: `SimplePath` for the location's people, a route round what
//!    is in the way (`ComplexPath`) for the group.
//!
//! The encounter's own update comes after (`LocationScene.cs:532-537`); the
//! caller runs it. Legacy games walk by their own code.

use alloc::vec::Vec;
use burngfx::complex_path::ComplexPath;
use burngfx::gait::{self, Stride, Walked};
use burngfx::gamdat::CharClass;
use burngfx::game::Point;
use burngfx::mind::{Held, Lead, Retarget};
use burngfx::zei::WalkGrid;

use crate::v12_combat::{Flight, Who, World};

/// What v1.2 keeps for the game rather than for a character: the leader's
/// fraction of a pixel and the creatures' bite cooldown. See
/// [`burngfx::gait::Motion`], which is what a save writes of it.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub leader: Stride,
    /// `creatureAttackCooldownRemaining`, in steps.
    pub cooldown: u32,
    /// The group's `ComplexPath`s, by [`tag_of`], with the location each
    /// was made in. `[NonSerialized]` searches (`ComplexPath.cs:452-453`):
    /// none survives a load, and a walker's is made afresh, standing, in
    /// each place he walks.
    paths: Vec<(u32, usize, ComplexPath)>,
    /// The leader is walking where a click sent him: v1.2's
    /// `Path.MoveTo != Position`, which its save keeps and its load walks on
    /// toward (`move` and `adjustedMove` serialized; `Character.Update` runs
    /// `Path.Process` each frame). Set by the game each frame.
    pub leader_walk: bool,
}

impl State {
    pub fn saved(&self, leader_at: Point) -> gait::Motion {
        gait::Motion { leader: self.leader.carry(leader_at), cooldown: self.cooldown }
    }

    pub fn loaded(m: gait::Motion, leader_at: Point, here: Option<usize>, paths: &[gait::PathRecord]) -> State {
        // `adjustedMove` as it was; the search, `[NonSerialized]`, is not.
        let paths = match here {
            Some(here) => paths.iter().map(|p| (p.who, here, ComplexPath::loaded(p.move_to, p.move_to, p.adjusted))).collect(),
            None => Vec::new(),
        };
        State { leader: Stride::carried(leader_at, m.leader), cooldown: m.cooldown, paths, leader_walk: false }
    }

    /// Where a loaded game's leader goes on walking: his saved `move`.
    pub fn resumed_walk(&self) -> Option<Point> {
        self.paths.iter().find(|(t, _, _)| *t == tag_of(Who::Leader)).map(|(_, _, p)| p.move_to())
    }

    /// The paths a save keeps: the leader's while he walks, anybody else's
    /// whose `adjustedMove` is not `move` (the rest a load rebuilds from the
    /// `move_to` it keeps).
    pub fn saved_paths(&self) -> Vec<gait::PathRecord> {
        let leader = tag_of(Who::Leader);
        self.paths
            .iter()
            .filter(|(t, _, p)| {
                if *t == leader {
                    self.leader_walk
                } else {
                    p.adjusted() != (p.move_to().x as f32, p.move_to().y as f32)
                }
            })
            .map(|(who, _, p)| gait::PathRecord { who: *who, move_to: p.move_to(), adjusted: p.adjusted() })
            .collect()
    }

    /// `who`'s `ComplexPath` in location `here`, standing at `at` if he has
    /// none there yet. Its `MoveTo` is kept equal to `move_to`, the port's
    /// copy of it, which others set — a fight's `Stop`, a click.
    pub fn path(&mut self, who: Who, here: usize, at: Point, move_to: Point) -> &mut ComplexPath {
        let path = path_in(&mut self.paths, who, here, at);
        if path.move_to() != move_to {
            path.set_move_to(move_to);
        }
        path
    }

    /// The leader's fraction of a pixel and his `ComplexPath`, together.
    pub fn leader_path(&mut self, here: usize, at: Point) -> (&mut Stride, &mut ComplexPath) {
        (&mut self.leader, path_in(&mut self.paths, Who::Leader, here, at))
    }
}

fn path_in(paths: &mut Vec<(u32, usize, ComplexPath)>, who: Who, here: usize, at: Point) -> &mut ComplexPath {
    let tag = tag_of(who);
    let n = match paths.iter().position(|(t, _, _)| *t == tag) {
        Some(n) if paths[n].1 == here => n,
        Some(n) => {
            paths[n] = (tag, here, ComplexPath::standing(at));
            n
        }
        None => {
            paths.push((tag, here, ComplexPath::standing(at)));
            paths.len() - 1
        }
    };
    &mut paths[n].2
}

/// The flights under way, as a save writes them.
pub fn saved_flights(moves: &crate::v12_combat::Moves) -> Vec<gait::Flight> {
    moves.flee.iter().map(|f| gait::Flight { who: tag_of(f.who), from: tag_of(f.from), to: f.to, left: f.left }).collect()
}

/// A load's fight moves: the flights it carries and nothing else — an
/// encounter's approaches and holds are not saved, as the encounter is not.
pub fn loaded_flights(moves: &mut crate::v12_combat::Moves, flights: &[gait::Flight]) {
    *moves = crate::v12_combat::Moves::default();
    moves.flee = flights.iter().map(|f| Flight { who: who_of(f.who), from: who_of(f.from), to: f.to, left: f.left }).collect();
}

/// What the step needs to know about the scene.
pub struct Env<'e> {
    /// `Location.IsCity`: creatures attack nobody.
    pub city: bool,
    /// `Player.SelectedCharacter`.
    pub selected: Who,
    /// `Location.HoverCharacter`: the person under the pointer, when the
    /// pointer is the mouse's.
    pub hovered: Option<usize>,
    pub grid: Option<&'e WalkGrid>,
    /// `Player.SingleMode`: the followers' minds do nothing.
    pub single_mode: bool,
    /// How the leader is walking, for `FellowerMind`.
    pub lead: Lead,
}

/// A creature's tag for whom it has chosen: 0 the leader, 1 + id a
/// companion, a million and up a camper by index.
const CAMPER_TAG: u32 = 1_000_000;

fn tag_of(who: Who) -> u32 {
    match who {
        Who::Leader => 0,
        Who::Friend(id) => 1 + id as u32,
        Who::Person(i) => CAMPER_TAG + i as u32,
    }
}

fn who_of(tag: u32) -> Who {
    match tag {
        0 => Who::Leader,
        t if t >= CAMPER_TAG => Who::Person((t - CAMPER_TAG) as usize),
        t => Who::Friend(t as usize - 1),
    }
}

/// `HasItemFunction(ItemFunction.CreatureDeterrent)`: in his own pack — a
/// camper's is his camp record's.
fn deterred(w: &World<'_>, who: Who) -> bool {
    let has = |pack: &burngfx::game::ItemBag| burngfx::v12_items::has_function(pack, w.types, "creature_deterrent");
    match who {
        Who::Leader => has(w.carried),
        Who::Friend(id) => w.party.companions.iter().find(|c| c.id == id).is_some_and(|c| has(&c.pack)),
        Who::Person(i) => w.people.get(i).is_some_and(|p| {
            has(&p.pack)
                || w.camps.get(w.here).and_then(|k| k.residents.iter().find(|r| r.origin == Some(p.record))).is_some_and(|r| has(&r.pack))
        }),
    }
}

fn held(w: &World<'_>, tag: u32) -> Option<Held> {
    let who = who_of(tag);
    let at = w.position(who)?;
    Some(Held { at, dead: burngfx::encounter::Arena::is_dead(w, who), deterred: deterred(w, who) })
}

/// `CreatureMind.SelectAttackTarget` (`AI/CreatureMind.cs:130-143`): the
/// selected character if he can be attacked, else the nearest of the group
/// who can — alive, and carrying no deterrent.
fn select(w: &World<'_>, env: &Env<'_>, from: Point) -> Option<(Point, u32)> {
    let can = |who: Who| !burngfx::encounter::Arena::is_dead(w, who) && !deterred(w, who);
    if can(env.selected) {
        if let Some(at) = w.position(env.selected) {
            return Some((at, tag_of(env.selected)));
        }
    }
    let mut group = Vec::with_capacity(1 + w.party.companions.len());
    group.push(Who::Leader);
    group.extend(w.party.companions.iter().map(|c| Who::Friend(c.id)));
    group
        .into_iter()
        .filter(|&g| can(g))
        .filter_map(|g| w.position(g).map(|at| (at, tag_of(g))))
        .min_by_key(|(at, _)| {
            let (dx, dy) = ((at.x - from.x) as i64, (at.y - from.y) as i64);
            dx * dx + dy * dy
        })
}

/// One step for everybody in the location and the group.
pub fn step(w: &mut World<'_>, st: &mut State, env: &Env<'_>) {
    // `Location.Update`: the cooldown first, then each of its people.
    st.cooldown = st.cooldown.saturating_sub(1);
    let here: Vec<usize> = w.people.at(w.here).collect();
    for i in here {
        person(w, st, env, i);
    }
    // `Player.Update`: the group.
    leader(w, st, env);
    let ids: Vec<usize> = w.party.companions.iter().map(|c| c.id).collect();
    for (index, id) in ids.iter().enumerate() {
        companion(w, st, env, *id, index, ids.len());
    }
}

/// The selected character's position, when he is in this location.
fn selected_at(w: &World<'_>, env: &Env<'_>) -> Option<Point> {
    w.position(env.selected)
}

/// `UpdateCombatApproach`'s answer for one step: `None` when he is not
/// approaching anybody, else where to walk (`Some(None)`: stand, he is in
/// range or the target is gone).
fn approach(w: &mut World<'_>, who: Who) -> Option<Option<Point>> {
    let (target, range) = w.moves.approaching(who)?;
    let gone = burngfx::encounter::Arena::is_dead(w, target) || !burngfx::encounter::Arena::same_place(w, who, target);
    if gone {
        // `ClearCombatApproach`: the walk stops.
        w.moves.approach.retain(|(a, t, _)| !(*a == who && *t == target));
        w.stop(who);
        return Some(None);
    }
    if burngfx::encounter::Arena::in_range(w, who, target, range) {
        w.stop(who);
        return Some(None);
    }
    Some(w.position(target))
}

/// The flight's part of a step: count it down, find a new point when the
/// last is reached, and say where to run (`Character.cs:1030-1051`).
/// `None` when he is not fleeing; `Some(None)` on the step it ends.
fn flight(w: &mut World<'_>, who: Who) -> Option<Option<Point>> {
    let n = w.moves.flee.iter().position(|f| f.who == who)?;
    let Flight { from, to, left, .. } = w.moves.flee[n];
    let left = left.saturating_sub(1);
    if left == 0 {
        w.moves.flee.remove(n);
        w.stop(who);
        return Some(None);
    }
    let at = w.position(who)?;
    let mut to = to;
    if gait::within(to, at, 4) {
        if let Some(next) = w.flee_point(who, from) {
            if next != at {
                to = next;
            }
        }
    }
    if let Some(f) = w.moves.flee.get_mut(n) {
        f.left = left;
        f.to = to;
    }
    Some(Some(to))
}

/// One of the location's people: `Character.Update` with a `SimplePath`.
fn person(w: &mut World<'_>, st: &mut State, env: &Env<'_>, i: usize) {
    let who = Who::Person(i);
    let Some(p) = w.people.get(i) else { return };
    let class = p.class;
    let creature = matches!(class, CharClass::Dog | CharClass::Mutant);
    // A free dog or mutant has a `CreatureMind` — unless a boss ever took
    // him on, and then a `SimpleMind` for good (`Dismiss`, `JoinCamp`);
    // anybody else standing in a place a `SimpleMind`, unless the player
    // took him in hand (`PlayerControlledMind`).
    let creature_mind = creature && p.employer.is_none() && !p.bearing.tame;
    let controlled_mind = p.bearing.controlled;
    let controlled = controlled_mind || env.selected == who;
    let simple_mind = !creature_mind && !controlled_mind;

    let fleeing = w.moves.fleeing(who);
    let approaching = w.moves.approaching(who).is_some();
    let held_now = w.moves.held.contains(&who);
    let mut speed = if controlled {
        gait::CONTROLLED
    } else if !simple_mind {
        gait::WALK
    } else {
        match class {
            CharClass::Dog => gait::WALK,
            CharClass::Mutant => gait::SLOW_MUTANT,
            _ => gait::WANDER,
        }
    };
    if fleeing {
        speed = gait::FLEE;
    }

    // `TALKING_DISTANCE` and `PROXIMITY_PAUSE_TIME`: a person stands still
    // for ten seconds when the selected character comes within thirty.
    let near = !held_now
        && !approaching
        && !fleeing
        && !creature
        && !controlled
        && selected_at(w, env).is_some_and(|s| {
            let (dx, dy) = ((s.x - p.at.x) as i64, (s.y - p.at.y) as i64);
            dx * dx + dy * dy < 30 * 30
        });
    let hovered = env.hovered == Some(i);
    if let Some(p) = w.people.get_mut(i) {
        let b = &mut p.bearing;
        if near && !b.near {
            b.pause = 10 * burngfx::mind::SECOND;
        } else if !near {
            b.pause = 0;
        }
        b.near = near;
        if b.pause > 0 {
            b.pause -= 1;
            if !fleeing && !approaching {
                speed = 0;
            }
        }
    }
    if hovered && !fleeing && !approaching && !controlled {
        speed = 0;
    }

    // The mind, unless a fight has his feet.
    if !w.moves.owns(who) {
        if creature_mind {
            let (record, at, mut move_to, target) = {
                let p = w.people.get(i).unwrap();
                (p.record, p.at, p.move_to, p.mind.target)
            };
            let seen = held(w, target);
            let chosen = select(w, env, at);
            let walkable = w.walkable;
            let mut mind = w.people.get(i).unwrap().mind;
            let bit = mind.prowl_v12(
                &mut burngfx::mind::Draws::Game(w.rng),
                record,
                at,
                &mut move_to,
                env.city,
                if mind.charging { seen } else { None },
                chosen,
                &mut st.cooldown,
                &|q| walkable(q),
            );
            if let Some(p) = w.people.get_mut(i) {
                p.mind = mind;
                p.move_to = move_to;
            }
            if let Some(tag) = bit {
                crate::v12_combat::creature_bites(w, who, who_of(tag));
            }
        } else if simple_mind {
            let walkable = w.walkable;
            if let Some(p) = w.people.get_mut(i) {
                let (record, at) = (p.record, p.at);
                let mut move_to = p.move_to;
                p.mind.wander(&mut burngfx::mind::Draws::Game(w.rng), record, at, &mut move_to, &|q| walkable(q));
                p.move_to = move_to;
            }
        }
    }

    // A camper the player selected has a `ComplexPath` from then on
    // (`Player.SelectCharacter`, `Logic/Player.cs:206-209`), and a fight
    // moves him by it: `UpdateMovingTarget` for an approach, `MoveTo` for a
    // flight (`Character.cs:557-567,616-617,1048`). While selected, his
    // other walks are the player's clicks, the caller's; once not, he goes
    // on to his `MoveTo` by the same path.
    if controlled_mind {
        let feet = fight_feet(w, who);
        let here = w.here;
        let Some(p) = w.people.get_mut(i) else { return };
        let path = path_in(&mut st.paths, who, here, p.at);
        match feet {
            FightFeet::None if env.selected == who => return,
            FightFeet::None => {
                if path.move_to() != p.move_to {
                    path.set_move_to(p.move_to);
                }
            }
            FightFeet::Stop => {
                path.stop(p.at, &mut p.bearing.stride);
                p.move_to = p.at;
                return;
            }
            FightFeet::Run(to) => path.set_move_to(to),
            FightFeet::Chase(to, force) => path.update_moving_target(to, force),
        }
        if speed == 0 {
            return;
        }
        let (now, _) = walk_round(env.grid, path, p.at, &mut p.bearing.stride, speed);
        p.at = now;
        p.move_to = path.move_to();
        return;
    }

    // The fight's approach: at the target by `SimplePath`, every step.
    if let Some(Some(to)) = approach(w, who) {
        if let Some(p) = w.people.get_mut(i) {
            p.move_to = to;
        }
    }
    // And its flight, which has the last word.
    if let Some(Some(to)) = flight(w, who) {
        if let Some(p) = w.people.get_mut(i) {
            p.move_to = to;
        }
    }

    if speed == 0 {
        return;
    }
    let walkable = w.walkable;
    if let Some(p) = w.people.get_mut(i) {
        let mut move_to = p.move_to;
        let (now, _) = gait::simple(p.at, &mut p.bearing.stride, &mut move_to, gait::per_step(speed), &|q| walkable(q));
        p.at = now;
        p.move_to = move_to;
    }
}

/// **`ComplexPath.Process`** for one of the group: v1.2's incremental
/// search and walk ([`burngfx::complex_path`]). Without a walk mask there is
/// nothing to go round, and he walks straight.
pub fn walk_round(grid: Option<&WalkGrid>, path: &mut ComplexPath, at: Point, stride: &mut Stride, speed: u32) -> (Point, Walked) {
    match grid {
        Some(g) => path.process(g, at, stride, speed, burngfx::complex_path::STEP_SECONDS),
        None => {
            let to = path.move_to();
            let now = gait::straight(at, stride, to, gait::per_step(speed));
            (now, if now == to { Walked::Arrived } else { Walked::Going })
        }
    }
}

/// How the fight moves somebody of the group this step, as v1.2 tells his
/// `ComplexPath`: a flight sets `MoveTo` every frame (`Character.cs:1048`);
/// an approach follows its target with `UpdateMovingTarget`, forced on the
/// step it began (`BeginCombatApproach`, `:566-567`; `UpdateCombatApproach`,
/// `:616-617`); either ending is `Path.Stop`.
enum FightFeet {
    None,
    Stop,
    Run(Point),
    Chase(Point, bool),
}

fn fight_feet(w: &mut World<'_>, who: Who) -> FightFeet {
    let fresh = w.moves.fresh.contains(&who);
    w.moves.fresh.retain(|f| *f != who);
    let chase = approach(w, who);
    match (chase, flight(w, who)) {
        (_, Some(Some(to))) => FightFeet::Run(to),
        (Some(Some(to)), None) => FightFeet::Chase(to, fresh),
        (Some(None), _) | (_, Some(None)) => FightFeet::Stop,
        _ => FightFeet::None,
    }
}

/// The leader's part of a fight's feet: an approach routes round what is in
/// the way, a flight runs; a click's walk is the caller's.
fn leader(w: &mut World<'_>, st: &mut State, env: &Env<'_>) {
    let speed = if w.moves.fleeing(Who::Leader) { gait::FLEE } else { gait::CONTROLLED };
    let feet = fight_feet(w, Who::Leader);
    let at = w.state.position;
    let (stride, path) = st.leader_path(w.here, at);
    match feet {
        FightFeet::None => return,
        FightFeet::Stop => {
            path.stop(at, stride);
            return;
        }
        FightFeet::Run(to) => path.set_move_to(to),
        FightFeet::Chase(to, force) => path.update_moving_target(to, force),
    }
    let (now, _) = walk_round(env.grid, path, at, stride, speed);
    w.state.position = now;
}

/// A companion: `FellowerMind` unless the player has him in hand or a fight
/// has his feet, then his walk.
fn companion(w: &mut World<'_>, st: &mut State, env: &Env<'_>, id: usize, index: usize, count: usize) {
    let who = Who::Friend(id);
    let led = env.selected == who;
    let fleeing = w.moves.fleeing(who);
    let mut speed = if fleeing { gait::FLEE } else { gait::CONTROLLED };
    let mut retarget = Retarget::None;
    if !w.moves.owns(who) && !led && !env.single_mode {
        let leader = w.state.position;
        let rng = &mut *w.rng;
        if let Some(c) = w.party.companions.iter_mut().find(|c| c.id == id) {
            let mut move_to = c.move_to;
            // `Leader.Path.Speed * CatchUpSpeedMultiplier`.
            if c.fellower.follow(rng, index, count, c.at, &mut move_to, leader, env.lead) {
                speed = speed.max(gait::CONTROLLED * 6 / 5);
            }
            c.move_to = move_to;
            retarget = c.fellower.retarget;
        }
    }
    let feet = fight_feet(w, who);
    // The selected one walks by the player's clicks, the caller's — unless
    // a fight has his feet.
    if led && !w.moves.owns(who) {
        return;
    }
    let grid = env.grid;
    let here = w.here;
    let Some(c) = w.party.companions.iter_mut().find(|c| c.id == id) else { return };
    let path = path_in(&mut st.paths, who, here, c.at);
    match retarget {
        Retarget::MoveTo => path.set_move_to(c.move_to),
        Retarget::Moving { force } => path.update_moving_target(c.move_to, force),
        Retarget::None => {}
    }
    match feet {
        FightFeet::Stop => path.stop(c.at, &mut c.stride),
        FightFeet::Run(to) => path.set_move_to(to),
        FightFeet::Chase(to, force) => path.update_moving_target(to, force),
        // Whatever else set where he goes — a load, a click while he was
        // selected — set `MoveTo`.
        FightFeet::None if retarget == Retarget::None && path.move_to() != c.move_to => path.set_move_to(c.move_to),
        FightFeet::None => {}
    }
    c.move_to = path.move_to();
    let (now, _) = walk_round(grid, path, c.at, &mut c.stride, speed);
    c.at = now;
}

/// **`Location.PlaceUnpositionedResidents`** (`Logic/Location.cs:127-154`),
/// on every entry to the location's scene (`LocationScene.cs:869`):
/// whoever of its people stands exactly on the entry point — somebody who
/// came back to life there, as `EnterLocation` puts them
/// (`Location.cs:120-125`) — is moved to where one of the others stands,
/// one of those more than 20 from the entry, drawn `Next(count)`; with
/// nobody else about, to the walkable points 8 beside the entry in turn,
/// and with none of those, left where he is.
pub fn place_unpositioned(
    people: &mut burngfx::hire::People,
    here: usize,
    entry: Point,
    walkable: &dyn Fn(Point) -> bool,
    rng: &mut burngfx::spawn::Rng,
) {
    let standing: Vec<usize> = people.at(here).collect();
    let away: Vec<Point> = standing
        .iter()
        .filter_map(|&i| people.get(i).map(|p| p.at))
        .filter(|&at| !gait::within(at, entry, 20))
        .collect();
    let beside: Vec<Point> = [(0, -8), (8, 0), (-8, 0), (0, 8)]
        .into_iter()
        .map(|(dx, dy)| Point::new(entry.x + dx, entry.y + dy))
        .filter(|&q| walkable(q))
        .collect();
    let mut fallback = 0;
    for i in standing {
        let Some(p) = people.get_mut(i) else { continue };
        if p.at != entry {
            continue;
        }
        let to = if !away.is_empty() {
            away[rng.below(away.len() as u32) as usize]
        } else if !beside.is_empty() {
            let q = beside[fallback % beside.len()];
            fallback += 1;
            q
        } else {
            entry
        };
        p.at = to;
        p.move_to = to;
        p.bearing.stride.stop(to);
    }
}

/// v1.2's movement rules, on the game's own people (`gam.dat`) and
/// Classic's rules, on open ground. Each expectation is upstream's, and the
/// source it comes from is named; the port's walk before these rules (two
/// pixels a step for everybody, no cooldown, a mind asked while fleeing)
/// fails each of them.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::v12_combat::{Moves, Strike};
    use burngfx::combat::Kit;
    use burngfx::game::{Camps, GameState, ItemBag, Party};
    use burngfx::hire::People;
    use burngfx::items::ItemTypes;
    use burngfx::rules::RuleBook;
    use burngfx::spawn::Rng;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../burngfx/testdata");
    const HERE: usize = 0;

    fn data() -> &'static (ItemTypes, RuleBook, People) {
        static DATA: std::sync::OnceLock<(ItemTypes, RuleBook, People)> = std::sync::OnceLock::new();
        DATA.get_or_init(|| {
            let rules = |f: &str| std::fs::read_to_string(format!("{DIR}/v1.2/rules/{f}")).unwrap();
            let types = ItemTypes::parse_v12(&rules("dos/items.txt")).unwrap();
            let book = RuleBook::parse(&rules("classic/game.txt"));
            let bytes = std::fs::read(format!("{DIR}/burntime.pak")).expect("testdata pack");
            let pak = burngfx::pak::ClassicPak::open(&bytes).unwrap();
            let gam = burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat").unwrap()).unwrap();
            let settings = burngfx::settings::GameSettings::parse("[0]\nnpc_respawn = 10\n", 0);
            let people = People::from_records(&gam.characters, &types, &settings);
            (types, book, people)
        })
    }

    /// A place with nobody in it but whom a test puts there.
    struct Place {
        state: GameState,
        carried: ItemBag,
        party: Party,
        people: People,
        camps: Camps,
        moves: Moves,
        struck: Vec<Strike>,
        rng: Rng,
        motion: State,
    }

    impl Place {
        fn new() -> Place {
            let (_, _, people) = data();
            let mut people = people.clone();
            for i in 0..people.len() {
                let p = people.get_mut(i).unwrap();
                p.location = 99;
                p.employer = None;
            }
            let mut state = GameState::new(Point::new(1000, 1000));
            state.needs.health = 100;
            Place {
                state,
                carried: ItemBag::new(6),
                party: Party::default(),
                people,
                camps: Camps::default(),
                moves: Moves::default(),
                struck: Vec::new(),
                rng: Rng::game(1993),
                motion: State::default(),
            }
        }

        /// One of the `class`, the `nth` such, put here at `at`, resting
        /// so long his mind does nothing.
        fn put(&mut self, class: CharClass, nth: usize, at: Point) -> usize {
            let who = (0..self.people.len())
                .filter(|&i| self.people.get(i).is_some_and(|p| p.class == class))
                .nth(nth)
                .unwrap();
            let p = self.people.get_mut(who).unwrap();
            p.location = HERE;
            p.at = at;
            p.move_to = at;
            p.mind = burngfx::mind::Mind { rest: 1_000_000, ..Default::default() };
            p.needs.health = 100;
            p.pack = ItemBag::new(6);
            p.kit = Kit::default();
            who
        }

        fn step(&mut self, env: &Env<'_>) {
            let (types, book, _) = data();
            let open = |_: Point| true;
            let mut w = World {
                state: &mut self.state,
                carried: &mut self.carried,
                party: &mut self.party,
                people: &mut self.people,
                camps: &mut self.camps,
                types,
                book,
                difficulty: 1,
                rng: &mut self.rng,
                here: HERE,
                hazard: burngfx::gamdat::Danger::None,
                leader_class: CharClass::Boss,
                leader_experience: 37,
                human_seats: &[0],
                moves: &mut self.moves,
                struck: &mut self.struck,
                walkable: &open,
                exposure: &mut burngfx::danger::Exposure::default(),
            };
            step(&mut w, &mut self.motion, env);
        }

        fn at(&self, who: usize) -> Point {
            self.people.get(who).unwrap().at
        }
    }

    fn env() -> Env<'static> {
        Env { city: false, selected: Who::Leader, hovered: None, grid: None, single_mode: true, lead: Lead::Clicked(None) }
    }

    /// Steps until `who` stands on `to`, at most `limit`.
    fn steps_to(place: &mut Place, who: usize, to: Point, env: &Env<'_>, limit: u32) -> u32 {
        place.people.get_mut(who).unwrap().move_to = to;
        for n in 1..=limit {
            place.step(env);
            if place.at(who) == to {
                return n;
            }
        }
        panic!("never arrived: at {:?}", place.at(who));
    }

    /// **Speeds** (`Character.cs:972-985`): a free mercenary with his
    /// `SimpleMind` walks `WALK_SPEED * 0.66f` = 23.1 px/s — 100 px in 4.33 s,
    /// 78 steps; a dog with its `CreatureMind` 35 px/s — 2.86 s, 52 steps.
    /// Everybody walked 2 px a step before (50 steps).
    #[test]
    fn a_free_man_and_a_dog_walk_at_their_own_speeds() {
        let mut place = Place::new();
        let man = place.put(CharClass::Mercenary, 0, Point::new(100, 100));
        let dog = place.put(CharClass::Dog, 0, Point::new(100, 300));
        let n = steps_to(&mut place, man, Point::new(200, 100), &env(), 200);
        assert!((77..=79).contains(&n), "the mercenary took {n} steps");
        let n = steps_to(&mut place, dog, Point::new(200, 300), &env(), 200);
        assert!((51..=53).contains(&n), "the dog took {n} steps");
    }

    /// The player's own camper, taken in hand (`PlayerControlledMind`),
    /// walks at `CONTROLLED_WALK_SPEED`, 50 px/s: 100 px in 36 steps.
    #[test]
    fn a_camper_in_hand_walks_at_fifty() {
        let mut place = Place::new();
        let man = place.put(CharClass::Mercenary, 0, Point::new(100, 100));
        place.people.get_mut(man).unwrap().employer = Some(0);
        place.people.take_in_hand(man);
        let n = steps_to(&mut place, man, Point::new(200, 100), &env(), 200);
        assert!((35..=37).contains(&n), "{n} steps");
    }

    /// **Hover** (`Character.cs:1007-1009`): the person under the pointer
    /// does not move.
    #[test]
    fn a_hovered_man_stands() {
        let mut place = Place::new();
        let man = place.put(CharClass::Mercenary, 0, Point::new(100, 100));
        place.people.get_mut(man).unwrap().move_to = Point::new(200, 100);
        let hovered = Env { hovered: Some(man), ..env() };
        for _ in 0..50 {
            place.step(&hovered);
        }
        assert_eq!(place.at(man), Point::new(100, 100));
        place.step(&env());
        assert_ne!(place.at(man), Point::new(100, 100), "and walks on once the pointer leaves");
    }

    /// **The proximity pause** (`Character.cs:987-1005`): the selected
    /// character coming within 30 px stops a person for ten seconds — 180
    /// steps — and then he walks on though the other stays.
    #[test]
    fn a_man_the_leader_walks_up_to_stands_ten_seconds() {
        let mut place = Place::new();
        let man = place.put(CharClass::Mercenary, 0, Point::new(100, 100));
        place.state.position = Point::new(120, 100);
        place.people.get_mut(man).unwrap().move_to = Point::new(100, 300);
        let mut still = 0;
        while place.at(man) == Point::new(100, 100) && still < 400 {
            place.step(&env());
            still += 1;
        }
        // Still for 180 steps, walking on the 181st.
        assert_eq!(still, 181, "stood {} steps", still - 1);
    }

    /// **One bite a second in the whole game** (`TryBeginCreatureAttack`,
    /// `States/ClassicGame.cs:19-22,106-121`; `CreatureMind.cs:49-60`):
    /// three dogs charging the leader and kept in reach bite on steps 1, 19
    /// and 37 — the cooldown is counted down at the start of each location
    /// update, and a dog it holds back keeps charging and keeps its target.
    /// The port bit three times on the first step.
    #[test]
    fn three_dogs_in_reach_bite_once_a_second() {
        let mut place = Place::new();
        let leader = place.state.position;
        let dogs: Vec<(usize, Point)> = (0..3)
            .map(|n| {
                let at = Point::new(leader.x + 5 + 3 * n as i32, leader.y);
                let d = place.put(CharClass::Dog, n, at);
                place.people.get_mut(d).unwrap().mind =
                    burngfx::mind::Mind { charging: true, target: 0, wait_after_attack: 10_000, ..Default::default() };
                (d, at)
            })
            .collect();
        place.state.needs.health = 1000;
        let mut bites = Vec::new();
        for n in 1..=40 {
            // Held where they are, still on their way somewhere — so no
            // decision is due and only the cooldown decides.
            for &(d, at) in &dogs {
                let p = place.people.get_mut(d).unwrap();
                p.at = at;
                p.move_to = Point::new(leader.x - 200, leader.y);
            }
            place.moves.flee.clear();
            place.step(&env());
            bites.extend(place.struck.drain(..).map(|_| n));
        }
        assert_eq!(bites, [1, 19, 37]);
    }

    /// **The target is held** (`attack`, `CreatureMind.cs:19,47-60`): a dog
    /// charging a companion bites him though the leader — selected, and
    /// nearer — stands beside it. The port re-chose every step.
    #[test]
    fn a_charging_dog_keeps_its_target() {
        let mut place = Place::new();
        let leader = place.state.position;
        let mut friend = burngfx::game::Companion::new(4);
        friend.at = Point::new(leader.x + 15, leader.y);
        friend.needs.health = 100;
        place.party.companions.push(friend);
        let dog = place.put(CharClass::Dog, 0, Point::new(leader.x + 3, leader.y));
        place.people.get_mut(dog).unwrap().mind =
            burngfx::mind::Mind { charging: true, target: 1 + 4, wait_after_attack: 10_000, ..Default::default() };
        place.step(&env());
        let blows: Vec<Strike> = place.struck.drain(..).collect();
        assert_eq!(blows.len(), 1);
        assert_eq!(blows[0].defender, Who::Friend(4));
    }

    /// **In a city a creature attacks nobody** (`canAttack = !IsCity`,
    /// `CreatureMind.cs:31,39-44,70`): its charge is dropped and it stops.
    #[test]
    fn a_dog_in_a_city_never_bites() {
        let mut place = Place::new();
        let leader = place.state.position;
        let dog = place.put(CharClass::Dog, 0, Point::new(leader.x + 5, leader.y));
        place.people.get_mut(dog).unwrap().mind = burngfx::mind::Mind { charging: true, wait_after_attack: -1, ..Default::default() };
        let city = Env { city: true, ..env() };
        for _ in 0..200 {
            place.step(&city);
            assert!(place.struck.is_empty());
        }
        assert!(!place.people.get(dog).unwrap().mind.charging);
    }

    /// **The flight** (`Character.cs:1030-1051`, `FLEE_SPEED` 70,
    /// `FLEE_DURATION` 2.5 s): 2.5 s after its bite a dog on open ground is
    /// 160 to 180 px from where it bit, having taken a new point every time
    /// it reached one; and it does not bite on the way, its mind not asked
    /// while it flees (`:1018-1023`). The port's ran 70 px and stopped.
    #[test]
    fn a_dog_runs_175_px_after_its_bite_and_bites_nobody_on_the_way() {
        let mut place = Place::new();
        let leader = place.state.position;
        let dog = place.put(CharClass::Dog, 0, Point::new(leader.x + 5, leader.y));
        place.people.get_mut(dog).unwrap().mind = burngfx::mind::Mind { charging: true, wait_after_attack: 10_000, ..Default::default() };
        place.step(&env());
        assert_eq!(place.struck.drain(..).count(), 1);
        let bit_at = place.at(dog);
        // Charging again at once, and the cooldown long over: only the
        // flight keeps it from biting.
        place.motion.cooldown = 0;
        place.people.get_mut(dog).unwrap().mind.charging = true;
        for _ in 0..(5 * burngfx::mind::SECOND / 2) {
            place.step(&env());
            assert!(place.struck.is_empty(), "a bite during the flight");
            place.motion.cooldown = 0;
        }
        let d = place.at(dog);
        let (dx, dy) = ((d.x - bit_at.x) as f64, (d.y - bit_at.y) as f64);
        let far = (dx * dx + dy * dy).sqrt();
        assert!((160.0..=180.0).contains(&far), "{far} px away");
        assert!(!place.moves.fleeing(Who::Person(dog)), "and the flight is over");
    }

    /// **Campers roam** (`JoinCamp` gives a `SimpleMind`,
    /// `Character.cs:372-380`): within a minute a camper has moved; taken in
    /// hand he stands until the day gives him his `SimpleMind` back
    /// (`ResetStationedMind`, `:893-904`).
    #[test]
    fn a_camper_roams_unless_taken_in_hand() {
        let mut place = Place::new();
        let man = place.put(CharClass::Mercenary, 0, Point::new(300, 300));
        let record = place.people.get(man).unwrap().record;
        place.people.get_mut(man).unwrap().employer = Some(1);
        place.people.joined_camp(record);
        for _ in 0..60 * burngfx::mind::SECOND {
            place.step(&env());
        }
        assert_ne!(place.at(man), Point::new(300, 300), "a camper roams");

        let stood = place.at(man);
        place.people.take_in_hand(man);
        for _ in 0..60 * burngfx::mind::SECOND {
            place.step(&Env { selected: Who::Leader, ..env() });
        }
        assert_eq!(place.at(man), stood, "in hand, he goes where he is sent");
        place.people.reset_stationed_minds();
        for _ in 0..60 * burngfx::mind::SECOND {
            place.step(&env());
        }
        assert_ne!(place.at(man), stood, "and roams again from the next day");
    }

    /// **`PlaceUnpositionedResidents`** (`Location.cs:127-154`): whoever
    /// stands on the entry is put where one of the others more than 20 from
    /// it stands, each as often as the others.
    #[test]
    fn a_man_on_the_entry_is_put_where_another_stands() {
        let entry = Point::new(50, 50);
        let mut counts = [0u32; 3];
        let spots = [Point::new(200, 50), Point::new(50, 200), Point::new(300, 300)];
        let mut rng = Rng::game(7);
        for _ in 0..3000 {
            let mut place = Place::new();
            for (n, s) in spots.iter().enumerate() {
                place.put(CharClass::Mercenary, n, *s);
            }
            place.put(CharClass::Mercenary, 3, Point::new(60, 50)); // within 20: not a spot
            let back = place.put(CharClass::Dog, 0, entry);
            place_unpositioned(&mut place.people, HERE, entry, &|_| true, &mut rng);
            let to = place.at(back);
            counts[spots.iter().position(|s| *s == to).expect("one of the spots")] += 1;
        }
        for c in counts {
            assert!((900..1100).contains(&c), "{counts:?}");
        }
    }

    /// **`FellowerMind`** (`AI/FellowerMind.cs:45-97`): a follower beside a
    /// standing leader settles in his slot, `radius · (sin, cos)(π/4 +
    /// 2π·i/n)` with the radius drawn 14..20 once; and a new destination of
    /// the leader starts him after `0.025·(i+1) + [0, 0.030]` s.
    #[test]
    fn a_follower_settles_in_his_slot() {
        let leader = Point::new(500, 500);
        let mut radii = std::collections::BTreeSet::new();
        let mut rng = Rng::game(3);
        for _ in 0..500 {
            for (i, n) in [(0usize, 1usize), (1, 3), (3, 4)] {
                let mut f = burngfx::mind::Fellower::default();
                let mut to = leader;
                f.follow(&mut rng, i, n, Point::new(502, 501), &mut to, leader, Lead::Clicked(None));
                radii.insert(f.radius);
                assert_eq!(to, burngfx::mind::slot(leader, f.radius, i, n));
                let a = core::f64::consts::PI / 4.0 + 2.0 * core::f64::consts::PI * i as f64 / n as f64;
                assert_eq!(to.x - leader.x, (a.sin() * f.radius as f64) as i32);
                assert_eq!(to.y - leader.y, (a.cos() * f.radius as f64) as i32);
            }
        }
        assert_eq!(radii.into_iter().collect::<Vec<_>>(), (14..=20).collect::<Vec<_>>());

        let mut delays = std::collections::BTreeSet::new();
        for _ in 0..2000 {
            let mut f = burngfx::mind::Fellower { radius: 16, ..Default::default() };
            let mut to = Point::new(520, 500);
            f.follow(&mut rng, 3, 4, Point::new(520, 500), &mut to, leader, Lead::Clicked(Some(Point::new(700, 500))));
            assert_eq!(to, Point::new(520, 500), "not yet");
            // In thousandths of a second, as drawn (one step is already
            // spent): 0.025 · 4 + 0 … 0.030.
            delays.insert((f.delay + 1000) / 18);
        }
        let (lo, hi) = (*delays.first().unwrap(), *delays.last().unwrap());
        assert_eq!((lo, hi), (100, 130), "{delays:?}");
    }

    /// **RULE-66, `UpdateMovingTarget`** (`ComplexPath.cs:425-442`): the
    /// leader's approach takes its target's place at once on the step it
    /// begins (`forceRepath: true`, `Character.cs:566-567`); after that a
    /// target who moves is followed only when the search is done and its
    /// route used up or within 8 pixels of its end (`:616-617`) — not every
    /// step, as the port's search found afresh each step did.
    #[test]
    fn an_approach_takes_up_a_moved_target_only_near_its_route_s_end() {
        let mut grid = WalkGrid::new(40, 40);
        for ty in 0..40 {
            for tx in 0..40 {
                grid.fill_tile(tx, ty, true);
            }
        }
        let env = Env { grid: Some(&grid), ..env() };
        let mut place = Place::new();
        let man = place.put(CharClass::Mercenary, 0, Point::new(1196, 1004));
        place.moves.approach.push((Who::Leader, Who::Person(man), 10.0));
        place.moves.fresh.push(Who::Leader);
        place.step(&env);
        let goal = |place: &mut Place| {
            let at = place.state.position;
            place.motion.leader_path(HERE, at).1.goal()
        };
        assert_eq!(goal(&mut place), Point::new(1196, 1004), "forced on the first step");
        // He steps aside; the search for the old place is still running.
        let p = place.people.get_mut(man).unwrap();
        (p.at, p.move_to) = (Point::new(1196, 1100), Point::new(1196, 1100));
        for _ in 0..3 {
            place.step(&env);
            assert_eq!(goal(&mut place), Point::new(1196, 1004), "not taken up mid-route");
        }
        // Once the route is nearly walked, it is.
        let mut took = None;
        for n in 0..200 {
            place.step(&env);
            if goal(&mut place) != Point::new(1196, 1004) {
                took = Some(n);
                break;
            }
        }
        let (at, g) = (place.state.position, goal(&mut place));
        assert!(took.is_some(), "never taken up: at {at:?}, goal {g:?}");
        let at = place.state.position;
        assert!(gait::within(at, Point::new(1196, 1004), 12), "taken up near the route's end: at {at:?}");
        assert_eq!(goal(&mut place), Point::new(1196, 1100));
    }

    /// **RULE-67**: a dog a boss let go gets a `SimpleMind` (`Dismiss`,
    /// `Character.cs:445-450`) and never a `CreatureMind` again — only
    /// `GameCreation` makes one (`GameCreation.cs:452`). He wanders; he does
    /// not charge the leader beside him in two minutes, where a wild dog
    /// on the same draws (seed 1) does. The port gave him back the
    /// creature's mind.
    #[test]
    fn a_dismissed_dog_wanders_and_never_charges() {
        let run = |dismiss: bool, seed: u64| {
            let mut place = Place::new();
            place.rng = Rng::game(seed);
            let leader = place.state.position;
            let at = Point::new(leader.x + 20, leader.y);
            let dog = place.put(CharClass::Dog, 0, at);
            place.people.get_mut(dog).unwrap().mind = burngfx::mind::Mind::default();
            if dismiss {
                let record = place.people.get(dog).unwrap().record;
                let mut c = burngfx::game::Companion::new(1);
                (c.at, c.needs.health) = (at, 100);
                assert!(place.people.dismissed(record, HERE, &c));
            }
            place.state.needs.health = 1000;
            let mut bites = 0;
            let (mut moved, mut charged) = (false, false);
            for _ in 0..120 * burngfx::mind::SECOND {
                place.step(&env());
                bites += place.struck.drain(..).count();
                moved |= place.at(dog) != at;
                charged |= place.people.get(dog).unwrap().mind.charging;
            }
            (bites, charged, moved, place.people.get(dog).unwrap().bearing.tame)
        };
        let (bites, charged, moved, tame) = run(true, 1);
        assert_eq!((bites, charged, tame), (0, false, true), "the dismissed dog charged");
        assert!(moved, "he wanders");
        let (bites, charged, _, _) = run(false, 1);
        assert!(charged && bites > 0, "a wild dog beside the leader charges and bites: {charged} {bites}");
    }

    /// A leader walking where a click sent him is saved with his path's
    /// `move` and `adjustedMove` (`ComplexPath.cs:418-421`, serialized) and
    /// a load walks him on (`Character.Update` → `Path.Process`); one
    /// standing is not, and a load leaves him standing.
    #[test]
    fn a_leader_s_click_walk_goes_on_after_a_load() {
        let mut st = State::default();
        let at = Point::new(100, 100);
        st.leader_path(HERE, at).1.set_move_to(Point::new(300, 140));
        st.leader_walk = true;
        let saved = st.saved_paths();
        assert_eq!(saved, [gait::PathRecord { who: 0, move_to: Point::new(300, 140), adjusted: (300.0, 140.0) }]);
        let back = State::loaded(gait::Motion::default(), at, Some(HERE), &saved);
        assert_eq!(back.resumed_walk(), Some(Point::new(300, 140)));
        st.leader_walk = false;
        assert!(st.saved_paths().is_empty());
        assert_eq!(State::loaded(gait::Motion::default(), at, Some(HERE), &st.saved_paths()).resumed_walk(), None);
    }

    /// A camper the player took in hand has a `ComplexPath`
    /// (`Player.SelectCharacter`, `Logic/Player.cs:206-209`), so a fight
    /// moves him by it — round a wall, where `SimplePath` gives up.
    #[test]
    fn a_camper_in_hand_approaches_round_a_wall() {
        let mut grid = WalkGrid::new(40, 40);
        for ty in 0..40 {
            for tx in 0..40 {
                grid.fill_tile(tx, ty, true);
            }
        }
        // A wall of closed tiles between them, open below.
        for ty in 20..30 {
            grid.fill_tile(31, ty, false);
        }
        let env = Env { grid: Some(&grid), selected: Who::Leader, ..env() };
        let mut place = Place::new();
        let man = place.put(CharClass::Mercenary, 0, Point::new(950, 900));
        place.people.get_mut(man).unwrap().employer = Some(0);
        place.people.take_in_hand(man);
        let foe = place.put(CharClass::Mercenary, 1, Point::new(1050, 900));
        place.moves.approach.push((Who::Person(man), Who::Person(foe), 10.0));
        place.moves.fresh.push(Who::Person(man));
        let mut lowest = 0;
        for _ in 0..400 {
            place.step(&env);
            lowest = lowest.max(place.at(man).y);
            if place.at(man).x > 1010 {
                break;
            }
        }
        assert!(place.at(man).x > 1010, "never got round: {:?}", place.at(man));
        assert!(lowest >= 960, "went round below the wall: lowest {lowest}");
    }
}
