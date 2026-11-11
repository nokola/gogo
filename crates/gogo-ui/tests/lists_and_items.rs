//! Lists and the rows a `repeat` makes: the handler a List names, the
//! scroll bar's lane, repeats and `empty` blocks that could never show,
//! what an item's fields may feed, which row an item keeps, and two
//! warnings that fired on correct files.

use gogo_ui::KeptFrame;
use gogo_ui::compile;
use gogo_ui::draw::{Draw, draw};
use gogo_ui::input::{Event, EventKind, Input, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{Action, Color_Straight_Srgb_4xU8_RGBA, NodeId, Page, PropKey, Value};
use gogo_ui::runtime::{DISABLED, HELD, Item, ItemKey, ItemValue, Runtime, SELECTED, resolve_animated};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
/// The width of a scroll bar's lane, epx.
const LANE: f32 = 4.0;

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{}: {}\n", d.code, d.message)).collect()
}

fn has(src: &str, code: &str) -> bool {
    codes(src).iter().any(|c| c == code)
}

fn page(src: &str) -> Page {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{src}\n{:#?}", c.diags);
    c.page
}

fn record(id: i64, title: &str) -> Item {
    Item::Record(vec![("id".to_string(), ItemValue::Int(id)), ("title".to_string(), ItemValue::Str(title.to_string()))])
}

/// A page whose repeat is named `rows`, filled with `count` items.
struct Rows {
    page: Page,
    rt: Runtime,
    input: Input,
    rows: Vec<NodeId>,
}

impl Rows {
    fn new(src: &str, count: i64) -> Rows {
        let mut page = page(src);
        let mut rt = Runtime::new(&page);
        let repeat = page.find("rows").expect("a repeat named rows");
        let items: Vec<Item> = (1..=count).map(|i| record(i, &format!("item {i}"))).collect();
        rt.set_items(&mut page, repeat, &items).unwrap();
        let rows = rt.items(&page, repeat);
        Rows { page, rt, input: Input::default(), rows }
    }

    fn kept(&mut self) -> KeptFrame {
        let look = resolve_animated(&self.page, &mut self.rt);
        KeptFrame::fresh(&self.page, look, VIEW, &FixedAdvance)
    }

    fn rect(&mut self, node: NodeId) -> Rect {
        self.kept().rects()[node.index()]
    }

    fn draws(&mut self) -> Vec<Draw> {
        let kept = self.kept();
        draw(&self.page, kept.frame())
    }

    fn tap(&mut self, row: usize) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        for (phase, t) in [(Phase::Down, 1.0), (Phase::Up, 1.05)] {
            let kept = self.kept();
            let r = kept.rects()[self.rows[row].index()];
            events.extend(self.input.touch(&self.page, &mut self.rt, kept.frame(), &FixedAdvance, Touch { phase, x: r.x + r.w / 2.0, y: r.y + r.h / 2.0, t: t + row as f64, id: 1 }));
        }
        events
    }
}

fn list(list_words: &str, row_words: &str) -> String {
    format!("View v {{\n List n width=200, height=300{list_words} {{\n empty {{ Label e text=\"none\" }}\n repeat rows by $item.id {{\n ListItem{row_words} {{ Label a text=$item.title }}\n }}\n }}\n}}")
}

// ----- the handler a List names -------------------------------------------

/// `List n on:activate=pick`: a tap on a row reports `pick`, with no
/// handler written on the row.
#[test]
fn a_row_tap_reports_the_handler_its_list_names() {
    let mut fx = Rows::new(&list(", on:activate=pick", ""), 3);
    let events = fx.tap(1);
    let pick = fx.page.intern_lookup("pick");
    let tap: &Event = events.iter().find(|e| e.kind == EventKind::Activate).expect("the tap activates the row");
    assert_eq!(tap.node, fx.rows[1], "the event still names the row");
    assert_eq!(tap.action, Some(Action::Named(pick)), "and carries the List's handler");
}

