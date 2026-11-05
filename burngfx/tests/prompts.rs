//! `burngfx::prompts`: v1.2's input prompt layer — `InputPromptController.cs`
//! (`Resolve`), `InputControlDisplay.cs` (which glyph), `InputControlLabelRenderer.cs`
//! (measure, draw) and `InputPromptOverlay.cs` / `.Row.cs` (the rows). The
//! expected numbers are worked out from that C#, cited at each test.

use burngfx::font::Font;
use burngfx::input::{Action, Bindings, Key, Pad};
use burngfx::prompts::*;

/// A font with `font-small.txt`'s metrics (v1.2, `resources/game/classic/`):
/// height 12, offset -2 (so `GetHeight()` 8), line height 11, and the
/// widths of the characters these tests use. `font.txt` has the same.
fn small_font() -> Font {
    Font::parse(concat!(
        "line_height=11\nimage=font.png\noffset=-2\nheight=12\n",
        "char0=\" .[]/+?123ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz\"\n",
        "width0=6 2 3 3 6 4 5 3 5 5 5 5 5 5 5 5 5 5 2 5 5 4 6 5 5 5 5 5 5 5 5 5 6 5 5 5 6 5 6 5 5 4 5 5 2 5 5 3 6 5 5 5 5 5 5 4 5 5 6 6 5 5\n",
    ))
    .expect("the test font parses")
}

/// The few words the tests need, standing in for `prompts.txt` and
/// `controls.txt`.
struct Words;

impl Texts for Words {
    fn label(&self, label: Label) -> &str {
        match label {
            Label::Prompts(25) => "Travel",
            Label::Prompts(26) => "Enter",
            Label::Prompts(27) => "Info",
            Label::Prompts(30) => "Page",
            Label::Prompts(43) => "Radio",
            Label::Prompts(_) | Label::SetupNotes(_) | Label::Tooltip(_) => "?",
            Label::Text(s) => s,
        }
    }

    fn control(&self, index: usize) -> &str {
        match index {
            line::BACKSPACE => "Back",
            line::SHIFT => "Shift",
            line::LEFT_STICK => "Left stick",
            _ => "?",
        }
    }
}

fn controls<'a>(b: &'a Bindings, family: Family, preferred: Chord) -> Controls<'a> {
    Controls { bindings: b, family, texts: &Words, preferred_primary: preferred }
}

const GAME: (i32, i32) = (341, 213); // 640 × 480 at 15/8 × 9/4, as on AROS

fn keys(parts: &[KeyGlyph]) -> Control {
    parts.iter().map(|&k| Part::Key(k)).collect()
}

// --- The map scene (`MapScene.cs:115-149`) --------------------------------

const BACK_DOTS: Prompt = Prompt::action(Action::Back, Label::Text("...")).with_mouse(Mouse::Right);

fn info(label: u16, mouse: Mouse) -> Prompt {
    Prompt::action(Action::Secondary, Label::Prompts(label)).with_mouse(mouse)
}

const ENTER: Prompt = Prompt::action(Action::Primary, Label::Prompts(26));
const TRAVEL: Prompt = Prompt::action(Action::Action, Label::Prompts(25)).with_mouse(Mouse::Left);

/// The map scene's prompts: Back "..." on the scene, and on the map view
/// (`IsPromptActive` always, `MapView.cs:64`) the location info for each
/// button and Enter/Travel in the Primary slot.
fn map_scene(info_left: Option<u16>, info_right: Option<u16>, go: Option<Prompt>) -> Scene {
    let view = Win {
        prompts: Collection::of(vec![
            Entry::dynamic(&[info(27, Mouse::Left), info(43, Mouse::Left)], info_left.map(|l| info(l, Mouse::Left))),
            Entry::dynamic(&[info(27, Mouse::Right), info(43, Mouse::Right)], info_right.map(|l| info(l, Mouse::Right))),
            Entry::dynamic_at(Role::Primary, &[ENTER, TRAVEL], go),
        ]),
        visible: true,
        active: true,
        ..Win::default()
    };
    Scene { prompts: Collection::of(vec![Entry::add(BACK_DOTS, true)]), children: vec![view] }
}

fn frame<'a>(scene: &'a Scene, mode: Mode, c: &'a Controls<'a>, font: &'a Font) -> Frame<'a> {
    Frame {
        scene,
        root: Root::Scene,
        mode: Some(mode),
        hints: Hints::Full,
        new_gfx: false,
        controls: c,
        small: font,
        font,
        game: GAME,
        scene_at: (0, 0),
    }
}

