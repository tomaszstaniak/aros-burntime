//! The world as the Modern opponent sees and changes it: the game's own
//! data — camps, rooms, the ground, people, springs — behind the planner.
//!
//! Reading: a picture of the places ([`Seat::sites`]), the people who may
//! be hired, the group and the defenders as fighters. Changing: travel, a
//! camp founded or freed, a fight's wounds and its dead, things stored or
//! picked up. Each change does what the v1.2 code it is named after does,
//! on the port's types; the decisions are in the other `modern_ai`
//! modules. The same seat serves the game and the tests, so a test sees
//! the world change, not just a decision.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::combat::Kit;
use crate::game::{Camps, CharacterNeeds, ItemBag, Party, Point, Respawns, RoomItems, Springs};
use crate::gamdat::{CharClass, Danger, Gamdat};
use crate::hire::People;
use crate::items::ItemTypes;
use crate::matmap::RoomType;
use crate::production::Productions;
use crate::rival::Rival;
use crate::rules::RuleBook;
use crate::spawn::Dropped;
use crate::trade::Prices;
use crate::v12_day::V12World;

use super::camps::{Guard, Site, SiteProduction, Sites};
use super::encounter::{Fighter, Rules};
use super::equipment::{give, take};
use super::group::{self, Items};
use super::recruitment::Candidate;
use super::state::ModernAi;

/// The human's group, when it stands where the seat is.
pub struct HumanHere<'a> {
    pub seat: usize,
    pub needs: &'a mut CharacterNeeds,
    /// His part of a danger's point not yet spent: `Health` reads below the
    /// whole number, and a blow spends it ([`crate::danger`]).
    pub exposure: &'a mut crate::danger::Exposure,
    pub pack: &'a mut ItemBag,
    pub kit: &'a mut Kit,
    pub experience: i32,
    pub party: &'a mut Party,
    /// Where his leader stands (`Position`), for `Die`'s drop.
    pub stood: crate::game::Point,
}

pub struct Seat<'a> {
    pub rival: &'a mut Rival,
    pub ai: &'a mut ModernAi,
    pub day: i32,
    pub difficulty: i32,
    pub gam: &'a Gamdat,
    pub people: &'a mut People,
    pub camps: &'a mut Camps,
    pub rooms: &'a mut RoomItems,
    pub ground: &'a mut Vec<Dropped>,
    pub springs: &'a mut Springs,
    pub types: &'a ItemTypes,
    pub prices: &'a Prices,
    pub productions: &'a Productions,
    pub book: &'a RuleBook,
    /// The recipes things are built by (`Constructions`).
    pub constructions: &'a crate::construction::Constructions,
    pub v12: &'a V12World,
    pub room_ids: &'a [Vec<u8>],
    /// Living human seats: `(seat, location, on the road)`.
    pub humans: &'a [(usize, usize, bool)],
    /// The other machine seats.
    pub others: Vec<&'a mut Rival>,
    pub human_here: Option<HumanHere<'a>>,
    pub respawns: &'a mut Respawns,
    pub traders: &'a mut crate::trader::Traders,
    pub stock_lists: &'a crate::trader::StockLists,
    /// `npc_respawn`: the days before a dead person comes back.
    pub npc_respawn: i32,
    /// The turn's camp assessments, once captured (`AiTurnContext.Camps`).
    pub camp_cache: Option<alloc::rc::Rc<core::cell::RefCell<super::camps::CampCache>>>,
    /// The turn's needs (`AiTurnContext.Needs`): taken at the turn's start
    /// and again after each exchange, and read in between.
    pub needs_cache: Option<alloc::rc::Rc<core::cell::RefCell<Option<super::needs::Needs>>>>,
    /// Every burial this seat settled, in order: the game acts on a human
    /// leader's death once the turn is over.
    pub buried: Vec<Fallen>,
    /// `AiTurnContext.BestTradeCity`: resolved at its first use in a turn
    /// and kept for the rest of it.
    pub trade_city_cache: Option<Option<usize>>,
    /// Every fight this seat fought, as it ended: who was in it and what
    /// each had on (`Character.Protection`, put on when struck).
    pub fights: Vec<Fought>,
}

/// One fighter at the end of a fight.
#[derive(Clone, Debug, PartialEq)]
pub struct Worn {
    pub attacker: bool,
    pub before: i32,
    pub after: i32,
    /// The kind of the protection worn, if any.
    pub worn: Option<usize>,
    /// Every protective thing in the pack (`class=protection`).
    pub carried: Vec<usize>,
}

/// A fight at `place`, under its danger.
#[derive(Clone, Debug, PartialEq)]
pub struct Fought {
    pub place: usize,
    pub hazard: Danger,
    pub fighters: Vec<Worn>,
}

/// Somebody among a camp's defenders, to write a fight back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defender {
    Camper(usize),
    HumanLeader,
    HumanCompanion(usize),
    Leader(usize),
    Companion(usize, usize),
}

/// `CombatStrength.Defenders` here (`GetCampDefenders`): the owner's living
/// campers, then every living party of the owner standing here — as
/// fighters, who they are, whether they came with the owner's party, and
/// which of them is the owner's own character.
pub struct Defence {
    pub fighters: Vec<Fighter>,
    pub who: Vec<Defender>,
    pub visiting: Vec<bool>,
    pub owner: Option<usize>,
    /// Alive below one point, read as 0 by `Health`'s getter.
    pub lifted: Vec<bool>,
}

/// Who a fight left dead, settled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fallen {
    /// Companions and campers, anybody's.
    pub people: usize,
    pub human_leader: bool,
    /// Machine leaders who fell, by player.
    pub leaders: Vec<usize>,
    /// What this seat's fallen companions dropped: indices into the ground.
    pub own_drops: Vec<usize>,
    /// What everybody else who fell here dropped.
    pub other_drops: Vec<usize>,
}

impl<'a> Seat<'a> {
    /// The item facts; they borrow the game's tables, not the seat.
    pub fn items(&self) -> Items<'a> {
        Items { types: self.types, prices: self.prices }
    }
}

