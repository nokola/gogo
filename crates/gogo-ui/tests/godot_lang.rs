//! Port of the Godot gogo suite's LANGUAGE batch
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`, slug `lang`):
//!
//! - `_parser_tests` 225-255 (17 checks)
//! - `_resolver_basics` 255-373 (23 checks)
//! - `_pin_matrix_tests` 2045-2121 (16 checks)
//! - `_example_tests` 2121-2136 (3 checks)
//! - `_cookbook_tests` 2136-2439 (44 checks)
//! - `_extends_tests` 2984-3040 (11 checks)
//! - `_epx_tests` 3040-3061 (5 checks)
//! - `_vocab_tests` 12323-12456 (47 checks)
//!
//! Godot's anchors/offsets/grow directions become laid-out rects here
//! (viewport 400x400 unless a test says otherwise); Godot stylebox content
//! margins become the child's inset; `size_flags` become the rect. Godot
//! snippets with a non-View/Panel root are wrapped in `View v { … }` (the
//! Rust root rule, gogo-design §7 "Root"); other dialect translations are
//! named at the test that makes them.
//!
//! ENGINE (not ported, no Rust counterpart):
//! - `parse.node-path` (231): AST carries no source path.
//! - `cookbook.border-padding-float`'s `border_width_left == 1` clause
//!   (2318): Godot stylebox paints whole-px borders; Rust keeps the float.
use gogo::ast::{AstItem, AstNode, AstValue, Document};
use gogo::draw::{Draw, Fit, draw, hit};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Kind, NO_STR, PropKey, Rgba, State, Value};
use gogo::parser::parse;
use gogo::runtime::{Resolved, Runtime, SELECTED, resolve};
use gogo::{Compiled, Page, compile};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 400.0 };

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn has(src: &str, code: &str) -> bool {
    codes(src).iter().any(|c| c == code)
}

/// The message of the first diagnostic carrying `code`.
fn message(src: &str, code: &str) -> String {
    let c = compile(src, &|_| None);
    c.diags.iter().find(|d| d.code == code).map(|d| d.message.clone()).unwrap_or_else(|| panic!("no {code} in {:#?}", c.diags))
}

struct Laid {
    page: Page,
    res: Resolved,
    rects: Vec<Rect>,
}

impl Laid {
    fn rect(&self, name: &str) -> Rect {
        self.rects[self.id(name) as usize]
    }
    fn id(&self, name: &str) -> u32 {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }
    fn draws(&self) -> Vec<Draw> {
        draw(&self.page, &self.res, &self.rects)
    }
    fn hit(&self, x: f32, y: f32) -> Option<u32> {
        hit(&self.page, &self.res, &self.rects, x, y, false)
    }
    fn center(&self, name: &str) -> (f32, f32) {
        let r = self.rect(name);
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }
}

fn lay(src: &str) -> Laid {
    lay_in(src, VIEW)
}

fn lay_in(src: &str, viewport: Rect) -> Laid {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, viewport, &FixedAdvance);
    Laid { page: c.page, res, rects }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn parse_ok(src: &str) -> Document {
    let mut diags = Vec::new();
    let doc = parse(src, &mut diags);
    assert!(diags.is_empty(), "{diags:#?}");
    doc
}

fn parse_codes(src: &str) -> Vec<String> {
    let mut diags = Vec::new();
    parse(src, &mut diags);
    diags.iter().map(|d| d.code.clone()).collect()
}

fn node(item: &AstItem) -> &AstNode {
    match item {
        AstItem::Node(n) => n,
        other => panic!("not a node: {other:?}"),
    }
}

fn quads(list: &[Draw]) -> Vec<(Rect, Rgba, f32)> {
    list.iter().filter_map(|d| if let Draw::Quad { rect, color, corner, .. } = d { Some((*rect, *color, *corner)) } else { None }).collect()
}

fn text_alpha(page: &Page, list: &[Draw], word: &str) -> u8 {
    list.iter()
        .find_map(|d| match d {
            Draw::Text { text, color, .. } if page.str(*text) == word => Some(color.0[3]),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {word:?} in {list:#?}"))
}

// ------------------------------------------------------------ _parser_tests

/// `_parser_tests` 226-233 (parse.ok, type+name, style, string-escape,
/// bool, size-arg, child): one node line carries every member kind and each
/// lands in its own AST slot. `parse.node-path` (233) is ENGINE — the Rust
/// AST records no file path.
#[test]
fn parser_node_anatomy() {
    let d = parse_ok("Button save .Primary text=\"Go\\n\", toggle=true, 44x36 { Rectangle margin=8 }");
    let n = node(&d.items[0]);
    assert_eq!((n.type_word.as_str(), n.name.as_deref()), ("Button", Some("save")), "{n:#?}");
    assert_eq!(n.styles, vec!["Primary".to_string()], "{n:#?}");
    assert_eq!(n.props[0].value, AstValue::Str { text: "Go\n".into(), key: None, line: 1 }, "{n:#?}");
    assert_eq!(n.props[1].value, AstValue::Bool(true), "{n:#?}");
    assert_eq!(n.args[0], AstValue::Size { w: Some(44.0), h: Some(36.0) }, "{n:#?}");
    assert_eq!(n.children.len(), 1, "{n:#?}");
    assert_eq!(node(&n.children[0]).type_word, "Rectangle", "{n:#?}");
}

/// `_parser_tests` 235-241 (parse.color, theme-set, theme-set-needs-equals,
/// theme-variant): a `$name = #RRGGBBAA` line is an assignment with the
/// bytes as written, `=` is required, and a `theme dark { }` block holds
/// its assignments as children.
#[test]
fn parser_theme_values() {
    let d = parse_ok("$a = #FF000080");
    assert!(matches!(&d.items[0], AstItem::Assign { key, dollar: true, value: AstValue::Color { rgba: [0xFF, 0, 0, 0x80], .. }, .. } if key == "a"), "{:#?}", d.items[0]);
    let d = parse_ok("$accent = #E51400FF");
    assert!(matches!(&d.items[0], AstItem::Assign { key, dollar: true, .. } if key == "accent"), "{:#?}", d.items[0]);
    assert!(parse_codes("$accent #E51400FF").contains(&"G0012".to_string()), "{:?}", parse_codes("$accent #E51400FF"));
    let d = parse_ok("theme dark {\n    $accent = #00FF00FF\n}");
    let t = node(&d.items[0]);
    assert_eq!((t.type_word.as_str(), t.name.as_deref()), ("theme", Some("dark")), "{t:#?}");
    assert_eq!(t.children.len(), 1, "{t:#?}");
    assert!(matches!(&t.children[0], AstItem::Assign { key, dollar: true, .. } if key == "accent"), "{t:#?}");
}