/// Keyboard: Back goes in the left row; the right row has Primary then
/// Secondary (`Resolve`, `:289-319`), each slot as wide as its widest
/// candidate (`BuildContextPrompts`).
///
/// Widths (`InputControlLabelRenderer.Measure`, label first, no brackets
/// round a glyph): label + half a space (3) + the 12-pixel glyph. "..." is
/// 6 → 21; "Enter" 24 → 39; "Travel" 29 → 44; "Info" 16 → 31; "Radio" 23
/// → 38. A row is its prompts, three spaces (18) between, padding 4 at
/// each end; height `GetHeight()` 8 + 2 + 2 (`Row.cs:198-201`).
#[test]
fn the_map_in_keyboard_mode() {
    let b = Bindings::defaults();
    let font = small_font();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let scene = map_scene(Some(27), None, Some(ENTER));
    let rows = overlay(&frame(&scene, Mode::Keyboard, &c, &font));
    assert_eq!(rows.len(), 2);

    // Bottom left, margin 6: `AnchorToScreenBottomLeft` (`Row.cs:87-94`).
    let left = &rows[0];
    assert_eq!((left.at, left.row.w, left.row.h), ((6, 213 - 6 - 12), 29, 12));
    assert_eq!(left.row.background, Some((0, 29)));
    assert_eq!(
        left.row.pieces,
        vec![
            Piece::Text { x: 4, y: 2, text: "...".into() },
            // `DrawGlyph`: y + (GetHeight() 8 - 10) / 2.
            Piece::Glyph { x: 13, y: 1, part: Part::Key(KeyGlyph::Escape) },
        ]
    );

    // Bottom right: Enter (Space, the scene's preferred key) reserving
    // Travel's 44, then Info (F) reserving Radio's 38.
    let right = &rows[1];
    assert_eq!((right.at, right.row.w, right.row.h), ((341 - 6 - 108, 195), 108, 12));
    assert_eq!(
        right.row.pieces,
        vec![
            Piece::Text { x: 73, y: 2, text: "Info".into() },
            Piece::Glyph { x: 92, y: 1, part: Part::Key(KeyGlyph::Letter('f')) },
            Piece::Text { x: 9, y: 2, text: "Enter".into() },
            Piece::Glyph { x: 36, y: 1, part: Part::Key(KeyGlyph::Space) },
        ]
    );
    // The background spans only what is drawn (`UpdateBackgroundBounds`).
    assert_eq!(right.row.background, Some((5, 103)));
}

/// Mouse: Back is not `ShowInMouseMode`, so it competes for the right row
/// by its button (Right → Secondary, `RoleForMouse`). Nothing is hovered:
/// the Primary slot keeps Travel's width, invisible.
#[test]
fn the_map_in_mouse_mode() {
    let b = Bindings::defaults();
    let font = small_font();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let scene = map_scene(None, None, None);

    let r = resolve(&scene, Root::Scene, Mode::Mouse, false);
    assert!(r.scene.is_empty());
    assert_eq!(r.context.len(), 2);
    assert_eq!(r.context[0].candidates, vec![info(27, Mouse::Left), info(43, Mouse::Left), ENTER, TRAVEL]);
    assert_eq!(r.context[0].active, None);
    assert_eq!(r.context[1].candidates, vec![BACK_DOTS, info(27, Mouse::Right), info(43, Mouse::Right)]);
    assert_eq!(r.context[1].active, Some(BACK_DOTS));

    let rows = overlay(&frame(&scene, Mode::Mouse, &c, &font));
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].at, rows[0].row.w), ((227, 195), 108));
    assert_eq!(
        rows[0].row.pieces,
        vec![
            Piece::Text { x: 83, y: 2, text: "...".into() },
            Piece::Glyph { x: 92, y: 1, part: Part::Key(KeyGlyph::MouseRight) },
        ]
    );
    assert_eq!(rows[0].row.background, Some((79, 29)));

    // Hovering a place with a radio report: the view's right-button prompt
    // wins over the scene's Back (`:312-315`).
    let scene = map_scene(None, Some(43), None);
    let r = resolve(&scene, Root::Scene, Mode::Mouse, false);
    assert_eq!(r.context[1].active, Some(info(43, Mouse::Right)));
}

