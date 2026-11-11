//! A placement word means one thing wherever it is written: on a node's
//! line, in a view-level `when`, and from code. Also the words a code
//! write used to miss (`max_length`, `select`) and the checks a seat
//! written from code gets.

use gogo_ui::KeptFrame;
use gogo_ui::compile;
use gogo_ui::input::{Input, Key, Mods, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{ArrangementId, NodeId, Page, PropKey, Value};
use gogo_ui::runtime::{Item, ItemValue, Runtime, SELECTED, resolve_animated};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
/// The host's insets: left, top, right, bottom.
const INSETS: [f32; 4] = [10.0, 20.0, 30.0, 40.0];

fn page(src: &str) -> Page {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{src}\n{:#?}", c.diags);
    c.page
}

fn runtime(page: &Page) -> Runtime {
    let mut rt = Runtime::new(page);
    rt.safe = INSETS;
    rt
}

fn rect(page: &Page, rt: &mut Runtime, name: &str) -> Rect {
    let look = resolve_animated(page, rt);
    KeptFrame::fresh(page, look, VIEW, &FixedAdvance).rects()[page.find(name).unwrap_or_else(|| panic!("no node named {name}")).index()]
}

/// A Panel `q` in a Panel 360 x 600, with a Label `t` inside it.
fn host(q_words: &str) -> String {
    format!("View v {{\n Panel host width=360, height=600 {{\n Panel q {q_words}, background=#202020 {{ Label t text=\"inside\" }}\n }}\n}}")
}

/// `q`'s rect, then `t`'s: with the word on `q`'s line, with `q` at rest,
/// and with the word written by `when wide { q.<word> = <value> }`.
fn line_rest_when(own: &str, word: &str, value: &str) -> [(Rect, Rect); 3] {
    let on_line = page(&host(&format!("{own}, {word}={value}")));
    let mut rt = runtime(&on_line);
    let line = (rect(&on_line, &mut rt, "q"), rect(&on_line, &mut rt, "t"));
    let posed = page(&format!("arrangement wide\n{}\nwhen wide {{ q.{word} = {value} }}", host(own)));
    let mut rt = runtime(&posed);
    let rest = (rect(&posed, &mut rt, "q"), rect(&posed, &mut rt, "t"));
    rt.go_instant_id(ArrangementId::from_index(0));
    let when = (rect(&posed, &mut rt, "q"), rect(&posed, &mut rt, "t"));
    [line, rest, when]
}

fn assert_same_in_a_when(own: &str, word: &str, value: &str) {
    let [line, rest, when] = line_rest_when(own, word, value);
    assert_ne!(line, rest, "`{word}={value}` on the line changes the box (or the test proves nothing)");
    assert_eq!(when, line, "`q.{word} = {value}` in a `when` gives what the word gives on the line (at rest: {rest:?})");
}

/// `q`'s rect, then `t`'s: with the word on the line, and with the word
/// written from code onto a `q` whose line does not have it.
fn assert_same_from_code(own: &str, word: &str, value: &str, write: impl Fn(&mut Page) -> (PropKey, Value)) {
    let on_line = page(&host(&format!("{own}, {word}={value}")));
    let mut rt = runtime(&on_line);
    let line = (rect(&on_line, &mut rt, "q"), rect(&on_line, &mut rt, "t"));
    let mut plain = page(&host(own));
    let mut rt = runtime(&plain);
    let rest = (rect(&plain, &mut rt, "q"), rect(&plain, &mut rt, "t"));
    let (key, v) = write(&mut plain);
    rt.set_prop(plain.find("q").unwrap(), key, v);
    let code = (rect(&plain, &mut rt, "q"), rect(&plain, &mut rt, "t"));
    assert_ne!(line, rest, "`{word}={value}` on the line changes the box (or the test proves nothing)");
    assert_eq!(code, line, "`{word}` written from code gives what `{word}={value}` gives on the line (at rest: {rest:?})");
}

#[test]
fn min_size_in_a_when_is_min_size() {
    assert_same_in_a_when("halign=left, valign=top", "min_size", "333x222");
}

#[test]
fn a_fill_cap_in_a_when_caps_at_the_parent() {
    // Not 0 wide: `max_width = fill` is the parent's width, as on a line.
    let [line, rest, when] = line_rest_when("width=fill, height=80", "max_width", "fill");
    assert_eq!(line.0.w, 360.0);
    assert_eq!(when, line, "at rest: {rest:?}");
}

#[test]
fn a_weighted_fill_in_a_when_keeps_its_weight() {
    let on_line = page("View v {\n Row r width=300, height=60 {\n Button a text=\"a\", width=fill(3)\n Button b text=\"b\", width=fill\n }\n}");
    let mut rt = runtime(&on_line);
    let line = rect(&on_line, &mut rt, "a");
    assert_eq!(line.w, 225.0, "three shares of four");
    let posed = page("arrangement wide\nView v {\n Row r width=300, height=60 {\n Button a text=\"a\", width=fill\n Button b text=\"b\", width=fill\n }\n}\nwhen wide { a.width = fill(3) }");
    let mut rt = runtime(&posed);
    assert_eq!(rect(&posed, &mut rt, "a").w, 150.0, "at rest: one share of two");
    rt.go_instant_id(ArrangementId::from_index(0));
    assert_eq!(rect(&posed, &mut rt, "a"), line, "`a.width = fill(3)` in a `when` gives what `width=fill(3)` gives on the line");
}

#[test]
fn safe_in_a_when_adds_the_inset() {
    assert_same_in_a_when("width=100, height=80, valign=bottom", "margin_bottom", "safe");
    assert_same_in_a_when("width=100, height=80", "margin_left", "safe");
    assert_same_in_a_when("width=100, height=80", "padding_left", "safe");
}

/// A side the node's line declares `safe` and a `when` gives a number: the
/// number is the whole margin, the inset is gone.
#[test]
fn a_number_in_a_when_over_a_safe_side_drops_the_inset() {
    let posed = page(&format!("arrangement open\n{}\nwhen open {{ q.margin_bottom = 0 }}", host("width=100, height=80, valign=bottom, margin_bottom=safe")));
    let mut rt = runtime(&posed);
    assert_eq!(rect(&posed, &mut rt, "q").y, 600.0 - 80.0 - INSETS[3], "at rest the box is lifted by the bottom inset");
    rt.go_instant_id(ArrangementId::from_index(0));
    assert_eq!(rect(&posed, &mut rt, "q").y, 600.0 - 80.0, "`margin_bottom = 0` puts the box on its parent's bottom edge");
    let padded = page(&format!("arrangement open\n{}\nwhen open {{ q.padding_left = 4 }}", host("width=100, height=80, padding_left=safe")));
    let mut rt = runtime(&padded);
    assert_eq!(rect(&padded, &mut rt, "t").x, INSETS[0], "at rest the content starts after the left inset");
    rt.go_instant_id(ArrangementId::from_index(0));
    assert_eq!(rect(&padded, &mut rt, "t").x, 4.0, "`padding_left = 4` is the whole padding");
}

#[test]
fn a_theme_number_in_a_when_is_read() {
    let posed = page(&format!("theme {{\n $wide_w = 77\n}}\narrangement wide\n{}\nwhen wide {{ q.width = $wide_w }}", host("width=100, height=80")));
    let mut rt = runtime(&posed);
    assert_eq!(rect(&posed, &mut rt, "q").w, 100.0);
    rt.go_instant_id(ArrangementId::from_index(0));
    assert_eq!(rect(&posed, &mut rt, "q").w, 77.0, "`q.width = $wide_w` reads the theme's number");
}

#[test]
fn placement_words_written_from_code_read_as_on_the_line() {
    assert_same_from_code("halign=left, valign=top", "min_size", "333x222", |_| (PropKey::MinSize, Value::VSize(Some(333.0), Some(222.0))));
    assert_same_from_code("width=100, height=80, valign=bottom", "margin_bottom", "safe", |p| (PropKey::MarginBottom, Value::VIdent(p.intern("safe"))));
    assert_same_from_code("width=100, height=80", "padding_left", "safe", |p| (PropKey::PaddingLeft, Value::VIdent(p.intern("safe"))));
    assert_same_from_code("height=80", "width", "HUG", |p| (PropKey::Width, Value::VIdent(p.intern("HUG"))));
    // A fill cap from code is the parent's width, not 0.
    let mut capped = page(&host("width=fill, height=80"));
    let mut rt = runtime(&capped);
    let fill = Value::VIdent(capped.intern("fill"));
    rt.set_prop(capped.find("q").unwrap(), PropKey::MaxWidth, fill);
    assert_eq!(rect(&capped, &mut rt, "q").w, 360.0);
    // A number from code over a `safe` side is the whole margin.
    let lifted = page(&host("width=100, height=80, valign=bottom, margin_bottom=safe"));
    let mut rt = runtime(&lifted);
    rt.set_prop(lifted.find("q").unwrap(), PropKey::MarginBottom, Value::VNum(0.0));
    assert_eq!(rect(&lifted, &mut rt, "q").y, 600.0 - 80.0);
}

// ----- code writes of `max_length` and `select` ---------------------------

struct Field {
    page: Page,
    rt: Runtime,
    input: Input,
    field: NodeId,
}

impl Field {
    /// An Input named `f`, focused.
    fn new(words: &str) -> Field {
        let page = page(&format!("View v {{\n Input f width=200, height=40{words}\n}}"));
        let mut rt = Runtime::new(&page);
        let mut input = Input::default();
        let field = page.find("f").unwrap();
        let look = resolve_animated(&page, &mut rt);
        let kept = KeptFrame::fresh(&page, look, VIEW, &FixedAdvance);
        input.focus_on(&page, &mut rt, kept.frame(), Some(field));
        Field { page, rt, input, field }
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let look = resolve_animated(&self.page, &mut self.rt);
            let kept = KeptFrame::fresh(&self.page, look, VIEW, &FixedAdvance);
            self.input.key(&mut self.page, &mut self.rt, kept.frame(), Key::Char(c), Mods::NONE);
        }
    }

    fn text(&self) -> String {
        self.input.ime_seed(&self.page, &self.rt).expect("a focused field").0
    }
}

#[test]
fn a_code_write_of_max_length_caps_typing_paste_and_the_phone_keyboard() {
    let mut in_file = Field::new(", max_length=3");
    in_file.type_text("abcdef");
    assert_eq!(in_file.text(), "abc", "the file's cap");

    let mut typed = Field::new("");
    typed.rt.set_prop(typed.field, PropKey::MaxLength, Value::VNum(3.0));
    typed.type_text("abcdef");
    assert_eq!(typed.text(), "abc", "typing stops at the cap code wrote");

    let mut pasted = Field::new("");
    pasted.rt.set_prop(pasted.field, PropKey::MaxLength, Value::VNum(3.0));
    pasted.input.paste(&mut pasted.page, &mut pasted.rt, "abcdef");
    assert_eq!(pasted.text(), "abc", "a paste is cut at the cap code wrote");

    let mut phone = Field::new("");
    phone.rt.set_prop(phone.field, PropKey::MaxLength, Value::VNum(3.0));
    phone.input.ime_replace(&mut phone.page, &mut phone.rt, "abcdef", 0, 0);
    assert_eq!(phone.text(), "abc", "the phone keyboard's text is cut at the cap code wrote");

    // A cap code takes away again: the file's word counts again.
    let mut back = Field::new(", max_length=5");
    back.rt.set_prop(back.field, PropKey::MaxLength, Value::VNum(2.0));
    back.rt.clear_prop(back.field, PropKey::MaxLength);
    back.type_text("abcdefgh");
    assert_eq!(back.text(), "abcde");
}

struct Rows {
    page: Page,
    rt: Runtime,
    input: Input,
    list: NodeId,
    rows: Vec<NodeId>,
}

impl Rows {
    /// A List named `n` with three rows.
    fn new(select: &str) -> Rows {
        let mut page = page(&format!("View v {{\n List n select={select}, width=200, height=300 {{\n empty {{ Label e text=\"none\" }}\n repeat rows by $item.id {{\n ListItem {{ Label a text=$item.title }}\n }}\n }}\n}}"));
        let mut rt = Runtime::new(&page);
        let repeat = page.find("rows").unwrap();
        let items: Vec<Item> = (0..3).map(|i| Item::Record(vec![("id".to_string(), ItemValue::Int(i + 1)), ("title".to_string(), ItemValue::Str(format!("item {i}")))])).collect();
        rt.set_items(&mut page, repeat, &items).unwrap();
        let rows = rt.items(&page, repeat);
        let list = page.find("n").unwrap();
        Rows { page, rt, input: Input::default(), list, rows }
    }

    fn tap(&mut self, row: usize) {
        for (phase, t) in [(Phase::Down, 1.0), (Phase::Up, 1.05)] {
            let look = resolve_animated(&self.page, &mut self.rt);
            let kept = KeptFrame::fresh(&self.page, look, VIEW, &FixedAdvance);
            let r = kept.rects()[self.rows[row].index()];
            self.input.touch(&self.page, &mut self.rt, kept.frame(), &FixedAdvance, Touch { phase, x: r.x + r.w / 2.0, y: r.y + r.h / 2.0, t: t + row as f64, id: 1 });
        }
    }

    fn selected(&self) -> Vec<bool> {
        self.rows.iter().map(|r| self.rt.has(*r, SELECTED)).collect()
    }

    fn write_select(&mut self, word: &str) {
        let w = self.page.intern(word);
        self.rt.set_prop(self.list, PropKey::Select, Value::VIdent(w));
        // The selection follows the new mode at the next tick.
        self.rt.tick(&mut self.page, 0.0);
    }
}

#[test]
fn a_code_write_of_select_changes_what_a_tap_does() {
    let mut off = Rows::new("off");
    off.tap(0);
    assert_eq!(off.selected(), [false, false, false], "the file says no selection");
    off.write_select("one");
    off.tap(0);
    assert_eq!(off.selected(), [true, false, false], "code turned picking on: the tap picks the row");
    off.tap(1);
    assert_eq!(off.selected(), [false, true, false], "one at a time");
    off.write_select("many");
    off.tap(2);
    assert_eq!(off.selected(), [false, true, true], "code switched to many: a second row joins");
}

/// Code turns selection off, or from many to one: rows that the new mode
/// could not have picked let go.
#[test]
fn a_code_write_of_select_lets_go_of_what_the_new_mode_cannot_hold() {
    let mut many = Rows::new("many");
    many.tap(0);
    many.tap(2);
    assert_eq!(many.selected(), [true, false, true]);
    many.write_select("one");
    assert_eq!(many.selected(), [true, false, false], "one at a time: the first picked row stays");
    many.write_select("off");
    assert_eq!(many.selected(), [false, false, false], "no selection: nothing stays picked");
    many.tap(1);
    assert_eq!(many.selected(), [false, false, false], "and a tap picks nothing");
}

// ----- a seat written from code -------------------------------------------

const SEATS: &str = "View v {\n Panel left width=100, height=100 {\n Panel q width=40, height=40, background=#202020 {\n seat inner\n }\n }\n Panel right width=100, height=100, halign=right {\n seat s\n }\n Panel plain width=50, height=50, valign=bottom\n}";

fn seat_from_code(name: &str) -> Rect {
    let mut page = page(SEATS);
    let mut rt = Runtime::new(&page);
    let word = page.intern(name);
    rt.set_prop(page.find("q").unwrap(), PropKey::SeatOf, Value::VIdent(word));
    rect(&page, &mut rt, "q")
}

#[test]
fn a_seat_written_from_code_moves_the_node() {
    assert_eq!(seat_from_code("s").x, 300.0, "q sits in the seat of the right Panel");
    assert_eq!(seat_from_code("home").x, 0.0, "`home` is where its line put it");
}

/// A name that is no declared seat: a debug build stops, as a file's
/// `q.seat = plain` stops the build (G1016).
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "is not a declared seat")]
fn a_code_written_seat_that_is_no_seat_stops_a_debug_build() {
    seat_from_code("plain");
}

/// A seat inside the node that moves: a debug build stops with a message.
/// It used to hang the app.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "sits inside")]
fn a_code_written_seat_inside_the_moved_node_stops_a_debug_build() {
    seat_from_code("inner");
}

/// The same two names in a release build: the node stays at home.
#[test]
#[cfg(not(debug_assertions))]
fn a_code_written_seat_that_cannot_be_taken_leaves_the_node_at_home() {
    assert_eq!(seat_from_code("plain").x, 0.0);
    assert_eq!(seat_from_code("inner").x, 0.0);
}
