//! E8 (2026-08-31): one table says which property words each kind takes
//! (`page::legal`), the compiler refuses the rest naming the gogo word, any
//! box has a plate, and `theme font_size=` inside a block reaches the text
//! under it. The Godot-suite ports in `godot_*.rs` cover the fences that
//! match Godot; these pins cover what E8 decides differently or adds.

use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Kind, PropKey, Rgba, legal};
use gogo::runtime::{Runtime, resolve};
use gogo::{CompileOptions, Compiled, Diagnostic, Page, compile, compile_with};

fn diags(src: &str) -> Vec<Diagnostic> {
    compile(src, &|_| None).diags
}

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

/// The first error's (code, message), or a panic naming the diags.
fn error(src: &str) -> (String, String) {
    let d = diags(src);
    let e = d.iter().find(|d| d.severity == gogo::Severity::Error).unwrap_or_else(|| panic!("expected an error: {d:#?}"));
    (e.code.clone(), e.message.clone())
}

fn scene(src: &str) -> (Page, Vec<Rect>, Vec<Draw>) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let draws = draw(&c.page, &res, &rects);
    (c.page, rects, draws)
}

fn rect_of(page: &Page, rects: &[Rect], name: &str) -> Rect {
    rects[page.find(name).unwrap_or_else(|| panic!("no node {name}")) as usize]
}

fn quads(draws: &[Draw]) -> Vec<(Rect, Rgba, f32)> {
    draws.iter().filter_map(|d| if let Draw::Quad { rect, color, corner, .. } = d { Some((*rect, *color, *corner)) } else { None }).collect()
}

// ------------------------------------------------------------ the table

/// The table is the one source: a word a kind takes is legal, the others
/// are not, and cell words count as legal on every box (the parent check
/// is the compiler's).
#[test]
fn table_answers_per_kind() {
    assert!(legal(Kind::Label, PropKey::Background), "any box has a plate");
    assert!(legal(Kind::Layers, PropKey::Background));
    assert!(legal(Kind::Image, PropKey::CornerRadius));
    assert!(legal(Kind::Custom, PropKey::Padding), "code controls take the shared row");
    assert!(!legal(Kind::Label, PropKey::Color), "text is `font_color=`");
    assert!(!legal(Kind::Rectangle, PropKey::Background), "a Rectangle's fill is `color=`");
    assert!(!legal(Kind::Rectangle, PropKey::Padding));
    assert!(!legal(Kind::Column, PropKey::FontSize), "font_size on a stack is dead — `theme font_size=` is the subtree spelling");
    assert!(legal(Kind::Column, PropKey::Span) && legal(Kind::Rectangle, PropKey::ColSpan), "cell words: the parent decides");
    assert!(!legal(Kind::Gap, PropKey::Width) && !legal(Kind::Repeat, PropKey::Opacity), "structure lines take clauses, not properties");
}

// ------------------------------------------------------- the messages

/// An unknown word: the closest legal word for a near-miss, the gogo
/// word for a known other-dialect spelling, the kind's own words when
/// nothing is close.
#[test]
fn unknown_word_names_the_nearest_legal_word() {
    let (code, msg) = error("View v { Label a textt=\"x\" }");
    assert_eq!(code, "G2001");
    assert!(msg.contains("did you mean `text=`"), "{msg}");
    let (_, msg) = error("View v { Button b foreground=red }");
    assert!(msg.contains("font_color="), "{msg}");
    let (_, msg) = error("View v { Scroll s zzz=1 }");
    assert!(msg.contains("`allow`") && msg.contains("`bars`"), "own words listed: {msg}");
}