/// `SuppressWhen` on the scene (a dialog, the menu, the manual) hides its
/// row and, with the scene the input root, every window's context prompt
/// too (`AddWindowContextCandidates`, `:334-335`).
#[test]
fn a_suppressed_scene_shows_nothing_but_inline_prompts() {
    let mut scene = map_scene(Some(27), None, Some(ENTER));
    scene.prompts.suppressed = true;
    let r = resolve(&scene, Root::Scene, Mode::Keyboard, false);
    assert!(r.scene.is_empty() && r.context.is_empty());

    // A window's inline prompt is still collected (`AddInlinePrompts`
    // checks each window's own collection).
    scene.children[0].prompts.entries.push(Entry::inline(Prompt::action(Action::Back, Label::Text("")), true, Inline::at(3, 4)));
    scene.children[0].at = (10, 20);
    let r = resolve(&scene, Root::Scene, Mode::Keyboard, false);
    assert_eq!(r.inline.len(), 1);
    assert_eq!(r.inline[0].place.at, (13, 24));
}

/// `HideInNewGfx` hides a collection only in NewGfx (`:255-256`).
#[test]
fn hide_in_new_gfx() {
    let grid = Win {
        prompts: Collection {
            entries: vec![Entry::add(Prompt::action(Action::Primary, Label::Prompts(36)), true)],
            hide_in_new_gfx: true,
            ..Collection::default()
        },
        visible: true,
        active: true,
        ..Win::default()
    };
    let scene = Scene { prompts: Collection::default(), children: vec![grid] };
    assert_eq!(resolve(&scene, Root::Scene, Mode::Keyboard, false).context.len(), 1);
    assert!(resolve(&scene, Root::Scene, Mode::Keyboard, true).context.is_empty());
}

/// A window without focus lists its candidates (they reserve width) but
/// none is shown; a selector may answer with a prompt it did not list,
/// which then joins the slot (`AddEntryContextCandidates`, `:385-420`).
#[test]
fn candidates_of_inactive_windows_and_unlisted_answers() {
    let listed = Prompt::action(Action::Primary, Label::Prompts(36));
    let other = Prompt::action(Action::Primary, Label::Prompts(37));
    let mut w = Win { prompts: Collection::of(vec![Entry::dynamic(&[listed], Some(other))]), visible: true, ..Win::default() };
    let scene = Scene { prompts: Collection::default(), children: vec![w.clone()] };
    let r = resolve(&scene, Root::Scene, Mode::Keyboard, false);
    assert_eq!(r.context, vec![Group { candidates: vec![listed], active: None }]);

    w.active = true;
    let scene = Scene { prompts: Collection::default(), children: vec![w] };
    let r = resolve(&scene, Root::Scene, Mode::Keyboard, false);
    assert_eq!(r.context, vec![Group { candidates: vec![listed, other], active: Some(other) }]);

    // An invisible window adds nothing.
    let mut hidden = scene.clone();
    hidden.children[0].visible = false;
    assert!(resolve(&hidden, Root::Scene, Mode::Keyboard, false).context.is_empty());
    // No input window, no context.
    assert!(resolve(&scene, Root::None, Mode::Keyboard, false).context.is_empty());
}

// --- The start screen's name buttons (`MenuScene.cs:322-334`) -------------

