//! v1.2's Amiga opponent (`AmigaAiState.cs`) against a small world.

use burngfx::amiga_ai::*;
use burngfx::gamdat::CharClass;
use burngfx::spawn::Rng;

#[derive(Default)]
struct Place {
    city: bool,
    owner: Option<usize>,
    neighbors: Vec<usize>,
    production: Option<usize>,
    tools: usize,
    water: i32,
    danger: Option<&'static str>,
    stationed: usize,
    rooms_full: bool,
    items: Vec<&'static str>,
}

struct World {
    day: i32,
    me: usize,
    here: usize,
    previous: Option<usize>,
    travelling: bool,
    places: Vec<Place>,
    /// Party: (health, xp, mercenary, weapon); index 0 is the leader.
    party: Vec<Duelist>,
    /// Defenders here.
    enemies: Vec<Duelist>,
    hireable: bool,
    benchmark: Benchmark,
    travelled: Option<usize>,
    log: Vec<String>,
}

impl World {
    fn new() -> World {
        World {
            day: 5,
            me: 2,
            here: 0,
            previous: None,
            travelling: false,
            places: (0..4).map(|_| Place::default()).collect(),
            party: vec![duelist(100, 37, false, Some("item_knife"))],
            enemies: Vec::new(),
            hireable: false,
            benchmark: Benchmark { restricts: true, strongest: 1 },
            travelled: None,
            log: Vec::new(),
        }
    }
}

fn duelist(health: i32, experience: i32, mercenary: bool, weapon: Option<&str>) -> Duelist {
    Duelist { health, experience, mercenary, weapon: weapon.map(String::from), with_boss: true }
}

impl AmigaWorld for World {
    fn day(&self) -> i32 { self.day }
    fn me(&self) -> usize { self.me }
    fn here(&self) -> usize { self.here }
    fn travelling(&self) -> bool { self.travelling }
    fn dead(&self) -> bool { self.party[0].health <= 0 }
    fn is_city(&self, l: usize) -> bool { self.places[l].city }
    fn owner(&self, l: usize) -> Option<usize> { self.places[l].owner }
    fn neighbors(&self, l: usize) -> Vec<usize> { self.places[l].neighbors.clone() }
    fn can_travel(&self, from: usize, to: usize) -> bool {
        from == self.here
            && self.places[from].neighbors.contains(&to)
            && match self.places[from].owner {
                Some(o) if o != self.me => self.previous == Some(to),
                _ => true,
            }
    }
    fn previous(&self) -> Option<usize> { self.previous }
    fn production(&self, l: usize) -> Option<usize> { self.places[l].production }
    fn production_valid(&self, _: usize, p: usize) -> bool { p <= 3 }
    fn production_tools(&self, l: usize) -> usize { self.places[l].tools }
    fn water_output(&self, l: usize) -> i32 { self.places[l].water }
    fn danger(&self, l: usize) -> Option<&'static str> { self.places[l].danger }
    fn stationed(&self, l: usize) -> usize { self.places[l].stationed }
    fn owned_camps(&self) -> usize { self.places.iter().filter(|p| p.owner == Some(self.me)).count() }
    fn benchmark(&self) -> Benchmark { self.benchmark }
    fn group_size(&self) -> usize { self.party.len() }
    fn leader_food(&self) -> i32 { 9 }
    fn fill_water(&mut self) {}
    fn recalculate_experience(&mut self) {}
    fn recover_group(&mut self) { self.log.push("recovered".into()) }
    fn recruit_one(&mut self) -> bool {
        if !self.hireable { return false }
        self.hireable = false;
        self.party.push(duelist(100, 45, true, Some("item_knife")));
        true
    }
    fn defenders(&self) -> Vec<usize> { (0..self.enemies.len()).map(|i| 100 + i).collect() }
    fn attackers(&self) -> Vec<usize> { (0..self.party.len()).collect() }
    fn leader(&self) -> usize { 0 }
    fn duelist(&self, id: usize) -> Option<Duelist> {
        if id >= 100 { self.enemies.get(id - 100).cloned() } else { self.party.get(id).cloned() }
    }
    fn select_weapon(&mut self, _: usize) {}
    fn hurt(&mut self, id: usize, amount: i32) {
        if id >= 100 { self.enemies[id - 100].health -= amount } else { self.party[id].health -= amount }
    }
    fn free_here(&mut self) { let h = self.here; self.places[h].owner = None }
    fn station_first(&mut self, protection: Option<&str>) -> bool {
        if self.party.len() < 2 { return false }
        self.party.remove(1);
        let (h, me) = (self.here, self.me);
        self.places[h].owner = Some(me);
        self.places[h].stationed += 1;
        if let Some(p) = protection { self.log.push(format!("protection {p}")) }
        true
    }
    fn create_camp_item(&mut self, item: &str, _water_room: bool) -> bool {
        let h = self.here;
        if self.places[h].rooms_full { return false }
        self.places[h].items.push(match item {
            "item_knife" => "item_knife",
            "item_rat_trap" => "item_rat_trap",
            "item_hand_pump" => "item_hand_pump",
            _ => "other",
        });
        true
    }
    fn set_production(&mut self, p: usize) { let h = self.here; self.places[h].production = Some(p) }
    fn travel(&mut self, to: usize) { self.travelled = Some(to); self.travelling = true }
    fn report(&mut self, what: String) { self.log.push(what) }
}