/// A row that names its own handler keeps it.
#[test]
fn a_rows_own_handler_wins_over_its_lists() {
    let mut fx = Rows::new(&list(", on:activate=pick", " on:activate=row_pick"), 3);
    let events = fx.tap(0);
    let own = fx.page.intern_lookup("row_pick");
    let tap: &Event = events.iter().find(|e| e.kind == EventKind::Activate).expect("the tap activates the row");
    assert_eq!(tap.action, Some(Action::Named(own)));
}

// ----- the scroll bar's lane ----------------------------------------------

/// A List short enough to show every row: `bars=reserve` and
/// `bars=always` keep the bar's lane, so the rows do not change width
/// when the content outgrows the List.
#[test]
fn a_short_list_keeps_the_bars_lane_when_told_to() {
    let mut auto = Rows::new(&list("", ""), 3);
    let row = auto.rows[0];
    assert_eq!(auto.rect(row).w, 200.0, "no word: the rows span the List while nothing overflows");
    for word in ["reserve", "always"] {
        let mut fx = Rows::new(&list(&format!(", bars={word}"), ""), 3);
        let row = fx.rows[0];
        assert_eq!(fx.rect(row).w, 200.0 - LANE, "bars={word}: the lane is held with nothing to scroll");
    }
    let mut never = Rows::new(&list(", bars=never", ""), 40);
    let row = never.rows[0];
    assert_eq!(never.rect(row).w, 200.0, "bars=never: no lane, overflow or not");
}

fn bar_quads(draws: &[Draw], right: f32) -> usize {
    draws.iter().filter(|d| matches!(d, Draw::Quad { rect, .. } if rect.w == LANE && rect.x == right - LANE)).count()
}

/// `bars=always` draws the bar with nothing to scroll, on a List and on a
/// Scroll; `bars=reserve` holds the lane and draws no bar.
#[test]
fn bars_always_shows_the_bar_with_nothing_to_scroll() {
    let mut always = Rows::new(&list(", bars=always", ""), 3);
    assert_eq!(bar_quads(&always.draws(), 200.0), 1, "a List: {:#?}", always.draws());
    let mut reserve = Rows::new(&list(", bars=reserve", ""), 3);
    assert_eq!(bar_quads(&reserve.draws(), 200.0), 0);
    let scroll = |word: &str| format!("View v {{\n Scroll s width=200, height=300, bars={word} {{\n Column c {{\n Label a text=\"short\"\n }}\n }}\n}}");
    let p = page(&scroll("always"));
    let mut rt = Runtime::new(&p);
    let kept = KeptFrame::fresh(&p, resolve_animated(&p, &mut rt), VIEW, &FixedAdvance);
    assert_eq!(bar_quads(&draw(&p, kept.frame()), 200.0), 1, "a Scroll");
    let p = page(&scroll("reserve"));
    let mut rt = Runtime::new(&p);
    let kept = KeptFrame::fresh(&p, resolve_animated(&p, &mut rt), VIEW, &FixedAdvance);
    assert_eq!(bar_quads(&draw(&p, kept.frame()), 200.0), 0);
}

// ----- repeats and `empty` blocks that could never show --------------------

/// A repeat inside another repeat's row can never be filled: `set_items`
/// takes a repeat by its one name, and every row would carry that name.
#[test]
fn a_repeat_inside_a_repeats_row_is_a_build_error() {
    let src = "View v {\n Column c {\n repeat groups by $item.id {\n Column g {\n Label t text=$item.title\n repeat members by $item.id {\n Label m text=$item.title\n }\n }\n }\n }\n}";
    assert!(has(src, "G9018"), "{}", messages(src));
}

#[test]
fn a_repeat_with_no_name_is_a_build_error() {
    let src = "View v {\n Column c {\n repeat by $item.id {\n Label t text=$item.title\n }\n }\n}";
    assert!(has(src, "G9002"), "{}", messages(src));
    assert!(messages(src).contains("name"), "the message asks for the name: {}", messages(src));
}

