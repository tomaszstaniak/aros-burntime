//! The doctor: medicine for health.

use burngfx::doctor::{check, heal, heal_value, Refusal, Standing, TABLE};
use burngfx::game::{CharacterNeeds, ItemBag, MAX_HEALTH, PACK_CAPACITY};
use burngfx::items::ItemTypes;

/// A snake is `food=7 heal=28` in the real table; bandages are made up but
/// shaped like the data. A knife heals nothing, which is the case that
/// matters most here.
fn types() -> ItemTypes {
    ItemTypes::parse(
        "[item_snake]\nclass=food\nburngfx=2\nfood=7\nheal=28\n\
         [item_bandage]\nclass=medical\nburngfx=40\nheal=10\n\
         [item_pill]\nclass=medical\nburngfx=41\nheal=3\n\
         [item_knife]\nclass=tool\nburngfx=10\n",
    )
}

fn kind(t: &ItemTypes, name: &str) -> usize {
    t.index_of(name).unwrap_or_else(|| panic!("{name}"))
}

fn hurt(health: i32) -> CharacterNeeds {
    CharacterNeeds { food: 5, max_food: 9, water: 3, max_water: 5, health }
}

fn pack(t: &ItemTypes, names: &[&str]) -> ItemBag {
    let kinds: Vec<usize> = names.iter().map(|n| kind(t, n)).collect();
    ItemBag::with(PACK_CAPACITY, &kinds)
}

// -- what medicine is worth --------------------------------------------------

#[test]
fn heal_values_come_from_the_item_table() {
    let t = types();
    assert_eq!(heal_value(&[kind(&t, "item_snake")], &t), 28);
    assert_eq!(heal_value(&[kind(&t, "item_bandage")], &t), 10);
    assert_eq!(heal_value(&[kind(&t, "item_knife")], &t), 0);
    assert_eq!(heal_value(&[], &t), 0);
}

/// Several at once add up.
#[test]
fn the_table_adds_up() {
    let t = types();
    let (b, p) = (kind(&t, "item_bandage"), kind(&t, "item_pill"));
    assert_eq!(heal_value(&[b, b, p], &t), 23);
}

// -- what the panel is told --------------------------------------------------

#[test]
fn a_treatment_says_what_it_would_do() {
    let t = types();
    let bag = pack(&t, &["item_bandage", "item_knife"]);
    let got = check(&[&bag], &[true], 0, &hurt(50), &[(0, 0)], &t).expect("a bandage is medicine");
    assert_eq!(got, Standing { heals: 10, from: 50, to: 60, wasted: 0 });
}

/// **Health stops at 100 and the surplus is thrown away.** `Health`'s
/// setter clamps and neither warns nor refunds; the only thing that is ours
/// is saying how much was wasted.
#[test]
fn healing_past_full_wastes_the_rest() {
    let t = types();
    let bag = pack(&t, &["item_snake"]);
    let got = check(&[&bag], &[true], 0, &hurt(90), &[(0, 0)], &t).expect("a snake heals 28");
    assert_eq!(got.to, MAX_HEALTH);
    assert_eq!(got.wasted, 18, "90 + 28 is 118, and eighteen of it is lost");
}

/// At full health the whole thing is wasted — and it is still allowed,
/// because the original allows it.
#[test]
fn treating_somebody_already_well_is_allowed_and_pointless() {
    let t = types();
    let mut bag = pack(&t, &["item_bandage"]);
    let mut patient = hurt(MAX_HEALTH);
    let got = {
        let mut packs = [&mut bag];
        heal(&mut packs, &[true], 0, &mut patient, &[(0, 0)], &t).expect("the doctor does not refuse")
    };
    assert_eq!(got.wasted, 10);
    assert_eq!(patient.health, MAX_HEALTH);
    assert!(bag.is_empty(), "and the bandage is gone all the same");
}

// -- refusals ----------------------------------------------------------------

/// **An item that heals nothing cannot be offered.**
/// `OnLeftClickItemInventory` will not even pick it up.
#[test]
fn a_knife_is_not_medicine() {
    let t = types();
    let bag = pack(&t, &["item_knife", "item_bandage"]);
    assert_eq!(check(&[&bag], &[true], 0, &hurt(50), &[(0, 0)], &t), Err(Refusal::NotMedicine));
    check(&[&bag], &[true], 0, &hurt(50), &[(0, 1)], &t).expect("the bandage is fine");
}

#[test]
fn an_empty_table_treats_nobody() {
    let t = types();
    let bag = pack(&t, &["item_bandage"]);
    assert_eq!(check(&[&bag], &[true], 0, &hurt(50), &[], &t), Err(Refusal::NothingToTreatWith));
}

