//! Blows, and what is held while they are struck.

use burngfx::combat::{
    attack, blow, difficulty_factor, in_reach, Contested, Fighter, Kit, Rules, Weapons,
    CREATURE_REACH, DEFAULT_ATTACK, GROUP_REACH, REACH,
};
use burngfx::game::{ItemBag, Point};

// -- a table small enough to read ------------------------------------------

const KNIFE: usize = 0;
const AXE: usize = 1;
/// Two rounds in one item, as the original table has it.
const RIFLE: usize = 2;
const UNLOADED: usize = 3;
/// One round, then another item holding one more — the extended table's way.
const PISTOL: usize = 4;
const PISTOL_1: usize = 5;
const JACKET: usize = 6;
const HELMET: usize = 7;
const MEAT: usize = 8;

struct Table;

impl Weapons for Table {
    fn damage(&self, kind: usize) -> i32 {
        match kind {
            KNIFE => 25,
            AXE => 33,
            RIFLE | PISTOL | PISTOL_1 => 55,
            _ => 0,
        }
    }

    fn defense(&self, kind: usize) -> i32 {
        match kind {
            JACKET => 1,
            HELMET => 3,
            _ => 0,
        }
    }

    fn ammo(&self, kind: usize) -> i32 {
        match kind {
            RIFLE => 2,
            PISTOL | PISTOL_1 => 1,
            _ => 0,
        }
    }

    fn empty(&self, kind: usize) -> Option<usize> {
        match kind {
            RIFLE => Some(UNLOADED),
            PISTOL => Some(PISTOL_1),
            PISTOL_1 => Some(UNLOADED),
            _ => None,
        }
    }
}

/// Somebody with a pack and nothing chosen out of it yet.
struct Body {
    health: i32,
    experience: i32,
    base_attack: i32,
    pack: ItemBag,
    kit: Kit,
}

impl Body {
    /// A person out of the classic data: ten bare-handed, and at difficulty
    /// 0 the thirty-seven experience `start_experience` hands out.
    fn person(experience: i32, carrying: &[usize]) -> Body {
        Body {
            health: 100,
            experience,
            base_attack: DEFAULT_ATTACK,
            pack: ItemBag::with(6, carrying),
            kit: Kit::default(),
        }
    }

    /// `dog_health = 31`, `dog_attack = 30`, and empty handed.
    fn dog(experience: i32) -> Body {
        Body {
            health: 31,
            experience,
            base_attack: 30,
            pack: ItemBag::new(6),
            kit: Kit::default(),
        }
    }

    fn fighter(&mut self) -> Fighter<'_> {
        Fighter {
            health: &mut self.health,
            experience: self.experience,
            base_attack: self.base_attack,
            pack: &mut self.pack,
            kit: &mut self.kit,
        }
    }
}

/// v0.9.1's answers to everything, at the difficulty the run uses.
fn as_v091(difficulty: i32) -> Rules {
    Rules { difficulty, contested: Contested::default() }
}

/// One exchange between one attacker and one defender.
fn duel(a: &mut Body, b: &mut Body, player_attacks: bool, difficulty: i32) -> burngfx::combat::Round {
    let mut group = [a.fighter()];
    let rounds =
        attack(&mut group, &mut b.fighter(), true, player_attacks, as_v091(difficulty), &Table);
    rounds[0]
}

// -- the two values --------------------------------------------------------

/// Both scale with experience, and both truncate.
#[test]
fn experience_blunts_everything() {
    let mut bare = Body::person(37, &[]);
    assert_eq!(bare.fighter().attack_value(&Table), 3, "10 * 37 / 100 is 3, not 3.7");
    assert_eq!(bare.fighter().defense_value(&Table), 3);

    let mut armed = Body::person(37, &[KNIFE]);
    // Nothing is chosen until somebody swings.
    assert_eq!(armed.fighter().attack_value(&Table), 3);
    assert_eq!(armed.fighter().use_best_equipment(true, Contested::default(), &Table), 9, "a knife at 25 * 37 / 100");

    let mut expert = Body::person(100, &[KNIFE]);
    assert_eq!(expert.fighter().use_best_equipment(true, Contested::default(), &Table), 25, "and the whole of it at a hundred");
}

