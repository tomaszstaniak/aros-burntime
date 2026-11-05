//! Taking somebody on.
//!
//! Two steps, and they are deliberately separate. [`offer`] answers what a
//! panel needs to draw — can this person be asked, what do they accept, what
//! would actually be handed over — and changes nothing. [`hire`] runs the
//! same checks again against the model and only then moves anything, so a
//! panel drawn a moment ago cannot authorise a hire that is no longer legal.
//!
//! Ported from v0.9.1's `Dialog.GetGreetingConversation`,
//! `Dialog.GetTalkConversation` and `Character.Hire`.

use alloc::vec::Vec;

use crate::game::{CharacterNeeds, ItemBag, ItemFacts, Party, PACK_CAPACITY};
use crate::gamdat::{CharClass, CharacterRecord, Placed};

/// `boss.Player.Group.Count < 5`. The count includes the leader, who is
/// always index 0 of `Group`, so it caps the companions at four.
pub const MAX_GROUP: usize = 5;

/// Why somebody will not join.
///
/// The order is the order the original tests in: the greeting decides
/// whether the offer is even shown, and only then does the talk conversation
/// weigh experience and payment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A dog, a mutant, or a trader — see [`CharClass::can_be_hired`].
    NotOpenToWork,
    /// `Parent.Player != null && Parent.Player != boss.Player`: somebody
    /// else's employee, whose greeting is switched to text file 510 and who
    /// is then offered neither trade nor work.
    ///
    /// **Your own camper is refused too**, for a reason the original does not
    /// have to state: there, a character of yours gets a different greeting
    /// because it is yours. Here a camper stays standing so he can be fought
    /// for, and without this an opponent could hire its own man out of his
    /// camp and leave him holding it as well.
    WorksForSomeoneElse,
    /// `boss.Player.Group.Count < 5` fails.
    GroupFull,
    /// `boss.Experience >= Parent.Experience * 0.66f` fails.
    TooInexperienced,
    /// The leader carries nothing this person accepts.
    NothingTheyAccept,
    /// No such person is standing here.
    NoSuchPerson,
}

/// What a panel needs to show, and nothing more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// Which person, by their index in [`People`].
    pub who: usize,
    /// Everything they accept, in `HireItems` order — the order the panel
    /// should list them in, because it is the order the search uses.
    pub accepts: Vec<usize>,
    /// The slot in the leader's pack that would be handed over. **The first
    /// match in `HireItems` order, not the first matching slot**: `Hire`
    /// walks the accepted types and asks the pack for each.
    pub pay_from_slot: usize,
    /// What is in that slot.
    pub pay_with: usize,
}

/// One person standing in a location, built from a `gam.dat` record.
///
/// They keep the record's own numbers — experience decides whether they can
/// be asked at all, food and water are what they bring with them.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    /// Their index in the file's character table, which is their identity.
    pub record: usize,
    pub location: usize,
    pub class: CharClass,
    pub name_id: u16,
    /// `TextId`: which `MEN_NNN.TXT` they speak from (`Dialog.MenFile`).
    pub men_file: u16,
    pub face_id: u8,
    /// The body: the first of its sixteen frames in `syssze.raw`'s
    /// numbering, with [`crate::player_colors::GRAY`] for v1.3's gray
    /// clothes ([`crate::player_colors::Body::id`]).
    pub sprite_id: u16,
    /// Name, face and body as the record gives them. v1.2's respawn makes a
    /// person somebody else (`RefreshIdentity`); a save writes the ones
    /// that differ.
    pub start_identity: (u16, u8, u16),
    pub experience: i32,
    /// Where in the location they stand, in map coordinates.
    pub at: crate::game::Point,
    /// `Path.MoveTo`: where they are walking, which is where they stand
    /// when they are not.
    pub move_to: crate::game::Point,
    /// What they are thinking about — see [`crate::mind::Mind`].
    pub mind: crate::mind::Mind,
    /// How they move in a v1.2 game, beyond where they are going — see
    /// [`crate::gait::Bearing`].
    pub bearing: crate::gait::Bearing,
    pub needs: CharacterNeeds,
    /// Resolved `HireItems`: item-type indices, in the order `Hire` searches.
    pub accepts: Vec<usize>,
    /// Their own six slots.
    pub pack: ItemBag,
    /// What they have drawn out of it — see [`crate::combat::Kit`]. A dog
    /// carries nothing and so draws nothing, but a mutant or a trader may.
    pub kit: crate::combat::Kit,
    /// `BaseAttackValue`, from the settings of the game being played:
    /// thirty for a dog, forty for a mutant, sixty for a trader, and ten
    /// for everybody else.
    pub base_attack: i32,
    /// What this one is at full health.
    ///
    /// **It is not always a hundred.** `GameCreation` gives a dog
    /// `dog_health` and a mutant `mutant_health` rather than the number in
    /// their record, `Revive` puts the same back, and `Dog.Turn` restores
    /// it every morning. For the four professions it is the hundred
    /// `Character.Revive` sets, though what they *start* with is their
    /// record's own health.
    pub full_health: i32,
    /// What they had when the world was built: a creature's class health
    /// or a profession's own record, and the record's food and water.
    ///
    /// **A save writes down whoever is not at this any more**, and the
    /// difference matters: 173 of the 300 placed records start below a
    /// hundred, so comparing against full health would name most of the
    /// world in every save.
    pub start: CharacterNeeds,
    /// And where they were put. A save writes down whoever has moved.
    pub start_at: crate::game::Point,
    /// The location and the experience of their record. Somebody dismissed
    /// stays wherever their boss let them go, knowing what they learnt.
    pub start_location: usize,
    pub start_experience: i32,
    /// What they carried when the world was built: nothing, or what v1.2
    /// Classic's `gam.dat` hands them ([`People::hand_out`]). A save writes
    /// down whoever carries anything else — **including nothing, when they
    /// started with something**: a man a machine player took his only item
    /// from looks built in every other way, and a load that rebuilt the
    /// world would hand it back to him.
    pub start_pack: ItemBag,
    /// Who they already work for, if anybody. `None` is nobody.
    pub employer: Option<usize>,
    /// Set once they join a party, so the same record cannot be hired twice.
    pub hired: bool,
    /// **They died.**
    ///
    /// A death leaves a record in neither of the two wrong states: not
    /// "hired" with nobody to show for it, and not standing about ready to
    /// be hired again. `Die` takes them out of the player's empire — so
    /// `hired` goes false, they belong to nobody — while this keeps them
    /// out of [`People::at`], so there is no figure to talk to.
    ///
    /// A respawn would clear it. Nothing does yet; see
    /// [`crate::game::Death::RESPAWNS`].
    pub dead: bool,
    /// **When they came into their location's list** (`Location.Characters`,
    /// v1.2): nought for everybody the world was built with, in record
    /// order (`GameCreation.LoadNPCs`), then one more than anybody's each
    /// time somebody is appended — a respawn (`EnterLocation`), a companion
    /// let go (`Dismiss`), a man left in a camp (`JoinCamp`), a trader who
    /// walked on (`NextSellLocation`). [`People::at`] goes by it.
    pub entered: u32,
    /// Dead, and still in his location's list: `Die` does not take a man
    /// out of `Location.Characters`, the end of his death animation does
    /// (`Character.cs:963-968`), and that plays only where the player is
    /// watching. A camper who dies in the day's turn, or a companion whose
    /// body `Dismiss` leaves at the group's place, lies there until the
    /// place is shown or he comes back. He has no day (`if (IsDead)
    /// return;`), but he holds his place in the list.
    pub lying: bool,
}