/// Four at a time, because the grid is four wide and one tall.
#[test]
fn the_table_holds_four() {
    let t = types();
    let bag = pack(&t, &["item_pill", "item_pill", "item_pill", "item_pill", "item_pill"]);
    let four: Vec<(usize, usize)> = (0..TABLE).map(|s| (0, s)).collect();
    check(&[&bag], &[true], 0, &hurt(50), &four, &t).expect("four is the limit");
    let five: Vec<(usize, usize)> = (0..TABLE + 1).map(|s| (0, s)).collect();
    assert_eq!(check(&[&bag], &[true], 0, &hurt(50), &five, &t), Err(Refusal::TooMuchOnTheTable));
}

/// **One item cannot heal twice.** The same fault the trade panel had.
#[test]
fn one_item_cannot_be_used_twice() {
    let t = types();
    let mut bag = pack(&t, &["item_snake", "item_bandage"]);
    let mut patient = hurt(10);
    let out = {
        let mut packs = [&mut bag];
        heal(&mut packs, &[true], 0, &mut patient, &[(0, 0), (0, 0)], &t)
    };
    assert_eq!(out, Err(Refusal::SlotNamedTwice));
    assert_eq!(patient.health, 10, "and nothing was healed");
    assert_eq!(bag.len(), 2, "nor spent");
}

#[test]
fn a_slot_with_nothing_in_it() {
    let t = types();
    let bag = pack(&t, &["item_bandage"]);
    assert_eq!(check(&[&bag], &[true], 0, &hurt(50), &[(0, 5)], &t), Err(Refusal::NoSuchSlot));
    assert_eq!(check(&[&bag], &[true], 0, &hurt(50), &[(9, 0)], &t), Err(Refusal::NoSuchSlot));
}

// -- doing it ----------------------------------------------------------------

#[test]
fn the_medicine_is_spent_and_the_health_arrives() {
    let t = types();
    let mut bag = pack(&t, &["item_knife", "item_bandage", "item_pill"]);
    let mut patient = hurt(40);
    let got = {
        let mut packs = [&mut bag];
        heal(&mut packs, &[true], 0, &mut patient, &[(0, 1), (0, 2)], &t).expect("13 worth")
    };
    assert_eq!(got.heals, 13);
    assert_eq!(patient.health, 53);
    assert_eq!(bag.iter().collect::<Vec<_>>(), [kind(&t, "item_knife")], "the knife is untouched");
}

/// **A refusal costs nothing.**
#[test]
fn a_refused_treatment_changes_nothing() {
    let t = types();
    let mut bag = pack(&t, &["item_knife"]);
    let mut patient = hurt(30);
    let out = {
        let mut packs = [&mut bag];
        heal(&mut packs, &[true], 0, &mut patient, &[(0, 0)], &t)
    };
    assert_eq!(out, Err(Refusal::NotMedicine));
    assert_eq!(patient.health, 30);
    assert_eq!(bag.len(), 1);
}

/// Checking is free, however often.
#[test]
fn checking_changes_nothing() {
    let t = types();
    let bag = pack(&t, &["item_bandage"]);
    let patient = hurt(50);
    for _ in 0..3 {
        check(&[&bag], &[true], 0, &patient, &[(0, 0)], &t).expect("fine");
    }
    assert_eq!(bag.len(), 1);
    assert_eq!(patient.health, 50);
}

/// Picking several slots takes the ones named, not the ones that end up in
/// those positions afterwards.
#[test]
fn several_slots_at_once() {
    let t = types();
    let mut bag = pack(&t, &["item_pill", "item_knife", "item_bandage", "item_snake"]);
    let mut patient = hurt(20);
    let got = {
        let mut packs = [&mut bag];
        heal(&mut packs, &[true], 0, &mut patient, &[(0, 0), (0, 2)], &t).expect("pill and bandage")
    };
    assert_eq!(got.heals, 13);
    assert_eq!(
        bag.iter().collect::<Vec<_>>(),
        [kind(&t, "item_knife"), kind(&t, "item_snake")],
        "the knife and the snake stayed"
    );
}

// -- the whole group ---------------------------------------------------------