#[test]
fn a_second_a_late_and_a_stray_empty_block_are_build_errors() {
    let repeat = |body: &str| format!("View v {{\n Column c {{\n repeat rows by $item.id {{\n{body}\n }}\n }}\n}}");
    let good = repeat(" empty { Label e text=\"none\" }\n Label t text=$item.title");
    assert!(!gogo_ui::has_errors(&compile(&good, &|_| None).diags), "{}", messages(&good));
    let second = repeat(" empty { Label e text=\"none\" }\n empty { Label f text=\"nothing\" }\n Label t text=$item.title");
    assert!(has(&second, "G9007"), "a second `empty`: {}", messages(&second));
    let late = repeat(" Label t text=$item.title\n empty { Label e text=\"none\" }");
    assert!(has(&late, "G9007"), "an `empty` after the row: {}", messages(&late));
    let stray = "View v {\n Column c {\n empty { Label e text=\"none\" }\n Label t text=\"x\"\n }\n}";
    assert!(has(stray, "G9007"), "an `empty` outside a repeat: {}", messages(stray));
}

// ----- two warnings that fired on correct files ----------------------------

/// G9015 tells the author to give the List a `height=`; a List that has
/// one is not warned.
#[test]
fn a_list_with_a_written_height_is_not_called_starved() {
    let src = |height: &str| format!("View v {{\n Scroll s {{\n Column c {{\n List n{height} {{\n empty {{ }}\n repeat rows by $item.id {{\n ListItem {{ Label a text=$item.title }}\n }}\n }}\n }}\n }}\n}}");
    assert!(has(&src(""), "G9015"), "with no height the warning stands: {}", messages(&src("")));
    assert!(!has(&src(" height=200"), "G9015"), "{}", messages(&src(" height=200")));
}

/// A Button with a `min_width` gives the wrapping label inside it a real
/// width to wrap at.
#[test]
fn a_wrap_label_in_a_floored_button_is_not_called_a_sliver() {
    let src = |floor: &str| format!("View v {{\n Button b{floor} halign=left {{\n Label cap text=\"rotate ninety degrees today\", wrap=true, width=fill\n }}\n}}");
    assert!(has(&src(""), "G1004"), "with no floor the warning stands: {}", messages(&src("")));
    assert!(!has(&src(" min_width=200,"), "G1004"), "{}", messages(&src(" min_width=200,")));
}

// ----- an item's fields ----------------------------------------------------

fn item(fields: &[(&str, ItemValue)]) -> Item {
    Item::Record(fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}

/// A row with a true/false, a number, a word, a size and a color fed by
/// the item.
const ROW_PAGE: &str = "View v {\n Column col width=300 {\n repeat rows by $item.id {\n Row r height=40 {\n Check c text=$item.title, selected=$item.done, disabled=$item.off\n Label l text=$item.title, opacity=$item.alpha, font_size=$item.size, visibility=$item.vis\n Rectangle bar width=$item.w, height=10, color=$item.tint\n }\n }\n }\n}";

fn full(id: i64, done: bool, off: bool, alpha: f32, size: i64, vis: &str, w: f32) -> Vec<(&'static str, ItemValue)> {
    vec![
        ("id", ItemValue::Int(id)),
        ("title", ItemValue::Str(format!("item {id}"))),
        ("done", ItemValue::Bool(done)),
        ("off", ItemValue::Bool(off)),
        ("alpha", ItemValue::Num(alpha)),
        ("size", ItemValue::Int(size)),
        ("vis", ItemValue::Str(vis.to_string())),
        ("w", ItemValue::Num(w)),
        ("tint", ItemValue::Color(Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255]))),
    ]
}