/// A focused player button: the paging pattern is Extended, so the row
/// reads Extended, Secondary, Primary (`:293-296`). Its Secondary shows
/// Shift+Up/Down on keys (`KeyboardPattern`) and X on a pad; it has no
/// mouse control at all (`MouseButton.None`).
#[test]
fn the_start_screen_name_button() {
    let face = Prompt::pattern(Pattern::HorizontalPaging, Label::Prompts(0));
    let colour = Prompt::action(Action::Secondary, Label::Prompts(1))
        .with_keyboard_pattern(Pattern::VerticalPaging)
        .with_mouse(Mouse::None);
    let setup = |l| Prompt::action(Action::Primary, Label::Prompts(l)).with_key(Chord::plain(Key::Enter));
    let button = Win {
        prompts: Collection::of(vec![
            Entry::add(face, true),
            Entry::add(colour, true),
            Entry::dynamic(&[setup(8), setup(6), setup(7)], Some(setup(7))),
        ]),
        visible: true,
        active: true,
        ..Win::default()
    };
    let scene = Scene { prompts: Collection::default(), children: vec![button] };

    let r = resolve(&scene, Root::Scene, Mode::Keyboard, false);
    let shown: Vec<_> = r.context.iter().map(|g| g.active).collect();
    assert_eq!(shown, vec![Some(face), Some(colour), Some(setup(7))]);

    let b = Bindings::defaults();
    let c = controls(&b, Family::Xbox, PREFER_ENTER);
    use KeyGlyph as K;
    assert_eq!(c.of(Mode::Keyboard, &face), keys(&[K::Shift, K::Left, K::Right]));
    assert_eq!(c.of(Mode::Keyboard, &colour), keys(&[K::Shift, K::Up, K::Down]));
    assert_eq!(c.of(Mode::Keyboard, &setup(7)), keys(&[K::Enter]));
    assert_eq!(c.of(Mode::Gamepad, &face), vec![Part::Pad(PadGlyph::LeftShoulder), Part::Pad(PadGlyph::RightShoulder)]);
    assert_eq!(c.of(Mode::Gamepad, &colour), vec![Part::Pad(PadGlyph::FaceWest)]);

    // With the mouse only the Primary has a button (Left, by default).
    let r = resolve(&scene, Root::Scene, Mode::Mouse, false);
    assert_eq!(r.context.len(), 1);
    assert_eq!(r.context[0].active, Some(setup(7)));
    assert_eq!(c.of(Mode::Mouse, &setup(7)), keys(&[K::MouseLeft]));
    // Its keyboard pattern still resolves in mouse mode (`Resolve`,
    // `:104-108`); it is the missing button that keeps it out of the row.
    assert_eq!(c.of(Mode::Mouse, &colour), keys(&[K::Shift, K::Up, K::Down]));
}

// --- Inline rows ------------------------------------------------------------

/// The location's character prompts (`LocationScene.cs:136-149,656-671`):
/// previous right-aligned left of the portrait, next and group actions
/// sharing one row right of it, one space apart. Only with keys or a pad.
#[test]
fn the_location_character_prompts() {
    let portrait_left = GAME.0 / 2 - 31;
    let baseline = GAME.1 - 6;
    let prev = Inline::at(portrait_left - 2, baseline).aligned(Align::Right, Align::Right);
    let next = Inline::at(portrait_left + 68 + 2, baseline).aligned(Align::Left, Align::Right).separated(" ");
    let blank = Label::Text("");
    let scene = Scene {
        prompts: Collection::of(vec![
            Entry::inline(Prompt::action(Action::LeftArea, blank), true, prev),
            Entry::inline(Prompt::action(Action::RightArea, blank), true, next),
            Entry::inline(Prompt::action(Action::Secondary, blank), true, next),
        ]),
        children: vec![],
    };
    let b = Bindings::defaults();
    let font = small_font();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let rows = overlay(&frame(&scene, Mode::Keyboard, &c, &font));
    use KeyGlyph as K;
    assert_eq!(rows.len(), 2);
    // Shift+Left, always bound (`KeyboardBindings.GetControls`): 24 wide.
    assert_eq!((rows[0].at, rows[0].row.w, rows[0].row.h), ((137 - 32, 195), 32, 12));
    assert_eq!(
        rows[0].row.pieces,
        vec![Piece::Glyph { x: 4, y: 1, part: Part::Key(K::Shift) }, Piece::Glyph { x: 16, y: 1, part: Part::Key(K::Left) }]
    );
    assert_eq!((rows[1].at, rows[1].row.w), ((209, 195), 50));
    assert_eq!(
        rows[1].row.pieces,
        vec![
            Piece::Glyph { x: 34, y: 1, part: Part::Key(K::Letter('f')) },
            Piece::Glyph { x: 4, y: 1, part: Part::Key(K::Shift) },
            Piece::Glyph { x: 16, y: 1, part: Part::Key(K::Right) },
        ]
    );
    // With the mouse an inline prompt needs a button of its own (`:370`).
    assert!(overlay(&frame(&scene, Mode::Mouse, &c, &font)).is_empty());
}