/// **A companion can be the patient, and can supply the medicine.**
/// `inventory.ActiveCharacter` is whoever's page is showing and the window
/// pages through the group.
#[test]
fn a_companion_can_be_treated_from_anybodys_pack() {
    let t = types();
    let mut leader = pack(&t, &["item_snake"]);
    let mut friend = pack(&t, &["item_bandage"]);
    let mut patient = hurt(35); // the companion

    let got = {
        let mut packs = [&mut leader, &mut friend];
        // The leader's snake and the companion's own bandage together.
        heal(&mut packs, &[true, true], 1, &mut patient, &[(0, 0), (1, 0)], &t).expect("38 worth")
    };
    assert_eq!(got.heals, 38);
    assert_eq!(patient.health, 73);
    assert!(leader.is_empty(), "the leader's snake went");
    assert!(friend.is_empty(), "and so did the companion's bandage");
}

// -- against the shipped table -----------------------------------------------

/// The classic data really does have medicine in it, and a snake is the
/// oddity the module docs keep pointing at: it feeds *and* heals.
#[test]
fn the_shipped_items_carry_heal_values() {
    let Ok(text) = std::fs::read_to_string("testdata/items_original.txt") else { return };
    let types = ItemTypes::parse(&text);
    let snake = types.index_of("item_snake").expect("snakes exist");
    assert_eq!(types.heal_of(snake), 28, "a snake heals 28 and feeds 7");
    assert_eq!(types.food_of(snake), 7);

    let medicines = (0..200).filter(|&i| types.heal_of(i) > 0).count();
    assert!(medicines > 1, "the doctor has more than one thing to work with");
}

/// **Medicine does not raise the dead.**
///
/// Upstream never says so because it never has to: `Die` dismisses the
/// character, so a corpse cannot be on the inventory page. Relying on that
/// is relying on circumstance, and the moment a corpse could be reached the
/// silence would become a resurrection.
#[test]
fn the_dead_cannot_be_treated() {
    let t = types();
    let mut bag = pack(&t, &["item_snake"]);
    let mut corpse = hurt(0);
    let out = {
        let mut packs = [&mut bag];
        heal(&mut packs, &[true], 0, &mut corpse, &[(0, 0)], &t)
    };
    assert_eq!(out, Err(Refusal::TheyAreDead));
    assert_eq!(corpse.health, 0, "still dead");
    assert_eq!(bag.len(), 1, "and the snake was not wasted on them");

    // Negative health is the same answer, for a state that should not
    // arise now that dying sets zero — but might, from an old save.
    let mut worse = hurt(-48);
    let mut packs = [&mut bag];
    assert_eq!(heal(&mut packs, &[true], 0, &mut worse, &[(0, 0)], &t), Err(Refusal::TheyAreDead));
}

// -- reach -------------------------------------------------------------------

/// **A companion fifty or more away has no page**, so the doctor cannot be
/// pointed at them. `InventoryWindow.SetGroup` builds pages only for members
/// `IsInRange` of the leader, and every scene with an inventory works that
/// way.
#[test]
fn a_far_companion_cannot_be_the_patient() {
    let t = types();
    let mut leader = pack(&t, &["item_bandage"]);
    let mut friend = pack(&t, &["item_bandage"]);
    let mut patient = hurt(40);
    let out = {
        let mut packs = [&mut leader, &mut friend];
        heal(&mut packs, &[true, false], 1, &mut patient, &[(0, 0)], &t)
    };
    assert_eq!(out, Err(Refusal::OutOfReach { who: 1 }));
    assert_eq!(patient.health, 40, "and nothing was spent on them");
    assert_eq!(leader.len(), 1);
    assert_eq!(friend.len(), 1);
}

/// Nor can their pack pay for somebody who is in reach.
#[test]
fn a_far_companions_pack_cannot_pay() {
    let t = types();
    let mut leader = pack(&t, &["item_pill"]);
    let mut friend = pack(&t, &["item_snake"]);
    let mut patient = hurt(40);
    let out = {
        let mut packs = [&mut leader, &mut friend];
        heal(&mut packs, &[true, false], 0, &mut patient, &[(0, 0), (1, 0)], &t)
    };
    assert_eq!(out, Err(Refusal::OutOfReach { who: 1 }));
    assert_eq!(patient.health, 40);
    assert_eq!(friend.len(), 1, "their snake stayed where it was");
    assert_eq!(leader.len(), 1, "and so did the pill: all or nothing");
}

/// The leader is always in reach of themselves, whatever the flags say about
/// anybody else.
#[test]
fn the_leader_is_always_in_reach() {
    let t = types();
    let mut leader = pack(&t, &["item_bandage"]);
    let mut friend = pack(&t, &["item_snake"]);
    let mut patient = hurt(40);
    let got = {
        let mut packs = [&mut leader, &mut friend];
        heal(&mut packs, &[true, false], 0, &mut patient, &[(0, 0)], &t).expect("their own bandage")
    };
    assert_eq!(got.heals, 10);
    assert_eq!(patient.health, 50);
}

