//! **`Character.Die` for everybody a machine player's fight left at
//! nought**, settled once the fight is over (the fight runs on copies of the
//! fighters). One rule for both v1.2 machine seats, the Amiga's and the
//! Modern one: out of their group or camp, their things on the ground where
//! they stood (`DropPosition = Position`), and a person due back after `npc_respawn` days. A
//! camp whose last camper fell is nobody's; v1.2 does not clear its rooms. A
//! leader's pack falls too; putting his player out is the day's
//! (`World.Turn`).
//!
//! The order is the seat's own group, the humans' here, the other machine
//! players' groups here, then the camp — each from its last member back.

use alloc::vec::Vec;

use crate::game::{companion_fell, Camps, Death, ItemBag, Party, Pending, Point, Respawns, MAX_HEALTH};
use crate::hire::People;
use crate::rival::Rival;
use crate::spawn::Dropped;

/// Whose a fallen person was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A companion in this player's group.
    Party(usize),
    /// A camper; the place's holder before the fight, if it had one.
    Camp(Option<usize>),
}

/// One person who fell, and where his things went on the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct Fell {
    pub source: Source,
    /// Indices into the ground.
    pub drops: Vec<usize>,
}

/// Who fell, as a fight names him for the burial: v1.2's `Die` happens at
/// the blow, so the order a fight's blows brought people down is the order
/// their things reach the ground and their returns are scheduled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A member of a player's group, by his identity there.
    Member { player: usize, id: usize },
    /// A camper of the camp here, by his identity there.
    Camper { id: usize },
    /// A player's leader.
    Leader(usize),
}

/// What a burial settled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Buried {
    /// Companions and campers, in the order they were settled.
    pub people: Vec<Fell>,
    /// Machine leaders who fell, by player, with their drops, in the order
    /// they were settled.
    pub leaders: Vec<(usize, Vec<usize>)>,
}

/// Everything a burial touches.
pub struct Here<'a, 'b> {
    pub location: usize,
    /// Where a machine leader stands: `Player.Travel` puts him on the entry
    /// point, and nothing moves him.
    pub entry: Point,
    /// The seat whose fight it was.
    pub rival: &'a mut Rival,
    /// The human groups standing here, in seat order: each one's seat and
    /// party.
    pub humans: Vec<(usize, &'a mut Party)>,
    /// The human leaders the fight brought down: seat, pack, and where he
    /// stands (`DropPosition = Position`).
    pub human_leaders: Vec<(usize, &'a mut ItemBag, Point)>,
    /// The other machine players, wherever they are.
    pub others: &'a mut [&'b mut Rival],
    pub camps: &'a mut Camps,
    pub ground: &'a mut Vec<Dropped>,
    pub people: &'a mut People,
    pub respawns: &'a mut Respawns,
    pub npc_respawn: i32,
    /// The order the fight's blows brought them down. Whoever it does not
    /// name follows, in the order below.
    pub order: Vec<Key>,
}

/// One body to settle.
enum Body<'a> {
    Person(Source, Point, Death),
    MachineLeader(usize, ItemBag),
    HumanLeader(&'a mut ItemBag, Point),
}

pub fn bury(h: Here) -> Buried {
    let Here { location: here, entry, rival, humans, human_leaders, others, camps, ground, people, respawns, npc_respawn, order } = h;
    let mut bodies: Vec<(Key, Body)> = Vec::new();
    {
        let mut parties: Vec<(usize, &mut Party)> = Vec::new();
        parties.push((rival.player, &mut rival.party));
        parties.extend(humans);
        for r in others.iter_mut().filter(|r| r.at == here && r.journey.is_none()) {
            parties.push((r.player, &mut r.party));
        }
        for (player, party) in parties {
            let mut i = party.companions.len();
            while i > 0 {
                i -= 1;
                let stood = party.companions[i].at;
                if let Some(d) = companion_fell(party, i) {
                    bodies.push((Key::Member { player, id: d.id }, Body::Person(Source::Party(player), stood, d)));
                }
            }
        }
    }
    if let Some(camp) = camps.at(here) {
        let holder = camp.owner;
        let mut i = camp.residents.len();
        while i > 0 {
            i -= 1;
            if camp.residents[i].needs.health > 0 {
                continue;
            }
            let mut who = camp.residents.remove(i);
            bodies.push((Key::Camper { id: who.id }, Body::Person(Source::Camp(holder), who.at, Death {
                id: who.id,
                origin: who.origin,
                at: Some(here),
                dropped: core::mem::replace(&mut who.pack, ItemBag::new(0)),
                camp_lost: false,
                food: who.needs.food,
                water: who.needs.water,
                position: who.at,
            })));
        }
        if camp.is_empty() {
            camp.owner = None;
        }
    }
    for (seat, pack, stood) in human_leaders {
        bodies.push((Key::Leader(seat), Body::HumanLeader(pack, stood)));
    }
    for r in others.iter_mut().filter(|r| r.at == here && r.journey.is_none()) {
        if !r.dead && r.needs.health <= 0 {
            let capacity = r.pack.capacity();
            bodies.push((Key::Leader(r.player), Body::MachineLeader(r.player, core::mem::replace(&mut r.pack, ItemBag::new(capacity)))));
        }
    }
    if rival.needs.health <= 0 && !rival.dead {
        let capacity = rival.pack.capacity();
        bodies.push((Key::Leader(rival.player), Body::MachineLeader(rival.player, core::mem::replace(&mut rival.pack, ItemBag::new(capacity)))));
    }
    // `Die` at the blow: the fight's order first; a stable sort keeps the
    // rest as they are.
    bodies.sort_by_key(|(k, _)| order.iter().position(|o| o == k).unwrap_or(usize::MAX));

    let mut out = Buried::default();
    let fall = |pack: &ItemBag, at: Point, ground: &mut Vec<Dropped>| -> Vec<usize> {
        let mut drops = Vec::new();
        for (i, kind) in pack.iter().enumerate() {
            drops.push(ground.len());
            ground.push(Dropped { location: here, at, kind, rounds: pack.rounds_left(i), icon: crate::spawn::PILE_UNDRAWN });
        }
        drops
    };
    for (_, body) in bodies {
        match body {
            Body::Person(source, stood, d) => {
                let drops = fall(&d.dropped, stood, ground);
                if let Some(record) = d.origin {
                    // The body stays in the place's list, a companion's
                    // put there by `Dismiss` (`Location = Player.Location`).
                    match source {
                        Source::Party(_) => people.died_with_group(record, here),
                        Source::Camp(_) => people.died_in_camp(record),
                    };
                    respawns.schedule(Pending { record, location: here, days: npc_respawn, food: d.food, water: d.water, revive_health: MAX_HEALTH });
                }
                out.people.push(Fell { source, drops });
            }
            Body::MachineLeader(player, pack) => out.leaders.push((player, fall(&pack, entry, ground))),
            Body::HumanLeader(pack, stood) => {
                let capacity = pack.capacity();
                let had = core::mem::replace(pack, ItemBag::new(capacity));
                fall(&had, stood, ground);
            }
        }
    }
    out
}