/// Everybody `LoadNPCs` puts on the map.
#[derive(Clone, Debug, Default)]
pub struct People {
    people: Vec<Person>,
    /// v1.2's `Location.Characters` order ([`Person::entered`]) is the
    /// order of [`People::at`]. Off, a legacy game keeps record order.
    pub arrival: bool,
    /// What each seat dresses its recruits in (`Player.CharacterBodyColorSet`,
    /// v1.3), by seat; `None` leaves a recruit's body alone.
    pub outfits: [Option<crate::player_colors::BodySet>; 4],
}

impl Person {
    /// `Character.Hire`'s new clothes (`Character.cs:466-472` at v1.3):
    /// somebody whose class has a body set of its own (`GetSetBodyId`) takes
    /// it in the colours of the boss who hires him. A dog, a mutant or a
    /// trader keeps his.
    pub fn wear(&mut self, outfit: Option<crate::player_colors::BodySet>) {
        // v0.9.1's `Hire` (`Character.cs:340-345` at `v0.9.1`): its own
        // body numbers, for everybody it hires (a trader has none).
        if let Some(crate::player_colors::BodySet::V091(set)) = outfit {
            if self.class != CharClass::Trader {
                self.sprite_id = crate::player_colors::v091_body(self.class, set);
            }
            return;
        }
        let (Some(set), Some(body)) = (outfit, crate::player_colors::set_body_id(self.class)) else { return };
        if let Some(b) = crate::player_colors::character_body(body, set) {
            self.sprite_id = b.id();
        }
    }

    /// Is anything about them no longer what the world was built with?
    fn has_moved_on(&self) -> bool {
        self.at != self.start_at
            || self.move_to != self.start_at
            || self.needs != self.start
            || self.mind != crate::mind::Mind::default()
            || !self.bearing.is_rest(self.at)
            || self.location != self.start_location
            || self.experience != self.start_experience
            || self.pack != self.start_pack
            || self.kit != crate::combat::Kit::default()
    }
}

impl People {
    /// Build the world's characters from the file's records.
    ///
    /// Only records that name a real location produce somebody: the four
    /// player slots and the five homeless traders are not people standing
    /// anywhere. Hire items are resolved here because the item table is what
    /// resolves them and it differs between game modes.
    pub fn from_records(
        records: &[CharacterRecord],
        facts: &impl ItemFacts,
        settings: &crate::settings::GameSettings,
    ) -> People {
        let mut people = People { people: Vec::new(), arrival: false, outfits: [None; 4] };
        people.add_records(records, 0, facts, settings);
        people
    }

    /// The same for records after the first `first` of the table — v1.3's
    /// residents (`population::resident_records`), appended as
    /// `AllCharacters += character` appends them: after everybody else.
    pub fn add_records(
        &mut self,
        records: &[CharacterRecord],
        first: usize,
        facts: &impl ItemFacts,
        settings: &crate::settings::GameSettings,
    ) {
        let staples = [
            facts.kind_named("item_snake"),
            facts.kind_named("item_meat"),
            facts.kind_named("item_water_bottle"),
        ];
        let people = &mut self.people;
        for (record, r) in records.iter().enumerate().skip(first) {
            let Placed::At(location) = r.placed else { continue };
            let accepts = accepted_by(r, &staples);
            // **A creature's health is not the record's.** `LoadNPCs`
            // overwrites it with the class number for a trader, a mutant
            // and a dog, and keeps the record's only for the four
            // professions.
            let health = match r.class {
                CharClass::Trader | CharClass::Mutant | CharClass::Dog => {
                    settings.full_health(r.class)
                }
                _ => r.health as i32,
            };
            people.push(Person {
                record,
                location,
                class: r.class,
                name_id: r.name_id,
                men_file: r.text_id as u16,
                face_id: r.face_id,
                sprite_id: r.sprite_id as u16,
                start_identity: (r.name_id, r.face_id, r.sprite_id as u16),
                experience: r.experience as i32,
                at: r.at,
                move_to: r.at,
                mind: crate::mind::Mind::default(),
                bearing: crate::gait::Bearing::default(),
                needs: CharacterNeeds {
                    water: r.water as i32,
                    max_water: crate::game::MAX_WATER,
                    food: r.food as i32,
                    max_food: crate::game::MAX_FOOD,
                    // **A creature's health is not the record's.**
                    // `LoadNPCs` overwrites it with the class number for a
                    // trader, a mutant and a dog, and keeps the record's
                    // only for the four professions.
                    health,
                },
                accepts,
                pack: ItemBag::new(PACK_CAPACITY),
                kit: crate::combat::Kit::default(),
                base_attack: settings.base_attack(r.class),
                full_health: settings.full_health(r.class),
                start_at: r.at,
                start_location: location,
                start_experience: r.experience as i32,
                start_pack: ItemBag::new(PACK_CAPACITY),
                start: CharacterNeeds {
                    water: r.water as i32,
                    max_water: crate::game::MAX_WATER,
                    food: r.food as i32,
                    max_food: crate::game::MAX_FOOD,
                    health,
                },
                employer: (r.employer != 0).then(|| r.employer as usize - 1),
                hired: false,
                dead: false,
                entered: 0,
                lying: false,
            });
        }
    }

    pub fn len(&self) -> usize {
        self.people.len()
    }