/// `Check selected=$item.done` and its kin build, and each row shows its
/// own item's values.
#[test]
fn an_items_field_feeds_a_bool_a_number_a_word_and_a_size() {
    let mut page = page(ROW_PAGE);
    let mut rt = Runtime::new(&page);
    let repeat = page.find("rows").unwrap();
    rt.set_items(&mut page, repeat, &[item(&full(1, true, false, 0.5, 30, "visible", 120.0)), item(&full(2, false, true, 1.0, 15, "gone", 40.0))]).unwrap();
    let rows = rt.items(&page, repeat);
    let a: Vec<NodeId> = page.children(rows[0]).collect();
    let b: Vec<NodeId> = page.children(rows[1]).collect();
    assert!(rt.has(a[0], SELECTED) && !rt.has(b[0], SELECTED), "`selected=` follows `done`");
    assert!(!rt.has(a[0], DISABLED) && rt.has(b[0], DISABLED), "`disabled=` follows `off`");
    let look = resolve_animated(&page, &mut rt);
    assert_eq!(look.num(&page, a[1], PropKey::Opacity), Some(0.5));
    assert_eq!(look.num(&page, a[1], PropKey::FontSize), Some(30.0));
    assert_eq!(look.num(&page, b[1], PropKey::FontSize), Some(15.0));
    let kept = KeptFrame::fresh(&page, look, VIEW, &FixedAdvance);
    assert_eq!(kept.rects()[a[2].index()].w, 120.0, "`width=` follows `w`");
    assert_eq!(kept.rects()[b[2].index()].w, 40.0);
    assert!(kept.rects()[a[1].index()].w > 0.0, "`visibility=visible`");
    assert_eq!(kept.rects()[b[1].index()].w, 0.0, "`visibility=gone`");
}

/// The docs' own row: the item's field reaches the Check through a
/// widget's parameter.
#[test]
fn an_items_field_passes_through_a_widgets_parameter() {
    let src = "widget TaskRow $title=\"\", $done=false {\n Row { Check c text=$title, selected=$done }\n}\nView v {\n Column col {\n repeat rows by $item.id {\n TaskRow title=$item.title, done=$item.done\n }\n }\n}";
    let mut page = page(src);
    let mut rt = Runtime::new(&page);
    let repeat = page.find("rows").unwrap();
    let task = |id: i64, done: bool| item(&[("id", ItemValue::Int(id)), ("title", ItemValue::Str(format!("task {id}"))), ("done", ItemValue::Bool(done))]);
    rt.set_items(&mut page, repeat, &[task(1, true), task(2, false)]).unwrap();
    let rows = rt.items(&page, repeat);
    let check = |row: NodeId| -> NodeId { page.children(row).next().expect("the row's Check") };
    assert!(rt.has(check(rows[0]), SELECTED), "the first task is done");
    assert!(!rt.has(check(rows[1]), SELECTED), "the second is not");
}

/// A field of the wrong type refuses the whole call and the rows shown
/// before stay: a number in a `text=`, text in a `color=`, a word the
/// property does not have.
#[test]
fn item_data_of_the_wrong_type_is_refused_whole() {
    let mut page = page(ROW_PAGE);
    let mut rt = Runtime::new(&page);
    let repeat = page.find("rows").unwrap();
    rt.set_items(&mut page, repeat, &[item(&full(1, true, false, 0.5, 30, "visible", 120.0)), item(&full(2, false, true, 1.0, 15, "gone", 40.0))]).unwrap();
    let wrong: Vec<(&str, ItemValue)> = vec![
        ("title", ItemValue::Int(7)),
        ("done", ItemValue::Str("yes".to_string())),
        ("alpha", ItemValue::Str("half".to_string())),
        ("vis", ItemValue::Str("maybe".to_string())),
        ("vis", ItemValue::Int(1)),
        ("w", ItemValue::Bool(true)),
        ("tint", ItemValue::Str("red".to_string())),
    ];
    for (field, bad) in wrong {
        let mut fields = full(3, false, false, 1.0, 15, "visible", 10.0);
        fields.iter_mut().find(|(k, _)| *k == field).unwrap().1 = bad.clone();
        let refused = rt.set_items(&mut page, repeat, &[item(&fields)]);
        let why: String = refused.expect_err(&format!("`{field}` given {bad:?} must be refused"));
        assert!(why.starts_with("G9101") && why.contains(&format!("`{field}`")), "the message names the field: {why}");
        assert_eq!(rt.items(&page, repeat).len(), 2, "the two rows shown before stay");
    }
    // A whole number where a number goes is a number.
    let mut fields = full(3, false, false, 1.0, 15, "visible", 10.0);
    fields.iter_mut().find(|(k, _)| *k == "alpha").unwrap().1 = ItemValue::Int(1);
    rt.set_items(&mut page, repeat, &[item(&fields)]).unwrap();
}