/// **The ten in the defence is not a weapon's.** It is
/// `DEFAULT_ATTACK_VALUE` reused as the floor a defence starts from, so
/// somebody with no armour still has one.
#[test]
fn everybody_has_some_defence() {
    let mut bare = Body::person(100, &[]);
    assert_eq!(bare.fighter().defense_value(&Table), 10);

    let mut dressed = Body::person(100, &[JACKET]);
    assert_eq!(
        dressed.fighter().defense_value(&Table),
        10,
        "a jacket in the pack is not a jacket worn"
    );
    dressed.fighter().use_best_equipment(true, Contested::default(), &Table);
    assert_eq!(dressed.fighter().defense_value(&Table), 11, "now it is on");
}

// -- a blow ----------------------------------------------------------------

/// **Every blow lands.** Armour reduces; it never stops.
#[test]
fn a_blow_is_never_less_than_one() {
    assert_eq!(blow(3, 3, 1.0), 1, "equal is still a hit");
    assert_eq!(blow(0, 99, 1.0), 1, "and so is hopeless");
    assert_eq!(blow(25, 10, 1.0), 15);
}

/// Ten per cent a step, on whichever side is not the player. The comment in
/// the source says twenty-five and the code says ten; the code runs.
#[test]
fn difficulty_is_ten_per_cent_a_step() {
    assert_eq!(difficulty_factor(0), 1.0);
    assert!((difficulty_factor(1) - 1.1).abs() < 1e-6);
    assert!((difficulty_factor(2) - 1.2).abs() < 1e-6);
}

// -- an exchange -----------------------------------------------------------

/// **A blow is answered in the same breath.** Attacking costs health even
/// when it works.
#[test]
fn striking_costs_you_too() {
    let (mut me, mut beast) = (Body::person(100, &[KNIFE]), Body::dog(100));

    let round = duel(&mut me, &mut beast, true, 0);

    assert_eq!(round.dealt, 15, "25 against a dog's 10");
    assert_eq!(round.taken, 20, "and 30 back against my 10");
    assert_eq!(beast.health, 31 - 15);
    assert_eq!(me.health, 100 - 20);
}

/// **The whole group joins in**, one after another, and each is answered
/// before the next steps up.
#[test]
fn the_group_takes_turns() {
    let mut band = [Body::person(100, &[]), Body::person(100, &[]), Body::person(100, &[])];
    let mut beast = Body { health: 200, ..Body::dog(100) };

    let mut group: Vec<_> = band.iter_mut().map(Body::fighter).collect();
    let rounds = attack(&mut group, &mut beast.fighter(), true, true, as_v091(0), &Table);
    drop(group);

    assert_eq!(rounds.len(), 3, "all three had a go");
    assert_eq!(beast.health, 200 - 3 * blow(10, 10, 1.0));
    for one in &band {
        assert_eq!(one.health, 100 - 20, "and each took one back");
    }
}

/// **It stops the moment either side falls.** A group does not keep hitting
/// a corpse, and a man who falls mid-fight takes nobody's turn after him.
#[test]
fn an_exchange_ends_with_the_first_to_fall() {
    let mut band =
        [Body::person(100, &[RIFLE]), Body::person(100, &[]), Body::person(100, &[])];
    let mut beast = Body::dog(100);

    let mut group: Vec<_> = band.iter_mut().map(Body::fighter).collect();
    let rounds = attack(&mut group, &mut beast.fighter(), false, true, as_v091(0), &Table);
    drop(group);

    assert_eq!(rounds.len(), 1, "the rifle finished it");
    assert!(rounds[0].defender_fell);
    assert!(beast.health <= 0);
    assert_eq!(band[1].health, 100, "the second never stepped in");
    assert_eq!(band[2].health, 100);
}