    /// Give one record an item as part of the world being built — v1.2
    /// Classic's `gam.dat` items held by characters — so that it is what
    /// they start with, not something they came by later. `false` for a
    /// record nobody stands for.
    pub fn hand_out(&mut self, record: usize, kind: usize) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(p) => {
                p.pack.push(kind);
                p.start_pack.push(kind);
                true
            }
            None => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
    }

    pub fn get(&self, who: usize) -> Option<&Person> {
        self.people.get(who)
    }

    /// Whoever that `gam.dat` record is, by the identity that outlives
    /// everything else about them.
    pub fn get_by_record(&self, record: usize) -> Option<&Person> {
        self.people.iter().find(|p| p.record == record)
    }

    pub fn get_mut(&mut self, who: usize) -> Option<&mut Person> {
        self.people.get_mut(who)
    }

    /// Who has been taken on, as `(record, employer)` — the whole of what a
    /// save needs about the world's people, because everything else about
    /// them comes back from `gam.dat` unchanged.
    pub fn hired(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.people
            .iter()
            .filter(|p| p.hired)
            .map(|p| (p.record, p.employer.unwrap_or(0)))
    }

    /// Mark somebody as taken on, when restoring a save. `false` when no
    /// such person stands anywhere — which is a save that does not match
    /// this world, not a person who has wandered off.
    /// The clothes `employer` hires into.
    pub fn outfit(&self, employer: usize) -> Option<crate::player_colors::BodySet> {
        self.outfits.get(employer).copied().flatten()
    }

    /// `AfterDeserialization`'s "fix character skins" for whoever works for
    /// somebody (`Character.cs:310-317` at v1.3): the body of his class in
    /// his employer's clothes, as a load rebuilds it.
    pub fn dress_employed(&mut self) {
        let outfits = self.outfits;
        for p in self.people.iter_mut() {
            if let Some(e) = p.employer {
                p.wear(outfits.get(e).copied().flatten());
            }
        }
    }

    pub fn set_hired(&mut self, record: usize, employer: usize) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(person) => {
                person.hired = true;
                person.employer = Some(employer);
                true
            }
            None => false,
        }
    }

    /// Put every trader's figure where their shop is.
    ///
    /// **[`crate::trader::Trader::at`] is the one authoritative answer to
    /// where a trader is.** The world holds each of them twice — here, which
    /// is where their figure stands and who can be talked to, and in
    /// `Traders`, which is what they carry and which one moves on its own —
    /// and only `NextSellLocation` decides. This copies that answer across,
    /// and it is the only thing that may write a trader's `location`.
    ///
    /// It has to run after a turn *and* after a load: a saved trader can be
    /// away from home, and rebuilding the people from `gam.dat` puts every
    /// figure back at its record's location.
    ///
    /// Returns how many figures it could not find, which is a save naming
    /// somebody this world does not have.
    pub fn follow_traders(&mut self, traders: &crate::trader::Traders) -> usize {
        let mut lost = 0;
        for trader in traders.iter() {
            if !self.move_person(trader.record, trader.at) {
                lost += 1;
            }
        }
        lost
    }

    /// `Mind.RequestToTalk`: somebody the player has walked up to stops
    /// and waits, rather than wandering off mid-sentence.
    pub fn asked_to_talk(&mut self, who: usize) {
        if let Some(person) = self.people.get_mut(who) {
            let at = person.at;
            person.mind.asked_to_talk(at, &mut person.move_to);
        }
    }

    /// Move one figure. Prefer [`People::follow_traders`]; this is what it
    /// is built from.
    pub fn move_person(&mut self, record: usize, location: usize) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(person) => {
                // A trader who has walked to another town starts their day
                // there, not mid-stride towards a point in the last one.
                // **Only when the town changes**: `EnterLocation` is what
                // ends the walk, and a trader still where he was has not
                // entered anywhere. This runs for every trader on every load,
                // and cancelling the walk each time made a resumed game
                // diverge from an unbroken one on its very first step.
                if person.location != location {
                    person.location = location;
                    person.move_to = person.at;
                    // `Location = …`: out of one list and onto the end of
                    // the other.
                    let next = self.next_entry();
                    if let Some(person) = self.people.iter_mut().find(|p| p.record == record) {
                        person.entered = next;
                    }
                }
                true
            }
            None => false,
        }
    }

    /// One more than anybody's: the end of every location's list.
    fn next_entry(&self) -> u32 {
        self.people.iter().map(|p| p.entered).max().unwrap_or(0) + 1
    }

    /// Put somebody at the end of his location's list.
    fn append(&mut self, record: usize) {
        let next = self.next_entry();
        if let Some(person) = self.people.iter_mut().find(|p| p.record == record) {
            person.entered = next;
        }
    }

    /// **Put somebody in a location as another player's camper.**
    ///
    /// `JoinCamp` does not take a character off the map: it moves them to the
    /// location and leaves them standing there, which is why an enemy camp is
    /// a person you can walk up to and fight rather than a flag. So this sets
    /// the employer — `offer` refuses anybody already working for another
    /// player — without setting `hired`, which is what marks somebody as
    /// walking in a group and hides them from [`People::at`].
    ///
    /// Returns `false` when no such record stands anywhere.
    pub fn guarding(
        &mut self,
        record: usize,
        employer: usize,
        location: usize,
        at: crate::game::Point,
    ) -> bool {
        // `JoinCamp`: `Location = Player.Location`, the end of its list.
        self.append(record);
        self.garrison(record, employer, location, at)
    }

    /// **The same person, read back from a file rather than put there.**
    ///
    /// A camp's defender is written down twice: as somebody standing, which
    /// is the only record of what he is doing, and as the camp's resident,
    /// which is the only record of *which* location he is in — a standing
    /// line carries no location, because everybody else in the world is
    /// still where `gam.dat` put them. The two are written in step, so the
    /// resident's position is where he actually is.
    pub fn garrison(
        &mut self,
        record: usize,
        employer: usize,
        location: usize,
        at: crate::game::Point,
    ) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(person) => {
                person.hired = false;
                person.employer = Some(employer);
                person.location = location;
                person.at = at;
                person.move_to = at;
                true
            }
            None => false,
        }
    }

    /// Where one record is standing, if they are standing anywhere. Used to
    /// keep a camp's resident in step with the figure in the location — see
    /// the save's camps.
    /// **Standing means standing.** Somebody walking with a party is `hired`
    /// and their position is wherever the world last had them — stale, and
    /// nothing to do with the camp their record is now a resident of. Asking
    /// about them and copying the answer into the camp is what made a save
    /// disagree with itself.
    pub fn standing_at(&self, record: usize) -> Option<crate::game::Point> {
        self.standing_pose(record).map(|(at, _)| at)
    }

    /// Where he is **and where he is walking**. Both, because a camper who
    /// is mid-step has a destination, and copying only the first of the two
    /// into the camp stops him where he stood and leaves the file disagreeing
    /// with itself about the other.
    pub fn standing_pose(
        &self,
        record: usize,
    ) -> Option<(crate::game::Point, crate::game::Point)> {
        self.people
            .iter()
            .find(|p| p.record == record && !p.dead && !p.hired)
            .map(|p| (p.at, p.move_to))
    }

    /// Everybody still standing in a location, by index, **in the order
    /// they came there** (`Location.Characters`: record order for those the
    /// world was built with, then each newcomer at the end — see
    /// [`Person::entered`]). The hired are walking with somebody and the
    /// dead are not standing at all.
    pub fn at(&self, location: usize) -> impl Iterator<Item = usize> + '_ {
        self.listed(location, false).into_iter()
    }

    /// `Location.Characters` whole: [`People::at`] and the dead still lying
    /// there ([`Person::lying`]), in the list's order. What `Location.Turn`
    /// walks.
    pub fn roll(&self, location: usize) -> Vec<usize> {
        self.listed(location, true)
    }

    fn listed(&self, location: usize, lying: bool) -> Vec<usize> {
        let mut here: Vec<usize> = (0..self.people.len())
            .filter(|&i| {
                let p = &self.people[i];
                !p.hired && p.location == location && (!p.dead || (lying && p.lying))
            })
            .collect();
        if self.arrival {
            here.sort_by_key(|&i| (self.people[i].entered, i));
        }
        here
    }

    /// Every list back to the world as built, before a save's order is put
    /// back.
    pub fn clear_order(&mut self) {
        for p in self.people.iter_mut() {
            p.entered = 0;
            p.lying = false;
        }
    }

    /// Who has come into a list since the world was built, `(record,
    /// entered)`, for a save.
    pub fn entries(&self) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.people.iter().filter(|p| self.arrival && p.entered != 0).map(|p| (p.record, p.entered))
    }

    /// Put one back, when restoring a save.
    pub fn set_entered(&mut self, record: usize, entered: u32) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(p) => {
                p.entered = entered;
                true
            }
            None => false,
        }
    }

    /// The dead still lying in a list, `(record, location)`, for a save:
    /// a dead man is written down nowhere else, and the place he lies in is
    /// not his record's.
    pub fn lying(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.people.iter().filter(|p| self.arrival && p.dead && p.lying).map(|p| (p.record, p.location))
    }

    /// `Location = …; Position = Location.EntryPoint; Path.Stop(Position)`:
    /// a wandering trader who walked on stands at his new place's entry
    /// (`Trader.NextSellLocation`, `Trader.cs:131-153`).
    pub fn stand_at_entry(&mut self, record: usize, entry: crate::game::Point) {
        if let Some(p) = self.people.iter_mut().find(|p| p.record == record) {
            p.at = entry;
            p.move_to = entry;
            p.bearing.stride.stop(entry);
        }
    }

    /// `Dog.Turn` and `Mutant.Turn` set `Health` to the class's before
    /// `IsDead` is asked (`Dog.cs:28-34`, `Mutant.cs:28-34`): a creature
    /// still lying in his place's list is alive again, and has his day.
    /// `false` for anybody else.
    pub fn revive_lying_creature(&mut self, who: usize) -> bool {
        let Some(p) = self.people.get_mut(who) else { return false };
        if !(p.dead && p.lying && matches!(p.class, CharClass::Dog | CharClass::Mutant)) {
            return false;
        }
        p.dead = false;
        p.lying = false;
        p.needs.health = p.full_health;
        true
    }

    /// One of those, when restoring a save; `false` for somebody not dead.
    pub fn set_lying(&mut self, record: usize, location: usize) -> bool {
        match self.people.iter_mut().find(|p| p.record == record && p.dead) {
            Some(p) => {
                p.lying = true;
                p.location = location;
                true
            }
            None => false,
        }
    }

    /// The end of the death animations where the player watches: the dead
    /// lying in this place leave its list (`Character.Update`, `Location =
    /// null`, `Character.cs:963-968`).
    pub fn laid_out(&mut self, location: usize) {
        for p in self.people.iter_mut().filter(|p| p.lying && p.location == location) {
            p.lying = false;
        }
    }

    /// A day for everybody who belongs to nobody.
    ///
    /// **`Character.Turn` sends them somewhere else entirely.** Its first
    /// two lines are `if (IsDead) return;` and `if (Player == null) {
    /// TurnNonPlayer(); return; }`, so a free person never reaches the
    /// eating, the drinking, the deprivation or the death that a player's
    /// people do. `TurnNonPlayer` is the whole of their day:
    ///
    /// ```csharp
    /// if (Food == 0)  Food  = Random.Next() % (MaxFood  - 1) + 1;
    /// if (Water == 0) Water = Random.Next() % (MaxWater - 1) + 1;
    /// Food--;  if (Food < 0) Food = 0;
    /// Water--; if (Water < 0) Water = 0;
    /// ```
    ///
    /// So **they forage rather than starve**: empty means they find one to
    /// eight of food and one to four of water, and then spend one of each.
    /// Their health is never touched. That is why the world does not empty
    /// itself out over a few weeks, and why somebody who comes back with
    /// nothing is in no danger.
    ///
    /// In a legacy game each draws from a stream of their own, mixed the
    /// same way a trader's is, so the order they are visited in changes
    /// nothing and a save needs no generator position. In a v1.2 game
    /// (`game`) every draw is the game generator's, in this order.
    ///
    /// **v1.2's traders forage nothing**: `Trader.Turn` does not call
    /// `base.Turn` (`Logic/Character/Trader.cs:94-102`), so `TurnNonPlayer`
    /// never reaches them — their health is restored and that is all.
    pub fn turn(&mut self, seed: u64, day: u16, mut game: Option<&mut crate::spawn::Rng>) {
        for person in self.people.iter_mut() {
            // `Character.Turn`: `TurnNonPlayer` only when `Player == null`.
            // A camper belongs to somebody — his day is the camp's, and
            // his figure here only stands for him.
            if person.dead || person.hired || person.employer.is_some() {
                continue;
            }
            // **A creature is whole again every morning.** `Dog.Turn` and
            // `Mutant.Turn` both open with `Health = …Health` before
            // anything else, so wounding one and walking away is worth
            // nothing the next day. `Trader.Turn` does the same at its
            // own two points. The professions have no such line: what a
            // free person's day does to them is in `TurnNonPlayer`, below,
            // and it never touches health.
            if matches!(
                person.class,
                CharClass::Dog | CharClass::Mutant | CharClass::Trader
            ) {
                person.needs.health = person.full_health;
            }
            match game.as_deref_mut() {
                Some(_) if person.class == CharClass::Trader => continue,
                Some(rng) => {
                    // `Next() % (MaxFood - 1) + 1`.
                    if person.needs.food == 0 {
                        person.needs.food = rng.next_mod((person.needs.max_food - 1).max(0) as u32) as i32 + 1;
                    }
                    if person.needs.water == 0 {
                        person.needs.water = rng.next_mod((person.needs.max_water - 1).max(0) as u32) as i32 + 1;
                    }
                }
                None => {
                    let mut rng = crate::trader::rng_for(seed, day, person.record);
                    if person.needs.food == 0 {
                        person.needs.food =
                            (rng.next_u64() % (person.needs.max_food as u64 - 1)) as i32 + 1;
                    }
                    if person.needs.water == 0 {
                        person.needs.water =
                            (rng.next_u64() % (person.needs.max_water as u64 - 1)) as i32 + 1;
                    }
                }
            }
            person.needs.food = (person.needs.food - 1).max(0);
            person.needs.water = (person.needs.water - 1).max(0);
        }
    }

    /// v1.2's day for one of a location's people who belongs to nobody, in
    /// his place in the list (`Location.Turn`): a creature is whole again
    /// (`Dog.Turn`, `Mutant.Turn`), and `TurnNonPlayer` forages from the
    /// game's generator when a need is at nought, then spends a level of
    /// each. The dead have no day. A wandering trader's is
    /// [`crate::trader::Traders::turn_one_v12`], not this.
    pub fn turn_one(&mut self, who: usize, rng: &mut crate::spawn::Rng) {
        let Some(person) = self.people.get_mut(who) else { return };
        if person.dead || person.hired || person.employer.is_some() {
            return;
        }
        if matches!(person.class, CharClass::Dog | CharClass::Mutant) {
            person.needs.health = person.full_health;
        }
        // `Next() % (MaxFood - 1) + 1`.
        if person.needs.food == 0 {
            person.needs.food = rng.next_mod((person.needs.max_food - 1).max(0) as u32) as i32 + 1;
        }
        if person.needs.water == 0 {
            person.needs.water = rng.next_mod((person.needs.max_water - 1).max(0) as u32) as i32 + 1;
        }
        person.needs.food = (person.needs.food - 1).max(0);
        person.needs.water = (person.needs.water - 1).max(0);
    }

    /// `Trader.RestoreTraderHealth`, at the end of a living trader's turn.
    pub fn restore_trader(&mut self, record: usize) {
        if let Some(p) = self.people.iter_mut().find(|p| p.record == record && !p.dead) {
            p.needs.health = p.full_health;
        }
    }

    /// **One frame of everybody's life in one location.**
    ///
    /// Only the location the player is in: upstream updates the scene's
    /// characters and nobody else's, so the rest of the world holds still
    /// while you are not looking at it. That is why a save can carry
    /// positions for a few dozen people rather than for three hundred.
    ///
    /// `walkable` answers `Location.Map.Mask` for a point, and `player` is
    /// where the played character is standing — `None` when they are not in
    /// this location, which is the same question upstream asks before a
    /// creature looks at anything.
    ///
    /// Returns whoever bit this frame, for the caller to settle: a mind
    /// decides to attack, but the blow needs both packs and both kits.
    pub fn think(
        &mut self,
        location: usize,
        d: &mut crate::mind::Draws,
        player: Option<crate::game::Point>,
        walkable: &impl Fn(crate::game::Point) -> bool,
    ) -> Vec<usize> {
        self.think_with(location, d, &|_| player.map(|p| (p, 0)), walkable)
            .into_iter()
            .map(|(i, _)| i)
            .collect()
    }

    /// [`People::think`], each creature watching whom `target` gives it
    /// from where it stands — v1.2's `SelectAttackTarget`, which passes over
    /// whoever carries bones or a skull. Returns the creature and the tag of
    /// whom it bit.
    pub fn think_with(
        &mut self,
        location: usize,
        d: &mut crate::mind::Draws,
        target: &impl Fn(crate::game::Point) -> Option<(crate::game::Point, usize)>,
        walkable: &impl Fn(crate::game::Point) -> bool,
    ) -> Vec<(usize, usize)> {
        let mut struck = Vec::new();
        // v1.2's `Location.Update` minds its people in the list's order, and
        // each draws from the one generator; a legacy mind draws from its own
        // stream, and record order is kept.
        let order: Vec<usize> = if d.is_game() { self.listed(location, false) } else { (0..self.people.len()).collect() };
        for i in order {
            let person = &mut self.people[i];
            if person.dead || person.hired || person.location != location {
                continue;
            }
            // **A camper stands where he was left.** `JoinCamp` gives him a
            // `PlayerControlledMind`, which does nothing until his player
            // moves him — so neither a human's nor a machine player's man
            // wanders about the place he holds.
            if person.employer.is_some() {
                continue;
            }
            let (record, at) = (person.record, person.at);
            let mut move_to = person.move_to;
            let watched = target(at);
            let bit = match person.class {
                CharClass::Dog | CharClass::Mutant => person.mind.prowl(
                    d,
                    record,
                    at,
                    &mut move_to,
                    watched.map(|(p, _)| p),
                    walkable,
                ),
                _ => {
                    person.mind.wander(d, record, at, &mut move_to, walkable);
                    false
                }
            };
            person.move_to = move_to;
            if bit {
                struck.push((i, watched.map_or(0, |(_, tag)| tag)));
            }
        }
        struck
    }

    /// **v1.2's `JoinCamp`** gives the new camper a fresh `SimpleMind` and a
    /// `SimplePath` stopped where he stands (`Character.cs:372-380`): he
    /// roams the place from then on, as anybody free does.
    pub fn joined_camp(&mut self, record: usize) {
        if let Some(p) = self.people.iter_mut().find(|p| p.record == record) {
            p.mind = crate::mind::Mind::default();
            p.bearing = crate::gait::Bearing { tame: matches!(p.class, CharClass::Dog | CharClass::Mutant), ..Default::default() };
            p.move_to = p.at;
        }
    }

    /// **`Player.SelectCharacter`** on one of the player's campers: a
    /// `PlayerControlledMind` and a path stopped where he stands
    /// (`Logic/Player.cs:199-211`). He walks where the player sends him and
    /// nowhere else until the next day.
    pub fn take_in_hand(&mut self, who: usize) {
        if let Some(p) = self.people.get_mut(who) {
            p.bearing.controlled = true;
            p.move_to = p.at;
            p.bearing.stride.stop(p.at);
        }
    }

    /// **`ResetStationedMind`**, in every employed character's day
    /// (`Character.cs:782-800,893-904`): a camper the player took in hand
    /// is given a `SimpleMind` again and roams.
    pub fn reset_stationed_minds(&mut self) {
        for who in 0..self.people.len() {
            self.reset_stationed_mind(who);
        }
    }

    /// [`People::reset_stationed_minds`] for one camper, at the end of his
    /// own day (`Character.Turn`, after `TurnEmployedCharacter` when he
    /// lives): a camper whose day `Location.Turn` skipped keeps his hand.
    pub fn reset_stationed_mind(&mut self, who: usize) {
        if let Some(p) = self.people.get_mut(who) {
            if !p.dead && !p.hired && p.employer.is_some() && p.bearing.controlled {
                p.bearing.controlled = false;
                p.mind = crate::mind::Mind::default();
                p.move_to = p.at;
                p.bearing.stride.stop(p.at);
            }
        }
    }

    /// `SimplePath` found the way blocked: stop where they are.
    pub fn stop(&mut self, who: usize) {
        if let Some(person) = self.people.get_mut(who) {
            person.move_to = person.at;
        }
    }

    /// Where somebody is walking to, and where they have got to.
    pub fn step(&mut self, who: usize, to: crate::game::Point) {
        if let Some(person) = self.people.get_mut(who) {
            person.at = to;
        }
    }

    /// Put somebody back on the map after their time was up.
    ///
    /// **The same record, not a second person.** `Revive` restores health
    /// and `EnterLocation` sets the place; they belong to nobody, so they
    /// can be hired again — and their pack stays empty, because `Die`
    /// emptied it and nothing fills it.
    ///
    /// They stand at the location's **entry point** — `EnterLocation` sets
    /// `character.Position = EntryPoint`, and that point is in `gam.dat`'s
    /// own location record at offsets 18 and 20, which this crate has
    /// parsed all along.
    pub fn returned(
        &mut self,
        record: usize,
        location: usize,
        at: crate::game::Point,
        needs: crate::game::CharacterNeeds,
    ) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(person) => {
                person.dead = false;
                person.lying = false;
                person.hired = false;
                person.employer = None;
                person.location = location;
                person.at = at;
                person.move_to = at;
                person.mind = crate::mind::Mind::default();
                // Nothing of how he moved before he died: a save does not
                // keep it for the dead, so neither does the running game.
                // His mind is the one he had: `Revive` does not make a new
                // one.
                let tame = person.bearing.tame;
                person.bearing = crate::gait::Bearing { tame, ..Default::default() };
                person.needs = needs;
                person.pack = ItemBag::new(PACK_CAPACITY);
                // `EnterLocation`: the end of the place's list.
                self.append(record);
                true
            }
            None => false,
        }
    }

    /// `Dismiss` of somebody walking with a group: [`People::dismissed`],
    /// and `Location = Player.Location` puts him at the end of the place's
    /// list. A camper let go stays where he is in it.
    pub fn dismissed_companion(&mut self, record: usize, location: usize, who: &crate::game::Companion) -> bool {
        let found = self.dismissed(record, location, who);
        if found {
            self.append(record);
        }
        found
    }

    /// **`Character.Dismiss`**, for somebody who was walking with a player or
    /// sitting in their camp: `Player = null` and a `SimpleMind`. They stay
    /// where they are — at the location the group was in, where they stood —
    /// and keep everything about themselves: their needs, what they know,
    /// their pack and what they had drawn from it. Nothing is dropped.
    ///
    /// `false` when no such record, which is a companion who never came from
    /// one (a scenario's handed-over man); they simply leave.
    pub fn dismissed(&mut self, record: usize, location: usize, who: &crate::game::Companion) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(person) => {
                person.hired = false;
                person.employer = None;
                person.dead = false;
                person.location = location;
                person.at = who.at;
                person.move_to = who.at;
                person.mind = crate::mind::Mind::default();
                person.bearing = crate::gait::Bearing { tame: matches!(person.class, CharClass::Dog | CharClass::Mutant), ..Default::default() };
                person.needs = who.needs;
                person.experience = who.experience;
                person.pack = who.pack.clone();
                person.kit = who.kit;
                true
            }
            None => false,
        }
    }

    /// Anybody carrying a wound into the save, as `(record, health)`.
    ///
    /// Everybody standing in the world is rebuilt from `gam.dat`, which
    /// knows nothing of this morning's fight, so the exceptions have to be
    /// written down. The dead are not here — [`People::fallen`] has them,
    /// and a corpse is not a wound.
    pub fn wounded(&self) -> impl Iterator<Item = (usize, i32)> + '_ {
        self.people
            .iter()
            .filter(|p| !p.dead && p.needs.health != p.start.health)
            .map(|p| (p.record, p.needs.health))
    }

    /// **Everything about one of the world's people that `gam.dat` cannot
    /// give back**: where they have walked to, what they have foraged, what
    /// a fight left them at, and what they are thinking about.
    ///
    /// Only those who differ from the world as it was built. That is most
    /// of a location the player has stood in for a minute and none of the
    /// thirty-six they have not, because minds run only where the player
    /// is.
    pub fn standing(&self) -> impl Iterator<Item = Standing> + '_ {
        self.people.iter().filter(|p| !p.dead && p.has_moved_on()).map(|p| Standing {
            record: p.record,
            at: p.at,
            move_to: p.move_to,
            food: p.needs.food,
            water: p.needs.water,
            health: p.needs.health,
            mind: p.mind,
            bearing: p.bearing,
            location: p.location,
            experience: p.experience,
            pack: p.pack.clone(),
            kit: p.kit,
        })
    }

    /// Put one of those back.
    pub fn restore_standing(&mut self, s: Standing) -> bool {
        match self.people.iter_mut().find(|p| p.record == s.record) {
            Some(person) => {
                person.at = s.at;
                person.move_to = s.move_to;
                person.needs.food = s.food;
                person.needs.water = s.water;
                person.needs.health = s.health;
                person.mind = s.mind;
                person.bearing = s.bearing;
                person.location = s.location;
                person.experience = s.experience;
                person.pack = s.pack;
                person.kit = s.kit;
                true
            }
            None => false,
        }
    }

    /// Which records are dead, for a save. The rest of what is known about
    /// them comes back from `gam.dat`.
    pub fn fallen(&self) -> impl Iterator<Item = usize> + '_ {
        self.people.iter().filter(|p| p.dead).map(|p| p.record)
    }

    /// `Character.Die` for somebody standing in the world.
    ///
    /// The same three things it always does: **drop what they carried**
    /// where they fell, take them out of anybody's employ, and leave them
    /// at no health. What they dropped comes back for the caller to put on
    /// the ground, because the rules know who died and not where anybody
    /// was standing. Scheduling the return is the caller's too — the
    /// countdown belongs to [`crate::game::Respawns`], and how long it is
    /// depends on the class.
    ///
    /// `None` when there is no such person, or they are already dead:
    /// dying twice would drop a second copy of everything they had.
    pub fn fell(&mut self, who: usize) -> Option<Fall> {
        let person = self.people.get_mut(who)?;
        if person.dead {
            return None;
        }
        let dropped = core::mem::replace(&mut person.pack, ItemBag::new(PACK_CAPACITY));
        person.kit = crate::combat::Kit::default();
        person.hired = false;
        person.employer = None;
        person.dead = true;
        // In his place's list until the place is next shown.
        person.lying = true;
        person.needs.health = 0;
        Some(Fall {
            record: person.record,
            location: person.location,
            at: person.at,
            class: person.class,
            needs: person.needs,
            dropped,
        })
    }

    /// Mark the record a dead companion came from.
    ///
    /// **Neither hired nor hireable.** Leaving the group is `Dismiss`'s
    /// work and is right; staying "hired" afterwards would leave a record
    /// nobody can see and nobody can take on, and clearing only `hired`
    /// would put a corpse back on the map for the next player to talk to.
    ///
    /// `false` when no such record, which is a death from a companion who
    /// never came from one.
    /// v1.2's `RefreshIdentity` for somebody coming back: his name
    /// swapped with `donor`'s, a new face unless his is the gas mask (10),
    /// and a new body when his class has one.
    pub fn refresh_identity(&mut self, record: usize, donor: usize, face: u8, body: Option<u16>) -> bool {
        let Some(mine) = self.people.iter().position(|p| p.record == record) else { return false };
        let Some(theirs) = self.people.iter().position(|p| p.record == donor) else { return false };
        let (a, b) = (self.people[mine].name_id, self.people[theirs].name_id);
        self.people[mine].name_id = b;
        self.people[theirs].name_id = a;
        let p = &mut self.people[mine];
        if p.face_id != 10 {
            p.face_id = face;
        }
        if let Some(body) = body {
            p.sprite_id = body;
        }
        true
    }

    /// Whoever is not who the record says any more: `(record, name, face,
    /// body)`.
    pub fn identities(&self) -> impl Iterator<Item = (usize, u16, u8, u16)> + '_ {
        self.people
            .iter()
            .filter(|p| (p.name_id, p.face_id, p.sprite_id) != p.start_identity)
            .map(|p| (p.record, p.name_id, p.face_id, p.sprite_id))
    }

    /// Put an identity back, when restoring a save.
    pub fn set_identity(&mut self, record: usize, name: u16, face: u8, body: u16) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(p) => {
                p.name_id = name;
                p.face_id = face;
                p.sprite_id = body;
                true
            }
            None => false,
        }
    }

    pub fn died(&mut self, record: usize) -> bool {
        match self.people.iter_mut().find(|p| p.record == record) {
            Some(person) => {
                person.hired = false;
                person.employer = None;
                person.dead = true;
                true
            }
            None => false,
        }
    }

    /// v1.2's `Die` for one of a group, away from where the player
    /// watches: [`People::died`], and `Dismiss` leaves the body in the
    /// group's place (`Location = Player.Location`), at the end of its list
    /// and lying there ([`Person::lying`]).
    pub fn died_with_group(&mut self, record: usize, location: usize) -> bool {
        if !self.died(record) {
            return false;
        }
        self.append(record);
        if let Some(p) = self.people.iter_mut().find(|p| p.record == record) {
            p.location = location;
            p.lying = true;
        }
        true
    }

    /// v1.2's `Die` for a camper in the day's turn: [`People::died`], and he
    /// lies where he was in his camp's list.
    pub fn died_in_camp(&mut self, record: usize) -> bool {
        if !self.died(record) {
            return false;
        }
        if let Some(p) = self.people.iter_mut().find(|p| p.record == record) {
            p.lying = true;
        }
        true
    }
}

