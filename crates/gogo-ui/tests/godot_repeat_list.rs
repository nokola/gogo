//! Port of the Godot gogo suite's repeat + List groups
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//! - helpers `_repeat_texts` … `_list_texts` 12456-12509 (no checks; the
//!   `texts` helper below is their draw-list twin: visible label texts in
//!   tree order),
//! - `_repeat_tests` 12509-12758 (58 checks),
//! - `_list_tests` 12758-13498 (131 checks).
//!
//! Godot hands a repeat its data through `set_items` (a keyed collection,
//! reconciled by key). The Rust port builds `repeat x max=N` items at
//! compile time and code shows the first n (`Runtime::set_count`) and
//! writes each item's text (`Runtime::set_text`) — the same picture, a
//! different hand-over; the keyed reconcile and `$item.field` bindings are
//! owed under GP27. The Godot fixture `repeat_view.gogo` is inlined and
//! translated (`by $item.id` → `max=2`, `text=$item.name` → `text=""`).
//!
//! NEW deviation found here (no decision, GPAR or GP covers it):
//! `list_level_empty_seat_hides_once_rows_exist` — the List-level
//! `empty { }` seat (the Godot List form, D128) keeps drawing under the
//! rows once items are present.
//!
//! ENGINE skips (Godot machinery with no Rust counterpart):
//! - `repeat.scrub-ok` / `repeat.scrub` (12631-12641): epx re-apply
//!   binding bookkeeping on freed nodes.
//! - `repeat.view-hook-set-items` (12692-12700): `rebuild_into`'s
//!   `on_built` hook ordering.
//! - `list.root-class` (12781): the root's GDScript class.
use gogo::draw::{Draw, draw};
use gogo::input::{Event, EventKind, Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Action, Kind, NodeId, PropKey, palette_color};
use gogo::runtime::{DISABLED, HELD, Runtime, SELECTED, resolve};
use gogo::{Page, compile};

const FRAME: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };

fn ok(src: &str) -> Page {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{src}\n{:#?}", c.diags);
    c.page
}

fn builds(src: &str) -> bool {
    !gogo::has_errors(&compile(src, &|_| None).diags)
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn warnings(src: &str) -> Vec<String> {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{src}\n{:#?}", c.diags);
    c.diags.iter().filter(|d| d.severity == gogo::Severity::Warning).map(|d| format!("{} {}", d.code, d.message)).collect()
}

fn lay(page: &Page, rt: &Runtime) -> Vec<Rect> {
    let res = resolve(page, rt);
    layout(page, &res, FRAME, &FixedAdvance)
}

/// `_repeat_texts` / `_list_texts`: every label text that reaches the
/// screen, in tree order (a `gone` node draws nothing, so the `empty`
/// block's text appears only while it shows).
fn texts(page: &Page, rt: &Runtime) -> Vec<String> {
    let res = resolve(page, rt);
    let rects = layout(page, &res, FRAME, &FixedAdvance);
    draw(page, &res, &rects).iter().filter_map(|d| if let Draw::Text { text, .. } = d { Some(page.str(*text).to_string()) } else { None }).collect()
}

/// The node that shows the item's text: the item itself when it has no
/// children (a Label or Button template), else the first Label under it
/// (`ListItem { Label text=$item.title }`).
fn label_of(page: &Page, item: NodeId) -> NodeId {
    let mut n = item;
    loop {
        let kids = page.children(n);
        if kids.is_empty() {
            return n;
        }
        if let Some(l) = kids.clone().find(|c| page.nodes[*c].kind == Kind::Label) {
            return l as NodeId;
        }
        n = kids.start as NodeId;
    }
}

/// The Rust hand-over for `rep.assign([...])`: one text per item, then the
/// count.
fn fill(page: &mut Page, rt: &mut Runtime, repeat: NodeId, rows: &[&str]) {
    let items = rt.items(page, repeat);
    assert!(rows.len() <= items.len(), "{} rows into a max={} repeat", rows.len(), items.len());
    for (i, text) in rows.iter().enumerate() {
        let label = label_of(page, items[i]);
        rt.set_text(page, label, text);
    }
    rt.set_count(page, repeat, rows.len());
}

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let page = ok(src);
        let rt = Runtime::new(&page);
        Fx { page, rt, input: Input::new() }
    }
    fn items(&self, repeat: &str) -> Vec<NodeId> {
        self.rt.items(&self.page, self.page.find(repeat).unwrap_or_else(|| panic!("no repeat {repeat}")))
    }
    fn fill(&mut self, repeat: &str, rows: &[&str]) {
        let r = self.page.find(repeat).unwrap_or_else(|| panic!("no repeat {repeat}"));
        fill(&mut self.page, &mut self.rt, r, rows);
    }
    fn rects(&self) -> Vec<Rect> {
        lay(&self.page, &self.rt)
    }
    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, FRAME, &FixedAdvance);
        self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase, x, y, t, id: 1 })
    }
    /// A tap at the middle of `node` (Godot's `_conf_press(at, true/false)`).
    fn tap(&mut self, node: NodeId, t: f64) -> Vec<Event> {
        let r = self.rects()[node as usize];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let mut ev = self.touch(Phase::Down, x, y, t);
        ev.extend(self.touch(Phase::Up, x, y, t + 0.1));
        ev
    }
    fn opacity(&self, node: NodeId) -> f32 {
        resolve(&self.page, &self.rt).num(&self.page, node, PropKey::Opacity).unwrap_or(1.0)
    }
}

// ------------------------------------------------------------ _repeat_tests

const TASKS: &str = r#"View v {
    Column list spacing=4, halign=left, valign=top {
        Label header text="Tasks"
        repeat tasks max=5 {
            empty { Label text="none yet" }
            Label text=""
        }
        Label footer text="end"
    }
}"#;

