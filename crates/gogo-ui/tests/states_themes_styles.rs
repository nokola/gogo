//! Who wins when several lines say what a node looks like: its own
//! `when` against its style's, a page's theme against an included
//! file's, a written value against a kind's default. Also the theme
//! words every control follows, and a state written from code.

use gogo_ui::KeptFrame;
use gogo_ui::compile;
use gogo_ui::draw::{Draw, draw};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{Color_Straight_Srgb_4xU8_RGBA, NodeId, Page, PropKey, Value};
use gogo_ui::runtime::{DISABLED, HELD, Runtime, SELECTED, resolve, resolve_animated};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
const RED: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255]);
const GREEN: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255]);
const BLUE: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0, 0, 255, 255]);

/// The page, compiled with one included file (`styles/theme.gogo`).
fn page_with(src: &str, theme_file: &str) -> Page {
    let c = compile(src, &|path| if path == "styles/theme.gogo" { Some(theme_file.to_string()) } else { None });
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c.page
}

fn page(src: &str) -> Page {
    page_with(src, "")
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{}: {}\n", d.code, d.message)).collect()
}

fn node(page: &Page, name: &str) -> NodeId {
    page.find(name).unwrap_or_else(|| panic!("no node named {name}"))
}

fn rects(page: &Page, rt: &Runtime) -> Vec<Rect> {
    KeptFrame::fresh(page, resolve(page, rt), VIEW, &FixedAdvance).rects().to_vec()
}

fn draws(page: &Page, rt: &Runtime) -> Vec<Draw> {
    let kept = KeptFrame::fresh(page, resolve(page, rt), VIEW, &FixedAdvance);
    draw(page, kept.frame())
}

/// A node's own `when held` beats its style's `when held`, also when the
/// style sits in an included file on a later line number than the node.
#[test]
fn a_nodes_own_when_beats_its_styles_when() {
    // The style's `when` is on line 30 of its file; the node's on line 3.
    let theme = format!("{}style Big {{\n when held background=#FF0000FF\n}}\n", "\n".repeat(28));
    let p = page_with("uses \"styles/theme.gogo\"\nView v { Button b .Big text=\"x\", background=#000000FF {\n when held background=#00FF00FF\n} }", &theme);
    let b = node(&p, "b");
    let mut rt = Runtime::new(&p);
    rt.set_state(b, HELD, true);
    assert_eq!(resolve(&p, &rt).color(&p, b, PropKey::Background), Some(GREEN), "the node's own response shows");
}

/// Between two styles the one worn later wins, whichever was declared
/// later in the file: the order a reader sees on the node's line.
#[test]
fn the_style_worn_later_wins_between_two_styles() {
    let p = page("style B {\n when held background=#0000FFFF\n}\nstyle A {\n when held background=#FF0000FF\n}\nView v { Button b .A .B text=\"x\", background=#000000FF }");
    let b = node(&p, "b");
    let mut rt = Runtime::new(&p);
    rt.set_state(b, HELD, true);
    assert_eq!(resolve(&p, &rt).color(&p, b, PropKey::Background), Some(BLUE), "`.B` is worn after `.A`");
}

/// The states still rank as before: `disabled` over `held` whatever the
/// order of the lines, and between a node's own two blocks of one rank
/// the later line.
#[test]
fn state_rank_and_line_order_still_decide_the_rest() {
    let p = page("arrangement one\narrangement two\nstyle S {\n when disabled background=#FF0000FF\n}\nView v { Button b .S text=\"x\", background=#000000FF {\n when held background=#00FF00FF\n}\n Panel q width=10, height=10, background=#000000FF }\nwhen one { q.background = #FF0000FF }\nwhen screen.landscape { q.background = #0000FFFF }");
    let (b, q) = (node(&p, "b"), node(&p, "q"));
    let mut rt = Runtime::new(&p);
    rt.set_state(b, HELD, true);
    rt.set_state(b, DISABLED, true);
    assert_eq!(resolve(&p, &rt).color(&p, b, PropKey::Background), Some(RED), "the style's `disabled` outranks the node's own `held`");
    rt.go_id(gogo_ui::page::ArrangementId::from_index(0));
    rt.set_screen(&p, gogo_ui::page::Orientation::Landscape, gogo_ui::page::ScreenClass::LandscapeSmall);
    assert_eq!(resolve(&p, &rt).color(&p, q, PropKey::Background), Some(BLUE), "equal rank: the later line");
}