impl Seat<'_> {

    pub fn here(&self) -> usize {
        self.rival.at
    }

    pub fn me(&self) -> usize {
        self.rival.player
    }

    pub fn room_count(&self, l: usize) -> usize {
        self.room_ids.get(l).map_or(0, |ids| ids.len())
    }

    pub fn room_capacity(&self, l: usize, room: usize) -> usize {
        self.capacity(l, room)
    }

    pub fn room_has_space(&self, l: usize, room: usize) -> bool {
        self.has_space(l, room)
    }

    pub fn is_water_room(&self, l: usize, room: usize) -> bool {
        self.is_water(l, room)
    }

    /// `AutoSelectFoodProduction(onlyIfCurrentProducesNothing: false)` for a
    /// camp, the rule every camp's day follows.
    pub fn auto_select_production(&mut self, l: usize) {
        let storage = SeatStorage { rooms: &*self.rooms, room_ids: self.room_ids, gam: self.gam, types: self.types, productions: self.productions };
        if let Some(camp) = self.camps.at(l) {
            crate::v12_day::auto_select_food_production(l, camp, &storage, self.productions, self.types, self.v12, false);
        }
    }

    /// `RefreshFoodProductionSelection` for a camp: v1.3's `JoinCamp` runs
    /// it as the camper joins (`758f323`, `Character.cs:434` at `v1.3`).
    pub fn refresh_production(&mut self, l: usize) {
        let storage = SeatStorage { rooms: &*self.rooms, room_ids: self.room_ids, gam: self.gam, types: self.types, productions: self.productions };
        if let Some(camp) = self.camps.at(l) {
            crate::v12_day::refresh_production(l, camp, &storage, self.productions, self.types, self.v12);
        }
    }

    /// `EquipReststopCaretaker` for the camp at Reststop.
    pub fn equip_reststop_caretaker_at(&mut self, l: usize) -> bool {
        self.equip_reststop_caretaker(l, None)
    }

    fn capacity(&self, l: usize, room: usize) -> usize {
        self.room_ids.get(l).and_then(|ids| ids.get(room)).map_or(0, |&id| RoomType::from_room_id(id).item_capacity())
    }

    fn is_water(&self, l: usize, room: usize) -> bool {
        self.room_ids.get(l).and_then(|ids| ids.get(room)).is_some_and(|&id| RoomType::from_room_id(id) == RoomType::WaterSource)
    }

    fn has_space(&self, l: usize, room: usize) -> bool {
        let cap = self.capacity(l, room);
        cap > 0 && self.rooms.get(l, room).is_none_or(|b| b.len() < cap)
    }

    fn entry(&self, l: usize) -> Point {
        self.gam.locations.get(l).map_or(Point::new(0, 0), |x| x.entry_point)
    }

    fn room_kinds(&self, l: usize) -> Vec<usize> {
        (0..self.room_count(l)).filter_map(|r| self.rooms.get(l, r)).flat_map(|b| b.iter().collect::<Vec<_>>()).collect()
    }

    fn has_in_water_room(&self, l: usize, name: &str) -> bool {
        (0..self.room_count(l)).filter(|&r| self.is_water(l, r)).filter_map(|r| self.rooms.get(l, r)).any(|b| b.iter().any(|k| self.types.name_of(k) == Some(name)))
    }

    /// `Source.Water` by the preset's `water_output` and the pumps in the
    /// spring's room.
    pub fn water_output(&self, l: usize) -> i32 {
        let base = self.springs.get(l).map_or(0, |s| s.refresh);
        let industrial = self.has_in_water_room(l, "item_industrial_pump");
        crate::v12_day::water_output(self.book.rules.water_output, base, !industrial && self.has_in_water_room(l, "item_hand_pump"), industrial)
    }

    pub fn class_of(&self, record: Option<usize>) -> CharClass {
        record.and_then(|r| self.people.get_by_record(r)).map_or(CharClass::Mercenary, |p| p.class)
    }

    fn resting(&self, pack: &ItemBag) -> bool {
        pack.iter().any(|k| super::needs::Needs::resting_sustenance(self.items(), k))
    }

    fn protected(&self, pack: &ItemBag, danger: Danger) -> bool {
        danger != Danger::None && pack.iter().any(|k| self.types.get(k).is_some_and(|t| t.protection.iter().any(|(d, _)| *d == danger)))
    }

    /// The picture of every place the planner reads, at this moment.
    pub fn sites(&self) -> Sites {
        let mut sites = Sites::default();
        for (l, loc) in self.gam.locations.iter().enumerate() {
            let camp = self.camps.get(l);
            let owner = camp.and_then(|c| c.owner);
            let kinds = self.room_kinds(l);
            let mut productions: Vec<SiteProduction> = Vec::new();
            for &id in loc.available_products.iter().flatten() {
                let Some(p) = self.productions.get(id) else { continue };
                let Some(produce) = self.types.index_of(&p.produce) else { continue };
                let mut installed = kinds.iter().filter(|&&k| self.types.name_of(k).is_some_and(|n| p.worked_by(n))).count();
                if p.allow_inventory {
                    if let Some(c) = camp {
                        installed += c.residents.iter().flat_map(|r| r.pack.iter().collect::<Vec<_>>()).filter(|&k| self.types.name_of(k).is_some_and(|n| p.worked_by(n))).count();
                    }
                }
                productions.push(SiteProduction { id, production: p.clone(), produce, installed: installed as i32 });
            }
            let current = camp.and_then(|c| c.production).and_then(|p| productions.iter().position(|sp| sp.id == p));
            let mut guards: Vec<Guard> = Vec::new();
            let mut elsewhere: Vec<usize> = Vec::new();
            let mut guard_items: Vec<usize> = Vec::new();
            if let (Some(c), Some(o)) = (camp, owner) {
                for r in &c.residents {
                    guards.push(Guard { owner: o, alive: r.needs.health > 0, resting: self.resting(&r.pack), protected: self.protected(&r.pack, loc.danger) });
                    elsewhere.extend(r.pack.iter());
                    guard_items.extend(r.pack.iter());
                }
                for &seat in &c.stationed {
                    guards.push(Guard { owner: seat, alive: true, resting: false, protected: false });
                }
            }
            elsewhere.extend(self.ground.iter().filter(|d| d.location == l).map(|d| d.kind));
            let stock = current.map_or(0, |i| kinds.iter().filter(|&&k| k == productions[i].produce).count() as i32);
            let technician = !loc.is_city
                && camp.is_some_and(|c| c.residents.iter().any(|r| r.needs.health > 0 && r.origin.is_some_and(|o| self.v12.technicians.contains(&o))));
            let source_room = (0..self.room_count(l)).any(|r| self.is_water(l, r));
            sites.sites.push(Site {
                city: loc.is_city,
                owner,
                danger: loc.danger,
                water: self.water_output(l),
                source_reserve: self.springs.get(l).map_or(0, |s| s.reserve),
                has_products: loc.available_products.iter().any(|p| p.is_some()),
                productions,
                current,
                guards,
                stored: kinds,
                elsewhere,
                guard_items,
                stock,
                roads: loc.roads.iter().map(|r| (r.neighbour, r.days as i32)).collect(),
                bonus: if technician { self.v12.technician_bonus } else { 0 },
                source_room,
                pump: self.has_in_water_room(l, "item_hand_pump") || self.has_in_water_room(l, "item_industrial_pump"),
                threatened: false,
            });
        }
        sites
    }

    /// The people at each place, as hiring candidates.
    pub fn candidates(&self) -> Vec<Vec<Candidate>> {
        let mut out: Vec<Vec<Candidate>> = vec![Vec::new(); self.gam.locations.len()];
        for (l, at) in out.iter_mut().enumerate() {
            for i in self.people.at(l) {
                let Some(p) = self.people.get(i) else { continue };
                let creature = crate::table_combat::is_creature(p.class);
                at.push(Candidate {
                    record: p.record,
                    name: String::new(),
                    class: p.class,
                    experience: p.experience,
                    health: p.needs.health,
                    hire_items: p.accepts.clone(),
                    pack: p.pack.clone(),
                    available: !p.dead && !p.hired && p.employer.is_none() && !creature,
                });
            }
        }
        out
    }


    /// The group as fighters, the leader first.
    pub fn party_fighters(&self) -> Vec<Fighter> {
        let r = &*self.rival;
        let mut out = vec![Fighter { class: CharClass::Boss, experience: r.experience, health: r.needs.health, pack: r.pack.clone(), kit: self.ai.leader_kit, human: false }];
        for c in &r.party.companions {
            out.push(Fighter { class: self.class_of(c.origin), experience: c.experience, health: c.needs.health, pack: c.pack.clone(), kit: c.kit, human: false });
        }
        out
    }

    /// A fight's wounds, ammunition and choices back onto the group.
    pub fn write_party(&mut self, f: &[Fighter]) {
        if let Some(l) = f.first() {
            self.rival.needs.health = l.health;
            self.rival.pack = l.pack.clone();
            self.ai.leader_kit = l.kit;
        }
        for (c, x) in self.rival.party.companions.iter_mut().zip(f.iter().skip(1)) {
            c.needs.health = x.health;
            c.pack = x.pack.clone();
            c.kit = x.kit;
        }
    }

    /// `CombatStrength.Defenders` where the group stands.
    pub fn defence_here(&self) -> Defence {
        let here = self.here();
        let mut d = Defence { fighters: Vec::new(), who: Vec::new(), visiting: Vec::new(), owner: None, lifted: Vec::new() };
        let Some(owner) = self.camps.get(here).and_then(|c| c.owner).filter(|&o| o != self.me()) else { return d };
        let human = self.humans.iter().any(|&(s, _, _)| s == owner);
        if let Some(c) = self.camps.get(here) {
            for (i, r) in c.residents.iter().enumerate().filter(|(_, r)| r.needs.health > 0) {
                d.fighters.push(Fighter { class: self.class_of(r.origin), experience: r.experience, health: r.needs.health, pack: r.pack.clone(), kit: r.kit, human });
                d.who.push(Defender::Camper(i));
                d.lifted.push(r.exposure.lifted);
                d.visiting.push(false);
            }
        }
        if let Some(h) = self.human_here.as_ref().filter(|h| h.seat == owner) {
            if h.needs.health > 0 {
                d.owner = Some(d.fighters.len());
                d.fighters.push(Fighter { class: CharClass::Boss, experience: h.experience, health: h.needs.health, pack: h.pack.clone(), kit: *h.kit, human: true });
                d.who.push(Defender::HumanLeader);
                d.lifted.push(h.exposure.lifted);
                d.visiting.push(true);
            }
            for (i, c) in h.party.companions.iter().enumerate().filter(|(_, c)| c.needs.health > 0) {
                d.fighters.push(Fighter { class: self.class_of(c.origin), experience: c.experience, health: c.needs.health, pack: c.pack.clone(), kit: c.kit, human: true });
                d.who.push(Defender::HumanCompanion(i));
                d.lifted.push(c.exposure.lifted);
                d.visiting.push(true);
            }
        }
        for (k, r) in self.others.iter().enumerate() {
            if r.player != owner || r.dead || r.journey.is_some() || r.at != here {
                continue;
            }
            if r.needs.health > 0 {
                d.owner = Some(d.fighters.len());
                let kit = r.modern.as_ref().map_or(Kit::default(), |m| m.leader_kit);
                d.fighters.push(Fighter { class: CharClass::Boss, experience: r.experience, health: r.needs.health, pack: r.pack.clone(), kit, human: false });
                d.who.push(Defender::Leader(k));
                d.lifted.push(false);
                d.visiting.push(true);
            }
            for (i, c) in r.party.companions.iter().enumerate().filter(|(_, c)| c.needs.health > 0) {
                d.fighters.push(Fighter { class: self.class_of(c.origin), experience: c.experience, health: c.needs.health, pack: c.pack.clone(), kit: c.kit, human: false });
                d.who.push(Defender::Companion(k, i));
                d.lifted.push(false);
                d.visiting.push(true);
            }
        }
        d
    }

    /// A fight's wounds and ammunition back onto the defenders. One a blow
    /// landed on (`Health -=`,
    /// `StrategicEncounter.cs:96`) has the danger's fraction spent with it.
    pub fn write_defence(&mut self, d: &Defence) {
        let here = self.here();
        fn wound(health: &mut i32, e: &mut crate::danger::Exposure, x: i32) {
            // The copy started from the number held, so it lives as
            // `IsDead` says (`health <= 0` on the float, 0 < h < 1 alive);
            // a blow's damage is then taken off the getter, through the
            // setter.
            if x != *health {
                let damage = *health - x;
                crate::danger::spend(health, e);
                *health -= damage;
            }
        }
        for (x, who) in d.fighters.iter().zip(d.who.iter()) {
            match *who {
                Defender::Camper(i) => {
                    if let Some(r) = self.camps.at(here).and_then(|c| c.residents.get_mut(i)) {
                        wound(&mut r.needs.health, &mut r.exposure, x.health);
                        r.pack = x.pack.clone();
                        r.kit = x.kit;
                    }
                }
                Defender::HumanLeader => {
                    if let Some(h) = self.human_here.as_mut() {
                        wound(&mut h.needs.health, h.exposure, x.health);
                        *h.pack = x.pack.clone();
                        *h.kit = x.kit;
                    }
                }
                Defender::HumanCompanion(i) => {
                    if let Some(c) = self.human_here.as_mut().and_then(|h| h.party.companions.get_mut(i)) {
                        wound(&mut c.needs.health, &mut c.exposure, x.health);
                        c.pack = x.pack.clone();
                        c.kit = x.kit;
                    }
                }
                Defender::Leader(k) => {
                    if let Some(r) = self.others.get_mut(k) {
                        r.needs.health = x.health;
                        r.pack = x.pack.clone();
                        if let Some(m) = r.modern.as_mut() {
                            m.leader_kit = x.kit;
                        }
                    }
                }
                Defender::Companion(k, i) => {
                    if let Some(c) = self.others.get_mut(k).and_then(|r| r.party.companions.get_mut(i)) {
                        c.needs.health = x.health;
                        c.pack = x.pack.clone();
                        c.kit = x.kit;
                    }
                }
            }
        }
    }

    /// `Player.CanTravel(from, to)`: from where it stands, not on the road,
    /// down an open road, and out of somebody else's camp only the way it
    /// came.
    pub fn can_travel(&self, from: usize, to: usize) -> bool {
        if self.rival.journey.is_some() || from != self.here() {
            return false;
        }
        if self.camps.get(from).and_then(|c| c.owner).is_some_and(|o| o != self.me()) && self.rival.previous != Some(to) {
            return false;
        }
        self.gam.locations.get(from).is_some_and(|x| x.roads.iter().any(|r| r.neighbour == to && r.days > 0))
    }

    /// `Player.Travel`: on the road to `to`. False when it may not.
    pub fn travel(&mut self, to: usize) -> bool {
        if !self.can_travel(self.here(), to) {
            return false;
        }
        match crate::game::depart(self.gam, self.rival.journey, self.rival.at, to) {
            Ok(j) => {
                self.rival.journey = Some(j);
                self.rival.previous = Some(self.rival.at);
                let entry = self.entry(to);
                crate::game::place_at_entry(&mut self.rival.party, entry);
                true
            }
            Err(_) => false,
        }
    }

    /// `location.Player = null`.
    pub fn free_here(&mut self) {
        let here = self.here();
        if let Some(c) = self.camps.at(here) {
            c.owner = None;
        }
    }

    /// **`Character.Die` for everybody a fight left at nought**: out of their
    /// group or camp, their things on the ground at the entry, the person
    /// due back after `npc_respawn` days. A camp whose last camper fell is
    /// nobody's. A leader's pack falls; putting his player out is the day's.
    pub fn bury(&mut self) -> Fallen {
        self.bury_in(Vec::new())
    }

    /// [`Seat::bury`], in the order the fight's blows brought them down.
    pub fn bury_in(&mut self, order: Vec<crate::burial::Key>) -> Fallen {
        let here = self.here();
        let me = self.me();
        let entry = self.entry(here);
        let human_out = self.human_here.as_ref().is_some_and(|h| h.needs.health <= 0);
        let (humans, human_leaders) = match self.human_here.as_mut() {
            Some(h) => {
                let leader = if human_out { alloc::vec![(h.seat, &mut *h.pack, h.stood)] } else { Vec::new() };
                (alloc::vec![(h.seat, &mut *h.party)], leader)
            }
            None => (Vec::new(), Vec::new()),
        };
        let b = crate::burial::bury(crate::burial::Here {
            location: here,
            entry,
            rival: self.rival,
            humans,
            human_leaders,
            order,
            others: &mut self.others,
            camps: self.camps,
            ground: self.ground,
            people: self.people,
            respawns: self.respawns,
            npc_respawn: self.npc_respawn,
        });
        let mut out = Fallen { people: b.people.len(), ..Fallen::default() };
        for f in &b.people {
            let into = if f.source == crate::burial::Source::Party(me) { &mut out.own_drops } else { &mut out.other_drops };
            into.extend(&f.drops);
        }
        for (player, drops) in &b.leaders {
            if *player != me {
                out.other_drops.extend(drops);
            }
            out.leaders.push(*player);
        }
        if let Some(h) = self.human_here.as_ref() {
            out.human_leader = h.needs.health <= 0;
        }
        self.buried.push(out.clone());
        out
    }

    /// `CampManagement.CanStoreItemInCamp`: a room with space, and — for
    /// anything but the produce or a tool — room left for the production's
    /// stock limit.
    pub fn can_store_item_in_camp(&self, l: usize, kind: usize) -> bool {
        let rooms = self.room_count(l);
        if !(0..rooms).any(|r| self.has_space(l, r)) {
            return false;
        }
        let Some(p) = self.camps.get(l).and_then(|c| c.production).and_then(|i| self.productions.get(i)) else { return true };
        let produce = self.types.index_of(&p.produce);
        let is_tool = self.types.name_of(kind).is_some_and(|n| self.productions.worked_by(n).is_some());
        if Some(kind) == produce || is_tool {
            return true;
        }
        let produced = self.room_kinds(l).iter().filter(|&&k| Some(k) == produce).count();
        let missing = crate::game::MAX_STOCK_FOOD.saturating_sub(produced);
        let free: usize = (0..rooms).map(|r| self.capacity(l, r).saturating_sub(self.rooms.get(l, r).map_or(0, |b| b.len()))).sum();
        free > missing
    }

    /// `CampManagement.StoreItemInCamp`: a water container to the spring's
    /// room first (filled there when the reserve covers it), anything else
    /// away from it.
    pub fn store_item_in_camp(&mut self, l: usize, kind: usize, rounds: Option<i32>, reserve_production: bool) -> bool {
        let rooms = self.room_count(l);
        if reserve_production && !self.can_store_item_in_camp(l, kind) || !reserve_production && !(0..rooms).any(|r| self.has_space(l, r)) {
            return false;
        }
        self.garrisons().store(l, kind, rounds)
    }

    /// The part of the seat [`Garrisons`] works on.
    fn garrisons(&mut self) -> Garrisons<'_> {
        Garrisons {
            me: self.rival.player,
            ai: &mut *self.ai,
            camps: &mut *self.camps,
            rooms: &mut *self.rooms,
            springs: &mut *self.springs,
            types: self.types,
            prices: self.prices,
            room_ids: self.room_ids,
        }
    }

    /// `StoreInReserveOrAtLocation`: the reserve, else the camp if it is
    /// ours, else the ground.
    pub fn store_in_reserve_or_at(&mut self, l: usize, kind: usize, rounds: Option<i32>) {
        let items = Items { types: self.types, prices: self.prices };
        if super::equipment::reserve_insert(self.ai, items, kind) {
            return;
        }
        if self.camps.get(l).and_then(|c| c.owner) == Some(self.me()) && self.store_item_in_camp(l, kind, rounds, true) {
            return;
        }
        let entry = self.entry(l);
        self.ground.push(Dropped { location: l, at: entry, kind, rounds, icon: crate::spawn::PILE_UNDRAWN });
    }

    fn unload_garrison_belongings(&mut self, l: usize, resident: usize) {
        self.garrisons().unload(l, resident);
    }

    /// The world's part of `JoinCamp` for a companion: protection from the
    /// reserve against the place's danger, into the camp, his belongings
    /// unloaded, Reststop's caretaker given a bible, and a tool installed —
    /// a better one from the reserve, or the group's compatible one unless
    /// `keep_last_tool`. True when he joined. [`super::upkeep::join_camp`]
    /// is the whole of it: it decides `keep_last_tool` and sees to the
    /// leader's places after.
    pub fn join_camp(&mut self, member: usize, keep_last_tool: bool) -> bool {
        let here = self.here();
        let me = self.me();
        let danger = self.gam.locations.get(here).map_or(Danger::None, |l| l.danger);
        let Some(c) = self.rival.party.companions.get(member) else { return false };
        if danger != Danger::None && !c.pack.is_full() {
            let names: &[&str] = if danger == Danger::Radiation { &["item_protection_suit", "item_paper_helmet"] } else { &["item_gas_mask", "item_paper_helmet", "item_protection_suit"] };
            let found = names.iter().find_map(|n| self.types.index_of(n).filter(|&k| self.ai.reserve_count(k) > 0));
            if let Some(k) = found {
                self.ai.reserve_take(k);
                let c = &mut self.rival.party.companions[member];
                c.kit.protection = give(&mut c.pack, k, None);
            }
        }
        let (origin, stood) = (self.rival.party.companions[member].origin, self.rival.party.companions[member].at);
        if !self.rival.party.leave(self.camps, here, me, member) {
            return false;
        }
        if let Some(record) = origin {
            self.people.guarding(record, me, here, stood);
            // `JoinCamp`: a fresh `SimpleMind`, and he roams.
            self.people.joined_camp(record);
        }
        // v1.3's `JoinCamp`: the production refreshed as he joins, before
        // `UnloadGarrisonBelongings` and the tool (`AiState.cs:1135-1150`).
        self.refresh_production(here);
        let resident = self.camps.get(here).map_or(0, |c| c.residents.len().saturating_sub(1));
        self.unload_garrison_belongings(here, resident);
        self.equip_reststop_caretaker(here, Some(resident));
        self.install_tool(here, keep_last_tool);
        true
    }

    /// `EquipReststopCaretaker`.
    pub fn equip_reststop_caretaker(&mut self, l: usize, preferred: Option<usize>) -> bool {
        if l != super::camps::RESTSTOP || self.camps.get(l).and_then(|c| c.owner) != Some(self.me()) {
            return false;
        }
        let items = Items { types: self.types, prices: self.prices };
        let Some(camp) = self.camps.get(l) else { return false };
        let guards: Vec<usize> = (0..camp.residents.len()).filter(|&i| camp.residents[i].needs.health > 0).collect();
        if guards.iter().any(|&i| self.resting(&camp.residents[i].pack)) {
            return true;
        }
        let Some(caretaker) = preferred.filter(|p| guards.contains(p)).or(guards.first().copied()) else { return false };
        self.unload_garrison_belongings(l, caretaker);
        if self.camps.get(l).and_then(|c| c.residents.get(caretaker)).is_none_or(|r| r.pack.is_full()) {
            return false;
        }
        let stored = (0..self.room_count(l)).find_map(|r| {
            let b = self.rooms.get(l, r)?;
            (0..b.len()).find(|&s| b.get(s).is_some_and(|k| super::needs::Needs::resting_sustenance(items, k))).map(|s| (r, s))
        });
        let bible = if let Some((r, s)) = stored {
            self.rooms.get_mut(l, r).and_then(|b| b.remove(s))
        } else if let Some(k) = self.ai.reserve.iter().find(|&&(k, n)| n > 0 && super::needs::Needs::resting_sustenance(items, k)).map(|&(k, _)| k) {
            self.ai.reserve_take(k);
            Some(k)
        } else {
            let carried = group::carried(self.rival).into_iter().find(|&(_, _, k)| super::needs::Needs::resting_sustenance(items, k));
            carried.and_then(|(m, s, _)| {
                if m == 0 {
                    let mut kit = self.ai.leader_kit;
                    let got = take(&mut self.rival.pack, &mut kit, s).map(|x| x.0);
                    self.ai.leader_kit = kit;
                    got
                } else {
                    let c = &mut self.rival.party.companions[m - 1];
                    take(&mut c.pack, &mut c.kit, s).map(|x| x.0)
                }
            })
        };
        let Some(bible) = bible else { return false };
        let given = self.camps.at(l).and_then(|c| c.residents.get_mut(caretaker)).is_some_and(|r| give(&mut r.pack, bible, None).is_some());
        if given {
            let id = String::from(self.types.name_of(bible).unwrap_or(""));
            self.ai.telemetry.report(|| alloc::format!("entrusted {id} to the Reststop caretaker"));
        }
        given
    }

    /// `JoinCamp`'s tool: a better trap from the reserve where the camp
    /// prefers production, else — with no tool installed and none to keep —
    /// the group's most filling compatible one; into the rooms, or the
    /// reserve, or the ground.
    fn install_tool(&mut self, l: usize, keep_last_tool: bool) {
        let items = self.items();
        let products: Vec<String> = self.gam.locations[l]
            .available_products
            .iter()
            .flatten()
            .filter_map(|&i| self.productions.get(i).map(|p| p.produce.clone()))
            .collect();
        let produces = |k: usize| self.types.name_of(k).and_then(|n| self.productions.worked_by(n)).and_then(|p| self.productions.get(p)).map(|p| p.produce.clone());
        let existing = self.room_kinds(l).into_iter().find(|&k| produces(k).is_some_and(|p| products.contains(&p)));
        let site = &self.sites().sites[l];
        let prefer = super::production::prefer_production(site);
        let existing_value = existing.and_then(|k| produces(k)).and_then(|p| self.types.index_of(&p)).map_or(-1.0, |p| items.value(p));
        let higher = self.ai.reserve.iter().any(|&(k, n)| {
            n > 0 && produces(k).is_some_and(|p| products.contains(&p) && self.types.index_of(&p).is_some_and(|pk| items.value(pk) > existing_value))
        });
        let upgrade = prefer && higher;
        if existing.is_some() && !upgrade {
            return;
        }
        let from_reserve = ["item_trap", "item_snake_trap", "item_rat_trap", "item_knife"].iter().find_map(|tool| {
            self.types.index_of(tool).filter(|&k| self.ai.reserve_count(k) > 0 && produces(k).is_some_and(|p| products.contains(&p)))
        });
        let tool = if let Some(k) = from_reserve {
            self.ai.reserve_take(k);
            Some((k, None))
        } else if existing.is_none() && !keep_last_tool {
            let mut best: Option<(usize, usize, i32)> = None;
            for (m, s, k) in group::carried(self.rival) {
                let Some(p) = produces(k).filter(|p| products.contains(p)) else { continue };
                let food = self.types.index_of(&p).map_or(0, |pk| self.types.food_of(pk));
                if best.is_none_or(|b| food > b.2) {
                    best = Some((m, s, food));
                }
            }
            best.and_then(|(m, s, _)| {
                if m == 0 {
                    let mut kit = self.ai.leader_kit;
                    let got = take(&mut self.rival.pack, &mut kit, s);
                    self.ai.leader_kit = kit;
                    got
                } else {
                    let c = &mut self.rival.party.companions[m - 1];
                    take(&mut c.pack, &mut c.kit, s)
                }
            })
        } else {
            None
        };
        if let Some((k, rounds)) = tool {
            if !self.store_item_in_camp(l, k, rounds, true) {
                self.store_in_reserve_or_at(l, k, rounds);
            }
        }
    }
}