/// `_parser_tests` 242-249 (parse.err.unterminated, dup-prop, unit,
/// stray-brace, unclosed): each malformed line is refused with its own
/// code — an unclosed block names the missing `}`.
#[test]
fn parser_errors() {
    assert!(parse_codes("Label text=\"oops").contains(&"G0003".to_string()));
    assert!(parse_codes("Button text=\"a\", text=\"b\"").contains(&"G0018".to_string()));
    assert!(parse_codes("Button width=4pt").contains(&"G0079".to_string()));
    assert!(parse_codes("}").contains(&"G0011".to_string()));
    let mut diags = Vec::new();
    parse("Column {\n  Button ok\n", &mut diags);
    assert!(diags.iter().any(|d| d.code == "G0002" && d.message.contains('}')), "{diags:#?}");
}

// ---------------------------------------------------------- _resolver_basics

/// `_resolver_basics` 256-273 (build.ok, vbox, spacing, named, label,
/// slider-alias, enum-ident): a Column with a Label, a Slider and a
/// Rectangle named `fill` builds; every named node is findable and each
/// prop reads back as written. (Godot's bare `Column` root is wrapped in
/// `View v`.)
#[test]
fn resolver_basics_stack_build() {
    let l = lay("View v {\n Column panel spacing=8 {\n  Label title text=\"Hi\", vertical_alignment=center\n  Slider vol min=1, max=300, value=80\n  Rectangle fill\n }\n}\n");
    let panel = l.id("panel");
    assert_eq!(l.page.nodes[panel as usize].kind, Kind::Column);
    assert_eq!(l.res.num(&l.page, panel, PropKey::Spacing), Some(8.0));
    for name in ["panel", "title", "vol", "fill"] {
        assert!(l.page.find(name).is_some(), "named {name}");
    }
    let title = l.id("title");
    assert!(matches!(l.res.prop(&l.page, title, PropKey::Text), Some(Value::VText { text, .. }) if l.page.str(text) == "Hi"));
    let vol = l.id("vol");
    assert_eq!((l.res.num(&l.page, vol, PropKey::Min), l.res.num(&l.page, vol, PropKey::Max), l.res.slider_value(&l.page, vol)), (Some(1.0), Some(300.0), 80.0));
    assert_eq!(l.res.ident(&l.page, title, PropKey::TextValign), Some("center"));
}

/// `_resolver_basics` 277-285 (pins.ok, grow, anchor, margin): a centered,
/// bottom-pinned root keeps its bottom edge 14 above the frame's bottom and
/// sits mid-width. Godot pinned anchors and offsets on a size-less Panel;
/// here the box is given 40x30 so the same edges are visible as a rect.
#[test]
fn resolver_basics_edge_pins() {
    let l = lay("Panel p 40x30, halign=center, valign=bottom, margin_bottom=14");
    assert_eq!(l.rect("p"), Rect::new(180.0, 356.0, 40.0, 30.0), "{:?}", l.rect("p"));
}

/// `_resolver_basics` 291-300 (pins.overhang-ok, slides, anchors, grow):
/// the D27 app bar — `height=80` pinned at `margin_bottom=-30` hangs 30
/// below the frame: top 50 above the bottom edge, bottom 30 past it, full
/// width. (Godot's raw `Control` root is `View` here.)
#[test]
fn resolver_basics_overhung_root() {
    let l = lay("View bar width=fill, valign=bottom, height=80, margin_bottom=-30");
    assert_eq!(l.rect("bar"), Rect::new(0.0, 350.0, 400.0, 80.0), "{:?}", l.rect("bar"));
}

/// `_resolver_basics` 307-317 (pins.detached-minsize-ok, flush): a hugging
/// right-aligned Row of two 50-wide Buttons at `spacing=0` is exactly 100
/// wide and flush with the right edge. (Godot pinned this against a
/// theme-cache staleness bug; the observable is the same rect.)
#[test]
fn resolver_basics_hug_row_flush_right() {
    let l = lay("View v {\n Row row halign=right, height=fill, spacing=0 {\n  Button a 50x60\n  Button b 50x60\n }\n}\n");
    let row = l.rect("row");
    assert_eq!((row.x, row.right()), (300.0, 400.0), "{row:?}");
}

/// `_resolver_basics` 324-345 (image.ok, defaults, box-centered,
/// one-axis-fills, halign-beats-centering): WPF Image semantics (D28) — the
/// default fit is contain with no texture minimum, a both-axes-sized Image
/// is an exact centered box in a stack, a one-axis size keeps the cross-axis
/// fill, `halign` beats the centering.
/// `image.explicit-flags-win` (343) is FLIP: `size_flags_horizontal=` is
/// gone (gogo-design §7 raw register; G0041) — the placement words say it.
#[test]
fn resolver_basics_image_semantics() {
    let l = lay("View v {\n Column panel {\n  Image dot 16x16, source=\"res://dot.png\"\n  Image banner min_height=32\n  Image pinned min_size=8x8\n  Image lefty halign=left, size=8x8\n }\n}\n");
    let fit = l.draws().iter().find_map(|d| if let Draw::Image { fit, .. } = d { Some(*fit) } else { None });
    assert_eq!(fit, Some(Fit::Contain), "{:#?}", l.draws());
    let dot = l.rect("dot");
    assert_eq!(dot, Rect::new(192.0, 0.0, 16.0, 16.0), "a both-axes Image centers: {dot:?}");
    let banner = l.rect("banner");
    assert_eq!((banner.x, banner.w, banner.h), (0.0, 400.0, 32.0), "one axis sized keeps the fill: {banner:?}");
    let lefty = l.rect("lefty");
    assert_eq!((lefty.x, lefty.w), (0.0, 8.0), "halign beats centering: {lefty:?}");
    assert!(has("View v { Column c { Image pinned min_size=8x8, size_flags_horizontal=3 } }", "G0041"), "FLIP: raw size flags are gone");
}

// --------------------------------------------------------- _pin_matrix_tests

/// `_pin_matrix_tests` 2047-2057 (pinmx.begin-ok, begin, dragged-axis): a
/// begin pin puts the near edge at the margin and the far edge at size
/// distance; the axis nobody wrote is a flush begin pin.
#[test]
fn pin_matrix_begin_pin() {
    let l = lay("Panel b 40x30, halign=left, margin_left=30");
    assert_eq!(l.rect("b"), Rect::new(30.0, 0.0, 40.0, 30.0), "{:?}", l.rect("b"));
}

