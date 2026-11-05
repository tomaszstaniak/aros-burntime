//! The world as v1.2's Amiga opponent sees and changes it: the game's own
//! data behind [`burngfx::amiga_ai::AmigaWorld`].
//!
//! Every method does what the v1.2 code it is named after does, with the
//! port's types; the decisions are in `burngfx::amiga_ai`. A duel takes
//! health, and the one thing a death changes at the blow in v1.2 that the
//! rest of the turn reads — a camp whose last camper falls is nobody's
//! (`Die` → `Dismiss` → `IsLastInCamp`, `Character.cs:206-209, 433-446`) —
//! happens at the blow; the caller settles the rest of whoever is left with
//! none (their things, their return) once the turn is over, the way every
//! other death in the game is settled.

use alloc::string::String;
use alloc::vec::Vec;

use burngfx::amiga_ai::{self, AmigaWorld, Benchmark, Duelist};
use burngfx::combat::Kit;
use burngfx::game::{Camps, CharacterNeeds, Companion, ItemBag, Party, Respawns, RoomItems, Springs};
use burngfx::gamdat::{CharClass, Danger, Gamdat};
use burngfx::hire::People;
use burngfx::items::ItemTypes;
use burngfx::matmap::RoomType;
use burngfx::production::Productions;
use burngfx::rival::Rival;
use burngfx::rules::RuleBook;
use burngfx::spawn::Dropped;

/// A human's group standing where the seat is.
pub struct HumanHere<'a> {
    pub seat: usize,
    pub needs: &'a mut CharacterNeeds,
    /// His part of a danger's point not yet spent, which a blow spends.
    pub exposure: &'a mut burngfx::danger::Exposure,
    pub pack: &'a mut ItemBag,
    /// The leader's chosen weapon and protection: the duel starts from his
    /// weapon and writes the one it finds back (`SelectOriginalWeapon`).
    pub kit: &'a mut Kit,
    pub experience: i32,
    pub party: &'a mut Party,
    /// Where his leader stands (`Position`), for `Die`'s drop.
    pub stood: burngfx::game::Point,
}

pub struct Seat<'a> {
    pub rival: &'a mut Rival,
    pub day: i32,
    pub gam: &'a Gamdat,
    pub people: &'a mut People,
    pub camps: &'a mut Camps,
    pub rooms: &'a mut RoomItems,
    pub ground: &'a mut Vec<Dropped>,
    pub springs: &'a mut Springs,
    pub types: &'a ItemTypes,
    pub productions: &'a Productions,
    pub book: &'a RuleBook,
    pub room_ids: &'a [Vec<u8>],
    /// `BaseExperience`: what `RecalculateExperience` adds camps to.
    pub base_experience: i32,
    /// Living human seats, and where each is and whether on the road.
    pub humans: &'a [(usize, usize, bool)],
    /// The other machine seats.
    pub others: Vec<&'a mut Rival>,
    /// Every human group standing here, in seat order: all of them defend
    /// (`GetCampDefenders`, `AI/Profiles/AiStateOperations.cs:60-70`).
    pub humans_here: Vec<HumanHere<'a>>,
    pub respawns: &'a mut Respawns,
    /// `npc_respawn`: the days before a dead person comes back.
    pub npc_respawn: i32,
    pub log: Vec<String>,
    /// A camp here lost its last camper to a blow of this turn's duel, and
    /// was freed then.
    pub freed_at_blow: bool,
    /// Who the duel brought down, in the order of the blows (`Die` at the
    /// blow), for the burial.
    pub fell_order: Vec<burngfx::burial::Key>,
}

const CAMPER: usize = 1000;
const HUMAN: usize = 2000;
const OTHER: usize = 3000;

impl Seat<'_> {
    fn here_(&self) -> usize {
        self.rival.at
    }

    fn room_count(&self, location: usize) -> usize {
        self.room_ids.get(location).map_or(0, |ids| ids.len())
    }

    fn capacity(&self, location: usize, room: usize) -> usize {
        self.room_ids.get(location).and_then(|ids| ids.get(room)).map_or(0, |&id| RoomType::from_room_id(id).item_capacity())
    }

    fn is_water(&self, location: usize, room: usize) -> bool {
        self.room_ids
            .get(location)
            .and_then(|ids| ids.get(room))
            .is_some_and(|&id| RoomType::from_room_id(id) == RoomType::WaterSource)
    }

    fn has_space(&self, location: usize, room: usize) -> bool {
        let cap = self.capacity(location, room);
        cap > 0 && self.rooms.get(location, room).is_none_or(|b| b.len() < cap)
    }

    /// The residents of this seat's camp here, as indices into it.
    fn my_campers(&self, location: usize) -> usize {
        let me = self.rival.player;
        self.camps
            .get(location)
            .filter(|c| c.owner == Some(me))
            .map_or(0, |c| c.residents.iter().filter(|r| r.needs.health > 0).count())
    }

    fn class_of(&self, record: Option<usize>) -> CharClass {
        record.and_then(|r| self.people.get_by_record(r)).map_or(CharClass::Mercenary, |p| p.class)
    }

    /// `SelectOriginalWeapon(allowUnloadedRifle: true)?.ID`.
    fn weapon(&self, pack: &ItemBag, chosen: Option<usize>, human: bool) -> Option<String> {
        burngfx::table_combat::find_original_weapon(pack, chosen, human, true, self.types)
            .and_then(|s| pack.get(s))
            .and_then(|k| self.types.name_of(k))
            .map(String::from)
    }

    fn enemy_camp_here(&self) -> Option<&burngfx::game::Camp> {
        let me = self.rival.player;
        self.camps.get(self.here_()).filter(|c| c.owner.is_some_and(|o| o != me))
    }
}

