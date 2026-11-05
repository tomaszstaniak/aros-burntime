//! A machine player: what it wants, and what it may do to get it.
//!
//! The rules it plays by are [`crate::rivals`] — the same ones the human
//! plays by. **This module is not a port.** v0.9.1's `ClassicAiState` is the
//! shape it follows, mode for mode, but that opponent keeps itself alive by
//! cheating, and upstream has since deleted the cheat:
//!
//! ```text
//! 88da26f  remove AI cheat for full water/food stock in cities.
//!          which requires a lot of rework to make it viable
//! ```
//!
//! What v0.9.1 does and this does not:
//!
//! * `RefreshGroupAttributes` — `Drink(null, 10)`, `Eat(null, 10)`,
//!   `Heal(null, 100)` on the whole group whenever it is home or in a city;
//! * `RefreshGroupReserves` — meat and a full wineskin conjured into the
//!   leader's pack;
//! * `HireNpc` — `Items.Clear()` and then the exact hire item generated out
//!   of nothing, so the price is never a problem.
//!
//! Here the opponent eats what it carries, drinks from the springs the world
//! has, and pays for a hire out of its own pack or does not hire. It may
//! starve. It may fail. What it may not do is need a gift to finish a
//! demonstration.
//!
//! # The shape that is kept
//!
//! `ClassicAiState.Turn`: hire if the group is empty, then a small machine of
//! modes — look for the next camp, hire somebody, wait an interval — and the
//! expansion limit `MaxHumanCampCount + MaxAdvance`. The pace comes from the
//! settings, not from here: [`crate::settings::GameSettings::ai_camp_interval`]
//! and `ai_camp_max_advance`.
//!
//! # Randomness
//!
//! One stream per player per day, built the way a trader's is —
//! [`crate::trader::rng_for`] — so a saved game resumes on the same numbers
//! without carrying a generator's position. The only draw is the wait
//! interval; it is taken in [`Rival::turn`] and written down.

use alloc::vec::Vec;

use crate::game::{CharacterNeeds, ItemBag, Party, Point};
use crate::rivals::can_travel;
use crate::settings::GameSettings;
use crate::spawn::Rng;

/// What the opponent is doing, in `ClassicAiState`'s own four modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// Between camps, counting down.
    #[default]
    Waiting,
    /// Looking for somewhere to put the next camp.
    Expanding,
    /// Out of people: find a city and take somebody on.
    Hiring,
}

/// Everything about a machine player that a save has to carry.
///
/// **All of it.** The mode, the countdown, where it is headed and what it is
/// travelling on are decisions in flight: a game resumed without them would
/// hand the opponent a free re-think, and a resumed run would differ from an
/// uninterrupted one.
#[derive(Clone, Debug, PartialEq)]
pub struct Rival {
    /// Which player this is, in the world's own numbering.
    pub player: usize,
    /// Where it stands.
    pub at: usize,
    /// Where it is going, and how many days are left — the same journey a
    /// human takes, one committed road at a time.
    pub journey: Option<crate::game::Journey>,
    /// Where it decided to head, which outlives a single day's travel.
    pub headed: Option<usize>,
    pub mode: Mode,
    /// Turns left of the interval it drew.
    pub wait: i32,
    /// **What its leader knows.** `offer` weighs it against the person being
    /// asked — `boss.Experience >= Parent.Experience * 0.66` — so an opponent
    /// without it could hire nobody, and one with too much could hire anybody.
    /// It is the `start_experience` of the difficulty being played, the same
    /// number the player is given.
    pub experience: i32,
    /// The leader's own needs. It eats and drinks like anybody else.
    pub needs: CharacterNeeds,
    pub pack: ItemBag,
    /// **Its own group, of the same kind the player has.**
    ///
    /// Not a count: a camper has to be somebody. The people in here were
    /// hired out of the world with [`crate::hire::hire`], they eat through
    /// [`crate::game::party_day`], and one of them left behind becomes a
    /// camp's resident through [`crate::game::Party::leave`] — the same three
    /// functions the player's group goes through, so there is one set of
    /// rules and not two.
    pub party: Party,
    /// Dead for good: `World.Turn` marks a player whose character died.
    pub dead: bool,
    /// **Classic's own computer player**, when this seat is played by it —
    /// v0.9.1's `ClassicAiState`, see [`crate::classic_ai`]. `None` is this
    /// module's opponent of our own design, kept for comparison and reached
    /// only through test switches. The fields above are the player either
    /// way; `mode`, `wait` and `headed` above belong to ours.
    pub classic: Option<crate::classic_ai::ClassicAi>,
    /// `previousLocation`, which `CanTravel` reads for Classic's player.
    pub previous: Option<usize>,
    /// **v1.2's Amiga opponent**, when a v1.2 game seats it here — see
    /// [`crate::amiga_ai`]. Its state is saved on an `amiga` line of its own.
    pub amiga: Option<crate::amiga_ai::AmigaAi>,
    /// **v1.2's Modern opponent**, when a v1.2 game seats it here — see
    /// [`crate::modern_ai`]. Saved on `modern` lines of its own.
    pub modern: Option<crate::modern_ai::state::ModernAi>,
    /// The leader's chosen weapon (`Character.Weapon`), by item kind: v1.2's
    /// duel writes back the weapon it found (`SelectOriginalWeapon`,
    /// `Character.cs:700-704`), an unloaded rifle included, and the next
    /// duel starts from it. A kind rather than a slot because the machine's
    /// pack is rearranged by its turn; the first slot holding that kind
    /// stands for the item.
    pub weapon_kind: Option<usize>,
}

