//! The doctor: medicine for health.
//!
//! **A doctor is a room, not a person.** `MapProcessor` marks an entrance
//! `RoomType.Doctor` — room id 20 — and walking into it is what opens the
//! surgery. The other doctor in this game is a member of your own group,
//! who changes how fast everyone mends overnight; that is a different rule
//! and lives in [`crate::game`].
//!
//! Ported from v0.9.1's `DoctorScene`.
//!
//! # What the source actually says
//!
//! * **Only medicine may go on the table.** `OnLeftClickItemInventory`
//!   begins `if ((state as Item).HealValue == 0) return;`, so an item that
//!   heals nothing cannot be offered at all.
//! * **The table holds four.** `grid.Grid = new Vector2(4, 1)`.
//! * **The healing is the sum, truncated once.** `grid.GetHealValue()` is
//!   the same `float v = 0; foreach … v += item.HealValue; return (int)v`
//!   that prices a trade.
//! * **Health stops at 100 and the excess is lost.** `Health`'s setter is
//!   `set { health = value; if (IsDead) Die(); if (health > 100) health =
//!   100; }`. There is no refund and no warning: overpaying is simply
//!   thrown away.
//! * **The patient is whoever's page is showing.** `Health += value` goes
//!   to `inventory.ActiveCharacter`, and the window pages through the whole
//!   group — so a companion can be the one treated.
//! * **The medicine is what pays.** There is no separate fee; the items are
//!   both the price and the cure, and they are gone afterwards.
//!
//! # Where we differ, and why
//!
//! Upstream takes an item out of the pack **as it is placed on the table**
//! and puts it back only if you leave by the Exit button. So the panel is
//! not a preview: half the transaction has already happened while you are
//! looking at it.
//!
//! [`check`] marks slots instead and changes nothing, and [`heal`] works
//! the verdict out again before moving anything — the same shape as
//! [`crate::trade`] and [`crate::hire`]. A refusal therefore costs nothing,
//! which cannot be said of the original.
//!
//! **A v1.2 game takes instead.** v1.2's `ServiceScene`
//! (`Scenes/ServiceScene.cs:328-350`) is followed there: the caller moves
//! each thing out of the pack shown onto a real table, keeps the table when
//! the page changes, gives it back to the group on EXIT, and treats the
//! page shown at HEAL with [`heal_from_table`]. The legacy game keeps the
//! preview.
//!
//! And **medicine does not raise the dead**. Upstream has no such check and
//! does not need one: `Die` dismisses the character, so a corpse is never
//! on the inventory page for the doctor to reach. That is a rule enforced
//! by circumstance rather than by code, and circumstance is not a thing to
//! rely on — [`Refusal::TheyAreDead`] says it outright.

use alloc::vec::Vec;

use crate::game::{CharacterNeeds, ItemBag, MAX_HEALTH};
use crate::items::ItemTypes;

/// `grid.Grid = new Vector2(4, 1)`: four things at a time.
pub const TABLE: usize = 4;

/// Why the doctor will not do it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Nothing was offered, or nothing that heals.
    NothingToTreatWith,
    /// An item whose `heal` is zero. `OnLeftClickItemInventory` refuses to
    /// pick such a thing up at all.
    NotMedicine,
    /// More than [`TABLE`] things offered.
    TooMuchOnTheTable,
    /// A slot that holds nothing.
    NoSuchSlot,
    /// The same slot twice. One item is one item.
    SlotNamedTwice,
    /// No such patient in the group.
    NoSuchPatient,
    /// **Fifty or more from the leader.** `DoctorScene` builds its pages
    /// from `InventoryWindow.SetGroup`, which keeps only members
    /// `group.IsInRange(leader, member)` — so somebody standing off has no
    /// page: they cannot be the patient and their pack cannot pay. `who` is
    /// the pack, 0 being the leader's.
    OutOfReach { who: usize },
    /// **The patient is dead.** Upstream never has to say this, because a
    /// dead man has already been dismissed and so cannot be on the
    /// inventory page at all. Ours says it, because a rule that holds only
    /// because nothing happens to reach it is not a rule.
    TheyAreDead,
}

