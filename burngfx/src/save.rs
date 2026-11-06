//! Writing a game down, and reading it back.
//!
//! **Our own format, versioned, and not the DOS game's.** The remaster is
//! not the DOS game's either: `Framework/SaveGame` writes a game name, a
//! version string, the players, and then its own serialised state. Its
//! `LoadGame` refuses anything whose name or version is not an exact match
//! and does not migrate. This does the same, for the same reason — a save
//! that half-loads is worse than one that does not load.
//!
//! It is text, one record per line. Not for elegance: a save you can read
//! and `diff` is worth more right now than a small one, and the first thing
//! anyone will want to know about a failed round trip is *which field*.
//!
//! # Reading is a transaction
//!
//! [`parse`] builds a whole [`Save`] or returns an error. It never writes
//! into the caller's world, so a truncated file, an unknown version or a
//! duplicate identity leaves the game running exactly as it was. **The
//! caller must keep that property**: replace the world with the parsed value
//! in one move, rather than filling the old one in field by field.
//!
//! # What a save is *tied* to
//!
//! The header carries the seed, the mode, the engine and the pack's
//! fingerprint, and a mismatch in any of them is a refusal. The fingerprint
//! is [`crate::pack::Pack::rules_fingerprint`]: the entries the world and
//! its rules are made from — `gam.dat`, the maps, the walk grids, the rooms,
//! the item and rule tables, v1.2's presets and `ai.txt`. A pack made again
//! with one more picture or line of text, or by a newer packer that adds
//! one, loads the saves of the old one; a different `gam.dat` or table does
//! not. (It was once every byte of the pack, so that a `demo` save would not
//! load under `full` and any repack lost every save.)
//!
//! The seed is in the header because the world's own generator still runs
//! after the game starts — see [`Save::seed`].

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::game::{
    Camp, CharacterNeeds, Companion, Engine, ItemBag, Journey, Mode, Place, Point, WaterSource,
};

/// What every save says on its first line, so a file that is not one is
/// rejected before anything else is read.
pub const MAGIC: &str = "burntime-rs-save";

/// Bumped whenever the meaning of any line changes. There is no migration:
/// a save from another version is refused, the way `LoadGame` refuses one.
///
/// 12 gives a machine player a group of its own rather than a count of
/// followers, written in the same `companion` lines the player's group uses,
/// and carries a scenario's shortened goal. **A camper's employer is not
/// written**: it follows from the camp they are in, and one fact in two
/// places is one fact too many.
///
/// 11 wrote down the machine players: where each stands, what it is
/// travelling on, which mode it is in, how many turns of its interval are
/// left, what it is carrying and how many people walk with it — and whether
/// the game has been decided. **All of it is a decision in flight.** A game
/// resumed without the mode and the countdown would hand an opponent a free
/// re-think, and one resumed without the decision would announce a victory
/// that had already been announced.
///
/// 10 writes down the difficulty. It used to come from `gam.dat` and could
/// not be anything else; now the start screen chooses it, and it decides how
/// much the world generates, what hiring costs and how hard a blow lands. A
/// game resumed at another difficulty would not be the game that was saved.
///
/// 9 writes down single mode — whether the leader is acting alone with the
/// group left standing. `Player.SingleMode` is saved upstream, and a game
/// resumed with the group suddenly following again would not be the game
/// that was saved.
///
/// 8 gives every companion — walking with the player or camped — a position
/// of their own, where they are walking to, and their follower's draw
/// counter. The group used to be one point; now who is near enough to join a
/// fight depends on where each of them is, and a load that put them all on
/// the leader would change that.
///
/// 7 replaced three lists of scraps about the world's people — what they
/// foraged, who was wounded, which creature had already bitten — with one
/// line each for whoever is no longer as the world built them: where they
/// are, where they are walking, what they carry, and what their mind is
/// doing. The minds are the reason: people move now, and a resumed game
/// that put everybody back at their record's spot would be a different
/// world.
///
/// 6 wrote down what the world's free people foraged, which creatures
/// had already struck, and the session's answers to the contested combat
/// rules — see
/// [`crate::combat::Contested`]. They decide what a fight does, so a file
/// that did not carry them could be resumed under different rules than it
/// was made under, and nothing would say so.
///
/// 5 gave every bag a length and the rounds left in each slot, so that a
/// half-fired rifle is still half-fired after a load, and gave each
/// character the weapon and armour they have drawn. It also records what a
/// respawn restores the health to, which is 31 for a dog and not 100.
///
/// 4 added the respawn schedule. Without it a load would make every death
/// final, which is not what the game does.
///
/// 3 added a companion's origin — which character in `gam.dat` they are.
/// A save without it cannot say who a companion was, and being able to say
/// so is the whole point of keeping it.
///
/// 2 added the traders. Their stock and their walk counter are state that
/// no amount of replaying can recover, so a version 1 save cannot describe
/// a world with traders in it and is refused rather than loaded short.
/// 13 added the turn: `World.Time` — what is left of the day's clock and
/// how long a day is (`roundtime`) — and two things v0.9.1 keeps on the
/// player and saves with it: `previousLocation`, which `CanTravel` reads,
/// and the map's INFO/TRAVEL mode. A resumed turn is the same turn, with the
/// same time left in it.
/// 14: a thing on the ground keeps the rounds left in it.
/// 15: what gas or radiation has spent and not yet made a whole point of,
/// for the player and each companion. v0.9.1's health is a float and a save
/// keeps all of it; without this line a save made in a danger and loaded
/// again lost up to a point that an unbroken run would have kept.
/// 16: the machine players' `previousLocation`, and Classic's computer
/// player's own state — its mode, countdown, last heading and item pool.
/// 19: who each human has selected (`Player.selectedCharacter`): the
/// leader, a companion by identity, or a camper of theirs by record.
/// 18: a human is `alive`, `out` or `gone`; somebody standing in the world says where and what they carry — a
/// companion dismissed, or a camp given up by a dead player, stays where the
/// group was, with his pack.
/// 17: two humans. Who plays each human seat — the start screen's name, face
/// and colour, which no earlier version wrote down at all — which of them is
/// playing now, everything about the other one under `bench`, and who has
/// already had their turn today, so that a game saved in the second human's
/// turn resumes in that turn and not at the first human's.
/// 20: a leader who has come back from the dead stands in his location for
/// good, one of its people (`stationed LOCATION SEAT`).
/// 21: the game's identity. A legacy game writes its `mode` and `engine`
/// lines as before; a v1.2 game writes `engine v12`, `preset` and `ai`.
/// Version 20 files are still read, as legacy games — they cannot be
/// anything else.
/// 22: a v1.2 game's generator (`rng`), its whole state — written from 22
/// to 24, dropped in 25 (see there). A legacy game writes no such line.
/// 23: two branches each wrote a version 23, both as new lines and words
/// at a line's end, and either's file is read as the one it is: the other's
/// additions are simply absent.
/// - v1.2's day (rules audit 1.1, 1.10, 1.11, 2.4, 2.9, 2.13, 2.14): what
///   gas or radiation has spent of a camper's health and not yet made a
///   whole point of (`exposed`), where each human leader last stood in a
///   location (`stood`), the arrival order of a location's people
///   (`entered`, `lying`), and a machine leader's chosen weapon.
/// - How a v1.2 game's people move (see [`crate::gait`]): a `motion` line
///   with the leader's fraction of a pixel and the creatures' bite
///   cooldown; on each `standing` line the held target of a creature, the
///   fraction of a pixel, the proximity pause and whether the player
///   controls him; on each companion line the fraction and `FellowerMind`'s
///   slot radius and start delays. v1.2 itself keeps all of it but the
///   controlled flag out of its save.
///
/// 24: both of version 23's sets together. Nothing new is written; a file
/// of 22 or either 23 reads as having none of what it lacks.
///
/// 25: **a v1.2 save holds what v1.2's holds** ([`Save::forget_unserialized`]). No generator: v1.2's is the
/// process's static `Random`, never saved, and a load continues the
/// process's own. No `motion` or `flight` line, and on the `standing` and
/// companion lines the fields v1.2 keeps out of its save are written as
/// the value a v1.2 load gives them, nought. The Modern opponent writes only
/// `settings`, `target`, `reserve` and `kit` ([`crate::modern_ai::state`]).
/// 26: **a v1.2 game's health is v1.2's float**: `needs`'
/// health is the getter, `(int)h`, and the `exposure`/`exposed` numbers the
/// float above it, plus one [`crate::danger::UNIT`] for one alive below a
/// point ([`crate::danger::Exposure`]). A v1.2 file of 25 or before is
/// turned into that on reading; a legacy file is as in 25.
/// A `path WHO MX MY AX AY` line keeps one of the group's `ComplexPath`s
/// whose `adjustedMove` is not its `move` (`AX AY` the floats' bits; v1.2
/// serializes both, see [`crate::gait::PathRecord`]).
/// A `standing` line may end in the word `tame`: a dog or mutant some boss
/// let go, who has a `SimpleMind` for good (v1.2 saves the mind; RULE-67);
/// a file without it reads as none tame.
/// A file of 22 to 24 is read as before and then forgotten the same way:
/// its `rng`, `motion`, `flight` lines and the Modern records v1.2 does not
/// save are checked and dropped. A legacy game is written as in 24.
pub const VERSION: u32 = 26;

/// The versions [`parse`] accepts: this one, 24, 23, 22, 21, and 20 read as
/// a legacy game.
pub const READS: [u32; 7] = [20, 21, 22, 23, 24, 25, 26];

/// Limits, so a corrupt or hostile file cannot make us allocate without
/// bound. They are far above anything the classic data produces — 37
/// locations, 309 characters — and are here to fail loudly rather than to
/// be tuned.
pub mod limits {
    /// The whole file. A real save is a few tens of kilobytes.
    pub const BYTES: usize = 4 * 1024 * 1024;
    pub const LINES: usize = 100_000;
    /// Any one collection: rooms, ground items, camps, springs, people.
    pub const ENTRIES: usize = 20_000;
    /// Companions in the party and in all camps together.
    pub const PEOPLE: usize = 2_000;
    /// A single bag. `PACK_CAPACITY` is 6 and a room's is smaller still.
    pub const BAG: usize = 64;
    pub const LOCATION: usize = 1_000;
    /// The four player records `gam.dat` begins with.
    pub const PLAYERS: usize = 4;
}

/// Why a file is not a save we can use.
///
/// Every one of these leaves the caller's world untouched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// Not one of ours at all.
    NotASave,
    /// Ours, but written by a different version. `LoadGame`'s rule: refuse,
    /// do not migrate.
    WrongVersion { found: u32, want: u32 },
    /// Made against a different pack — see the note on the module.
    WrongPack { found: u64, want: u64 },
    /// Made under different rules.
    WrongRules,
    /// Made under a different answer to one of the contested combat rules,
    /// and this run was told on the command line which answer to use.
    ///
    /// **Never adopted silently, and never overridden silently.** A save
    /// made while a swung axe was being put away describes a fight fought
    /// under that rule; playing it on under the other one would make the
    /// resumed run differ from the uninterrupted one, which is the single
    /// thing a save has to avoid. When the run says nothing, the file's
    /// answer is adopted instead of this being raised.
    ContestedRules { found: crate::combat::Contested, want: crate::combat::Contested },
    /// A line we do not understand, or one that is missing.
    BadLine(String),
    /// A number that is not one, or is out of range.
    BadValue(String),
    /// Two companions with the same identity, or a room listed twice.
    Duplicate(String),
    /// Every part is well formed but they do not describe one world: a
    /// journey that disagrees with where the player is, a camp with people
    /// and no owner, a character hired twice.
    Inconsistent(String),
    /// A count or a size beyond [`limits`].
    TooBig(String),
    /// The file ends before the save does. **The commonest real failure**,
    /// and the one that most needs to leave the old world alone.
    Truncated,
}

impl SaveError {
    /// The refusal in words for the player: which file it is and why it
    /// cannot be continued, not the variant's fields.
    pub fn explain(&self) -> String {
        use alloc::format;
        match self {
            SaveError::NotASave => "not a Burntime save".into(),
            SaveError::WrongVersion { found, want } => format!(
                "written in save format {found}, and this game reads format {want}; saves from test builds before the first release cannot be continued"
            ),
            SaveError::WrongPack { found, want } => format!(
                "made with other game data or by a test build before the first release (its data {found:016x}, this game's {want:016x}); it cannot be continued with this game"
            ),
            SaveError::WrongRules => "made under other rules than the ones it names".into(),
            SaveError::ContestedRules { .. } => "made under the other answer to a contested combat rule".into(),
            SaveError::Truncated => "the file ends before the save does (damaged or cut short)".into(),
            other => format!("damaged or not readable ({other:?})"),
        }
    }
}

/// A camped person, with the location they are camped at.
#[derive(Clone, Debug, PartialEq)]
pub struct Resident {
    pub location: usize,
    pub who: Companion,
}

/// `Player.selectedCharacter`: whom the player's clicks move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    Leader,
    /// A member of the group, by identity.
    Companion(usize),
    /// One of the player's campers, by `gam.dat` record.
    Camper(usize),
}

/// Who plays a human seat. `GameCreation` gives player *i* the name, face and
/// colour of start-screen slot *i*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Human {
    pub seat: usize,
    pub name: String,
    pub face: i32,
    /// The colour he flies (v1.3's four; a save from before them says green
    /// or red).
    pub color: crate::player_colors::PlayerColor,
    /// `Player.IsDead`: out of the game. Set by `World.Turn` for a player
    /// whose character died, or by `LocationScene` for the active player
    /// whose character dies in front of him — and then `World.Turn` does
    /// not dismiss his people.
    pub out: bool,
    /// The server has given the `DeathNews` for him (`GameClient.Die`).
    pub gone: bool,
}