/// A real gogo word on the wrong kind: G2002 says whose word it is and
/// what this kind writes instead.
#[test]
fn wrong_kind_says_whose_word_it_is() {
    let (code, msg) = error("View v { Label a color=red }");
    assert_eq!(code, "G2002");
    assert!(msg.contains("font_color=") && msg.contains("background="), "{msg}");
    let (_, msg) = error("View v { Rectangle r background=red }");
    assert!(msg.contains("color="), "{msg}");
    let (_, msg) = error("View v { Column c value=3 }");
    assert!(msg.contains("Slider"), "{msg}");
    let (_, msg) = error("View v { Image i text=\"x\", source=\"res://a.png\" }");
    assert!(msg.contains("Label"), "{msg}");
    let (_, msg) = error("View v { Panel p font_size=20 }");
    assert!(msg.contains("shows text"), "{msg}");
}

/// Cell words belong to the parent's lattice: `span=` under a Column and
/// `col_span=` under Tiles are refused naming the parent.
#[test]
fn cell_words_are_checked_against_the_parent() {
    let (code, msg) = error("View v { Column c { Rectangle a span=2, min_size=1x1 } }");
    assert_eq!(code, "G2002");
    assert!(msg.contains("Tiles cell word") && msg.contains("Column"), "{msg}");
    let (_, msg) = error("View v { Tiles t cols=2 { Rectangle a col_span=2, min_size=1x1 } }");
    assert!(msg.contains("Grid cell word") && msg.contains("Tiles"), "{msg}");
    ok("View v { Tiles t cols=2 { Rectangle a span=2, min_size=1x1 } }");
    ok("View v { Grid g cols=[hug, hug] { row { Rectangle a col_span=2, min_size=1x1 } } }");
}

/// GPAR82: a `when` response aimed at a child gets the same word check
/// once the child is found — the target's kind, not the owner's, decides
/// (`opacity` on the child Rectangle is fine; `text=` on it is not).
#[test]
fn a_child_aimed_when_response_is_checked_against_the_childs_kind() {
    ok("View v { Button b text=\"x\" {\n when held flash.opacity=0.5\n Rectangle flash 8x8\n} }");
    let (code, msg) = error("View v { Button b text=\"x\" {\n when held flash.text=\"y\"\n Rectangle flash 8x8\n} }");
    assert_eq!(code, "G2002");
    assert!(msg.contains("Rectangle") && msg.contains("shows none"), "{msg}");
    let (code, msg) = error("View v { Button b text=\"x\" {\n when held flash.opacityy=0.5\n Rectangle flash 8x8\n} }");
    assert_eq!(code, "G2001");
    assert!(msg.contains("did you mean `opacity=`"), "{msg}");
    // The style spelling walks the same path.
    let (code, _) = error("style S { when held flash.z_as_relative=maybe }\nView v { Button b .S {\n Rectangle flash 8x8\n} }");
    assert_eq!(code, "G2001");
}

/// A code control takes the shared row (box, plate, padding) and refuses
/// the rest — `GradientEdit foo=1` used to build with no diagnostic.
#[test]
fn a_code_control_takes_the_shared_row() {
    let options = CompileOptions { code_controls: &["GradientEdit"], ..Default::default() };
    let c = compile_with("View v { GradientEdit g width=100, height=40, background=red, padding=4 }", &|_| None, options);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let c = compile_with("View v { GradientEdit g width=100, stops=[red, blue] }", &|_| None, options);
    assert!(gogo::has_errors(&c.diags), "{:#?}", c.diags);
    assert!(c.diags.iter().any(|d| d.code == "G2002" && d.message.contains("code control")), "{:#?}", c.diags);
    let c = compile_with("View v { GradientEdit g width=100, foo=1 }", &|_| None, options);
    assert!(c.diags.iter().any(|d| d.code == "G2001"), "{:#?}", c.diags);
}

/// `for=` on a style names a kind (checked) and is not a property of the
/// nodes wearing the style.
#[test]
fn style_for_names_a_kind() {
    ok("style S for=Button { when held opacity=0.8 }\nView v { Button b .S text=\"x\" }");
    let (code, _) = error("style S for=Buttn { when held opacity=0.8 }\nView v { Button b .S text=\"x\" }");
    assert_eq!(code, "G3006");
}

// ---------------------------------------------------------- the plate