/// The planner's picture of the world at one moment, owned, so that the
/// planning modules can borrow it while the seat changes the world.
pub struct View {
    pub sites: Sites,
    pub demand: Vec<i32>,
    pub people: Vec<Vec<Candidate>>,
    /// The rooms of this seat's camps.
    pub camp_rooms: Vec<Vec<ItemBag>>,
    /// Other living leaders not on the road, by place.
    pub rivals_at: Vec<usize>,
    pub dead_players: Vec<usize>,
    pub humans: Vec<usize>,
    pub group: super::supplies::GroupSupplies,
    pub party: Vec<Fighter>,
    pub defenders_here: Vec<Fighter>,
    pub traders: Vec<super::needs::TraderView>,
}

impl Seat<'_> {
    /// The picture, with threats marked by the policy's radius and the
    /// equipment the recipients want.
    pub fn view(&self, policy: &super::policy::AiPolicy, extended: bool) -> View {
        let mut sites = self.sites();
        sites.cache = self.camp_cache.clone();
        let mut rivals_at: Vec<usize> = self.others.iter().filter(|r| !r.dead && r.needs.health > 0 && r.journey.is_none()).map(|r| r.at).collect();
        rivals_at.extend(self.humans.iter().filter(|&&(_, _, road)| !road).map(|&(_, at, _)| at));
        super::reinforcement::mark_threatened(&mut sites, self.me(), policy.threat_radius, &rivals_at);
        let me = self.me();
        let camp_rooms: Vec<Vec<ItemBag>> = (0..self.gam.locations.len())
            .filter(|&l| self.camps.get(l).and_then(|c| c.owner) == Some(me))
            .map(|l| (0..self.room_count(l)).map(|r| self.rooms.get(l, r).cloned().unwrap_or_else(|| ItemBag::new(self.capacity(l, r)))).collect())
            .collect();
        // Recipients: the group, then the guards of threatened camps, the
        // most experienced first (`EquipmentPlanning.Recipients`).
        let mut packs: Vec<(ItemBag, Kit, i32, bool, usize)> = Vec::new();
        packs.push((self.rival.pack.clone(), self.ai.leader_kit, self.rival.experience, true, self.here()));
        for c in &self.rival.party.companions {
            packs.push((c.pack.clone(), c.kit, c.experience, true, self.here()));
        }
        let mut guards: Vec<(ItemBag, Kit, i32, bool, usize)> = Vec::new();
        for (l, s) in sites.sites.iter().enumerate() {
            if s.owner == Some(me) && s.threatened {
                if let Some(c) = self.camps.get(l) {
                    for r in c.residents.iter().filter(|r| r.needs.health > 0) {
                        guards.push((r.pack.clone(), r.kit, r.experience, false, l));
                    }
                }
            }
        }
        guards.sort_by_key(|g| core::cmp::Reverse(g.2));
        packs.extend(guards);
        let members: Vec<super::equipment::Member> = packs
            .iter_mut()
            .map(|(pack, kit, experience, in_party, location)| super::equipment::Member { pack, kit, experience: *experience, in_party: *in_party, location: *location })
            .collect();
        let demand = super::equipment::demands(self.types, &members, self.ai, policy, extended, self.day, me);
        drop(members);
        let mut dead_players: Vec<usize> = self.others.iter().filter(|r| r.dead).map(|r| r.player).collect();
        if self.rival.dead {
            dead_players.push(me);
        }
        View {
            sites,
            demand,
            people: self.candidates(),
            camp_rooms,
            rivals_at,
            dead_players,
            humans: self.humans.iter().map(|h| h.0).collect(),
            group: group::supplies(self.rival, self.types),
            party: self.party_fighters(),
            defenders_here: self.defence_here().fighters,
            traders: self.trader_views(),
        }
    }

    /// `ClassicAiState.CanCollectLocalLoot`: everybody living in the group
    /// passes the place's daily hazard check.
    pub fn can_collect_local_loot(&self) -> bool {
        let danger = self.gam.locations.get(self.here()).map_or(Danger::None, |l| l.danger);
        let immune = self.book.immune_faces(danger);
        let face_of = |origin: Option<usize>| origin.and_then(|o| self.v12.faces.iter().find(|f| f.0 == o)).map_or(0, |f| f.1);
        let leader = self.rival.needs.health <= 0 || super::super::v12_items::passes_daily_check(&self.rival.pack, self.ai.leader_kit.protection, danger, 0, immune, self.types);
        leader
            && self.rival.party.companions.iter().filter(|c| c.needs.health > 0).all(|c| {
                super::super::v12_items::passes_daily_check(&c.pack, c.kit.protection, danger, face_of(c.origin), immune, self.types)
            })
    }
}