/// `_pin_matrix_tests` 2061-2068 (pinmx.park-ok, park-slides): a negative
/// end pin parks the box past the edge — bottom 90 below the frame, top
/// following at the declared height.
#[test]
fn pin_matrix_negative_end_pin_parks() {
    let l = lay("Panel b valign=bottom, height=50, margin_bottom=-90");
    let b = l.rect("b");
    assert_eq!((b.y, b.bottom()), (440.0, 490.0), "{b:?}");
}

/// `_pin_matrix_tests` 2071-2078 (pinmx.end-key-ok, end-key): a far-edge
/// margin with no alignment pins that edge — the box hangs off the right at
/// 10, its left at size distance.
#[test]
fn pin_matrix_far_key_is_an_end_pin() {
    let l = lay("Panel b 40x30, margin_right=10");
    let b = l.rect("b");
    assert_eq!((b.x, b.right()), (350.0, 390.0), "{b:?}");
}

/// `_pin_matrix_tests` 2081-2088 (pinmx.span-ok, span): two margin keys
/// and no alignment pin both edges — the box spans between them.
#[test]
fn pin_matrix_two_keys_span() {
    let l = lay("Panel b margin_left=20, margin_right=30");
    let b = l.rect("b");
    assert_eq!((b.x, b.right()), (20.0, 370.0), "{b:?}");
}

/// `_pin_matrix_tests` 2091-2097 (pinmx.fill-signed-ok, fill-signed):
/// `width=fill` takes signed margins on both edges — wider than the frame.
#[test]
fn pin_matrix_fill_takes_signed_margins() {
    let l = lay("Panel b width=fill, margin_left=-20, margin_right=-20");
    let b = l.rect("b");
    assert_eq!((b.x, b.right()), (-20.0, 420.0), "{b:?}");
}

/// `_pin_matrix_tests` 2101-2108 (pinmx.shorthand-ok, shorthand-corner):
/// `margin=16` is breathing room — it binds only the edges the alignment
/// touches, so a corner button sits 16 in from the right and bottom.
#[test]
fn pin_matrix_margin_shorthand_binds_touched_edges() {
    let l = lay("Panel b 40x30, halign=right, valign=bottom, margin=16");
    assert_eq!(l.rect("b"), Rect::new(344.0, 354.0, 40.0, 30.0), "{:?}", l.rect("b"));
}

/// `_pin_matrix_tests` 2112-2113 (pinmx.err-center-margin): Godot refuses
/// a `margin_left` under `halign=center` at load ("doesn't bind" — a
/// centered box pins no edge).
#[test]
#[ignore = "NEW: Godot refuses margin_left under halign=center at load; Rust builds clean and ignores the margin (box stays centered at x=180)"]
fn pin_matrix_center_with_edge_margin_is_refused() {
    assert!(gogo::has_errors(&compile("Panel b 40x30, halign=center, margin_left=4", &|_| None).diags));
}

/// `_pin_matrix_tests` 2114-2115 (pinmx.err-far-on-aligned): Godot refuses
/// a far-edge margin under a near-edge alignment (`halign=left,
/// margin_right=4`) at load.
#[test]
#[ignore = "NEW: Godot refuses margin_right under halign=left at load; Rust builds clean and ignores the margin (x stays 0)"]
fn pin_matrix_far_margin_under_near_alignment_is_refused() {
    assert!(gogo::has_errors(&compile("Panel b 40x30, halign=left, margin_right=4", &|_| None).diags));
}

/// `_pin_matrix_tests` 2116-2117 (pinmx.err-span-overconstrained): both
/// margins plus an exact width over-constrain the axis — a load error.
#[test]
fn pin_matrix_span_plus_width_over_constrains() {
    assert!(has("Panel b width=50, margin_left=2, margin_right=2", "G1006"));
}

// ------------------------------------------------------------ _example_tests

/// The addon's `examples/tutorial.gogo` (copied 2026-08-30), the first file
/// a new user opens.
const TUTORIAL: &str = r#"// Gogo in one file — a guided tour. Open it next to the Gogo editor panel,
// change anything, save, and watch it hot reload. Errors point at the line
// and usually suggest the fix; while a file has an error, the last good
// version stays on screen.
//
// One line shape is the whole language:
//   WidgetType name .StyleName property=value, property=value { children }
//
// Bare numbers on sizes/margins are effective pixels (epx) — density
// independent, so the layout means the same thing on desktop and phone.

// ---- theme values: $name = value, referenced anywhere a value goes --------
$accent = #E51400
$chrome = #1F1F1F
$scrim  = #000000AA

theme {
    font_size = 15    // propagating default: every Label without its own
}                     // font_size inherits this, epx-scaled

// ---- styles: a named property bag applied with .Name; `when` responses
// react to conditions, `transition` rules time the changes ------------------
style ToolButton min_height=36, background=#00000000 {
    transition any -> held duration=0.05, easing=cubic_out   // finger down
    transition held -> any duration=0.2,  easing=cubic_out   // finger up
    when held  background=$accent
    when disabled opacity=0.4
}

// ---- a reusable widget: $params with defaults, one root, one `slot` -------
widget Card $title="" {
    Panel background=$chrome, corner_radius=8, padding=12 {
        Column spacing=8 {
            Label text=$title, font_size=18
            slot                                 // instance children land here
        }
    }
}

// ---- an animation: a chain of steps (`and` = together, `then` = after) ----
// Motion words (translate/scale/rotate/opacity) are visual-only — layout
// never moves, and the transform disarms itself at rest. from= is asserted
// from play start; `duration_each=` is every step's default (the word says
// per-step), a line's own easing= or duration= wins.
anim appear duration_each=0.4, easing=cubic_out {
    panel.opacity     from=0,  to=1
    and panel.translate.y from=24, to=0, easing=back_out
}