// -- v1.2: the table takes, the patient is the page shown ---------------------

/// **The medicine on the table heals whoever's page is shown at HEAL**
/// (`ServiceScene.CommitOffer`, `Scenes/ServiceScene.cs:219-223`): it may have
/// come out of anybody's pack. Here the bandage is a companion's; the leader
/// is treated with it. The table is empty afterwards.
#[test]
fn v12_the_table_heals_the_page_shown() {
    use burngfx::doctor::{heal_from_table, Cure};
    let t = types();
    let mut companion = pack(&t, &["item_bandage", "item_knife"]);
    let mut table = ItemBag::new(TABLE);
    burngfx::game::move_item(&mut companion, 0, &mut table).expect("onto the table");
    let mut leader = hurt(40);

    let got = heal_from_table(&mut leader, &mut table, &t, Cure::V12 { factor: 1.0, cap: 95 }).expect("treated");

    assert_eq!(got, Standing { heals: 10, from: 40, to: 50, wasted: 0 });
    assert_eq!(leader.health, 50);
    assert!(table.is_empty(), "the bandage is spent");
    assert_eq!(companion.len(), 1, "and it left the companion's pack when it was placed");
}

/// HEAL with nothing on the table: `DoctorResult(health, 0)` is the health
/// itself (`Logic/Rules/RuleFormulas.cs:56-57`) — nothing changes.
#[test]
fn v12_an_empty_table_changes_nothing() {
    use burngfx::doctor::{heal_from_table, Cure};
    let t = types();
    let mut table = ItemBag::new(TABLE);
    let mut patient = hurt(99);
    let got = heal_from_table(&mut patient, &mut table, &t, Cure::V12 { factor: 1.0, cap: 95 }).expect("nothing to do");
    assert_eq!((got.heals, got.from, got.to), (0, 99, 99));
    assert_eq!(patient.health, 99);
}

/// **What the doctor says after HEAL** (`UpdateDoctorText`,
/// `ServiceScene.cs:281-300`): the gain is worked out again *after* the
/// health is set, with the offer still on the table — so whenever the
/// treatment reached the cap, it is 0 and the line is 522, "nothing more".
/// Below the cap, the health after decides: 45, 60, 95.
#[test]
fn v12_the_line_after_heal_at_the_cap_is_522() {
    use burngfx::doctor::{said_after, Cure};
    let v12 = Cure::V12 { factor: 1.0, cap: 95 };
    let st = |heals, from, to| Standing { heals, from, to, wasted: 0 };
    assert_eq!(said_after(&st(36, 80, 95), v12), 522, "80 + 36 capped at 95");
    assert_eq!(said_after(&st(0, 40, 40), v12), 522, "nothing on the table");
    assert_eq!(said_after(&st(10, 30, 40), v12), 516);
    assert_eq!(said_after(&st(10, 50, 60), v12), 513);
    assert_eq!(said_after(&st(10, 80, 90), v12), 510);
    // A healing factor that rounds a small dose to nothing: 0 again.
    assert_eq!(said_after(&st(1, 50, 50), Cure::V12 { factor: 0.75, cap: 95 }), 522);
    // The legacy game keeps its lines: health 95 reached is 510, not 522.
    assert_eq!(said_after(&st(36, 80, 95), Cure::Legacy), 510);
    assert_eq!(said_after(&st(36, 80, 100), Cure::Legacy), 519);
    assert_eq!(said_after(&st(0, 40, 40), Cure::Legacy), 522);
}

/// HEAL writes `Health = CalculateDoctorResult(Health, offer)`
/// (`ServiceScene.cs:219-220`): even with nothing on the table the setter
/// runs, so a danger's fraction is gone — 40.65 becomes 40 (RULE-41).
#[test]
fn v12_heal_drops_the_float_s_fraction() {
    use burngfx::danger::{Exposure, UNIT};
    use burngfx::doctor::{heal_from_table_carrying, Cure};
    let t = types();
    let mut table = ItemBag::new(TABLE);
    let mut patient = hurt(40);
    let mut carry = Exposure { carried: 65 * UNIT / 100, lifted: false };
    let got = heal_from_table_carrying(&mut patient, &mut carry, &mut table, &t, Cure::V12 { factor: 1.0, cap: 95 }).unwrap();
    assert_eq!((got.from, got.to, patient.health), (40, 40, 40));
    assert_eq!(carry, Exposure::default());
}