/// And **the dying still answer**: the break is tested after both halves,
/// so a killed dog takes its bite.
#[test]
fn the_dying_still_answer() {
    let (mut me, mut beast) = (Body::person(100, &[RIFLE]), Body::dog(100));

    let round = duel(&mut me, &mut beast, true, 0);

    assert!(round.defender_fell);
    assert_eq!(round.taken, 20, "and it bit on the way down");
    assert_eq!(me.health, 80);
}

/// The other way round: the attacker can be the one who falls.
#[test]
fn the_attacker_can_fall_instead() {
    let mut me = Body { health: 5, ..Body::person(100, &[]) };
    let mut beast = Body { health: 500, ..Body::dog(100) };

    let round = duel(&mut me, &mut beast, true, 0);

    assert!(round.attacker_fell);
    assert!(!round.defender_fell);
    assert!(me.health <= 0);
}

/// **Difficulty weights the other side.** The same fight hurts more when
/// the player is not the attacker.
#[test]
fn difficulty_falls_on_whoever_is_not_the_player() {
    let hit = |player_attacks: bool, difficulty: i32| {
        let (mut me, mut beast) =
            (Body::person(100, &[KNIFE]), Body { health: 500, ..Body::dog(100) });
        let round = duel(&mut me, &mut beast, player_attacks, difficulty);
        (round.dealt, round.taken)
    };

    assert_eq!(hit(true, 0), hit(false, 0), "at difficulty 0 there is no weighting");

    let (dealt, taken) = hit(true, 2);
    assert_eq!(dealt, 15, "the player's own blow is unweighted");
    assert_eq!(taken, 24, "and the answer is 20 * 1.2");

    let (dealt, taken) = hit(false, 2);
    assert_eq!(dealt, 18, "attacking as the other side, 15 * 1.2");
    assert_eq!(taken, 20);
}

// -- what is held ---------------------------------------------------------

/// The best weapon, once — and **the choice sticks**. `FindBestWeapon` only
/// replaces it with something strictly better that needs no ammunition, so a
/// man who has drawn a knife does not reach for the rifle next time.
#[test]
fn a_drawn_weapon_stays_drawn() {
    let mut me = Body::person(100, &[KNIFE, RIFLE]);

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 25, "the knife, not the rifle");
    assert_eq!(me.kit.weapon, Some(0));
    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 25, "and again");
    assert_eq!(me.pack.rounds_left(1), None, "the rifle is untouched");
}

/// With nothing chosen, though, **the first weapon in the pack is taken
/// whatever it costs** — including a loaded gun.
#[test]
fn an_empty_hand_takes_the_first_weapon_it_finds() {
    let mut me = Body::person(100, &[RIFLE, KNIFE]);

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 55);
    assert_eq!(me.pack.rounds_left(0), Some(1), "a round gone");
}

/// The search is not "the first weapon" but "the best one that needs no
/// ammunition", and it gets there in a single pass: the knife becomes the
/// candidate because nothing was chosen, and the axe replaces it at once.
///
/// **Then the axe is put away again** — and this is the original's doing,
/// not ours. `Use()` decrements the count on everything it is called with,
/// so the swung axe sits at minus one round; `ConsumesAmmo` is `ammo != 0`,
/// which minus one satisfies; and the choice made at the end of the swing
/// therefore refuses the very weapon that was just used. From the second
/// blow on, this man fights with his knife.
#[test]
fn swinging_an_axe_puts_it_down_again() {
    let mut me = Body::person(100, &[KNIFE, AXE]);

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 33, "the axe, first swing");
    assert_eq!(me.kit.weapon, Some(0), "and the knife after it");
    assert_eq!(me.pack.rounds_left(1), Some(-1), "the count that did it");

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 25, "so the knife from here on");
}

