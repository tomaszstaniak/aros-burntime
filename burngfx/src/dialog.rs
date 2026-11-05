//! Conversations as v0.9.1 holds them: `Dialog.GetConversation` choosing what
//! is said and offered, and `DialogWindow` paging through it.
//!
//! What is *said* is a section of the speaker's `MEN_NNN.TXT` —
//! `men_NNN?sK`: the lines after the K-th `}#` up to the next one. Which
//! file is the speaker's `TextId` in `gam.dat` (`Dialog.MenFile`); somebody
//! who works for another player speaks from `men_510`. What is *offered* is
//! three lines of `BURN.TXT` with an action each.

use alloc::string::String;
use alloc::vec::Vec;

/// `ConversationType`, the ones a click on somebody can lead to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Talk,
    Greeting,
    Hire,
    Profession,
}

/// `ConversationActionType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Talk,
    Trade,
    Exit,
    HireRequirements,
    Hire,
    Profession,
    /// Yes to building the thing that was offered. `GetConstruction` puts
    /// this line in only when the parts are all there.
    Construct,
    /// A confirmation's two answers (`ConversationActionType.Yes`, `.No`).
    Yes,
    No,
}

/// One of the three lines offered: `burn?N`, or nothing. `@burn?498`
/// ("Would you take |C?") has the payment put in for `|C`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    pub burn: Option<u32>,
    pub action: Action,
}

const NOTHING: Choice = Choice { burn: None, action: Action::None };

/// What the speaker makes of the question about work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HireAnswer {
    /// Experienced enough, and the pack holds something they take: section 4,
    /// with that thing named.
    Accepts(usize),
    /// Experienced enough, nothing they take: section 5, naming the first
    /// thing they would.
    WantsSomething(usize),
    /// `boss.Experience < Parent.Experience * 0.66`: section 7.
    TooInexperienced,
}

/// Who is being spoken to, as far as choosing a conversation needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Speaker {
    pub record: usize,
    pub men_file: u16,
    pub is_trader: bool,
    /// Not a mutant and not a dog: somebody who can be asked about work.
    pub can_work: bool,
    /// Works for a player other than the one asking: `men_510`, and no offer.
    pub others: bool,
    /// The one asking is the player's own character (`boss.IsPlayerCharacter`).
    /// A companion who speaks is offered neither trade nor work.
    pub by_leader: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub file: u16,
    pub section: u8,
    pub choices: [Choice; 3],
    /// The item type named for `|C`, in the text and in `burn?498`.
    pub item: Option<usize>,
    /// `|B`: the speaker's profession, `burn?(40 + class)`, for sections 4–7.
    pub substitutes: bool,
}

/// `Constructions.GetConstruction`'s conversation.
///
/// Always section 3 — the original's own `int index = 3; // TODO` — of the
/// `MEN_5NN.TXT` the recipe's `dialog=` names. The Yes line, `burn?501`
/// ("OK, make me |I."), is there only when the parts are all to hand; the No
/// line, `burn?503`, always is.
///
/// The text's `|I` is the thing that would be made and `|E`, `|F`, `|G`,
/// `|H` are the recipe's items in the file's order — which is why reordering
/// them at `v1.1` changed what the protective suit's line reads. The
/// substitution is positional and the lines do not all use `|E`, so a recipe
/// whose first item is not the main one loses it from the sentence.
pub fn construction(file: u16, can_build: bool) -> Conversation {
    Conversation {
        file,
        section: 3,
        choices: [
            NOTHING,
            if can_build {
                Choice { burn: Some(501), action: Action::Construct }
            } else {
                NOTHING
            },
            Choice { burn: Some(503), action: Action::Exit },
        ],
        item: None,
        substitutes: false,
    }
}

/// `Dialog.talkedPlayer` for the one human there is, and the static
/// `lastDialog` that decides between "hello" and "hello again".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    /// Everybody the player has spoken to.
    pub talked: Vec<usize>,
    /// Whose dialog was last opened. Not saved: a static in the original.
    pub last: Option<usize>,
}

/// The first two choices every kind of conversation shares
/// (`GetGreetingConversation`, `GetTalkConversation`): trade and talk for a
/// trader, talk and "about work" for somebody who can work and has room in
/// the group, only talk otherwise.
fn usual_choices(s: &Speaker, group: usize) -> [Choice; 2] {
    let foreign = s.others || !s.by_leader;
    if s.is_trader && !foreign {
        [Choice { burn: Some(500), action: Action::Trade }, Choice { burn: Some(492), action: Action::Talk }]
    } else if s.can_work && !foreign && group < 5 {
        [Choice { burn: Some(492), action: Action::Talk },
         Choice { burn: Some(491), action: Action::HireRequirements }]
    } else {
        [NOTHING, Choice { burn: Some(492), action: Action::Talk }]
    }
}

const EXIT: Choice = Choice { burn: Some(490), action: Action::Exit };

