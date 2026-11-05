//! `burngfx::prompt_sets`: each v1.2 scene's prompts for given window and
//! input-mode states, resolved (`InputPromptController.Resolve`) and
//! compared with what the C# registers (`Burntime.Remaster/Scenes/*.cs`,
//! cited per test).

use burngfx::input::{Action, Key};
use burngfx::prompt_sets::*;
use burngfx::prompts::*;

const GAME: (i32, i32) = (341, 213);

fn act(action: Action, label: u16) -> Prompt {
    Prompt::action(action, Label::Prompts(label))
}

fn resolve_set(set: &Set, mode: Mode) -> Resolved {
    resolve(&set.scene, Root::Scene, mode, false)
}

/// The shown prompt of each context slot, in row order.
fn context(r: &Resolved) -> Vec<Option<Prompt>> {
    r.context.iter().map(|g| g.active).collect()
}

fn map_state(mode: Mode, place: Option<MapPlace>) -> MapState {
    MapState { mode, suppressed: false, party: 1, info_mode: false, place, game: GAME }
}

const HOME: MapPlace = MapPlace { own: true, travel: false, info: true, radio: false };
const NEXT_DOOR: MapPlace = MapPlace { own: false, travel: true, info: false, radio: false };

/// `MapScene.cs:121-149`: at the keys, Back "..." in the left row; Enter
/// (Primary, `GetEnterOrTravelPrompt`) and the place's info (both
/// `AddDynamic`s answer at the keys; the later, the right button's, wins)
/// in the right one, each slot listing its candidates.
#[test]
fn map_at_the_keys() {
    let set = map(&map_state(Mode::Keyboard, Some(HOME)));
    assert_eq!((set.scheme, set.preferred), (Scheme::Hud, PREFER_SPACE));
    let r = resolve_set(&set, Mode::Keyboard);
    assert_eq!(r.scene, vec![BACK_DOTS]);
    assert_eq!(context(&r), vec![Some(MAP_ENTER), Some(map_info(27, Mouse::Right))]);
    assert_eq!(r.context[0].candidates, vec![MAP_ENTER, MAP_TRAVEL]);
    assert_eq!(r.context[1].candidates.len(), 4);
    // One in the party: no character prompts.
    assert!(r.inline.is_empty());

    // A neighbour with a radio's report: Travel, and Radio (`?43`).
    let radio = MapPlace { radio: true, info: true, ..NEXT_DOOR };
    let r = resolve_set(&map(&map_state(Mode::Keyboard, Some(radio))), Mode::Keyboard);
    assert_eq!(context(&r), vec![Some(MAP_TRAVEL), Some(map_info(43, Mouse::Right))]);
}

/// `CanShowCharacterPrompts` and `UpdateCharacterPromptPositions`
/// (`MapScene.cs:503-520`): LeftArea right-aligned 2 left of the
/// portrait's left (`Game.x / 2 - 31`), RightArea 2 right of its right
/// (+68), on the baseline `Game.y - 6`; never with the mouse.
#[test]
fn map_character_prompts() {
    let s = MapState { party: 2, ..map_state(Mode::Keyboard, None) };
    let r = resolve_set(&map(&s), Mode::Keyboard);
    let at: Vec<((i32, i32), Align, &str)> = r.inline.iter().map(|p| (p.place.at, p.place.h_align, p.place.separator)).collect();
    assert_eq!(at, vec![((170 - 33, 207), Align::Right, "   "), ((170 + 39, 207), Align::Left, " ")]);
    assert_eq!(r.inline[0].prompt.action, Some(Action::LeftArea));
    // No place selected: neither Enter nor info, but the slots keep their
    // candidates.
    assert_eq!(context(&r), vec![None, None]);

    let s = MapState { mode: Mode::Mouse, ..s };
    assert!(resolve_set(&map(&s), Mode::Mouse).inline.is_empty());
}