impl Rival {
    pub fn new(player: usize, at: usize) -> Rival {
        Rival {
            player,
            at,
            journey: None,
            headed: None,
            mode: Mode::Waiting,
            wait: 0,
            experience: 0,
            needs: CharacterNeeds::at_game_start(),
            pack: ItemBag::new(crate::game::PACK_CAPACITY),
            party: Party::new(),
            dead: false,
            classic: None,
            previous: None,
            amiga: None,
            modern: None,
            weapon_kind: None,
        }
    }

    /// The pack slot of [`Rival::weapon_kind`], if he still carries one.
    pub fn weapon_slot(&self) -> Option<usize> {
        let kind = self.weapon_kind?;
        (0..self.pack.len()).find(|&s| self.pack.get(s) == Some(kind))
    }
}

/// What the world must answer for the opponent to play. The caller owns the
/// world; this only asks about it.
pub trait Ground {
    /// Roads out of a location, as `(neighbour, days)`.
    fn roads(&self, location: usize) -> Vec<(usize, u8)>;
    fn is_city(&self, location: usize) -> bool;
    /// Who holds it, if anybody.
    fn owner(&self, location: usize) -> Option<usize>;
    /// Is there anybody standing here who would work for this player, and
    /// what would they take? `None` when there is nobody to ask.
    fn hireable(&self, location: usize, player: usize) -> Option<usize>;
    /// What the location's spring will give a thirsty character today.
    fn water_here(&self, location: usize) -> i32;
    /// **Something edible lying here that anybody standing here could pick
    /// up** — on the ground, or in one of the rooms — named by whatever
    /// handle the caller uses for it. `None` when there is nothing.
    ///
    /// The item table is the caller's and so is the question of what counts
    /// as food; this asks the world the same question the player answers by
    /// walking in and looking.
    fn food_here(&self, location: usize) -> Option<usize>;
    /// **A production this player could set going here, that yields
    /// something to eat**, named by the caller's own handle for it — the slot
    /// in the location's list. `None` when the place makes no food, when one
    /// is already set, or when this player may not set it.
    ///
    /// What a production yields is the item table's business, so the answer
    /// is the caller's.
    fn food_product(&self, location: usize) -> Option<usize>;
    /// **Where this player's own stores are**: locations it holds that have
    /// something edible standing in them.
    ///
    /// The opponent is allowed to know this and nothing more about the
    /// world's larder, because it is what the player knows — the info screen
    /// lists what is in the places you hold, and refuses to open anybody
    /// else's.
    fn my_larder(&self) -> Vec<usize>;
    /// Every location this player holds, whether or not anything is standing
    /// in it today. A camp is where food *appears*, so it is somewhere to go
    /// even when its store happens to be empty this morning.
    fn my_camps(&self) -> Vec<usize>;
    /// **A tool in this player's own pack that would make this place
    /// produce**, as a pack slot.
    ///
    /// `production_turn` counts the tools lying in a camp's rooms *and* the
    /// ones its people are carrying — "leaving somebody a knife in their
    /// pack is as good as leaving it on the floor".
    ///
    /// **A camp with no tool is not a camp that makes nothing.** The classic
    /// table's knife reads `amount=1 2`: one a day with no tool at all, two
    /// with one. One a day is exactly one man's keep, so an untooled camp
    /// feeds its resident and stores nothing; the tool is what turns keep
    /// into a surplus somebody can come and fetch. That surplus is what a
    /// travelling player and a travelling opponent live on, so it is the
    /// thing that joins holding ground to going anywhere.
    ///
    /// It holds only under the conditions the source sets: the location has
    /// a production available, one is chosen, the camp has an owner, and the
    /// numbers come from that production's own line — see
    /// `a_tool_is_the_difference_between_subsisting_and_storing`.
    ///
    /// Handing it over costs it the knife, in a game where a knife is what
    /// you fight with. That is the player's trade too, and it is made for
    /// the same reason.
    fn camp_tool(&self, location: usize) -> Option<usize>;
    /// **Something lying here that would pay a mercenary** — a snake, some
    /// meat, a bottle of water — named by the caller's handle for it.
    ///
    /// Not the same question as food, though the answers overlap. A camp set
    /// to work by a knife makes maggots, and maggots feed a man without
    /// paying one: an opponent living on its own production had food and
    /// still could not hire, and walked to a city and back for weeks over
    /// it. What it can pay with is a thing to go and fetch.
    fn payment_here(&self, location: usize) -> Option<usize>;
    /// **A production tool lying here**, named by the caller's handle.
    ///
    /// A camp feeds one man without a tool and makes a **surplus** with one,
    /// and the opponent leaves its own behind when it camps — so a second
    /// camp wants a second tool, and the world lays a few of them out. Which
    /// tool also decides what the place makes: a knife makes maggots, a trap
    /// makes meat, and meat is wages where maggots are only supper.
    fn tool_here(&self, location: usize) -> Option<usize>;
    /// Its own places with something standing in them that would pay a
    /// mercenary — where its wages are, as against where its supper is.
    fn my_wages(&self) -> Vec<usize>;
    /// **An empty container in its pack that the water here would fill**, as
    /// a pack slot.
    ///
    /// This is the short way to wages, and it is the one the world already
    /// gives everybody: a mercenary takes a snake, some meat or **a bottle
    /// of water**, and a bottle of water is an empty bottle plus a spring.
    /// The opponent starts with the empty bottle, like the player.
    fn fillable_here(&self, location: usize) -> Option<usize>;
    /// **Could a man left here keep himself?**
    ///
    /// A camp that is set to make something feeds exactly one man for ever —
    /// the classic table's knife is `amount=1 2`, one a day with no tool at
    /// all — and a spring keeps him watered. A place with neither is a place
    /// where leaving somebody is leaving him to run down: see
    /// `a_dry_camp_with_no_tool_kills_the_man_in_it`.
    ///
    /// Asked before camping, because the man cannot ask it himself.
    fn can_keep(&self, location: usize) -> bool;
}