impl<'a> Seat<'a> {
    /// The fight's rules where the group stands; they borrow the game's
    /// tables, not the seat.
    pub fn rules(&self) -> Rules<'a> {
        let hazard = self.gam.locations.get(self.here()).map_or(Danger::None, |l| l.danger);
        Rules { book: self.book, types: self.types, difficulty: self.difficulty, hazard }
    }
}

/// Somebody of this opponent's: its leader, a follower, or a guard in one
/// of its camps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Leader,
    Follower(usize),
    Guard(usize, usize),
}

/// The tables the planner reads, borrowed from the game, not the seat.
#[derive(Clone, Copy)]
pub struct Tables<'a> {
    pub types: &'a ItemTypes,
    pub prices: &'a Prices,
    pub productions: &'a Productions,
    pub book: &'a RuleBook,
    pub gam: &'a Gamdat,
}

impl Tables<'_> {
    pub fn items(&self) -> Items<'_> {
        Items { types: self.types, prices: self.prices }
    }
}

/// A picture of the world and the needs worked out from it, owned.
pub struct Snap {
    pub v: View,
    pub needs: super::needs::Needs,
    pub barter: super::trading::Barter,
}

impl<'a> Seat<'a> {
    pub fn tables(&self) -> Tables<'a> {
        Tables { types: self.types, prices: self.prices, productions: self.productions, book: self.book, gam: self.gam }
    }
}