/// One of the world's people, as far as a save is concerned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    pub record: usize,
    pub at: crate::game::Point,
    pub move_to: crate::game::Point,
    pub food: i32,
    pub water: i32,
    pub health: i32,
    pub mind: crate::mind::Mind,
    pub bearing: crate::gait::Bearing,
    /// Which location they stand in: their record's, unless a boss let them
    /// go somewhere else (`Dismiss`) or they came back where they died.
    pub location: usize,
    pub experience: i32,
    /// What they carry and have drawn: a dismissed man keeps his things.
    pub pack: ItemBag,
    pub kit: crate::combat::Kit,
}

/// What somebody standing in the world left behind.
#[derive(Clone, Debug, PartialEq)]
pub struct Fall {
    pub record: usize,
    pub location: usize,
    /// Where they were standing, which is where `Die` drops their things:
    /// `Location.Items.DropPosition = Position`.
    pub at: crate::game::Point,
    pub class: CharClass,
    /// What they had. **`Revive` restores health and nothing else**, so
    /// food and water come back with them.
    pub needs: CharacterNeeds,
    pub dropped: ItemBag,
}

/// Resolve one record's `HireItems`, dropping any staple the item table does
/// not have. A mode without snakes cannot demand one.
fn accepted_by(record: &CharacterRecord, staples: &[Option<usize>; 3]) -> Vec<usize> {
    let mut accepts = Vec::new();
    if record.hire_item_id == 0 || record.hire_item_id < 4 {
        accepts.extend(staples.iter().flatten().copied());
    }
    if record.hire_item_id != 0 {
        accepts.push(record.hire_item_id as usize);
    }
    accepts
}