/// **One human player, whole**: where they are, what they carry, who walks
/// with them, and the parts of `Player` v0.9.1 keeps for them between turns
/// — `previousLocation`, `InfoMode`, `FightMode`, `SingleMode`. The one
/// playing now is the flat part of [`Save`]; the other is [`Save::bench`].
#[derive(Clone, Debug, PartialEq)]
pub struct Bench {
    pub seat: usize,
    pub at_location: usize,
    pub place: Place,
    pub position: Point,
    pub return_position: Option<Point>,
    /// Where the leader stands in a location while the player is on the
    /// map (v1.2's `Character.Position`): where he last stood there, or the
    /// destination's entry once he has set out (`Player.Travel`). `None`
    /// is the entry of where he counts as being.
    pub stood: Option<Point>,
    pub needs: CharacterNeeds,
    pub experience: i32,
    pub carried: ItemBag,
    pub kit: crate::combat::Kit,
    pub exposure: crate::danger::Exposure,
    pub fight_mode: bool,
    pub single_mode: bool,
    pub previous_location: Option<usize>,
    pub info_mode: bool,
    pub talked: Vec<usize>,
    pub journey: Option<Journey>,
    pub companions: Vec<Companion>,
    /// Their own party's next identity.
    pub next_id: usize,
    pub selected: Selection,
}

/// Everything a game is, and nothing that can be worked out again.
///
/// What is deliberately *not* here: which panel is open, what the camera is
/// looking at, the path being walked, whose pack the left-hand grid shows.
/// Those are answers to "what is on screen", and a load answers that
/// afresh.
#[derive(Clone, Debug, PartialEq)]
pub struct Save {
    /// The seed the game was made from. **A v1.2 game's numbers come from
    /// the process's one generator**, which no save carries (version 25):
    /// for it the seed is provenance. What follows is about a legacy game.
    ///
    /// Two things draw random numbers after a legacy game starts, and both
    /// are rebuilt rather than carried:
    ///
    /// * the per-day generator that finds a spot on the ground for a
    ///   produced item that will not fit in a room, built from
    ///   `seed ^ day`;
    /// * each trader's own stream, built by [`crate::trader::rng_for`] from
    ///   the seed, the day and **their record index in `gam.dat`** — an
    ///   identity that does not move when the packer's profile changes.
    ///
    /// So `(seed, day)` plus each actor's own saved state reproduces every
    /// draw, and there is no sequence position to preserve. **That is a
    /// property of how those two are written, not a general fact**, and it
    /// stopped being true of the first one alone the day traders arrived —
    /// which is why `the_generator_carries_no_state` and
    /// `the_stream_is_a_function_of_seed_day_and_record` both exist. A
    /// third actor that threads a generator through the turn would need its
    /// state here, and those tests are what should fail first.
    pub seed: u64,
    /// The playing human's leader's fraction of a pixel and the creature
    /// bite cooldown — see [`crate::gait::Motion`]. Nought in a legacy game,
    /// and in a v1.2 game once saved (`[NonSerialized]` upstream).
    pub motion: crate::gait::Motion,
    /// Flights under way (version 23), see [`crate::gait::Flight`].
    pub flights: Vec<crate::gait::Flight>,
    /// The group's paths whose `adjustedMove` is not their `move` (v1.2
    /// games), see [`crate::gait::PathRecord`].
    pub paths: Vec<crate::gait::PathRecord>,
    /// What game this is: the legacy engine and mode, or a v1.2 preset and
    /// its opponent. See [`crate::rules::Identity`].
    pub identity: crate::rules::Identity,
    /// The pack the world was laid out from.
    pub pack: u64,
    /// The layout's own fingerprint, so a load can say whether the world it
    /// restored is the world that was saved.
    pub world: u64,
    /// Which section of `gamesettings.txt` this game is played by — 0, 1 or
    /// 2. Chosen on the start screen, and `gam.dat`'s own value when nobody
    /// chose.
    pub difficulty: u16,
    /// The places v1.3's `LocationPopulationCreator` peopled when the game
    /// was created (New Village: its residents, `populated 37`). Their
    /// residents are records after `gam.dat`'s, rebuilt on load only where
    /// this says so; a game from before v1.3, or one made from an older
    /// save, has none. Written only when
    /// not empty, so other saves are as before.
    pub populated: alloc::vec::Vec<usize>,

    pub day: u16,
    /// Which location the player counts as being at, on the map.
    pub at_location: usize,
    pub place: Place,
    pub position: Point,
    pub return_position: Option<Point>,
    /// See [`Bench::stood`].
    pub stood: Option<Point>,
    pub needs: CharacterNeeds,
    pub experience: i32,
    pub carried: ItemBag,
    /// What the player has drawn out of that pack. See
    /// [`crate::combat::Kit`]: which weapon a character ends up holding
    /// depends on the order they were drawn in, so it is state and not a
    /// view of the pack.
    pub kit: crate::combat::Kit,
    /// The player's unpaid part of a danger's damage — see
    /// [`crate::danger::Exposure`]. Each companion carries their own.
    pub exposure: crate::danger::Exposure,
    /// Speak or fight: `Player.FightMode`, which decides what a click on
    /// somebody does. The original keeps it on the player, so it survives a
    /// save the way the rest of the player does.
    pub fight_mode: bool,
    /// Single mode: the leader acting alone, the group standing.
    pub single_mode: bool,
    /// `World.Time`, in milliseconds left of `round_ms`.
    pub time_left_ms: u32,
    /// `roundtime`, the length of a day's turn, in milliseconds.
    pub round_ms: u32,
    /// `Player.previousLocation`.
    pub previous_location: Option<usize>,
    /// `Player.InfoMode`.
    pub info_mode: bool,
    /// Everybody the player has spoken to — `Dialog.talkedPlayer`, which
    /// turns a first greeting into a talk. Record indices.
    pub talked: Vec<usize>,
    /// **Which answers this session gives to the contested rules.**
    ///
    /// Not a fact about the world but a configuration of the rules it is
    /// played under, and kept for the same reason `mode` and `engine` are:
    /// a fight resumed under a different answer is a different fight.
    pub contested: crate::combat::Contested,
    /// **Everyone in the world who is no longer as the world built them**:
    /// where they have walked, what they have foraged, what a fight left
    /// them at, and what their mind is doing.
    ///
    /// The mind is the part that has to be here rather than left to start
    /// again. Upstream's is `[NonSerialized]` and nobody notices, because
    /// upstream's timers restart on their own; ours decides when a creature
    /// may next come at the player, and a load that forgot it would hand
    /// out a free attack. `wait_after_attack` is the one that matters;
    /// `rest`, `chasing` and the draw counter are here so that a resumed
    /// walk is the same walk.
    pub standing: Vec<crate::hire::Standing>,
    /// **A journey survives a load; a walk does not.** They are different
    /// mechanisms: a journey is days on a road and belongs to the world,
    /// while a walk is a path across a location and belongs to the screen.
    pub journey: Option<Journey>,

    pub companions: Vec<Companion>,
    /// The next identity to hand out. Saved rather than derived from the
    /// largest id in use: someone dismissed must not have their number
    /// handed to the next person hired.
    pub next_id: usize,
    /// Which character records have been hired, and by whom.
    pub hired: Vec<(usize, usize)>,
    /// Everybody waiting to come back, with their countdown and where.
    /// **Not derivable**: the schedule is the only record that a death was
    /// not the end.
    pub respawns: Vec<crate::game::Pending>,
    /// Which records died. **Not derivable from anything else**: the
    /// companion is gone from the party, and the record is neither hired
    /// nor standing about, so without this a load would put them back on
    /// the map alive and hireable.
    pub fallen: Vec<usize>,
    /// v1.2: people who came back as somebody else — `(record, name, face,
    /// body)`. One line each, `identity`, and only for those.
    pub identities: Vec<(usize, u16, u8, u16)>,
    /// v1.2's order of a location's people (`Location.Characters`): whoever
    /// has come into a list since the world was built, `(record, entered)`
    /// — see [`crate::hire::Person::entered`]. One `entered` line each.
    pub entries: Vec<(usize, u32)>,
    /// The dead still lying in a list, `(record, location)` (`lying RECORD
    /// LOCATION`) — see [`crate::hire::Person::lying`].
    pub lying: Vec<(usize, usize)>,

    /// Every machine player, with its decisions in flight. See
    /// [`crate::rival::Rival`].
    pub rivals: Vec<crate::rival::Rival>,
    /// How the game ended, once it has. `None` while it is still being
    /// played — and a game that has been decided says so rather than
    /// deciding again on the next day.
    pub decided: Option<crate::rivals::Decision>,
    /// **A scenario's shortened goal**, counted in locations: the game is
    /// won by holding this many of the places the victory condition wants,
    /// rather than all of them. `None` is the real rule.
    ///
    /// It is saved because it is part of what game this is. A run resumed
    /// without it would be a different game — a longer one — and would
    /// silently stop agreeing with the run it continues.
    pub goals: Option<usize>,
    /// v1.2's `knownConstructionResults`: what the construction dialog has
    /// offered to make in this game, by item id — an item's tooltip names
    /// those results and only counts the others. Empty for a legacy game.
    pub known: Vec<String>,
    /// A v1.2 game's machine players' faces, `(seat, face)`: drawn on the
    /// game's first update ([`crate::v12_economy::machine_faces`]) and kept.
    /// Empty for a legacy game, and in a v1.2 save made before version 22 —
    /// which then draws them, as v1.2 does for an old save.
    pub faces: Vec<(usize, i32)>,
    /// v1.2's `Player.visitedLocations`, for the humans: the places each
    /// seat has stood at, by location. The map's labels show what a player
    /// has seen. Empty for a legacy game, and for a v1.2 save made before
    /// it was kept (the game then rebuilds it as v1.2 migrates one).
    pub visited: Vec<(usize, Vec<usize>)>,
    /// v1.2's `ClassicGame.CheatsEnabled`: the `/petko` dialog has been
    /// answered in this game (see [`crate::cheat`]). One mark for the game,
    /// not per player; a `[Serializable]` auto-property, so v1.2's save
    /// keeps it and nothing clears it. Written as `cheats` only when set,
    /// so a game without it writes what it always did.
    pub cheats: bool,

    pub camps: Vec<(usize, Camp)>,
    pub rooms: Vec<(usize, usize, ItemBag)>,
    pub ground: Vec<crate::spawn::Dropped>,
    pub springs: Vec<(usize, WaterSource)>,
    /// Every trader: what they carry, where they are, and how far through
    /// their two-turn walk. The rest of them — their home, their name —
    /// comes back from `gam.dat`, but these three do not.
    pub traders: Vec<crate::trader::Trader>,

    /// Which seat the flat player fields above belong to: the human whose
    /// turn it is.
    pub seat: usize,
    /// Every human seat, in seat order. Empty in a run that never had a
    /// start screen and was given no name.
    pub humans: Vec<Human>,
    /// v1.3: every seat's colour (`PlayerColorSetup.Assign` of the start
    /// screen's two), a `colours` line. `None` in a save from before v1.3's
    /// colours: the game then gives seats 0 and 1 the humans' green and red
    /// and keeps the old look for seats 2 and 3.
    pub colours: Option<[crate::player_colors::PlayerColor; 4]>,
    /// The other human, when there are two.
    pub bench: Option<Bench>,
    /// Seats that have finished today (`Player.IsFinished`).
    pub finished: Vec<usize>,
    /// Whom the player playing has selected.
    pub selected: Selection,
}

impl Save {
    /// The player whose turn it is, in the same shape as the bench.
    pub fn player(&self) -> Bench {
        Bench {
            seat: self.seat,
            at_location: self.at_location,
            place: self.place,
            position: self.position,
            return_position: self.return_position,
            stood: self.stood,
            needs: self.needs,
            experience: self.experience,
            carried: self.carried.clone(),
            kit: self.kit,
            exposure: self.exposure,
            fight_mode: self.fight_mode,
            single_mode: self.single_mode,
            previous_location: self.previous_location,
            info_mode: self.info_mode,
            talked: self.talked.clone(),
            journey: self.journey,
            companions: self.companions.clone(),
            next_id: self.next_id,
            selected: self.selected,
        }
    }

    /// **What a v1.2 load leaves of this game**: every field
    /// v1.2 keeps out of its save at the value its `Deserialize` gives it —
    /// the CLR default — and the rest as it is. The writer saves a v1.2 game
    /// through this, and the parser applies it to an older file, so a v1.2
    /// save and a v1.2 load agree. Nothing for a legacy game.
    ///
    /// * the bite cooldown `creatureAttackCooldownRemaining`
    ///   (`ClassicGame.cs:21-22`) and the leader's `precisePosition`
    ///   (`PathState.cs:17-22`): [`Save::motion`] nought;
    /// * flights `fleeTimeRemaining`, `fleeDestination`, `fleeFromCharacter`
    ///   (`Character.cs:62-67`): none;
    /// * each standing figure: `CreatureMind.waitAfterAttack`, `tryToAttack`,
    ///   `attack` (`CreatureMind.cs:15-19`) nought — `takeSomeRest` and
    ///   `first` (`:12-13`) are serialized and kept; `precisePosition`,
    ///   `proximityPauseRemaining`, `wasInTalkingDistance`
    ///   (`Character.cs:56-61`) nought; the player's control (a serialized
    ///   `PlayerControlledMind`) kept;
    /// * each companion, of every group and camp: `precisePosition` and
    ///   every `FellowerMind` field (`FellowerMind.cs:20-37`) nought, so the
    ///   formation radius is drawn again, `Next(14, 21)`, the first time the
    ///   mind needs it;
    /// * the Modern opponents: [`crate::modern_ai::state::ModernAi::forget_unserialized`].
    ///
    /// The port's own legacy counter `Mind::draws` is no v1.2 field and is
    /// kept.
    /// The carries as read (the file's tokens, in `carried`) into what they
    /// mean in this game. Legacy: as written, under a whole point. v1.2 from
    /// version 26: the float's fraction and lift. v1.2 before 26 wrote the
    /// legacy meaning — health one high while a fraction was carried — and
    /// is turned into v1.2's: `h = H - c`, the getter `H - 1`, `1 - c` above.
    fn read_fractions(&mut self, version: u32) -> Result<(), SaveError> {
        use crate::danger::{from_v12_token, Exposure, UNIT};
        let legacy = self.identity.is_legacy();
        let fix = |health: &mut i32, e: &mut Exposure| -> Result<(), SaveError> {
            let raw = e.carried;
            if legacy {
                return if raw < UNIT { Ok(()) } else { Err(SaveError::BadValue("exposure".into())) };
            }
            *e = if version >= 26 {
                from_v12_token(raw)
            } else if raw >= UNIT {
                return Err(SaveError::BadValue("exposure".into()));
            } else if raw == 0 {
                Exposure::default()
            } else if *health > 1 {
                *health -= 1;
                Exposure { carried: UNIT - raw, lifted: false }
            } else {
                Exposure { carried: UNIT - raw, lifted: *health == 1 }
            };
            Ok(())
        };
        fix(&mut self.needs.health, &mut self.exposure)?;
        for c in &mut self.companions {
            fix(&mut c.needs.health, &mut c.exposure)?;
        }
        if let Some(b) = self.bench.as_mut() {
            fix(&mut b.needs.health, &mut b.exposure)?;
            for c in &mut b.companions {
                fix(&mut c.needs.health, &mut c.exposure)?;
            }
        }
        for (_, camp) in &mut self.camps {
            for r in &mut camp.residents {
                fix(&mut r.needs.health, &mut r.exposure)?;
            }
        }
        Ok(())
    }

