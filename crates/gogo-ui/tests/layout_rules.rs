//! Layout: where a box lands and how big it is, and the size lines the
//! build refuses. Tiles and their `cells=`, a wrapping label's smallest
//! width, a revealed box, two nodes after one seat, margins and floors
//! and caps beside a size, a capped label, a Scroll that cannot pan, cell
//! words in a style, and a Button's padding.

use gogo_ui::layout::{FixedAdvance, Rect, layout};
use gogo_ui::runtime::{Resolved, Runtime, resolve_animated};
use gogo_ui::{ArrangementId, Draw, KeptFrame, NodeId, Orientation, Page, ScreenClass, Severity, compile, draw, hit};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
/// What `FixedAdvance` answers at the default font size (15): the width
/// of one letter and the height of one line.
const LETTER: f32 = 8.25;
const LINE: f32 = 18.75;
/// Eight four-letter words: 39 letters wide on one line.
const LONG: &str = "aaaa bbbb cccc dddd eeee ffff gggg hhhh";

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn errors(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.clone()).collect()
}

fn warnings(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.code.clone()).collect()
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{}: {}\n", d.code, d.message)).collect()
}

fn page(src: &str) -> Page {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{src}\n{:#?}", c.diags);
    c.page
}

/// One page with its runtime, laid out in a 400 x 800 window.
struct Fx {
    page: Page,
    rt: Runtime,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let page = page(src);
        let rt = Runtime::new(&page);
        Fx { page, rt }
    }

    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node `{name}`"))
    }

    fn kept(&mut self) -> KeptFrame {
        let look = resolve_animated(&self.page, &mut self.rt);
        KeptFrame::fresh(&self.page, look, VIEW, &FixedAdvance)
    }

    fn rect(&mut self, name: &str) -> Rect {
        let id = self.id(name);
        self.kept().rects()[id.index()]
    }

    fn go(&mut self, arrangement: &str) {
        let at = self.page.arrangements.iter().position(|a| self.page.str(*a) == arrangement).unwrap_or_else(|| panic!("no arrangement `{arrangement}`"));
        self.rt.go_instant_id(ArrangementId::from_index(at));
    }

    /// The node under a point, counting only what takes input.
    fn under(&mut self, x: f32, y: f32) -> Option<NodeId> {
        let kept = self.kept();
        hit(&self.page, kept.frame(), x, y, false)
    }
}

fn rect(src: &str, name: &str) -> Rect {
    Fx::new(src).rect(name)
}

// ----- Tiles and `cells=` ---------------------------------------------------

/// `t.cells = natural` in a `when`: the layout kept between frames packs
/// the cells again, as a layout from scratch does.
#[test]
fn cells_written_in_a_when_packs_the_tiles_again() {
    let mut fx = Fx::new("arrangement loose\nwhen loose { t.cells = natural }\nView v {\n Tiles t cols=2, width=200, halign=left, valign=top {\n Label a text=\"ab\"\n Label b text=\"cd\"\n }\n}");
    let b = fx.id("b");
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    let before: Vec<Rect> = fx.rt.layout_retained(&fx.page, VIEW, &FixedAdvance).to_vec();
    assert_eq!(before[b.index()].x, 100.0, "two columns of 100");
    fx.go("loose");
    let look: Resolved = resolve_animated(&fx.page, &mut fx.rt);
    let kept: Vec<Rect> = fx.rt.layout_retained(&fx.page, VIEW, &FixedAdvance).to_vec();
    let fresh: Vec<Rect> = layout(&fx.page, &look, VIEW, &FixedAdvance);
    assert_eq!(fresh[b.index()].x, 2.0 * LETTER, "from scratch: `cd` starts where `ab` ends");
    assert_eq!(kept[b.index()], fresh[b.index()], "the kept layout agrees");
}