/// The side doing the asking: the character the player is playing.
///
/// Their pack is where the payment comes from and their experience is what
/// is weighed against the other person's, so the three travel together.
pub struct Boss<'a> {
    pub pack: &'a mut ItemBag,
    pub experience: i32,
    /// Which player, counted from zero. Somebody already working for this
    /// same player is not a stranger.
    pub player: usize,
}

/// Would this person join, and what would it cost? Changes nothing.
pub fn offer(
    people: &People,
    who: usize,
    party: &Party,
    boss: &Boss,
) -> Result<Offer, Refusal> {
    offer_with(people, who, party, boss, Terms::Legacy)
}

/// Whose rules a human's hire is under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terms {
    /// v0.9.1's: two thirds of the leader's experience, the payment fed,
    /// a floor of `2 - difficulty`.
    Legacy,
    /// v1.2's, by the preset's named rules (`MeetsRecruitmentExperience`,
    /// `InitializeRecruit`). The water drawn under `remaster_difficulty`
    /// comes from the game's generator, which [`hire_with`] is handed.
    V12 {
        recruitment: crate::rules::Recruitment,
        supplies: crate::rules::RecruitSupplies,
        difficulty: u16,
    },
}

/// [`offer`], under the given terms.
pub fn offer_with(
    people: &People,
    who: usize,
    party: &Party,
    boss: &Boss,
    terms: Terms,
) -> Result<Offer, Refusal> {
    // The employer is no longer asked for: **anybody who already works is
    // refused**, whoever they work for — see the note on
    // `Refusal::WorksForSomeoneElse`.
    let (leader_pack, leader_experience) = (&*boss.pack, boss.experience);
    let Some(person) = people.get(who) else { return Err(Refusal::NoSuchPerson) };
    if person.dead {
        // Not `NotOpenToWork`: the dead are not refusing, they are gone.
        return Err(Refusal::NoSuchPerson);
    }
    if person.hired {
        return Err(Refusal::WorksForSomeoneElse);
    }
    if !person.class.can_be_hired() {
        return Err(Refusal::NotOpenToWork);
    }
    // **Somebody already camped is not standing about looking for work** —
    // including your own. He stays on his feet so that he can be spoken to
    // and fought, and that is exactly what let an opponent hire its own
    // camper back: one man ended up a resident of two camps at once, in the
    // save, holding both places at the same time. Whoever he works for, he
    // already works.
    if person.employer.is_some() {
        return Err(Refusal::WorksForSomeoneElse);
    }
    // The leader counts towards the five.
    if party.len() + 1 >= MAX_GROUP {
        return Err(Refusal::GroupFull);
    }
    // `boss.Experience >= Parent.Experience * 0.66f`, in the original's own
    // arithmetic. Integer scaling would differ where the product lands
    // exactly on a whole number, so the float is kept.
    let experienced = match terms {
        Terms::Legacy => (leader_experience as f32) >= person.experience as f32 * 0.66,
        Terms::V12 { recruitment, .. } => {
            crate::v12_economy::can_recruit(recruitment, leader_experience, person.experience)
        }
    };
    if !experienced {
        return Err(Refusal::TooInexperienced);
    }
    let Some((pay_from_slot, pay_with)) = payment(leader_pack, &person.accepts) else {
        return Err(Refusal::NothingTheyAccept);
    };
    Ok(Offer { who, accepts: person.accepts.clone(), pay_from_slot, pay_with })
}

