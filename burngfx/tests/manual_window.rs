//! v1.2's in-game manual (`GUI/ManualWindow.cs`, `GUI/ManualTextLayout.cs`):
//! the text's layout, its entries, and the window's paging and scrolling.

use burngfx::manual_window::*;

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn joined(lines: &[String]) -> String {
    lines.join("|")
}

// `ManualTextLayoutTests.cs`, case for case.

#[test]
fn changelog_subheadings_wrap_using_small_font_metrics() {
    let len = |t: &str| t.chars().count() as i32;
    let twice = |t: &str| 2 * t.chars().count() as i32;
    assert_eq!(joined(&wrap(&strings(&["##one two three"]), 7, &len, &twice)), "##one two|##three");
}

#[test]
fn manual_reflows_paragraphs_for_active_font_metrics() {
    let paragraphs = strings(&["one two three four"]);
    let len = |t: &str| t.chars().count() as i32;
    let half = |t: &str| (t.chars().count() as i32 + 1) / 2;
    assert_eq!(joined(&wrap(&paragraphs, 8, &len, &len)), "one two|three|four");
    assert_eq!(joined(&wrap(&paragraphs, 8, &half, &len)), "one two three|four");
}

#[test]
fn manual_preserves_headings_paragraphs_and_illustration_directives() {
    let paragraphs = strings(&["#ONE TWO", "one two", "", "@construction|item_rat_trap", "@extended-only-end"]);
    let len = |t: &str| t.chars().count() as i32;
    let twice = |t: &str| 2 * t.chars().count() as i32;
    assert_eq!(
        joined(&wrap(&paragraphs, 7, &len, &twice)),
        "#ONE|#TWO|one two||@construction|item_rat_trap|@extended-only-end"
    );
}

/// A game where every item is 32 high and the class pictures 16.
struct Game {
    extended_items: bool,
}

impl Content for Game {
    fn has_item(&self, id: &str) -> bool {
        id != "item_steel_helmet" || self.extended_items
    }
    fn item_height(&self, _: &str) -> Option<i32> {
        Some(32)
    }
    fn class_height(&self, _: u8) -> i32 {
        16
    }
    fn city_height(&self) -> i32 {
        14
    }
    fn flag_height(&self) -> i32 {
        36
    }
    fn recipe_items(&self, _: &str) -> Option<Vec<String>> {
        Some(strings(&["item_wire", "item_woodpile", "item_screws"]))
    }
}

const SMALL: Metrics = Metrics { line: 11, title_line: 11 };

/// `ParseEntry`: a heading is two title lines tall in the manual and one in
/// the notes; a picture takes as many text lines as its height needs, an
/// item never under three; `@entry|` with no item the game has is dropped.
#[test]
fn entries_count_their_lines() {
    let game = Game { extended_items: false };
    let lines = strings(&["#WATER", "Refill.", "@supply-example|item_water_bottle", "@city", "@flag", "@doctor",
                          "@tiers", "##Sub", "@entry|item_steel_helmet", "", ""]);
    let e = entries(&lines, false, false, SMALL, &game);
    let got: Vec<(Kind, i32)> = e.iter().map(|e| (e.kind, e.lines)).collect();
    assert_eq!(got, [(Kind::Heading, 2), (Kind::Text, 1), (Kind::SupplyExample, 3), (Kind::Goal, 2), (Kind::Goal, 3),
                     (Kind::ClassImage, 2), (Kind::Tiers, 1), (Kind::Subheading, 1)]);
    assert_eq!(e[0].payload, "WATER");
    assert_eq!(entries(&strings(&["#Notes"]), false, true, SMALL, &game)[0].lines, 1);
}

/// `@extended-only-begin/end`: PROTECTION is shown under Extended only,
/// and never in the notes.
#[test]
fn the_extended_block_is_extended_only() {
    let game = Game { extended_items: true };
    let lines = strings(&["@extended-only-begin", "#PROTECTION", "@entry|item_steel_helmet", "@extended-only-end", "After"]);
    let classic: Vec<String> = entries(&lines, false, false, SMALL, &game).into_iter().map(|e| e.payload).collect();
    let extended: Vec<String> = entries(&lines, true, false, SMALL, &game).into_iter().map(|e| e.payload).collect();
    assert_eq!(classic, ["After"]);
    assert_eq!(extended, ["PROTECTION", "item_steel_helmet", "After"]);
    assert_eq!(entries(&lines, true, true, SMALL, &game).len(), 1);
}

/// In the notes a heading drops the empty lines and anything back to a
/// heading right before it (`BuildEntries`).
#[test]
fn notes_drop_an_empty_heading() {
    let game = Game { extended_items: false };
    let lines = strings(&["#A", "", "#B", "text"]);
    let kept: Vec<String> = entries(&lines, false, true, SMALL, &game).into_iter().map(|e| e.payload).collect();
    assert_eq!(kept, ["A", "B", "text"]);
}