/// **A loaded gun is the exception**: it can be taken when nothing is
/// chosen, but it can never replace a weapon already in hand, however much
/// harder it hits.
#[test]
fn a_gun_never_replaces_what_is_already_drawn() {
    let mut me = Body::person(100, &[KNIFE, RIFLE]);

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 25);
    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 25, "and still the knife");
    assert_eq!(me.pack.get(1), Some(RIFLE));
    assert_eq!(me.pack.rounds_left(1), None, "not a round spent");
}

/// **Two shots, whichever table.** The original's rifle holds both rounds in
/// one item; the extended one holds a second item behind the first. The
/// count has to live with the item for the first of those to work at all.
#[test]
fn a_rifle_holds_two_shots_and_then_is_a_stick() {
    let mut me = Body::person(100, &[RIFLE]);

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 55, "first shot");
    assert_eq!(me.pack.get(0), Some(RIFLE), "still a loaded rifle, with one left");
    assert_eq!(me.pack.rounds_left(0), Some(1));

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 55, "second shot");
    assert_eq!(me.pack.get(0), Some(UNLOADED), "and now it is unloaded");

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 10, "bare hands after that");
    assert_eq!(me.kit.weapon, None);
}

/// The two-stage kind spends the same two rounds by changing type instead.
#[test]
fn the_two_stage_gun_spends_the_same_two() {
    let mut me = Body::person(100, &[PISTOL]);

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 55);
    assert_eq!(me.pack.get(0), Some(PISTOL_1), "the next stage holds the second round");

    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 55);
    assert_eq!(me.pack.get(0), Some(UNLOADED));
}

/// **A creature's bite spends nothing**: `defendWithAmmo: false` is what
/// `CreatureMind` passes, and it is why a dog cannot drain a rifle.
#[test]
fn defending_against_a_dog_costs_no_ammunition() {
    let mut beast = Body::dog(100);
    let mut me = Body::person(100, &[RIFLE]);

    // The dog attacks; the player is the defender and keeps their rounds.
    let mut group = [beast.fighter()];
    attack(&mut group, &mut me.fighter(), false, false, as_v091(0), &Table);

    assert_eq!(me.pack.get(0), Some(RIFLE));
    assert_eq!(me.pack.rounds_left(0), None, "no round spent defending");
    assert_eq!(me.kit.weapon, Some(0), "though the rifle is now in hand");
}

/// **The first piece of armour is worn for good**, which is `FindBestDefense`
/// being `current ??= item` whatever its name promises. Whether that is a
/// rule or a slip is [`Contested::best_armour_wins`]; this pins what v0.9.1
/// does, which is what we default to.
#[test]
fn the_first_armour_found_is_the_one_worn() {
    let mut me = Body::person(100, &[JACKET, HELMET]);

    me.fighter().use_best_equipment(true, Contested::default(), &Table);
    assert_eq!(me.kit.protection, Some(0));
    assert_eq!(me.fighter().defense_value(&Table), 11);

    me.fighter().use_best_equipment(true, Contested::default(), &Table);
    assert_eq!(me.kit.protection, Some(0), "the helmet never replaces it");
}

/// Nothing in the pack is a weapon, so the bare hand stands.
#[test]
fn a_pack_of_food_is_not_a_weapon() {
    let mut me = Body::person(100, &[MEAT, MEAT]);
    assert_eq!(me.fighter().use_best_equipment(true, Contested::default(), &Table), 10);
    assert_eq!(me.kit.weapon, None);
}

// -- the two contested rules ----------------------------------------------