    pub fn forget_unserialized(&mut self) {
        if self.identity.is_legacy() {
            return;
        }
        self.motion = crate::gait::Motion::default();
        self.flights.clear();
        for p in &mut self.standing {
            p.mind.wait_after_attack = 0;
            p.mind.chasing = 0;
            p.mind.charging = false;
            p.mind.target = 0;
            let (controlled, tame) = (p.bearing.controlled, p.bearing.tame);
            p.bearing = crate::gait::Bearing { controlled, tame, ..Default::default() };
        }
        let forget = |c: &mut Companion| {
            c.stride = crate::gait::Stride::default();
            c.fellower = crate::mind::Fellower::default();
        };
        self.companions.iter_mut().for_each(forget);
        if let Some(b) = self.bench.as_mut() {
            b.companions.iter_mut().for_each(forget);
        }
        for (_, camp) in &mut self.camps {
            camp.residents.iter_mut().for_each(forget);
        }
        for r in &mut self.rivals {
            r.party.companions.iter_mut().for_each(forget);
            if let Some(ai) = r.modern.as_mut() {
                ai.forget_unserialized();
            }
        }
    }

    /// Dead is not a field. It is `health <= 0`, and asking it here means
    /// the two can never disagree — which they could if a save carried a
    /// flag alongside the number it is derived from.
    pub fn dead(&self) -> bool {
        self.needs.health <= 0
    }
}

// -- writing -----------------------------------------------------------------

/// Write a save out. Never fails: everything in a [`Save`] can be written.
pub fn write(save: &Save) -> String {
    write_stamped(save, None)
}

/// When a save was made, as dos.library keeps time (`struct DateStamp`):
/// days since 1 January 1978 and the minute of that day, local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    pub days: i32,
    pub minute: i32,
}

/// [`write`], with the moment it was made, for the Saves page's order and
/// its `MMM-dd HH:mm` (v1.2's `datetime` hint). Header-only, like `held`
/// and `player`: nothing in the game is restored from them.
pub fn write_stamped(save: &Save, stamp: Option<Stamp>) -> String {
    // A v1.2 game is written as v1.2 writes it: without what its load
    // would not have anyway.
    let kept;
    let save = if save.identity.is_legacy() {
        save
    } else {
        kept = {
            let mut s = save.clone();
            s.forget_unserialized();
            s
        };
        &kept
    };
    let mut out = String::new();
    let _ = writeln!(out, "{MAGIC} {VERSION}");
    let _ = writeln!(out, "seed {}", save.seed);
    match save.identity {
        crate::rules::Identity::Legacy { engine, mode } => {
            let _ = writeln!(out, "mode {}", mode.name());
            let _ = writeln!(out, "engine {}", engine.name());
        }
        crate::rules::Identity::V12 { preset, ai } => {
            let _ = writeln!(out, "engine {}", crate::rules::Identity::V12_ENGINE);
            let _ = writeln!(out, "preset {}", preset.name());
            let _ = writeln!(out, "ai {}", ai.name());
        }
    }
    let _ = writeln!(out, "pack {:016x}", save.pack);
    let _ = writeln!(out, "world {:016x}", save.world);
    let _ = writeln!(out, "difficulty {}", save.difficulty);
    for l in &save.populated {
        let _ = writeln!(out, "populated {l}");
    }
    // Before the day, so the header reads it: how many places the player
    // holds, for the Saves page's "3 camps in 12 days". Not read back — the
    // camps below are the truth.
    let held = save.camps.iter().filter(|(_, c)| c.owner == Some(save.seat)).count();
    let _ = writeln!(out, "held {held}");
    // Whose game it is, for the Saves page's row (v1.2's `player` hint):
    // the name of the seat that saved, the rest of the line.
    if let Some(h) = save.humans.iter().find(|h| h.seat == save.seat) {
        let _ = writeln!(out, "player {}", h.name);
    }
    if let Some(t) = stamp {
        let _ = writeln!(out, "saved {} {}", t.days, t.minute);
    }
    let _ = writeln!(out, "day {}", save.day);
    let _ = writeln!(out, "time {} {}", save.time_left_ms, save.round_ms);
    let _ = writeln!(out, "seat {}", save.seat);
    // The name last and counted in bytes, so that whatever was typed —
    // spaces included — comes back as it was.
    if let Some(c) = save.colours {
        let _ = writeln!(out, "colours {} {} {} {}", c[0].word(), c[1].word(), c[2].word(), c[3].word());
    }
    for h in &save.humans {
        let _ = writeln!(
            out,
            "human {} {} {} {} {} {}",
            h.seat,
            h.color.word(),
            h.face,
            match (h.out, h.gone) {
                (false, _) => "alive",
                (true, false) => "out",
                (true, true) => "gone",
            },
            h.name.len(),
            h.name
        );
    }
    let mut line = String::from("finished");
    for s in &save.finished {
        let _ = write!(line, " {s}");
    }
    let _ = writeln!(out, "{line}");
    write_player(&mut out, "", &save.player(), !save.identity.is_legacy());
    // Written by name rather than by position: this is the line somebody
    // will read when a resumed fight surprises them.
    let yes_no = |on: bool| if on { "yes" } else { "no" };
    let _ = writeln!(
        out,
        "contested melee-spends-a-round={} best-armour-wins={}",
        yes_no(save.contested.melee_spends_a_round),
        yes_no(save.contested.best_armour_wins)
    );
    for (seat, face) in &save.faces {
        let _ = writeln!(out, "face {seat} {face}");
    }
    if save.motion != crate::gait::Motion::default() {
        let m = save.motion;
        let _ = writeln!(out, "motion {} {} {}", m.leader.0, m.leader.1, m.cooldown);
    }
    for p in &save.paths {
        let _ = writeln!(out, "path {} {} {} {:08x} {:08x}", p.who, p.move_to.x, p.move_to.y, p.adjusted.0.to_bits(), p.adjusted.1.to_bits());
    }
    for f in &save.flights {
        let _ = writeln!(out, "flight {} {} {} {} {}", f.who, f.from, f.to.x, f.to.y, f.left);
    }

    for rival in &save.rivals {
        write_rival(&mut out, rival);
    }
    // v1.2's Amiga opponent: `amiga PLAYER DIFFICULTY MODE HEADED WAIT
    // LAST-ARRIVAL PENDING`.
    for rival in &save.rivals {
        if let Some(a) = &rival.amiga {
            let _ = writeln!(
                out,
                "amiga {} {} {} {} {} {} {}",
                rival.player,
                a.difficulty,
                a.mode.name(),
                a.headed.map_or(String::from("none"), |h| h.to_string()),
                a.wait,
                a.last_arrival_day,
                if a.maintenance_pending { "pending" } else { "done" }
            );
        }
    }
    // v1.2's Modern opponent: `modern PLAYER WHAT …`, see
    // `modern_ai::state`.
    for rival in &save.rivals {
        if let Some(m) = &rival.modern {
            m.write(rival.player, &mut out);
        }
    }
    if let Some(goals) = save.goals {
        let _ = writeln!(out, "goals {goals}");
    }
    if !save.known.is_empty() {
        let _ = write!(out, "known");
        for k in &save.known {
            let _ = write!(out, " {k}");
        }
        let _ = writeln!(out);
    }
    if save.cheats {
        let _ = writeln!(out, "cheats");
    }
    for (seat, places) in &save.visited {
        let _ = write!(out, "visited {seat}");
        for p in places {
            let _ = write!(out, " {p}");
        }
        let _ = writeln!(out);
    }
    if let Some(decided) = save.decided {
        let _ = writeln!(
            out,
            "decided {} {}",
            match decided {
                crate::rivals::Decision::Won(_) => "won",
                crate::rivals::Decision::Lost(_) => "lost",
            },
            match decided {
                crate::rivals::Decision::Won(p) | crate::rivals::Decision::Lost(p) => p,
            }
        );
    }
    if let Some(b) = &save.bench {
        write_player(&mut out, "bench ", b, !save.identity.is_legacy());
    }

    for (record, player) in &save.hired {
        let _ = writeln!(out, "hired {record} {player}");
    }
    for (record, name, face, body) in &save.identities {
        let _ = writeln!(out, "identity {record} {name} {face} {body}");
    }
    for record in &save.fallen {
        let _ = writeln!(out, "fallen {record}");
    }
    for (record, location) in &save.lying {
        let _ = writeln!(out, "lying {record} {location}");
    }
    for (record, entered) in &save.entries {
        let _ = writeln!(out, "entered {record} {entered}");
    }
    for p in &save.standing {
        let _ = write!(
            out,
            "standing {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            p.record,
            p.at.x,
            p.at.y,
            p.move_to.x,
            p.move_to.y,
            p.food,
            p.water,
            p.health,
            p.mind.rest,
            if p.mind.first { 1 } else { 0 },
            p.mind.wait_after_attack,
            p.mind.chasing,
            if p.mind.charging { 1 } else { 0 },
            p.mind.draws
        );
        // Where they stand and what they carry, for anybody a boss let go.
        let _ = write!(out, " {} {}", p.location, p.experience);
        write_bag_tokens(&mut out, &p.pack);
        write_kit(&mut out, &p.kit);
        let b = &p.bearing;
        let (fx, fy) = b.stride.carry(p.at);
        let _ = write!(out, " {} {} {} {} {} {}", p.mind.target, fx, fy, b.pause, b.near as u8, b.controlled as u8);
        if b.tame {
            let _ = write!(out, " tame");
        }
        let _ = writeln!(out);
    }
    for w in &save.respawns {
        let _ = writeln!(
            out,
            "respawn {} {} {} {} {} {}",
            w.record, w.location, w.days, w.food, w.water, w.revive_health
        );
    }

    for (location, camp) in &save.camps {
        let _ = write!(out, "camp {location}");
        match camp.owner {
            Some(p) => {
                let _ = write!(out, " owner {p}");
            }
            None => {
                let _ = write!(out, " owner none");
            }
        }
        match camp.production {
            Some(p) => {
                let _ = write!(out, " production {p}");
            }
            None => {
                let _ = write!(out, " production none");
            }
        }
        let _ = write!(out, " progress {}", camp.progress);
        let _ = writeln!(out, "{}", if camp.manual_production { " manual" } else { "" });
        for who in &camp.residents {
            let mut key = String::from("resident ");
            let _ = write!(key, "{location}");
            write_companion(&mut out, &key, who);
        }
        // `exposed LOCATION RESIDENT CARRIED`: a camper's fraction of a
        // danger's point, by his place among the camp's residents.
        for (n, who) in camp.residents.iter().enumerate() {
            let t = carry_token(&who.exposure, !save.identity.is_legacy());
            if t != 0 {
                let _ = writeln!(out, "exposed {location} {n} {t}");
            }
        }
        for seat in &camp.stationed {
            let _ = writeln!(out, "stationed {location} {seat}");
        }
    }

    for (location, room, bag) in &save.rooms {
        let mut key = String::from("room ");
        let _ = write!(key, "{location} {room}");
        write_bag(&mut out, &key, bag);
    }
    // `ground LOCATION X Y KIND [ROUNDS] [pile ICON]`: the pile's picture
    // only once it has been drawn, which a legacy game never does.
    for d in &save.ground {
        let mut line = String::new();
        let _ = write!(line, "ground {} {} {} {}", d.location, d.at.x, d.at.y, d.kind);
        if let Some(r) = d.rounds {
            let _ = write!(line, " {r}");
        }
        if d.icon != crate::spawn::PILE_UNDRAWN {
            let _ = write!(line, " pile {}", d.icon);
        }
        let _ = writeln!(out, "{line}");
    }
    for (location, s) in &save.springs {
        let _ = writeln!(
            out,
            "spring {location} {} {} {}",
            s.reserve, s.refresh, s.capacity
        );
    }
    for t in &save.traders {
        let _ = write!(
            out,
            "trader {} {} {} {} {}",
            t.record,
            t.name_id,
            t.at,
            t.home,
            t.counter
        );
        write_bag_tokens(&mut out, &t.stock);
        let _ = writeln!(out);
    }
    let _ = writeln!(out, "end");
    out
}

/// One human player's lines. `prefix` is empty for the one playing and
/// `"bench "` for the other: the same lines, read by the same code.
/// A carry as the file holds it: a legacy game's as it is, a v1.2 game's
/// with its lift ([`crate::danger::v12_token`]).
fn carry_token(e: &crate::danger::Exposure, v12: bool) -> u32 {
    if v12 {
        crate::danger::v12_token(e)
    } else {
        e.carried
    }
}