impl Seat<'_> {
    pub fn pack_of(&self, who: Who) -> Option<&ItemBag> {
        match who {
            Who::Leader => Some(&self.rival.pack),
            Who::Follower(i) => self.rival.party.companions.get(i).map(|c| &c.pack),
            Who::Guard(l, i) => self.camps.get(l)?.residents.get(i).map(|r| &r.pack),
        }
    }

    pub fn kit_of(&self, who: Who) -> Option<Kit> {
        match who {
            Who::Leader => Some(self.ai.leader_kit),
            Who::Follower(i) => self.rival.party.companions.get(i).map(|c| c.kit),
            Who::Guard(l, i) => self.camps.get(l)?.residents.get(i).map(|r| r.kit),
        }
    }

    /// The pack and the kit, to change together.
    pub fn with_pack<R>(&mut self, who: Who, f: impl FnOnce(&mut ItemBag, &mut Kit) -> R) -> Option<R> {
        match who {
            Who::Leader => Some(f(&mut self.rival.pack, &mut self.ai.leader_kit)),
            Who::Follower(i) => self.rival.party.companions.get_mut(i).map(|c| f(&mut c.pack, &mut c.kit)),
            Who::Guard(l, i) => self.camps.at(l)?.residents.get_mut(i).map(|r| f(&mut r.pack, &mut r.kit)),
        }
    }

    pub fn experience_of(&self, who: Who) -> i32 {
        match who {
            Who::Leader => self.rival.experience,
            Who::Follower(i) => self.rival.party.companions.get(i).map_or(0, |c| c.experience),
            Who::Guard(l, i) => self.camps.get(l).and_then(|c| c.residents.get(i)).map_or(0, |r| r.experience),
        }
    }

    pub fn location_of(&self, who: Who) -> usize {
        match who {
            Who::Guard(l, _) => l,
            _ => self.here(),
        }
    }

    /// The group, the leader first.
    pub fn group(&self) -> Vec<Who> {
        let mut out = vec![Who::Leader];
        out.extend((0..self.rival.party.companions.len()).map(Who::Follower));
        out
    }

    /// The living guards of this opponent's camp at `l`.
    pub fn guards(&self, l: usize) -> Vec<Who> {
        let me = self.me();
        self.camps
            .get(l)
            .filter(|c| c.owner == Some(me))
            .map(|c| (0..c.residents.len()).filter(|&i| c.residents[i].needs.health > 0).map(|i| Who::Guard(l, i)).collect())
            .unwrap_or_default()
    }

    /// `Plans` for the needs: the settler's payment, the attack, the
    /// mission's danger, a doctor here.
    pub fn plans(&self, v: &View, policy: &super::policy::AiPolicy) -> super::needs::Plans {
        let t = self.tables();
        let me = self.me();
        let target = self.ai.target;
        let hostile = target.is_some_and(|l| v.sites.sites.get(l).is_some_and(|s| !s.city && s.owner.is_some_and(|o| o != me)));
        let has_doctor = self.room_ids.get(self.here()).is_some_and(|ids| ids.iter().any(|&id| RoomType::from_room_id(id) == RoomType::Doctor));
        let w = super::needs::World {
            items: t.items(),
            productions: t.productions,
            sites: &v.sites,
            player: me,
            policy,
            day: self.day,
            here: self.here(),
            equipment_demand: &v.demand,
            traders: &[],
        };
        let empty = super::needs::Needs::default();
        let kit = self.ai.leader_kit;
        let planner = super::hiring::Planner::new(&w, &empty, self.rival, &kit, self.ai, super::trading::Barter { factor: 100, benefit: 1.0 }, self.rival.previous, &v.people, &v.camp_rooms, &v.rivals_at);
        let capacity: i32 = group::carried(self.rival).iter().filter(|&&(_, _, k)| t.items().is_water_container(k)).map(|&(_, _, k)| t.items().water_capacity(k)).sum();
        super::needs::Plans {
            settlement_payment: planner.planned_settlement_payment(),
            has_attack_plan: hostile,
            attack_water_needed: capacity < super::needs::desired_water_container_capacity(group::size(self.rival) as i32),
            mission_hazard: target.and_then(|l| v.sites.sites.get(l)).map(|s| s.danger).filter(|d| *d != Danger::None),
            has_doctor,
        }
    }

    /// A fresh picture, its needs (the turn's, when the controller keeps
    /// them), and the barter terms.
    pub fn snap(&self, policy: &super::policy::AiPolicy, extended: bool) -> Snap {
        let v = self.view(policy, extended);
        if let Some(cached) = self.needs_cache.as_ref().and_then(|c| c.borrow().clone()) {
            return Snap { v, needs: cached, barter: super::trading::Barter { factor: self.book.barter_factor(self.difficulty), benefit: policy.trade_benefit } };
        }
        self.fresh_snap(v, policy)
    }

    /// The needs worked out now, whatever the turn keeps.
    pub fn fresh_needs(&self, policy: &super::policy::AiPolicy, extended: bool) -> super::needs::Needs {
        let v = self.view(policy, extended);
        self.fresh_snap(v, policy).needs
    }

    fn fresh_snap(&self, v: View, policy: &super::policy::AiPolicy) -> Snap {
        let plans = self.plans(&v, policy);
        let t = self.tables();
        let needs = {
            let w = super::needs::World {
                items: t.items(),
                productions: t.productions,
                sites: &v.sites,
                player: self.me(),
                policy,
                day: self.day,
                here: self.here(),
                equipment_demand: &v.demand,
                traders: &[],
            };
            super::needs::Needs::new(self.rival, self.ai, &w, plans)
        };
        Snap { v, needs, barter: super::trading::Barter { factor: self.book.barter_factor(self.difficulty), benefit: policy.trade_benefit } }
    }
}