impl AmigaWorld for Seat<'_> {
    fn day(&self) -> i32 {
        self.day
    }
    fn me(&self) -> usize {
        self.rival.player
    }
    fn here(&self) -> usize {
        self.rival.at
    }
    fn travelling(&self) -> bool {
        self.rival.journey.is_some()
    }
    fn dead(&self) -> bool {
        self.rival.dead || self.rival.needs.health <= 0
    }
    fn is_city(&self, l: usize) -> bool {
        self.gam.locations.get(l).is_some_and(|x| x.is_city)
    }
    fn owner(&self, l: usize) -> Option<usize> {
        self.camps.get(l).and_then(|c| c.owner)
    }
    fn neighbors(&self, l: usize) -> Vec<usize> {
        self.gam.locations.get(l).map_or(Vec::new(), |x| x.roads.iter().map(|r| r.neighbour).collect())
    }
    /// `CanTravel`: from where it stands, down a road, and out of somebody
    /// else's place only the way it came.
    fn can_travel(&self, from: usize, to: usize) -> bool {
        if self.rival.journey.is_some() || from != self.rival.at {
            return false;
        }
        if self.owner(from).is_some_and(|o| o != self.rival.player) && self.rival.previous != Some(to) {
            return false;
        }
        self.gam.locations.get(from).is_some_and(|x| x.roads.iter().any(|r| r.neighbour == to && r.days > 0))
    }
    fn previous(&self) -> Option<usize> {
        self.rival.previous
    }
    fn production(&self, l: usize) -> Option<usize> {
        self.camps.get(l).and_then(|c| c.production)
    }
    fn production_valid(&self, l: usize, p: usize) -> bool {
        self.gam.locations.get(l).is_some_and(|x| x.available_products.iter().flatten().any(|&q| q == p))
    }
    /// Tools installed in the rooms (v1.2's productions do not count packs).
    fn production_tools(&self, l: usize) -> usize {
        let Some(p) = self.production(l).and_then(|i| self.productions.get(i)) else { return 0 };
        (0..self.room_count(l))
            .filter_map(|r| self.rooms.get(l, r))
            .flat_map(|b| b.iter())
            .filter(|&k| self.types.name_of(k).is_some_and(|n| p.worked_by(n)))
            .count()
    }
    fn water_output(&self, l: usize) -> i32 {
        let base = self.springs.get(l).map_or(0, |s| s.refresh);
        let water_room = (0..self.room_count(l)).find(|&r| self.is_water(l, r));
        let has = |name: &str| {
            water_room
                .and_then(|r| self.rooms.get(l, r))
                .is_some_and(|b| b.iter().any(|k| self.types.name_of(k) == Some(name)))
        };
        let industrial = has("item_industrial_pump");
        burngfx::v12_day::water_output(self.book.rules.water_output, base, !industrial && has("item_hand_pump"), industrial)
    }
    fn danger(&self, l: usize) -> Option<&'static str> {
        match self.gam.locations.get(l).map(|x| x.danger) {
            Some(Danger::Gas) => Some("gas"),
            Some(Danger::Radiation) => Some("radiation"),
            _ => None,
        }
    }
    fn stationed(&self, l: usize) -> usize {
        self.my_campers(l)
    }
    fn owned_camps(&self) -> usize {
        let me = self.rival.player;
        self.camps.iter().filter(|(_, c)| c.owner == Some(me)).count()
    }
    /// `GetProgressBenchmark`: the living humans; with none, the other living
    /// machine players; with neither, no restriction.
    fn benchmark(&self) -> Benchmark {
        let camps_of = |p: usize| self.camps.iter().filter(|(_, c)| c.owner == Some(p)).count();
        let humans: Vec<usize> = self.humans.iter().map(|&(s, _, _)| s).collect();
        let candidates: Vec<usize> = if !humans.is_empty() {
            humans
        } else {
            self.others.iter().filter(|r| !r.dead).map(|r| r.player).collect()
        };
        match candidates.iter().map(|&p| camps_of(p)).max() {
            Some(strongest) => Benchmark { restricts: true, strongest },
            None => Benchmark { restricts: false, strongest: 0 },
        }
    }
    fn group_size(&self) -> usize {
        1 + self.rival.party.len()
    }
    fn leader_food(&self) -> i32 {
        self.rival.needs.food
    }

    fn fill_water(&mut self) {
        self.rival.needs.water = self.rival.needs.max_water;
        for c in self.rival.party.companions.iter_mut() {
            c.needs.water = c.needs.max_water;
        }
    }

    fn recalculate_experience(&mut self) {
        let camps = self.owned_camps();
        self.rival.experience = burngfx::v12_day::camp_boss_experience(self.base_experience, camps);
    }

    /// `RecoverGroup(arrived: true)`: `ApplyArrivalRecovery`, then
    /// `CleanupAmiga` over the ground here and — in a free or own place —
    /// the rooms.
    fn recover_group(&mut self) {
        let here = self.here_();
        let me = self.rival.player;
        let owner = self.owner(here);
        let left_hostile = self.rival.previous.and_then(|p| self.owner(p)).is_some_and(|o| o != me);
        let (food, health) = amiga_ai::arrival_recovery(self.is_city(here), left_hostile);
        let fed = owner.is_none_or(|o| o == me);
        let mut all: Vec<&mut CharacterNeeds> = Vec::new();
        all.push(&mut self.rival.needs);
        for c in self.rival.party.companions.iter_mut() {
            all.push(&mut c.needs);
        }
        for n in all {
            if fed {
                n.food = (n.food + food).min(9);
            }
            n.health = (n.health + health).min(100);
        }

        // The party as `AllCharacters` orders it: the leader (a player
        // record), then the others by record.
        let mut order: Vec<Option<usize>> = Vec::new();
        order.push(None);
        let mut hired: Vec<(usize, usize)> = self
            .rival
            .party
            .companions
            .iter()
            .enumerate()
            .filter(|(_, c)| c.needs.health > 0)
            .map(|(i, c)| (c.origin.unwrap_or(usize::MAX), i))
            .collect();
        hired.sort();
        order.extend(hired.into_iter().map(|(_, i)| Some(i)));
        let tool = amiga_ai::production_tool_title(self.production(here));

        // The ground first, then the rooms when the place is free or its own.
        let ground_at: Vec<usize> = (0..self.ground.len()).filter(|&i| self.ground[i].location == here).collect();
        let ground_titles: Vec<i32> = ground_at
            .iter()
            .map(|&i| amiga_ai::title_id(self.types.get(self.ground[i].kind).and_then(|t| t.sprite)))
            .collect();
        let mut room_at: Vec<(usize, usize)> = Vec::new();
        if owner.is_none_or(|o| o == me) {
            for r in 0..self.room_count(here) {
                if let Some(b) = self.rooms.get(here, r) {
                    for s in 0..b.len() {
                        room_at.push((r, s));
                    }
                }
            }
        }
        let room_titles: Vec<i32> = room_at
            .iter()
            .map(|&(r, s)| {
                let kind = self.rooms.get(here, r).and_then(|b| b.get(s));
                amiga_ai::title_id(kind.and_then(|k| self.types.get(k)).and_then(|t| t.sprite))
            })
            .collect();
        let free: Vec<usize> = order.iter().map(|m| 6usize.saturating_sub(self.pack_of(*m).len())).collect();
        let rooms = owner.is_none_or(|o| o == me).then_some(&room_titles[..]);
        let (from_ground, from_rooms) = amiga_ai::cleanup(&ground_titles, rooms, &free, tool);

        let mut gone: Vec<usize> = Vec::new();
        for s in from_ground {
            match s {
                amiga_ai::Sweep::Take { record, member } => {
                    let kind = self.ground[ground_at[record]].kind;
                    if self.pack_of_mut(order[member]).push(kind) {
                        gone.push(ground_at[record]);
                    }
                }
                amiga_ai::Sweep::Destroy { record } => gone.push(ground_at[record]),
            }
        }
        gone.sort_unstable();
        for i in gone.into_iter().rev() {
            self.ground.remove(i);
        }

        let mut out: Vec<(usize, usize)> = Vec::new();
        for s in from_rooms {
            match s {
                amiga_ai::Sweep::Take { record, member } => {
                    let (r, slot) = room_at[record];
                    if let Some(kind) = self.rooms.get(here, r).and_then(|b| b.get(slot)) {
                        if self.pack_of_mut(order[member]).push(kind) {
                            out.push((r, slot));
                        }
                    }
                }
                amiga_ai::Sweep::Destroy { record } => out.push(room_at[record]),
            }
        }
        out.sort_unstable();
        for (r, slot) in out.into_iter().rev() {
            let cap = self.capacity(here, r);
            self.rooms.get_or_insert(here, r, || ItemBag::new(cap)).remove(slot);
        }
    }

    /// `RecruitOneAtCurrentLocation` with `HireWithoutInitialization`: the
    /// first here, in the world's order, who is alive, nobody's, a person and
    /// not a trader, and can be hired by [`amiga_ai::can_recruit`] — paid
    /// nothing, and keeping what he has and how fed he is.
    fn recruit_one(&mut self) -> bool {
        let here = self.here_();
        let boss = self.rival.experience;
        let found = self.people.at(here).find(|&i| {
            self.people.get(i).is_some_and(|p| {
                p.employer.is_none() && !p.dead && amiga_ai::can_recruit(p.class, p.experience, boss)
            })
        });
        let Some(who) = found else { return false };
        let player = self.rival.player;
        let outfit = self.people.outfit(player);
        let Some(p) = self.people.get_mut(who) else { return false };
        p.hired = true;
        p.employer = Some(player);
        p.wear(outfit);
        // The person's things go with him rather than being copied.
        let pack = core::mem::replace(&mut p.pack, ItemBag::new(burngfx::game::PACK_CAPACITY));
        let (needs, kit, experience, record, at) = (p.needs, p.kit, p.experience, p.record, p.at);
        let id = self.rival.party.recruit();
        if let Some(c) = self.rival.party.companions.iter_mut().find(|c| c.id == id) {
            *c = Companion { needs, pack, kit, experience, origin: Some(record), at, move_to: at, ..Companion::new(id) };
        }
        true
    }

    fn defenders(&self) -> Vec<usize> {
        let here = self.here_();
        let mut out = Vec::new();
        if let Some(c) = self.enemy_camp_here() {
            out.extend(c.residents.iter().enumerate().filter(|(_, r)| r.needs.health > 0).map(|(i, _)| CAMPER + i));
        }
        // Then every enemy party standing here, in the world's order of
        // players: (seat, leader alive, companions alive, first id).
        let mut parties: Vec<(usize, bool, Vec<bool>, usize)> = Vec::new();
        for (k, h) in self.humans_here.iter().enumerate() {
            let alive = h.party.companions.iter().map(|c| c.needs.health > 0).collect();
            parties.push((h.seat, h.needs.health > 0, alive, HUMAN + 100 * k));
        }
        for (k, r) in self.others.iter().enumerate() {
            if r.dead || r.journey.is_some() || r.at != here {
                continue;
            }
            let alive = r.party.companions.iter().map(|c| c.needs.health > 0).collect();
            parties.push((r.player, r.needs.health > 0, alive, OTHER + 100 * k));
        }
        parties.sort_by_key(|p| p.0);
        for (_, leader, alive, first) in parties {
            if leader {
                out.push(first);
            }
            out.extend(alive.iter().enumerate().filter(|(_, a)| **a).map(|(i, _)| first + 1 + i));
        }
        out
    }

    fn attackers(&self) -> Vec<usize> {
        (0..=self.rival.party.len()).collect()
    }

    fn leader(&self) -> usize {
        0
    }

    fn duelist(&self, id: usize) -> Option<Duelist> {
        let mercenary = |c: CharClass| c == CharClass::Mercenary;
        if id < CAMPER {
            if id == 0 {
                let r = &self.rival;
                return Some(Duelist {
                    health: r.needs.health,
                    experience: r.experience,
                    mercenary: false,
                    weapon: self.weapon(&r.pack, r.weapon_slot(), false),
                    with_boss: true,
                });
            }
            let c = self.rival.party.companions.get(id - 1)?;
            return Some(Duelist {
                health: c.needs.health,
                experience: c.experience,
                mercenary: mercenary(self.class_of(c.origin)),
                weapon: self.weapon(&c.pack, c.kit.weapon, false),
                with_boss: true,
            });
        }
        if id < HUMAN {
            let c = self.enemy_camp_here()?.residents.get(id - CAMPER)?;
            let human = self.enemy_camp_here().and_then(|x| x.owner).is_some_and(|o| self.humans.iter().any(|&(s, _, _)| s == o));
            return Some(Duelist {
                health: c.needs.health,
                experience: c.experience,
                mercenary: mercenary(self.class_of(c.origin)),
                weapon: self.weapon(&c.pack, c.kit.weapon, human),
                with_boss: false,
            });
        }
        if id < OTHER {
            let (k, m) = ((id - HUMAN) / 100, (id - HUMAN) % 100);
            let h = self.humans_here.get(k)?;
            if m == 0 {
                return Some(Duelist {
                    health: h.needs.health,
                    experience: h.experience,
                    mercenary: false,
                    weapon: self.weapon(h.pack, h.kit.weapon, true),
                    with_boss: true,
                });
            }
            let c = h.party.companions.get(m - 1)?;
            return Some(Duelist {
                health: c.needs.health,
                experience: c.experience,
                mercenary: mercenary(self.class_of(c.origin)),
                weapon: self.weapon(&c.pack, c.kit.weapon, true),
                with_boss: true,
            });
        }
        let k = (id - OTHER) / 100;
        let m = (id - OTHER) % 100;
        let r = self.others.get(k)?;
        if m == 0 {
            return Some(Duelist {
                health: r.needs.health,
                experience: r.experience,
                mercenary: false,
                weapon: self.weapon(&r.pack, r.weapon_slot(), false),
                with_boss: true,
            });
        }
        let c = r.party.companions.get(m - 1)?;
        Some(Duelist {
            health: c.needs.health,
            experience: c.experience,
            mercenary: mercenary(self.class_of(c.origin)),
            weapon: self.weapon(&c.pack, c.kit.weapon, false),
            with_boss: true,
        })
    }

    /// `SelectOriginalWeapon(allowUnloadedRifle: true)` writes the weapon it
    /// finds back as the chosen one (`Character.cs:700-704`) — a machine
    /// player's leader's too (register AMI-8).
    fn select_weapon(&mut self, id: usize) {
        let types = self.types;
        let pick = |pack: &ItemBag, kit: &mut Kit, human: bool| {
            kit.weapon = burngfx::table_combat::find_original_weapon(pack, kit.weapon, human, true, types);
        };
        let pick_leader = |r: &mut Rival| {
            let slot = burngfx::table_combat::find_original_weapon(&r.pack, r.weapon_slot(), false, true, types);
            r.weapon_kind = slot.and_then(|s| r.pack.get(s));
        };
        if id < CAMPER {
            if id == 0 {
                pick_leader(self.rival);
            }
            if let Some(c) = id.checked_sub(1).and_then(|i| self.rival.party.companions.get_mut(i)) {
                pick(&c.pack, &mut c.kit, false);
            }
        } else if id < HUMAN {
            let here = self.rival.at;
            let human = self.enemy_camp_here().and_then(|x| x.owner).is_some_and(|o| self.humans.iter().any(|&(s, _, _)| s == o));
            if let Some(c) = self.camps.at(here).and_then(|c| c.residents.get_mut(id - CAMPER)) {
                pick(&c.pack, &mut c.kit, human);
            }
        } else if id < OTHER {
            let (k, m) = ((id - HUMAN) / 100, (id - HUMAN) % 100);
            match self.humans_here.get_mut(k) {
                Some(h) if m == 0 => pick(h.pack, h.kit, true),
                Some(h) => {
                    if let Some(c) = h.party.companions.get_mut(m - 1) {
                        pick(&c.pack, &mut c.kit, true);
                    }
                }
                None => {}
            }
        } else {
            let (k, m) = ((id - OTHER) / 100, (id - OTHER) % 100);
            if m == 0 {
                if let Some(r) = self.others.get_mut(k) {
                    pick_leader(r);
                }
            }
            if let Some(c) = m.checked_sub(1).and_then(|i| self.others.get_mut(k)?.party.companions.get_mut(i)) {
                pick(&c.pack, &mut c.kit, false);
            }
        }
    }

    fn hurt(&mut self, id: usize, amount: i32) {
        let health: Option<(&mut i32, burngfx::burial::Key)> = if id < CAMPER {
            let player = self.rival.player;
            if id == 0 {
                Some((&mut self.rival.needs.health, burngfx::burial::Key::Leader(player)))
            } else {
                self.rival.party.companions.get_mut(id - 1).map(|c| (&mut c.needs.health, burngfx::burial::Key::Member { player, id: c.id }))
            }
        } else if id < HUMAN {
            // `Die` at the blow: the last camper's `Dismiss` frees the place
            // (`IsLastInCamp`: nobody else of anybody's camped there).
            let here = self.rival.at;
            let Some(camp) = self.camps.at(here) else { return };
            let Some(r) = camp.residents.get_mut(id - CAMPER) else { return };
            let was_alive = r.needs.health > 0;
            // `defender.Health -= attack` (`AmigaAiState.cs:394,400`).
            burngfx::danger::spend(&mut r.needs.health, &mut r.exposure);
            r.needs.health -= amount;
            if was_alive && r.needs.health <= 0 {
                self.fell_order.push(burngfx::burial::Key::Camper { id: r.id });
                let others = camp.residents.iter().enumerate().any(|(i, o)| i != id - CAMPER && o.needs.health > 0);
                if !others && camp.stationed.is_empty() && camp.owner.is_some() {
                    camp.owner = None;
                    self.freed_at_blow = true;
                }
            }
            return;
        } else if id < OTHER {
            let (k, m) = ((id - HUMAN) / 100, (id - HUMAN) % 100);
            match self.humans_here.get_mut(k) {
                Some(h) if m == 0 => {
                    burngfx::danger::spend(&mut h.needs.health, h.exposure);
                    Some((&mut h.needs.health, burngfx::burial::Key::Leader(h.seat)))
                }
                Some(h) => {
                    let seat = h.seat;
                    h.party.companions.get_mut(m - 1).map(|c| {
                        burngfx::danger::spend(&mut c.needs.health, &mut c.exposure);
                        (&mut c.needs.health, burngfx::burial::Key::Member { player: seat, id: c.id })
                    })
                }
                None => None,
            }
        } else {
            let (k, m) = ((id - OTHER) / 100, (id - OTHER) % 100);
            match self.others.get_mut(k) {
                Some(r) if m == 0 => {
                    let player = r.player;
                    Some((&mut r.needs.health, burngfx::burial::Key::Leader(player)))
                }
                Some(r) => {
                    let player = r.player;
                    r.party.companions.get_mut(m - 1).map(|c| (&mut c.needs.health, burngfx::burial::Key::Member { player, id: c.id }))
                }
                None => None,
            }
        };
        if let Some((h, key)) = health {
            let was_alive = *h > 0;
            *h -= amount;
            // `Die` at the blow: the order of the falls is the burial's.
            if was_alive && *h <= 0 {
                self.fell_order.push(key);
            }
        }
    }

    fn free_here(&mut self) {
        let here = self.here_();
        if let Some(c) = self.camps.at(here) {
            c.owner = None;
        }
    }

    /// `JoinCamp` for the first living companion, protection made and
    /// chosen when asked for (refused with a full pack).
    fn station_first(&mut self, protection: Option<&str>) -> bool {
        let Some(idx) = self.rival.party.companions.iter().position(|c| c.needs.health > 0) else { return false };
        if let Some(name) = protection {
            let Some(kind) = self.types.index_of(name) else { return false };
            let c = &mut self.rival.party.companions[idx];
            if c.pack.is_full() {
                return false;
            }
            c.pack.push(kind);
            c.kit.protection = Some(c.pack.len() - 1);
        }
        let here = self.here_();
        let player = self.rival.player;
        let (origin, stood) = (self.rival.party.companions[idx].origin, self.rival.party.companions[idx].at);
        if !self.rival.party.leave(self.camps, here, player, idx) {
            return false;
        }
        if let Some(record) = origin {
            self.people.guarding(record, player, here, stood);
            // `JoinCamp`: a fresh `SimpleMind`, and he roams.
            self.people.joined_camp(record);
        }
        // v1.3's `JoinCamp` (`758f323`): the production refreshed as he
        // joins — before `TryStationGuard` puts a knife in and sets
        // production 0 on a claim (`AmigaAiState.cs:217-219` at `v1.3`).
        let v12 = crate::v12_world(self.book, &[], self.people, Some(self.gam));
        let storage = burngfx::modern_ai::seat::SeatStorage {
            rooms: &*self.rooms, room_ids: self.room_ids, gam: self.gam, types: self.types, productions: self.productions,
        };
        if let Some(camp) = self.camps.at(here) {
            burngfx::v12_day::refresh_production(here, camp, &storage, self.productions, self.types, &v12);
        }
        true
    }

    /// `TryCreateCampItem` → `Location.StoreItem`: the preferred room (the
    /// spring's) when it has space, else the first room with space; a
    /// container into the spring's room is filled from it.
    fn create_camp_item(&mut self, item: &str, water_room: bool) -> bool {
        let here = self.here_();
        let Some(mut kind) = self.types.index_of(item) else { return false };
        let rooms: Vec<usize> = (0..self.room_count(here)).filter(|&r| self.has_space(here, r)).collect();
        if rooms.is_empty() {
            return false;
        }
        let preferred = if water_room { (0..self.room_count(here)).find(|&r| self.is_water(here, r)) } else { None };
        let room = preferred.filter(|r| rooms.contains(r)).unwrap_or(rooms[0]);
        if self.is_water(here, room) {
            if let (Some(full), Some(spring)) = (self.types.full_of(kind), self.springs.at(here)) {
                let w = self.types.water_of(full);
                if spring.reserve >= w {
                    spring.reserve -= w;
                    kind = full;
                }
            }
        }
        let cap = self.capacity(here, room);
        self.rooms.get_or_insert(here, room, || ItemBag::new(cap)).push(kind);
        true
    }

    fn set_production(&mut self, production: usize) {
        let here = self.here_();
        if let Some(c) = self.camps.at(here) {
            c.production = Some(production);
        }
    }

    fn travel(&mut self, to: usize) {
        if let Ok(j) = burngfx::game::depart(self.gam, self.rival.journey, self.rival.at, to) {
            self.rival.journey = Some(j);
            self.rival.previous = Some(self.rival.at);
            let entry = self.gam.locations.get(to).map_or(burngfx::game::Point::new(0, 0), |l| l.entry_point);
            burngfx::game::place_at_entry(&mut self.rival.party, entry);
        }
    }

    fn report(&mut self, what: String) {
        self.log.push(what);
    }
}