fn write_player(out: &mut String, prefix: &str, p: &Bench, v12: bool) {
    match p.previous_location {
        Some(n) => { let _ = writeln!(out, "{prefix}previous {n}"); }
        None => { let _ = writeln!(out, "{prefix}previous none"); }
    }
    let _ = writeln!(out, "{prefix}mapmode {}", if p.info_mode { "info" } else { "travel" });
    let _ = write!(out, "{prefix}talked");
    for r in &p.talked {
        let _ = write!(out, " {r}");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "{prefix}at {}", p.at_location);
    match p.place {
        Place::World => { let _ = writeln!(out, "{prefix}place world"); }
        Place::Location(n) => { let _ = writeln!(out, "{prefix}place location {n}"); }
    }
    let _ = writeln!(out, "{prefix}position {} {}", p.position.x, p.position.y);
    if let Some(r) = p.return_position {
        let _ = writeln!(out, "{prefix}return {} {}", r.x, r.y);
    }
    if let Some(r) = p.stood {
        let _ = writeln!(out, "{prefix}stood {} {}", r.x, r.y);
    }
    let mut key = String::from(prefix);
    key.push_str("needs");
    write_needs(out, &key, &p.needs);
    // The player's carry first, then each companion's in party order.
    let _ = write!(out, "{prefix}exposure {}", carry_token(&p.exposure, v12));
    for c in &p.companions {
        let _ = write!(out, " {}", carry_token(&c.exposure, v12));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "{prefix}experience {}", p.experience);
    let mut key = String::from(prefix);
    key.push_str("carried");
    write_bag(out, &key, &p.carried);
    let _ = write!(out, "{prefix}kit");
    write_kit(out, &p.kit);
    let _ = writeln!(
        out,
        " {} {}",
        if p.fight_mode { "fight" } else { "speak" },
        if p.single_mode { "single" } else { "group" }
    );
    if let Some(j) = &p.journey {
        let _ = writeln!(out, "{prefix}journey {} {} {} {}", j.from, j.to, j.total, j.remaining);
    }
    let _ = writeln!(out, "{prefix}next-id {}", p.next_id);
    match p.selected {
        Selection::Leader => { let _ = writeln!(out, "{prefix}select leader"); }
        Selection::Companion(id) => { let _ = writeln!(out, "{prefix}select companion {id}"); }
        Selection::Camper(r) => { let _ = writeln!(out, "{prefix}select camper {r}"); }
    }
    let mut key = String::from(prefix);
    key.push_str("companion");
    for who in &p.companions {
        write_companion(out, &key, who);
    }
}

fn write_needs(out: &mut String, key: &str, n: &CharacterNeeds) {
    let _ = writeln!(
        out,
        "{key} {} {} {} {} {}",
        n.food, n.max_food, n.water, n.max_water, n.health
    );
}

fn write_bag(out: &mut String, key: &str, bag: &ItemBag) {
    let _ = write!(out, "{key}");
    write_bag_tokens(out, bag);
    let _ = writeln!(out);
}

/// A bag as a fixed number of words: how much it holds, how much is in it,
/// the kinds, then the rounds left in each slot.
///
/// **The length is what lets anything follow it on the same line.** Version
/// 4 let a bag run to the end of its line, which is why a companion's
/// weapon could not be written after their pack.
fn write_bag_tokens(out: &mut String, bag: &ItemBag) {
    let _ = write!(out, " {} {}", bag.capacity(), bag.len());
    for kind in bag.iter() {
        let _ = write!(out, " {kind}");
    }
    for slot in 0..bag.len() {
        match bag.rounds_left(slot) {
            // `*` is "as many as that kind holds when new", which is all
            // but a fired gun — and a swung knife, which the original
            // leaves at minus one. See `Fighter::fire`.
            None => {
                let _ = write!(out, " *");
            }
            Some(rounds) => {
                let _ = write!(out, " {rounds}");
            }
        }
    }
}

fn write_kit(out: &mut String, kit: &crate::combat::Kit) {
    let slot = |s: Option<usize>| match s {
        Some(slot) => {
            let mut text = String::new();
            let _ = write!(text, "{slot}");
            text
        }
        None => String::from("none"),
    };
    let _ = write!(out, " {} {}", slot(kit.weapon), slot(kit.protection));
}

fn write_companion(out: &mut String, key: &str, who: &Companion) {
    let _ = write!(
        out,
        "{key} {} {} {} {} {} {} {}",
        who.id,
        // `-1` for somebody who came from no record, which is what a test
        // scenario's handed-over companion is.
        who.origin.map_or(-1i64, |r| r as i64),
        who.needs.food,
        who.needs.max_food,
        who.needs.water,
        who.needs.max_water,
        who.needs.health
    );
    let _ = write!(out, " {}", who.experience);
    write_bag_tokens(out, &who.pack);
    write_kit(out, &who.kit);
    let _ = write!(
        out,
        " {} {} {} {} {}",
        who.at.x, who.at.y, who.move_to.x, who.move_to.y, who.draws
    );
    let (fx, fy) = who.stride.carry(who.at);
    let f = &who.fellower;
    let (dx, dy) = f.leader_dest.map_or((-1, -1), |d| (d.x, d.y));
    let _ = write!(
        out,
        " {fx} {fy} {} {} {dx} {dy} {} {} {} {}",
        f.radius, f.leader_dest.is_some() as u8, f.delay, f.manual as u8, f.manual_delay, f.manual_stop
    );
    let _ = writeln!(out);
}

/// One machine player. The same shape as a companion line — needs, pack —
/// with the decisions in flight on the end.
fn write_rival(out: &mut String, rival: &crate::rival::Rival) {
    let _ = write!(
        out,
        "rival {} {} {} {} {} {} {} {}",
        rival.player,
        rival.at,
        rival.experience,
        rival.needs.food,
        rival.needs.max_food,
        rival.needs.water,
        rival.needs.max_water,
        rival.needs.health
    );
    write_bag_tokens(out, &rival.pack);
    let _ = write!(
        out,
        " {} {} {} {}",
        match rival.mode {
            crate::rival::Mode::Waiting => "waiting",
            crate::rival::Mode::Expanding => "expanding",
            crate::rival::Mode::Hiring => "hiring",
        },
        rival.wait,
        rival.party.next_id(),
        if rival.dead { "dead" } else { "alive" }
    );
    match rival.headed {
        Some(to) => {
            let _ = write!(out, " headed {to}");
        }
        None => {
            let _ = write!(out, " headed none");
        }
    }
    match &rival.journey {
        Some(j) => {
            let _ = write!(out, " road {} {} {} {}", j.from, j.to, j.total, j.remaining);
        }
        None => {
            let _ = write!(out, " road none");
        }
    }
    // `previousLocation`, and Classic's own state when this seat has it:
    // the mode, the countdown, where it last headed, and the pool — every
    // type in the order it first went in, with its count.
    match rival.previous {
        Some(p) => {
            let _ = write!(out, " previous {p}");
        }
        None => {
            let _ = write!(out, " previous none");
        }
    }
    match &rival.classic {
        None => {
            let _ = write!(out, " classic none");
        }
        Some(ai) => {
            let _ = write!(out, " classic {} {}", ai.mode.name(), ai.wait);
            match ai.headed {
                Some(h) => {
                    let _ = write!(out, " {h}");
                }
                None => {
                    let _ = write!(out, " none");
                }
            }
            let _ = write!(out, " {}", ai.pool.items.len());
            for (id, n) in &ai.pool.items {
                let _ = write!(out, " {id} {n}");
            }
        }
    }
    if let Some(kind) = rival.weapon_kind {
        let _ = write!(out, " weapon {kind}");
    }
    let _ = writeln!(out);
    // Its group, in the same lines the player's group is written in — one
    // set of rules, one format.
    for who in &rival.party.companions {
        let mut key = String::from("rival-mate ");
        let _ = write!(key, "{}", rival.player);
        write_companion(out, &key, who);
    }
}

// -- reading -----------------------------------------------------------------

/// Read a save, or say why not. **Builds a whole value or none.**
///
/// `pack` and the rules are what the caller is currently running; a save
/// made against anything else is refused rather than adapted.
/// `asked` is what the run was told to play under, or `None` when it was
/// told nothing — see [`SaveError::ContestedRules`].
pub fn parse(
    text: &str,
    pack: u64,
    identity: crate::rules::Identity,
    asked: Option<crate::combat::Contested>,
) -> Result<Save, SaveError> {
    if text.len() > limits::BYTES {
        return Err(SaveError::TooBig("file".into()));
    }

    let mut lines = text.lines();
    let header = lines.next().ok_or(SaveError::NotASave)?;
    let mut header = header.split_whitespace();
    if header.next() != Some(MAGIC) {
        return Err(SaveError::NotASave);
    }
    let found = num::<u32>(header.next(), "version")?;
    if !READS.contains(&found) {
        return Err(SaveError::WrongVersion { found, want: VERSION });
    }

    let mut b = Builder { version: found, ..Builder::default() };
    let mut seen = 0usize;
    let mut ended = false;
    for line in lines {
        seen += 1;
        if seen > limits::LINES {
            return Err(SaveError::TooBig("lines".into()));
        }
        // Only the start is trimmed: a name is counted in bytes and may end
        // in a space.
        let line = line.trim_start().trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if line.trim_end() == "end" {
            ended = true;
            break;
        }
        b.record(line)?;
    }
    // A save says when it is finished. Without that line the file was cut
    // short, and no amount of well-formed records before the cut makes it a
    // world — this is the check that turns a half-written file into a
    // refusal rather than a half-restored game.
    if !ended {
        return Err(SaveError::Truncated);
    }
    b.finish(pack, identity, asked)
}

/// What a save says about itself, read without restoring anything.
///
/// The start screen needs this: which rules the file was made under, so that
/// taking it up configures the game from the file rather than asking the
/// player to name the same things again. **It is not a validation** — the
/// version and the pack are reported, not judged, because the screen wants to
/// say what is there and [`parse`] is what refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub identity: crate::rules::Identity,
    pub pack: u64,
    pub difficulty: u16,
    pub day: u16,
    /// Places the player held when it was saved; `None` in older files.
    pub held: Option<u16>,
    /// The saving player's name, and when; `None` in older files.
    pub player: Option<String>,
    pub saved: Option<Stamp>,
}

/// Read a save's header. Stops at the first line that is not one.
pub fn header(text: &str) -> Result<Header, SaveError> {
    if text.len() > limits::BYTES {
        return Err(SaveError::TooBig("file".into()));
    }
    let mut lines = text.lines();
    let first = lines.next().ok_or(SaveError::NotASave)?;
    let mut first = first.split_whitespace();
    if first.next() != Some(MAGIC) {
        return Err(SaveError::NotASave);
    }
    let version = num::<u32>(first.next(), "version")?;

    let (mut mode, mut engine, mut pack, mut day) = (None, None, None, None);
    let (mut preset, mut ai) = (None, None);
    let mut difficulty = 0u16;
    let mut held = None;
    let (mut player, mut saved) = (None, None);
    for line in lines.take(18) {
        let mut w = line.split_whitespace();
        match w.next().unwrap_or("") {
            "player" => player = line.strip_prefix("player ").map(String::from),
            "saved" => saved = Some(Stamp { days: num(w.next(), "saved")?, minute: num(w.next(), "saved")? }),
            "mode" => {
                let name = word(w.next(), "mode")?;
                mode = [Mode::Original, Mode::Extended].into_iter().find(|m| m.name() == name);
            }
            "engine" => engine = Some(word(w.next(), "engine")?.to_string()),
            "preset" => preset = Some(word(w.next(), "preset")?.to_string()),
            "ai" => ai = Some(word(w.next(), "ai")?.to_string()),
            "pack" => pack = Some(hex(w.next(), "pack")?),
            // Absent in every version before 10, which is refused anyway;
            // reading it as the easiest difficulty keeps the screen able to
            // describe such a file instead of failing to.
            "difficulty" => difficulty = num(w.next(), "difficulty")?,
            "held" => held = Some(num(w.next(), "held")?),
            "day" => {
                day = Some(num(w.next(), "day")?);
                break;
            }
            _ => {}
        }
    }
    let identity = identity_of(version, mode, engine.as_deref(), preset.as_deref(), ai.as_deref())?;
    Ok(Header {
        version,
        identity,
        pack: pack.ok_or(SaveError::Truncated)?,
        difficulty,
        day: day.ok_or(SaveError::Truncated)?,
        held,
        player,
        saved,
    })
}

/// A game's identity from its header lines, or why it has none.
///
/// **Exactly one shape is valid.** A legacy file names a known `mode` and a
/// legacy `engine`, and nothing about presets; a v1.2 file names `engine
/// v12`, a known `preset` and a known `ai`, and no `mode`. A version 20 file
/// is legacy or nothing. Anything else is refused as [`SaveError::WrongRules`]
/// — never guessed into the nearest identity.
fn identity_of(
    version: u32,
    mode: Option<Mode>,
    engine: Option<&str>,
    preset: Option<&str>,
    ai: Option<&str>,
) -> Result<crate::rules::Identity, SaveError> {
    use crate::rules::{AiProfile, Identity, RuleSet};
    match engine {
        Some(e) if e == Identity::V12_ENGINE => {
            if version < 21 || mode.is_some() {
                return Err(SaveError::WrongRules);
            }
            let preset = preset.and_then(RuleSet::parse).ok_or(SaveError::WrongRules)?;
            let ai = ai.and_then(AiProfile::parse).ok_or(SaveError::WrongRules)?;
            Ok(Identity::V12 { preset, ai })
        }
        Some(e) => {
            if preset.is_some() || ai.is_some() {
                return Err(SaveError::WrongRules);
            }
            let engine = [Engine::V091, Engine::Current].into_iter().find(|x| x.name() == e).ok_or(SaveError::WrongRules)?;
            let mode = mode.ok_or(SaveError::WrongRules)?;
            Ok(Identity::Legacy { engine, mode })
        }
        None => Err(SaveError::WrongRules),
    }
}