/// `theme { font_color = … }` is the page's text color.
#[test]
fn a_theme_block_sets_the_pages_text_color() {
    let p = page("theme {\n font_color = #FF0000FF\n}\nView v { Label a text=\"hi\"\n Button b text=\"go\" }");
    let rt = Runtime::new(&p);
    let look = resolve(&p, &rt);
    assert_eq!(look.font_color(&p, node(&p, "a")), Some(RED), "a Label");
    assert_eq!(look.font_color(&p, node(&p, "b")), Some(RED), "a Button's text");
    let inks: Vec<Color_Straight_Srgb_4xU8_RGBA> = draws(&p, &rt).iter().filter_map(|d| if let Draw::Text { color, .. } = d { Some(*color) } else { None }).collect();
    assert_eq!(inks, vec![RED, RED], "both texts draw red");
}

/// `color` in a `theme` block is refused and the message names
/// `font_color`: text color has one word.
#[test]
fn color_in_a_theme_block_is_refused_naming_font_color() {
    let src = "theme {\n color = #FF0000FF\n}\nView v { Label a text=\"hi\" }";
    assert!(codes(src).contains(&"G3006".to_string()), "{}", messages(src));
    assert!(messages(src).contains("font_color"), "{}", messages(src));
}

/// A style on a `widget` line is refused: the line declares a name and
/// its parameters, it is not a node.
#[test]
fn a_style_on_a_widget_line_is_refused() {
    for word in ["widget", "control"] {
        let src = format!("style Big font_size=30\n{word} W .Big {{\n Panel width=10, height=10\n}}\nView v {{ W w }}");
        assert!(codes(&src).contains(&"G3006".to_string()), "{word}: {}", messages(&src));
        assert!(messages(&src).contains("first node"), "{word}: the message says where a style goes: {}", messages(&src));
    }
    // The two places that exist: the first node of the body, the instance line.
    let p = page("style Big corner_radius=7\nwidget W {\n Panel .Big width=10, height=10\n}\nwidget X {\n Panel width=10, height=10\n}\nView v { W w\n X x .Big }");
    let rt = Runtime::new(&p);
    let look = resolve(&p, &rt);
    assert_eq!(look.num(&p, node(&p, "w"), PropKey::CornerRadius), Some(7.0), "on the first node of the body");
    assert_eq!(look.num(&p, node(&p, "x"), PropKey::CornerRadius), Some(7.0), "on the instance line");
}

/// `theme { accent = … }` reaches the marks gogo draws itself: a Check's
/// box, a Radio's dot and a Toggle's pill.
#[test]
fn the_pages_accent_reaches_the_built_in_marks() {
    let p = page("theme {\n accent = #FF00FFFF\n}\nView v { Column {\n Check c text=\"a\", selected=true\n choice size value=m { Radio m text=\"b\" }\n Toggle t value=1\n} }");
    let rt = Runtime::new(&p);
    let purple = Color_Straight_Srgb_4xU8_RGBA([255, 0, 255, 255]);
    let list = draws(&p, &rt);
    let quads: Vec<Color_Straight_Srgb_4xU8_RGBA> = list.iter().filter_map(|d| if let Draw::Quad { color, .. } = d { Some(*color) } else { None }).collect();
    let dots: Vec<Color_Straight_Srgb_4xU8_RGBA> = list.iter().filter_map(|d| if let Draw::Circle { color, .. } = d { Some(*color) } else { None }).collect();
    assert_eq!(quads.iter().filter(|c| **c == purple).count(), 2, "the Check's box and the Toggle's pill: {quads:?}");
    assert!(dots.contains(&purple), "the Radio's dot: {dots:?}");
    assert!(p.find("c").is_some() && rt.has(node(&p, "c"), SELECTED));
}