/// What the panel needs to show before anything is spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    /// What the medicine is worth, summed and truncated the way the
    /// original sums it.
    pub heals: i32,
    pub from: i32,
    /// Where the patient ends up — never above [`MAX_HEALTH`].
    pub to: i32,
    /// **What the cap throws away.** The original neither warns nor
    /// refunds; showing it is ours, and it is the difference between a
    /// deliberate overpayment and a surprise.
    pub wasted: i32,
}

/// One pick: which pack, and which slot in it.
pub type Pick = (usize, usize);

/// What the medicine on the table is worth.
///
/// Summed as fractions and truncated once, like every other total in this
/// game. `heal` is a whole number in the item table, so the fractions do
/// not arise today — it is written this way because the moment one does,
/// rounding each item first would be wrong.
pub fn heal_value(kinds: &[usize], types: &ItemTypes) -> i32 {
    let mut total = 0.0f32;
    for &kind in kinds {
        total += types.heal_of(kind) as f32;
    }
    total as i32
}

/// Would this treatment work, and what would it do? Changes nothing.
///
/// `reach` is one flag per pack, in the same order, and `patient` names the
/// one being treated: both are the inventory window's pages, which exist
/// only for members within fifty of the leader. Index 0 is the leader, who
/// is always in reach of themselves.
pub fn check(
    packs: &[&ItemBag],
    reach: &[bool],
    patient_is: usize,
    patient: &CharacterNeeds,
    picks: &[Pick],
    types: &ItemTypes,
) -> Result<Standing, Refusal> {
    check_with(packs, reach, patient_is, patient, picks, types, Cure::Legacy)
}

/// Whose surgery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cure {
    /// v0.9.1's: health plus the medicine, capped at a hundred.
    Legacy,
    /// v1.2's `DoctorResult`: the medicine times `healing_factor`, capped
    /// at `health_cap`, never below where the patient was.
    V12 { factor: f32, cap: i32 },
}

/// [`check`], in the given surgery.
pub fn check_with(
    packs: &[&ItemBag],
    reach: &[bool],
    patient_is: usize,
    patient: &CharacterNeeds,
    picks: &[Pick],
    types: &ItemTypes,
    cure: Cure,
) -> Result<Standing, Refusal> {
    // Whether this patient can be treated at all comes first: upstream has
    // no page for them, so nothing else about the table is worth saying.
    if patient_is != 0 && !reach.get(patient_is).copied().unwrap_or(false) {
        return Err(Refusal::OutOfReach { who: patient_is });
    }
    if picks.is_empty() {
        return Err(Refusal::NothingToTreatWith);
    }
    if picks.len() > TABLE {
        return Err(Refusal::TooMuchOnTheTable);
    }

    let mut medicine = Vec::new();
    for (i, &(bag, slot)) in picks.iter().enumerate() {
        if picks[..i].contains(&(bag, slot)) {
            return Err(Refusal::SlotNamedTwice);
        }
        // A pack that is not there at all is a stale pick, not somebody
        // standing too far off — so that question is asked first.
        let pack = packs.get(bag).ok_or(Refusal::NoSuchSlot)?;
        if bag != 0 && !reach.get(bag).copied().unwrap_or(false) {
            return Err(Refusal::OutOfReach { who: bag });
        }
        let kind = pack.get(slot).ok_or(Refusal::NoSuchSlot)?;
        if types.heal_of(kind) == 0 {
            return Err(Refusal::NotMedicine);
        }
        medicine.push(kind);
    }

    let heals = heal_value(&medicine, types);
    if heals == 0 {
        return Err(Refusal::NothingToTreatWith);
    }
    let from = patient.health;
    if from <= 0 {
        return Err(Refusal::TheyAreDead);
    }
    match cure {
        Cure::Legacy => {
            let to = (from + heals).min(MAX_HEALTH);
            Ok(Standing { heals, from, to, wasted: (from + heals - MAX_HEALTH).max(0) })
        }
        Cure::V12 { factor, cap } => {
            let to = crate::v12_economy::doctor_result(from, heals, factor, cap);
            let gained = (heals as f32 * factor) as i32;
            Ok(Standing { heals, from, to, wasted: (from + gained - to).max(0) })
        }
    }
}