/// What the opponent's group is carrying, as far as its decisions care.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Carried {
    /// How many days it could eat for.
    pub food: usize,
    /// Whether anything in the group's packs would pay a mercenary.
    pub payment: bool,
    /// Whether it is carrying a tool to leave in the next camp it makes.
    pub tool: bool,
}

/// One decision, so that the caller does it with the machinery it has and
/// the rule stays here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Set out for this location; the road is one day or more.
    Travel(usize),
    /// Leave one follower here: this is what claims a location. `tool` is a
    /// slot in the opponent's own pack to hand over as they go — see
    /// [`Ground::camp_tool`].
    Camp { at: usize, tool: Option<usize> },
    /// Take on the person standing here, paying with the named pack slot.
    Hire { at: usize, pay_with: usize },
    /// Drink from the location's spring, up to what the character needs.
    Drink(usize),
    /// Pick up the edible thing [`Ground::food_here`] named, where it lies.
    Take { at: usize, what: usize },
    /// Fill the empty container in this pack slot from the spring here.
    Fill { at: usize, slot: usize },
    /// Set this camp making the thing [`Ground::food_product`] named.
    Produce { at: usize, what: usize },
}

/// The expansion limit: `MaxHumanCampCount + MaxAdvance <= CampCount` stops
/// it. Kept as a function because the comparison is the rule and the numbers
/// are the caller's.
pub fn may_expand(settings: &GameSettings, held: usize, best_human: usize) -> bool {
    (best_human as i32) + settings.ai_camp_max_advance > held as i32
}

