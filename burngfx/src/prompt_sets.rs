//! The prompts each v1.2 scene registers, built from what the scene knows
//! this frame. [`crate::prompts`] resolves and lays them out; this module is
//! the per-scene part: which collections, which windows, which conditions —
//! `Prompts.Add`, `AddDynamic` and `SuppressWhen` in the scenes'
//! constructors (`Burntime.Remaster/Scenes/*.cs`), with the conditions and
//! selectors already evaluated, since the port has no window tree to ask.
//!
//! Each builder takes a small state of facts the game has (what is under
//! the pointer or the keys' selection, whether a dialog is open) and returns
//! a [`Set`]: the [`Scene`], its colour scheme and its preferred primary key.
//! The selectors themselves (`GetEnterOrTravelPrompt`,
//! `GetDirectInteractionPrompt`, …) are here too, so that a test can check
//! the set a state gives against the C#.

use alloc::vec;
use alloc::vec::Vec;

use crate::input::{Action, Key};
use crate::prompts::{
    Align, Chord, Collection, Entry, Inline, Label, Mode, Mouse, Pattern, Prompt, Role, Scene, Scheme, Win, PREFER_ENTER,
    PREFER_SPACE,
};

/// What a scene's overlay draws from: its prompts, `InputPromptOverlay`'s
/// colour scheme and `Scene.PreferredPrimaryKeyboardControl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Set {
    pub scene: Scene,
    pub scheme: Scheme,
    pub preferred: Chord,
}

const fn p(n: u16) -> Label {
    Label::Prompts(n)
}

fn keys(mode: Mode) -> bool {
    matches!(mode, Mode::Keyboard | Mode::Gamepad)
}

fn win(entries: Vec<Entry>, visible: bool, active: bool) -> Win {
    Win { prompts: Collection::of(entries), visible, active, ..Win::default() }
}

/// A button with an inline prompt beside it: `Prompts.Add(action, "",
/// new Vector2(Size.x + 2, -2))`. `at` is the button's position in the
/// scene, `w` its width.
fn button_inline(action: Action, at: (i32, i32), w: i32, visible: bool) -> Win {
    prompt_inline(Prompt::action(action, Label::Text("")), at, w, visible)
}

/// The ACCEPT and service buttons' Action prompt: `MouseControl =
/// MouseButton.None, TouchControl = None` (`TraderScene.cs:127-132`,
/// `ServiceScene.cs:104-109`, upstream `2f6502e` "don't show long press for
/// accept deal button"). The button is clicked, not bound to a mouse button.
pub const ACCEPT_ACTION: Prompt = Prompt::action(Action::Action, Label::Text("")).with_mouse(Mouse::None);

fn prompt_inline(prompt: Prompt, at: (i32, i32), w: i32, visible: bool) -> Win {
    Win {
        prompts: Collection::of(vec![Entry::inline(prompt, true, Inline::at(w + 2, -2))]),
        visible,
        active: false,
        at,
        children: Vec::new(),
    }
}

/// `UpdateCharacterPromptPositions` (`MapScene.cs:507-520`,
/// `LocationScene.cs:651-666`): either side of the portrait, on the
/// bottom margin's baseline.
pub fn character_prompt_positions(game: (i32, i32)) -> ((i32, i32), (i32, i32)) {
    let left = game.0 / 2 - 31;
    let baseline = game.1 - 6;
    ((left - 2, baseline), (left + 68 + 2, baseline))
}

fn character_prompts(game: (i32, i32), shown: bool) -> [Entry; 2] {
    let (prev, next) = character_prompt_positions(game);
    [
        Entry::inline(
            Prompt::action(Action::LeftArea, Label::Text("")),
            shown,
            Inline::at(prev.0, prev.1).aligned(Align::Right, Align::Right),
        ),
        Entry::inline(
            Prompt::action(Action::RightArea, Label::Text("")),
            shown,
            Inline::at(next.0, next.1).aligned(Align::Left, Align::Right).separated(" "),
        ),
    ]
}