// ---- the view --------------------------------------------------------------
// Placement is two questions with two props each:
//   WHERE    halign=left/center/right   valign=top/center/bottom
//   HOW BIG  width/height = 200 (exact epx) | hug (natural) | fill (leftover)
// The same words work under a Layers (free stacking) and inside Column/Row
// (stacked layout). min_width means "at least — still stretchy".
// The default: a child takes the space its parent gives it — the backdrop
// below fills the screen with no placement keys at all. A view file opens
// with `View` (the root keyword): its children place freely, like Layers.
View screen {
    Rectangle backdrop color=$scrim

    Card panel halign=center, valign=center, min_width=260, on:show=play(appear) {
        Row spacing=8 {
            Label caption text="Size", valign=center
            Slider sizeSlider width=fill, min=1, max=100, value=40
        }
        Row spacing=8 {
            Button cancelButton .ToolButton width=fill, text="Cancel"
            Button okButton .ToolButton width=fill(2), text="OK"   // 2x share
        }
        Label hint text="edit me and save — the view reloads live",
            font_size=12, halign=center
    }
}

// Where to go next:
//   COOKBOOK.md  — task-shaped recipes ("How do I center a control?")
//   REFERENCE.md — every property, condition, easing and error, for lookup
// Wiring: a named node auto-connects its signals to your host script by
// convention (okButton + held -> OnOkButtonPressed), or explicitly with
// on:activate=HandlerName / on:activate=emit(HostSignal).
"#;

/// `_example_tests` 2124-2134 (tutorial.readable, builds, named): the
/// shipped tutorial builds clean and its `panel`, `okButton` and
/// `sizeSlider` are addressable. (The file is inlined above; "readable" is
/// the literal being present.)
#[test]
fn example_tutorial_builds() {
    assert!(!TUTORIAL.is_empty());
    let c = ok(TUTORIAL);
    for name in ["panel", "okButton", "sizeSlider"] {
        assert!(c.page.find(name).is_some(), "named {name}");
    }
}

// ----------------------------------------------------------- _cookbook_tests

/// `_cookbook_tests` 2141-2153 (cookbook.center-ok, center-anchors): "How
/// do I center a control?" — `halign=center, valign=center` under a free
/// parent puts the dialog's middle at the parent's middle.
#[test]
fn cookbook_center_under_free_parent() {
    let l = lay("$chrome = #1F1F1F\nView page {\n Panel dialog halign=center, valign=center, background=$chrome, corner_radius=8, padding=16 {\n  Label text=\"Centered both ways\", font_size=15\n }\n}\n");
    let (cx, cy) = l.center("dialog");
    assert!(near(cx, 200.0) && near(cy, 200.0), "{:?}", l.rect("dialog"));
}

/// `_cookbook_tests` 2155-2164 (cookbook.halign-ok, halign-centers): the
/// same `halign=center` word centers a Button inside a Column.
#[test]
fn cookbook_center_inside_a_column() {
    let l = lay("View v {\n Column c spacing=8 {\n  Label title text=\"brush\", font_size=15\n  Button okButton halign=center, text=\"OK\", min_width=120\n }\n}\n");
    let (cx, _) = l.center("okButton");
    assert!(near(cx, 200.0), "{:?}", l.rect("okButton"));
    assert_eq!(l.rect("okButton").w, 120.0, "{:?}", l.rect("okButton"));
}

/// `_cookbook_tests` 2167-2183 (cookbook.text-ok, text-halign, text-valign,
/// text-wrap): "How do I center or wrap text?" — the alignment words read
/// back on the Label and the wrap switch is on. Dialect: Godot's
/// `autowrap_mode=word_smart` is `wrap=true` here (the old key is G0041).
#[test]
fn cookbook_text_alignment_and_wrap() {
    let l = lay("View v {\n Column c {\n  Label caption text=\"fullscreen\", font_size=12, horizontal_alignment=center\n  Label rowLabel text=\"save + share\", font_size=15, min_height=48, vertical_alignment=center\n  Label blurb text=\"A longer description that wraps onto several lines\",\n   font_size=13, min_width=180, wrap=true\n }\n}\n");
    assert_eq!(l.res.ident(&l.page, l.id("caption"), PropKey::TextHalign), Some("center"));
    assert_eq!(l.res.ident(&l.page, l.id("rowLabel"), PropKey::TextValign), Some("center"));
    assert_eq!(l.res.bool(&l.page, l.id("blurb"), PropKey::Wrap), Some(true));
    assert!(has("View v { Label b text=\"x\", autowrap_mode=word_smart }", "G0041"), "the Godot key is refused with the fix");
}

/// `_cookbook_tests` 2188-2210 (cookbook.bodied-radio-ok, bodied-radio-
/// seat): the bodied Radio recipe (D142) builds, and the choice's
/// `value=color` starts that Radio selected and the other not.
#[test]
fn cookbook_bodied_radio_starts_from_the_choice_value() {
    let src = "$accent = #E51400FF\nView v {\n Column c {\n  choice paintMode value=color {\n   Radio color 52x50 {\n    Circle colorRing 35x35, halign=center, valign=top,\n     border_color=#FFFFFFFF, border_thickness=1.5\n    Label text=\"color\", font_size=11, valign=top, margin_top=36,\n     width=fill, horizontal_alignment=center\n    when selected colorRing.color=$accent\n   }\n   Radio rainbow 52x50 {\n    Circle rainbowRing 35x35, halign=center, valign=top,\n     border_color=#FFFFFFFF, border_thickness=1.5\n    when selected rainbowRing.color=$accent\n   }\n  }\n }\n}\n";
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    assert!(rt.has(c.page.find("color").unwrap(), SELECTED));
    assert!(!rt.has(c.page.find("rainbow").unwrap(), SELECTED));
}

/// `_cookbook_tests` 2214-2224 (cookbook.padding-ok, padding-margins,
/// padding-invisible): "How do I add margins?" — a Panel with `padding=12`
/// and no background is the invisible wrapper: 12 of air on every side,
/// nothing painted for the Panel itself.
#[test]
fn cookbook_padding_panel_is_invisible_air() {
    let l = lay("Panel air padding=12 {\n Label inner text=\"12 epx of air on every side\", font_size=15\n}\n");
    let air = l.rect("air");
    let inner = l.rect("inner");
    assert!(near(inner.x - air.x, 12.0) && near(air.bottom() - inner.bottom(), 12.0), "air={air:?} inner={inner:?}");
    assert!(quads(&l.draws()).is_empty(), "no plate for a bare padded Panel: {:#?}", l.draws());
}