/// The city report's Back (`InfoScene.cs:126-129`): left, centred
/// vertically, no background, padding 2; the scene's own position added
/// (`InputPromptOverlay.RefreshInline`, `:147-148`).
#[test]
fn the_info_back_prompt() {
    let place = Inline::at(50, 4).aligned(Align::Left, Align::Center).bare(2);
    let back = Win {
        prompts: Collection::of(vec![Entry::inline(Prompt::action(Action::Back, Label::Text("")), true, place)]),
        visible: true,
        ..Win::default()
    };
    let scene = Scene { prompts: Collection::default(), children: vec![back] };
    let b = Bindings::defaults();
    let font = small_font();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let mut f = frame(&scene, Mode::Keyboard, &c, &font);
    f.scene_at = (10, 6);
    let rows = overlay(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].at, rows[0].row.w, rows[0].row.h), ((60, 4), 16, 12));
    assert_eq!(rows[0].row.background, None);
    assert_eq!(rows[0].row.pieces, vec![Piece::Glyph { x: 2, y: 1, part: Part::Key(KeyGlyph::Escape) }]);
}

/// `prompts=2` hides everything (`Row.cs:120-121`); before any input
/// nothing is drawn either (`:123-124`). `prompts=1` shows as 0 does.
#[test]
fn hidden_hints_and_no_input() {
    let b = Bindings::defaults();
    let font = small_font();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let scene = map_scene(Some(27), None, Some(ENTER));
    let mut f = frame(&scene, Mode::Keyboard, &c, &font);
    f.hints = Hints::Hide;
    assert!(overlay(&f).is_empty());
    f.hints = Hints::Less;
    assert_eq!(overlay(&f).len(), 2);
    f.mode = None;
    assert!(overlay(&f).is_empty());
}

/// NewGfx's bottom rows (`UseSmallFontLayout`): height `LineHeight` (at
/// least the glyph's 10) + 2 + 1, the text centred on it, and padding 2
/// instead of 4 after a closing glyph (`Row.cs:198-209`).
#[test]
fn the_new_gfx_small_row_arithmetic() {
    let b = Bindings::defaults();
    let font = small_font();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let row = layout_row(&font, &c, Mode::Keyboard, &[RowPrompt::shown(BACK_DOTS)], RowStyle::bottom(true)).unwrap();
    assert_eq!((row.w, row.h), (27, 14));
    assert_eq!(row.background, Some((0, 27)));
    assert_eq!(
        row.pieces,
        vec![
            Piece::Text { x: 4, y: 3, text: "...".into() },
            Piece::Glyph { x: 13, y: 2, part: Part::Key(KeyGlyph::Escape) },
        ]
    );
}

// --- Which control (`InputControlDisplay.cs`) -------------------------------

#[test]
fn keyboard_controls_and_the_preferred_key() {
    use KeyGlyph as K;
    let b = Bindings::defaults();
    let primary = Prompt::action(Action::Primary, Label::Prompts(5));
    // `accept = space enter`: a scene's preferred key picks among them.
    assert_eq!(controls(&b, Family::Xbox, PREFER_SPACE).of(Mode::Keyboard, &primary), keys(&[K::Space]));
    assert_eq!(controls(&b, Family::Xbox, PREFER_ENTER).of(Mode::Keyboard, &primary), keys(&[K::Enter]));
    // The preference is for Primary only; Back is Escape.
    let back = Prompt::action(Action::Back, Label::Prompts(17));
    assert_eq!(controls(&b, Family::Xbox, PREFER_ENTER).of(Mode::Keyboard, &back), keys(&[K::Escape]));
    // A preferred key that is not bound falls back to the first.
    let only_space = Bindings::from_settings("[keyboard]\naccept = space\n");
    assert_eq!(controls(&only_space, Family::Xbox, PREFER_ENTER).of(Mode::Keyboard, &primary), keys(&[K::Space]));
    // Tab and Shift+Tab are always there.
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    assert_eq!(c.action(Mode::Keyboard, Action::PreviousTarget, None, None, None), keys(&[K::Shift, K::Tab]));
    assert_eq!(c.action(Mode::Keyboard, Action::NextTarget, None, None, None), keys(&[K::Tab]));
    // Unbound: no prompt.
    let none = Bindings::from_settings("[keyboard]\nback =\n");
    assert_eq!(controls(&none, Family::Xbox, PREFER_SPACE).of(Mode::Keyboard, &back), Control::new());
}