/// `Tiles spacing=8`: 8 between the cells of a row and between the rows.
#[test]
fn spacing_on_tiles_puts_gutters_between_cells() {
    let mut fx = Fx::new("View v {\n Tiles t cols=2, spacing=8, width=208, halign=left, valign=top {\n Rectangle a height=10\n Rectangle b height=10\n Rectangle c height=10\n Rectangle d height=10, span=2\n }\n}");
    assert_eq!(fx.rect("a"), Rect::new(0.0, 0.0, 100.0, 10.0));
    assert_eq!(fx.rect("b"), Rect::new(108.0, 0.0, 100.0, 10.0));
    assert_eq!(fx.rect("c"), Rect::new(0.0, 18.0, 100.0, 10.0));
    assert_eq!(fx.rect("d"), Rect::new(0.0, 36.0, 208.0, 10.0), "a cell of two columns covers the gutter between them");
    assert_eq!(fx.rect("t").h, 46.0, "three rows and two gutters");
}

// ----- a wrapping label's smallest width ------------------------------------

/// A wrapping label that fills what is left of a tight Row is never
/// narrower than its longest word.
#[test]
fn a_wrapping_fill_label_keeps_room_for_its_longest_word() {
    let r = rect("View v {\n Row r width=60, halign=left, valign=top {\n Rectangle k width=50, height=10\n Label t text=\"hello wonderful world\", wrap=true, width=fill\n }\n}", "t");
    assert_eq!(r.w, 9.0 * LETTER, "`wonderful` is 9 letters");
    assert_eq!(r.h, 3.0 * LINE, "one word per line at that width");
}

/// In a box with a written width the same label takes that width, as any
/// box that fills a written box does, and its text folds there: a text
/// box dragged narrower than a word.
#[test]
fn a_wrapping_fill_label_takes_a_written_width_narrower_than_a_word() {
    let mut fx = Fx::new("View v {\n Layers box width=20, height=100, halign=left, valign=top {\n Label t text=\"wide\", wrap=true, width=fill, height=hug, valign=center\n }\n}");
    assert_eq!(fx.rect("t").w, 20.0);
    // With no written width above it the label keeps its word.
    let mut fx = Fx::new("View v {\n Layers box halign=left, valign=top {\n Rectangle k width=20, height=10\n Label t text=\"wide\", wrap=true, width=fill\n }\n}");
    assert_eq!(fx.rect("t").w, 4.0 * LETTER);
}

// ----- a revealed box -------------------------------------------------------

/// `reveal_y=0`: the box is 0 tall, its content is cut there and takes
/// no touch.
#[test]
fn a_box_revealed_to_nothing_shows_and_takes_nothing() {
    let mut fx = Fx::new("View v {\n Panel p halign=left, valign=top, reveal_y=0 {\n Button b text=\"Go\", width=100, height=40\n }\n}");
    let (p, b) = (fx.id("p"), fx.id("b"));
    let kept = fx.kept();
    let shown: Rect = kept.rects()[p.index()];
    assert_eq!(shown.h, 0.0);
    let draws: Vec<Draw> = draw(&fx.page, kept.frame());
    let cut: Option<Rect> = draws.iter().find_map(|d| if let Draw::ClipPush(r) = d { Some(*r) } else { None });
    assert_eq!(cut, Some(shown), "the Button is drawn inside a cut of no height");
    assert_ne!(fx.under(50.0, 20.0), Some(b), "and a tap where it would be does not reach it");
}

/// `reveal_y=0.5`: the content shows and takes touches in the shown half
/// only.
#[test]
fn a_half_revealed_box_cuts_its_content_at_the_part_it_shows() {
    let mut fx = Fx::new("View v {\n Panel p halign=left, valign=top, reveal_y=0.5 {\n Button b text=\"Go\", width=100, height=90\n }\n}");
    let b = fx.id("b");
    assert_eq!(fx.rect("p"), Rect::new(0.0, 0.0, 100.0, 45.0));
    assert_eq!(fx.under(50.0, 25.0), Some(b), "inside the shown half");
    assert_ne!(fx.under(50.0, 70.0), Some(b), "below it");
    let kept = fx.kept();
    let cut: Option<Rect> = draw(&fx.page, kept.frame()).iter().find_map(|d| if let Draw::ClipPush(r) = d { Some(*r) } else { None });
    assert_eq!(cut, Some(Rect::new(0.0, 0.0, 100.0, 45.0)), "the Button is drawn inside the shown half");
}