#[test]
fn the_tables() {
    assert!(can_recruit(CharClass::Mercenary, 41, 37));
    assert!(!can_recruit(CharClass::Mercenary, 42, 37));
    assert!(!can_recruit(CharClass::Dog, 0, 99));
    assert_eq!(production_capacity(Some(0), 0), 1);
    assert_eq!(production_capacity(Some(0), 1), 3);
    assert_eq!(production_capacity(Some(3), 2), 7);
    assert_eq!(production_capacity(None, 5), 0);
    assert_eq!(arrival_recovery(true, false), (9, 30));
    assert_eq!(arrival_recovery(true, true), (3, 20));
    assert_eq!(arrival_recovery(false, true), (9, 0), "0x7ca2 skips the hostile check outside cities");
    let ai = AmigaAi::new(1, 1);
    assert_eq!(ai.maximum_group(), 4);
    assert_eq!(AmigaAi::new(0, 1).maximum_group(), 3);
}

/// Basis plus experience, a group member's minimum by day, and the random
/// part: 0..3, or 0..7 with a pitchfork or from day 200.
#[test]
fn strategic_damage_is_the_recovered_formula() {
    let mut rng = Rng::seeded(7);
    for _ in 0..50 {
        let knife = strategic_damage(&duelist(100, 37, false, Some("item_knife")), 5, &mut rng);
        // 9 + 9*18/100 = 10, plus 0..3.
        assert!((10..=13).contains(&knife), "{knife}");
        let merc = strategic_damage(&duelist(100, 50, true, Some("item_axe")), 5, &mut rng);
        // 12 + 6 = 18, plus 0..3.
        assert!((18..=21).contains(&merc), "{merc}");
        let late = strategic_damage(&duelist(100, 0, true, None), 250, &mut rng);
        // Bare hands 5, raised to the day's 18, plus 0..7.
        assert!((18..=25).contains(&late), "{late}");
        let camper = Duelist { with_boss: false, ..duelist(100, 0, true, None) };
        assert!((5..=8).contains(&strategic_damage(&camper, 250, &mut rng)));
    }
}

/// A free place that makes nothing: the man hired here stays, a knife is
/// made in a room and the place set to make food with it.
#[test]
fn a_free_place_is_claimed_with_the_man_just_hired() {
    let mut w = World::new();
    w.hireable = true;
    let mut ai = AmigaAi::new(1, 5);
    let mut rng = Rng::seeded(1);
    ai.turn(&mut w, &mut rng);
    assert_eq!(w.places[0].owner, Some(2), "{:?}", w.log);
    assert_eq!(w.places[0].items, vec!["item_knife"]);
    assert_eq!(w.places[0].production, Some(0));
    assert!(!ai.maintenance_pending);
    assert_eq!(w.travelled, None, "a day's stop after arriving");
}

/// From an enemy camp there is only the road back.
#[test]
fn from_an_enemy_camp_only_back_the_way_it_came() {
    let mut w = World::new();
    w.places[0].owner = Some(0);
    w.places[0].neighbors = vec![1, 2, 3];
    w.previous = Some(3);
    w.enemies = vec![duelist(100, 90, true, Some("item_loaded_rifle"))];
    let mut ai = AmigaAi::new(0, 5);
    ai.maintenance_pending = false;
    let mut rng = Rng::seeded(3);
    ai.turn(&mut w, &mut rng);
    assert_eq!(w.travelled, Some(3));
}

/// After a stationary day, the one random slot, if it may be travelled.
#[test]
fn after_three_stationary_days_any_road() {
    let mut w = World::new();
    w.places[0].neighbors = vec![1];
    w.places[0].city = true;
    w.places[1].owner = Some(0);
    w.day = 10;
    let mut ai = AmigaAi::new(1, 7);
    ai.maintenance_pending = false;
    let mut rng = Rng::seeded(5);
    ai.turn(&mut w, &mut rng);
    assert_eq!(w.travelled, Some(1), "three days still: any road");
}