/// `new InputPrompt(InputAction.Back, "...") { MouseControl = Right }`.
pub const BACK_DOTS: Prompt = Prompt::action(Action::Back, Label::Text("...")).with_mouse(Mouse::Right);

// ---------------------------------------------------------------------------
// MapScene (`MapScene.cs:115-149`, the selectors `:470-625`).

/// The place a map prompt is about (`PromptLocation`): the hovered entrance
/// with the mouse, the keys' selection otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapPlace {
    /// It is where the player is.
    pub own: bool,
    /// A neighbour he can travel to (`Neighbors.Contains`, `CanTravel`).
    pub travel: bool,
    /// `CanShowInfo`.
    pub info: bool,
    /// `RadioIntel.IsAvailable`.
    pub radio: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapState {
    pub mode: Mode,
    /// A dialog, the menu or the manual is open (`SuppressWhen`).
    pub suppressed: bool,
    /// `Party.Count`: the leader and his companions.
    pub party: usize,
    pub info_mode: bool,
    /// `None` when there is no valid prompt location.
    pub place: Option<MapPlace>,
    /// `Resolution.Game`.
    pub game: (i32, i32),
}

pub const MAP_ENTER: Prompt = Prompt::action(Action::Primary, p(26));
pub const MAP_TRAVEL: Prompt = Prompt::action(Action::Action, p(25)).with_mouse(Mouse::Left);

/// `LocationInfoPrompt`.
pub const fn map_info(label: u16, mouse: Mouse) -> Prompt {
    Prompt::action(Action::Secondary, p(label)).with_mouse(mouse)
}

/// `GetLocationInfoPrompt(mouseButton)`.
pub fn map_info_prompt(s: &MapState, button: Mouse) -> Option<Prompt> {
    let place = s.place?;
    if s.mode == Mode::Mouse {
        let expected = if s.info_mode { Mouse::Left } else { Mouse::Right };
        if button != expected {
            return None;
        }
    }
    place.info.then(|| map_info(if place.radio { 43 } else { 27 }, button))
}

/// `GetEnterOrTravelPrompt`.
pub fn map_enter_or_travel(s: &MapState) -> Option<Prompt> {
    let place = s.place?;
    if s.mode == Mode::Mouse && s.info_mode {
        return None;
    }
    if place.own {
        Some(MAP_ENTER)
    } else if place.travel {
        Some(MAP_TRAVEL)
    } else {
        None
    }
}

pub fn map(s: &MapState) -> Set {
    let view = win(
        vec![
            Entry::dynamic(&[map_info(27, Mouse::Left), map_info(43, Mouse::Left)], map_info_prompt(s, Mouse::Left)),
            Entry::dynamic(&[map_info(27, Mouse::Right), map_info(43, Mouse::Right)], map_info_prompt(s, Mouse::Right)),
            Entry::dynamic_at(Role::Primary, &[MAP_ENTER, MAP_TRAVEL], map_enter_or_travel(s)),
        ],
        true,
        // `MapView.IsPromptActive` is always true (`MapView.cs:64`).
        true,
    );
    let [prev, next] = character_prompts(s.game, keys(s.mode) && s.party > 1);
    let mut prompts = Collection::of(vec![Entry::add(BACK_DOTS, true), prev, next]);
    prompts.suppressed = s.suppressed;
    Set { scene: Scene { prompts, children: vec![view] }, scheme: Scheme::Hud, preferred: PREFER_SPACE }
}

// ---------------------------------------------------------------------------
// LocationScene (`LocationScene.cs:110-149`, the selectors `:566-671`).