/// `_cookbook_tests` 2229-2242 (cookbook.padding-sides-ok, padding-sides):
/// per-side padding keys each move exactly their side, and a later per-side
/// key refines an earlier uniform one (source order).
#[test]
fn cookbook_per_side_padding_refines_uniform() {
    let l = lay("Panel air2 padding=12, padding_top=8, padding_bottom=10 {\n Label inner text=\"asymmetric air\", font_size=15\n}\n");
    let air = l.rect("air2");
    let inner = l.rect("inner");
    let insets = (inner.x - air.x, inner.y - air.y, air.right() - inner.right(), air.bottom() - inner.bottom());
    assert_eq!(insets, (12.0, 8.0, 12.0, 10.0), "air={air:?} inner={inner:?}");
}

/// `_cookbook_tests` 2252-2290 (cookbook.border-padding-ok, stacks,
/// border-alone-insets, border-padding-order, border-padding-float): padding
/// is air INSIDE the border (D80) — content sits border + padding from the
/// edge; a border alone insets by its width; the sum is order-independent
/// (style row or inline, padding before or after border) and keeps a
/// fractional border. Godot's `border_width_left == 1` (painted whole px)
/// is ENGINE.
#[test]
fn cookbook_padding_stacks_inside_the_border() {
    let l = lay("style Framed border_color=#FFFFFFFF, border_thickness=2 {\n}\nView v {\n Column c {\n  Panel framed .Framed padding=10 { Label fl text=\"x\", font_size=15 }\n  Panel borderOnly border_color=#FFFFFFFF, border_thickness=2 { Label bl text=\"x\", font_size=15 }\n  Panel swapped border_thickness=2, border_color=#FFFFFFFF, padding=10 { Label sl text=\"x\", font_size=15 }\n  Panel fractional padding=12, padding_top=6, border_color=#FFFFFFFF, border_thickness=1.5 { Label xl text=\"x\", font_size=15 }\n }\n}\n");
    let inset = |panel: &str, label: &str| {
        let p = l.rect(panel);
        let c = l.rect(label);
        (c.x - p.x, c.y - p.y, p.bottom() - c.bottom())
    };
    assert_eq!(inset("framed", "fl"), (12.0, 12.0, 12.0), "style border + inline padding stack");
    assert_eq!(inset("borderOnly", "bl").0, 2.0, "border alone insets");
    let (sl, _, sb) = inset("swapped", "sl");
    assert_eq!((sl, sb), (12.0, 12.0), "order-independent");
    let (xl, xt, _) = inset("fractional", "xl");
    assert!(near(xl, 13.5) && near(xt, 7.5), "fractional border kept: l={xl} t={xt}");
}

/// `_cookbook_tests` 2295-2303 (cookbook.panel-blank-ok, panel-blank-
/// slate): a Panel is a blank canvas — nothing drawn until a stylebox key
/// says so (no theme gray, no corner).
#[test]
fn cookbook_panel_is_a_blank_slate() {
    let l = lay("Panel bare {\n Label x text=\"x\"\n}");
    assert!(quads(&l.draws()).is_empty(), "{:#?}", l.draws());
}

/// `_cookbook_tests` 2306-2318 (cookbook.button-chrome-ok, button-recolors-
/// not-reshapes): ONE meaning for `background=` on a Button — it recolors
/// the plate and never reshapes it: the corner radius and the padding stay
/// what an unstyled Button has. (The chrome's numbers are the Rust look by
/// decision — todo-gogo.md port inventory, DECIDED 2026-08-30 — so the pin
/// compares against an unstyled twin, not Godot's theme values.)
#[test]
fn cookbook_button_background_recolors_not_reshapes() {
    let l = lay("View v {\n Button b text=\"go\", background=#4488FFFF, halign=left, valign=top\n Button plain text=\"go\", halign=left, valign=top, margin_top=100\n}\n");
    let list = l.draws();
    let plate = |name: &str| {
        let r = l.rect(name);
        quads(&list).into_iter().find(|(q, _, _)| *q == r).unwrap_or_else(|| panic!("no plate for {name}: {list:#?}"))
    };
    let (b, b_color, b_corner) = plate("b");
    let (p, _, p_corner) = plate("plain");
    assert_eq!(b_color, Rgba([0x44, 0x88, 0xFF, 0xFF]), "recolored");
    assert_eq!(b_corner, p_corner, "same corner as the unstyled twin");
    assert_eq!((b.w, b.h), (p.w, p.h), "same padding as the unstyled twin: {b:?} vs {p:?}");
}

/// `_cookbook_tests` 2325-2345 (cookbook.margins-ok, margins-edges,
/// margins-inset-filled-axis, margins-pin-aligned-axis): margins never pick
/// the sizing mode (D71) — on the ALIGNED axis they offset the pinned edge;
/// on an unkeyed axis they inset the default-filled box.
#[test]
fn cookbook_margins_offset_pinned_edges_and_inset_filled_axes() {
    let l = lay("View body {\n Column fileOps valign=bottom, margin_left=40, margin_bottom=90 {\n  Button newButton text=\"new\"\n }\n Column hud halign=left, valign=top, margin_left=30 {\n  Button hudButton text=\"hud\"\n }\n}\n");
    let ops = l.rect("fileOps");
    assert!(near(ops.x, 40.0) && near(ops.bottom(), 310.0), "edges: {ops:?}");
    assert!(near(ops.right(), 400.0), "the unkeyed axis is the filled box inset by the margin: {ops:?}");
    let hud = l.rect("hud");
    assert!(near(hud.x, 30.0) && hud.w < 200.0, "aligned axis pins and hugs: {hud:?}");
}

/// `_cookbook_tests` 2351-2372 (place.mcarve-ok, far-inset, shorthand-
/// insets, hug-still-pins): the D71 margin-carving matrix on free children —
/// a lone far margin insets the filled box (a negative one hangs it 10
/// below), `margin=` is breathing room all around, `hug` + a margin is
/// still a natural-size pin. (Godot's raw `Control` root is `View`.)
#[test]
fn cookbook_margin_carving_on_free_children() {
    let l = lay("View screen3 {\n Rectangle strip color=#FFFFFFFF, margin_bottom=-10\n Rectangle padded color=#FFFFFFFF, margin=12\n Rectangle pinned color=#FFFFFFFF, width=hug, margin_left=8\n}\n");
    let strip = l.rect("strip");
    assert!(near(strip.y, 0.0) && near(strip.bottom(), 410.0), "{strip:?}");
    let padded = l.rect("padded");
    assert_eq!(padded, Rect::new(12.0, 12.0, 376.0, 376.0), "{padded:?}");
    let pinned = l.rect("pinned");
    assert!(near(pinned.x, 8.0) && near(pinned.w, 0.0), "{pinned:?}");
}