/// Any box has a plate: a Label with `background=` draws one quad behind
/// its text covering the Label's own rectangle, padding included — the
/// same rectangle a Panel's plate covers — and the text sits inside the
/// padding. A Label without plate words draws no quad at all.
#[test]
fn a_label_plate_covers_its_padding() {
    let (page, rects, draws) = scene("View v { Label chip text=\"hi\", background=#FF0000FF, corner_radius=6, padding=4, halign=left, valign=top }");
    let chip = rect_of(&page, &rects, "chip");
    let q = quads(&draws);
    assert_eq!(q.len(), 1, "{draws:#?}");
    assert_eq!(q[0].0, chip, "the plate is the Label's own rectangle: {q:?} vs {chip:?}");
    assert_eq!(q[0].1, Rgba([0xFF, 0, 0, 0xFF]));
    assert_eq!(q[0].2, 6.0);
    let text = draws.iter().find_map(|d| if let Draw::Text { rect, .. } = d { Some(*rect) } else { None }).expect("text");
    assert_eq!(text, chip.inset(4.0, 4.0, 4.0, 4.0), "text inside the padding: {text:?}");
    let plate_index = draws.iter().position(|d| matches!(d, Draw::Quad { .. })).unwrap();
    let text_index = draws.iter().position(|d| matches!(d, Draw::Text { .. })).unwrap();
    assert!(plate_index < text_index, "plate under the text");
    let (_, _, bare) = scene("View v { Label plain text=\"hi\", padding=4 }");
    assert!(quads(&bare).is_empty(), "no plate words, no quad: {bare:#?}");
}

/// The plate on the other boxes: a Layers, a Column and an Image draw it
/// under their content; a border alone draws a transparent plate with the
/// border (as on a Panel).
#[test]
fn layers_stacks_and_images_take_a_plate() {
    let (page, rects, draws) = scene("View v { Layers l background=#00FF00FF, size=50x50, halign=left, valign=top { Rectangle r 10x10, color=blue } }");
    let q = quads(&draws);
    assert_eq!(q[0].0, rect_of(&page, &rects, "l"), "{q:?}");
    assert_eq!(q[0].1, Rgba([0, 0xFF, 0, 0xFF]));
    let (page, rects, draws) = scene("View v { Column c border_thickness=2, border_color=white, size=50x50, halign=left, valign=top { Rectangle r 10x10, color=blue } }");
    let plate = draws.iter().find_map(|d| if let Draw::Quad { rect, color, border, .. } = d { if *rect == rect_of(&page, &rects, "c") { Some((*color, *border)) } else { None } } else { None }).expect("a bordered plate");
    assert_eq!(plate, (Rgba::TRANSPARENT, 2.0));
    ok("View v { Image i source=\"res://icon.png\", background=black, corner_radius=4, size=20x20 }");
}

// ------------------------------------------------ subtree text defaults

/// `theme font_size=` / `theme font_color=` inside a block are the
/// subtree's defaults: every Label under the block draws with them unless
/// it says otherwise. Before E8 the root's `theme font_size=$fontNormal`
/// landed on the root alone and every Label under it drew at 15 — the
/// ad plate, caption and settings pages showed the wrong text size.
#[test]
fn theme_text_defaults_inside_a_block_reach_the_text_under_it() {
    let (_, _, draws) = scene("View v {\n theme font_size=22\n theme font_color=#123456FF\n Column c {\n  Label a text=\"a\"\n  Label b text=\"b\", font_size=9\n }\n}");
    let texts: Vec<(f32, Rgba)> = draws.iter().filter_map(|d| if let Draw::Text { font_size, color, .. } = d { Some((*font_size, *color)) } else { None }).collect();
    assert_eq!(texts, vec![(22.0, Rgba([0x12, 0x34, 0x56, 0xFF])), (9.0, Rgba([0x12, 0x34, 0x56, 0xFF]))], "{texts:?}");
    // The words are the text defaults only.
    let (code, msg) = error("View v {\n theme background=red\n Label a text=\"a\"\n}");
    assert_eq!(code, "G3006");
    assert!(msg.contains("font_size"), "{msg}");
}