/// The map object a location prompt is about: `HoveredObject` with the
/// mouse, the nearby target (`nearbyAction.Object`) at the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Object {
    None,
    /// A `DroppedItem`.
    Item,
    /// A character of the player's (`Player == view.Player`): the leader, a
    /// companion, a camper. `selected`: the overlay's selected character.
    Own { selected: bool },
    /// Anybody else.
    Other { dog: bool, in_party: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocationState {
    pub mode: Mode,
    pub suppressed: bool,
    /// `interactionMode == Fight` (the port has no `Auto`; v1.2 never sets it).
    pub fight_mode: bool,
    pub is_city: bool,
    /// An entrance: `ActiveEntrance >= 0` with the mouse,
    /// `nearbyAction.EntranceNumber != -1` at the keys.
    pub entrance: bool,
    pub object: Object,
    /// `combatRecovery > 0` or an encounter running.
    pub combat_busy: bool,
    /// `CanShowCharacterPrompts`' second half: more than one in the party, or
    /// a selected character outside it (a camper).
    pub characters: bool,
    /// `HasGroupMenuCommands`.
    pub group_menu: bool,
    pub game: (i32, i32),
}

/// `GetDirectInteractionPrompt`: the label of the Primary prompt.
pub fn location_direct(s: &LocationState) -> Option<u16> {
    let talkable = |dog: bool, in_party: bool| !dog && !in_party;
    match s.mode {
        Mode::Mouse => {
            if s.entrance && !s.fight_mode {
                return Some(26);
            }
            match s.object {
                Object::Item => Some(23),
                Object::Own { selected } if !s.fight_mode => Some(if selected { 41 } else { 31 }),
                Object::Other { dog, in_party } if !s.fight_mode && talkable(dog, in_party) => Some(34),
                _ => None,
            }
        }
        Mode::Keyboard | Mode::Gamepad => {
            if s.entrance {
                return Some(26);
            }
            match s.object {
                Object::Item => Some(23),
                Object::Own { .. } => Some(31),
                Object::Other { dog, in_party } if talkable(dog, in_party) => Some(34),
                _ => None,
            }
        }
    }
}

const fn fight(mouse: Mouse) -> Prompt {
    Prompt::action(Action::Action, p(38)).with_mouse(mouse)
}

/// `GetFightPrompt`: `CanFightCharacter` is a character not the player's
/// in a place that is no city. With no `Auto` mode the mouse's button is
/// always the left one, and the mouse prompt wants Fight mode.
pub fn location_fight(s: &LocationState) -> Option<Prompt> {
    if s.combat_busy || s.is_city || !matches!(s.object, Object::Other { .. }) {
        return None;
    }
    match s.mode {
        Mode::Mouse => s.fight_mode.then_some(fight(Mouse::Left)),
        Mode::Keyboard | Mode::Gamepad => Some(fight(Mouse::Left)),
    }
}

pub fn location(s: &LocationState) -> Set {
    const LABELS: [Label; 5] = [p(26), p(23), p(41), p(31), p(34)];
    let view = win(
        vec![
            Entry::dynamic_label(Action::Primary, &LABELS, location_direct(s).map(p)),
            Entry::dynamic(&[fight(Mouse::Left), fight(Mouse::Right)], location_fight(s)),
        ],
        true,
        true,
    );
    let [prev, next] = character_prompts(s.game, keys(s.mode) && s.characters);
    let (_, at) = character_prompt_positions(s.game);
    let group = Entry::inline(
        Prompt::action(Action::Secondary, Label::Text("")),
        keys(s.mode) && s.group_menu,
        Inline::at(at.0, at.1).aligned(Align::Left, Align::Right).separated(" "),
    );
    // `CanShowActionsMenuPrompt`: only an `Auto` fight target hides Back.
    let mut prompts = Collection::of(vec![Entry::add(BACK_DOTS, true), prev, next, group]);
    prompts.suppressed = s.suppressed;
    Set { scene: Scene { prompts, children: vec![view] }, scheme: Scheme::Hud, preferred: PREFER_SPACE }
}

// ---------------------------------------------------------------------------
// MenuScene, the start screen (`MenuScene.cs:155-204`, `:304-337`, `:674-681`).

/// A button's `IsPromptActive`: hovered with the mouse, selected at the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lit {
    pub hover: bool,
    pub selected: bool,
}