/// `_cookbook_tests` 2376-2394 (cookbook.layers-ok, layers-fill, layers-
/// toast): "How do I overlay controls?" — a fill/fill scrim covers the
/// parent; a centered toast sits 20 from the top.
#[test]
fn cookbook_layers_scrim_and_toast() {
    let l = lay("$chrome = #1F1F1F\nView screen {\n Rectangle scrim color=#000000B0, width=fill, height=fill\n Panel toast halign=center, valign=top, margin_top=20,\n   background=$chrome, corner_radius=6, padding=10 {\n  Label text=\"Saved!\", font_size=15\n }\n}\n");
    assert_eq!(l.rect("scrim"), VIEW, "{:?}", l.rect("scrim"));
    let (cx, _) = l.center("toast");
    assert!(near(cx, 200.0) && near(l.rect("toast").y, 20.0), "{:?}", l.rect("toast"));
}

/// `_cookbook_tests` 2398-2412 (cookbook.fill-ok, fill-icon, fill-margins):
/// "How do I make something fill the leftover space?" under Layers — the
/// icon sits 8 in and vertically centered, the fill/fill title reaches the
/// right and bottom edges from 48 in. (`valign=top` added so the row hugs
/// its 48 minimum as Godot's detached root did.)
#[test]
fn cookbook_fill_under_layers() {
    let l = lay("View v {\n Layers row min_height=48, valign=top {\n  Image icon size=32x32, valign=center, margin_left=8\n  Label title width=fill, height=fill, margin_left=48,\n   font_size=15, vertical_alignment=center\n }\n}\n");
    let row = l.rect("row");
    assert_eq!(row.h, 48.0, "{row:?}");
    let icon = l.rect("icon");
    assert!(near(icon.x, 8.0) && near(icon.y, 8.0), "{icon:?}");
    let title = l.rect("title");
    assert!(near(title.x, 48.0) && near(title.right(), row.right()) && near(title.bottom(), row.bottom()), "{title:?} in {row:?}");
}

/// `_cookbook_tests` 2414-2426 (cookbook.rowfill-ok, rowfill-fills,
/// rowfill-label-centered): in a Row, `width=fill` takes what the Label
/// leaves, and `valign=center` centers the Label on the cross axis.
#[test]
fn cookbook_fill_in_a_row() {
    let l = lay("View v {\n Row r spacing=8 {\n  Label label text=\"Size\", font_size=15, valign=center\n  Slider sizeSlider width=fill, min=1, max=100\n }\n}\n");
    let r = l.rect("r");
    let label = l.rect("label");
    let slider = l.rect("sizeSlider");
    assert!(near(slider.w, r.w - label.w - 8.0) && near(slider.right(), r.right()), "{slider:?} label={label:?}");
    assert!(near(label.y + label.h / 2.0, r.y + r.h / 2.0), "{label:?} in {r:?}");
}

/// `_cookbook_tests` 2430-2439 (cookbook.hidden-ok, hidden-holds-space,
/// hidden-sibling-live): "How do I hide something without collapsing the
/// layout?" — WPF Hidden = `opacity=0, interactive=false`: the ghost still
/// holds its slot and its text is drawn at alpha 0, a tap on it lands on
/// nothing, and the sibling stays live.
#[test]
fn cookbook_hidden_holds_space() {
    let l = lay("View v {\n Column c spacing=8 {\n  Button ghostButton text=\"soon\", opacity=0, interactive=false\n  Button saveButton  text=\"save\"\n }\n}\n");
    let ghost = l.rect("ghostButton");
    let save = l.rect("saveButton");
    assert!(ghost.h > 0.0 && near(save.y, ghost.bottom() + 8.0), "ghost={ghost:?} save={save:?}");
    assert_eq!(text_alpha(&l.page, &l.draws(), "soon"), 0);
    assert_eq!(l.hit(ghost.x + 5.0, ghost.y + 5.0), None, "the ghost is hit-invisible");
    assert_eq!(l.hit(save.x + 5.0, save.y + 5.0), Some(l.id("saveButton")));
}

// ------------------------------------------------------------ _extends_tests

/// `_extends_tests` 2986-2997 (extends.ok, inherits, overrides): a style
/// that `extends=` another inherits its props and overrides the ones it
/// re-states.
#[test]
#[ignore = "GP40: not ported: style extends= (the key is a G2001 warning, the parent's props are not inherited)"]
fn extends_inherits_and_overrides() {
    let c = ok("style BaseLook min_height=36, background=#112233FF\nstyle Warn extends=BaseLook, background=#AA0000FF\nView v { Button b .Warn }\n");
    let b = c.page.find("b").unwrap();
    assert_eq!(c.page.nodes[b as usize].placement.min_height, 36.0, "inherited");
    assert_eq!(c.page.prop(b, PropKey::Background), Some(Value::VColor(Rgba([0xAA, 0, 0, 0xFF]))), "overridden");
}

/// `_extends_tests` 3000-3019 (extends.chain-ok, chain-props, chain-states,
/// state-overridden): a three-level chain keeps the inherited `when`
/// responses and the deepest override of a shared one wins.
#[test]
#[ignore = "GP40: not ported: style extends= (chained props and whens are not inherited)"]
fn extends_three_level_chain_with_whens() {
    let c = ok("style A min_height=20 { when hovered background=#22222200 }\nstyle B extends=A, min_width=30 { when held background=#11111100 }\nstyle C extends=B { when held background=#33333300 }\nView v { Button b .C }\n");
    let b = c.page.find("b").unwrap();
    let pl = c.page.nodes[b as usize].placement;
    assert_eq!((pl.min_width, pl.min_height), (30.0, 20.0));
    let (wf, wn) = c.page.nodes[b as usize].whens;
    let whens = &c.page.whens[wf as usize..(wf + wn) as usize];
    assert!(whens.iter().any(|w| w.state == State::Hovered), "{whens:#?}");
    let held = whens.iter().filter(|w| w.state == State::Held).last().unwrap_or_else(|| panic!("{whens:#?}"));
    let bg = c.page.props[held.props.0 as usize..(held.props.0 + held.props.1) as usize].iter().rev().find(|p| p.key == PropKey::Background).map(|p| p.value);
    assert_eq!(bg, Some(Value::VColor(Rgba([0x33, 0x33, 0x33, 0]))), "the deepest held response wins");
}