/// With the mouse (`CanPromptLocationInfo`): the info prompt is the right
/// button's in travel mode, the left button's in info mode, where Enter and
/// Travel are not offered. Back "..." competes for the right button's slot
/// and loses to the view's prompt (`Resolve`, `:300-318`).
#[test]
fn map_with_the_mouse() {
    let r = resolve_set(&map(&map_state(Mode::Mouse, Some(HOME))), Mode::Mouse);
    assert!(r.scene.is_empty());
    assert_eq!(context(&r), vec![Some(MAP_ENTER), Some(map_info(27, Mouse::Right))]);

    let r = resolve_set(&map(&map_state(Mode::Mouse, Some(NEXT_DOOR))), Mode::Mouse);
    assert_eq!(context(&r), vec![Some(MAP_TRAVEL), Some(BACK_DOTS)]);

    let s = MapState { info_mode: true, ..map_state(Mode::Mouse, Some(HOME)) };
    let r = resolve_set(&map(&s), Mode::Mouse);
    assert_eq!(context(&r), vec![Some(map_info(27, Mouse::Left)), Some(BACK_DOTS)]);

    // A dialog, the menu or the manual: nothing (`SuppressWhen`).
    let s = MapState { suppressed: true, ..map_state(Mode::Mouse, Some(HOME)) };
    assert_eq!(resolve_set(&map(&s), Mode::Mouse), Resolved::default());
}

fn location_state(mode: Mode, entrance: bool, object: Object) -> LocationState {
    LocationState {
        mode,
        suppressed: false,
        fight_mode: false,
        is_city: false,
        entrance,
        object,
        combat_busy: false,
        characters: false,
        group_menu: false,
        game: GAME,
    }
}

const STRANGER: Object = Object::Other { dog: false, in_party: false };
const DOG: Object = Object::Other { dog: true, in_party: false };

/// `GetDirectInteractionPrompt` (`LocationScene.cs:566-605`) and
/// `GetFightPrompt` (`:607-630`) at the keys: Talk (`?34`) and Fight
/// (`?38`, Action) for a stranger; a dog only Fight; an item Inspect
/// (`?23`); a door Enter (`?26`); one's own Select (`?31`); no fight in a
/// city.
#[test]
fn location_at_the_keys() {
    let fight = Prompt::action(Action::Action, Label::Prompts(38)).with_mouse(Mouse::Left);
    let r = resolve_set(&location(&location_state(Mode::Keyboard, false, STRANGER)), Mode::Keyboard);
    assert_eq!(r.scene, vec![BACK_DOTS]);
    assert_eq!(context(&r), vec![Some(act(Action::Primary, 34)), Some(fight)]);
    // The Primary slot reserves room for all five labels.
    assert_eq!(r.context[0].candidates.len(), 5);

    let r = resolve_set(&location(&location_state(Mode::Keyboard, false, DOG)), Mode::Keyboard);
    assert_eq!(context(&r), vec![None, Some(fight)]);

    let city = LocationState { is_city: true, ..location_state(Mode::Keyboard, false, STRANGER) };
    assert_eq!(context(&resolve_set(&location(&city), Mode::Keyboard)), vec![Some(act(Action::Primary, 34)), None]);

    for (entrance, object, label) in [
        (false, Object::Item, 23),
        (true, Object::None, 26),
        (false, Object::Own { selected: true }, 31),
    ] {
        let r = resolve_set(&location(&location_state(Mode::Keyboard, entrance, object)), Mode::Keyboard);
        assert_eq!(context(&r)[0], Some(act(Action::Primary, label)));
    }
}

/// With the mouse: the selected character of one's own is Inventory
/// (`?41`), another Select; Talk mode's fight prompt is none, Fight mode's
/// the left button; in Fight mode a door or a stranger gives no Primary.
#[test]
fn location_with_the_mouse() {
    let own = |selected| resolve_set(&location(&location_state(Mode::Mouse, false, Object::Own { selected })), Mode::Mouse);
    assert_eq!(context(&own(true))[0], Some(act(Action::Primary, 41)));
    assert_eq!(context(&own(false))[0], Some(act(Action::Primary, 31)));

    let talk = resolve_set(&location(&location_state(Mode::Mouse, false, STRANGER)), Mode::Mouse);
    // Primary Talk; the right button's slot has Back "..." from the scene.
    assert_eq!(context(&talk), vec![Some(act(Action::Primary, 34)), Some(BACK_DOTS)]);

    let s = LocationState { fight_mode: true, ..location_state(Mode::Mouse, true, STRANGER) };
    let r = resolve_set(&location(&s), Mode::Mouse);
    // Fight with the left button takes the Primary slot.
    assert_eq!(context(&r)[0], Some(Prompt::action(Action::Action, Label::Prompts(38)).with_mouse(Mouse::Left)));
}