impl Lit {
    fn active(self, mode: Mode) -> bool {
        if mode == Mode::Mouse { self.hover } else { self.selected }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuState {
    pub mode: Mode,
    /// The setup notes are open.
    pub notes_open: bool,
    pub start: Lit,
    pub load: Lit,
    pub exit: Lit,
    pub notes: Lit,
    /// The two name switches, and whether each slot takes part.
    pub players: [Lit; 2],
    pub used: [bool; 2],
}

/// `SetupPlayerPrompt`.
pub const fn setup_player(label: u16) -> Prompt {
    Prompt::action(Action::Primary, p(label)).with_key(Chord::plain(Key::Enter))
}

pub fn menu(s: &MenuState) -> Set {
    let button = |label: Label, lit: Lit| win(vec![Entry::add(Prompt::action(Action::Primary, label), true)], true, lit.active(s.mode));
    let player = |i: usize| {
        let enabled = s.used[i];
        let other = s.used[1 - i];
        let chosen = setup_player(if !enabled { 8 } else if other { 6 } else { 7 });
        win(
            vec![
                Entry::add(Prompt::pattern(Pattern::HorizontalPaging, p(0)), enabled),
                Entry::add(
                    Prompt::action(Action::Secondary, p(1))
                        .with_keyboard_pattern(Pattern::VerticalPaging)
                        .with_mouse(Mouse::None),
                    enabled,
                ),
                Entry::dynamic(&[setup_player(8), setup_player(6), setup_player(7)], Some(chosen)),
            ],
            true,
            s.players[i].active(s.mode),
        )
    };
    let children = vec![
        button(p(2), s.start),
        button(p(3), s.load),
        button(p(4), s.exit),
        button(Label::SetupNotes(3), s.notes),
        player(0),
        player(1),
    ];
    // The scene itself registers nothing; `SuppressWhen(_setupNotes.IsVisible)`.
    let prompts = Collection { suppressed: s.notes_open, ..Collection::default() };
    Set { scene: Scene { prompts, children }, scheme: Scheme::Default, preferred: PREFER_ENTER }
}

// ---------------------------------------------------------------------------
// OptionsScene (`OptionsScene.cs:146-157`).

/// The windows that own Primary "Select": the tab rail and the pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsFocus {
    Tabs,
    Saves,
    Settings,
    GiveUp,
    Jukebox,
    /// The empty page behind Back: no prompt of its own.
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptionsState {
    pub mode: Mode,
    /// The page shown.
    pub page: OptionsFocus,
    /// The tab rail has the focus, else the page.
    pub tabs_focused: bool,
}

pub fn options(s: &OptionsState) -> Set {
    let select = Prompt::action(Action::Primary, p(31));
    let owner = |page: OptionsFocus| {
        let visible = page == OptionsFocus::Tabs || page == s.page;
        let focused = if page == OptionsFocus::Tabs { s.tabs_focused } else { !s.tabs_focused && page == s.page };
        win(vec![Entry::add(select, s.mode != Mode::Mouse)], visible, focused)
    };
    let children = [OptionsFocus::Tabs, OptionsFocus::Saves, OptionsFocus::Settings, OptionsFocus::GiveUp, OptionsFocus::Jukebox]
        .into_iter()
        .map(owner)
        .collect();
    let prompts = Collection::of(vec![
        Entry::add(Prompt::action(Action::Back, p(17)), true),
        Entry::add(Prompt::pattern(Pattern::VerticalPaging, p(30)), true),
    ]);
    Set { scene: Scene { prompts, children }, scheme: Scheme::Options, preferred: PREFER_ENTER }
}

// ---------------------------------------------------------------------------
// Item grids.

/// A grid's state: whether it has a focused item (`ItemGridWindow.
/// IsPromptActive`), whether it is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    pub visible: bool,
    pub focused: bool,
}

/// A button the scene puts an inline prompt beside: its position in the
/// scene and its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Button {
    pub at: (i32, i32),
    pub w: i32,
    pub visible: bool,
}