/// `_repeat_tests` 12518-12545 + 12583-12592 (`repeat.ok`, `.registered`,
/// `.marker`, `.initial-empty`, `.assign`, `.order`, `.reorder`, `.clear`,
/// `.refill`): a repeat in a Column is an addressable node that takes no
/// room of its own; at zero items the `empty` text shows between the
/// statics; handing it rows shows them in order, in the repeat's spot; a
/// new order shows in the new order; zero clears back to the `empty`
/// text and a refill returns.
#[test]
fn repeat_stack_shows_items_by_count() {
    let mut fx = Fx::new(TASKS);
    let tasks = fx.page.find("tasks").expect("repeat.registered");
    assert_eq!(fx.page.nodes[tasks as usize].kind, Kind::Repeat);
    let r = fx.rects();
    assert_eq!((r[tasks as usize].w, r[tasks as usize].h), (0.0, 0.0), "repeat.marker: the repeat itself takes no room: {:?}", r[tasks as usize]);
    let header = fx.page.find("header").unwrap() as usize;
    let footer = fx.page.find("footer").unwrap() as usize;
    assert_eq!(texts(&fx.page, &fx.rt), ["Tasks", "none yet", "end"], "repeat.initial-empty");
    fx.fill("tasks", &["a", "b", "c"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["Tasks", "a", "b", "c", "end"], "repeat.order");
    let r = fx.rects();
    let items = fx.items("tasks");
    // The items sit between the statics, spaced like siblings (4).
    assert_eq!(r[items[0] as usize].y, r[header].bottom() + 4.0, "{r:?}");
    assert_eq!(r[footer].y, r[items[2] as usize].bottom() + 4.0, "{r:?}");
    fx.fill("tasks", &["c", "b", "a"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["Tasks", "c", "b", "a", "end"], "repeat.reorder");
    fx.fill("tasks", &[]);
    assert_eq!(texts(&fx.page, &fx.rt), ["Tasks", "none yet", "end"], "repeat.clear");
    fx.fill("tasks", &["back"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["Tasks", "back", "end"], "repeat.refill");
}

/// `_repeat_tests` 12546-12560 (`repeat.reorder-keeps-nodes`,
/// `.changed-rebuilds`): Godot reconciles by key — a pure reorder keeps
/// each item's node, changed data rebuilds exactly that item. Rust has
/// fixed item slots and no keyed hand-over yet.
#[test]
#[ignore = "GP27: not ported: reconcile-by-key set_items (owed in GP27's SHIPPED line)"]
fn repeat_reconcile_keeps_nodes_by_key() {
    panic!("Godot: assign([3,2,1]) after [1,2,3] keeps the node showing `b` (same node), assign([3, 2:'B']) rebuilds only b's node — Rust: no keyed set_items; items are fixed slots filled by set_text");
}

/// `_repeat_tests` 12562-12582 (`repeat.refuse-dup`, `.refuse-missing`,
/// `.refuse-float`, `.refuse-mixed`, `.refuse-null`, `.refuse-nonarray`,
/// `.refuse-template`, `.refusals-kept-items`): bad data refuses the
/// whole assignment with G9101/G9102/G9103 and the last good items stay.
#[test]
#[ignore = "GP27: not ported: reconcile-by-key set_items with data validation (G9101-G9103)"]
fn repeat_refuses_bad_data_whole() {
    panic!("Godot: duplicate key = G9103 naming both rows, missing/float/mixed key = G9102, null/non-array/missing bound field = G9101, and the shown items do not change — Rust: no data hand-over to refuse");
}

/// `_repeat_tests` 12594-12610 (`repeat.as-ok`, `.as-binding`): `repeat
/// brushes as $brush by $brush.id` renames the item binding and
/// `text=$brush.name` reads the item's field.
#[test]
#[ignore = "GP27: not ported: `$item.field` bindings (Rust: G9001 unresolved `$brush.name`)"]
fn repeat_renamed_binding_reads_item_fields() {
    let src = "View v { Column c { repeat brushes as $brush by $brush.id, max=1 { empty { } Label text=$brush.name } } }";
    assert!(builds(src), "{:?}", codes(src));
}

/// `_repeat_tests` 12594-12610 (`repeat.empty-bare-initial`,
/// `.empty-bare-clear`): a literally empty `empty { }` is legal and shows
/// nothing at zero items — before any data and after a clear.
#[test]
fn repeat_bare_empty_block_shows_nothing() {
    let mut fx = Fx::new("View v { Column c { repeat brushes max=2 { empty { } Label text=\"\" } } }");
    assert!(texts(&fx.page, &fx.rt).is_empty(), "repeat.empty-bare-initial: {:?}", texts(&fx.page, &fx.rt));
    fx.fill("brushes", &["round"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["round"]);
    fx.fill("brushes", &[]);
    assert!(texts(&fx.page, &fx.rt).is_empty(), "repeat.empty-bare-clear: {:?}", texts(&fx.page, &fx.rt));
}

/// `_repeat_tests` 12612-12618 (`repeat.prim-ok`, `.prim`): `repeat names
/// by $item { Label text=$item }` binds a primitive item straight to the
/// label.
#[test]
#[ignore = "GP27: not ported: `$item` bindings (Rust: G9001 unresolved `$item`)"]
fn repeat_primitive_items_bind_dollar_item() {
    let src = "View v { Column c { repeat names by $item, max=2 { empty { } Label text=$item } } }";
    assert!(builds(src), "{:?}", codes(src));
}

/// `_repeat_tests` 12620-12641 (`repeat.duo-ok`, `.duo-spans`,
/// `.duo-anchored`): two repeats in one parent each keep their own span —
/// refilling the first never moves the second's items.
#[test]
fn two_repeats_keep_their_own_spans() {
    let mut fx = Fx::new("View v { Column c { repeat one max=2 { empty { } Label text=\"\" } repeat two max=1 { empty { } Label text=\"\" } } }");
    fx.fill("one", &["a", "b"]);
    fx.fill("two", &["x"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["a", "b", "x"], "repeat.duo-spans");
    fx.fill("one", &["c"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["c", "x"], "repeat.duo-anchored");
}

/// `_repeat_tests` 12643-12660 (`repeat.wire-ok`, `.local-names`): a
/// handler inside the body compiles, and body names stay per item — the
/// page has no `rowButton`, each item owns `rows/<i>/rowButton`.
#[test]
fn repeat_body_names_are_per_item() {
    let mut fx = Fx::new("View v { Column c width=200, halign=left, valign=top { repeat rows max=2 { empty { } Button rowButton text=\"\", on:activate=OnRowActivate } } }");
    fx.fill("rows", &["x", "y"]);
    assert!(fx.page.find("rowButton").is_none(), "repeat.local-names");
    let y = fx.page.find("rows/1/rowButton").expect("the second item's button");
    let ev = fx.tap(y, 0.0);
    assert!(ev.iter().any(|e| e.node == y && e.kind == EventKind::Activate && matches!(e.action, Some(Action::Named(id)) if fx.page.str(id) == "OnRowActivate")), "{ev:?}");
}

/// `_repeat_tests` 12655-12660 (`repeat.wire-key`): the row's handler
/// receives the item's KEY as its trailing argument (spec §2.5). Rust's
/// event names the item node; there is no key to pass.
#[test]
#[ignore = "GP27: not ported: the item key as the handler's trailing argument"]
fn repeat_handler_carries_the_item_key() {
    panic!("Godot: tapping the `y` row's button calls OnRowActivate(9) (its key) — Rust: Event.node = rows/1/rowButton, no key value");
}

/// `_repeat_tests` 12643-12660 + 12662-12670 (`repeat.err-top-level`,
/// `.err-dup-name`, `.err-dotted-outside`): the refusals both runtimes
/// share — a repeat cannot stand alone (Rust: G7003, the root must be a
/// View/Panel; Godot: "needs a container"), two repeats of one name are
/// one name used twice (Rust G0044, Godot G9001), and a dotted `$a.b`
/// outside a repeat resolves to nothing (Rust G9001, Godot G9008).
#[test]
fn repeat_build_errors_both_runtimes_share() {
    assert!(codes("repeat t max=1 { Label text=\"x\" }").contains(&"G7003".to_string()), "repeat.err-top-level: {:?}", codes("repeat t max=1 { Label text=\"x\" }"));
    let dup = "View v { Column c { repeat t max=1 { empty { } Label text=\"x\" }; repeat t max=1 { empty { } Label text=\"y\" } } }";
    assert!(codes(dup).contains(&"G0044".to_string()), "repeat.err-dup-name: {:?}", codes(dup));
    assert!(codes("View v { Label text=$a.b }").contains(&"G9001".to_string()), "repeat.err-dotted-outside: {:?}", codes("View v { Label text=$a.b }"));
}

/// `_repeat_tests` 12664 (`repeat.err-nameless`): a repeat needs a name
/// (Godot G9001) — the name is the code address. Rust builds a nameless
/// repeat.
#[test]
#[ignore = "GP41: Rust accepts a nameless repeat (Godot G9001)"]
fn repeat_needs_a_name() {
    let src = "View v { Column c { repeat max=1 { empty { } Label text=\"x\" } } }";
    assert!(!builds(src), "Godot refuses a nameless repeat; Rust built it: {:?}", codes(src));
}

/// `_repeat_tests` 12674 (`repeat.err-nested`) + `_list_tests` 13473
/// (`list.err-nested-in-repeat`): a repeat (or a List) inside a repeat
/// body is refused (Godot G9005). Rust builds N×M copies.
#[test]
#[ignore = "GP41: Rust accepts a repeat/List nested inside a repeat body (Godot G9005)"]
fn repeat_does_not_nest() {
    let src = "View v { Column c { repeat t max=1 { empty { } Column { repeat u max=1 { empty { } Label text=\"x\" } } } } }";
    assert!(!builds(src), "repeat.err-nested: {:?}", codes(src));
    let list = "View v { Column { repeat outer max=1 { empty { } List l { empty { } repeat t max=1 { ListItem { Label text=\"x\" } } } } } }";
    assert!(!builds(list), "list.err-nested-in-repeat: {:?}", codes(list));
}

/// `_repeat_tests` 12676-12686 + 12702 (`repeat.err-empty-missing`,
/// `.err-empty-late`, `.err-empty-dup`, `.err-empty-stray`,
/// `.err-no-template`): the `empty { }` block is required, first, single,
/// only inside a repeat, and never the whole body (Godot G9007 / "repeat"
/// / "template"). Rust builds every one of these.
#[test]
#[ignore = "GP41: Rust accepts a missing/late/duplicate/stray `empty` block and a template-less repeat (Godot G9007)"]
fn repeat_empty_block_rules() {
    let cases = [
        ("repeat.err-empty-missing", "View v { Column c { repeat t max=1 { Label text=\"x\" } } }"),
        ("repeat.err-empty-late", "View v { Column c { repeat t max=1 { Label text=\"x\"; empty { Label text=\"e\" } } } }"),
        ("repeat.err-empty-dup", "View v { Column c { repeat t max=1 { empty { Label text=\"e\" }; empty { Label text=\"f\" }; Label text=\"x\" } } }"),
        ("repeat.err-empty-stray", "View v { Column c { empty { Label text=\"x\" } } }"),
        ("repeat.err-no-template", "View v { Column c { repeat t max=1 { empty { Label text=\"e\" } } } }"),
    ];
    for (name, src) in cases {
        assert!(!builds(src), "{name}: Godot refuses, Rust built it: {:?}", codes(src));
    }
}

/// `_repeat_tests` 12688 (`repeat.err-props`): the repeat line takes no
/// look/size props (Godot G9009) — they belong on the parent or the
/// template. Rust stores `spacing=4` on the repeat node and nobody reads it.
#[test]
#[ignore = "GP41: Rust accepts props on the repeat line (Godot G9009)"]
fn repeat_line_takes_no_props() {
    let src = "View v { Column c { repeat t max=1, spacing=4 { empty { } Label text=\"x\" } } }";
    assert!(!builds(src), "{:?}", codes(src));
}

/// `_repeat_tests` 12666-12672 + 12704 (`repeat.err-no-by`,
/// `.err-by-root`, `.err-collision`, `.err-as-after-by`): the clause
/// fences — `by` required (G9002), `by`'s root must be the binding
/// (G9003), `$item` may not also be a theme value (G9004), `as` comes
/// before `by`. Rust parses the clauses and drops them.
#[test]
#[ignore = "GPAR24: repeat clause fences gone — `as`/`by` in any order, unchecked, unread"]
fn repeat_clause_fences() {
    let cases = [
        ("repeat.err-no-by", "View v { Column c { repeat t { empty { } Label text=\"x\" } } }"),
        ("repeat.err-by-root", "View v { Column c { repeat t as $a by $b.id { empty { } Label text=\"x\" } } }"),
        ("repeat.err-collision", "$item = 5\nView v { Column c { repeat t by $item.id { empty { } Label text=\"x\" } } }"),
        ("repeat.err-as-after-by", "View v { Column c { repeat t by $item.id as $x { empty { } Label text=\"x\" } } }"),
    ];
    for (name, src) in cases {
        assert!(!builds(src), "{name}: Godot refuses, Rust built it: {:?}", codes(src));
    }
}

/// `_repeat_tests` 12706 (`repeat.err-dotted-theme`): `$a.b = 4` — a
/// theme value name holds no `.` (that spelling is the item-field path).
/// Rust declares it.
#[test]
#[ignore = "GPAR25: `.` allowed in `$names` (Godot refuses `$a.b = 4`)"]
fn dotted_theme_value_name_refused() {
    let src = "$a.b = 4\nView v { Label text=\"x\" }";
    assert!(!builds(src), "{:?}", codes(src));
}

/// `_repeat_tests` 12678 (`repeat.err-widget-body`) + `_list_tests` 13471
/// (`list.err-in-widget`): Godot refuses a repeat or a List inside a
/// widget body (G9006 — "its name is the set_items address, and a body
/// used twice would split it"). FLIP: Rust scopes widget-body names per
/// instance (GPAR31 SHIPPED, `inst/name`), so the address never splits,
/// and the decided FanRust form of a code-behind control IS a widget
/// holding a repeat (todo-gogo.md Decisions 2026-08-30: `widget X { }` +
/// `repeat marks max=61`; GP30 SHIPPED `widget GradientEdit`).
#[test]
fn repeat_and_list_inside_a_widget_body_are_legal() {
    let src = "widget W { Column { repeat t max=2 { empty { } Label text=\"x\" } } }\nView v { W a\n W b }";
    let page = ok(src);
    assert!(page.find("a/t").is_some() && page.find("b/t").is_some(), "one repeat per instance");
    let list = "widget W { Column { List l { empty { } repeat t max=1 { ListItem { Label text=\"x\" } } } } }\nView v { W w }";
    assert!(builds(list), "list.err-in-widget flipped: {:?}", codes(list));
}

/// Godot fixture `test/fixtures/repeat_view.gogo`, translated (`by
/// $item.id` → `max=2`, `text=$item.name` → `text=""`).
const REPEAT_VIEW: &str = r#"View v {
    Column list {
        repeat rows max=2 {
            empty { Label text="zero" }
            Label text=""
        }
    }
}
"#;

/// `_repeat_tests` 12708-12722 (`repeat.view-ok`, `.view-initial`,
/// `.view-set`, `.view-items`, `.view-unknown`): the page-level entry —
/// the fixture shows "zero" until code hands it rows, then the rows; an
/// unknown repeat name is nothing to write to (Godot: `set_items` returns
/// false; Rust: names become code, `find` is `None`).
#[test]
fn repeat_view_fixture_takes_items_by_name() {
    let mut fx = Fx::new(REPEAT_VIEW);
    assert_eq!(texts(&fx.page, &fx.rt), ["zero"], "repeat.view-initial");
    fx.fill("rows", &["a", "b"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["a", "b"], "repeat.view-items");
    assert!(fx.page.find("nope").is_none(), "repeat.view-unknown");
}

/// `_repeat_tests` 12723-12729 (`repeat.view-rebuild-ok`,
/// `.view-reapply`): the stashed collection re-applies across a rebuild
/// (theme flip / hot reload) — the rebuilt list shows the same rows.
#[test]
#[ignore = "GP42: not ported: hot-reload baseline carry (items re-applied across a rebuild)"]
fn repeat_items_survive_a_rebuild() {
    panic!("Godot: rebuild_into the same file after set_items([a, b]) shows [a, b] again — Rust: a fresh Runtime::new starts at zero items");
}

// -------------------------------------------------------------- _list_tests

const PICKER: &str = r#"View v {
    Column outer width=200, halign=left, valign=top {
        List picker spacing=2 {
            Label header text="Tasks"
            empty { Label text="none yet" }
            repeat tasks max=40 {
                ListItem { Label text="" }
            }
        }
    }
}"#;

/// `_list_tests` 12770-12792 + 12805-12811 + 12878-12882 (`list.builds`,
/// `.spacing`, `.initial-empty`, `.repeat-addressable`, `.row-roots`,
/// `.rows-full-width`): a List builds as a stack — statics and the
/// `empty` text at zero, the rows in order with `spacing` between them;
/// every row root is a `ListItem` box and takes the stack's full width,
/// however narrow its content.
#[test]
fn list_builds_as_a_spaced_stack() {
    let mut fx = Fx::new(PICKER);
    assert_eq!(texts(&fx.page, &fx.rt), ["Tasks", "none yet"], "list.initial-empty");
    assert!(fx.page.find("tasks").is_some(), "list.repeat-addressable");
    fx.fill("tasks", &["a", "b", "c"]);
    let shown = texts(&fx.page, &fx.rt);
    assert!(shown.starts_with(&["Tasks".to_string()]) && shown.ends_with(&["a".to_string(), "b".to_string(), "c".to_string()]), "rows in order after the header: {shown:?}");
    let items = fx.items("tasks");
    assert!(items.iter().all(|i| fx.page.nodes[*i as usize].kind == Kind::ListItem && Kind::ListItem.is_free()), "list.row-roots");
    let r = fx.rects();
    let picker = fx.page.find("picker").unwrap() as usize;
    assert_eq!(r[items[1] as usize].y, r[items[0] as usize].bottom() + 2.0, "list.spacing: {r:?}");
    assert_eq!(r[items[2] as usize].y, r[items[1] as usize].bottom() + 2.0, "list.spacing: {r:?}");
    let shown_rows: Vec<Rect> = items[..3].iter().map(|i| r[*i as usize]).collect();
    assert!(r[picker].w > 150.0 && shown_rows.iter().all(|row| row.w == r[picker].w), "list.rows-full-width: picker={:?} rows={shown_rows:?}", r[picker]);
}

/// `_list_tests` 12808-12809 (`list.items`): once rows are present the
/// List-level `empty { Label text="none yet" }` seat hides (D128) — the
/// shown texts are the header and the rows, nothing else. Rust treats
/// the List-level `empty` as an ordinary child: it stays visible under
/// the rows (a repeat's OWN `empty { }` does hide, see
/// `repeat_stack_shows_items_by_count`).
#[test]
#[ignore = "NEW: Godot hides the List-level `empty` seat once rows exist vs Rust keeps drawing it ([Tasks, none yet, a, b, c])"]
fn list_level_empty_seat_hides_once_rows_exist() {
    let mut fx = Fx::new(PICKER);
    fx.fill("tasks", &["a", "b", "c"]);
    assert_eq!(texts(&fx.page, &fx.rt), ["Tasks", "a", "b", "c"], "list.items");
}

/// `_list_tests` 12782-12786 + 12813-12827 (`list.signals`,
/// `.mode-default`, `.selected-initial`, `.select-checks`,
/// `.changed-fired`, `.same-write-silent`, `.null-clears`): the List owns
/// the selection — `select=one` by default, `selected` starts null, a
/// code write checks that row and fires `change` once, a same-value
/// write is silent, null clears. Rust's List is a Column: no `selected`,
/// and `on:activate` on it is even refused (G0072).
#[test]
#[ignore = "GP28: not ported: List selection owner (`selected`, `change`, `activate(key,item)` on the List)"]
fn list_owns_selection_and_reports_change() {
    let src = "View v { List picker on:change=pick, on:activate=open { empty { } repeat tasks max=3 { ListItem { Label text=\"\" } } } }";
    assert!(builds(src), "list.signals: a List emits change and activate; Rust: {:?}", codes(src));
    panic!("Godot: picker.selected = 2 checks row b, change fires [2]; the same write is silent; null clears — Rust: no List-level selection");
}

/// `_list_tests` 12793-12804 + 12862-12877 + 12883-12887
/// (`list.scroll-host`, `.scroll-follows-focus`, `.expands-default`,
/// `.scrolls`, `.scroll-starts-top`, `.follow-focus-scrolls`): the List
/// self-scrolls (an internal scroll host holding statics AND rows), starts
/// at the top, keeps the cursor in view, and an unplaced List EXPANDS to
/// the room its parent offers. Rust's List is a plain Column: it hugs its
/// rows and never pans.
#[test]
#[ignore = "GP28: not ported: List self-scroll + viewport doctrine (observed: an unplaced List hugs its rows)"]
fn list_self_scrolls_and_expands_by_default() {
    let mut fx = Fx::new("View v { Column outer width=200, height=300, halign=left, valign=top { Button b1 text=\"New Game\"; List picker { empty { } repeat tasks max=3 { ListItem { Label text=\"\" } } } } }");
    fx.fill("tasks", &["a", "b", "c"]);
    let r = fx.rects();
    let picker = fx.page.find("picker").unwrap() as usize;
    let outer = fx.page.find("outer").unwrap() as usize;
    assert_eq!(r[picker].bottom(), r[outer].bottom(), "list.expands-default: an unplaced List fills the column's leftover room: {:?} in {:?}", r[picker], r[outer]);
}

/// `_list_tests` 12829-12841 (`list.reorder-keeps`, `.rebuild-rechecks`,
/// `.clear-keeps-inert`, `.inert-reattaches`): the selection is kept BY
/// KEY across reconciles — a reorder moves it, changed data re-checks the
/// fresh row, `[]` leaves it inert, the key coming back re-attaches it.
#[test]
#[ignore = "GP28: not ported: selection kept by key across a keyed reconcile (needs GP27's set_items too)"]
fn list_selection_survives_reconcile_by_key() {
    panic!("Godot: selected=2 stays on `b` through reorder, rebuild (`B!`), clear (inert, still 2) and refill — Rust: no List selection, no keyed reconcile");
}

/// `_list_tests` 12843-12856 + 12857-12860 + 12869-12872 (`list.tap-activates`,
/// `.disabled-mute`): a tap on a row fires `activate` for THAT row, once;
/// a disabled List mutes the gesture. Godot's activate carries (key,
/// item) — Rust names the row node (the key argument is GP27's).
#[test]
fn tap_on_a_row_activates_it() {
    let mut fx = Fx::new(PICKER);
    fx.fill("tasks", &["a", "b", "c"]);
    let items = fx.items("tasks");
    let ev = fx.tap(items[1], 0.0);
    let activations: Vec<&Event> = ev.iter().filter(|e| e.kind == EventKind::Activate).collect();
    assert_eq!(activations.len(), 1, "list.tap-activates: {ev:?}");
    assert_eq!(activations[0].node, items[1], "the tapped row, not another: {ev:?}");
    let picker = fx.page.find("picker").unwrap();
    fx.rt.set_state(picker, DISABLED, true);
    let ev = fx.tap(items[0], 1.0);
    assert!(ev.is_empty(), "list.disabled-mute: {ev:?}");
}

/// `_list_tests` 12852-12861 (`list.tap-selects`, `.tap-again-no-deselect`):
/// in `select=one` the tap SELECTS the row (its `selected` state goes on)
/// and a second tap re-activates without deselecting (radio doctrine).
/// Rust's ListItem tap activates only — the row's state never changes.
#[test]
#[ignore = "GP28: not ported: tap selects in `select=one` (observed: the row's SELECTED bit stays off after a tap)"]
fn tap_selects_the_row_in_one_mode() {
    let mut fx = Fx::new(PICKER);
    fx.fill("tasks", &["a", "b", "c"]);
    let items = fx.items("tasks");
    fx.tap(items[1], 0.0);
    assert!(fx.rt.has(items[1], SELECTED), "list.tap-selects");
    let ev = fx.tap(items[1], 1.0);
    assert!(fx.rt.has(items[1], SELECTED) && ev.iter().filter(|e| e.kind == EventKind::Activate).count() == 1, "list.tap-again-no-deselect: {ev:?}");
}

/// `_list_tests` 12856 + 12863-12868 + 13389-13391 (`list.tap-focuses`,
/// `.arrow-moves-cursor`, `.key-activates`, `.arrow-up`,
/// `.layout-row-cursor`): a tap parks the cursor on the row; arrows move
/// it (down/up in a column, right/left in a `layout Row`); Enter
/// activates. gogo has no key events yet.
#[test]
#[ignore = "GP26: not ported: keyboard focus and key events (arrow cursor, Enter activates)"]
fn keyboard_moves_the_cursor_and_enter_activates() {
    panic!("Godot: tap b → focus on b; KEY_DOWN → c; KEY_ENTER → selected 3 + activate; KEY_UP → b; KEY_RIGHT in a layout Row → next chip — Rust: no key input surface");
}

/// `_list_tests` 12889-12945 + 12996-13003 (`list.default-checked-look`,
/// `.default-focus-pointer-hidden`, `.default-focus-look`,
/// `.default-checked-beats-focus`, `.default-rest-is-slate`,
/// `.default-pressed-look`, `.pressed-slide-off-clears`,
/// `.pressed-slide-back-rearms`, `.default-pressed-clears`,
/// `.default-focus-on-styled-row`): a BARE row shows selection, the press
/// and the keyboard cursor out of the box (D122) — the theme's selected
/// fill, a half-strength fill while held, a quarter-strength echo that
/// draws only after a keyboard move; untouched rows stay transparent.
#[test]
#[ignore = "GP28: not ported: ListItem default looks (selected / held / focus fills)"]
fn bare_row_default_looks() {
    let mut fx = Fx::new(PICKER);
    fx.fill("tasks", &["a", "b", "c"]);
    let items = fx.items("tasks");
    // list.default-rest-is-slate: an untouched bare row draws no plate.
    let res = resolve(&fx.page, &fx.rt);
    let rects = fx.rects();
    let row = rects[items[0] as usize];
    let plates: Vec<Draw> = draw(&fx.page, &res, &rects).into_iter().filter(|d| matches!(d, Draw::Quad { rect, .. } if *rect == row)).collect();
    assert!(plates.is_empty(), "{plates:?}");
    // list.default-pressed-look: a held bare row draws a plate.
    let (x, y) = (row.x + row.w / 2.0, row.y + row.h / 2.0);
    fx.touch(Phase::Down, x, y, 0.0);
    let res = resolve(&fx.page, &fx.rt);
    let plates: Vec<Draw> = draw(&fx.page, &res, &rects).iter().filter(|d| matches!(d, Draw::Quad { rect, .. } if *rect == row)).cloned().collect();
    assert_eq!(plates.len(), 1, "Godot: a half-strength fill while held; Rust draws: {plates:?}");
    panic!("and the selected fill / keyboard-only focus echo (D122) have no Rust counterpart yet");
}

const STYLED: &str = r#"style SelRow {
    when selected opacity=0.4, background=accent
    when held opacity=0.7
}
View v {
    List styledPicker width=200, height=120, halign=left, valign=top {
        empty { }
        repeat rows max=2 {
            ListItem .SelRow { Label text="" }
        }
    }
}"#;

/// `_list_tests` 12947-12995 (`list.styled-ok`, `.checked-restyles`,
/// `.checked-bg-renders`, `.checked-unstyles`, `.pressed-arms`,
/// `.pressed-disarms`, `.pressed-release-away-disarms`): `when selected`
/// on the row root restyles it while its `selected` state is on (opacity
/// 0.4, the accent plate drawn) and unstyles when the state moves; `when
/// held` arms on press (0.7, outranking selected), disarms on release —
/// including a release OFF the row, which also leaves the selection
/// alone. Godot flips the row's state through the List's `selected`;
/// Rust writes the row's SELECTED bit (the same universal state).
#[test]
fn styled_row_restyles_on_selected_and_held() {
    let mut fx = Fx::new(STYLED);
    fx.fill("rows", &["x", "y"]);
    let items = fx.items("rows");
    let (x, y) = (items[0], items[1]);
    fx.rt.set_state(y, SELECTED, true);
    assert!((fx.opacity(y) - 0.4).abs() < 1e-3 && (fx.opacity(x) - 1.0).abs() < 1e-3, "list.checked-restyles: y={} x={}", fx.opacity(y), fx.opacity(x));
    let res = resolve(&fx.page, &fx.rt);
    let rects = fx.rects();
    let accent = palette_color("accent").expect("accent").0;
    let plate = draw(&fx.page, &res, &rects).iter().find_map(|d| match d {
        Draw::Quad { rect, color, .. } if *rect == rects[y as usize] => Some(*color),
        _ => None,
    });
    assert!(matches!(plate, Some(c) if c.0[..3] == accent[..3] && c.0[3] > 0), "list.checked-bg-renders: {plate:?} vs accent {accent:?}");
    fx.rt.set_state(y, SELECTED, false);
    fx.rt.set_state(x, SELECTED, true);
    assert!((fx.opacity(y) - 1.0).abs() < 1e-3, "list.checked-unstyles: {}", fx.opacity(y));
    // held outranks selected while the press holds x; release unwinds.
    let r = rects[x as usize];
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    fx.touch(Phase::Down, cx, cy, 0.0);
    assert!((fx.opacity(x) - 0.7).abs() < 1e-3, "list.pressed-arms: {}", fx.opacity(x));
    fx.touch(Phase::Up, cx, cy, 0.1);
    assert!((fx.opacity(x) - 0.4).abs() < 1e-3, "list.pressed-disarms: {}", fx.opacity(x));
    fx.touch(Phase::Down, cx, cy, 1.0);
    fx.touch(Phase::Move, 390.0, 790.0, 1.05);
    let ev = fx.touch(Phase::Up, 390.0, 790.0, 1.1);
    assert!((fx.opacity(x) - 0.4).abs() < 1e-3 && fx.rt.has(x, SELECTED) && !fx.rt.has(x, HELD), "list.pressed-release-away-disarms: opacity={} ev={ev:?}", fx.opacity(x));
    assert!(!ev.iter().any(|e| e.kind == EventKind::Activate), "no tap from a release off the row: {ev:?}");
}

/// `_list_tests` 13005-13030 (`list.carry-selected`, `.carry-silent`,
/// `.carry-mode-resets`, `.carry-by-resets`): hot-reload carry — the
/// selection crosses a rebuild by key without a `change`; a changed
/// `select=` mode or `by` path resets to the fresh default.
#[test]
#[ignore = "GP42: not ported: hot-reload baseline carry (selection across a rebuild)"]
fn selection_carries_across_a_rebuild() {
    panic!("Godot: capture_state/restore_state carries selected=3 silently; select=many → [], a new `by` path → null — Rust: no state carry, no List selection");
}

/// `_list_tests` 13032-13048 + 13089-13098 (`list.many-ok`, `.off-ok`):
/// the three `select=` modes build.
#[test]
fn list_select_modes_build() {
    for mode in ["one", "many", "off"] {
        let src = format!("View v {{ List l select={mode} {{ empty {{ }} repeat t max=2 {{ ListItem {{ Label text=\"\" }} }} }} }}");
        assert!(builds(&src), "select={mode}: {:?}", codes(&src));
    }
}

/// `_list_tests` 13049-13086 (`list.many-initial`,
/// `.many-collection-order`, `.many-tap-toggles`, `.many-inert-order`,
/// `.many-dup-refused`, `.many-shape-refused`, `.many-keytype-refused`):
/// `select=many` — taps toggle, read-back is present keys in collection
/// order then inert keys, malformed writes are refused whole (G9104).
#[test]
#[ignore = "GP28: not ported: `select=many` (toggle taps, ordered read-back, G9104 refusals)"]
fn select_many_toggles_and_reads_back_in_order() {
    let mut fx = Fx::new("View v { List tags select=many, width=200, height=120, halign=left, valign=top { empty { } repeat opts max=3 { ListItem { Label text=\"\" } } } }");
    fx.fill("opts", &["red", "green", "blue"]);
    let items = fx.items("opts");
    fx.tap(items[2], 0.0);
    fx.tap(items[0], 1.0);
    assert!(fx.rt.has(items[0], SELECTED) && fx.rt.has(items[2], SELECTED), "list.many-collection-order: red and blue selected");
    fx.tap(items[2], 2.0);
    assert!(!fx.rt.has(items[2], SELECTED), "list.many-tap-toggles");
    panic!("and the List-level read-back/refusals ([red, blue], inert keys, G9104) have no Rust surface");
}

/// `_list_tests` 13099-13122 (`list.off-activates`, `.off-never-checks`):
/// `select=off` — a tap activates the row and never checks anything.
#[test]
fn select_off_activates_without_selecting() {
    let mut fx = Fx::new("View v { List picker select=off, width=200, height=100, halign=left, valign=top { empty { } repeat entries max=2 { ListItem { Label text=\"\" } } } }");
    fx.fill("entries", &["open", "save"]);
    let items = fx.items("entries");
    let ev = fx.tap(items[1], 0.0);
    let activations: Vec<&Event> = ev.iter().filter(|e| e.kind == EventKind::Activate).collect();
    assert!(activations.len() == 1 && activations[0].node == items[1], "list.off-activates: {ev:?}");
    assert!(!fx.rt.has(items[1], SELECTED), "list.off-never-checks");
}

/// `_list_tests` 13123-13135 (`list.off-write-refused`,
/// `.off-no-checked-default`): in `select=off` a code write to `selected`
/// is refused silently and the rows carry the focus and held defaults but
/// never the selected one.
#[test]
#[ignore = "GP28: not ported: `select=off` write refusal and per-mode default looks"]
fn select_off_refuses_writes_and_drops_the_checked_default() {
    panic!("Godot: menu.selected = 'save' stays null with no change; rows have focused/held conditions, no selected — Rust: no List selection surface");
}

/// `_list_tests` 13137-13170 (`list.hug-ok`, `.hug-sizes-to-content`,
/// `.hug-grows-with-data`, `.hug-no-stretch`): `height=hug` makes the box
/// its content — it grows with the rows and never stretches to the
/// parent's leftover room.
#[test]
fn hug_list_is_its_content() {
    let mut fx = Fx::new("View v { Column hmenu width=200, height=400, halign=left, valign=top { Button b1 text=\"New Game\"; List hugger height=hug, spacing=2 { empty { } repeat tasks max=4 { ListItem { Label text=\"\" } } } } }");
    fx.fill("tasks", &["one", "two"]);
    let hugger = fx.page.find("hugger").unwrap() as usize;
    let h2 = fx.rects()[hugger].h;
    assert!(h2 > 20.0, "list.hug-sizes-to-content: {h2}");
    fx.fill("tasks", &["one", "two", "three", "four"]);
    let r = fx.rects();
    assert!(r[hugger].h > h2 + 10.0, "list.hug-grows-with-data: {h2} -> {}", r[hugger].h);
    let items = fx.items("tasks");
    // The box ends with its last row — it does not stretch to the column's
    // 400 (Godot: size_flags_vertical != EXPAND_FILL).
    assert_eq!(r[hugger].bottom(), r[items[3] as usize].bottom(), "list.hug-no-stretch: {r:?}");
    assert!(r[hugger].bottom() < 200.0, "list.hug-no-stretch: not the column's leftover: {r:?}");
}

/// `_list_tests` 13165 + 13194-13201 + 13210 (`list.hug-not-starved`,
/// `.starved-warns`, `.placed-stays-quiet`, `.half-starved-warns`): the
/// no-height nudge — a viewport List a hug parent starves reports
/// `starved` at runtime; `height=`/`hug` keeps it quiet; a headed list
/// whose rows get less than one row's room is half-starved and nudges.
#[test]
#[ignore = "GP28: not ported: viewport doctrine + the runtime `starved` nudge (Rust's List hugs, nothing starves)"]
fn viewport_list_starved_by_a_hug_parent_warns() {
    panic!("Godot: `List starving` under a hug Column reports starved=true, `List sized height=100` false, the headed list true after data arrives — Rust: no starved flag");
}

/// `_list_tests` 13172-13193 + 13207 (`list.maxheight-ok`,
/// `.maxheight-under-cap-hugs`, `.maxheight-caps`,
/// `.err-maxheight-bad-value`): `height=hug, max_height=64` grows with the
/// rows until the cap and stops there; a non-number `max_height` is a
/// build error naming the key (Rust G0058).
#[test]
fn max_height_caps_a_hug_list() {
    let mut fx = Fx::new("View v { Column mx halign=left, valign=top { List capped height=hug, max_height=64, spacing=0 { empty { } repeat tasks max=12 { ListItem { Label text=\"\" } } } } }");
    fx.fill("tasks", &["one"]);
    let capped = fx.page.find("capped").unwrap() as usize;
    let h = fx.rects()[capped].h;
    assert!(h > 5.0 && h < 64.0, "list.maxheight-under-cap-hugs: {h}");
    let many_owned: Vec<String> = (0..12).map(|i| format!("t{i}")).collect();
    let many: Vec<&str> = many_owned.iter().map(String::as_str).collect();
    fx.fill("tasks", &many);
    assert_eq!(fx.rects()[capped].h, 64.0, "list.maxheight-caps");
    let bad = "View v { List l height=hug, max_height=nope { empty { } repeat t max=1 { ListItem { Label text=\"\" } } } }";
    let c = compile(bad, &|_| None);
    assert!(c.diags.iter().any(|d| d.code == "G0058" && d.message.contains("max_height")), "list.err-maxheight-bad-value: {:#?}", c.diags);
}

/// `_list_tests` 13190-13193 + 13205 (`list.maxheight-scrolls-past-cap`,
/// `.err-maxheight-needs-hug`): past the cap the List's internal scroll
/// takes over, and `max_height` without `height=hug` is a build error
/// ("height=hug"). Rust caps any box and never pans a List.
#[test]
#[ignore = "GP28: not ported: List scroll past the hug cap; `max_height` needs `height=hug` (Rust: a general cap)"]
fn max_height_scrolls_past_the_cap_and_needs_hug() {
    let src = "View v { List l max_height=100 { empty { } repeat t max=1 { ListItem { Label text=\"\" } } } }";
    assert!(!builds(src), "list.err-maxheight-needs-hug: Rust built it: {:?}", codes(src));
}

/// `_list_tests` 13194-13203 + 13212-13241 (`list.starve-ok`,
/// `.fixed-content-ok`, `.fixed-content-shows`): fixed content sizes the
/// box (D127) — a bare List in a hug Column SHOWS its header and "none
/// yet" text (they count into the minimum).
#[test]
fn fixed_content_sizes_the_box() {
    assert!(builds("View v { Column smenu { Button sb text=\"x\"; List starving { empty { } repeat t max=1 { ListItem { Label text=\"\" } } }; List sized height=100 { empty { } repeat u max=1 { ListItem { Label text=\"\" } } } } }"), "list.starve-ok");
    let fx = Fx::new("View v { Column fmenu halign=left, valign=top { Button fb text=\"x\"; List headed { Label text=\"Tasks:\"; empty { Label none text=\"none yet\" }; repeat t max=3 { ListItem padding=12 { Label text=\"\" } } } } }");
    let r = fx.rects();
    let headed = fx.page.find("headed").unwrap() as usize;
    let none = fx.page.find("none").unwrap() as usize;
    assert!(r[headed].h > 20.0 && r[none].h > 0.0, "list.fixed-content-shows: headed={:?} none={:?}", r[headed], r[none]);
    assert!(texts(&fx.page, &fx.rt).contains(&"none yet".to_string()), "{:?}", texts(&fx.page, &fx.rt));
}

/// `_list_tests` 13250-13253 + 13258-13261 + 13270-13281
/// (`list.lint-inner-checked-toggle-quiet`, `.lint-root-checked-quiet`,
/// `.lint-starved-hug-quiet`, `.lint-starved-bounded-quiet`,
/// `.lint-starved-unbounded-quiet`): the shapes Godot's D127 lints leave
/// alone build with no warning here too — a Toggle's own `when selected`
/// inside a row, a `when selected` style on the row ROOT, a hug List
/// under a centered Column, a bounded chain, an unbounded Column.
#[test]
fn list_lints_stay_quiet_where_godot_is_quiet() {
    let cases = [
        ("list.lint-inner-checked-toggle-quiet", "View v { List l { empty { } repeat t max=1 { ListItem { Toggle text=\"x\" { when selected opacity=0.5 } } } } }"),
        ("list.lint-root-checked-quiet", "style Sel3 { when selected opacity=0.5 }\nView v { List l { empty { } repeat t max=1 { ListItem .Sel3 { Label text=\"x\" } } } }"),
        ("list.lint-starved-hug-quiet", "View v { Column m valign=center { List l height=hug { empty { } repeat t max=1 { ListItem { Label text=\"x\" } } } } }"),
        ("list.lint-starved-bounded-quiet", "View v { Column m valign=center { Column inner height=100 { List l { empty { } repeat t max=1 { ListItem { Label text=\"x\" } } } } } }"),
        ("list.lint-starved-unbounded-quiet", "View v { Column m { List l { empty { } repeat t max=1 { ListItem { Label text=\"x\" } } } } }"),
    ];
    for (name, src) in cases {
        let w = warnings(src);
        assert!(w.is_empty(), "{name}: {w:?}");
    }
}

/// `_list_tests` 13246-13249 + 13254-13257 + 13262-13269
/// (`list.lint-inner-checked`, `.lint-inner-checked-style`,
/// `.lint-starved-chain`, `.lint-starved-scroll`): advisory lints —
/// G9014 for a `when selected` INSIDE the row (the List checks the root,
/// it never fires), G9015 for provable starvation (a valign-anchored
/// Column chain, a Scroll page).
#[test]
#[ignore = "GP28: not ported: List lints G9014 (inner `when selected`) and G9015 (provable starvation)"]
fn list_lints_inner_selected_and_provable_starvation() {
    let cases = [
        ("list.lint-inner-checked", "G9014", "View v { List l { empty { } repeat t max=1 { ListItem { Label text=\"x\" { when selected opacity=0.5 } } } } }"),
        ("list.lint-inner-checked-style", "G9014", "style Sel2 { when selected opacity=0.5 }\nView v { List l { empty { } repeat t max=1 { ListItem { Label .Sel2 text=\"x\" } } } }"),
        ("list.lint-starved-chain", "G9015", "View v { Column m valign=center { List l { empty { } repeat t max=1 { ListItem { Label text=\"x\" } } } } }"),
        ("list.lint-starved-scroll", "G9015", "View v { Scroll { Column { List l { empty { } repeat t max=1 { ListItem { Label text=\"x\" } } } } } }"),
    ];
    for (name, code, src) in cases {
        let w = warnings(src);
        assert!(w.iter().any(|x| x.starts_with(code)), "{name}: want {code}, got {w:?}");
    }
}

/// `_list_tests` 13283-13330 (`list.layout-row-ok`, `.layout-row-panel`,
/// `.layout-row-parents`, `.layout-row-hug-caps`, `.layout-flow-ok`,
/// `.layout-flow-panel`, `.layout-flow-parents`, `.layout-tiles-ok`,
/// `.layout-tiles-panel`): `layout <Kind>` picks the items' panel — the
/// rows parent under a Row / Flow / Tiles node inside the List, a Row
/// strip lays them side by side, and `width=hug, max_width=120` caps the
/// strip at 120.
#[test]
fn layout_kind_picks_the_items_panel() {
    let mut fx = Fx::new("View v { List strip select=one, width=hug, max_width=120, halign=left, valign=top { empty { Label text=\"empty\" } layout Row spacing=2 { repeat chips max=8 { ListItem { Label text=\"\" } } } } }");
    let chips_owned: Vec<String> = ["aa", "bb", "cc", "dd", "ee", "ff", "gg", "hh"].iter().map(|s| format!("chip {s}")).collect();
    let chips: Vec<&str> = chips_owned.iter().map(String::as_str).collect();
    fx.fill("chips", &chips);
    let repeat = fx.page.find("chips").unwrap();
    let panel = fx.page.nodes[repeat as usize].parent;
    let strip = fx.page.find("strip").unwrap();
    assert_eq!(fx.page.nodes[panel as usize].kind, Kind::Row, "list.layout-row-panel");
    assert_eq!(fx.page.nodes[panel as usize].parent, strip, "the panel sits inside the List");
    let items = fx.items("chips");
    let r = fx.rects();
    assert!(r[items[1] as usize].x > r[items[0] as usize].x && r[items[1] as usize].y == r[items[0] as usize].y, "list.layout-row-parents: side by side: {:?}", items.iter().map(|i| r[*i as usize]).collect::<Vec<_>>());
    assert_eq!(r[strip as usize].w, 120.0, "list.layout-row-hug-caps: {:?}", r[strip as usize]);

    let page = ok("View v { List g { empty { } layout Flow spacing=4 { repeat t max=2 { ListItem { Label text=\"x\" } } } } }");
    let t = page.find("t").unwrap();
    assert_eq!(page.nodes[page.nodes[t as usize].parent as usize].kind, Kind::Flow, "list.layout-flow-panel");
    assert_eq!(page.nodes[page.nodes[t as usize].parent as usize].parent, page.find("g").unwrap(), "list.layout-flow-parents");
    let page = ok("View v { List m2 { empty { } layout Tiles cols=2 { repeat t max=2 { ListItem { Label text=\"x\" } } } } }");
    let t = page.find("t").unwrap();
    assert_eq!(page.nodes[page.nodes[t as usize].parent as usize].kind, Kind::Tiles, "list.layout-tiles-panel");
}

/// `_list_tests` 13309-13312 (`list.layout-row-scrolls`): a Row strip
/// past its `max_width` pans sideways (the List's internal scroll).
#[test]
#[ignore = "GP28: not ported: List self-scroll (a Row strip past its cap pans sideways)"]
fn layout_row_strip_scrolls_sideways() {
    panic!("Godot: 8 chips in a max_width=120 strip give the horizontal bar range — Rust: the rows overflow the clipped box, nothing pans");
}

/// `_list_tests` 13332-13347 (`list.err-empty-missing`, `.err-empty-inner`,
/// `.err-empty-dup`, `.err-layout-unknown`, `.err-layout-statics`,
/// `.err-layout-empty-block`, `.err-layout-stray`, `.err-row-max-height`):
/// the List's `empty` seat (G9016 — required at the List level, not
/// inside the repeat, single) and `layout` block rules (G9017 — a closed
/// set, rows only, never empty, only inside a List; a Row strip caps with
/// `max_width`, not `max_height`). Rust builds all of them (`layout Grid`
/// is even a known kind).
#[test]
#[ignore = "GP28: not ported: List `empty` seat (G9016) and `layout` block (G9017) rules"]
fn list_empty_seat_and_layout_block_rules() {
    let cases = [
        ("list.err-empty-missing", "View v { List l { repeat t max=1 { ListItem { Label text=\"\" } } } }"),
        ("list.err-empty-inner", "View v { List l { empty { } repeat t max=1 { empty { } ListItem { Label text=\"\" } } } }"),
        ("list.err-empty-dup", "View v { List l { empty { }; empty { } repeat t max=1 { ListItem { Label text=\"\" } } } }"),
        ("list.err-layout-unknown", "View v { List l { empty { } layout Grid { repeat t max=1 { ListItem { Label text=\"\" } } } } }"),
        ("list.err-layout-statics", "View v { List l { empty { } layout Row { Label text=\"x\"; repeat t max=1 { ListItem { Label text=\"\" } } } } }"),
        ("list.err-layout-empty-block", "View v { List l { empty { } layout Row { } repeat t max=1 { ListItem { Label text=\"\" } } } }"),
        ("list.err-layout-stray", "View v { Column { layout Row { } } }"),
        ("list.err-row-max-height", "View v { List l width=hug, max_height=100 { empty { } layout Row { repeat t max=1 { ListItem { Label text=\"\" } } } } }"),
    ];
    for (name, src) in cases {
        assert!(!builds(src), "{name}: Godot refuses, Rust built it: {:?}", codes(src));
    }
}

/// `_list_tests` 13363-13375 (`list.err-no-repeat`, `.err-two-repeats`,
/// `.err-multi-root`, `.err-root-not-listitem`): a List holds exactly one
/// repeat (G9010) whose template has one root and that root is a
/// `ListItem` (G9011, the fix-it names the wrap).
#[test]
#[ignore = "GP28: not ported: List structure rules G9010/G9011 (one repeat, one ListItem root)"]
fn list_holds_one_repeat_with_one_listitem_root() {
    let cases = [
        ("list.err-no-repeat", "View v { List l { Label text=\"x\" } }"),
        ("list.err-two-repeats", "View v { List l { empty { } repeat a max=1 { ListItem { Label text=\"\" } }; repeat b max=1 { ListItem { Label text=\"\" } } } }"),
        ("list.err-multi-root", "View v { List l { empty { } repeat t max=1 { ListItem { Label text=\"\" }; ListItem { Label text=\"\" } } } }"),
        ("list.err-root-not-listitem", "View v { List l { empty { } repeat t max=1 { Label text=\"\" } } }"),
    ];
    for (name, src) in cases {
        assert!(!builds(src), "{name}: Godot refuses, Rust built it: {:?}", codes(src));
    }
}

/// `_list_tests` 13377-13388 + 13392 + 13475 (`list.err-listitem-static`,
/// `.err-listitem-plain-repeat`, `.err-listitem-nested`,
/// `.err-listitem-in-empty`, `.err-bad-select`, `.err-slot`): `ListItem`
/// belongs only at a List repeat's root (G9013), `select=` is
/// one|many|off (G9012), a List has no slots. Rust accepts all of them
/// (`select=` is an unread host word here).
#[test]
#[ignore = "GP28: not ported: ListItem placement (G9013), `select=` values (G9012), no-slot rule"]
fn listitem_belongs_at_a_list_row_root() {
    let cases = [
        ("list.err-listitem-static", "View v { Column { ListItem { Label text=\"x\" } } }"),
        ("list.err-listitem-plain-repeat", "View v { Column { repeat t max=1 { empty { } ListItem { Label text=\"\" } } } }"),
        ("list.err-listitem-nested", "View v { List l { empty { } repeat t max=1 { ListItem { ListItem { Label text=\"\" } } } } }"),
        ("list.err-listitem-in-empty", "View v { List l { empty { ListItem { Label text=\"e\" } } repeat t max=1 { ListItem { Label text=\"\" } } } }"),
        ("list.err-bad-select", "View v { List l select=both { empty { } repeat t max=1 { ListItem { Label text=\"\" } } } }"),
        ("list.err-slot", "View v { List l { slot s { }; empty { } repeat t max=1 { ListItem { Label text=\"\" } } } }"),
    ];
    for (name, src) in cases {
        assert!(!builds(src), "{name}: Godot refuses, Rust built it: {:?}", codes(src));
    }
}

/// `_list_tests` 13352-13361 (`list.checked-universal`,
/// `.pressed-on-listitem-ok`): `selected` styling is legal on any node
/// (dormant on a Label), and `when held` is legal on a ListItem row (a
/// press surface).
#[test]
fn selected_is_universal_and_held_is_legal_on_a_row() {
    let label = "style Sel { when selected opacity=0.5 }\nView v { Label q .Sel text=\"q\" }";
    assert!(builds(label), "list.checked-universal: {:?}", codes(label));
    let row = "View v { List l { empty { } repeat t max=1 { ListItem { when held opacity=0.5\n    Label text=\"x\" } } } }";
    assert!(builds(row), "list.pressed-on-listitem-ok: {:?}", codes(row));
}

/// `_list_tests` 13356-13358 (`list.pressed-still-gated`): `when held`
/// stays gated to press surfaces — a style with it on a Label is refused
/// ("'held' isn't available"). Rust builds it (the row's inline form is
/// caught by another check; the style route is not).
#[test]
#[ignore = "GP41: Rust accepts a `when held` style on a Label (Godot refuses: 'held' isn't available)"]
fn held_is_gated_to_press_surfaces() {
    let src = "style P { when held opacity=0.5 }\nView v { Label q .P text=\"q\" }";
    assert!(!builds(src), "{:?}", codes(src));
}

/// `_list_tests` 13477-13480 (`list.err-render-native`): Godot refuses
/// `render=native` on a List ("no native backend"). FLIP: FanRust has ONE
/// renderer (docs/DECISIONS.md D1) and `render=` is accepted for old
/// files and read by nobody (`page::UNREAD_BY_DESIGN`, `PropKey::Render`).
#[test]
fn render_word_is_accepted_one_renderer() {
    let src = "View v { List l render=native { empty { } repeat t max=1 { ListItem { Label text=\"\" } } } }";
    assert!(builds(src), "{:?}", codes(src));
}

/// `_list_tests` 13481-13495 (`list.render-gogo-ok`, `.listitem-box-props`,
/// `.height-wins`): `render=gogo` builds; a ListItem is a drawn box like
/// Panel (`background=`/`padding=` are its own); an authored `height=90`
/// List is 90 high, no silent stretch.
#[test]
fn listitem_is_a_drawn_box_and_an_authored_height_wins() {
    assert!(builds("View v { List l render=gogo { empty { } repeat t max=1 { ListItem { Label text=\"\" } } } }"), "list.render-gogo-ok");
    let src = "View v { List l { empty { } repeat t max=1 { ListItem background=accent, padding=8 { Label text=\"\" } } } }";
    assert!(builds(src), "list.listitem-box-props: {:?}", codes(src));
    let fx = Fx::new("View v { Column c height=400, halign=left, valign=top { List sized height=90 { empty { } repeat t max=1 { ListItem { Label text=\"\" } } } } }");
    let r = fx.rects();
    assert_eq!(r[fx.page.find("sized").unwrap() as usize].h, 90.0, "list.height-wins: {r:?}");
}