/// `Hire`'s own search: walk the accepted types in order and take the first
/// one the pack holds. Not the first matching slot — if somebody accepts a
/// snake or meat and the pack holds meat in slot 0 and a snake in slot 3,
/// the snake goes.
pub fn payment(pack: &ItemBag, accepts: &[usize]) -> Option<(usize, usize)> {
    for &kind in accepts {
        if let Some(slot) = (0..pack.len()).find(|&s| pack.get(s) == Some(kind)) {
            return Some((slot, kind));
        }
    }
    None
}

/// Take somebody on: **re-checks every condition against the model**, then
/// removes the payment and moves the person into the party in one step.
///
/// The returned value is the new companion's identity. On any refusal
/// nothing has moved — the payment is still in the pack and the person is
/// still standing where they were.
///
/// `Character.Hire`, in its order: the character leaves the map, joins the
/// group, the payment is found again and removed, and then it is *used* —
/// food and water separately, each capped, and each floored afterwards at
/// `2 - difficulty`.
pub fn hire(
    people: &mut People,
    who: usize,
    party: &mut Party,
    boss: &mut Boss,
    difficulty: i32,
    facts: &impl ItemFacts,
) -> Result<usize, Refusal> {
    hire_with(people, who, party, boss, difficulty, facts, Terms::Legacy, &mut crate::spawn::Rng::seeded(0))
}