/// **The axe that puts itself down is a switch, not a law.**
///
/// v0.9.1 spends a round on everything swung, so a melee weapon ends at
/// minus one and `ConsumesAmmo` then refuses it. Turn that off and the axe
/// stays in hand, which is what the name `FindBestWeapon` implies and what
/// a player would expect. Neither reading is verified against the DOS game;
/// the default is the reference's.
#[test]
fn whether_a_swung_axe_is_put_down_is_a_decision() {
    let keeps_the_count = |contested: Contested| {
        let mut me = Body::person(100, &[KNIFE, AXE]);
        let first = me.fighter().use_best_equipment(true, contested, &Table);
        let second = me.fighter().use_best_equipment(true, contested, &Table);
        (first, second, me.pack.rounds_left(1))
    };

    assert_eq!(
        keeps_the_count(Contested::default()),
        (33, 25, Some(-1)),
        "v0.9.1: the axe once, then the knife for ever, and a count of minus one"
    );
    assert_eq!(
        keeps_the_count(Contested { melee_spends_a_round: false, ..Contested::default() }),
        (33, 33, None),
        "the other reading: an axe is not ammunition and keeps being the axe"
    );
}

/// A gun still spends its rounds either way — that part is not in question.
#[test]
fn a_gun_spends_rounds_under_both_readings() {
    for contested in
        [Contested::default(), Contested { melee_spends_a_round: false, ..Contested::default() }]
    {
        let mut me = Body::person(100, &[RIFLE]);
        assert_eq!(me.fighter().use_best_equipment(true, contested, &Table), 55);
        assert_eq!(me.pack.rounds_left(0), Some(1), "one of the two gone");
        me.fighter().use_best_equipment(true, contested, &Table);
        assert_eq!(me.pack.get(0), Some(UNLOADED), "and the second");
    }
}

/// **Which armour goes on is the other one.** `FindBestDefense` takes the
/// first and keeps it; the name says otherwise, and so does having four
/// protections in the extended table.
#[test]
fn whether_the_best_armour_is_worn_is_a_decision() {
    let wears = |contested: Contested| {
        let mut me = Body::person(100, &[JACKET, HELMET]);
        me.fighter().use_best_equipment(true, contested, &Table);
        (me.kit.protection, me.fighter().defense_value(&Table))
    };

    assert_eq!(wears(Contested::default()), (Some(0), 11), "v0.9.1: the jacket, for good");
    assert_eq!(
        wears(Contested { best_armour_wins: true, ..Contested::default() }),
        (Some(1), 13),
        "the other reading: the helmet, because it is better"
    );
}

/// And the switch reaches a real fight, not only the picking.
#[test]
fn the_switches_reach_the_blows() {
    let hit = |contested: Contested| {
        let mut me = Body::person(100, &[JACKET, HELMET]);
        let mut beast = Body { health: 500, ..Body::dog(100) };
        let rules = Rules { difficulty: 0, contested };
        let mut group = [me.fighter()];
        let r = attack(&mut group, &mut beast.fighter(), true, true, rules, &Table);
        r[0].taken
    };

    assert_eq!(hit(Contested::default()), 30 - 11, "30 against a jacket");
    assert_eq!(
        hit(Contested { best_armour_wins: true, ..Contested::default() }),
        30 - 13,
        "and against the helmet, two less"
    );
}

// -- reach ----------------------------------------------------------------

/// Three distances, and they are three different rules.
#[test]
fn the_three_reaches() {
    let here = Point::new(0, 0);
    assert!(in_reach(here, Point::new(29, 0), REACH), "a person strikes inside 30");
    assert!(!in_reach(here, Point::new(30, 0), REACH));
    assert!(in_reach(here, Point::new(19, 0), CREATURE_REACH), "a dog lunges inside 20");
    assert!(!in_reach(here, Point::new(20, 0), CREATURE_REACH));
    assert!(in_reach(here, Point::new(24, 0), GROUP_REACH), "and joins in inside 25");
    assert!(!in_reach(here, Point::new(25, 0), GROUP_REACH));
}