#[derive(Default)]
struct Builder {
    version: u32,
    seed: Option<u64>,
    /// Files 22 to 24 carry the generator; it is checked and dropped.
    rng: bool,
    faces: Vec<(usize, i32)>,
    mode: Option<String>,
    engine: Option<String>,
    preset: Option<String>,
    ai: Option<String>,
    pack: Option<u64>,
    world: Option<u64>,
    difficulty: Option<u16>,
    populated: Vec<usize>,
    day: Option<u16>,
    at_location: Option<usize>,
    place: Option<Place>,
    position: Option<Point>,
    return_position: Option<Point>,
    stood: Option<Point>,
    needs: Option<CharacterNeeds>,
    exposure: Option<Vec<u32>>,
    motion: crate::gait::Motion,
    flights: Vec<crate::gait::Flight>,
    paths: Vec<crate::gait::PathRecord>,
    experience: Option<i32>,
    carried: Option<ItemBag>,
    kit: Option<crate::combat::Kit>,
    fight_mode: Option<bool>,
    single_mode: Option<bool>,
    time: Option<(u32, u32)>,
    previous: Option<Option<usize>>,
    info_mode: Option<bool>,
    talked: Option<Vec<usize>>,
    contested: Option<crate::combat::Contested>,
    journey: Option<Journey>,
    next_id: Option<usize>,
    companions: Vec<Companion>,
    hired: Vec<(usize, usize)>,
    standing: Vec<crate::hire::Standing>,
    fallen: Vec<usize>,
    identities: Vec<(usize, u16, u8, u16)>,
    entries: Vec<(usize, u32)>,
    lying: Vec<(usize, usize)>,
    respawns: Vec<crate::game::Pending>,
    rivals: Vec<crate::rival::Rival>,
    /// Their groups, gathered as they are read and attached in `finish`, so
    /// the order of the lines does not matter.
    rival_mates: Vec<(usize, Companion)>,
    /// v1.2 Amiga states, by player, attached to their seats in `finish`.
    amigas: Vec<(usize, crate::amiga_ai::AmigaAi)>,
    /// v1.2 Modern states, by player, the same way.
    moderns: Vec<(usize, Option<crate::modern_ai::state::ModernAi>)>,
    decided: Option<crate::rivals::Decision>,
    goals: Option<usize>,
    known: Vec<String>,
    visited: Vec<(usize, Vec<usize>)>,
    cheats: bool,
    camps: Vec<(usize, Camp)>,
    residents: Vec<Resident>,
    /// Campers' exposure, `(location, resident, carried)`, attached in
    /// `finish`.
    exposed: Vec<(usize, usize, u32)>,
    /// Leaders standing in a location, `(location, seat)`, attached to its
    /// camp in `finish`.
    stationed: Vec<(usize, usize)>,
    rooms: Vec<(usize, usize, ItemBag)>,
    ground: Vec<crate::spawn::Dropped>,
    springs: Vec<(usize, WaterSource)>,
    traders: Vec<crate::trader::Trader>,
    seat: Option<usize>,
    humans: Vec<Human>,
    colours: Option<[crate::player_colors::PlayerColor; 4]>,
    finished: Option<Vec<usize>>,
    selected: Option<Selection>,
    /// The other human's lines, read by the same code into a builder of
    /// their own.
    bench: Option<alloc::boxed::Box<Builder>>,
}

/// What a `bench` line may say: a player's own lines and nothing about the
/// world.
const PLAYER_KEYS: [&str; 16] = [
    "previous", "mapmode", "talked", "at", "place", "position", "return", "stood", "needs", "exposure",
    "experience", "carried", "kit", "journey", "next-id", "select",
];

impl Builder {
    fn record(&mut self, line: &str) -> Result<(), SaveError> {
        let mut w = line.split_whitespace();
        let key = w.next().unwrap_or("");
        match key {
            "seed" => self.seed = Some(num(w.next(), "seed")?),
            "rng" if self.version < 25 => {
                if self.rng {
                    return Err(SaveError::Duplicate("rng".into()));
                }
                let inext = num(w.next(), "rng")?;
                let inextp = num(w.next(), "rng")?;
                let mut table = [0i32; 56];
                for v in table.iter_mut() {
                    *v = num(w.next(), "rng")?;
                }
                if w.next().is_some() {
                    return Err(SaveError::BadValue("rng".into()));
                }
                // Read to check the file, and dropped: a load continues the
                // process's generator, as v1.2's does.
                crate::net_random::NetRandom::from_state(table, inext, inextp)
                    .ok_or_else(|| SaveError::BadValue("rng".into()))?;
                self.rng = true;
            }
            "flight" => {
                cap(self.flights.len(), limits::ENTRIES, "flight")?;
                let who = num(w.next(), "flight")?;
                let from = num(w.next(), "flight")?;
                let to = point(&mut w)?;
                self.flights.push(crate::gait::Flight { who, from, to, left: num(w.next(), "flight")? });
            }
            "path" => {
                cap(self.paths.len(), limits::ENTRIES, "path")?;
                let who = num(w.next(), "path")?;
                let move_to = point(&mut w)?;
                let bits = |t: Option<&str>| {
                    t.and_then(|t| u32::from_str_radix(t, 16).ok()).map(f32::from_bits).filter(|v| v.is_finite()).ok_or(SaveError::BadValue("path".into()))
                };
                let adjusted = (bits(w.next())?, bits(w.next())?);
                self.paths.push(crate::gait::PathRecord { who, move_to, adjusted });
            }
            "motion" => {
                let leader = (num(w.next(), "motion")?, num(w.next(), "motion")?);
                self.motion = crate::gait::Motion { leader, cooldown: num(w.next(), "motion")? };
            }
            "face" => {
                cap(self.faces.len(), limits::PLAYERS, "faces")?;
                let seat: usize = num(w.next(), "face seat")?;
                let face: i32 = num(w.next(), "face")?;
                if seat >= limits::PLAYERS || !(0..5).contains(&face) || self.faces.iter().any(|f| f.0 == seat) {
                    return Err(SaveError::BadValue("face".into()));
                }
                self.faces.push((seat, face));
            }
            "mode" => self.mode = Some(word(w.next(), "mode")?.into()),
            "engine" => self.engine = Some(word(w.next(), "engine")?.into()),
            "preset" => self.preset = Some(word(w.next(), "preset")?.into()),
            "ai" => self.ai = Some(word(w.next(), "ai")?.into()),
            "pack" => self.pack = Some(hex(w.next(), "pack")?),
            "world" => self.world = Some(hex(w.next(), "world")?),
            "difficulty" => self.difficulty = Some(num(w.next(), "difficulty")?),
            "populated" => self.populated.push(num(w.next(), "populated")?),
            "day" => self.day = Some(num(w.next(), "day")?),
            "held" | "player" | "saved" => {}
            "seat" => self.seat = Some(num(w.next(), "seat")?),
            "select" => {
                self.selected = Some(match word(w.next(), "selection")? {
                    "leader" => Selection::Leader,
                    "companion" => Selection::Companion(num(w.next(), "selected companion")?),
                    "camper" => Selection::Camper(num(w.next(), "selected camper")?),
                    _ => return Err(SaveError::BadValue("selection".into())),
                })
            }
            "colours" => {
                let mut c = [crate::player_colors::PlayerColor::Green; 4];
                for slot in c.iter_mut() {
                    *slot = crate::player_colors::PlayerColor::from_word(word(w.next(), "colour")?)
                        .ok_or_else(|| SaveError::BadValue("colour".into()))?;
                }
                self.colours = Some(c);
            }
            "human" => {
                cap(self.humans.len(), 4, "humans")?;
                let seat = num(w.next(), "human seat")?;
                let color = crate::player_colors::PlayerColor::from_word(word(w.next(), "colour")?)
                    .ok_or_else(|| SaveError::BadValue("colour".into()))?;
                let face = num(w.next(), "face")?;
                let (out, gone) = match word(w.next(), "alive")? {
                    "alive" => (false, false),
                    "out" => (true, false),
                    "gone" => (true, true),
                    _ => return Err(SaveError::BadValue("alive".into())),
                };
                let len: usize = num(w.next(), "name length")?;
                // The name is the last `len` bytes after the sixth space.
                let mut rest = line;
                for _ in 0..6 {
                    rest = rest.split_once(' ').map(|(_, r)| r).unwrap_or("");
                }
                let name = rest.get(..len).ok_or_else(|| SaveError::BadValue("name".into()))?;
                if name.len() != rest.len() {
                    return Err(SaveError::BadValue("name".into()));
                }
                self.humans.push(Human { seat, name: name.into(), face, color, out, gone });
            }
            "finished" => {
                let mut v = Vec::new();
                for t in w {
                    cap(v.len(), 4, "finished")?;
                    v.push(num(Some(t), "finished")?);
                }
                self.finished = Some(v);
            }
            "bench" => {
                let rest = line["bench".len()..].trim_start();
                let key = rest.split_whitespace().next().unwrap_or("");
                if !PLAYER_KEYS.contains(&key) && key != "companion" {
                    return Err(SaveError::BadLine(line.into()));
                }
                self.bench.get_or_insert_with(Default::default).record(rest)?;
            }
            "time" => {
                let left: u32 = num(w.next(), "time")?;
                let round: u32 = num(w.next(), "time")?;
                if round == 0 || left > round {
                    return Err(SaveError::BadValue("time".into()));
                }
                self.time = Some((left, round));
            }
            "previous" => {
                self.previous = Some(match w.clone().next() {
                    Some("none") => None,
                    _ => Some(location(w.next())?),
                })
            }
            "talked" => {
                let mut v = Vec::new();
                for r in w {
                    v.push(r.parse::<usize>().map_err(|_| SaveError::BadValue("talked".into()))?);
                }
                self.talked = Some(v);
            }
            "mapmode" => {
                self.info_mode = Some(match word(w.next(), "map mode")? {
                    "info" => true,
                    "travel" => false,
                    _ => return Err(SaveError::BadValue("map mode".into())),
                })
            }
            "at" => self.at_location = Some(location(w.next())?),
            "place" => {
                self.place = Some(match word(w.next(), "place")? {
                    "world" => Place::World,
                    "location" => Place::Location(location(w.next())?),
                    other => return Err(SaveError::BadValue(other.into())),
                })
            }
            "position" => self.position = Some(point(&mut w)?),
            "return" => self.return_position = Some(point(&mut w)?),
            "stood" => self.stood = Some(point(&mut w)?),
            "needs" => self.needs = Some(needs(&mut w)?),
            "exposure" => {
                let mut carried = Vec::new();
                for token in w {
                    let n: u32 = num(Some(token), "exposure")?;
                    // Never a whole point or more: `Exposure::step` pays
                    // those at once, so a file saying otherwise is not ours
                    // (a v1.2 file's lift adds one, `danger::v12_token`;
                    // the legacy bound is checked once the game is known).
                    if n >= 2 * crate::danger::UNIT {
                        return Err(SaveError::BadValue("exposure".into()));
                    }
                    carried.push(n);
                }
                self.exposure = Some(carried);
            }
            "experience" => self.experience = Some(num(w.next(), "experience")?),
            "carried" => self.carried = Some(bag(&mut w)?),
            "contested" => {
                let mut contested = crate::combat::Contested::default();
                for token in w {
                    let (name, value) = token
                        .split_once('=')
                        .ok_or_else(|| SaveError::BadValue("contested rule".into()))?;
                    let on = match value {
                        "yes" => true,
                        "no" => false,
                        _ => return Err(SaveError::BadValue("contested answer".into())),
                    };
                    match name {
                        "melee-spends-a-round" => contested.melee_spends_a_round = on,
                        "best-armour-wins" => contested.best_armour_wins = on,
                        // Not skipped: a rule this build does not know is a
                        // file whose fights this build cannot reproduce.
                        _ => return Err(SaveError::BadValue("contested rule".into())),
                    }
                }
                self.contested = Some(contested);
            }
            "kit" => {
                self.kit = Some(kit(&mut w)?);
                self.fight_mode = Some(match word(w.next(), "fight mode")? {
                    "fight" => true,
                    "speak" => false,
                    _ => return Err(SaveError::BadValue("fight mode".into())),
                });
                self.single_mode = Some(match word(w.next(), "single mode")? {
                    "single" => true,
                    "group" => false,
                    _ => return Err(SaveError::BadValue("single mode".into())),
                });
            }
            "journey" => {
                self.journey = Some(Journey {
                    from: location(w.next())?,
                    to: location(w.next())?,
                    total: num(w.next(), "total")?,
                    remaining: num(w.next(), "remaining")?,
                })
            }
            "rival" => {
                cap(self.rivals.len(), limits::ENTRIES, "rivals")?;
                self.rivals.push(rival(&mut w)?);
            }
            "amiga" => {
                cap(self.amigas.len(), limits::PLAYERS, "amiga")?;
                let player: usize = num(w.next(), "amiga player")?;
                let difficulty: i32 = num(w.next(), "amiga difficulty")?;
                let mode = crate::amiga_ai::Mode::parse(word(w.next(), "amiga mode")?)
                    .ok_or_else(|| SaveError::BadValue("amiga mode".into()))?;
                let headed = maybe(w.next(), "amiga headed")?;
                let wait: i32 = num(w.next(), "amiga wait")?;
                let last_arrival_day: i32 = num(w.next(), "amiga last arrival")?;
                let maintenance_pending = match word(w.next(), "amiga pending")? {
                    "pending" => true,
                    "done" => false,
                    _ => return Err(SaveError::BadValue("amiga pending".into())),
                };
                if !(0..=2).contains(&difficulty) || self.amigas.iter().any(|(p, _)| *p == player) {
                    return Err(SaveError::BadValue("amiga".into()));
                }
                self.amigas.push((player, crate::amiga_ai::AmigaAi {
                    difficulty, mode, headed, wait, last_arrival_day, maintenance_pending,
                }));
            }
            "rival-mate" => {
                cap(self.rival_mates.len(), limits::PEOPLE, "rival group")?;
                let player = num(w.next(), "rival-mate player")?;
                self.rival_mates.push((player, companion(&mut w)?));
            }
            "goals" => self.goals = Some(num(w.next(), "goals")?),
            "known" => self.known = w.map(String::from).collect(),
            "cheats" => self.cheats = true,
            "visited" => {
                let seat = num(w.next(), "visited seat")?;
                cap(self.visited.len(), limits::PLAYERS, "visited")?;
                let mut places = Vec::new();
                for p in w {
                    cap(places.len(), limits::LOCATION, "visited place")?;
                    places.push(num(Some(p), "visited place")?);
                }
                self.visited.push((seat, places));
            }
            "decided" => {
                let how = word(w.next(), "decision")?;
                let who = num(w.next(), "decided player")?;
                self.decided = Some(match how {
                    "won" => crate::rivals::Decision::Won(who),
                    "lost" => crate::rivals::Decision::Lost(who),
                    _ => return Err(SaveError::BadValue("decision".into())),
                });
            }
            "next-id" => self.next_id = Some(num(w.next(), "next-id")?),
            "companion" => {
                cap(self.companions.len(), limits::PEOPLE, "companions")?;
                self.companions.push(companion(&mut w)?);
            }
            "hired" => {
                cap(self.hired.len(), limits::ENTRIES, "hired")?;
                self.hired.push((num(w.next(), "record")?, num(w.next(), "player")?));
            }
            "modern" => {
                let player: usize = num(w.next(), "modern player")?;
                let what = word(w.next(), "modern record")?;
                let i = match self.moderns.iter().position(|(p, _)| *p == player) {
                    Some(i) => i,
                    None => {
                        cap(self.moderns.len(), limits::PLAYERS, "modern")?;
                        self.moderns.push((player, None));
                        self.moderns.len() - 1
                    }
                };
                crate::modern_ai::state::ModernAi::read(&mut self.moderns[i].1, what, &mut w)
                    .map_err(|e| SaveError::BadValue(alloc::format!("modern {what}: {e}")))?;
            }
            "identity" => {
                cap(self.identities.len(), limits::ENTRIES, "identities")?;
                let record = num(w.next(), "record")?;
                if self.identities.iter().any(|i| i.0 == record) {
                    return Err(SaveError::Duplicate("identity".into()));
                }
                self.identities.push((record, num(w.next(), "name")?, num(w.next(), "face")?, num(w.next(), "body")?));
            }
            "fallen" => {
                cap(self.fallen.len(), limits::ENTRIES, "fallen")?;
                self.fallen.push(num(w.next(), "record")?);
            }
            "lying" => {
                cap(self.lying.len(), limits::ENTRIES, "lying")?;
                let record = num(w.next(), "record")?;
                let at = location(w.next())?;
                self.lying.push((record, at));
            }
            "entered" => {
                cap(self.entries.len(), limits::ENTRIES, "entered")?;
                let record = num(w.next(), "record")?;
                let entered = num(w.next(), "entered")?;
                if self.entries.iter().any(|&(r, _)| r == record) {
                    return Err(SaveError::Duplicate("entered".into()));
                }
                self.entries.push((record, entered));
            }

            "standing" => {
                cap(self.standing.len(), limits::ENTRIES, "standing")?;
                let record = num(w.next(), "record")?;
                let at = point(&mut w)?;
                let move_to = point(&mut w)?;
                self.standing.push(crate::hire::Standing {
                    record,
                    at,
                    move_to,
                    food: num(w.next(), "food")?,
                    water: num(w.next(), "water")?,
                    health: num(w.next(), "health")?,
                    mind: crate::mind::Mind {
                        rest: num(w.next(), "rest")?,
                        first: num::<u8>(w.next(), "first")? != 0,
                        wait_after_attack: num(w.next(), "wait")?,
                        chasing: num(w.next(), "chasing")?,
                        charging: num::<u8>(w.next(), "charging")? != 0,
                        target: 0,
                        draws: num(w.next(), "draws")?,
                    },
                    bearing: crate::gait::Bearing::default(),
                    location: location(w.next())?,
                    experience: num(w.next(), "experience")?,
                    pack: bag(&mut w)?,
                    kit: kit(&mut w)?,
                });
                // Version 23's movement, absent from a version 22 file.
                if let Some(t) = w.next() {
                    let s = self.standing.last_mut().ok_or(SaveError::Truncated)?;
                    s.mind.target = num(Some(t), "target")?;
                    let carry = (num(w.next(), "fx")?, num(w.next(), "fy")?);
                    s.bearing.stride = crate::gait::Stride::carried(s.at, carry);
                    s.bearing.pause = num(w.next(), "pause")?;
                    s.bearing.near = num::<u8>(w.next(), "near")? != 0;
                    s.bearing.controlled = num::<u8>(w.next(), "controlled")? != 0;
                    s.bearing.tame = match w.next() {
                        None => false,
                        Some("tame") => true,
                        Some(_) => return Err(SaveError::BadValue("tame".into())),
                    };
                }
            }
            "respawn" => {
                cap(self.respawns.len(), limits::ENTRIES, "respawns")?;
                self.respawns.push(crate::game::Pending {
                    record: num(w.next(), "record")?,
                    location: location(w.next())?,
                    days: num(w.next(), "days")?,
                    food: num(w.next(), "food")?,
                    water: num(w.next(), "water")?,
                    revive_health: num(w.next(), "revive health")?,
                });
            }
            "camp" => {
                cap(self.camps.len(), limits::ENTRIES, "camps")?;
                let at = location(w.next())?;
                expect(w.next(), "owner")?;
                let owner = maybe(w.next(), "owner")?;
                expect(w.next(), "production")?;
                let production = maybe(w.next(), "production")?;
                expect(w.next(), "progress")?;
                let progress: f32 = word(w.next(), "progress")?
                    .parse()
                    .map_err(|_| SaveError::BadValue("progress".into()))?;
                if !progress.is_finite() || progress < 0.0 {
                    return Err(SaveError::BadValue("progress".into()));
                }
                // v1.2: a production its player chose (`manual`), optional.
                let manual_production = match w.next() {
                    None => false,
                    Some("manual") => true,
                    Some(_) => return Err(SaveError::BadValue("camp".into())),
                };
                self.camps.push((at, Camp { owner, residents: Vec::new(), production, progress, stationed: Vec::new(), manual_production }));
            }
            "stationed" => {
                cap(self.stationed.len(), limits::PLAYERS, "stationed")?;
                let at = location(w.next())?;
                let seat: usize = num(w.next(), "seat")?;
                if seat >= limits::PLAYERS {
                    return Err(SaveError::TooBig("seat".into()));
                }
                // One man stands in one place.
                if self.stationed.iter().any(|&(_, s)| s == seat) {
                    return Err(SaveError::Duplicate("stationed".into()));
                }
                self.stationed.push((at, seat));
            }
            "exposed" => {
                cap(self.exposed.len(), limits::PEOPLE, "exposed")?;
                let at = location(w.next())?;
                let n = num(w.next(), "exposed")?;
                let carried: u32 = num(w.next(), "exposed")?;
                if carried >= 2 * crate::danger::UNIT {
                    return Err(SaveError::BadValue("exposure".into()));
                }
                self.exposed.push((at, n, carried));
            }
            "resident" => {
                cap(self.residents.len(), limits::PEOPLE, "residents")?;
                let at = location(w.next())?;
                self.residents.push(Resident { location: at, who: companion(&mut w)? });
            }
            "room" => {
                cap(self.rooms.len(), limits::ENTRIES, "rooms")?;
                let at = location(w.next())?;
                let room = num(w.next(), "room")?;
                self.rooms.push((at, room, bag(&mut w)?));
            }
            "ground" => {
                cap(self.ground.len(), limits::ENTRIES, "ground")?;
                let at = location(w.next())?;
                let p = point(&mut w)?;
                let kind = num(w.next(), "kind")?;
                let (mut rounds, mut icon) = (None, crate::spawn::PILE_UNDRAWN);
                let mut next = w.next();
                if let Some(r) = next.filter(|t| *t != "pile") {
                    rounds = Some(r.parse::<i32>().map_err(|_| SaveError::BadValue("rounds".into()))?);
                    next = w.next();
                }
                if next == Some("pile") {
                    icon = num(w.next(), "pile")?;
                    if icon > 1 {
                        return Err(SaveError::BadValue("pile".into()));
                    }
                    next = w.next();
                }
                if next.is_some() {
                    return Err(SaveError::BadValue("ground".into()));
                }
                self.ground.push(crate::spawn::Dropped { location: at, at: p, kind, rounds, icon });
            }
            "spring" => {
                cap(self.springs.len(), limits::ENTRIES, "springs")?;
                let at = location(w.next())?;
                self.springs.push((
                    at,
                    WaterSource {
                        reserve: num(w.next(), "reserve")?,
                        refresh: num(w.next(), "refresh")?,
                        capacity: num(w.next(), "capacity")?,
                    },
                ));
            }
            "trader" => {
                cap(self.traders.len(), limits::ENTRIES, "traders")?;
                let record = num(w.next(), "record")?;
                let name_id = num(w.next(), "name")?;
                let at = location(w.next())?;
                let home = location(w.next())?;
                let counter = num(w.next(), "counter")?;
                self.traders.push(crate::trader::Trader {
                    record,
                    name_id,
                    at,
                    home,
                    counter,
                    stock: bag(&mut w)?,
                });
            }
            other => return Err(SaveError::BadLine(other.into())),
        }
        Ok(())
    }