impl Snap {
    /// The planner's world over this picture.
    pub fn world<'s>(&'s self, t: &Tables<'s>, policy: &'s super::policy::AiPolicy, player: usize, day: i32, here: usize) -> super::needs::World<'s> {
        super::needs::World {
            items: Items { types: t.types, prices: t.prices },
            productions: t.productions,
            sites: &self.v.sites,
            player,
            policy,
            day,
            here,
            equipment_demand: &self.v.demand,
            traders: &self.v.traders,
        }
    }
}

/// The rooms as `game::Storage` reads them, for the production rule. Read
/// only: what it cannot see (the ground, a pump) it answers as nothing.
/// The Amiga seat uses it too, for `JoinCamp`'s refresh.
pub struct SeatStorage<'s> {
    pub rooms: &'s RoomItems,
    pub room_ids: &'s [Vec<u8>],
    pub gam: &'s Gamdat,
    pub types: &'s ItemTypes,
    pub productions: &'s Productions,
}

impl crate::game::Storage for SeatStorage<'_> {
    fn count_in_rooms(&self, l: usize, kind: usize) -> usize {
        (0..self.room_ids.get(l).map_or(0, |r| r.len())).filter_map(|r| self.rooms.get(l, r)).map(|b| b.iter().filter(|&k| k == kind).count()).sum()
    }
    fn tools(&self, l: usize, production: usize) -> usize {
        let Some(p) = self.productions.get(production) else { return 0 };
        (0..self.room_ids.get(l).map_or(0, |r| r.len()))
            .filter_map(|r| self.rooms.get(l, r))
            .map(|b| b.iter().filter(|&k| self.types.name_of(k).is_some_and(|n| p.worked_by(n))).count())
            .sum()
    }
    fn any_room_has_space(&self, l: usize) -> bool {
        let ids = self.room_ids.get(l).map_or(&[][..], |r| r.as_slice());
        ids.iter().enumerate().any(|(r, &id)| {
            let cap = RoomType::from_room_id(id).item_capacity();
            cap > 0 && self.rooms.get(l, r).is_none_or(|b| b.len() < cap)
        })
    }
    fn store(&mut self, _: usize, _: usize) -> bool {
        false
    }
    fn pump_at(&self, _: usize) -> crate::game::Pump {
        crate::game::Pump::None
    }
    fn valid_productions(&self, l: usize) -> Vec<usize> {
        self.gam.locations.get(l).map(|x| x.available_products.iter().flatten().copied().collect()).unwrap_or_default()
    }
}