/// A key with no glyph is named in text, and text alone is bracketed in a
/// row (`UsesBrackets`): `[Back]` for Backspace (`controls.txt` line 8).
#[test]
fn text_controls_and_brackets() {
    let b = Bindings::from_settings("[keyboard]\naccept = backspace\n");
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let primary = Prompt::action(Action::Primary, Label::Prompts(5));
    let control = c.of(Mode::Keyboard, &primary);
    assert_eq!(control, vec![Part::Text("Back".into())]);
    let font = small_font();
    // "[]" 6 + "Back" 22; with a label, a whole space (6) before it.
    assert_eq!(measure(&font, ROW_BRACKETS, &control, "", true), 28);
    assert_eq!(measure(&font, ROW_BRACKETS, &control, "Info", true), 28 + 6 + 16);
    let mut pieces = Vec::new();
    draw_label_first(&font, ROW_BRACKETS, &control, "Info", 100, 2, &mut pieces);
    assert_eq!(
        pieces,
        vec![
            Piece::Text { x: 50, y: 2, text: "Info".into() },
            Piece::Text { x: 72, y: 2, text: "[".into() },
            Piece::Text { x: 75, y: 2, text: "Back".into() },
            Piece::Text { x: 97, y: 2, text: "]".into() },
        ]
    );
}

/// Mouse mode: the prompt's button, else Primary's left and Secondary's or
/// Action's right; without one, the keyboard's key. `MouseButton.None` is
/// no control at all.
#[test]
fn mouse_controls() {
    use KeyGlyph as K;
    let b = Bindings::defaults();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let p = |a| Prompt::action(a, Label::Prompts(0));
    assert_eq!(c.of(Mode::Mouse, &p(Action::Primary)), keys(&[K::MouseLeft]));
    assert_eq!(c.of(Mode::Mouse, &p(Action::Secondary)), keys(&[K::MouseRight]));
    assert_eq!(c.of(Mode::Mouse, &p(Action::Action)), keys(&[K::MouseRight]));
    assert_eq!(c.of(Mode::Mouse, &p(Action::Back)), keys(&[K::Escape]));
    assert_eq!(c.of(Mode::Mouse, &BACK_DOTS), keys(&[K::MouseRight]));
    assert_eq!(c.of(Mode::Mouse, &p(Action::Primary).with_mouse(Mouse::None)), Control::new());
    // Patterns are the keyboard's (`ResolvePattern`, `:69-70`).
    let nav = Prompt::pattern(Pattern::HorizontalNavigation, Label::Text("Language"));
    assert_eq!(c.of(Mode::Mouse, &nav), keys(&[K::Left, K::Right]));
    assert_eq!(c.of(Mode::Gamepad, &nav), vec![Part::Pad(PadGlyph::DPadHorizontal)]);
}

#[test]
fn gamepad_controls() {
    let b = Bindings::defaults();
    let c = controls(&b, Family::Xbox, PREFER_SPACE);
    let pad = |a| c.action(Mode::Gamepad, a, None, None, None);
    assert_eq!(pad(Action::Primary), vec![Part::Pad(PadGlyph::FaceSouth)]);
    assert_eq!(pad(Action::Back), vec![Part::Pad(PadGlyph::FaceEast)]);
    assert_eq!(pad(Action::Statistics), vec![Part::Pad(PadGlyph::DPadLeft)]);
    assert_eq!(pad(Action::LocationInfo), vec![Part::Pad(PadGlyph::DPadRight)]);
    assert_eq!(pad(Action::Options), vec![Part::Pad(PadGlyph::Menu)]);
    assert_eq!(pad(Action::NextTarget), vec![Part::Pad(PadGlyph::RightTrigger)]);
    // Nothing bound: nothing, unless the prompt names a control.
    assert_eq!(pad(Action::MoveUp), Control::new());
    assert_eq!(c.action(Mode::Gamepad, Action::MoveUp, None, Some(Pad::DPadUp), None), vec![Part::Pad(PadGlyph::DPadUp)]);

    // The left stick has no glyph and is named by the family
    // (`InputControlDisplay.Format`, `:368-393`).
    let stick = Bindings::from_settings("[gamepad]\naccept = left_stick\n");
    let named = |f| controls(&stick, f, PREFER_SPACE).action(Mode::Gamepad, Action::Primary, None, None, None);
    assert_eq!(named(Family::Xbox), vec![Part::Text("Left stick".into())]);
    assert_eq!(named(Family::PlayStation), vec![Part::Text("L3".into())]);
    assert_eq!(named(Family::Steam), vec![Part::Text("L3".into())]);
    assert_eq!(named(Family::Switch), vec![Part::Text("LS".into())]);
}