/// **Supply comes first, and it is honest: a spring, what is lying about, and
/// nothing invented.**
///
/// Eating itself is not here. A group eats through [`crate::game::party_day`],
/// which is the same day the player's group has — one turn per actor, one
/// mouthful per day, and no second helping from a rule of its own. What that
/// day cannot know about is what is standing or lying in the location, so
/// that is what the opponent asks for: drink where there is water, and pick
/// up something to eat when it has nothing.
///
/// Nothing is created. A dry location and an empty pack mean a thirsty
/// opponent, and it may die of it.
pub fn supply(rival: &Rival, ground: &impl Ground, carried: Carried) -> Vec<Act> {
    let mut acts = Vec::new();
    if rival.needs.water < rival.needs.max_water && ground.water_here(rival.at) > 0 {
        acts.push(Act::Drink(rival.at));
    }
    // **And it picks things up.** A month of an ordinary game showed what
    // was missing: it hired, camped, walked on, and died of hunger on the
    // fourteenth day, because the only food it ever had was the meat in its
    // start pack and it had paid that to a mercenary. It drank and it never
    // ate.
    //
    // What it does now is what a player does — take what is lying where you
    // stand, out of the same ground and the same rooms, through the same
    // accounting. Nothing is conjured; a place with nothing in it feeds
    // nobody, and it can still starve between two empty locations.
    //
    // Only when it is carrying nothing to eat: an opponent that swept up
    // every scrap it walked past would be stripping the world rather than
    // living in it.
    // **It picks up what it needs, and each need is asked once.** Eating
    // first; then something to pay a man with, which is the other thing it
    // needs and cannot make — a knife sets a camp making maggots, and maggots
    // feed a man without paying one.
    //
    // One thing may answer both, and then it is taken once. Nothing here is a
    // budget on how much an opponent may do in a day: it is two errands, and
    // it does the ones it has.
    let mut eat = None;
    if carried.food == 0 {
        eat = ground.food_here(rival.at);
        if let Some(what) = eat {
            acts.push(Act::Take { at: rival.at, what });
        }
    }
    let mut taken: Vec<usize> = eat.into_iter().collect();
    if !carried.payment {
        if let Some(what) = ground.payment_here(rival.at).filter(|w| !taken.contains(w)) {
            taken.push(what);
            acts.push(Act::Take { at: rival.at, what });
        }
    }
    // **And it fills its bottle.** A bottle of water is one of the three
    // things a mercenary will take, and a bottle of water is an empty bottle
    // and a spring. Without this the opponent had food it could not pay
    // anybody with, and walked to a city and back for weeks over it.
    //
    // After the drinking, so that what it needs itself comes first and the
    // filling takes only what the spring has left.
    if !carried.payment {
        if let Some(slot) = ground.fillable_here(rival.at) {
            acts.push(Act::Fill { at: rival.at, slot });
        }
    }
    // And a tool, when it has none to leave in the next camp. Without one a
    // camp feeds its one man and stores nothing, so this is the difference
    // between a place that keeps somebody and a place that supplies a
    // journey.
    if !carried.tool {
        if let Some(what) = ground.tool_here(rival.at).filter(|w| !taken.contains(w)) {
            acts.push(Act::Take { at: rival.at, what });
        }
    }
    acts
}