/// `_extends_tests` 3023-3028 (extends.err-cycle, err-unknown-parent,
/// err-not-a-name): a cycle, an unknown parent and a non-name value are
/// load errors.
#[test]
#[ignore = "GP40: not ported: style extends= (no cycle / unknown-parent / not-a-name checks; all three build clean)"]
fn extends_errors() {
    assert!(gogo::has_errors(&compile("style A extends=B\nstyle B extends=A\nView v { Button x .A }", &|_| None).diags), "cycle");
    assert!(gogo::has_errors(&compile("style A extends=Nope\nView v { Button x .A }", &|_| None).diags), "unknown parent");
    assert!(gogo::has_errors(&compile("style A extends=12\nView v { Button x .A }", &|_| None).diags), "not a name");
}

/// `_extends_tests` 3031-3038 (extends.across-uses): a local style extends
/// one brought in with `uses`.
#[test]
#[ignore = "GP40: not ported: style extends= (the imported parent's spacing is not inherited)"]
fn extends_across_uses() {
    let loader = |path: &str| if path == "fixtures/theme_inc.gogo" { Some("// Shared include for view_test.gogo (exercises `uses` + theme + style merge).\n$swatchTest = #EB6428FF\nstyle TestPanel spacing=8\n".to_string()) } else { None };
    let c = compile("uses \"fixtures/theme_inc.gogo\"\nstyle Local extends=TestPanel, min_height=99\nView v { Column c .Local }\n", &loader);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let col = c.page.find("c").unwrap();
    assert_eq!(c.page.nodes[col as usize].placement.min_height, 99.0);
    assert_eq!(c.page.prop(col, PropKey::Spacing), Some(Value::VNum(8.0)), "inherited across uses");
}

// ---------------------------------------------------------------- _epx_tests

/// `_epx_tests` 3043-3049 (epx.ok, size-scaled, px-raw) — FLIP: Godot
/// scaled `220x` through an epx provider (440 px at 2x) and kept `36px`
/// raw. Rust's layout stays in epx — the renderer scales (gogo-design
/// "`epx` → device px scaling is a uniform in that shader") — and `px` is
/// not a unit a file may write (gogo-design §7 "Units: epx only; device
/// px never appear in a `.gogo`"; G0038).
#[test]
fn epx_layout_stays_in_epx_and_px_is_refused() {
    assert!(has("View v { Button b 220x, min_height=36px }", "G0038"), "px is gone");
    let l = lay("View v { Button b 220x, min_height=36, halign=left, valign=top }");
    let b = l.rect("b");
    assert_eq!((b.w, b.h), (220.0, 36.0), "epx as written, no scaling in the layout: {b:?}");
}

/// `_epx_tests` 3052-3055 (epx.scalar-raw): a bare number on a non-length
/// prop is the raw value — a Slider's `value=80` is 80.
#[test]
fn epx_scalar_props_are_raw() {
    let l = lay("View v { Slider s value=80, max=100 }");
    assert_eq!(l.res.slider_value(&l.page, l.id("s")), 80.0);
}

/// `_epx_tests` 3057 (epx.percent-error): `min_height=50%` is refused
/// ("not supported").
#[test]
fn epx_percent_is_refused() {
    assert!(gogo::has_errors(&compile("View v { Button b min_height=50% }", &|_| None).diags));
}

// -------------------------------------------------------------- _vocab_tests

/// `_vocab_tests` 12325-12339 (vocab.retired-textblock, textbox, border,
/// scrollviewer, wrappanel, raw-panelcontainer, vbox, hbox, flowcontainer,
/// colorrect, hslider, lineedit) and 12340-12342 (vocab.raw-escape-stays)
/// — FLIP: Godot redirected retired and raw engine names with a hint and
/// kept raw `Control` as an escape; Rust has no alias table and no raw
/// register (gogo-design §7 "Retired-spelling redirects", "The raw
/// register"): every one of them is G7011, `Control` included.
#[test]
fn vocab_retired_and_raw_words_are_unknown() {
    for line in ["TextBlock t text=\"x\"", "TextBox t", "Border b", "ScrollViewer s", "WrapPanel w", "PanelContainer p", "VBoxContainer v", "HBoxContainer h", "FlowContainer f", "ColorRect c", "HSlider s", "LineEdit l", "Control c min_width=10"] {
        let src = format!("View v {{ {line} }}");
        assert!(has(&src, "G7011"), "{line}: {:?}", codes(&src));
    }
}

/// `_vocab_tests` 12345-12353 (vocab.new-names-build, flow-class,
/// panel-class, label-class, input-class, slider-class) and 12362-12365
/// (vocab.panel-shadows-raw): the current names build to their kinds, and
/// `Panel` is gogo's Panel.
#[test]
fn vocab_new_names_build_to_their_kinds() {
    let c = ok("View v { Column c { Flow f; Panel p; Label l text=\"x\"; Input i; Slider s } }");
    let col = c.page.find("c").unwrap();
    let kinds: Vec<Kind> = c.page.children(col).map(|i| c.page.nodes[i].kind).collect();
    assert_eq!(kinds, vec![Kind::Flow, Kind::Panel, Kind::Label, Kind::Input, Kind::Slider]);
    let p = ok("Panel p");
    assert_eq!(p.page.nodes[0].kind, Kind::Panel);
}

/// `_vocab_tests` 12355-12358 (vocab.flow-orientation): `Flow` keeps
/// WrapPanel's `orientation=vertical` — items run down, not across.
#[test]
#[ignore = "NEW: Godot Flow orientation=vertical runs items down; Rust warns G2001 (unknown prop) and lays them across (b at x=10,y=0)"]
fn vocab_flow_orientation_vertical() {
    let l = lay("View v { Flow f orientation=vertical { Rectangle a 10x10; Rectangle b 10x10 } }");
    let a = l.rect("a");
    let b = l.rect("b");
    assert!(near(b.x, a.x) && near(b.y, a.bottom()), "a={a:?} b={b:?}");
}

/// `_vocab_tests` 12371-12391 (vocab.view-defaults, view-default-bg,
/// view-default-borderless) and 12394-12398 (vocab.view-file-wins):
/// `View` is THE root (D104): it fills both axes, draws no plate and no
/// border, and a `background=` in the file draws the plate.
#[test]
fn vocab_view_defaults_and_file_wins() {
    let l = lay("View v");
    assert_eq!(l.rect("v"), VIEW, "{:?}", l.rect("v"));
    assert!(l.draws().is_empty(), "transparent, borderless: {:#?}", l.draws());
    let l = lay("View v background=#FF0000FF");
    assert_eq!(quads(&l.draws()), vec![(VIEW, Rgba([0xFF, 0, 0, 0xFF]), 0.0)], "{:#?}", l.draws());
}