// --- Sheets and colours -------------------------------------------------------

/// `GetKeyboardAtlasIndex` (`InputControlLabelRenderer.cs:157-176`) and the
/// controller sheets' `InputGlyph - 1` on a 4 × 4 grid of 22-pixel cells.
#[test]
fn glyph_cells() {
    use KeyGlyph as K;
    let at = |k: K| sheet_cell(&Part::Key(k), Family::Xbox).map(|c| (c.sheet, c.x, c.y));
    assert_eq!(at(K::Letter('a')), Some((SHEET_KEYBOARD, 0, 0)));
    assert_eq!(at(K::Letter('h')), Some((SHEET_KEYBOARD, 7 * 22, 0)));
    assert_eq!(at(K::Letter('i')), Some((SHEET_KEYBOARD, 0, 22)));
    assert_eq!(at(K::Digit(0)), Some((SHEET_KEYBOARD, 2 * 22, 3 * 22)));
    assert_eq!(at(K::Escape), Some((SHEET_KEYBOARD, 4 * 22, 4 * 22)));
    assert_eq!(at(K::Space), Some((SHEET_KEYBOARD, 7 * 22, 4 * 22)));
    assert_eq!(at(K::Ctrl), Some((SHEET_KEYBOARD, 8 * 22, 0)));
    assert_eq!(at(K::Right), Some((SHEET_KEYBOARD, 9 * 22, 22)));
    assert_eq!(at(K::MouseLeft), Some((SHEET_KEYBOARD, 9 * 22, 2 * 22)));
    assert_eq!(at(K::MouseRight), Some((SHEET_KEYBOARD, 9 * 22, 3 * 22)));
    assert_eq!(at(K::Down), Some((SHEET_KEYBOARD, 8 * 22, 4 * 22)));
    assert_eq!(K::MouseRight.ordinal(), 49);

    let pad = |g: PadGlyph, f| sheet_cell(&Part::Pad(g), f).map(|c| (c.sheet, c.x, c.y));
    assert_eq!(pad(PadGlyph::FaceSouth, Family::Xbox), Some((SHEET_XBOX, 0, 0)));
    assert_eq!(pad(PadGlyph::DPadRight, Family::PlayStation), Some((SHEET_PLAYSTATION, 3 * 22, 22)));
    assert_eq!(pad(PadGlyph::Menu, Family::Switch), Some((SHEET_SWITCH, 0, 2 * 22)));
    assert_eq!(pad(PadGlyph::RightShoulder, Family::Steam), Some((SHEET_STEAM, 3 * 22, 3 * 22)));
    assert_eq!(sheet_cell(&Part::Text("x".into()), Family::Xbox), None);
}

/// `Row` colours (`InputPromptOverlay.Row.cs:57-69`), and the Xbox and
/// PlayStation face buttons drawn untinted (`InputControlLabelRenderer.cs:118-120`).
#[test]
fn colours_and_tint() {
    assert_eq!(colours(Scheme::Hud), ((144, 160, 212), (144, 160, 212)));
    assert_eq!(colours(Scheme::Options), ((190, 77, 12), (190, 77, 12)));
    assert_eq!(colours(Scheme::Muted), ((212, 212, 212), (212, 212, 212)));
    assert_eq!(colours(Scheme::Default), ((212, 212, 212), (255, 255, 255)));
    let a = Part::Pad(PadGlyph::FaceSouth);
    assert!(untinted(&a, Family::Xbox) && untinted(&a, Family::PlayStation));
    assert!(!untinted(&a, Family::Steam) && !untinted(&a, Family::Switch));
    assert!(!untinted(&Part::Pad(PadGlyph::Menu), Family::Xbox));
    assert!(!untinted(&Part::Key(KeyGlyph::Escape), Family::Xbox));
}

// --- Settings (`Application.cs`) --------------------------------------------