/// It is a circle, not a square.
#[test]
fn reach_is_a_distance_not_a_box() {
    let here = Point::new(0, 0);
    assert!(!in_reach(here, Point::new(25, 25), REACH), "35 away on the diagonal");
    assert!(in_reach(here, Point::new(20, 20), REACH), "28 away");
}

// -- against the shipped data ---------------------------------------------

/// A knife is 25 and a rifle 55 in both modes, and both give two shots —
/// **by different means**. Only Extended has anything to wear. Both are
/// differences in the item table, which is what a mode is.
#[test]
fn the_two_tables_differ_in_armour_and_in_how_a_rifle_empties() {
    let read = |f: &str| std::fs::read_to_string(format!("testdata/{f}")).ok();
    let (Some(original), Some(extended)) = (read("items_original.txt"), read("items.txt"))
    else {
        return;
    };
    let original = burngfx::items::ItemTypes::parse(&original);
    let extended = burngfx::items::ItemTypes::parse(&extended);

    for table in [&original, &extended] {
        let knife = table.index_of("item_knife").expect("a knife");
        assert_eq!(table.damage_of(knife), 25);
        let rifle = table.index_of("item_loaded_rifle").expect("a rifle");
        assert_eq!(table.damage_of(rifle), 55);
        assert_eq!(shots(table, rifle), 2, "two shots either way");
    }

    let rifle = original.index_of("item_loaded_rifle").unwrap();
    assert_eq!(original.ammo_of(rifle), 2, "the original holds both in one item");
    let rifle = extended.index_of("item_loaded_rifle").unwrap();
    assert_eq!(extended.ammo_of(rifle), 1, "the extended holds one, and one behind it");

    let armour = |t: &burngfx::items::ItemTypes| (0..t.len()).filter(|&i| t.defense_of(i) > 0).count();
    assert_eq!(armour(&original), 0, "nothing to wear in the original game");
    assert!(armour(&extended) >= 4, "and something in the extended one");
}

/// How many rounds a weapon gives before it is a stick, following the chain
/// of empty forms.
fn shots(table: &burngfx::items::ItemTypes, mut kind: usize) -> i32 {
    let mut total = 0;
    for _ in 0..8 {
        total += table.ammo_of(kind);
        match table.empty_of(kind) {
            Some(next) if table.damage_of(next) > 0 => kind = next,
            _ => break,
        }
    }
    total
}

#[test]
fn a_chosen_weapon_follows_its_item_and_is_forgotten_when_it_leaves() {
    use burngfx::combat::Kit;
    let mut k = Kit { weapon: Some(2), protection: Some(3) };
    // Slot 0 went; the knife (7) and the jacket (9) shift down.
    k.follow(&[1, 4, 7, 9], &[4, 7, 9]);
    assert_eq!((k.weapon, k.protection), (Some(1), Some(2)));
    // The knife itself went.
    k.follow(&[4, 7, 9], &[4, 9, 5]);
    assert_eq!((k.weapon, k.protection), (None, Some(1)));
    // Nothing changed: nothing moves.
    k.follow(&[4, 9, 5], &[4, 9, 5]);
    assert_eq!(k.protection, Some(1));
}

#[test]
fn select_item_toggles_as_character_select_item_does() {
    use burngfx::combat::Kit;
    let mut k = Kit::default();
    k.select(1, true, false);
    assert_eq!(k.weapon, Some(1));
    k.select(2, true, false);
    assert_eq!(k.weapon, Some(2), "another weapon replaces it");
    k.select(2, true, false);
    assert_eq!(k.weapon, None, "the same one again deselects");
    k.select(0, false, true);
    assert_eq!(k.protection, Some(0));
}

#[test]
fn a_bottle_filled_in_place_does_not_lose_the_chosen_knife() {
    use burngfx::combat::Kit;
    let mut k = Kit { weapon: Some(2), protection: None };
    k.follow(&[1, 4, 7, 9], &[1, 5, 7, 9]);
    assert_eq!(k.weapon, Some(2));
}