/// [`hire`], under the given terms. `rng` is the game's generator, which
/// only v1.2's drawn water reads; a legacy hire draws nothing.
#[allow(clippy::too_many_arguments)]
pub fn hire_with(
    people: &mut People,
    who: usize,
    party: &mut Party,
    boss: &mut Boss,
    difficulty: i32,
    facts: &impl ItemFacts,
    terms: Terms,
    rng: &mut crate::spawn::Rng,
) -> Result<usize, Refusal> {
    let offer = offer_with(people, who, party, boss, terms)?;
    let player = boss.player;

    let outfit = people.outfit(player);
    let person = people.get_mut(who).ok_or(Refusal::NoSuchPerson)?;
    // Their index in `gam.dat`'s character table, which is the identity that
    // outlives the group.
    let record = person.record;
    let experience = person.experience;
    // `Hire` leaves them where they stood — `Path.MoveTo = Position` — and
    // `FellowerMind` walks them after the leader from there.
    let stood = person.at;
    let mut needs = person.needs;
    let pack = core::mem::replace(&mut person.pack, ItemBag::new(PACK_CAPACITY));
    person.hired = true;
    person.employer = Some(player);
    person.wear(outfit);

    boss.pack.remove(offer.pay_from_slot);

    match terms {
        Terms::Legacy => {
            // "if hire item is food or water then use it" — both, if it is both.
            let food = facts.food(offer.pay_with);
            if food != 0 {
                needs.food = (needs.food + food).min(needs.max_food);
            }
            let water = facts.water(offer.pay_with);
            if water != 0 {
                needs.water = (needs.water + water).min(needs.max_water);
            }
            // "prevent 0 food / 0 water situation depending on difficulty setting".
            // The floor is `2 - difficulty`, so it is two on easy and none on hard,
            // and it applies whether or not the payment was edible.
            let floor = 2 - difficulty;
            needs.food = needs.food.max(floor);
            needs.water = needs.water.max(floor);
        }
        Terms::V12 { supplies, difficulty, .. } => {
            let (food, water) = crate::v12_economy::recruit_supplies(
                supplies,
                difficulty,
                needs.food,
                needs.max_food,
                facts.food(offer.pay_with),
                rng,
            );
            needs.food = food;
            needs.water = water;
        }
    }

    let id = party.recruit();
    let joined = party.companions.last_mut().expect("recruit pushed one");
    // Where they came from, kept for as long as they are anybody's: it is
    // what says a dead companion was *this* person rather than a number.
    joined.origin = Some(record);
    joined.needs = needs;
    joined.experience = experience;
    joined.pack = pack;
    joined.at = stood;
    joined.move_to = stood;
    Ok(id)
}