impl Seat<'_> {
    fn pack_of(&self, member: Option<usize>) -> &ItemBag {
        match member {
            None => &self.rival.pack,
            Some(i) => &self.rival.party.companions[i].pack,
        }
    }
    fn pack_of_mut(&mut self, member: Option<usize>) -> &mut ItemBag {
        match member {
            None => &mut self.rival.pack,
            Some(i) => &mut self.rival.party.companions[i].pack,
        }
    }
}

/// **An Amiga seat's difficulty** (`GameCreation.cs:265-269`): the seat's
/// word in `ai.txt [players] difficulties` (`AiPolicy.SettingsForPlayer`,
/// `AI/PlayerBehavior/Core/AiPolicy.cs:61-70`), the game's for `game` or a
/// seat the file does not name. v1.2 makes no game with a computer seat
/// without the file (`AiPolicy` throws), so an unusable one is `None`.
pub fn seat_difficulty(ai_txt: Option<&[u8]>, seat: usize, game: u8) -> Option<i32> {
    let text = core::str::from_utf8(ai_txt?).ok()?;
    let config = burngfx::modern_ai::policy::AiConfig::parse(text).ok()?;
    Some(config.difficulty_for(seat, game) as i32)
}

/// Who a turn's duel left dead, in the order they were settled.
#[derive(Default)]
pub struct Fallen {
    /// Companions and campers, anybody's.
    pub people: usize,
    /// The human leaders here who fell, by seat.
    pub human_leaders: Vec<usize>,
    /// Other machine players' leaders who fell, by player.
    pub leaders: Vec<usize>,
    /// Whose each of `people` was.
    pub who: Vec<String>,
}