/// A box at rest (no reveal, or a fraction of 1) and a bare bar cut
/// nothing.
#[test]
fn a_box_at_rest_and_a_bare_bar_cut_nothing() {
    for words in ["", ", reveal_y=1"] {
        let mut fx = Fx::new(&format!("View v {{\n Layers p halign=left, valign=top{words} {{\n Button b text=\"Go\", halign=left, valign=top\n }}\n}}"));
        let kept = fx.kept();
        assert!(!draw(&fx.page, kept.frame()).iter().any(|d| matches!(d, Draw::ClipPush(_))), "`{words}`");
    }
    let mut fx = Fx::new("View v {\n Rectangle bar width=fill, height=4, valign=top, color=#FF0000FF, reveal_x=0.3\n}");
    let kept = fx.kept();
    assert!(!draw(&fx.page, kept.frame()).iter().any(|d| matches!(d, Draw::ClipPush(_))));
}

/// The shown part starts at the edge the box is aligned to, whatever its
/// size word.
#[test]
fn a_box_shows_its_fraction_from_the_edge_its_alignment_names() {
    let at = |words: &str| rect(&format!("View v {{\n Rectangle r {words}\n}}"), "r");
    assert_eq!(at("width=100, height=40, valign=top, halign=left, reveal_x=0.5"), Rect::new(0.0, 0.0, 50.0, 40.0));
    assert_eq!(at("width=100, height=40, valign=top, halign=right, reveal_x=0.5"), Rect::new(350.0, 0.0, 50.0, 40.0));
    assert_eq!(at("width=100, height=40, valign=top, halign=center, reveal_x=0.5"), Rect::new(175.0, 0.0, 50.0, 40.0));
    assert_eq!(at("width=100, height=40, halign=left, valign=bottom, reveal_y=0.25"), Rect::new(0.0, 790.0, 100.0, 10.0));
    assert_eq!(at("width=fill, height=40, valign=top, halign=right, reveal_x=0.25"), Rect::new(300.0, 0.0, 100.0, 40.0));
    assert_eq!(at("width=fill, height=40, valign=top, reveal_x=0.25"), Rect::new(0.0, 0.0, 100.0, 40.0), "the slider's fill bar");
}