/// The first step of the shortest road to any of `targets`, by the same walk
/// [`next_step`] uses and with the same restriction: never on through ground
/// somebody else holds.
pub fn step_towards(rival: &Rival, ground: &impl Ground, targets: &[usize]) -> Option<usize> {
    if targets.contains(&rival.at) {
        return None;
    }
    let mut seen = alloc::vec![rival.at];
    let mut edge: Vec<(usize, usize)> = Vec::new();
    for (neighbour, days) in ground.roads(rival.at) {
        if days > 0 && can_travel(ground.owner(rival.at), rival.player, None, neighbour) {
            edge.push((neighbour, neighbour));
        }
    }
    while !edge.is_empty() {
        let mut next = Vec::new();
        for (here, first) in edge {
            if seen.contains(&here) {
                continue;
            }
            seen.push(here);
            if targets.contains(&here) {
                return Some(first);
            }
            match ground.owner(here) {
                Some(owner) if owner != rival.player => continue,
                _ => {}
            }
            for (neighbour, days) in ground.roads(here) {
                if days > 0 && !seen.contains(&neighbour) {
                    next.push((neighbour, first));
                }
            }
        }
        edge = next;
    }
    None
}

/// Where to go next: the nearest location this player could camp, walking the
/// road graph outward the way `NearestFreeCamp` does — shortest road first,
/// never through ground somebody else holds.
///
/// Returns the **first step**, not the destination: a journey is one road at
/// a time, and the opponent re-decides when it arrives.
pub fn next_step(rival: &Rival, ground: &impl Ground) -> Option<usize> {
    // Somewhere to camp right here is not a step at all.
    let mut seen = Vec::new();
    let mut edge: Vec<(usize, usize)> = Vec::new(); // (location, first step from home)
    seen.push(rival.at);
    for (neighbour, days) in ground.roads(rival.at) {
        if days == 0 {
            continue;
        }
        if !can_travel(ground.owner(rival.at), rival.player, None, neighbour) {
            continue;
        }
        edge.push((neighbour, neighbour));
    }
    while !edge.is_empty() {
        let mut next = Vec::new();
        for (here, first) in edge {
            if seen.contains(&here) {
                continue;
            }
            seen.push(here);
            if can_camp(here, rival.player, ground) {
                return Some(first);
            }
            // Only travel on through ground that is nobody's or ours.
            match ground.owner(here) {
                Some(owner) if owner != rival.player => continue,
                _ => {}
            }
            for (neighbour, days) in ground.roads(here) {
                if days > 0 && !seen.contains(&neighbour) {
                    next.push((neighbour, first));
                }
            }
        }
        edge = next;
    }
    None
}

/// **Is this a road it can eat its way along?**
///
/// The travel panel shows the player their supplies before they confirm a
/// journey, for exactly this reason, and a month of an ordinary game showed
/// what ignoring it costs: it lived off its camp, then committed to a
/// four-day march with an empty pack and died on the way back.
///
/// It may still starve — a short road to a place with nothing in it is
/// allowed, and so is being caught out — but not by walking into a week of
/// desert it cannot cross.
///
/// **Unless standing still is worse.** With nothing carried, nothing here and
/// no place of its own to walk back to, the choice is between a road it
/// cannot feed and starving where it stands. The road is the better of the
/// two, and taking it is a mistake it is allowed to make rather than a rule
/// that pins it to an empty place — an earlier version of this test pinned
/// it, and it died on the tenth day without ever leaving the city it hired
/// in.
pub fn can_feed(rival: &Rival, ground: &impl Ground, step: usize, food_carried: usize) -> bool {
    let nowhere_better = food_carried == 0
        && ground.food_here(rival.at).is_none()
        && ground.my_camps().is_empty();
    nowhere_better
        || ground
            .roads(rival.at)
            .into_iter()
            .find(|&(to, _)| to == step)
            // **One day's slack, because one day is survivable.** Going
            // without costs a character health and does not kill them
            // outright, and the travel panel warns the player about supplies
            // rather than refusing the journey. Without the slack the
            // opponent never leaves its first camp: it lives on one maggot
            // every three days and can never bank the two a two-day road
            // would want.
            .is_none_or(|(_, days)| days as usize <= food_carried + 1)
}