impl Seat<'_> {
    /// **`Character.Die` for everybody the duel left at nought**: out of
    /// their group or camp, their things on the ground where each stood (`burial::bury`), and a
    /// person due back after `npc_respawn` days. A camp whose last camper
    /// fell is nobody's (the duel frees it at the blow); v1.2 does not clear
    /// its rooms. A leader's pack falls too; putting his player out is the
    /// day's (`World.Turn`).
    pub fn bury_the_fallen(&mut self) -> Fallen {
        let here = self.rival.at;
        let entry = self.gam.locations.get(here).map_or(burngfx::game::Point::new(0, 0), |l| l.entry_point);
        let mut humans = Vec::new();
        let mut human_leaders = Vec::new();
        for h in self.humans_here.iter_mut() {
            if h.needs.health <= 0 {
                human_leaders.push((h.seat, &mut *h.pack, h.stood));
            }
            humans.push((h.seat, &mut *h.party));
        }
        let b = burngfx::burial::bury(burngfx::burial::Here {
            location: here,
            entry,
            rival: self.rival,
            humans,
            human_leaders,
            order: core::mem::take(&mut self.fell_order),
            others: &mut self.others,
            camps: self.camps,
            ground: self.ground,
            people: self.people,
            respawns: self.respawns,
            npc_respawn: self.npc_respawn,
        });
        // A camp freed at the blow may be this seat's by now; its fallen were
        // the campers of a place the duel freed.
        let freed = self.freed_at_blow;
        let who = b
            .people
            .iter()
            .map(|f| match f.source {
                burngfx::burial::Source::Party(p) => alloc::format!("a companion of player {p}"),
                burngfx::burial::Source::Camp(Some(p)) if !freed => alloc::format!("a camper of player {p}"),
                burngfx::burial::Source::Camp(_) => String::from("a camper of a place now free"),
            })
            .collect();
        Fallen {
            people: b.people.len(),
            human_leaders: self.humans_here.iter().filter(|h| h.needs.health <= 0).map(|h| h.seat).collect(),
            leaders: b.leaders.iter().map(|l| l.0).collect(),
            who,
        }
    }
}