impl Seat<'_> {
    /// `Character.Hire`: the person at this place, by record, into the
    /// group, with his things, then `InitializeRecruit`'s food and water
    /// for an AI boss (`AiState.HireNpc`, `AiState.cs:1366`;
    /// `Character.cs:429-430`). False when he is not here to be hired.
    pub fn hire(&mut self, record: usize) -> bool {
        let here = self.here();
        let me = self.me();
        let found = self.people.at(here).find(|&i| self.people.get(i).is_some_and(|p| p.record == record && !p.dead && !p.hired && p.employer.is_none()));
        let Some(who) = found else { return false };
        let outfit = self.people.outfit(me);
        let Some(p) = self.people.get_mut(who) else { return false };
        p.hired = true;
        p.employer = Some(me);
        p.wear(outfit);
        let pack = core::mem::replace(&mut p.pack, ItemBag::new(crate::game::PACK_CAPACITY));
        let (mut needs, kit, experience, at) = (p.needs, p.kit, p.experience, p.at);
        (needs.food, needs.water) = crate::v12_economy::ai_recruit_supplies(self.book.rules.recruit_supplies);
        let id = self.rival.party.recruit();
        if let Some(c) = self.rival.party.companions.iter_mut().find(|c| c.id == id) {
            *c = crate::game::Companion { needs, pack, kit, experience, origin: Some(record), at, move_to: at, ..crate::game::Companion::new(id) };
        }
        true
    }

    /// `Character.LeaveCamp`: a guard of this camp here back into the group.
    /// `Character.Name` of a follower, as a report token (nothing for one
    /// with no `gam.dat` record, which only tests make).
    pub fn companion_name(&self, member: usize) -> String {
        self.rival.party.companions.get(member).and_then(|c| c.origin).map_or_else(String::new, super::telemetry::person)
    }

    /// `Character.Name` of one of this player's people, as a report token.
    pub fn who_name(&self, who: Who) -> String {
        match who {
            Who::Leader => super::telemetry::player(self.me()),
            Who::Follower(i) => self.companion_name(i),
            Who::Guard(l, r) => self.resident_name(l, r),
        }
    }

    /// `Character.Name` of one of the defenders here, as a report token.
    pub fn defender_name(&self, d: Defender) -> String {
        let here = self.here();
        let of = |origin: Option<usize>| origin.map_or_else(String::new, super::telemetry::person);
        match d {
            Defender::Camper(r) => self.resident_name(here, r),
            Defender::HumanLeader => self.human_here.as_ref().map_or_else(String::new, |h| super::telemetry::player(h.seat)),
            Defender::HumanCompanion(i) => self.human_here.as_ref().and_then(|h| h.party.companions.get(i)).map_or_else(String::new, |c| of(c.origin)),
            Defender::Leader(p) => super::telemetry::player(p),
            Defender::Companion(p, i) => self.others.iter().find(|r| r.player == p).and_then(|r| r.party.companions.get(i)).map_or_else(String::new, |c| of(c.origin)),
        }
    }

    /// `Character.Name` of a camper here.
    pub fn resident_name(&self, l: usize, resident: usize) -> String {
        self.camps.get(l).and_then(|c| c.residents.get(resident)).and_then(|c| c.origin).map_or_else(String::new, super::telemetry::person)
    }

    pub fn leave_camp(&mut self, resident: usize) -> bool {
        let here = self.here();
        let me = self.me();
        let Some((one, _)) = self.camps.dismiss(here, resident) else { return false };
        if let Some(record) = one.origin {
            self.people.set_hired(record, me);
        }
        self.rival.party.companions.push(one);
        true
    }

    /// `Character.Dismiss` for a follower: out of the group, staying here.
    pub fn dismiss_follower(&mut self, member: usize) -> bool {
        let here = self.here();
        let Some(one) = self.rival.party.dismiss(member) else { return false };
        if let Some(record) = one.origin {
            self.people.dismissed_companion(record, here, &one);
        }
        true
    }
}

