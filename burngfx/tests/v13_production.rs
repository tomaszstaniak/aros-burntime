//! v1.3's production changes: the tie on
//! the automatic choice (`36ecfc3`) and the refresh when a camp is made
//! (`758f323`), on Extended's tables (`production.txt` is the same at
//! `v1.2` and `v1.3`).

use burngfx::game::{Camp, Companion, Pump, Storage};
use burngfx::items::ItemTypes;
use burngfx::production::Productions;
use burngfx::rules::{FoodProduction, Survival, WaterOutput};
use burngfx::v12_day::{auto_select_food_production, refresh_production, V12World};

fn read(f: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

struct Rooms {
    tools: Vec<(usize, usize)>,
    valid: Vec<usize>,
}

impl Storage for Rooms {
    fn count_in_rooms(&self, _: usize, _: usize) -> usize {
        0
    }
    fn tools(&self, _: usize, production: usize) -> usize {
        self.tools.iter().find(|(p, _)| *p == production).map_or(0, |(_, n)| *n)
    }
    fn any_room_has_space(&self, _: usize) -> bool {
        true
    }
    fn store(&mut self, _: usize, _: usize) -> bool {
        true
    }
    fn pump_at(&self, _: usize) -> Pump {
        Pump::None
    }
    fn valid_productions(&self, _: usize) -> Vec<usize> {
        self.valid.clone()
    }
}

fn v12() -> V12World {
    V12World {
        survival: Survival::RemasterSupplyPool,
        stabilization: 1,
        water_output: WaterOutput::AmigaMinimum,
        food_production: FoodProduction::DosFeedThenStore,
        thresholds: [None; 4],
        doctors: vec![],
        dangers: vec![],
        gas_immune: vec![],
        radiation_immune: vec![],
        faces: vec![],
        technicians: vec![],
        technician_bonus: 0,
        cities: vec![],
    }
}

struct T {
    types: ItemTypes,
    productions: Productions,
}

impl T {
    fn new() -> T {
        T {
            types: ItemTypes::parse_v12(&read("v1.2/rules/extended/items.txt")).unwrap(),
            productions: Productions::parse(&read("v1.2/rules/extended/production.txt")),
        }
    }
    fn pid(&self, tool: &str) -> usize {
        self.productions.iter().position(|p| p.tool == tool).unwrap()
    }
}

fn camp(people: usize, production: Option<usize>) -> Camp {
    Camp { owner: Some(1), residents: (0..people).map(Companion::new).collect(), production, ..Camp::default() }
}

/// Upstream's own case ("prefer rat trap before knifes with 2 NPCs"): two
/// people, a knife and a rat trap installed. The knife makes 3 a day for two
/// (`amount2=1 3`), one rat trap 3 too (`amount=0 3 4`): a tie, and the rat
/// trap, later in `production.txt`, wins — from either side.
#[test]
fn an_equal_positive_rate_goes_to_the_later_production() {
    let t = T::new();
    let (knife, rats) = (t.pid("item_knife"), t.pid("item_rat_trap"));
    assert!(knife < rats);
    let rooms = Rooms { tools: vec![(knife, 1), (rats, 1)], valid: vec![knife, rats] };
    for start in [None, Some(knife), Some(rats)] {
        let mut c = camp(2, start);
        let rate = auto_select_food_production(0, &mut c, &rooms, &t.productions, &t.types, &v12(), false);
        assert_eq!((c.production, rate), (Some(rats), 3), "from {start:?}");
    }
    // The same order the other way round in the place's list: the id decides,
    // not the order the candidates are tried in.
    let reversed = Rooms { tools: rooms.tools.clone(), valid: vec![rats, knife] };
    let mut c = camp(2, Some(knife));
    auto_select_food_production(0, &mut c, &reversed, &t.productions, &t.types, &v12(), false);
    assert_eq!(c.production, Some(rats));
}

/// The tie needs a rate above nought, and a better rate still wins outright.
#[test]
fn the_tie_rule_has_its_limits() {
    let t = T::new();
    let (knife, rats, snakes) = (t.pid("item_knife"), t.pid("item_rat_trap"), t.pid("item_snake_trap"));
    // One person: the knife makes 1, the trap 3 — no tie, the trap wins as before.
    let rooms = Rooms { tools: vec![(knife, 1), (rats, 1)], valid: vec![knife, rats] };
    let mut c = camp(1, Some(knife));
    assert_eq!(auto_select_food_production(0, &mut c, &rooms, &t.productions, &t.types, &v12(), false), 3);
    assert_eq!(c.production, Some(rats));
    // No trap installed where only traps can be made: every rate is
    // nought, so no tie — the current choice stays, and none is made where
    // there was none. (A knife makes 1 with no tool at all, `amount=1 2`.)
    let empty = Rooms { tools: vec![], valid: vec![rats, snakes] };
    for start in [Some(rats), None] {
        let mut c = camp(2, start);
        assert_eq!(auto_select_food_production(0, &mut c, &empty, &t.productions, &t.types, &v12(), false), 0);
        assert_eq!(c.production, start);
    }
    // A rat trap and a snake trap: 3 and 4, the snake trap outright.
    let both = Rooms { tools: vec![(rats, 1), (snakes, 1)], valid: vec![rats, snakes] };
    let mut c = camp(2, Some(rats));
    assert_eq!(auto_select_food_production(0, &mut c, &both, &t.productions, &t.types, &v12(), false), 4);
    assert_eq!(c.production, Some(snakes));
    // `only_if_nothing` with a productive choice: untouched, even on a tie.
    let mut c = camp(2, Some(knife));
    auto_select_food_production(0, &mut c, &rooms, &t.productions, &t.types, &v12(), true);
    assert_eq!(c.production, Some(knife));
}

/// `RefreshFoodProductionSelection`, which v1.3 runs in `JoinCamp`: a
/// player's productive choice stands; an unproductive one, or none, goes to
/// the automatic choice and the mark is cleared.
#[test]
fn the_refresh_keeps_a_productive_choice_and_replaces_the_rest() {
    let t = T::new();
    let (knife, rats) = (t.pid("item_knife"), t.pid("item_rat_trap"));
    let rooms = Rooms { tools: vec![(knife, 1), (rats, 1)], valid: vec![knife, rats] };
    let mut c = Camp { manual_production: true, ..camp(2, Some(knife)) };
    assert_eq!(refresh_production(0, &mut c, &rooms, &t.productions, &t.types, &v12()), 3);
    assert_eq!((c.production, c.manual_production), (Some(knife), true), "the player's tie stands");
    let snakes = t.pid("item_snake_trap");
    let only_rats = Rooms { tools: vec![(rats, 1)], valid: vec![knife, rats, snakes] };
    // The player chose snakes and has no snake trap: nought, so replaced.
    let mut c = Camp { manual_production: true, ..camp(1, Some(snakes)) };
    assert_eq!(refresh_production(0, &mut c, &only_rats, &t.productions, &t.types, &v12()), 3);
    assert_eq!((c.production, c.manual_production), (Some(rats), false));
    // A new camp with a trap left in its rooms by the last owner and no
    // production chosen: the camp starts on the trap.
    let mut c = camp(1, None);
    assert_eq!(refresh_production(0, &mut c, &only_rats, &t.productions, &t.types, &v12()), 3);
    assert_eq!(c.production, Some(rats));
}