/// The seat against the real map, Classic's rules and v1.2's item table:
/// the duel's rules as `AmigaAiState.ResolveCurrentOpposition` and
/// `GetCampDefenders` have them.
#[cfg(test)]
mod duel {
    use super::*;
    use burngfx::game::{Companion, Respawns};
    use burngfx::spawn::Rng;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../burngfx/testdata");
    const AMIGA: usize = 2;

    fn data() -> &'static (Gamdat, ItemTypes, RuleBook, Productions) {
        static DATA: std::sync::OnceLock<(Gamdat, ItemTypes, RuleBook, Productions)> = std::sync::OnceLock::new();
        DATA.get_or_init(|| {
            let rules = |f: &str| std::fs::read_to_string(format!("{DIR}/v1.2/rules/{f}")).unwrap();
            let types = ItemTypes::parse_v12(&rules("dos/items.txt")).unwrap();
            let book = RuleBook::parse(&rules("classic/game.txt"));
            let bytes = std::fs::read(format!("{DIR}/burntime.pak")).expect("testdata pack");
            let pak = burngfx::pak::ClassicPak::open(&bytes).unwrap();
            let gam = burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat").unwrap()).unwrap();
            (gam, types, book, Productions::parse(""))
        })
    }

    /// A human group: its leader's needs, pack and choice, and his party.
    struct Group {
        seat: usize,
        needs: CharacterNeeds,
        pack: ItemBag,
        kit: Kit,
        party: Party,
        exposure: burngfx::danger::Exposure,
    }

    impl Group {
        fn new(seat: usize, health: i32) -> Group {
            let mut needs = CharacterNeeds::at_game_start();
            needs.health = health;
            Group { seat, needs, pack: ItemBag::new(6), kit: Kit::default(), party: Party::new(), exposure: Default::default() }
        }
    }

    /// Everything the seat borrows: player 2's Amiga group (its leader and
    /// one man) at `here`, the first place that is no city, has no danger
    /// and a road out; it came down that road.
    struct Fixture {
        here: usize,
        rival: Rival,
        others: Vec<Rival>,
        people: People,
        camps: Camps,
        rooms: RoomItems,
        ground: Vec<Dropped>,
        springs: Springs,
        room_ids: Vec<Vec<u8>>,
        humans: Vec<(usize, usize, bool)>,
        groups: Vec<Group>,
        respawns: Respawns,
    }

    impl Fixture {
        fn new() -> Fixture {
            let (gam, ..) = data();
            let (here, back) = (0..gam.locations.len())
                .find_map(|l| {
                    let loc = &gam.locations[l];
                    let road = loc.roads.iter().find(|r| r.days > 0)?;
                    (!loc.is_city && loc.danger == Danger::None).then_some((l, road.neighbour))
                })
                .unwrap();
            let mut rival = Rival::new(AMIGA, here);
            rival.previous = Some(back);
            rival.needs.health = 100;
            rival.party.recruit();
            rival.party.companions[0].needs.health = 100;
            let mut room_ids = vec![Vec::new(); gam.locations.len()];
            room_ids[here] = vec![0];
            Fixture {
                here,
                rival,
                others: Vec::new(),
                people: People::default(),
                camps: Camps::new(),
                rooms: RoomItems::default(),
                ground: Vec::new(),
                springs: Springs::new(),
                room_ids,
                humans: Vec::new(),
                groups: Vec::new(),
                respawns: Respawns::default(),
            }
        }

        /// `owner`'s camp here, its campers at these healths.
        fn camp(&mut self, owner: usize, healths: &[i32]) {
            for (i, &h) in healths.iter().enumerate() {
                let mut c = Companion::new(100 + i);
                c.needs.health = h;
                self.camps.join(self.here, owner, c);
            }
        }

        /// A human group standing here.
        fn human(&mut self, g: Group) {
            self.humans.push((g.seat, self.here, false));
            self.groups.push(g);
        }

        fn seat(&mut self) -> Seat<'_> {
            let (gam, types, book, productions) = data();
            Seat {
                rival: &mut self.rival,
                day: 5,
                gam,
                people: &mut self.people,
                camps: &mut self.camps,
                rooms: &mut self.rooms,
                ground: &mut self.ground,
                springs: &mut self.springs,
                types,
                productions,
                book,
                room_ids: &self.room_ids,
                base_experience: 37,
                humans: &self.humans,
                others: self.others.iter_mut().collect(),
                humans_here: self
                    .groups
                    .iter_mut()
                    .map(|g| HumanHere { seat: g.seat, needs: &mut g.needs, exposure: &mut g.exposure, pack: &mut g.pack, kit: &mut g.kit, experience: 37, party: &mut g.party, stood: burngfx::game::Point::new(0, 0) })
                    .collect(),
                respawns: &mut self.respawns,
                npc_respawn: 10,
                log: Vec::new(),
                freed_at_blow: false,
                fell_order: Vec::new(),
            }
        }
    }

    /// A blow that leaves the last camper at nought frees the place there
    /// and then (`Character.Health` runs `Die`, `Character.cs:206-209`;
    /// `Dismiss` with `IsLastInCamp`, `:438-446`), so although the human
    /// leader standing there lives, `TryClaimOrReinforceCurrentCamp`
    /// (`AI/Profiles/Amiga/AmigaAiState.cs:87`) claims it — a man stationed,
    /// a knife made, production 0 — and `ChooseNextStep` (`:97`) keeps the
    /// group where it arrived today.
    #[test]
    fn a_last_camper_falling_frees_the_place_at_the_blow() {
        for seed in 0..20 {
            let mut f = Fixture::new();
            f.camp(0, &[1]);
            f.human(Group::new(0, 100));
            let mut ai = amiga_ai::AmigaAi::new(0, 5);
            let mut seat = f.seat();
            ai.turn(&mut seat, &mut Rng::seeded(seed));
            let fallen = seat.bury_the_fallen();
            assert_eq!(fallen.who, vec![String::from("a camper of a place now free")], "seed {seed}");
            assert!(fallen.human_leaders.is_empty());
            drop(seat);
            let camp = f.camps.get(f.here).unwrap();
            assert_eq!(camp.owner, Some(AMIGA), "seed {seed}");
            assert_eq!(camp.residents.len(), 1);
            assert_eq!(camp.production, Some(0));
            let (_, types, ..) = data();
            let knife = types.index_of("item_knife").unwrap();
            assert!(f.rooms.get(f.here, 0).is_some_and(|b| b.iter().any(|k| k == knife)));
            assert!(f.rival.journey.is_none(), "seed {seed}: the group stays");
            assert!(f.groups[0].needs.health > 0);
        }
    }

    /// `defender.Health -= attack` (`AmigaAiState.cs:394,400`) goes
    /// through the property: the float's fraction goes with the blow —
    /// 49.65 less 5 is 44, nothing above it.
    #[test]
    fn a_duel_blow_spends_a_danger_s_fraction() {
        let mut f = Fixture::new();
        f.camp(0, &[49]);
        f.human(Group::new(0, 49));
        let carry = burngfx::danger::Exposure { carried: 65 * burngfx::danger::UNIT / 100, lifted: false };
        let here = f.here;
        f.camps.at(here).unwrap().residents[0].exposure = carry;
        f.groups[0].exposure = carry;
        let mut seat = f.seat();
        seat.hurt(CAMPER, 5);
        seat.hurt(HUMAN, 5);
        drop(seat);
        let r = &f.camps.get(here).unwrap().residents[0];
        assert_eq!((r.needs.health, r.exposure.carried), (44, 0));
        assert_eq!((f.groups[0].needs.health, f.groups[0].exposure.carried), (44, 0));
    }

    /// `GetCampDefenders` (`AI/Profiles/AiStateOperations.cs:60-70`): the
    /// living campers, then every enemy party standing here in the world's
    /// order of players, humans and machines alike — each leader before
    /// his men.
    #[test]
    fn every_party_here_defends_in_seat_order() {
        let mut f = Fixture::new();
        f.camp(0, &[50]);
        f.human(Group::new(0, 90));
        let mut other = Rival::new(1, f.here);
        other.needs.health = 85;
        f.others.push(other);
        let mut second = Group::new(3, 80);
        second.party.recruit();
        second.party.companions[0].needs.health = 75;
        f.human(second);
        let seat = f.seat();
        let healths: Vec<i32> = seat.defenders().into_iter().map(|d| seat.duelist(d).unwrap().health).collect();
        assert_eq!(healths, vec![50, 90, 85, 80, 75]);
    }

    /// `StrategicDamage` asks `SelectOriginalWeapon(allowUnloadedRifle:
    /// true)` (`AmigaAiState.cs:326`), which starts from the chosen weapon
    /// (`Character.cs:674-697`) and makes the one it finds the chosen one
    /// (`:700-704`): a human leader who chose his loaded rifle fights with
    /// it (basis 20) and still has it chosen; his man, who chose nothing,
    /// has his knife chosen after the duel. (The camper and the leader fall
    /// at the first blow, so the man is reached whatever the draws.)
    #[test]
    fn the_duel_starts_from_the_chosen_weapon_and_keeps_the_one_found() {
        let (_, types, ..) = data();
        let knife = types.index_of("item_knife").unwrap();
        let rifle = types.index_of("item_loaded_rifle").unwrap();
        let mut f = Fixture::new();
        f.camp(0, &[1]);
        let mut g = Group::new(0, 1);
        g.pack = ItemBag::with(6, &[knife, rifle]);
        g.kit.weapon = Some(1);
        g.party.recruit();
        g.party.companions[0].needs.health = 100;
        g.party.companions[0].pack = ItemBag::with(6, &[knife]);
        f.human(g);
        let mut ai = amiga_ai::AmigaAi::new(0, 5);
        let mut seat = f.seat();
        let leader = seat.defenders()[1];
        assert_eq!(seat.duelist(leader).unwrap().weapon.as_deref(), Some("item_loaded_rifle"));
        ai.turn(&mut seat, &mut Rng::seeded(1));
        drop(seat);
        assert_eq!(f.groups[0].kit.weapon, Some(1), "the rifle stays chosen");
        assert_eq!(f.groups[0].party.companions[0].kit.weapon, Some(0), "the knife is chosen");
    }

    /// The same write-back for a machine player's leader (register AMI-8):
    /// `SelectOriginalWeapon` makes the weapon it finds his
    /// `Character.Weapon` (`Character.cs:700-704`), and the next duel
    /// starts from it — an unloaded rifle he holds stays chosen
    /// (`allowUnloadedRifle: true`) rather than being passed over for the
    /// best weapon a computer player would reach for without a choice.
    #[test]
    fn a_machine_leader_keeps_the_weapon_the_duel_found() {
        let (_, types, ..) = data();
        let knife = types.index_of("item_knife").unwrap();
        let unloaded = types.index_of("item_unloaded_rifle").unwrap();
        let mut f = Fixture::new();
        f.rival.pack = ItemBag::with(9, &[knife]);
        let mut seat = f.seat();
        seat.select_weapon(0);
        drop(seat);
        assert_eq!(f.rival.weapon_kind, Some(knife), "written back");
        f.rival.pack = ItemBag::with(9, &[knife, unloaded]);
        f.rival.weapon_kind = Some(unloaded);
        let seat = f.seat();
        let unchosen = burngfx::table_combat::find_original_weapon(&seat.rival.pack, None, false, true, types)
            .and_then(|s| seat.rival.pack.get(s));
        let chosen = seat.duelist(0).unwrap().weapon;
        let expect = burngfx::table_combat::find_original_weapon(&seat.rival.pack, Some(1), false, true, types)
            .and_then(|s| seat.rival.pack.get(s)).and_then(|k| types.name_of(k)).map(String::from);
        assert_eq!(chosen, expect, "the duel starts from his choice");
        assert_ne!(unchosen, Some(unloaded), "without it he would not take the unloaded rifle");
    }
}

#[cfg(test)]
mod difficulty {
    const AI: &str = include_str!("../../burngfx/testdata/v1.2/rules/classic/ai.txt");

    /// `ai.txt`'s `[players] difficulties` gives each seat its own
    /// (`GameCreation.cs:265-269`, `AiPolicy.cs:61-70`): with `hard easy
    /// game game`, seat 1 plays easy whatever the game's, seat 0 hard, and
    /// the others the game's.
    #[test]
    fn each_seat_from_ai_txt() {
        let text = AI.replacen("difficulties = game game game game", "difficulties = hard easy game game", 1);
        assert_ne!(text, AI);
        let d = |seat: usize| super::seat_difficulty(Some(text.as_bytes()), seat, 1);
        assert_eq!([d(0), d(1), d(2), d(3)], [Some(2), Some(0), Some(1), Some(1)]);
        // The shipped file: the game's for all.
        assert_eq!(super::seat_difficulty(Some(AI.as_bytes()), 1, 2), Some(2));
        assert_eq!(super::seat_difficulty(None, 1, 2), None);
    }
}