/// `groupActionsPrompt` (`LocationScene.cs:143-148`): Secondary "" at the
/// next-character position, so it shares RightArea's row.
#[test]
fn location_group_actions_share_a_row() {
    let s = LocationState { characters: true, group_menu: true, ..location_state(Mode::Keyboard, false, Object::None) };
    let r = resolve_set(&location(&s), Mode::Keyboard);
    assert_eq!(r.inline.len(), 3);
    assert_eq!(r.inline[1].place, r.inline[2].place);
    assert_eq!(r.inline[2].prompt.action, Some(Action::Secondary));
}

fn menu_state(mode: Mode) -> MenuState {
    MenuState {
        mode,
        notes_open: false,
        start: Lit::default(),
        load: Lit::default(),
        exit: Lit::default(),
        notes: Lit::default(),
        players: [Lit::default(); 2],
        used: [false; 2],
    }
}

/// `MenuScene.cs:155-204,322-334`: a button's Primary while hovered (the
/// mouse) or selected (the keys); a name switch's Face (HorizontalPaging),
/// Color (Secondary; Shift+Up/Down at the keys, none with the mouse) and
/// Enable/Disable/Change name with Enter; nothing while the notes are open.
#[test]
fn the_start_screen() {
    let set = menu(&MenuState { start: Lit { hover: true, selected: false }, ..menu_state(Mode::Mouse) });
    assert_eq!((set.scheme, set.preferred), (Scheme::Default, PREFER_ENTER));
    assert_eq!(context(&resolve_set(&set, Mode::Mouse)), vec![Some(act(Action::Primary, 2))]);

    // Selected at the keys, hovered by nobody.
    let s = MenuState {
        players: [Lit { hover: false, selected: true }, Lit::default()],
        used: [true, false],
        ..menu_state(Mode::Keyboard)
    };
    let r = resolve_set(&menu(&s), Mode::Keyboard);
    let face = Prompt::pattern(Pattern::HorizontalPaging, Label::Prompts(0));
    let colour = act(Action::Secondary, 1).with_keyboard_pattern(Pattern::VerticalPaging).with_mouse(Mouse::None);
    // Navigation first: Extended, Secondary, Primary (`Resolve`, `:289-293`).
    assert_eq!(context(&r), vec![Some(face), Some(colour), Some(setup_player(7))]);
    assert_eq!(setup_player(7).key, Some(Chord::plain(Key::Enter)));

    // The second slot in use too: Disable (`?6`); an unused one: Enable.
    let both = MenuState { used: [true, true], ..s };
    assert_eq!(context(&resolve_set(&menu(&both), Mode::Keyboard))[2], Some(setup_player(6)));
    // Face and Color keep their slots (an `Add` with a false condition is
    // still a candidate), shown by nobody.
    let off = MenuState { used: [false, false], ..s };
    assert_eq!(context(&resolve_set(&menu(&off), Mode::Keyboard)), vec![None, None, Some(setup_player(8))]);

    // NOTES' own label, `@setupnotes?3`.
    let notes = MenuState { notes: Lit { hover: false, selected: true }, ..menu_state(Mode::Keyboard) };
    assert_eq!(context(&resolve_set(&menu(&notes), Mode::Keyboard)), vec![None, None, Some(Prompt::action(Action::Primary, Label::SetupNotes(3)))]);

    let closed = MenuState { notes_open: true, ..notes };
    assert_eq!(resolve_set(&menu(&closed), Mode::Keyboard), Resolved::default());
}

/// `OptionsScene.cs:146-157`: Back (`?17`) and Page (`?30`, Shift+Up/Down)
/// at the bottom left; Select (`?31`) for the focused owner, not with the
/// mouse. With the mouse nothing is shown: neither scene prompt has a
/// button, and Select only keeps the left button's slot.
#[test]
fn options_scene() {
    let s = OptionsState { mode: Mode::Keyboard, page: OptionsFocus::Saves, tabs_focused: true };
    let set = options(&s);
    assert_eq!((set.scheme, set.preferred), (Scheme::Options, PREFER_ENTER));
    let r = resolve_set(&set, Mode::Keyboard);
    assert_eq!(r.scene, vec![act(Action::Back, 17), Prompt::pattern(Pattern::VerticalPaging, Label::Prompts(30))]);
    assert_eq!(context(&r), vec![Some(act(Action::Primary, 31))]);

    let page = OptionsState { tabs_focused: false, ..s };
    assert_eq!(context(&resolve_set(&options(&page), Mode::Keyboard)), vec![Some(act(Action::Primary, 31))]);

    let mouse = OptionsState { mode: Mode::Mouse, ..s };
    let r = resolve_set(&options(&mouse), Mode::Mouse);
    assert_eq!((r.scene.len(), context(&r)), (0, vec![None]));
}