    fn finish(
        mut self,
        pack: u64,
        identity: crate::rules::Identity,
        asked: Option<crate::combat::Contested>,
    ) -> Result<Save, SaveError> {
        let found_pack = self.pack.ok_or(SaveError::Truncated)?;
        if found_pack != pack {
            return Err(SaveError::WrongPack { found: found_pack, want: pack });
        }
        let mode = match self.mode.as_deref() {
            Some(m) => Some([Mode::Original, Mode::Extended].into_iter().find(|x| x.name() == m).ok_or(SaveError::WrongRules)?),
            None => None,
        };
        let found = identity_of(self.version, mode, self.engine.as_deref(), self.preset.as_deref(), self.ai.as_deref())?;
        // The caller runs under the identity it took from this file's
        // header; a file that says otherwise is not the game asked for.
        if found != identity {
            return Err(SaveError::WrongRules);
        }
        // A v1.2 game from version 22 to 24 carries its generator; a legacy
        // one has none to carry.
        if !identity.is_legacy() && (22..25).contains(&self.version) && !self.rng {
            return Err(SaveError::Truncated);
        }
        if identity.is_legacy() && self.rng {
            return Err(SaveError::Inconsistent("a legacy game with a generator".into()));
        }
        if identity.is_legacy() && !self.faces.is_empty() {
            return Err(SaveError::Inconsistent("a legacy game with machine faces".into()));
        }
        if identity.is_legacy() && !self.paths.is_empty() {
            return Err(SaveError::Inconsistent("a legacy game with v1.2 paths".into()));
        }
        // **Restore, or refuse. Never quietly override.** With nothing
        // asked for, the file's answers are the session's from here on;
        // with something asked for that disagrees, the two authorities are
        // in conflict and picking one of them silently is the failure this
        // guards against.
        let contested = self.contested.ok_or(SaveError::Truncated)?;
        // The `/petko` mark is v1.2's; a legacy game has no dialog to set it.
        if self.cheats && identity.is_legacy() {
            return Err(SaveError::Inconsistent("a cheats mark in a legacy game".into()));
        }
        if let Some(want) = asked {
            if contested != want {
                return Err(SaveError::ContestedRules { found: contested, want });
            }
        }

        // **Identities are a party's, not the world's.** Each player hands
        // out its own, starting at one, so two players' first companions are
        // both number one and that is not a collision — while a companion
        // left in a camp and one walking with the same player are the same
        // kind of thing and must not collide.
        //
        // A camper's party is its camp's owner, which is why the camp is not
        // stored on the camper twice; a camp with residents and no owner is
        // refused further down.
        let owner_of = |location: usize| {
            self.camps.iter().find(|(l, _)| *l == location).and_then(|(_, c)| c.owner)
        };
        let mut by_player: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut note = |player: usize, id: usize| -> Result<(), SaveError> {
            let seen = match by_player.iter_mut().find(|(p, _)| *p == player) {
                Some((_, ids)) => ids,
                None => {
                    by_player.push((player, Vec::new()));
                    &mut by_player.last_mut().expect("just pushed").1
                }
            };
            if seen.contains(&id) {
                return Err(SaveError::Duplicate("companion id".into()));
            }
            seen.push(id);
            Ok(())
        };
        let seat = self.seat.ok_or(SaveError::Truncated)?;
        // The other human, read whole before anything is checked across the
        // two of them.
        let bench = match self.bench.take() {
            Some(b) => {
                let other = self
                    .humans
                    .iter()
                    .map(|h| h.seat)
                    .find(|&s| s != seat)
                    .ok_or_else(|| SaveError::Inconsistent("a bench with no second human".into()))?;
                Some(b.into_bench(other)?)
            }
            None => None,
        };
        for c in &self.companions {
            note(seat, c.id)?;
        }
        if let Some(b) = &bench {
            for c in &b.companions {
                note(b.seat, c.id)?;
            }
        }
        for (player, who) in &self.rival_mates {
            note(*player, who.id)?;
        }
        for r in &self.residents {
            // A resident of a camp nobody owns is refused later, by the check
            // that knows how to say so.
            if let Some(owner) = owner_of(r.location) {
                note(owner, r.who.id)?;
            }
        }
        let next_id = self.next_id.ok_or(SaveError::Truncated)?;
        // `next-id` is the player's own counter, so it answers for the
        // player's own people. A machine player's counter is rebuilt from
        // the identities its own party came back with.
        let past = |player: usize, next_id: usize| {
            !by_player
                .iter()
                .find(|(p, _)| *p == player)
                .is_some_and(|(_, ids)| ids.iter().any(|&id| id >= next_id))
        };
        if !past(seat, next_id) || bench.as_ref().is_some_and(|b| !past(b.seat, b.next_id)) {
            return Err(SaveError::BadValue("next-id is not past every id in use".into()));
        }

        // **One man is in one place.** An identity number belongs to a party,
        // but a `gam.dat` record belongs to the world: it is *which person*
        // this is, and a person cannot be walking with the player, walking
        // with an opponent and sitting in two camps at once.
        //
        // This is checked because it has happened. An opponent hired its own
        // camper back — he stays on his feet so he can be fought for, which
        // made him look like somebody to take on — and the save that came out
        // had record 52 as a resident of two camps, holding both places. The
        // hiring path is closed now; this refuses the *state*, whatever path
        // produced it, and refuses it here, where a refusal costs the caller
        // nothing: [`parse`] builds a whole value or none, so a session
        // loading a contradictory file is left exactly as it was.
        //
        // The figure standing in a camper's location is not a second claim.
        // It is the same person seen from the world's side — see the note on
        // the two ledgers above — and is not counted here.
        //
        // A companion with no record is a scenario's handed-over man and
        // claims nothing.
        {
            let mut claimed: Vec<usize> = Vec::new();
            let mut claim = |record: usize| -> Result<(), SaveError> {
                if claimed.contains(&record) {
                    return Err(SaveError::Duplicate("character in two places".into()));
                }
                claimed.push(record);
                Ok(())
            };
            for c in self.companions.iter().chain(bench.iter().flat_map(|b| b.companions.iter())) {
                if let Some(record) = c.origin {
                    claim(record)?;
                }
            }
            for (_, who) in &self.rival_mates {
                if let Some(record) = who.origin {
                    claim(record)?;
                }
            }
            for r in &self.residents {
                if let Some(record) = r.who.origin {
                    claim(record)?;
                }
            }
        }

        // One entry per trader, or a load would make two of somebody.
        for (i, t) in self.traders.iter().enumerate() {
            if self.traders[..i].iter().any(|o| o.record == t.record) {
                return Err(SaveError::Duplicate("trader".into()));
            }
        }

        // A room may be listed once.
        for (i, (l, r, _)) in self.rooms.iter().enumerate() {
            if self.rooms[..i].iter().any(|(a, b, _)| a == l && b == r) {
                return Err(SaveError::Duplicate("room".into()));
            }
        }
        for (i, (l, _)) in self.springs.iter().enumerate() {
            if self.springs[..i].iter().any(|(a, _)| a == l) {
                return Err(SaveError::Duplicate("spring".into()));
            }
        }
        for (i, (l, _)) in self.camps.iter().enumerate() {
            if self.camps[..i].iter().any(|(a, _)| a == l) {
                return Err(SaveError::Duplicate("camp".into()));
            }
        }

        // A record waits once, and only somebody dead is waiting: a
        // schedule for a living person would bring a second one back.
        for (i, w) in self.respawns.iter().enumerate() {
            if self.respawns[..i].iter().any(|o| o.record == w.record) {
                return Err(SaveError::Duplicate("respawn".into()));
            }
            if w.days <= 0 {
                return Err(SaveError::Inconsistent("a respawn already due".into()));
            }
            // A player's own leader is one of the four player records: he
            // is waiting only if his character is dead.
            let leader_dead = w.record < 4 && {
                let seat = w.record;
                if seat == self.seat.unwrap_or(usize::MAX) {
                    self.needs.is_some_and(|n| n.health <= 0)
                } else if let Some(b) = bench.as_ref().filter(|b| b.seat == seat) {
                    b.needs.health <= 0
                } else {
                    self.rivals.iter().any(|r| r.player == seat && r.needs.health <= 0)
                }
            };
            if !self.fallen.contains(&w.record) && !leader_dead {
                return Err(SaveError::Inconsistent("a respawn for somebody alive".into()));
            }
        }

        // Nobody dies twice, and nobody is both hired and dead.
        for (i, record) in self.fallen.iter().enumerate() {
            if self.fallen[..i].contains(record) {
                return Err(SaveError::Duplicate("fallen character".into()));
            }
            if self.hired.iter().any(|(r, _)| r == record) {
                return Err(SaveError::Inconsistent("a character both hired and dead".into()));
            }
        }

        // A character may be hired once. Two records claiming the same
        // person would make two of them on the next load.
        for (i, (record, _)) in self.hired.iter().enumerate() {
            if self.hired[..i].iter().any(|(r, _)| r == record) {
                return Err(SaveError::Duplicate("hired character".into()));
            }
        }

        // **One man, written down twice, and which half means what.**
        //
        // A camper is the camp's resident — that line is the man himself: his
        // identity, his needs, his pack, his experience, and which location
        // he is in, because the camp is the only record of that. He is also
        // somebody standing, and that line is the figure in the place: what
        // it is doing (its mind, which a companion does not carry) and how
        // much of it is left to beat down.
        //
        // **The camp is the man and the figure is the figure.** His health,
        // his food and his water are the camp's — a fight against a camper
        // takes them off the camp's copy, which is why a defender beaten
        // down stays beaten down when he is taken back out. Where he stands
        // is the figure's, because he wanders after he is left there.
        //
        // The writer therefore fills each half from the other's column, and
        // a file where they still differ is one two different things
        // happened to. Taking either half would let the order the lines are
        // read in settle it, so it is refused instead.
        for r in &self.residents {
            let Some(record) = r.who.origin else { continue };
            // **Somebody hired is not standing anywhere.** A man taken on and
            // then left in a camp keeps a `standing` line from before he was
            // hired, and its position is wherever the world last had him —
            // stale by definition, and nothing to hold the camp to.
            if self.hired.iter().any(|(who, _)| *who == record) {
                continue;
            }
            let Some(standing) = self.standing.iter().find(|p| p.record == record) else {
                continue;
            };
            if standing.at != r.who.at
                || standing.move_to != r.who.move_to
                || standing.food != r.who.needs.food
                || standing.water != r.who.needs.water
                || standing.health != r.who.needs.health
            {
                return Err(SaveError::Inconsistent(
                    "a camper and the figure standing in his camp disagree".into(),
                ));
            }
        }

        // Residents name their camp, so a resident without one is a file
        // that does not describe a world.
        for r in self.residents {
            let Some((_, camp)) = self.camps.iter_mut().find(|(l, _)| *l == r.location) else {
                return Err(SaveError::BadValue("resident of no camp".into()));
            };
            camp.residents.push(r.who);
        }
        for (location, n, carried) in self.exposed {
            let who = self.camps.iter_mut().find(|(l, _)| *l == location).and_then(|(_, c)| c.residents.get_mut(n));
            let Some(who) = who else { return Err(SaveError::BadValue("exposed".into())) };
            who.exposure.carried = carried;
        }
        for (location, seat) in self.stationed {
            let Some((_, camp)) = self.camps.iter_mut().find(|(l, _)| *l == location) else {
                return Err(SaveError::BadValue("stationed in no camp".into()));
            };
            camp.stationed.push(seat);
        }

        // A machine player's campers carry identities its party handed out,
        // so its counter has to clear them too — otherwise the first person
        // it hires after a load is given a number one of its own campers is
        // already using.
        let camped_ids: Vec<(usize, usize)> = self
            .camps
            .iter()
            .filter_map(|(_, c)| c.owner.map(|o| (o, c)))
            .flat_map(|(o, c)| c.residents.iter().map(move |w| (o, w.id)))
            .collect();

        let carried = self.exposure.take().ok_or(SaveError::Truncated)?;
        if carried.len() != 1 + self.companions.len() {
            return Err(SaveError::Inconsistent("an exposure for every member of the group".into()));
        }
        let mut companions = core::mem::take(&mut self.companions);
        for (c, n) in companions.iter_mut().zip(carried.iter().skip(1)) {
            c.exposure = crate::danger::Exposure { carried: *n, lifted: false };
        }
        let mut save = Save {
            seed: self.seed.ok_or(SaveError::Truncated)?,
            motion: self.motion,
            flights: core::mem::take(&mut self.flights),
            paths: core::mem::take(&mut self.paths),
            faces: core::mem::take(&mut self.faces),
            identity,
            pack: found_pack,
            world: self.world.ok_or(SaveError::Truncated)?,
            difficulty: self.difficulty.ok_or(SaveError::Truncated)?,
            populated: core::mem::take(&mut self.populated),
            day: self.day.ok_or(SaveError::Truncated)?,
            at_location: self.at_location.ok_or(SaveError::Truncated)?,
            place: self.place.ok_or(SaveError::Truncated)?,
            position: self.position.ok_or(SaveError::Truncated)?,
            return_position: self.return_position,
            stood: self.stood,
            needs: self.needs.ok_or(SaveError::Truncated)?,
            experience: self.experience.ok_or(SaveError::Truncated)?,
            carried: self.carried.take().ok_or(SaveError::Truncated)?,
            kit: self.kit.ok_or(SaveError::Truncated)?,
            exposure: crate::danger::Exposure { carried: carried[0], lifted: false },
            fight_mode: self.fight_mode.ok_or(SaveError::Truncated)?,
            single_mode: self.single_mode.ok_or(SaveError::Truncated)?,
            time_left_ms: self.time.ok_or(SaveError::Truncated)?.0,
            round_ms: self.time.ok_or(SaveError::Truncated)?.1,
            previous_location: self.previous.ok_or(SaveError::Truncated)?,
            info_mode: self.info_mode.ok_or(SaveError::Truncated)?,
            talked: self.talked.take().ok_or(SaveError::Truncated)?,
            contested,
            journey: self.journey,
            rivals: {
                let mut rivals = self.rivals;
                for (player, who) in self.rival_mates {
                    match rivals.iter_mut().find(|r| r.player == player) {
                        Some(rival) => rival.party.take_back(who),
                        // A group with nobody to belong to is a file that
                        // does not describe a world.
                        None => {
                            return Err(SaveError::Inconsistent("a rival's man with no rival".into()))
                        }
                    }
                }
                for (owner, id) in camped_ids {
                    if let Some(rival) = rivals.iter_mut().find(|r| r.player == owner) {
                        rival.party.clear_id(id);
                    }
                }
                // An Amiga state belongs to a seat of a v1.2 game, and to one
                // seat only.
                for (player, ai) in core::mem::take(&mut self.amigas) {
                    match rivals.iter_mut().find(|r| r.player == player) {
                        Some(rival) if rival.classic.is_none() && !identity.is_legacy() => rival.amiga = Some(ai),
                        _ => return Err(SaveError::Inconsistent("an amiga state with no v1.2 seat for it".into())),
                    }
                }
                for (player, ai) in core::mem::take(&mut self.moderns) {
                    let ai = ai.ok_or_else(|| SaveError::Inconsistent("a modern state with no settings line".into()))?;
                    match rivals.iter_mut().find(|r| r.player == player) {
                        Some(rival) if rival.classic.is_none() && rival.amiga.is_none() && !identity.is_legacy() => {
                            rival.modern = Some(ai)
                        }
                        _ => return Err(SaveError::Inconsistent("a modern state with no v1.2 seat for it".into())),
                    }
                }
                rivals
            },
            decided: self.decided,
            goals: self.goals,
            known: core::mem::take(&mut self.known),
            visited: core::mem::take(&mut self.visited),
            cheats: self.cheats,
            companions,
            next_id,
            hired: self.hired,
            fallen: self.fallen,
            identities: self.identities,
            entries: self.entries,
            lying: self.lying,
            standing: self.standing,
            respawns: self.respawns,
            camps: self.camps,
            rooms: self.rooms,
            ground: self.ground,
            springs: self.springs,
            traders: self.traders,
            seat,
            humans: self.humans,
            colours: self.colours,
            bench,
            finished: self.finished.ok_or(SaveError::Truncated)?,
            selected: self.selected.ok_or(SaveError::Truncated)?,
        };
        save.read_fractions(self.version)?;
        check(&save)?;
        // An older v1.2 file still holds what v1.2 does not save.
        save.forget_unserialized();
        Ok(save)
    }