/// Can this player put a camp here? `CanCreateCamp`, minus the trap and
/// hazard-gear tests, which belong to production and protection and are not
/// in this slice — and said so rather than silently dropped.
pub fn can_camp(location: usize, player: usize, ground: &impl Ground) -> bool {
    if ground.is_city(location) {
        return false;
    }
    match ground.owner(location) {
        None => true,
        Some(owner) => owner == player,
    }
}

/// One turn of the opponent: at most one strategic act, plus whatever
/// supplying itself needs.
///
/// The order is `ClassicAiState.Turn`'s: with nobody to leave in a camp it
/// goes hiring; then the mode machine runs; and the interval is drawn from
/// the settings when a camp has just been made.
pub fn turn(
    rival: &mut Rival,
    ground: &impl Ground,
    settings: &GameSettings,
    held: usize,
    best_human: usize,
    // What the group is carrying, counted by the caller because what is
    // edible and what pays a man are the item table's business.
    carried: Carried,
    rng: &mut Rng,
) -> Vec<Act> {
    let mut acts = Vec::new();
    if rival.dead {
        return acts;
    }
    acts.extend(supply(rival, ground, carried));
    // **A camp is set making something to eat.** The world lays out about a
    // dozen edible things across thirty-seven locations, so living off what
    // is lying about is not living — food in this game is grown, and the
    // player grows it in the places they hold. An opponent that camped and
    // never set a production was keeping a place rather than living in it.
    if let Some(what) = ground.food_product(rival.at) {
        acts.push(Act::Produce { at: rival.at, what });
    }
    // On the road: a journey is days, and nothing strategic happens until it
    // ends. The caller ticks the journey with the same `travel_a_day` the
    // player uses.
    if rival.journey.is_some() {
        return acts;
    }

    // **Hungry with nothing here means the errand is food, not ground.**
    // Its own camps are where its stores are, and they are the only stores
    // it is allowed to know about — the info screen shows a player what is
    // in the places they hold and refuses to open anybody else's.
    if carried.food == 0 && ground.food_here(rival.at).is_none() {
        // A place of its own with something standing in it is the best
        // errand; any place of its own is the next best, because that is
        // where food appears. An empty store this morning is not a reason to
        // walk four days into the desert — the first version did exactly
        // that, emptied its camp, and died on the road it took next.
        let mut home = ground.my_larder();
        if home.is_empty() {
            home = ground.my_camps();
        }
        if !home.is_empty() {
            match step_towards(rival, ground, &home) {
                Some(step) => {
                    rival.headed = Some(step);
                    acts.push(Act::Travel(step));
                    return acts;
                }
                // Standing in it already: wait for the day's yield rather
                // than set out hungry. Waiting is a decision, and it is the
                // one a player makes here.
                None if home.contains(&rival.at) => return acts,
                None => {}
            }
        }
    }

    if rival.party.is_empty() && rival.mode != Mode::Hiring {
        rival.mode = Mode::Hiring;
    }

    match rival.mode {
        Mode::Waiting => {
            rival.wait -= 1;
            if rival.wait <= 0 && may_expand(settings, held, best_human) {
                rival.mode = Mode::Expanding;
            }
        }
        Mode::Expanding => {
            if !may_expand(settings, held, best_human) {
                rival.mode = Mode::Waiting;
                rival.wait = draw_interval(settings, rng);
                return acts;
            }
            // **A second man in the same camp is not a second camp.** Camping
            // is allowed on ground of its own — that is how a camp grows —
            // but the victory condition counts *places*, and an opponent that
            // put every man it hired into the first camp would never hold a
            // second one. So expansion means somewhere that is not already
            // its own; growing a camp is a different thing and not this.
            let unclaimed = ground.owner(rival.at).is_none();
            // **And it does not leave a man where he cannot keep himself.**
            // A camp that makes something feeds one man indefinitely and a
            // spring waters him; a place with neither is a countdown with a
            // flag on it. Holding ground that kills the man holding it is not
            // holding ground.
            let keeps = ground.can_keep(rival.at);
            if !rival.party.is_empty() && unclaimed && keeps
                && can_camp(rival.at, rival.player, ground)
            {
                acts.push(Act::Camp {
                    at: rival.at,
                    tool: ground.camp_tool(rival.at),
                });
                rival.headed = None;
                // The caller leaves one of them behind — `Party::leave`, the
                // player's own function — so the group is one shorter
                // afterwards. Asking it now is asking before the act.
                if rival.party.len() == 1 {
                    rival.mode = Mode::Hiring;
                } else {
                    rival.mode = Mode::Waiting;
                    rival.wait = draw_interval(settings, rng);
                }
            } else if let Some(step) =
                next_step(rival, ground).filter(|&step| can_feed(rival, ground, step, carried.food))
            {
                rival.headed = Some(step);
                acts.push(Act::Travel(step));
            }
        }
        Mode::Hiring => {
            match ground.hireable(rival.at, rival.player) {
                Some(slot) => {
                    acts.push(Act::Hire { at: rival.at, pay_with: slot });
                    rival.mode = Mode::Expanding;
                }
                // **Do not walk to a city you cannot pay in.** A month of an
                // ordinary game had it commuting between its camp and the
                // nearest city for weeks: it went to hire, found that maggots
                // are supper and not wages, and walked home for more maggots.
                // Nobody there was ever going to take them.
                //
                // Carrying wages is the change of state that makes the
                // journey worth making, so that is the test. Until then the
                // errand is to *get* wages: a place of its own with a staple
                // standing in it, which is what a camp set to a trap makes.
                None if !carried.payment => {
                    // Its own stores: a camp set to a trap makes meat, and
                    // meat is wages. Walking anywhere else to look would be
                    // the same fruitless journey in another direction — the
                    // way to *make* wages is [`Act::Fill`], which it does
                    // wherever it finds water.
                    let wages = ground.my_wages();
                    if !wages.is_empty() {
                        if let Some(step) = step_towards(rival, ground, &wages)
                            .filter(|&step| can_feed(rival, ground, step, carried.food))
                        {
                            rival.headed = Some(step);
                            acts.push(Act::Travel(step));
                        }
                    }
                }
                None => {
                    // A city is where people stand about. The nearest one,
                    // by road, and one road at a time.
                    // The same supply test the expansion road gets: a city
                    // four days off is no use to somebody who starves on the
                    // third. It was the hiring errand, not the expansion,
                    // that walked the first version to death.
                    if let Some(step) = nearest_city_step(rival, ground)
                        .filter(|&step| can_feed(rival, ground, step, carried.food))
                    {
                        rival.headed = Some(step);
                        acts.push(Act::Travel(step));
                    }
                }
            }
        }
    }
    acts
}

