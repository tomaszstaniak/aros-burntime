//! v1.2's nearby target for the keyboard and pad
//! (`Maps/MapViewOverlayNearbyAction.cs`): what is in reach of the selected
//! character, the nearest by default, and Tab's stable cycle over them.
//!
//! The game says what is near and how far ([`Candidate`]); this keeps the
//! order and the lock, as the overlay does from frame to frame.

use alloc::vec::Vec;

/// An item lying within this of the selected character is a candidate.
pub const ITEM_RANGE: f32 = 20.0;
/// A person within this.
pub const CHARACTER_RANGE: f32 = 35.0;
/// A person chosen by Tab stays the target out to this.
pub const CHARACTER_RELEASE_RANGE: f32 = 50.0;

/// What a target is. The game's own identities: a door by its entrance
/// number, an item by an identity the game gives it, a person by index, a
/// companion by id, the leader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Door(usize),
    Item(u64),
    Person(usize),
    Friend(usize),
    Leader,
}

impl Key {
    fn is_character(self) -> bool {
        matches!(self, Key::Person(_) | Key::Friend(_) | Key::Leader)
    }
}

/// Something near, and how near.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub key: Key,
    pub distance: f32,
}

/// The candidates as `UpdateOverlay` makes them: doors the game says are in
/// reach, items within [`ITEM_RANGE`], characters within [`CHARACTER_RANGE`];
/// nearest first, a stable sort keeping the game's order among equals.
pub fn candidates(doors: &[Candidate], items: &[Candidate], characters: &[Candidate]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = doors.to_vec();
    out.extend(items.iter().filter(|c| c.distance < ITEM_RANGE));
    out.extend(characters.iter().filter(|c| c.distance < CHARACTER_RANGE));
    out.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(core::cmp::Ordering::Equal));
    out
}

/// The overlay's memory: Tab's cycle, the target it locked, and for whom.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    cycle: Vec<Key>,
    locked: Option<Key>,
    /// The selected character the lock belongs to, by the game's identity.
    owner: Option<u64>,
}

impl Targets {
    /// `ClearTargetLock`: the mouse took over, or nobody is selected.
    pub fn clear(&mut self) {
        self.cycle.clear();
        self.locked = None;
        self.owner = None;
    }

    /// The target this frame (`UpdateOverlay`): the locked one while it is a
    /// candidate, or a person still within [`CHARACTER_RELEASE_RANGE`]
    /// (`retained` gives his distance); else the nearest candidate. A lock
    /// belongs to the character selected when it was made.
    pub fn current(&mut self, owner: u64, candidates: &[Candidate], retained: impl Fn(Key) -> Option<f32>) -> Option<Key> {
        if self.owner.is_some_and(|o| o != owner) {
            self.clear();
        }
        if let Some(locked) = self.locked {
            if candidates.iter().any(|c| c.key == locked) || Self::retains(locked, &retained) {
                return Some(locked);
            }
            self.clear();
        }
        candidates.first().map(|c| c.key)
    }

    fn retains(key: Key, retained: &impl Fn(Key) -> Option<f32>) -> bool {
        key.is_character() && retained(key).is_some_and(|d| d < CHARACTER_RELEASE_RANGE)
    }

    /// `CycleTarget`: the next (or previous) in a cycle that keeps its order
    /// while the candidates come and go, newcomers at its end. Nothing when
    /// nothing is near.
    pub fn cycle(&mut self, owner: u64, candidates: &[Candidate], forward: bool, retained: impl Fn(Key) -> Option<f32>) -> Option<Key> {
        if candidates.is_empty() {
            return None;
        }
        if self.owner != Some(owner) || self.cycle.is_empty() {
            self.locked = None;
            self.cycle = candidates.iter().map(|c| c.key).collect();
        } else {
            let locked = self.locked;
            self.cycle.retain(|k| candidates.iter().any(|c| c.key == *k) || Some(*k) == locked);
            for c in candidates {
                if !self.cycle.contains(&c.key) {
                    self.cycle.push(c.key);
                }
            }
        }
        if self.cycle.is_empty() {
            return None;
        }
        let current = self.locked.unwrap_or(candidates[0].key);
        let n = self.cycle.len() as isize;
        let at = match self.cycle.iter().position(|k| *k == current) {
            Some(i) => i as isize,
            None if forward => -1,
            None => 0,
        };
        let next = ((at + if forward { 1 } else { -1 }) % n + n) % n;
        let key = self.cycle[next as usize];
        self.locked = Some(key);
        self.owner = Some(owner);
        if candidates.iter().any(|c| c.key == key) || Self::retains(key, &retained) {
            Some(key)
        } else {
            None
        }
    }
}