const EXIT: Button = Button { at: (25, 183), w: 20, visible: true };

/// `InventoryScene.cs:81-87,301-306`: Back inline right of EXIT (its width
/// + 2, 2 up); Page while the group has more than one page; Store (`?39`)
/// from the pack, Take (`?37`) from the room when the other grid has room;
/// the grids' prompts hidden in NewGfx.
#[test]
fn inventory_scene() {
    let focused = Grid { visible: true, focused: true };
    let s = InventoryState {
        suppressed: false,
        pages: 2,
        exit: EXIT,
        inventory: focused,
        room: Some(Grid { visible: true, focused: false }),
        can_transfer: true,
    };
    let r = resolve_set(&inventory(&s), Mode::Keyboard);
    assert_eq!(r.scene, vec![Prompt::pattern(Pattern::HorizontalPaging, Label::Prompts(30))]);
    assert_eq!(context(&r), vec![Some(act(Action::Primary, 39))]);
    assert_eq!(r.inline.len(), 1);
    assert_eq!((r.inline[0].place.at, r.inline[0].prompt.action), ((25 + 22, 181), Some(Action::Back)));

    let room = InventoryState { inventory: Grid { visible: true, focused: false }, room: Some(focused), pages: 1, ..s };
    let r = resolve_set(&inventory(&room), Mode::Mouse);
    // With the mouse: Take by the left button; no Page, no inline Back.
    assert_eq!((r.scene.len(), r.inline.len()), (0, 0));
    assert_eq!(context(&r), vec![Some(act(Action::Primary, 37))]);

    let full = InventoryState { can_transfer: false, ..room };
    assert_eq!(context(&resolve_set(&inventory(&full), Mode::Keyboard)), vec![None]);
    assert!(resolve(&inventory(&room).scene, Root::Scene, Mode::Keyboard, true).context.is_empty());

    let dialog = InventoryState { suppressed: true, ..s };
    assert_eq!(resolve_set(&inventory(&dialog), Mode::Keyboard).scene, vec![]);
}

/// `TraderScene.cs:101-122,519-552`: the player's grid Offer (`?36`) or
/// Take back (`?40`) and Put away (`?45`, Secondary); the trader's Take
/// (`?37`) or Put back (`?44`); the temporary space Take back (`?46`);
/// Back and Action inline right of EXIT and ACCEPT.
#[test]
fn trader_scene() {
    let off = Grid { visible: true, focused: false };
    let on = Grid { visible: true, focused: true };
    let s = TraderState {
        mode: Mode::Keyboard,
        pages: 1,
        exit: EXIT,
        accept: Button { at: (170, 183), w: 40, visible: true },
        inventory: on,
        trader: off,
        exchange_top: off,
        exchange_bottom: off,
        temporary: off,
        for_trade: false,
        can_park: true,
    };
    let r = resolve_set(&trader(&s), Mode::Keyboard);
    assert_eq!(context(&r), vec![Some(act(Action::Primary, 36)), Some(act(Action::Secondary, 45))]);
    let actions: Vec<Option<Action>> = r.inline.iter().map(|p| p.prompt.action).collect();
    assert_eq!(actions, vec![Some(Action::Back), Some(Action::Action)]);
    assert_eq!(r.inline[1].place.at, (212, 181));
    // ACCEPT names no mouse button (`MouseControl = None`): at the keys it
    // shows its key, with the mouse it shows nothing; EXIT neither.
    assert_eq!(r.inline[1].prompt, ACCEPT_ACTION);
    assert_eq!(r.inline[1].prompt.mouse, Some(Mouse::None));
    let with_mouse = TraderState { mode: Mode::Mouse, ..s };
    assert!(resolve_set(&trader(&with_mouse), Mode::Mouse).inline.is_empty());

    let offered = TraderState { for_trade: true, ..s };
    assert_eq!(context(&resolve_set(&trader(&offered), Mode::Keyboard))[0], Some(act(Action::Primary, 40)));
    // With the mouse Put away wants a free parking slot and an item not offered.
    let mouse = TraderState { mode: Mode::Mouse, for_trade: true, can_park: false, ..s };
    assert_eq!(context(&resolve_set(&trader(&mouse), Mode::Mouse)), vec![Some(act(Action::Primary, 40)), None]);

    let theirs = TraderState { inventory: off, trader: on, ..s };
    assert_eq!(context(&resolve_set(&trader(&theirs), Mode::Keyboard))[0], Some(act(Action::Primary, 37)));
    let parked = TraderState { inventory: off, temporary: on, ..s };
    assert_eq!(context(&resolve_set(&trader(&parked), Mode::Keyboard))[0], Some(act(Action::Primary, 46)));
}