/// The duel frees a place whose defenders all fall.
#[test]
fn the_duel_frees_a_place() {
    let mut w = World::new();
    w.places[0].owner = Some(0);
    w.party = vec![duelist(100, 90, true, Some("item_loaded_rifle")), duelist(100, 90, true, Some("item_loaded_rifle"))];
    w.enemies = vec![duelist(20, 0, false, None)];
    w.benchmark = Benchmark { restricts: false, strongest: 0 };
    let mut ai = AmigaAi::new(2, 5);
    let mut rng = Rng::seeded(9);
    ai.turn(&mut w, &mut rng);
    assert!(w.enemies[0].health <= 0);
    assert!(w.log.iter().any(|l| l.starts_with("defeated the opposition")), "{:?}", w.log);
    // Then claimed with the second man.
    assert_eq!(w.places[0].owner, Some(2));
}

/// Held back by the strongest rival: at or above his camps plus the
/// margin, no local work — maintenance stays pending.
#[test]
fn held_back_by_the_strongest_rival() {
    let mut w = World::new();
    w.hireable = true;
    w.places[1].owner = Some(2);
    w.places[2].owner = Some(2);
    w.benchmark = Benchmark { restricts: true, strongest: 0 };
    let mut ai = AmigaAi::new(0, 5);
    // Margin up to 3: with two camps held, work happens only when the draw is 2 or 3.
    let mut worked = 0;
    for seed in 0..40 {
        let mut w2 = World { hireable: true, ..World::new() };
        w2.places[1].owner = Some(2);
        w2.places[2].owner = Some(2);
        w2.benchmark = w.benchmark;
        let mut a = ai.clone();
        a.turn(&mut w2, &mut Rng::seeded(seed));
        if !a.maintenance_pending {
            worked += 1;
        }
    }
    assert!(worked > 0 && worked < 40, "{worked}");
    ai.turn(&mut w, &mut Rng::seeded(0));
}

/// `CleanupAmiga`: food thrown away without a slot, the production tool
/// left, weapons taken, traps and the first other thing thrown away.
#[test]
fn the_cleanup_on_arrival() {
    // snake(0x35) knife(0x3d = tool of production 0) axe(0x3e) rope(0x3a) rat trap(0x43) rags(0x3b)
    let titles = [0x35, 0x3d, 0x3e, 0x3a, 0x43, 0x3b];
    let mut free = [2, 1];
    let got = sweep(&titles, &mut free, production_tool_title(Some(0)));
    assert_eq!(
        got,
        vec![
            Sweep::Destroy { record: 0 },
            Sweep::Take { record: 2, member: 0 },
            Sweep::Destroy { record: 4 },
        ],
        "the rope and the rags are left: the axe already counted as dealt with"
    );
    assert_eq!(free, [0, 1]);
    assert_eq!(title_id(Some(10)), 0x3d);
    assert_eq!(title_id(None), 0x32);
}

/// `CleanupAmiga`'s two passes each start with nothing thrown away:
/// `discardedOther` is local to `SweepRecords`
/// (`Logic/Rules/OriginalItemRecords.cs:50-54`). The ground's knife is
/// taken, and the rooms' rope is still the first other thing of its pass,
/// so it is thrown away.
#[test]
fn the_rooms_pass_starts_with_nothing_thrown_away() {
    let knife = 0x3d;
    let rope = 0x3a;
    let (ground, rooms) = cleanup(&[knife], Some(&[rope]), &[6], production_tool_title(None));
    assert_eq!(ground, vec![Sweep::Take { record: 0, member: 0 }]);
    assert_eq!(rooms, vec![Sweep::Destroy { record: 0 }], "the rope is destroyed");
    // In a place somebody else holds, only the ground.
    let (_, rooms) = cleanup(&[knife], None, &[6], production_tool_title(None));
    assert!(rooms.is_empty());
}

/// The free slots of the rooms' pass are counted from the packs again: a
/// thing the ground pass took fills one, a thing it threw away does not
/// (`free = Math.Max(0, 6 - character.Items.Count)`, `:57`).
#[test]
fn the_rooms_pass_counts_the_packs_again() {
    // Ground: a rope thrown away, an axe taken; the leader had two free.
    let (ground, rooms) = cleanup(&[0x3a, 0x3e], Some(&[0x3f, 0x40]), &[2, 0], 0);
    assert_eq!(ground, vec![Sweep::Destroy { record: 0 }, Sweep::Take { record: 1, member: 0 }]);
    // One slot left: the first of the rooms' weapons.
    assert_eq!(rooms, vec![Sweep::Take { record: 0, member: 0 }]);
}