/// What a revealed box holds keeps its size and travels with the box: it
/// is laid out from the corner where the shown part starts, whatever the
/// box's size word. A sheet aligned to the bottom shows the top of its
/// content and carries it out through the bottom.
#[test]
fn the_content_of_a_revealed_box_travels_with_it() {
    for sheet_words in ["valign=bottom, halign=left", "width=100, height=200, valign=bottom, halign=left"] {
        let mut fx = Fx::new(&format!("View v {{\n Panel sheet {sheet_words}, reveal_y=0.5 {{\n Rectangle top width=100, height=200, valign=top\n }}\n}}"));
        assert_eq!(fx.rect("sheet"), Rect::new(0.0, 700.0, 100.0, 100.0), "`{sheet_words}`: the lower half of the sheet's place");
        assert_eq!(fx.rect("top"), Rect::new(0.0, 700.0, 100.0, 200.0), "`{sheet_words}`: the content starts where the shown part starts");
    }
    // Aligned to the right: the content starts at the shown part's left.
    let mut fx = Fx::new("View v {\n Panel p width=200, height=40, halign=right, valign=top, reveal_x=0.25 {\n Rectangle k width=200, height=40, halign=left, valign=top\n }\n}");
    assert_eq!(fx.rect("p"), Rect::new(350.0, 0.0, 50.0, 40.0));
    assert_eq!(fx.rect("k"), Rect::new(350.0, 0.0, 200.0, 40.0));
    // In a Column the box gets a slot of its shown height; what fills the
    // box keeps the box's full height.
    let mut fx = Fx::new("View v {\n Column c halign=left, valign=top {\n Panel p width=100, height=200, reveal_y=0.5 {\n Rectangle body width=fill, height=fill\n }\n Rectangle next width=100, height=10\n }\n}");
    assert_eq!(fx.rect("p"), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert_eq!(fx.rect("body"), Rect::new(0.0, 0.0, 100.0, 200.0));
    assert_eq!(fx.rect("next").y, 100.0, "the sibling closes up under the shown part");
}

/// The layout kept between frames moves the content with a box whose
/// fraction a `when` changes, as a layout from scratch does.
#[test]
fn the_kept_layout_moves_the_content_of_a_box_as_it_is_revealed() {
    let mut fx = Fx::new("arrangement open\nwhen open { sheet.reveal_y = 1 }\nView v {\n Panel sheet width=100, height=200, valign=bottom, halign=left, reveal_y=0.5 {\n Rectangle top width=100, height=50, valign=top\n }\n Rectangle other width=10, height=10, halign=right, valign=top\n}");
    let top = fx.id("top");
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    let before: Vec<Rect> = fx.rt.layout_retained(&fx.page, VIEW, &FixedAdvance).to_vec();
    assert_eq!(before[top.index()].y, 700.0);
    fx.go("open");
    let look: Resolved = resolve_animated(&fx.page, &mut fx.rt);
    let kept: Vec<Rect> = fx.rt.layout_retained(&fx.page, VIEW, &FixedAdvance).to_vec();
    let fresh: Vec<Rect> = layout(&fx.page, &look, VIEW, &FixedAdvance);
    assert_eq!(fresh[top.index()].y, 600.0, "fully shown: the content is at the top of the sheet's place");
    assert_eq!(kept, fresh, "the kept layout agrees, node for node");
}

// ----- two nodes after one seat ---------------------------------------------

const TWO_CLAIMS: &str = "View v {\n Row r halign=left, valign=top, height=20 {\n Column ha { Rectangle a 10x10 }\n Column hb { Rectangle b 10x10 }\n seat s\n }\n}";

/// An arrangement and a screen class both send a node to one seat. With
/// both on, the later line gets the seat and the other node stays where
/// its line is.
#[test]
fn the_later_line_gets_a_seat_two_nodes_are_sent_to() {
    for (first, second, winner, loser, home) in [("when open { a.seat = s }", "when screen.landscapeSmall { b.seat = s }", "b", "a", "ha"), ("when screen.landscapeSmall { b.seat = s }", "when open { a.seat = s }", "a", "b", "hb")] {
        let mut fx = Fx::new(&format!("arrangement open\n{first}\n{second}\n{TWO_CLAIMS}"));
        fx.go("open");
        fx.rt.set_screen(&fx.page, Orientation::Landscape, ScreenClass::LandscapeSmall);
        let (seat, win, lose, at_home) = (fx.id("s"), fx.id(winner), fx.id(loser), fx.id(home));
        let kept = fx.kept();
        assert_eq!(kept.look().placed_parent(&fx.page, win), seat, "`{second}` is the later line");
        assert_eq!(kept.look().placed_parent(&fx.page, lose), at_home, "`{loser}` stays in `{home}`");
    }
}

/// One of the two conditions alone seats its node, as before.
#[test]
fn one_claim_alone_takes_the_seat() {
    let mut fx = Fx::new(&format!("arrangement open\nwhen open {{ a.seat = s }}\nwhen screen.landscapeSmall {{ b.seat = s }}\n{TWO_CLAIMS}"));
    let (seat, a, b) = (fx.id("s"), fx.id("a"), fx.id("b"));
    fx.go("open");
    assert_eq!(fx.kept().look().placed_parent(&fx.page, a), seat);
    fx.rt.go_instant_id(gogo_ui::BASE);
    fx.rt.set_screen(&fx.page, Orientation::Landscape, ScreenClass::LandscapeSmall);
    assert_eq!(fx.kept().look().placed_parent(&fx.page, b), seat);
}

// ----- margins --------------------------------------------------------------

/// A margin on a Scroll's child insets it; the build takes it.
#[test]
fn a_margin_on_a_scrolls_child_insets_it() {
    let src = "View v {\n Scroll s width=200, height=300, halign=left, valign=top {\n Column c margin_left=8, margin_top=5 {\n Label a text=\"x\"\n }\n }\n}";
    assert_eq!(codes(src), Vec::<String>::new(), "{}", messages(src));
    let c = rect(src, "c");
    assert_eq!((c.x, c.y), (8.0, 5.0));
}

/// A margin on a child of a box that deals its children their places is
/// one build error, G1014.
#[test]
fn a_margin_under_a_box_that_places_its_children_is_one_error() {
    for parent in ["Column c", "Row c", "Flow c", "Tiles c cols=2"] {
        let src = format!("View v {{ {parent} {{ Label a text=\"x\", margin_top=3 }} }}");
        assert_eq!(codes(&src), ["G1014"], "{parent}: {}", messages(&src));
    }
    let grid = "View v { Grid g cols=[fill] { row { Label a text=\"x\", margin_top=3 } } }";
    assert_eq!(codes(grid), ["G1014"], "{}", messages(grid));
    let list = "View v {\n List n width=200, height=300 {\n empty { Label e text=\"none\" }\n repeat rows by $item.id {\n ListItem margin_top=3 { Label a text=$item.title }\n }\n }\n}";
    assert_eq!(codes(list), ["G1014"], "{}", messages(list));
}

/// `width=fill, max_width=380, halign=center, margin_left=100` in a
/// window 400 wide: the box starts at its margin.
#[test]
fn a_lone_margin_on_a_centered_capped_fill_is_where_the_box_starts() {
    let r = rect("View v {\n Rectangle r width=fill, max_width=380, halign=center, margin_left=100, height=10, valign=top\n}", "r");
    assert_eq!((r.x, r.w), (100.0, 300.0));
    let both = rect("View v {\n Rectangle r width=fill, max_width=380, halign=center, margin_left=100, margin_right=0, height=10, valign=top\n}", "r");
    assert_eq!((both.x, both.w), (100.0, 300.0), "as with the other margin written 0");
}

/// One margin beside `halign=center` or `valign=center`: the box is
/// centered in the room the margin leaves.
#[test]
fn one_margin_beside_a_centered_box_counts() {
    let at = |words: &str| rect(&format!("View v {{\n Panel p width=360, height=200, halign=left, valign=top {{\n Rectangle r width=100, height=10, {words}\n }}\n}}"), "r");
    assert_eq!(at("halign=center, valign=top").x, 130.0);
    assert_eq!(at("halign=center, valign=top, margin_left=20").x, 140.0);
    assert_eq!(at("halign=center, valign=top, margin_right=20").x, 120.0);
    assert_eq!(at("valign=center, halign=left, margin_top=20").y, 105.0);
    assert_eq!(at("valign=center, halign=left, margin_bottom=20").y, 85.0);
}

/// An Image with both sizes written is centered in a Panel with no
/// alignment word; a margin shifts it as it shifts a box that says
/// `halign=center`.
#[test]
fn a_margin_shifts_an_image_centered_by_default() {
    let at = |words: &str| rect(&format!("View v {{\n Panel p width=360, height=200, halign=left, valign=top {{\n Image i width=100, height=80{words}\n }}\n}}"), "i");
    assert_eq!(at(""), Rect::new(130.0, 60.0, 100.0, 80.0));
    assert_eq!(at(", margin_left=20"), Rect::new(140.0, 60.0, 100.0, 80.0));
    assert_eq!(at(", halign=left, margin_left=20"), Rect::new(20.0, 60.0, 100.0, 80.0), "the picture 20 from the left");
}

// ----- floors and caps beside a size ----------------------------------------

/// A floor or a cap beside an exact size for the same side is a build
/// error, and the cap's message shows the form that works.
#[test]
fn a_floor_or_a_cap_beside_an_exact_size_is_refused() {
    for line in ["width=100, min_width=200", "height=100, min_height=200", "size=100x50, min_height=20", "width=100, min_size=44", "width=100, max_width=40", "height=100, max_height=40", "width=100, max_width=fill"] {
        let src = format!("View v {{ Rectangle r {line} }}");
        assert_eq!(codes(&src), ["G1019"], "{line}: {}", messages(&src));
    }
    assert!(messages("View v { Rectangle r width=100, max_width=40 }").contains("`width=fill, max_width=40`"), "{}", messages("View v { Rectangle r width=100, max_width=40 }"));
    assert!(messages("View v { Rectangle r height=100, max_height=40 }").contains("`height=fill, max_height=40`"));
    // The other side's floor and cap say something.
    for line in ["width=100, min_height=20", "height=100, max_width=40", "width=fill, min_width=50, max_width=90"] {
        let src = format!("View v {{ Rectangle r {line} }}");
        assert_eq!(codes(&src), Vec::<String>::new(), "{line}: {}", messages(&src));
    }
}

/// The pair is refused on ONE line. A floor that a control's template
/// carries stands beside the size its instance writes, and a floor or a
/// cap beside a size that a `when` poses bounds what the `when` asks for;
/// the floor holds in a stack as in a free parent.
#[test]
fn a_floor_from_another_line_or_under_a_posed_size_builds_and_holds() {
    let knob = "control Knob $value=0, min_size=48x48 {\n Circle color=#333333FF\n}\n";
    for (parent, at) in [("Row r halign=left, valign=top", "20x20"), ("Layers r", "20x20, halign=left, valign=top")] {
        let src = format!("{knob}View v {{\n {parent} {{\n Knob k {at}\n }}\n}}");
        assert_eq!(codes(&src), Vec::<String>::new(), "{parent}: {}", messages(&src));
        let k = rect(&src, "k");
        assert_eq!((k.w, k.h), (48.0, 48.0), "{parent}: the template's floor holds under the instance's 20x20");
    }
    let posed = "arrangement thin\nwhen thin { p.width = 10 }\nView v {\n Row r halign=left, valign=top {\n Rectangle p width=50, min_width=30, height=20\n }\n}";
    assert_eq!(codes(posed), Vec::<String>::new(), "{}", messages(posed));
    let mut fx = Fx::new(posed);
    assert_eq!(fx.rect("p").w, 50.0);
    fx.go("thin");
    assert_eq!(fx.rect("p").w, 30.0, "the floor bounds the posed width in a Row too");
    let capped = "arrangement wide\nwhen wide { p.width = fill }\nView v {\n Rectangle p width=50, max_width=300, height=20, halign=left, valign=top\n}";
    assert_eq!(codes(capped), Vec::<String>::new(), "{}", messages(capped));
    let mut fx = Fx::new(capped);
    fx.go("wide");
    assert_eq!(fx.rect("p").w, 300.0, "the cap bounds the posed `fill`");
    // A style is one line too.
    let style = "style Wide width=100, min_width=50\nView v { Rectangle r .Wide }";
    assert_eq!(codes(style), ["G1019"], "{}", messages(style));
}

/// `min_size=x36` leaves a `min_width` written before it alone, and
/// `min_size=44` sets both sides.
#[test]
fn min_size_writes_the_sides_it_names() {
    let r = rect("View v { Rectangle r min_width=50, min_size=x36, halign=left, valign=top }", "r");
    assert_eq!((r.w, r.h), (50.0, 36.0));
    let r = rect("View v { Rectangle r min_size=44, halign=left, valign=top }", "r");
    assert_eq!((r.w, r.h), (44.0, 44.0));
    let r = rect("View v { Rectangle r min_size=30x20, halign=left, valign=top }", "r");
    assert_eq!((r.w, r.h), (30.0, 20.0));
}

/// The checks of one side exist for the other: a floor above its cap,
/// two margins beside an exact size, a floor on the side `aspect=`
/// works out.
#[test]
fn size_contradictions_are_refused_on_both_sides() {
    for (line, code) in [
        ("min_width=300, max_width=100", "G1013"),
        ("min_height=300, max_height=100", "G1013"),
        ("width=40, margin_left=5, margin_right=5", "G1006"),
        ("height=40, margin_top=5, margin_bottom=5", "G1006"),
        ("width=100, aspect=2, min_height=80", "G1017"),
        ("height=100, aspect=2, min_width=80", "G1017"),
    ] {
        let src = format!("View v {{ Rectangle r {line} }}");
        assert_eq!(codes(&src), [code], "{line}: {}", messages(&src));
    }
    // A floor on the side that drives the ratio holds.
    let src = "View v { Rectangle r width=fill, aspect=2, min_width=80 }";
    assert_eq!(codes(src), Vec::<String>::new(), "{}", messages(src));
}

// ----- a capped label -------------------------------------------------------

/// A wrapping label with `max_width=100` and no width word in a Row that
/// hugs it: the Row is as tall as the wrapped label.
#[test]
fn a_row_is_as_tall_as_the_capped_label_it_hugs() {
    let mut fx = Fx::new(&format!("View v {{\n Row r halign=left, valign=top {{\n Label t text=\"{LONG}\", wrap=true, max_width=100\n }}\n}}"));
    let (r, t) = (fx.rect("r"), fx.rect("t"));
    assert_eq!(t.h, 4.0 * LINE, "two words to a line");
    assert_eq!(r.h, t.h);
}

/// The same in a Panel that hugs the label.
#[test]
fn a_panel_is_as_tall_as_the_capped_label_it_hugs() {
    let mut fx = Fx::new(&format!("View v {{\n Panel p halign=left, valign=top {{\n Label t text=\"{LONG}\", wrap=true, max_width=100\n }}\n}}"));
    let (p, t) = (fx.rect("p"), fx.rect("t"));
    assert_eq!(t.h, 4.0 * LINE);
    assert_eq!(p.h, t.h);
}

/// A cap that wraps a hugging label makes the label as wide as the cap,
/// a number as `fill`, and the Row's label with no width word is the same
/// box as one that says `width=hug`.
#[test]
fn a_cap_binds_a_hugging_label_the_same_however_it_is_written() {
    let number = rect(&format!("View v {{\n Label t text=\"{LONG}\", wrap=true, width=hug, max_width=100, halign=left, valign=top\n}}"), "t");
    assert_eq!((number.w, number.h), (100.0, 4.0 * LINE));
    let fill = rect(&format!("View v {{\n Panel p width=100, halign=left, valign=top {{\n Label t text=\"{LONG}\", wrap=true, width=hug, max_width=fill, halign=left, valign=top\n }}\n}}"), "t");
    assert_eq!((fill.w, fill.h), (100.0, 4.0 * LINE));
    let in_row = |words: &str| rect(&format!("View v {{\n Row r halign=left, valign=top {{\n Label t text=\"{LONG}\", wrap=true, max_width=100{words}\n }}\n}}"), "t");
    assert_eq!(in_row(""), in_row(", width=hug"));
    // A text shorter than the cap stays as wide as its line.
    let short = rect("View v {\n Label t text=\"aaaa\", wrap=true, width=hug, max_width=100, halign=left, valign=top\n}", "t");
    assert_eq!((short.w, short.h), (4.0 * LETTER, LINE));
}

// ----- a Scroll that cannot pan ---------------------------------------------

const BODY: &str = "{ Column c { Label a text=\"x\" } }";

/// A Scroll that comes out with no length on a side it pans, or that can
/// never reach what its cap cuts, is a build error.
#[test]
fn a_scroll_that_cannot_pan_or_has_no_length_is_refused() {
    for words in ["allow=vertical, height=hug", "allow=horizontal, width=hug", "allow=both, width=hug", "allow=horizontal, height=hug, max_height=300", "allow=vertical, width=hug, max_width=300", "height=hug, max_height=fill"] {
        let src = format!("View v {{ Scroll s {words} {BODY} }}");
        assert_eq!(errors(&src), ["G1020"], "{words}: {}", messages(&src));
    }
}

/// A Scroll that hugs both sides scrolls nothing: a warning.
#[test]
fn a_scroll_that_hugs_both_sides_gets_a_warning() {
    let src = format!("View v {{ Scroll s width=hug, height=hug {BODY} }}");
    assert_eq!(errors(&src), Vec::<String>::new(), "{}", messages(&src));
    assert_eq!(warnings(&src), ["G1021"], "{}", messages(&src));
}

/// The Scrolls that work build with no message.
#[test]
fn the_scrolls_that_work_build_clean() {
    for words in ["", "allow=vertical", "allow=horizontal", "allow=both", "height=hug, max_height=300", "width=hug, height=100", "allow=vertical, height=hug, max_height=300", "width=200, height=300"] {
        let src = format!("View v {{ Scroll s {words} {BODY} }}");
        assert_eq!(codes(&src), Vec::<String>::new(), "{words}: {}", messages(&src));
    }
}

// ----- cell words, and words that did nothing -------------------------------

/// Where a cell sits is written on the cell's line: a cell word in a
/// style is a build error.
#[test]
fn a_cell_word_in_a_style_is_refused() {
    for (style, view) in [
        ("col_span=2", "Grid g cols=[fill, fill] { row { Rectangle a .Wide; Rectangle b } }"),
        ("row_span=2", "Grid g cols=[fill, fill] { row { Rectangle a .Wide; Rectangle b } }"),
        ("span=5", "Tiles t cols=2 { Rectangle a .Wide }"),
        ("full_row=true", "Tiles t cols=2 { Rectangle a .Wide }"),
    ] {
        let src = format!("style Wide {style}\nView v {{ {view} }}");
        assert!(errors(&src).contains(&"G1002".to_string()), "{style}: {}", messages(&src));
        let word: &str = style.split('=').next().unwrap();
        assert!(messages(&src).contains(&format!("`{word}=`")), "{style}: {}", messages(&src));
    }
}

/// A Button has no `icon=` and no `icon_size=`: a Button with a picture
/// holds an Image.
#[test]
fn a_button_takes_no_icon_words() {
    for word in ["icon=\"res://a.png\"", "icon_size=24"] {
        let src = format!("View v {{ Button b text=\"x\", {word} }}");
        assert_eq!(errors(&src), ["G2001"], "{word}: {}", messages(&src));
    }
}

/// An anim lane on a gradient stop the Gradient does not have is a build
/// error, as a `when` on it is.
#[test]
fn an_anim_lane_on_a_gradient_stop_that_is_not_there_is_refused() {
    let view = "View v { Gradient g stops=[white, black], width=10, height=10 }";
    let missing = format!("anim glow {{ g.stop_5_color to=#FF0000FF, duration=0.2 }}\n{view}");
    assert_eq!(errors(&missing), ["G2005"], "{}", messages(&missing));
    let offset = format!("anim glow {{ g.stop_3_offset to=0.5, duration=0.2 }}\n{view}");
    assert_eq!(errors(&offset), ["G2005"], "{}", messages(&offset));
    let there = format!("anim glow {{ g.stop_2_color to=#FF0000FF, duration=0.2 }}\n{view}");
    assert_eq!(errors(&there), Vec::<String>::new(), "{}", messages(&there));
}

/// `row_height=fill` on a Grid: its rows share the Grid's height.
#[test]
fn a_grids_row_height_takes_fill() {
    let src = "View v {\n Grid g cols=[fill], row_height=fill, width=50, height=100, halign=left, valign=top {\n row { Rectangle a }\n row { Rectangle b }\n }\n}";
    assert_eq!(codes(src), Vec::<String>::new(), "{}", messages(src));
    let mut fx = Fx::new(src);
    assert_eq!(fx.rect("a"), Rect::new(0.0, 0.0, 50.0, 50.0));
    assert_eq!(fx.rect("b"), Rect::new(0.0, 50.0, 50.0, 50.0));
    let weighted = "View v {\n Grid g cols=[fill], row_height=fill(2), width=50, height=100 {\n row { Rectangle a }\n }\n}";
    assert_eq!(codes(weighted), Vec::<String>::new(), "{}", messages(weighted));
}

// ----- a Button's padding ---------------------------------------------------

/// One padding side written on a text-only Button keeps the 12/6 air on
/// the other three, as the same side written from code does.
#[test]
fn one_written_padding_side_keeps_the_buttons_air_on_the_others() {
    let padding = |words: &str| -> [f32; 4] {
        let p = page(&format!("View v {{ Button b text=\"Go\", width=120, height=60{words} }}"));
        p.nodes[p.find("b").unwrap()].placement.padding
    };
    assert_eq!(padding(""), [12.0, 6.0, 12.0, 6.0]);
    assert_eq!(padding(", padding_left=9"), [9.0, 6.0, 12.0, 6.0]);
    assert_eq!(padding(", padding_top=0, padding_right=20"), [12.0, 0.0, 20.0, 6.0]);
    assert_eq!(padding(", padding=0"), [0.0; 4], "all four written");
    assert_eq!(padding(", padding=3, padding_left=9"), [9.0, 3.0, 3.0, 3.0]);
    // A Button that holds content spells its own padding.
    let p = page("View v { Button b padding_left=9 { Label t text=\"Go\" } }");
    assert_eq!(p.nodes[p.find("b").unwrap()].placement.padding, [9.0, 0.0, 0.0, 0.0]);
}