fn grid_win(g: Grid, entries: Vec<Entry>) -> Win {
    let mut w = win(entries, g.visible, g.focused);
    w.prompts.hide_in_new_gfx = true;
    w
}

// ---------------------------------------------------------------------------
// InventoryScene (`InventoryScene.cs:81-87`, `:301-306`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryState {
    pub suppressed: bool,
    pub pages: usize,
    pub exit: Button,
    pub inventory: Grid,
    /// The room's or the pile's grid, when there is one.
    pub room: Option<Grid>,
    /// `CanTransferSelectedItem` for the grid with the focus.
    pub can_transfer: bool,
}

pub fn inventory(s: &InventoryState) -> Set {
    let transfer = |g: Grid, label: u16| {
        grid_win(g, vec![Entry::add(Prompt::action(Action::Primary, p(label)), g.focused && s.can_transfer)])
    };
    let mut children = vec![
        Win { visible: true, children: vec![transfer(s.inventory, 39)], ..Win::default() },
        button_inline(Action::Back, s.exit.at, s.exit.w, s.exit.visible),
    ];
    if let Some(room) = s.room {
        children.push(transfer(room, 37));
    }
    let mut prompts = Collection::of(vec![Entry::add(Prompt::pattern(Pattern::HorizontalPaging, p(30)), s.pages > 1)]);
    prompts.suppressed = s.suppressed;
    Set { scene: Scene { prompts, children }, scheme: Scheme::Hud, preferred: PREFER_SPACE }
}

// ---------------------------------------------------------------------------
// TraderScene (`TraderScene.cs:101-122`, `:519-552`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraderState {
    pub mode: Mode,
    pub pages: usize,
    pub exit: Button,
    pub accept: Button,
    pub inventory: Grid,
    pub trader: Grid,
    pub exchange_top: Grid,
    pub exchange_bottom: Grid,
    pub temporary: Grid,
    /// The focused item is in the deal (`IsFocusedItemForTrade`), for the
    /// grid with the focus.
    pub for_trade: bool,
    /// The focused item of the player's grid is not in the deal and the
    /// temporary space has room (`CanShowMovePrompt`'s mouse half).
    pub can_park: bool,
}

pub fn trader(s: &TraderState) -> Set {
    // `AddTradeGridPrompts`: Move (Secondary) only on `inventory.Grid`.
    let trade = |g: Grid, player: bool, with_move: bool| {
        let (yes, no) = if player { (40, 36) } else { (44, 37) };
        let active = g.focused.then(|| p(if s.for_trade { yes } else { no }));
        let mut entries = vec![Entry::dynamic_label(Action::Primary, &[p(yes), p(no)], active)];
        if with_move {
            let shown = g.focused && (s.mode != Mode::Mouse || s.can_park);
            entries.push(Entry::add(Prompt::action(Action::Secondary, p(45)), shown));
        }
        grid_win(g, entries)
    };
    let children = vec![
        Win { visible: s.inventory.visible, children: vec![trade(s.inventory, true, true)], ..Win::default() },
        Win { visible: s.trader.visible, children: vec![trade(s.trader, false, false)], ..Win::default() },
        trade(s.exchange_top, false, false),
        trade(s.exchange_bottom, true, false),
        grid_win(s.temporary, vec![Entry::add(Prompt::action(Action::Primary, p(46)), true)]),
        button_inline(Action::Back, s.exit.at, s.exit.w, s.exit.visible),
        prompt_inline(ACCEPT_ACTION, s.accept.at, s.accept.w, s.accept.visible),
    ];
    let prompts = Collection::of(vec![Entry::add(Prompt::pattern(Pattern::HorizontalPaging, p(30)), s.pages > 1)]);
    Set { scene: Scene { prompts, children }, scheme: Scheme::Hud, preferred: PREFER_SPACE }
}