// ----- a row follows its key -----------------------------------------------

const PLAIN_ROWS: &str = "View v {\n Column col width=300 {\n repeat rows by $item.id {\n Button b text=$item.title\n }\n }\n}";

/// The text a row's Button shows.
fn shown(page: &Page, rt: &mut Runtime, row: NodeId) -> String {
    match resolve_animated(page, rt).prop(page, row, PropKey::Text) {
        Some(Value::VText { text, .. }) => page.str(text).to_string(),
        other => panic!("no text on the row: {other:?}"),
    }
}

/// Two items swap places and both change: each key keeps the row it had,
/// with what was on that row (here a held look), and shows its new text.
#[test]
fn a_changed_item_keeps_its_row() {
    let mut page = page(PLAIN_ROWS);
    let mut rt = Runtime::new(&page);
    let repeat = page.find("rows").unwrap();
    rt.set_items(&mut page, repeat, &[record(1, "a"), record(2, "b")]).unwrap();
    let before = rt.items(&page, repeat);
    rt.set_state(before[0], HELD, true);
    rt.set_items(&mut page, repeat, &[record(2, "c"), record(1, "d")]).unwrap();
    let after = rt.items(&page, repeat);
    assert_eq!(after, vec![before[1], before[0]], "each key keeps its row; the order follows the items");
    assert_eq!(shown(&page, &mut rt, after[0]), "c");
    assert_eq!(shown(&page, &mut rt, after[1]), "d");
    assert!(rt.has(before[0], HELD) && !rt.has(before[1], HELD), "the held look stays with key 1");
    assert_eq!(rt.item_key(&page, before[0]), Some(&ItemKey::Int(1)));
    assert_eq!(rt.item_key(&page, before[1]), Some(&ItemKey::Int(2)));
}

/// A key that leaves frees its row, and the next new key takes it.
#[test]
fn a_key_that_leaves_frees_its_row_for_a_new_key() {
    let mut page = page(PLAIN_ROWS);
    let mut rt = Runtime::new(&page);
    let repeat = page.find("rows").unwrap();
    rt.set_items(&mut page, repeat, &[record(1, "one"), record(2, "two"), record(3, "three")]).unwrap();
    let before = rt.items(&page, repeat);
    rt.set_items(&mut page, repeat, &[record(1, "one"), record(3, "three")]).unwrap();
    assert_eq!(rt.items(&page, repeat), vec![before[0], before[2]], "keys 1 and 3 keep their rows");
    rt.set_items(&mut page, repeat, &[record(1, "one"), record(3, "three"), record(4, "four")]).unwrap();
    let after = rt.items(&page, repeat);
    assert_eq!(after, vec![before[0], before[2], before[1]], "key 4 takes the row key 2 left");
    assert_eq!(shown(&page, &mut rt, after[2]), "four");
    assert_eq!(rt.item_key(&page, after[2]), Some(&ItemKey::Int(4)));
}

/// String keys, reordered and partly changed, with a row in the middle
/// gone: every key that stays keeps its row.
#[test]
fn string_keys_keep_their_rows_through_a_reorder() {
    let src = PLAIN_ROWS.replace("$item.id", "$item.name");
    let named = |name: &str, title: &str| Item::Record(vec![("name".to_string(), ItemValue::Str(name.to_string())), ("title".to_string(), ItemValue::Str(title.to_string()))]);
    let mut page = page(&src);
    let mut rt = Runtime::new(&page);
    let repeat = page.find("rows").unwrap();
    rt.set_items(&mut page, repeat, &[named("ann", "1"), named("bob", "2"), named("cy", "3"), named("dee", "4")]).unwrap();
    let before = rt.items(&page, repeat);
    rt.set_items(&mut page, repeat, &[named("dee", "4"), named("ann", "one"), named("cy", "3")]).unwrap();
    let after = rt.items(&page, repeat);
    assert_eq!(after, vec![before[3], before[0], before[2]]);
    assert_eq!(shown(&page, &mut rt, after[1]), "one");
}