/// v1.3's clicked target (`MapViewOverlayHoverText.SelectTarget` /
/// `ClearTarget`, `LocationScene.SelectInteractionTarget`): what a mouse
/// command sent the selected character to, with the place and the
/// selection it was set under. `S` is how the caller names a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClickedTarget<S> {
    pub key: Key,
    pub place: usize,
    pub who: S,
}

/// Why the location scene is not there to show it, or not the same one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Showing {
    /// A room, the pack window, INFO or another scene is open over it
    /// (`OnInactivateScene` / `OnActivateScene`).
    pub other_scene: bool,
    /// The keys or a pad are the input (`UpdateOverlayVisibility`).
    pub keyboard: bool,
    /// The place the player is in, `None` on the world map.
    pub place: Option<usize>,
}

impl<S: Copy + PartialEq> ClickedTarget<S> {
    /// `SelectInteractionTarget`: a command taken with the mouse, in a
    /// place, sets it; one taken at the keys does not.
    pub fn select(slot: &mut Option<Self>, by_mouse: bool, key: Key, place: Option<usize>, who: S) {
        if let (true, Some(place)) = (by_mouse, place) {
            *slot = Some(ClickedTarget { key, place, who });
        }
    }

    /// `ClearTarget`, every frame: the scene gone, the keys in use, another
    /// place or another selected character. Once cleared it stays cleared.
    pub fn keep(slot: &mut Option<Self>, now: Showing, who: S) {
        if slot.is_some_and(|t| now.other_scene || now.keyboard || now.place != Some(t.place) || t.who != who) {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod clicked_target {
    use super::*;

    fn here(other_scene: bool, keyboard: bool) -> Showing {
        Showing { other_scene, keyboard, place: Some(4) }
    }

    #[test]
    fn a_mouse_command_labels_its_target_until_upstream_clears_it() {
        let mut t: Option<ClickedTarget<u8>> = None;
        ClickedTarget::select(&mut t, false, Key::Door(1), Some(4), 0);
        assert!(t.is_none(), "a command at the keys sets nothing");
        ClickedTarget::select(&mut t, true, Key::Door(1), None, 0);
        assert!(t.is_none(), "not in a place");
        ClickedTarget::select(&mut t, true, Key::Person(9), Some(4), 0);
        ClickedTarget::keep(&mut t, here(false, false), 0);
        assert_eq!(t.map(|t| t.key), Some(Key::Person(9)), "kept while he walks or strikes");
        // A room opens over the place: gone, and not back when it closes.
        ClickedTarget::keep(&mut t, here(true, false), 0);
        ClickedTarget::keep(&mut t, here(false, false), 0);
        assert!(t.is_none());
        // Another character selected; the keys taking over; another place.
        for (now, who) in [(here(false, false), 1u8), (here(false, true), 0), (Showing { place: Some(5), ..Showing::default() }, 0)] {
            ClickedTarget::select(&mut t, true, Key::Item(7), Some(4), 0);
            ClickedTarget::keep(&mut t, now, who);
            assert!(t.is_none(), "{now:?} {who}");
        }
        // A new command replaces the old target.
        ClickedTarget::select(&mut t, true, Key::Door(2), Some(4), 0);
        ClickedTarget::select(&mut t, true, Key::Person(3), Some(4), 0);
        assert_eq!(t.map(|t| t.key), Some(Key::Person(3)));
    }
}