// ---------------------------------------------------------------------------
// ServiceScene (`ServiceScene.cs:79-106`, `:170`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceState {
    pub pages: usize,
    pub exit: Button,
    /// Shown once the service is ready.
    pub action: Button,
    pub offer: Grid,
    pub inventory: Grid,
    /// `CanMoveFocusedItem` for the grid with the focus.
    pub can_move: bool,
}

pub fn service(s: &ServiceState) -> Set {
    let moving = |g: Grid, label: u16| {
        grid_win(g, vec![Entry::add(Prompt::action(Action::Primary, p(label)), g.focused && s.can_move)])
    };
    let children = vec![
        moving(s.offer, 37),
        Win { visible: true, children: vec![moving(s.inventory, 36)], ..Win::default() },
        button_inline(Action::Back, s.exit.at, s.exit.w, s.exit.visible),
        prompt_inline(ACCEPT_ACTION, s.action.at, s.action.w, s.action.visible),
    ];
    let prompts = Collection::of(vec![Entry::add(Prompt::pattern(Pattern::HorizontalPaging, p(16)), s.pages > 1)]);
    Set { scene: Scene { prompts, children }, scheme: Scheme::Hud, preferred: PREFER_SPACE }
}

// ---------------------------------------------------------------------------
// InfoScene (`InfoScene.cs:124-129`, `:207`).

/// INFO's back button (`InfoScene`, `(5, 174)`).
pub const INFO_BACK: (i32, i32) = (5, 174);

pub fn info(german: bool) -> Set {
    let (dx, dy) = if german { (56, 7) } else { (50, 4) };
    let exit = Win {
        prompts: Collection::of(vec![Entry::inline(
            Prompt::action(Action::Back, Label::Text("")),
            true,
            Inline::at(dx, dy).aligned(Align::Left, Align::Center).bare(2),
        )]),
        visible: true,
        active: false,
        at: INFO_BACK,
        children: Vec::new(),
    };
    Set { scene: Scene { prompts: Collection::default(), children: vec![exit] }, scheme: Scheme::Muted, preferred: PREFER_SPACE }
}

// ---------------------------------------------------------------------------
// The item tooltip's prompt and status (`ItemGridTooltip` with
// `InventoryScene.GetItemTooltipDetails`, `InventoryScene.cs:171-283`, and
// `GetItemPrompt`, `:308-376`). The trader's and a service's tooltips have
// no details in the classic look (`GetTradeTooltip`, `GetTransferTooltip`:
// only with `IsNewGfx`).

/// `tooltip.txt` lines the status names (`InventoryScene.cs:16-24`).
pub mod status {
    pub const NEEDS_TECHNICIAN: u16 = 47;
    pub const PLACE_IN_ROOM: u16 = 48;
    pub const UNSUITABLE_CAMP: u16 = 49;
    pub const NOT_HUNGRY: u16 = 70;
    pub const NOT_ENOUGH_WATER: u16 = 71;
    pub const CHECK_MATERIALS: u16 = 72;
    pub const NEEDS_AMMUNITION: u16 = 73;
    pub const SELECTED_TRAP: u16 = 77;
    pub const NOT_THIRSTY: u16 = 94;
}

/// A production tool, as the place sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolFacts {
    /// `ActiveLocationObj.ValidProductions.Contains`.
    pub valid: bool,
    /// The place's production is this one.
    pub selected: bool,
}

/// `EvaluateConstruction` for the focused item, by its inspector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildFacts {
    pub can_build: bool,
    /// `IsConstructionKnown(recipe.Result)`.
    pub known: bool,
    /// The item loads the result (`Type.Loads.Contains`).
    pub loads_weapon: bool,
    /// Everything missing is ammunition.
    pub only_ammunition: bool,
}