/// `Button padding=0` is as big as its text; a Button with no padding
/// written keeps the 12/6 air around its label.
#[test]
fn a_written_zero_padding_is_zero() {
    let p = page("View v { Column {\n Button tight text=\"OK\", padding=0, halign=left\n Button plain text=\"OK\", halign=left\n} }");
    let rt = Runtime::new(&p);
    let r = rects(&p, &rt);
    let (tight, plain) = (r[node(&p, "tight").index()], r[node(&p, "plain").index()]);
    assert_eq!(plain.w - tight.w, 24.0, "12 left and 12 right: tight {tight:?}, plain {plain:?}");
    assert_eq!(plain.h - tight.h, 12.0, "6 top and 6 bottom");
}

/// The page's theme default wins over an included file's, and within one
/// file the later block wins: the rule styles and widgets follow.
#[test]
fn the_later_theme_default_wins() {
    let p = page_with("uses \"styles/theme.gogo\"\ntheme {\n font_size = 30\n}\nView v { Label a text=\"hi\" }", "theme {\n font_size = 20\n}\n");
    assert_eq!(resolve(&p, &Runtime::new(&p)).font_size(&p, node(&p, "a")), Some(30.0), "the page over the file it includes");
    let p = page("theme {\n font_size = 20\n}\ntheme {\n font_size = 30\n}\nView v { Label a text=\"hi\" }");
    assert_eq!(resolve(&p, &Runtime::new(&p)).font_size(&p, node(&p, "a")), Some(30.0), "the later block of one file");
}

/// A timed rule slides a `font_color` the node never wrote: it starts
/// from the ink the text shows.
#[test]
fn an_unwritten_font_color_slides_under_a_timed_rule() {
    let mut p = page("View v { Button b text=\"x\" {\n transition any -> held duration=1, easing=linear\n when held font_color=#000000FF\n} }");
    let b = node(&p, "b");
    let mut rt = Runtime::new(&p);
    // No theme and no `font_color` written: the text shows gogo's own ink.
    let rest = Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255]);
    assert_eq!(resolve_animated(&p, &mut rt).font_color(&p, b), None);
    rt.set_state(b, HELD, true);
    resolve_animated(&p, &mut rt);
    rt.tick(&mut p, 0.5);
    let mid = resolve_animated(&p, &mut rt).font_color(&p, b).unwrap();
    assert_ne!(mid, rest, "half way it has left the resting ink");
    assert_ne!(mid, Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255]), "and has not arrived yet: {mid:?}");
}

/// A code write of `disabled` sets the disabled state, as `set_state`
/// does, and clears it again.
#[test]
fn a_code_write_of_disabled_sets_the_state() {
    let p = page("View v { Button b text=\"x\" }");
    let b = node(&p, "b");
    let mut rt = Runtime::new(&p);
    rt.set_prop(b, PropKey::Disabled, Value::VBool(true));
    assert!(rt.has(b, DISABLED), "the write disables the button");
    rt.set_prop(b, PropKey::Disabled, Value::VBool(false));
    assert!(!rt.has(b, DISABLED), "and enables it again");
}

/// A code write of `selected` sets the selected state, as `set_state`
/// does: a Check is ticked and cleared, and in a `choice` the Radio
/// picked before lets go.
#[test]
fn a_code_write_of_selected_sets_the_state() {
    let p = page("View v {\n Check agree text=\"I agree\"\n choice size value=small {\n Radio small text=\"Small\"\n Radio large text=\"Large\"\n }\n}");
    let (agree, small, large) = (node(&p, "agree"), node(&p, "small"), node(&p, "large"));
    let mut rt = Runtime::new(&p);
    assert!(rt.has(small, SELECTED) && !rt.has(large, SELECTED), "the file's pick");
    rt.set_prop(agree, PropKey::SelectedProp, Value::VBool(true));
    assert!(rt.has(agree, SELECTED), "the write ticks the Check");
    rt.set_prop(agree, PropKey::SelectedProp, Value::VBool(false));
    assert!(!rt.has(agree, SELECTED), "and clears it again");
    rt.set_prop(large, PropKey::SelectedProp, Value::VBool(true));
    assert!(rt.has(large, SELECTED), "the write picks the Radio");
    assert!(!rt.has(small, SELECTED), "and the Radio picked before lets go");
}