/// `Random.Next(MinInterval, MaxInterval)` — the half-open interval C# gives,
/// so a `3 6` line means three, four or five.
pub fn draw_interval(settings: &GameSettings, rng: &mut Rng) -> i32 {
    let (min, max) = settings.ai_camp_interval;
    if max <= min {
        return min;
    }
    min + (rng.next_u64() % ((max - min) as u64)) as i32
}

/// The first step towards the nearest city, by road.
pub fn nearest_city_step(rival: &Rival, ground: &impl Ground) -> Option<usize> {
    let mut seen = alloc::vec![rival.at];
    let mut edge: Vec<(usize, usize)> = Vec::new();
    for (neighbour, days) in ground.roads(rival.at) {
        if days > 0 && can_travel(ground.owner(rival.at), rival.player, None, neighbour) {
            edge.push((neighbour, neighbour));
        }
    }
    while !edge.is_empty() {
        let mut next = Vec::new();
        for (here, first) in edge {
            if seen.contains(&here) {
                continue;
            }
            seen.push(here);
            if ground.is_city(here) {
                return Some(first);
            }
            match ground.owner(here) {
                Some(owner) if owner != rival.player => continue,
                _ => {}
            }
            for (neighbour, days) in ground.roads(here) {
                if days > 0 && !seen.contains(&neighbour) {
                    next.push((neighbour, first));
                }
            }
        }
        edge = next;
    }
    None
}

/// Where a machine player stands inside a location, for drawing. It has no
/// business walking about a town; the entry point is where it is.
pub fn stands_at(entry: Point) -> Point {
    entry
}