/// What the scene knows of the focused item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemFacts {
    /// In the viewer's pack (`inventory.Grid`), else the room's or pile's.
    pub in_inventory: bool,
    /// `InventoryRoom != null`: a room, not a pile on the ground.
    pub in_room: bool,
    /// The room is a water source, and its reserve.
    pub water_source: bool,
    pub reserve: i32,
    pub food: i32,
    pub water: i32,
    /// `CanSupplyGroup`: somebody in range is hungry, thirsty.
    pub hungry: bool,
    pub thirsty: bool,
    /// `Type.Full?.WaterValue`: an empty container's capacity.
    pub capacity: i32,
    /// `IsSelectable` (a weapon or protection) and whether it is the viewer's.
    pub selectable: bool,
    pub equipped: bool,
    pub tool: Option<ToolFacts>,
    /// A recipe uses it, and nobody in the group may build any of them.
    pub in_recipes: bool,
    pub needs_technician: bool,
    /// `None` without a room or pile beside, or without a recipe.
    pub build: Option<BuildFacts>,
}

/// `ItemTooltipDetails`: the prompt, the status (a `tooltip.txt` line),
/// success or muted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Details {
    pub prompt: Option<Prompt>,
    pub status: Option<u16>,
    pub success: bool,
    pub muted: bool,
}

/// `GetItemPrompt`.
pub fn item_prompt(f: &ItemFacts) -> Option<Prompt> {
    if f.food != 0 {
        return f.hungry.then_some(act(Action::Action, 18));
    }
    if f.water != 0 {
        return f.thirsty.then_some(act(Action::Action, 19));
    }
    if f.capacity != 0 {
        return (f.in_inventory && f.water_source && f.reserve >= f.capacity).then_some(act(Action::Action, 24));
    }
    if f.in_inventory && f.selectable {
        return Some(act(Action::Secondary, if f.equipped { 22 } else { 21 }));
    }
    if let (false, true, Some(t)) = (f.in_inventory, f.in_room, f.tool) {
        return (t.valid && !t.selected).then_some(act(Action::Action, 42));
    }
    // A production tool never inspects.
    if f.tool.is_some() {
        return None;
    }
    match f.build {
        Some(b) if b.can_build => Some(act(Action::Secondary, if b.loads_weapon { 3 } else { 20 })),
        _ => Some(act(Action::Secondary, 23)),
    }
}

const fn act(action: Action, label: u16) -> Prompt {
    Prompt::action(action, p(label))
}

/// `GetItemTooltipDetails` in the classic look.
pub fn item_details(f: &ItemFacts) -> Details {
    let mut d = Details { prompt: item_prompt(f), ..Details::default() };
    let set = |d: &mut Details, line: u16, muted: bool| {
        d.status = Some(line);
        d.muted = muted;
    };
    if f.food > 0 || f.water > 0 {
        let can = f.food > 0 && f.hungry || f.water > 0 && f.thirsty;
        if !can {
            d.prompt = None;
            set(&mut d, if f.food > 0 { status::NOT_HUNGRY } else { status::NOT_THIRSTY }, true);
        }
    }
    if f.water == 0 && f.capacity > 0 && f.in_inventory && f.water_source && f.reserve < f.capacity {
        d.prompt = None;
        set(&mut d, status::NOT_ENOUGH_WATER, false);
    }
    if let Some(t) = f.tool {
        if !f.selectable && (f.in_inventory || !f.in_room) {
            set(&mut d, status::PLACE_IN_ROOM, false);
        } else if !f.selectable && !t.valid {
            set(&mut d, status::UNSUITABLE_CAMP, false);
        }
    }
    if f.in_recipes && f.needs_technician && !f.selectable {
        set(&mut d, status::NEEDS_TECHNICIAN, false);
    } else if !f.selectable {
        if let Some(b) = f.build.filter(|b| !b.can_build && b.known) {
            if b.only_ammunition {
                set(&mut d, status::NEEDS_AMMUNITION, false);
            } else {
                d.prompt = Some(Prompt::action(Action::Secondary, Label::Tooltip(status::CHECK_MATERIALS)));
            }
        }
    }
    if let (false, true, Some(ToolFacts { selected: true, .. })) = (f.in_inventory, f.in_room, f.tool) {
        d.prompt = None;
        set(&mut d, status::SELECTED_TRAP, false);
        d.success = true;
    }
    d
}
