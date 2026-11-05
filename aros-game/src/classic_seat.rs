//! The world as Classic's computer player sees and changes it: the game's own
//! data behind `burngfx::classic_ai::AiWorld`.
//!
//! Every method does what the v0.9.1 method it is named after does, with the
//! port's types. The rules — which method runs when — are in
//! `burngfx::classic_ai`; this file only carries them out.

use alloc::string::String;
use alloc::vec::Vec;

use burngfx::classic_ai::{self, AiWorld, Camper};
use burngfx::game::{Camps, ItemBag, Point, RoomItems, Springs};
use burngfx::gamdat::{CharClass, Danger, Gamdat};
use burngfx::hire::People;
use burngfx::items::ItemTypes;
use burngfx::matmap::RoomType;
use burngfx::production::Productions;
use burngfx::rival::Rival;
use burngfx::spawn::{Dropped, Rng};
use burngfx::trade::Prices;
use burngfx::zei::WalkGrid;

/// One machine player's turn, with the world lent to it.
pub struct Seat<'a> {
    pub rival: &'a mut Rival,
    pub day: u32,
    pub difficulty: i32,
    pub gam: &'a Gamdat,
    pub people: &'a mut People,
    pub camps: &'a mut Camps,
    pub rooms: &'a mut RoomItems,
    pub ground: &'a mut Vec<Dropped>,
    pub springs: &'a mut Springs,
    pub types: &'a ItemTypes,
    pub productions: &'a Productions,
    pub prices: &'a Prices,
    pub room_ids: &'a [Vec<u8>],
    pub walk: &'a [Option<WalkGrid>],
    /// The human seats: `MaxHumanCampCount` asks every player without an
    /// `AiState`.
    pub humans: &'a [usize],
    /// What happened, for the console.
    pub log: Vec<String>,
}

impl Seat<'_> {
    fn entry(&self, location: usize) -> Point {
        self.gam.locations.get(location).map_or(Point::new(0, 0), |l| l.entry_point)
    }

    fn capacity(&self, location: usize, room: usize) -> usize {
        self.room_ids
            .get(location)
            .and_then(|ids| ids.get(room))
            .map_or(0, |&id| RoomType::from_room_id(id).item_capacity())
    }

    fn is_water(&self, location: usize, room: usize) -> bool {
        self.room_ids
            .get(location)
            .and_then(|ids| ids.get(room))
            .is_some_and(|&id| RoomType::from_room_id(id) == RoomType::WaterSource)
    }

    /// `GetHireableNpc`: the first person here, in the world's order, who is
    /// not dead, works for nobody, and is a person and not a trader.
    fn hireable(&self) -> Option<usize> {
        let here = self.rival.at;
        self.people.at(here).find(|&i| {
            self.people.get(i).is_some_and(|p| {
                p.employer.is_none()
                    && !matches!(p.class, CharClass::Dog | CharClass::Mutant | CharClass::Trader)
            })
        })
    }

    /// `StoreItemRandom` at this seat's location — see [`store_item_random`].
    fn store_random(&mut self, kind: usize, rng: &mut Rng) {
        store_item_random(
            self.rival.at, kind, self.room_ids, self.walk, self.rooms, self.ground, self.springs,
            self.types, rng,
        );
    }
}