/// `ServiceScene.cs:79-106`: Offer (`?36`) from the pack, Take (`?37`)
/// from the table, Page (`?16`), Back and the service's Action inline.
#[test]
fn service_scene() {
    let s = ServiceState {
        pages: 2,
        exit: EXIT,
        action: Button { at: (116, 183), w: 30, visible: true },
        offer: Grid { visible: true, focused: false },
        inventory: Grid { visible: true, focused: true },
        can_move: true,
    };
    let r = resolve_set(&service(&s), Mode::Keyboard);
    assert_eq!(r.scene, vec![Prompt::pattern(Pattern::HorizontalPaging, Label::Prompts(16))]);
    assert_eq!(context(&r), vec![Some(act(Action::Primary, 36))]);
    assert_eq!(r.inline[1].place.at, (148, 181));
    assert_eq!(r.inline[1].prompt.mouse, Some(Mouse::None));
    assert!(resolve_set(&service(&s), Mode::Mouse).inline.is_empty());

    let table = ServiceState { offer: Grid { visible: true, focused: true }, inventory: Grid { visible: true, focused: false }, ..s };
    assert_eq!(context(&resolve_set(&service(&table), Mode::Keyboard)), vec![Some(act(Action::Primary, 37))]);
    let no_action = ServiceState { action: Button { visible: false, ..s.action }, ..s };
    assert_eq!(resolve_set(&service(&no_action), Mode::Keyboard).inline.len(), 1);
}

/// `InfoScene.cs:124-129`: Back inline beside the map button (5, 174), at
/// (+50, +4), (+56, +7) in German, left and vertically centred, with no
/// background and 2 of padding; the mouse shows none.
#[test]
fn info_scene() {
    let set = info(false);
    assert_eq!(set.scheme, Scheme::Muted);
    let r = resolve_set(&set, Mode::Keyboard);
    assert!(r.scene.is_empty() && r.context.is_empty());
    let p = r.inline[0].place;
    assert_eq!((p.at, p.h_align, p.v_align, p.background, p.padding), ((55, 178), Align::Left, Align::Center, false, 2));
    assert_eq!(resolve_set(&info(true), Mode::Keyboard).inline[0].place.at, (61, 181));
    assert!(resolve_set(&set, Mode::Mouse).inline.is_empty());
}

/// The exchange frames take only the mouse's focus (`FocusVisible` off,
/// `TraderScene.cs:237-238`); what they hold is in the deal, so Take back
/// (`?40`, the player's) or Put back (`?44`, the trader's).
#[test]
fn trader_exchange_frames() {
    let off = Grid { visible: true, focused: false };
    let on = Grid { visible: true, focused: true };
    let s = TraderState {
        mode: Mode::Mouse,
        pages: 1,
        exit: EXIT,
        accept: Button { at: (170, 183), w: 40, visible: true },
        inventory: off,
        trader: off,
        exchange_top: on,
        exchange_bottom: off,
        temporary: off,
        for_trade: true,
        can_park: false,
    };
    assert_eq!(context(&resolve_set(&trader(&s), Mode::Mouse))[0], Some(act(Action::Primary, 44)));
    let bottom = TraderState { exchange_top: off, exchange_bottom: on, ..s };
    // Put away is `inventory.Grid`'s only.
    assert_eq!(context(&resolve_set(&trader(&bottom), Mode::Mouse)), vec![Some(act(Action::Primary, 40)), None]);
}

fn item() -> ItemFacts {
    ItemFacts { in_inventory: true, in_room: true, ..ItemFacts::default() }
}

