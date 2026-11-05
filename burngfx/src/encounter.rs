//! `LocalCombatEncounter` (v1.2): one visible group attack, over time.
//!
//! A human's attack in a location is no longer one atomic exchange. Every
//! attacker approaches with its own weapon's reach and strikes once when in
//! range; a defender whose weapon reaches a whole class further strikes that
//! attacker first, while it closes, and that is its one answer to him. Only
//! when every attacker has struck or given up does the defender answer the
//! others, one at a time, walking up to each; and only after the last answer
//! does it flee — from whoever it answered last.
//!
//! The state machine is the source's, step for step. Movement, reach and the
//! blow itself belong to the world, behind [`Arena`], so the order of events
//! can be tested without a map.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// `AttackApproachTimeout`: an attacker who has not reached the defender in
/// ten seconds gives up.
pub const ATTACK_APPROACH_TIMEOUT: f32 = 10.0;
/// `RetaliationApproachTimeout`: after two seconds of failing to reach its
/// target, the defender's answer lands anyway.
pub const RETALIATION_APPROACH_TIMEOUT: f32 = 2.0;
/// `FLEE_DISTANCE`: how far a fleeing character runs.
pub const FLEE_DISTANCE: f32 = 70.0;

/// `WeaponReachRules`: a weapon's reach, in classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    Close,
    Medium,
    Long,
    Ranged,
}

impl Reach {
    pub fn from_range(range: f32) -> Reach {
        if range <= 16.0 {
            Reach::Close
        } else if range <= 22.0 {
            Reach::Medium
        } else if range <= 29.0 {
            Reach::Long
        } else {
            Reach::Ranged
        }
    }
}

/// `Character.AttackRange`: a mutant 16, a dog or a trader 24, anyone else
/// his weapon's `attack_range` if it has one, else 16.
pub fn attack_range(class: crate::gamdat::CharClass, weapon_range: Option<i32>) -> f32 {
    use crate::gamdat::CharClass;
    match class {
        CharClass::Mutant => 16.0,
        CharClass::Dog | CharClass::Trader => 24.0,
        _ => match weapon_range {
            Some(r) if r > 0 => r as f32,
            _ => 16.0,
        },
    }
}

/// What the encounter asks of the world.
pub trait Arena<C: Copy + PartialEq> {
    fn is_dead(&self, who: C) -> bool;
    /// `attacker.Location == target.Location`.
    fn same_place(&self, a: C, b: C) -> bool;
    /// `Character.AttackRange` now.
    fn attack_range(&self, who: C) -> f32;
    /// `IsInAttackRange(target, range)`: squared distance within range².
    fn in_range(&self, who: C, target: C, range: f32) -> bool;
    /// `ResolveSingleAttack`: one blow, `false` when either is already dead.
    fn strike(&mut self, attacker: C, defender: C) -> bool;
    fn begin_approach(&mut self, who: C, target: C, range: f32);
    fn clear_approach(&mut self, who: C, target: C);
    fn hold(&mut self, who: C);
    fn release(&mut self, who: C);
    fn flee(&mut self, who: C, from: C);
}

#[derive(Clone, Copy, Debug)]
struct Order<C> {
    attacker: C,
    defender: C,
    range: f32,
    elapsed: f32,
    defender_attacked: bool,
}

impl<C> Order<C> {
    fn new(attacker: C, defender: C, range: f32) -> Order<C> {
        Order { attacker, defender, range, elapsed: 0.0, defender_attacked: false }
    }
}

pub struct Encounter<C: Copy + PartialEq> {
    defender: C,
    attacks: Vec<Order<C>>,
    completed: Vec<C>,
    queue: VecDeque<Order<C>>,
    retaliation: Option<Order<C>>,
    flee_from: Option<C>,
    complete: bool,
}

