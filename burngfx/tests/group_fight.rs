//! A group fight, with the group spread out: each member is filtered on their
//! own, and whoever stays out is left exactly as they were.

use burngfx::combat::{attack, Contested, Fighter, Rules, Weapons, DEFAULT_ATTACK};
use burngfx::game::{attack_party, Companion, ItemBag, Party, Point, PACK_CAPACITY};

struct Table;
const KNIFE: usize = 0;
impl Weapons for Table {
    fn damage(&self, kind: usize) -> i32 {
        if kind == KNIFE { 25 } else { 0 }
    }
    fn defense(&self, _: usize) -> i32 {
        0
    }
    fn ammo(&self, _: usize) -> i32 {
        0
    }
    fn empty(&self, _: usize) -> Option<usize> {
        None
    }
}

fn companion(id: usize, at: Point) -> Companion {
    Companion {
        experience: 100,
        pack: ItemBag::with(PACK_CAPACITY, &[KNIFE]),
        at,
        move_to: at,
        ..Companion::new(id)
    }
}

/// **Two companions at once: the near one fights, the far one is untouched.**
///
/// Not the boundary on its own — that is `a_companion_joins_the_attack_only_within_twenty_five`
/// — but the filter applied to a whole group in one exchange: both are in the
/// party, one is 20 from the leader and one is 30, and only one of them takes
/// part. The other keeps every point of health, every item, and no weapon drawn.
#[test]
fn a_near_companion_fights_and_a_far_one_is_left_as_they_were() {
    let leader = Point::new(200, 200);
    let mut party = Party::restored(
        vec![companion(1, Point::new(220, 200)), companion(2, Point::new(230, 200))],
        3,
    );
    let far_before = party.companions[1].clone();

    let joining = attack_party(&party, leader);
    assert_eq!(joining, vec![0], "only the one at 20");

    let mut leader_health = 100;
    let mut leader_pack = ItemBag::new(PACK_CAPACITY);
    let mut leader_kit = Default::default();
    let mut dog_health = 500;
    let mut dog_pack = ItemBag::new(PACK_CAPACITY);
    let mut dog_kit = Default::default();

    {
        let mut side = vec![Fighter {
            health: &mut leader_health,
            experience: 100,
            base_attack: DEFAULT_ATTACK,
            pack: &mut leader_pack,
            kit: &mut leader_kit,
        }];
        for (i, c) in party.companions.iter_mut().enumerate() {
            if joining.contains(&i) {
                side.push(Fighter {
                    health: &mut c.needs.health,
                    experience: c.experience,
                    base_attack: DEFAULT_ATTACK,
                    pack: &mut c.pack,
                    kit: &mut c.kit,
                });
            }
        }
        let mut dog = Fighter {
            health: &mut dog_health,
            experience: 100,
            base_attack: 30,
            pack: &mut dog_pack,
            kit: &mut dog_kit,
        };
        let rules = Rules { difficulty: 0, contested: Contested::default() };
        let rounds = attack(&mut side, &mut dog, true, true, rules, &Table);
        assert_eq!(rounds.len(), 2, "the leader and the near companion, nobody else");
    }

    let near = &party.companions[0];
    assert!(near.needs.health < 100, "the near one took the dog's answer");
    assert_eq!(near.kit.weapon, Some(0), "and drew the knife");

    assert_eq!(party.companions[1], far_before, "the far one is exactly as before");
    // The leader carries nothing, so strikes bare-handed: 10 against the
    // dog's 10 is a blow of one. The near companion's knife is 25 against 10.
    assert_eq!(dog_health, 500 - 1 - 15, "struck twice: the leader's fist and the near companion's knife");
}