/// `ContentHeight` and `VisibleLineCount` in a 200-high scene with
/// `font-small.txt` (line 11, `GetHeight()` 8): 146 high, thirteen lines.
#[test]
fn a_200_high_scene_shows_thirteen_lines() {
    assert_eq!(content_height(200, 11), 146);
    assert_eq!(visible_lines(200, 11, 8), 13);
}

/// `Open`, `MovePage` (round five), `MoveTextScroll` (never above the top),
/// Back.
#[test]
fn paging_wraps_and_back_closes() {
    let mut w = Window::new(5);
    w.open();
    assert!(w.open && w.page == 0);
    w.press(Press::Left);
    assert_eq!(w.page, 4);
    w.press(Press::Right);
    w.press(Press::Right);
    assert_eq!(w.page, 1);
    w.press(Press::Up);
    assert_eq!(w.scroll(), 0);
    w.press(Press::Down);
    w.press(Press::Down);
    assert_eq!(w.scroll(), 2);
    w.clamp(1);
    assert_eq!(w.scroll(), 1);
    // Reopened: the first tab, every tab at its top.
    w.press(Press::Back);
    assert!(!w.open);
    w.open();
    w.move_page(1);
    assert_eq!((w.page, w.scroll()), (1, 0));
}

/// Held Up or Down: one line at once (the press), the first repeat after
/// 0.3 s, then every 0.06 s (`OnUpdate`).
#[test]
fn a_held_key_repeats_after_a_delay() {
    let mut w = Window::new(5);
    w.open();
    for _ in 0..18 {
        w.hold(1, 16);
    }
    assert_eq!(w.scroll(), 0, "288 ms: not yet");
    w.hold(1, 16);
    assert_eq!(w.scroll(), 1, "304 ms: the first repeat");
    for _ in 0..4 {
        w.hold(1, 16);
    }
    assert_eq!(w.scroll(), 2, "368 ms: one more after 60");
    // Let go and the delay starts again.
    w.hold(0, 16);
    w.hold(1, 16);
    assert_eq!(w.scroll(), 2);
}

/// The tab strip (`GetTabAt`): y 2–21, x from 10, 56 a tab (93 in the
/// notes); a click there picks the tab, a click on Close closes.
#[test]
fn tabs_and_close_by_mouse() {
    let mut w = Window::new(5);
    w.open();
    assert_eq!(w.tab_at(10, 2), Some(0));
    assert_eq!(w.tab_at(65, 21), Some(0));
    assert_eq!(w.tab_at(66, 10), Some(1));
    assert_eq!(w.tab_at(289, 10), Some(4));
    assert_eq!(w.tab_at(290, 10), None);
    assert_eq!(w.tab_at(100, 22), None);
    w.click(150, 10, (130, 183, 40, 10));
    assert_eq!(w.page, 2);
    w.click(140, 185, (130, 183, 40, 10));
    assert!(!w.open);
    assert_eq!(Window::new(3).tab_at(103, 10), Some(1));
}

/// `SetupPatchNotes.Format`.
#[test]
fn a_changelog_becomes_notes() {
    let md = "# Changelog\n\n## [1.2] - 2026\n\n### Fixed\n- one\n\n- two\n#### Deep\n";
    assert_eq!(patch_notes(md), strings(&["#1.2 - 2026", "##Fixed", "- one", "", "- two", "#Deep"]));
}

/// The sections of `manual.txt`: five tabs and the labels.
#[test]
fn the_v12_manual_has_five_tabs_and_labels() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/v1.2/lang/en/manual.txt");
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let s = sections(&text);
    assert_eq!(s.len(), 6);
    assert_eq!(s[0][0], "#CONQUER");
    assert_eq!(s[LABELS][0], "Damage T1: |A-|B / T4: |C-|D");
    assert_eq!(s[LABELS].len(), 20);
}

/// `ManualWindow.OnMouseWheel` (`ManualWindow.cs:840-844`): a turn up is a
/// line up, a turn down a line down, whatever its size; it reaches the
/// window only over it (`Window.MouseWheel`, `Boundings.PointInside`).
#[test]
fn the_wheel_scrolls_a_line_over_the_window() {
    let mut w = Window::new(5);
    w.open();
    assert!(w.wheel(true, -1));
    assert!(w.wheel(true, -3));
    assert_eq!(w.scroll(), 2);
    assert!(w.wheel(true, 120));
    assert_eq!(w.scroll(), 1);
    // Outside the window: not its to take.
    assert!(!w.wheel(false, -1));
    assert_eq!(w.scroll(), 1);
    // Never above the top.
    w.wheel(true, 1);
    w.wheel(true, 1);
    assert_eq!(w.scroll(), 0);
}