impl<C: Copy + PartialEq> Encounter<C> {
    /// The living attackers other than the defender, each once, begin their
    /// approach; the defender is held.
    pub fn new(attackers: &[C], defender: C, arena: &mut impl Arena<C>) -> Encounter<C> {
        let mut participants: Vec<C> = Vec::new();
        for &a in attackers {
            if a != defender && !arena.is_dead(a) && !participants.contains(&a) {
                participants.push(a);
            }
        }
        // "An encounter owns the defender's movement."
        arena.hold(defender);
        let mut attacks = Vec::new();
        for a in participants {
            let order = Order::new(a, defender, arena.attack_range(a));
            arena.begin_approach(a, defender, order.range);
            attacks.push(order);
        }
        let complete = attacks.is_empty() || arena.is_dead(defender);
        if complete {
            arena.release(defender);
        }
        Encounter {
            defender,
            attacks,
            completed: Vec::new(),
            queue: VecDeque::new(),
            retaliation: None,
            flee_from: None,
            complete,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn update(&mut self, elapsed: f32, arena: &mut impl Arena<C>) {
        if self.complete {
            return;
        }
        if arena.is_dead(self.defender) {
            self.cancel(arena);
            return;
        }
        self.update_attacks(elapsed, arena);
        if arena.is_dead(self.defender) {
            self.cancel(arena);
            return;
        }
        // The whole group strikes before the defender moves.
        if !self.attacks.is_empty() {
            return;
        }
        self.update_retaliation(elapsed, arena);
        if self.attacks.is_empty() && self.retaliation.is_none() && self.queue.is_empty() {
            self.finish(arena);
        }
    }

    fn can_continue(arena: &impl Arena<C>, attacker: C, target: C) -> bool {
        !arena.is_dead(attacker) && !arena.is_dead(target) && arena.same_place(attacker, target)
    }

    fn update_attacks(&mut self, elapsed: f32, arena: &mut impl Arena<C>) {
        // `attacks.ToArray()`: a snapshot, each order handled once.
        let snapshot: Vec<Order<C>> = self.attacks.clone();
        for original in snapshot {
            let Some(index) = self.attacks.iter().position(|o| o.attacker == original.attacker) else { continue };
            let mut order = self.attacks[index];
            let attacker = order.attacker;
            if !Self::can_continue(arena, attacker, order.defender) {
                self.remove_attack(index, arena);
                continue;
            }
            // A defender with a longer reach intercepts while the attacker
            // closes — its one response to him.
            let defender_range = arena.attack_range(order.defender);
            if !order.defender_attacked
                && Reach::from_range(defender_range) > Reach::from_range(order.range)
                && arena.in_range(order.defender, attacker, defender_range)
            {
                order.defender_attacked = arena.strike(order.defender, attacker);
                self.attacks[index] = order;
                if !Self::can_continue(arena, attacker, order.defender) {
                    self.remove_attack(index, arena);
                    continue;
                }
            }
            if !arena.in_range(attacker, order.defender, order.range) {
                order.elapsed += elapsed;
                self.attacks[index] = order;
                if order.elapsed >= ATTACK_APPROACH_TIMEOUT {
                    self.remove_attack(index, arena);
                }
                continue;
            }
            let attacked = arena.strike(attacker, order.defender);
            self.attacks.remove(index);
            if attacked && !arena.is_dead(order.defender) {
                if !self.completed.contains(&attacker) {
                    self.completed.push(attacker);
                }
                if order.defender_attacked {
                    arena.clear_approach(attacker, order.defender);
                    self.flee_from = Some(attacker);
                } else {
                    arena.hold(attacker);
                    let range = arena.attack_range(self.defender);
                    self.queue.push_back(Order::new(self.defender, attacker, range));
                }
            } else {
                arena.clear_approach(attacker, order.defender);
            }
        }
    }

    fn update_retaliation(&mut self, elapsed: f32, arena: &mut impl Arena<C>) {
        if self.retaliation.is_none() {
            while let Some(queued) = self.queue.pop_front() {
                if !Self::can_continue(arena, queued.attacker, queued.defender) {
                    arena.release(queued.defender);
                    continue;
                }
                arena.begin_approach(self.defender, queued.defender, queued.range);
                self.retaliation = Some(queued);
                break;
            }
        }
        let Some(mut order) = self.retaliation else { return };
        if !Self::can_continue(arena, order.attacker, order.defender) {
            self.complete_retaliation(order, arena);
            return;
        }
        if !arena.in_range(order.attacker, order.defender, order.range) {
            order.elapsed += elapsed;
            self.retaliation = Some(order);
            if order.elapsed >= RETALIATION_APPROACH_TIMEOUT {
                // "Every successful group strike earns a response."
                arena.strike(order.attacker, order.defender);
                self.complete_retaliation(order, arena);
            }
            return;
        }
        arena.strike(order.attacker, order.defender);
        self.complete_retaliation(order, arena);
    }

    fn complete_retaliation(&mut self, order: Order<C>, arena: &mut impl Arena<C>) {
        arena.clear_approach(order.attacker, order.defender);
        arena.release(order.defender);
        self.retaliation = None;
        self.flee_from = Some(order.defender);
        if !self.queue.is_empty() {
            arena.hold(self.defender);
        }
    }

    fn remove_attack(&mut self, index: usize, arena: &mut impl Arena<C>) {
        let order = self.attacks.remove(index);
        arena.clear_approach(order.attacker, order.defender);
    }

    /// `CancelOffense`: every attacker still approaching gives up.
    pub fn cancel_offense(&mut self, arena: &mut impl Arena<C>) {
        while !self.attacks.is_empty() {
            self.remove_attack(0, arena);
        }
    }

    /// `ReleaseAfterCompletedAttack`: an attacker who has struck may move
    /// again while the rest of the group finishes.
    pub fn release_after_completed_attack(&mut self, attacker: C, arena: &mut impl Arena<C>) -> bool {
        if !self.completed.contains(&attacker) {
            return false;
        }
        arena.release(attacker);
        true
    }

    pub fn cancel(&mut self, arena: &mut impl Arena<C>) {
        self.cancel_offense(arena);
        if let Some(r) = self.retaliation.take() {
            arena.clear_approach(r.attacker, r.defender);
            arena.release(r.defender);
        }
        for q in self.queue.drain(..) {
            arena.release(q.defender);
        }
        arena.release(self.defender);
        self.complete = true;
    }

    fn finish(&mut self, arena: &mut impl Arena<C>) {
        arena.release(self.defender);
        if let Some(from) = self.flee_from {
            if !arena.is_dead(self.defender) {
                arena.flee(self.defender, from);
            }
        }
        self.complete = true;
    }
}