/// `GetItemPrompt` (`InventoryScene.cs:308-376`): Eat (`?18`) and Drink
/// (`?19`) by Action while somebody in range wants it; Refill (`?24`) in the
/// pack at a water source with enough; Equip/Unequip (`?21`/`?22`) for a
/// weapon in the pack; Select trap (`?42`) for a valid tool on the room's
/// floor not yet chosen; Build (`?20`) or Load (`?3`) when a recipe can be
/// built; else Inspect (`?23`).
#[test]
fn item_prompts() {
    let food = ItemFacts { food: 3, hungry: true, ..item() };
    assert_eq!(item_prompt(&food), Some(act(Action::Action, 18)));
    assert_eq!(item_prompt(&ItemFacts { water: 2, thirsty: true, ..item() }), Some(act(Action::Action, 19)));
    let bottle = ItemFacts { capacity: 2, water_source: true, reserve: 2, ..item() };
    assert_eq!(item_prompt(&bottle), Some(act(Action::Action, 24)));
    assert_eq!(item_prompt(&ItemFacts { reserve: 1, ..bottle }), None);
    let knife = ItemFacts { selectable: true, ..item() };
    assert_eq!(item_prompt(&knife), Some(act(Action::Secondary, 21)));
    assert_eq!(item_prompt(&ItemFacts { equipped: true, ..knife }), Some(act(Action::Secondary, 22)));
    let trap = ItemFacts { in_inventory: false, tool: Some(ToolFacts { valid: true, selected: false }), ..item() };
    assert_eq!(item_prompt(&trap), Some(act(Action::Action, 42)));
    // In the pack a tool has no prompt at all.
    assert_eq!(item_prompt(&ItemFacts { in_inventory: true, ..trap }), None);
    let part = ItemFacts { build: Some(BuildFacts { can_build: true, ..BuildFacts::default() }), ..item() };
    assert_eq!(item_prompt(&part), Some(act(Action::Secondary, 20)));
    let ammo = ItemFacts { build: Some(BuildFacts { can_build: true, loads_weapon: true, ..BuildFacts::default() }), ..item() };
    assert_eq!(item_prompt(&ammo), Some(act(Action::Secondary, 3)));
    assert_eq!(item_prompt(&item()), Some(act(Action::Secondary, 23)));
}

/// `GetItemTooltipDetails` (`InventoryScene.cs:171-283`), classic: the
/// statuses Not hungry/thirsty (muted), Not enough water, Place in a room,
/// Unsuitable camp, Needs a technician, Needs ammunition, Check materials
/// (a `tooltip` prompt), Selected (success, no prompt).
#[test]
fn item_tooltip_details() {
    let full = item_details(&ItemFacts { food: 3, hungry: false, ..item() });
    assert_eq!((full.prompt, full.status, full.muted), (None, Some(status::NOT_HUNGRY), true));
    let sated = item_details(&ItemFacts { water: 2, ..item() });
    assert_eq!(sated.status, Some(status::NOT_THIRSTY));
    let dry = item_details(&ItemFacts { capacity: 2, water_source: true, reserve: 1, ..item() });
    assert_eq!((dry.prompt, dry.status, dry.muted), (None, Some(status::NOT_ENOUGH_WATER), false));

    let tool = Some(ToolFacts { valid: false, selected: false });
    assert_eq!(item_details(&ItemFacts { tool, ..item() }).status, Some(status::PLACE_IN_ROOM));
    let pile = ItemFacts { in_inventory: false, in_room: false, tool, ..item() };
    assert_eq!(item_details(&pile).status, Some(status::PLACE_IN_ROOM));
    let floor = ItemFacts { in_inventory: false, tool, ..item() };
    assert_eq!(item_details(&floor).status, Some(status::UNSUITABLE_CAMP));
    let chosen = ItemFacts { in_inventory: false, tool: Some(ToolFacts { valid: true, selected: true }), ..item() };
    let d = item_details(&chosen);
    assert_eq!((d.prompt, d.status, d.success), (None, Some(status::SELECTED_TRAP), true));

    let tech = item_details(&ItemFacts { in_recipes: true, needs_technician: true, ..item() });
    assert_eq!((tech.prompt, tech.status), (Some(act(Action::Secondary, 23)), Some(status::NEEDS_TECHNICIAN)));
    let known = BuildFacts { known: true, ..BuildFacts::default() };
    let check = item_details(&ItemFacts { build: Some(known), ..item() });
    assert_eq!(check.prompt, Some(Prompt::action(Action::Secondary, Label::Tooltip(status::CHECK_MATERIALS))));
    let ammo = item_details(&ItemFacts { build: Some(BuildFacts { only_ammunition: true, ..known }), ..item() });
    assert_eq!(ammo.status, Some(status::NEEDS_AMMUNITION));
    // An unknown recipe stays Inspect.
    assert_eq!(item_details(&ItemFacts { build: Some(BuildFacts::default()), ..item() }).prompt, Some(act(Action::Secondary, 23)));
}