/// `Dialog.GetConversation(boss, type)`. `group` is the asking player's
/// group size, leader included; `hire` is asked only for [`Kind::Hire`] and
/// [`Kind::Profession`].
pub fn conversation(m: &mut Memory, s: &Speaker, kind: Kind, group: usize,
                    hire: impl FnOnce() -> HireAnswer) -> Conversation {
    // `index` for a greeting is decided before `lastDialog` moves on.
    let greeting_index = if m.last == Some(s.record) { 3 } else { 2 };
    m.last = Some(s.record);
    let mut kind = kind;
    // "change conversation to talk if met the first time"
    if !m.talked.contains(&s.record) {
        m.talked.push(s.record);
        kind = Kind::Talk;
    }
    let file = if s.others { 510 } else { s.men_file };
    match kind {
        Kind::Greeting | Kind::Talk => {
            let [a, b] = usual_choices(s, group);
            Conversation {
                file,
                section: if kind == Kind::Greeting { greeting_index } else { 0 },
                choices: [a, b, EXIT],
                item: None,
                substitutes: false,
            }
        }
        Kind::Hire | Kind::Profession => {
            // Sections 4–7 are the speaker's own file, even for somebody
            // else's man: `men_510` is only for the greeting and the talk.
            let (section, item, offered) = if kind == Kind::Profession {
                match hire() {
                    HireAnswer::Accepts(k) => (6, Some(k), true),
                    // `Profession` is only offered after an Accepts; if the
                    // pack changed meanwhile the original would fail here.
                    HireAnswer::WantsSomething(k) => (6, Some(k), true),
                    HireAnswer::TooInexperienced => (6, None, true),
                }
            } else {
                match hire() {
                    HireAnswer::Accepts(k) => (4, Some(k), true),
                    HireAnswer::WantsSomething(k) => (5, Some(k), false),
                    HireAnswer::TooInexperienced => (7, None, false),
                }
            };
            let choices = if s.is_trader {
                [Choice { burn: Some(500), action: Action::Trade },
                 Choice { burn: Some(492), action: Action::Talk }, EXIT]
            } else if s.can_work {
                if offered {
                    [Choice { burn: Some(497), action: Action::Profession },
                     Choice { burn: Some(498), action: Action::Hire }, EXIT]
                } else {
                    [Choice { burn: Some(492), action: Action::Talk },
                     Choice { burn: Some(491), action: Action::HireRequirements }, EXIT]
                }
            } else {
                [NOTHING, Choice { burn: Some(492), action: Action::Talk }, EXIT]
            };
            Conversation { file: s.men_file, section, choices, item, substitutes: true }
        }
    }
}

/// `ResourceManager.GetStrings("men_NNN?sK")`: the lines after the K-th line
/// that is exactly `}#`, up to the next such line, each with its `}`s taken
/// out. Empty lines count.
pub fn section(text: &str, k: usize) -> Vec<String> {
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut start = 0;
    let mut seen = 0;
    for (i, l) in lines.iter().enumerate() {
        if seen >= k {
            break;
        }
        if *l == "}#" {
            seen += 1;
            start = i + 1;
        }
    }
    if seen < k {
        return Vec::new();
    }
    let end = lines[start..].iter().position(|l| *l == "}#").map_or(start, |p| start + p);
    lines[start..end].iter().map(|l| l.replace('}', "")).collect()
}

/// `DialogWindow`'s paging: two lines at a time, then the choices. With
/// fewer than three lines the choices show at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pages {
    pub offset: usize,
    /// `dialogmode`: false while there is more text ("…" is offered).
    pub choosing: bool,
}

impl Pages {
    /// `dialogmode = (Text.Length < 3) ? 1 : 0` when a dialog opens.
    pub fn open(lines: usize) -> Pages {
        Pages { offset: 0, choosing: lines < 3 }
    }

    /// After a choice: `(Text.Length <= 3) ? 1 : 0`. The original uses `<=`
    /// here and `<` on opening, so three lines show at once after a choice
    /// but are paged when the dialog first opens.
    pub fn after_choice(lines: usize) -> Pages {
        Pages { offset: 0, choosing: lines <= 3 }
    }

    /// The "more" line was clicked.
    pub fn more(&mut self, lines: usize) {
        self.offset += 2;
        if self.offset + 2 >= lines {
            self.choosing = true;
        }
    }
}

/// `GetDismissConversation`: `men_502?s3`, then "no" (`burn?495`) and "yes"
/// (`burn?496`). Asked when the selected one is the last in his camp.
pub fn dismiss() -> Conversation {
    confirmation(502, 495, 496)
}

/// `GetAbandonConversation`: `men_501?s3`, "no" `burn?494`, "yes"
/// `burn?505`. Asked when REJOIN would take the last one out of a camp.
pub fn abandon() -> Conversation {
    confirmation(501, 494, 505)
}

/// `GetCaptureConversation`: `men_500?s3` and one answer, `burn?490`, which
/// closes it. STAY THERE on another player's ground: nothing else happens.
pub fn capture() -> Conversation {
    Conversation {
        file: 500,
        section: 3,
        choices: [NOTHING, NOTHING, Choice { burn: Some(490), action: Action::Exit }],
        item: None,
        substitutes: false,
    }
}

fn confirmation(file: u16, no: u32, yes: u32) -> Conversation {
    Conversation {
        file,
        section: 3,
        choices: [
            NOTHING,
            Choice { burn: Some(no), action: Action::No },
            Choice { burn: Some(yes), action: Action::Yes },
        ],
        item: None,
        substitutes: false,
    }
}