/// `_vocab_tests` 12401-12406 (vocab.view-halign-skips-fill): a positioned
/// root skips the fill default on that axis only — `halign=center` hugs
/// the width (0 for an empty View, at mid-frame) while the height still
/// fills.
#[test]
fn vocab_view_alignment_skips_fill_on_that_axis() {
    let l = lay("View v halign=center");
    assert_eq!(l.rect("v"), Rect::new(200.0, 0.0, 0.0, 400.0), "{:?}", l.rect("v"));
}

/// `_vocab_tests` 12408-12409 (vocab.view-non-root): `View` inside the
/// tree is an error that routes to `Layers` / `Panel`.
#[test]
fn vocab_view_is_root_only() {
    let m = message("View a { Column c { View v } }", "G7012");
    assert!(m.contains("Layers") && m.contains("Panel"), "{m}");
}

/// `_vocab_tests` 12410 (vocab.view-not-widget-body): a widget body may
/// not be a `View`.
#[test]
#[ignore = "NEW: Godot refuses a View as a widget body; Rust builds `widget W { View v { } }` + `W w` clean (G7012 fires only when the instance sits inside a tree)"]
fn vocab_view_is_not_a_widget_body() {
    assert!(gogo::has_errors(&compile("widget W { View v { } }\nW w", &|_| None).diags));
}

/// `_vocab_tests` 12413-12420 (vocab.gap-builds, gap-node, gap-min-y,
/// gap-mouse-ignore, gap-anonymous): `gap 12` in a Column is an anonymous
/// Gap node 12 tall that nothing can tap.
#[test]
fn vocab_gap_in_a_column() {
    let l = lay("View v { Column c { Label a text=\"x\"; gap 12; Label b text=\"y\" } }");
    let col = l.id("c");
    let mid = l.page.children(col).nth(1).unwrap() as u32;
    assert_eq!(l.page.nodes[mid as usize].kind, Kind::Gap);
    assert_eq!((l.page.nodes[mid as usize].gap, l.page.nodes[mid as usize].gap_fill), (12.0, false));
    assert!(near(l.rect("b").y - l.rect("a").bottom(), 12.0), "a={:?} b={:?}", l.rect("a"), l.rect("b"));
    let g = l.rects[mid as usize];
    assert_eq!(l.hit(g.x + 5.0, g.y + 5.0), None, "a gap takes no input");
    assert_eq!(l.page.nodes[mid as usize].name, NO_STR);
    assert!(l.page.find("gap").is_none());
}

/// `_vocab_tests` 12421-12435 (vocab.gap-row-axis, gap-fill-col, gap-fill-
/// row): in a Row the gap runs along x; `gap fill` takes the leftover on
/// the stack's axis in either.
#[test]
fn vocab_gap_axis_and_fill_forms() {
    let l = lay("View v { Row r { Rectangle a 10x10; gap 8; Rectangle b 10x10 } }");
    assert!(near(l.rect("b").x - l.rect("a").right(), 8.0), "a={:?} b={:?}", l.rect("a"), l.rect("b"));
    let l = lay("View v { Column c { gap fill } }");
    let g = l.page.children(l.id("c")).next().unwrap();
    assert!(l.page.nodes[g].gap_fill);
    assert_eq!(l.rects[g].h, 400.0, "{:?}", l.rects[g]);
    let l = lay("View v { Row r { gap fill } }");
    let g = l.page.children(l.id("r")).next().unwrap();
    assert!(l.page.nodes[g].gap_fill);
    assert_eq!(l.rects[g].w, 400.0, "{:?}", l.rects[g]);
}

/// `_vocab_tests` 12437-12441 (vocab.gap-top-level, gap-in-flow, gap-in-
/// layers, gap-in-grid, gap-in-panel): a gap outside a Column/Row is
/// refused and the message names the mechanism that owns spacing there
/// (Flow → `spacing=`, Grid → tracks, Layers → margins).
#[test]
fn vocab_gap_wrong_homes() {
    let m = message("gap 8", "G0047");
    assert!(m.contains("Column/Row"), "{m}");
    assert!(message("View v { Flow f { gap 8 } }", "G0047").contains("spacing="));
    assert!(message("View v { Layers l { gap 8 } }", "G0047").contains("margins"));
    assert!(message("View v { Grid g cols=[hug] { row { gap 4 } } }", "G0047").contains("tracks"));
    assert!(has("View v { Panel p { gap 8 } }", "G0047"));
}

/// `_vocab_tests` 12443-12444, 12446 (vocab.gap-no-size, gap-bad-word,
/// gap-no-props): one size word and nothing else — no word, a wrong word,
/// or a property is refused.
#[test]
fn vocab_gap_takes_one_size_word() {
    let m = message("View v { Column c { gap } }", "G0048");
    assert!(m.contains("needs a size"), "{m}");
    let m = message("View v { Column c { gap huge } }", "G0048");
    assert!(m.contains("gap 12") && m.contains("gap fill"), "{m}");
    let m = message("View v { Column c { gap 8, background=red } }", "G0045");
    assert!(m.contains("nothing else"), "{m}");
}

/// `_vocab_tests` 12445 (vocab.gap-both-forms): `gap fill 8` is refused —
/// never both forms. (Rust refuses it through the member grammar: a second
/// positional value needs a comma, G0017 — not a gap-specific message.)
#[test]
fn vocab_gap_refuses_both_forms() {
    let c = compile("View v { Column c { gap fill 8 } }", &|_| None);
    assert!(gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

/// `_vocab_tests` 12447 (vocab.gap-no-children): a gap IS the space — it
/// takes no children.
#[test]
#[ignore = "NEW: Godot refuses children on a gap (IS the space); Rust builds `gap 8 { Label x }` clean, the Label is never laid out (rect 0x0)"]
fn vocab_gap_refuses_children() {
    assert!(gogo::has_errors(&compile("View v { Column c { gap 8 { Label x text=\"x\" } } }", &|_| None).diags));
}

/// `_vocab_tests` 12448 (vocab.gap-reserved-widget): `gap` is a built-in
/// word — a widget may not take the name.
#[test]
#[ignore = "NEW: Godot refuses `widget gap`; Rust accepts it and a later `gap` line expands the widget (widget lookup precedes the built-in)"]
fn vocab_gap_is_reserved_as_a_widget_name() {
    assert!(gogo::has_errors(&compile("widget gap { Panel p }\nView c", &|_| None).diags));
}