    /// A `bench` builder's lines as the other human.
    fn into_bench(mut self, seat: usize) -> Result<Bench, SaveError> {
        let carried = self.exposure.take().ok_or(SaveError::Truncated)?;
        if carried.len() != 1 + self.companions.len() {
            return Err(SaveError::Inconsistent("an exposure for every member of the group".into()));
        }
        let mut companions = core::mem::take(&mut self.companions);
        for (c, n) in companions.iter_mut().zip(carried.iter().skip(1)) {
            c.exposure = crate::danger::Exposure { carried: *n, lifted: false };
        }
        Ok(Bench {
            seat,
            at_location: self.at_location.ok_or(SaveError::Truncated)?,
            place: self.place.ok_or(SaveError::Truncated)?,
            position: self.position.ok_or(SaveError::Truncated)?,
            return_position: self.return_position,
            stood: self.stood,
            needs: self.needs.ok_or(SaveError::Truncated)?,
            experience: self.experience.ok_or(SaveError::Truncated)?,
            carried: self.carried.take().ok_or(SaveError::Truncated)?,
            kit: self.kit.ok_or(SaveError::Truncated)?,
            exposure: crate::danger::Exposure { carried: carried[0], lifted: false },
            fight_mode: self.fight_mode.ok_or(SaveError::Truncated)?,
            single_mode: self.single_mode.ok_or(SaveError::Truncated)?,
            previous_location: self.previous.ok_or(SaveError::Truncated)?,
            info_mode: self.info_mode.ok_or(SaveError::Truncated)?,
            talked: self.talked.take().ok_or(SaveError::Truncated)?,
            journey: self.journey,
            companions,
            next_id: self.next_id.ok_or(SaveError::Truncated)?,
            selected: self.selected.ok_or(SaveError::Truncated)?,
        })
    }
}