/// **`Location.StoreItemRandom`**: a random room among those with space —
/// never the last of them unless it is the only one, because the source
/// draws `Random.Next(0, rooms.Count - 1)` — or, with none, dropped on a
/// random walkable spot. A room that is the spring fills a container on the
/// way.
#[allow(clippy::too_many_arguments)]
pub fn store_item_random(
    here: usize,
    kind: usize,
    room_ids: &[Vec<u8>],
    walk: &[Option<WalkGrid>],
    rooms: &mut RoomItems,
    ground: &mut Vec<Dropped>,
    springs: &mut Springs,
    types: &ItemTypes,
    rng: &mut Rng,
) {
    let capacity = |room: usize| {
        room_ids.get(here).and_then(|ids| ids.get(room)).map_or(0, |&id| RoomType::from_room_id(id).item_capacity())
    };
    let is_water = |room: usize| {
        room_ids
            .get(here)
            .and_then(|ids| ids.get(room))
            .is_some_and(|&id| RoomType::from_room_id(id) == RoomType::WaterSource)
    };
    let count = room_ids.get(here).map_or(0, |ids| ids.len());
    let open: Vec<usize> = (0..count)
        .filter(|&r| {
            let cap = capacity(r);
            cap > 0 && rooms.get(here, r).is_none_or(|b| b.len() < cap)
        })
        .collect();
    if open.is_empty() {
        if let Some(Some(grid)) = walk.get(here) {
            if let Some(at) = burngfx::spawn::walkable_spot(grid, rng) {
                ground.push(Dropped { location: here, at, kind, rounds: None, icon: burngfx::spawn::PILE_UNDRAWN });
            }
        }
        return;
    }
    let i = if open.len() <= 1 { 0 } else { rng.below(open.len() as u32 - 1) as usize };
    let room = open[i];
    let mut kind = kind;
    if is_water(room) {
        if let (Some(full), Some(spring)) = (types.full_of(kind), springs.at(here)) {
            let water = types.water_of(full);
            if spring.reserve >= water {
                kind = full;
                spring.reserve -= water;
            }
        }
    }
    let cap = capacity(room);
    rooms.get_or_insert(here, room, || ItemBag::new(cap)).push(kind);
}