impl Seat<'_> {
    /// This opponent's living guards at `l` as fighters, in [`Seat::guards`]'
    /// order.
    pub fn defence_of_own(&self, l: usize) -> Vec<Fighter> {
        self.guards(l)
            .into_iter()
            .filter_map(|w| match w {
                Who::Guard(l, i) => self.camps.get(l)?.residents.get(i).map(|r| Fighter {
                    class: self.class_of(r.origin),
                    experience: r.experience,
                    health: r.needs.health,
                    pack: r.pack.clone(),
                    kit: r.kit,
                    human: false,
                }),
                _ => None,
            })
            .collect()
    }

    /// Write the rooms of this opponent's camps back, in [`View::camp_rooms`]'
    /// order.
    pub fn write_camp_rooms(&mut self, rooms: &[Vec<ItemBag>]) {
        let me = self.me();
        let camps: Vec<usize> = (0..self.gam.locations.len()).filter(|&l| self.camps.get(l).and_then(|c| c.owner) == Some(me)).collect();
        for (l, bags) in camps.into_iter().zip(rooms.iter()) {
            for (r, b) in bags.iter().enumerate() {
                if self.rooms.get(l, r).is_some() || !b.is_empty() {
                    self.rooms.set(l, r, b.clone());
                }
            }
        }
    }
}

impl Seat<'_> {
    /// Every trader, as the planner sees him.
    pub fn trader_views(&self) -> Vec<super::needs::TraderView> {
        let mut out = Vec::new();
        for (index, t) in self.traders.iter().enumerate() {
            let shop = (0..self.gam.locations.len()).find(|&l| self.traders.shop_of(l) == Some(index));
            let alive = self.people.get_by_record(t.record).is_none_or(|p| !p.dead);
            let mut assortment: Vec<usize> = Vec::new();
            for name in self.stock_lists.of(t.name_id) {
                if let Some(k) = self.types.index_of(name) {
                    if !assortment.contains(&k) {
                        assortment.push(k);
                    }
                }
            }
            out.push(super::needs::TraderView { index, id: t.name_id as usize, at: t.at, shop, alive, stock: t.stock.clone(), assortment });
        }
        out
    }
}

/// The camps of one Modern seat, their rooms and springs: what
/// `CampManagement.UnloadGarrisonBelongings` reads and changes, whether a
/// turn calls it (a guard stationed) or a load does
/// ([`normalize_loaded_garrison_belongings`]).
pub struct Garrisons<'a> {
    pub me: usize,
    pub ai: &'a mut ModernAi,
    pub camps: &'a mut Camps,
    pub rooms: &'a mut RoomItems,
    pub springs: &'a mut Springs,
    pub types: &'a ItemTypes,
    pub prices: &'a Prices,
    pub room_ids: &'a [Vec<u8>],
}

impl Garrisons<'_> {
    fn capacity(&self, l: usize, room: usize) -> usize {
        self.room_ids.get(l).and_then(|ids| ids.get(room)).map_or(0, |&id| RoomType::from_room_id(id).item_capacity())
    }

    fn is_water(&self, l: usize, room: usize) -> bool {
        self.room_ids.get(l).and_then(|ids| ids.get(room)).is_some_and(|&id| RoomType::from_room_id(id) == RoomType::WaterSource)
    }

    fn has_space(&self, l: usize, room: usize) -> bool {
        let cap = self.capacity(l, room);
        cap > 0 && self.rooms.get(l, room).is_none_or(|b| b.len() < cap)
    }

    /// `CampManagement.StoreItemInCamp` once it has been allowed: a water
    /// container to the spring's room first (filled there when the reserve
    /// covers it), anything else away from it. False when no room has space.
    fn store(&mut self, l: usize, kind: usize, rounds: Option<i32>) -> bool {
        let rooms = self.room_ids.get(l).map_or(0, |ids| ids.len());
        let container = Items { types: self.types, prices: self.prices }.is_water_container(kind);
        let mut order: Vec<usize> = (0..rooms).filter(|&r| self.has_space(l, r)).collect();
        order.sort_by_key(|&r| if container { !self.is_water(l, r) } else { self.is_water(l, r) });
        let Some(&room) = order.first() else { return false };
        let mut kind = kind;
        if self.is_water(l, room) {
            if let Some(full) = self.types.full_of(kind) {
                let w = self.types.water_of(full);
                if let Some(s) = self.springs.at(l).filter(|s| s.reserve >= w) {
                    s.reserve -= w;
                    kind = full;
                }
            }
        }
        let cap = self.capacity(l, room);
        let bag = self.rooms.get_or_insert(l, room, || ItemBag::new(cap));
        give(bag, kind, rounds).is_some()
    }

    /// `CampManagement.UnloadGarrisonBelongings`: what a guard carries that
    /// is no weapon or protection (nor Reststop's bible) into the rooms,
    /// while they take it.
    fn unload(&mut self, l: usize, resident: usize) {
        let reststop = l == super::camps::RESTSTOP;
        let Some(pack) = self.camps.get(l).and_then(|c| c.residents.get(resident)).map(|r| r.pack.clone()) else { return };
        let t = self.types;
        let mut unloaded = 0;
        let mut slot = pack.len();
        while slot > 0 {
            slot -= 1;
            let Some(k) = pack.get(slot) else { continue };
            let ty = t.get(k);
            let keep = t.damage_of(k) > 0
                || ty.is_some_and(|x| x.is_class("weapon") || x.is_class("protection"))
                || reststop && super::needs::Needs::resting_sustenance(Items { types: self.types, prices: self.prices }, k);
            if keep {
                continue;
            }
            let rounds = pack.rounds_left(slot);
            if self.store(l, k, rounds) {
                unloaded += 1;
                if let Some(r) = self.camps.at(l).and_then(|c| c.residents.get_mut(resident)) {
                    let mut kit = r.kit;
                    take(&mut r.pack, &mut kit, slot);
                    r.kit = kit;
                }
            }
        }
        if unloaded > 0 {
            let name = self.camps.get(l).and_then(|c| c.residents.get(resident)).and_then(|c| c.origin).map_or_else(String::new, super::telemetry::person);
            self.ai.telemetry.report(|| alloc::format!("unloaded {unloaded} belongings from {name} into {} storage", super::telemetry::place(l)));
        }
    }

}

/// `CampManagement.NormalizeLoadedGarrisonBelongings` (`CampManagement.cs:102-111`),
/// the last call of `InitAfterLoad` (`AiState.cs:177`): in every camp of
/// this seat, every living guard of it unloads what he carries that is no
/// weapon or protection into the camp's rooms, as when he was stationed.
/// Not a no-op for a port-made save: stationing unloads only into rooms
/// with space, so a guard stationed at a full camp still carries his
/// belongings when the game is saved.
pub fn normalize_loaded_garrison_belongings(mut g: Garrisons) {
    let me = g.me;
    // `World.Locations` order: by location.
    let mut camps: Vec<(usize, usize)> = g
        .camps
        .iter()
        .filter(|(_, c)| c.owner == Some(me))
        .flat_map(|(l, c)| (0..c.residents.len()).filter(|&i| c.residents[i].needs.health > 0).map(move |i| (l, i)))
        .collect();
    camps.sort_by_key(|&(l, _)| l);
    for (l, resident) in camps {
        g.unload(l, resident);
    }
}