/// The parts agree with each other.
///
/// **A valid index is not enough.** Each of these is a rule the running
/// game maintains, so a file that breaks one describes a world the game
/// could not have been in — and restoring it would produce behaviour that
/// no sequence of play could.
fn holds(pack: &ItemBag, kit: &crate::combat::Kit) -> bool {
    [kit.weapon, kit.protection]
        .into_iter()
        .flatten()
        .all(|slot| pack.get(slot).is_some())
}

pub fn check(save: &Save) -> Result<(), SaveError> {
    check_player(&save.player())?;
    if let Some(b) = &save.bench {
        check_player(b)?;
    }
    // Whom a player has selected is one of theirs.
    for p in core::iter::once(save.player()).chain(save.bench.iter().cloned()) {
        let ok = match p.selected {
            Selection::Leader => true,
            Selection::Companion(id) => p.companions.iter().any(|c| c.id == id),
            Selection::Camper(r) => save.camps.iter().any(|(_, c)| {
                c.owner == Some(p.seat) && c.residents.iter().any(|w| w.origin == Some(r))
            }),
        };
        if !ok {
            return Err(SaveError::Inconsistent("a selection that is not the player's".into()));
        }
    }
    // Each human seat once; the one playing is one of them, and so is the
    // bench — and the bench is not the one playing.
    for (i, h) in save.humans.iter().enumerate() {
        if save.humans[..i].iter().any(|o| o.seat == h.seat) {
            return Err(SaveError::Duplicate("human seat".into()));
        }
    }
    if !save.humans.is_empty() && !save.humans.iter().any(|h| h.seat == save.seat) {
        return Err(SaveError::Inconsistent("the player playing is no human".into()));
    }
    if let Some(b) = &save.bench {
        if b.seat == save.seat {
            return Err(SaveError::Inconsistent("the bench is the player playing".into()));
        }
    }
    if save.humans.len() == 2 && save.bench.is_none() {
        return Err(SaveError::Truncated);
    }
    // Somebody who has finished today is not the one playing today.
    if save.finished.contains(&save.seat) {
        return Err(SaveError::Inconsistent("the player playing has finished".into()));
    }
    // A machine seat is not playing, and not benched.
    if save.rivals.iter().any(|r| r.player == save.seat || save.bench.as_ref().is_some_and(|b| b.seat == r.player)) {
        return Err(SaveError::Inconsistent("a human seat played by the machine".into()));
    }

    // A trader's walk is home, out, home: the counter only ever holds zero
    // or one, because reaching two moves them and clears it in the same
    // breath.
    for t in &save.traders {
        if t.counter > 1 {
            return Err(SaveError::Inconsistent("a trader's counter past its cycle".into()));
        }
        if t.stock.len() > crate::trader::MAX_STOCK {
            return Err(SaveError::Inconsistent("a trader over their stock cap".into()));
        }
    }

    for (location, camp) in &save.camps {
        // `JoinCamp` sets the owner as it puts somebody there, and taking
        // the last one back clears it. So people imply an owner — though
        // not the reverse: the 1993 save gives locations an owner with
        // nobody camped, and that is a legal starting world.
        if !camp.residents.is_empty() && camp.owner.is_none() {
            return Err(SaveError::Inconsistent("a camp with people and no owner".into()));
        }
        let _ = location;
    }

    Ok(())
}

/// What holds for each human player on their own.
fn check_player(save: &Bench) -> Result<(), SaveError> {
    // `Player.Turn` takes a day off the road and arrives when the count
    // reaches zero, so a saved journey always has days left.
    if let Some(j) = &save.journey {
        if j.remaining == 0 || j.remaining > j.total || j.total == 0 {
            return Err(SaveError::Inconsistent("journey days".into()));
        }
        if j.from == j.to {
            return Err(SaveError::Inconsistent("journey to where you are".into()));
        }
        // Travelling, the player stays counted at the place they left until
        // they arrive — `at_location` becomes the destination on arrival and
        // the journey ends in the same breath.
        if save.at_location != j.from {
            return Err(SaveError::Inconsistent("travelling from somewhere else".into()));
        }
        // There is no going inside while on the road: the click that would
        // is refused with "already on the road".
        if save.place != Place::World {
            return Err(SaveError::Inconsistent("travelling from indoors".into()));
        }
    }

    // You can only be inside the place you are at.
    if let Place::Location(n) = save.place {
        if n != save.at_location {
            return Err(SaveError::Inconsistent("inside somewhere else".into()));
        }
    }

    // A drawn weapon is a slot in that character's own pack, so a save
    // cannot name a seventh slot or an empty one. Getting this wrong would
    // arm somebody with whatever later moved into the slot.
    if !holds(&save.carried, &save.kit) {
        return Err(SaveError::Inconsistent("a weapon drawn from an empty slot".into()));
    }
    for who in &save.companions {
        if !holds(&who.pack, &who.kit) {
            return Err(SaveError::Inconsistent("a companion holding nothing".into()));
        }
    }

    Ok(())
}

// -- the small readers -------------------------------------------------------

fn word<'a>(w: Option<&'a str>, what: &str) -> Result<&'a str, SaveError> {
    w.ok_or_else(|| SaveError::BadLine(what.into()))
}

fn expect(w: Option<&str>, want: &str) -> Result<(), SaveError> {
    match w {
        Some(got) if got == want => Ok(()),
        _ => Err(SaveError::BadLine(want.into())),
    }
}

fn num<T: core::str::FromStr>(w: Option<&str>, what: &str) -> Result<T, SaveError> {
    word(w, what)?.parse().map_err(|_| SaveError::BadValue(what.into()))
}

fn hex(w: Option<&str>, what: &str) -> Result<u64, SaveError> {
    u64::from_str_radix(word(w, what)?, 16).map_err(|_| SaveError::BadValue(what.into()))
}

/// A location index, range-checked so a number from a corrupt file cannot
/// be used to size anything.
fn location(w: Option<&str>) -> Result<usize, SaveError> {
    let n: usize = num(w, "location")?;
    if n >= limits::LOCATION {
        return Err(SaveError::TooBig("location".into()));
    }
    Ok(n)
}

fn point<'a>(w: &mut impl Iterator<Item = &'a str>) -> Result<Point, SaveError> {
    Ok(Point { x: num(w.next(), "x")?, y: num(w.next(), "y")? })
}

fn needs<'a>(w: &mut impl Iterator<Item = &'a str>) -> Result<CharacterNeeds, SaveError> {
    Ok(CharacterNeeds {
        food: num(w.next(), "food")?,
        max_food: num(w.next(), "max_food")?,
        water: num(w.next(), "water")?,
        max_water: num(w.next(), "max_water")?,
        health: num(w.next(), "health")?,
    })
}

fn bag<'a>(w: &mut impl Iterator<Item = &'a str>) -> Result<ItemBag, SaveError> {
    let capacity: usize = num(w.next(), "capacity")?;
    if capacity > limits::BAG {
        return Err(SaveError::TooBig("bag".into()));
    }
    let count: usize = num(w.next(), "bag length")?;
    if count > capacity {
        return Err(SaveError::TooBig("bag contents".into()));
    }
    let mut kinds: Vec<usize> = Vec::new();
    for _ in 0..count {
        kinds.push(num(w.next(), "item")?);
    }
    let mut bag = ItemBag::with(capacity, &kinds);
    for slot in 0..count {
        match word(w.next(), "rounds")? {
            "*" => {}
            rounds => bag.set_rounds_left(
                slot,
                Some(rounds.parse().map_err(|_| SaveError::BadValue("rounds".into()))?),
            ),
        }
    }
    Ok(bag)
}

fn kit<'a>(w: &mut impl Iterator<Item = &'a str>) -> Result<crate::combat::Kit, SaveError> {
    Ok(crate::combat::Kit {
        weapon: maybe(w.next(), "weapon")?,
        protection: maybe(w.next(), "protection")?,
    })
}

fn companion<'a>(w: &mut impl Iterator<Item = &'a str>) -> Result<Companion, SaveError> {
    let id = num(w.next(), "id")?;
    let from: i64 = num(w.next(), "origin")?;
    let origin = (from >= 0).then_some(from as usize);
    let needs = needs(w)?;
    let experience = num(w.next(), "experience")?;
    let pack = bag(w)?;
    let kit = kit(w)?;
    let at = point(w)?;
    let move_to = point(w)?;
    let draws = num(w.next(), "draws")?;
    // Version 23's movement, absent from a version 22 file.
    let (mut stride, mut fellower) = (crate::gait::Stride::default(), crate::mind::Fellower::default());
    if let Some(t) = w.next() {
        stride = crate::gait::Stride::carried(at, (num(Some(t), "fx")?, num(w.next(), "fy")?));
        fellower.radius = num(w.next(), "radius")?;
        let has: u8 = num(w.next(), "dest")?;
        let dest = point(w)?;
        fellower.leader_dest = (has != 0).then_some(dest);
        fellower.delay = num(w.next(), "delay")?;
        fellower.manual = num::<u8>(w.next(), "manual")? != 0;
        fellower.manual_delay = num(w.next(), "manual delay")?;
        fellower.manual_stop = num(w.next(), "manual stop")?;
    }
    // `exposure` is deliberately not in the file: it is the part of a
    // danger's damage not yet worth a whole point, and starting it again
    // after a load costs at most one.
    Ok(Companion {
        id,
        origin,
        needs,
        experience,
        pack,
        exposure: crate::danger::Exposure::default(),
        kit,
        at,
        move_to,
        draws,
        stride,
        fellower,
    })
}

fn rival<'a>(w: &mut impl Iterator<Item = &'a str>) -> Result<crate::rival::Rival, SaveError> {
    let player = num(w.next(), "rival player")?;
    let at = location(w.next())?;
    let experience = num(w.next(), "rival experience")?;
    let needs = needs(w)?;
    let pack = bag(w)?;
    let mode = match word(w.next(), "rival mode")? {
        "waiting" => crate::rival::Mode::Waiting,
        "expanding" => crate::rival::Mode::Expanding,
        "hiring" => crate::rival::Mode::Hiring,
        _ => return Err(SaveError::BadValue("rival mode".into())),
    };
    let wait = num(w.next(), "rival wait")?;
    let next_id = num(w.next(), "rival next-id")?;
    let dead = match word(w.next(), "rival state")? {
        "dead" => true,
        "alive" => false,
        _ => return Err(SaveError::BadValue("rival state".into())),
    };
    if word(w.next(), "headed")? != "headed" {
        return Err(SaveError::BadLine("rival headed".into()));
    }
    let headed = match word(w.next(), "headed where")? {
        "none" => None,
        n => Some(n.parse().map_err(|_| SaveError::BadValue("headed where".into()))?),
    };
    if word(w.next(), "road")? != "road" {
        return Err(SaveError::BadLine("rival road".into()));
    }
    let journey = match word(w.next(), "road where")? {
        "none" => None,
        from => Some(Journey {
            from: from.parse().map_err(|_| SaveError::BadValue("road from".into()))?,
            to: location(w.next())?,
            total: num(w.next(), "road total")?,
            remaining: num(w.next(), "road remaining")?,
        }),
    };
    if word(w.next(), "previous")? != "previous" {
        return Err(SaveError::BadLine("rival previous".into()));
    }
    let previous = match word(w.next(), "previous where")? {
        "none" => None,
        n => Some(n.parse().map_err(|_| SaveError::BadValue("previous where".into()))?),
    };
    if word(w.next(), "classic")? != "classic" {
        return Err(SaveError::BadLine("rival classic".into()));
    }
    let classic = match word(w.next(), "classic mode")? {
        "none" => None,
        m => {
            let mode = crate::classic_ai::Mode::from_name(m)
                .ok_or_else(|| SaveError::BadValue("classic mode".into()))?;
            let wait = num(w.next(), "classic wait")?;
            let headed = match word(w.next(), "classic headed")? {
                "none" => None,
                n => Some(n.parse().map_err(|_| SaveError::BadValue("classic headed".into()))?),
            };
            let n: usize = num(w.next(), "classic pool")?;
            if n > crate::classic_ai::FILTER.len() {
                return Err(SaveError::BadValue("classic pool".into()));
            }
            let mut pool = crate::classic_ai::Pool::default();
            for _ in 0..n {
                let id = word(w.next(), "pool item")?;
                if !crate::classic_ai::FILTER.contains(&id) {
                    return Err(SaveError::BadValue("pool item".into()));
                }
                let count = num(w.next(), "pool count")?;
                pool.items.push((id.to_string(), count));
            }
            Some(crate::classic_ai::ClassicAi { mode, wait, headed, pool })
        }
    };
    Ok(crate::rival::Rival {
        player,
        at,
        journey,
        headed,
        mode,
        wait,
        experience,
        needs,
        pack,
        // The group is read from the `rival-mate` lines that follow and
        // attached when the file is finished; a rival on its own has none.
        party: crate::game::Party::restored(Vec::new(), next_id),
        dead,
        classic,
        previous,
        amiga: None,
        modern: None,
        // `weapon KIND` at the line's end, since version 23.
        weapon_kind: match w.next() {
            Some("weapon") => Some(num(w.next(), "rival weapon")?),
            _ => None,
        },
    })
}

fn maybe(w: Option<&str>, what: &str) -> Result<Option<usize>, SaveError> {
    match word(w, what)? {
        "none" => Ok(None),
        n => Ok(Some(n.parse().map_err(|_| SaveError::BadValue(what.into()))?)),
    }
}

fn cap(have: usize, most: usize, what: &str) -> Result<(), SaveError> {
    if have >= most {
        return Err(SaveError::TooBig(what.into()));
    }
    Ok(())
}