impl AiWorld for Seat<'_> {
    fn day(&self) -> u32 {
        self.day
    }
    fn here(&self) -> usize {
        self.rival.at
    }
    fn previous(&self) -> Option<usize> {
        self.rival.previous
    }
    fn me(&self) -> usize {
        self.rival.player
    }
    fn is_city(&self, l: usize) -> bool {
        self.gam.locations.get(l).is_some_and(|x| x.is_city)
    }
    fn owner(&self, l: usize) -> Option<usize> {
        self.camps.get(l).and_then(|c| c.owner)
    }
    fn ways(&self, l: usize) -> Vec<(usize, u8)> {
        self.gam
            .locations
            .get(l)
            .map_or(Vec::new(), |x| x.roads.iter().map(|r| (r.neighbour, r.days)).collect())
    }
    fn products(&self, l: usize) -> Vec<String> {
        let Some(loc) = self.gam.locations.get(l) else { return Vec::new() };
        loc.available_products
            .iter()
            .flatten()
            .filter_map(|&i| self.productions.get(i))
            .map(|p| p.produce.clone())
            .collect()
    }
    fn danger(&self, l: usize) -> Danger {
        self.gam.locations.get(l).map_or(Danger::None, |x| x.danger)
    }
    fn produce_of(&self, tool: &str) -> Option<String> {
        self.productions.iter().find(|p| p.worked_by(tool)).map(|p| p.produce.clone())
    }
    fn camp_count(&self) -> usize {
        let me = self.rival.player;
        self.camps.iter().filter(|(_, c)| c.owner == Some(me)).count()
    }
    fn max_human_camp_count(&self) -> usize {
        self.humans
            .iter()
            .map(|&h| self.camps.iter().filter(|(_, c)| c.owner == Some(h)).count())
            .max()
            .unwrap_or(0)
    }
    fn group_count(&self) -> usize {
        1 + self.rival.party.len()
    }
    fn can_hire(&self) -> bool {
        self.hireable().is_some()
    }

    fn clear_ground(&mut self) -> Vec<String> {
        let here = self.rival.at;
        let mut took = Vec::new();
        self.ground.retain(|d| {
            if d.location == here {
                if let Some(name) = self.types.name_of(d.kind) {
                    took.push(String::from(name));
                }
                false
            } else {
                true
            }
        });
        took
    }

    fn hire(&mut self, weapon: Option<&str>) {
        let Some(who) = self.hireable() else { return };
        let player = self.rival.player;
        let outfit = self.people.outfit(player);
        let Some(person) = self.people.get_mut(who) else { return };
        let accepts = person.accepts.clone();
        let Some(hire_item) = classic_ai::ai_pays(&mut self.rival.pack, &accepts) else { return };
        person.hired = true;
        person.employer = Some(player);
        person.wear(outfit);
        // `ch.Items.Clear()`: what they carried is destroyed, not dropped.
        person.pack = ItemBag::new(burngfx::game::PACK_CAPACITY);
        let needs = classic_ai::hired_needs(person.needs, hire_item, self.types, self.difficulty);
        let (record, experience, stood) = (person.record, person.experience, person.at);
        let weapon = weapon.and_then(|w| self.types.index_of(w)).map(|k| {
            let is_weapon = self.types.get(k).is_some_and(|t| t.is_class("weapon"));
            (k, is_weapon)
        });
        classic_ai::armed_recruit(&mut self.rival.party, needs, experience, record, stood, weapon);
        let mut line = String::from("hired a man");
        if let Some(w) = weapon.and_then(|(k, _)| self.types.name_of(k)) {
            line.push_str(", armed with ");
            line.push_str(w);
        }
        self.log.push(line);
    }

    fn join_camp(&mut self, who: Camper, trap: Option<&str>, rng: &mut Rng) {
        let here = self.rival.at;
        let player = self.rival.player;
        let idx = match who {
            Camper::JustHired => self.rival.party.len().saturating_sub(1),
            Camper::First => 0,
        };
        let Some(c) = self.rival.party.companions.get(idx) else { return };
        let (origin, stood) = (c.origin, c.at);
        if !self.rival.party.leave(self.camps, here, player, idx) {
            return;
        }
        if let Some(record) = origin {
            self.people.guarding(record, player, here, stood);
        }
        let mut line = String::from("camped");
        if let Some(kind) = trap.and_then(|t| self.types.index_of(t)) {
            let has_rooms = self.room_ids.get(here).is_some_and(|ids| !ids.is_empty());
            if has_rooms {
                self.store_random(kind, rng);
            } else if let Some(camper) =
                self.camps.at(here).and_then(|camp| camp.residents.last_mut())
            {
                camper.pack.push(kind);
            }
            line.push_str(", with a ");
            line.push_str(trap.unwrap_or(""));
        }
        self.log.push(line);
    }

    fn travel(&mut self, to: usize) {
        if let Ok(j) = burngfx::game::depart(self.gam, self.rival.journey, self.rival.at, to) {
            self.rival.journey = Some(j);
            self.rival.previous = Some(self.rival.at);
            // `chr.Position = destination.EntryPoint` for the whole group.
            let entry = self.entry(to);
            for c in self.rival.party.companions.iter_mut() {
                c.at = entry;
                c.move_to = entry;
            }
        }
    }

    fn refresh(&mut self) {
        classic_ai::refresh_reserves(&mut self.rival.pack, self.types);
        classic_ai::refresh_attributes(&mut self.rival.needs, &mut self.rival.party.companions);
    }

    fn water_goal(&mut self) {
        let here = self.rival.at;
        let leader_at = self.entry(here);
        let count = self.room_ids.get(here).map_or(0, |ids| ids.len());
        let water_room = (0..count).find(|&r| self.is_water(here, r));
        let cap = water_room.map_or(0, |r| self.capacity(here, r));
        let types = self.types;
        let prices = self.prices;
        let worth = |k: usize| burngfx::trade::worth_of(k, types, prices);
        let room = match water_room {
            Some(r) => Some(self.rooms.get_or_insert(here, r, || ItemBag::new(cap))),
            None => None,
        };
        let moved = classic_ai::water_goal(
            &mut self.rival.needs,
            leader_at,
            &mut self.rival.pack,
            &mut self.rival.party.companions,
            self.springs.at(here),
            room,
            types,
            &worth,
        );
        if moved > 0 {
            let mut line = String::from("carried ");
            crate::push_num(&mut line, moved as u32);
            line.push_str(" thing(s) into the water room");
            self.log.push(line);
        }
    }
}