#[test]
fn controller_glyphs_setting() {
    assert_eq!(ControllerGlyphs::parse(" XBOX "), ControllerGlyphs::Xbox);
    assert_eq!(ControllerGlyphs::parse("switch"), ControllerGlyphs::Switch);
    assert_eq!(ControllerGlyphs::parse("nintendo"), ControllerGlyphs::Auto);
    assert_eq!(ControllerGlyphs::parse(""), ControllerGlyphs::Auto);
    // Auto without Steam is Xbox (`BurntimeGame.cs:166-181`).
    assert_eq!(ControllerGlyphs::Auto.family(), Family::Xbox);
    assert_eq!(ControllerGlyphs::Steam.family(), Family::Steam);
    let mut m = ControllerGlyphs::Auto;
    let mut seen = Vec::new();
    for _ in 0..5 {
        seen.push((m.name(), m.line()));
        m = m.next();
    }
    assert_eq!(m, ControllerGlyphs::Auto);
    assert_eq!(seen, vec![("auto", 0), ("xbox", 1), ("playstation", 2), ("steam", 3), ("switch", 4)]);
}

#[test]
fn prompts_setting() {
    assert_eq!(Hints::parse(""), Hints::Full);
    assert_eq!(Hints::parse("x"), Hints::Full);
    assert_eq!(Hints::parse("-4"), Hints::Full);
    assert_eq!(Hints::parse(" 1 "), Hints::Less);
    assert_eq!(Hints::parse("2"), Hints::Hide);
    assert_eq!(Hints::parse("9"), Hints::Hide);
    assert!(Hints::Less.shows() && !Hints::Hide.shows());
    // The Settings line toggles Full and Hide only.
    assert_eq!(Hints::Full.next(), Hints::Hide);
    assert_eq!(Hints::Less.next(), Hints::Hide);
    assert_eq!(Hints::Hide.next(), Hints::Full);
    assert_eq!((Hints::Full.line(), Hints::Less.line(), Hints::Hide.line()), (0, 0, 1));
    assert_eq!(Hints::Hide.value(), 2);
}

// --- The packed sheets (`pack::names::V12_PROMPT_GLYPHS`) --------------------

/// The table names every sheet `prompts` draws from, under names that fit.
#[test]
fn the_glyph_table_covers_the_sheets() {
    use burngfx::pack::{names, NAME_LEN};
    let keys: Vec<&str> = names::V12_PROMPT_GLYPHS.iter().map(|(k, _)| *k).collect();
    for sheet in [SHEET_KEYBOARD, SHEET_XBOX, SHEET_PLAYSTATION, SHEET_STEAM, SHEET_SWITCH, SHEET_HOLD] {
        assert!(keys.contains(&sheet), "{sheet}");
    }
    for (key, _) in names::V12_PROMPT_GLYPHS.iter().chain(names::V12_PROMPT_TEXTS) {
        assert!(key.len() <= NAME_LEN, "{key}");
    }
}

/// A pack built from v1.2's files holds the sheets at the sizes the cells
/// assume: 10 × 5 and 4 × 4 cells of 22, the hold mark one cell. Skipped
/// when the pack has none of them (the shared test pack predates them);
/// `BURNTIME_PACK` names another pack to check.
#[test]
fn the_packed_glyph_sheets() {
    let path = std::env::var("BURNTIME_PACK").unwrap_or_else(|_| String::from("testdata/burntime.pack"));
    let Ok(bytes) = std::fs::read(&path) else { return };
    let pack = burngfx::pack::Pack::open(&bytes).expect("a pack");
    if !pack.contains(SHEET_KEYBOARD) {
        return;
    }
    for (sheet, size) in [
        (SHEET_KEYBOARD, (220, 110)),
        (SHEET_XBOX, (88, 88)),
        (SHEET_PLAYSTATION, (88, 88)),
        (SHEET_STEAM, (88, 88)),
        (SHEET_SWITCH, (88, 88)),
        (SHEET_HOLD, (22, 22)),
    ] {
        let raw = pack.get(sheet).unwrap_or_else(|| panic!("{sheet} missing"));
        let (w, h) = (u16::from_le_bytes([raw[0], raw[1]]), u16::from_le_bytes([raw[2], raw[3]]));
        assert_eq!((w, h), size, "{sheet}");
        assert_eq!(raw.len(), 4 + w as usize * h as usize * 4, "{sheet}");
    }
    // `controls.txt` has its 26 lines; `prompts.txt` reaches `@prompts?46`.
    let lines = |key: &str| pack.get(key).map(|t| String::from_utf8_lossy(t).lines().count());
    assert_eq!(lines("v12/controls"), Some(26));
    assert!(lines("v12/prompts").is_some_and(|n| n >= 47));
}