/// Treat them. **Re-checks first, and then either everything moves or
/// nothing does.**
///
/// The medicine goes whether or not it was all needed — the cap throws the
/// surplus away rather than returning it, which is the original's own
/// behaviour and worth being deliberate about.
pub fn heal(
    packs: &mut [&mut ItemBag],
    reach: &[bool],
    patient_is: usize,
    patient: &mut CharacterNeeds,
    picks: &[Pick],
    types: &ItemTypes,
) -> Result<Standing, Refusal> {
    heal_with(packs, reach, patient_is, patient, picks, types, Cure::Legacy)
}

/// [`heal`], in the given surgery.
pub fn heal_with(
    packs: &mut [&mut ItemBag],
    reach: &[bool],
    patient_is: usize,
    patient: &mut CharacterNeeds,
    picks: &[Pick],
    types: &ItemTypes,
    cure: Cure,
) -> Result<Standing, Refusal> {
    let standing = {
        let borrowed: Vec<&ItemBag> = packs.iter().map(|b| &**b).collect();
        check_with(&borrowed, reach, patient_is, patient, picks, types, cure)?
    };

    // Highest slot first, so removing one does not renumber the next.
    let mut order: Vec<Pick> = picks.to_vec();
    order.sort_unstable_by(|a, b| b.cmp(a));
    for &(bag, slot) in &order {
        packs[bag].remove(slot);
    }
    patient.health = standing.to;
    Ok(standing)
}

/// v1.2's HEAL (`ServiceScene.CommitOffer`, `Scenes/ServiceScene.cs:219-223`,
/// then `offer.Clear()`, `:318-326`): the patient is whoever's page is shown,
/// `table` is what was taken out of the packs — anybody's — as it was
/// placed. Everything on it is spent, whatever the cap leaves of it; an
/// empty table changes nothing (`DoctorResult(health, 0)` is the health).
pub fn heal_from_table(
    patient: &mut CharacterNeeds,
    table: &mut ItemBag,
    types: &ItemTypes,
    cure: Cure,
) -> Result<Standing, Refusal> {
    heal_from_table_carrying(patient, &mut crate::danger::Exposure::default(), table, types, cure)
}

/// [`heal_from_table`] for a patient carrying part of a danger's point: in a
/// v1.2 game `Health = CalculateDoctorResult(Health, offer)` reads the
/// getter and writes through the setter (`ServiceScene.cs:219-220`), so the
/// float's fraction is gone, whatever the table holds.
pub fn heal_from_table_carrying(
    patient: &mut CharacterNeeds,
    carry: &mut crate::danger::Exposure,
    table: &mut ItemBag,
    types: &ItemTypes,
    cure: Cure,
) -> Result<Standing, Refusal> {
    if patient.health <= 0 {
        return Err(Refusal::TheyAreDead);
    }
    if matches!(cure, Cure::V12 { .. }) {
        crate::danger::spend(&mut patient.health, carry);
    }
    let kinds: Vec<usize> = table.iter().collect();
    let heals = heal_value(&kinds, types);
    let from = patient.health;
    let (to, gained) = match cure {
        Cure::Legacy => ((from + heals).min(MAX_HEALTH).max(from), heals),
        Cure::V12 { factor, cap } => {
            (crate::v12_economy::doctor_result(from, heals, factor, cap), (heals as f32 * factor) as i32)
        }
    };
    while table.remove(0).is_some() {}
    patient.health = to;
    Ok(Standing { heals, from, to, wasted: (from + gained - to).max(0) })
}

/// The first of the three lines (`burn?…`) the doctor says after HEAL.
///
/// v1.2's `UpdateDoctorText` (`Scenes/ServiceScene.cs:281-300`) works the
/// gain out again **after** the health is set, with the offer still on the
/// table: at the cap, or with medicine the healing factor rounds to
/// nothing, it is 0 and the line is 522 ("nothing more I can do"). The
/// legacy game says 522 only for a table that heals nothing. Otherwise the
/// health after decides: 45 or less 516, 60 or less 513, 95 or less 510,
/// above that 519.
pub fn said_after(s: &Standing, cure: Cure) -> u32 {
    let nothing = match cure {
        Cure::Legacy => s.heals == 0,
        Cure::V12 { factor, cap } => crate::v12_economy::doctor_result(s.to, s.heals, factor, cap) == s.to,
    };
    if nothing {
        522
    } else if s.to <= 45 {
        516
    } else if s.to <= 60 {
        513
    } else if s.to <= 95 {
        510
    } else {
        519
    }
}
