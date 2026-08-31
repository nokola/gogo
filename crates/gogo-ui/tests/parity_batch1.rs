//! Pins for the first parity batch (docs/todo-gogo-parity.md GPAR11,
//! GPAR37, GPAR42, GPAR43, GPAR52): the built-in disabled dim is a
//! bottom-of-the-ladder `when`, a text-only Button's padding reaches the
//! draw pass, `scale` shrinks glyphs/corners/borders with the box, and a
//! player whose channels a newer one took keeps its clock and hands over
//! the true base.
use gogo::anim::Finish;
use gogo::draw::{Draw, draw, hit};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::DISABLED_OPACITY;
use gogo::runtime::{HELD, Runtime, resolve, resolve_animated};
use gogo::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn draws(page: &Page, rt: &Runtime) -> Vec<Draw> {
    let res = resolve(page, rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    draw(page, &res, &rects)
}

fn text_alpha(page: &Page, list: &[Draw], word: &str) -> u8 {
    list.iter()
        .find_map(|d| match d {
            Draw::Text { text, color, .. } if page.str(*text) == word => Some(color.0[3]),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {word:?} in {list:#?}"))
}

fn about(alpha: u8, fraction: f32) -> bool {
    (alpha as i32 - (255.0 * fraction).round() as i32).abs() <= 1
}

/// GPAR11: `disabled` dims like a built-in `when disabled opacity=0.3` at
/// the bottom of the ladder — a plain `opacity=0.5` does NOT cancel it
/// (the when replaces the value), an authored `when disabled opacity=1`
/// opts out, an unrelated `when held opacity=` changes nothing, and
/// `theme { disabled_opacity = … }` moves the default.
#[test]
fn disabled_dim_is_a_bottom_of_ladder_when() {
    let src = "View v {\n Column c {\n  Label a text=\"a\", opacity=0.5, disabled=true\n  Label b text=\"b\", disabled=true { when disabled { opacity = 1 } }\n  Label d text=\"d\", disabled=true { when held { opacity = 0.8 } }\n  Label e text=\"e\"\n }\n}\n";
    let c = ok(src);
    let list = draws(&c.page, &Runtime::new(&c.page));
    assert!(about(text_alpha(&c.page, &list, "a"), DISABLED_OPACITY), "a static opacity does not cancel the dim: {}", text_alpha(&c.page, &list, "a"));
    assert_eq!(text_alpha(&c.page, &list, "b"), 255, "an authored `when disabled opacity=1` opts out");
    assert!(about(text_alpha(&c.page, &list, "d"), DISABLED_OPACITY), "a `when held` is not a disabled look");
    assert_eq!(text_alpha(&c.page, &list, "e"), 255);
    let c = ok(&format!("theme {{ disabled_opacity = 0.6 }}\n{src}"));
    let list = draws(&c.page, &Runtime::new(&c.page));
    assert!(about(text_alpha(&c.page, &list, "a"), 0.6), "the theme knob moves the default: {}", text_alpha(&c.page, &list, "a"));
}

/// GPAR48 (cut 2026-08-31): a plain `opacity=1` on a node does NOT switch
/// off its style's `when held opacity=0.5` — that would be the GPAR11
/// accident one rung up (nothing on the node's line says `held`). Opting
/// out is written: the node's own `when held opacity=1` beats the style's
/// at equal rank; an unstyled neighbour is untouched.
#[test]
fn a_plain_prop_never_cancels_a_styles_when() {
    let src = "style Dim { when held opacity=0.5 }
View v {
 Column c {
  Label a .Dim text=\"a\", opacity=1
  Label b .Dim text=\"b\" { when held { opacity = 1 } }
  Label d .Dim text=\"d\"
  Label e text=\"e\"
 }
}
";
    let c = ok(src);
    let mut rt = Runtime::new(&c.page);
    for name in ["a", "b", "d", "e"] {
        rt.set_state(c.page.find(name).unwrap(), HELD, true);
    }
    let list = draws(&c.page, &rt);
    assert!(about(text_alpha(&c.page, &list, "a"), 0.5), "a plain `opacity=1` does not cancel the style's when: {}", text_alpha(&c.page, &list, "a"));
    assert_eq!(text_alpha(&c.page, &list, "b"), 255, "the written `when held opacity=1` opts out");
    assert!(about(text_alpha(&c.page, &list, "d"), 0.5), "the style's when applies to a bare wearer: {}", text_alpha(&c.page, &list, "d"));
    assert_eq!(text_alpha(&c.page, &list, "e"), 255, "an unstyled node is untouched");
    let list = draws(&c.page, &Runtime::new(&c.page));
    assert_eq!(text_alpha(&c.page, &list, "a"), 255, "at rest the static prop is the look");
}

/// GPAR42: the text-only Button's default padding (12/6) is one value for
/// layout AND draw — a left-aligned label sits 12 epx in, not flush; a
/// Button holding its own content gets no such air.
#[test]
fn text_only_button_padding_reaches_the_draw_pass() {
    let c = ok("View v {\n Column col halign=left, valign=top {\n  Button b text=\"Save\", width=200, height=40, horizontal_alignment=left, vertical_alignment=top\n  Button f width=200, height=40 { Label inner text=\"in\", horizontal_alignment=left }\n }\n}\n");
    let list = draws(&c.page, &Runtime::new(&c.page));
    let save = list.iter().find_map(|d| match d { Draw::Text { rect, text, .. } if c.page.str(*text) == "Save" => Some(*rect), _ => None }).unwrap();
    assert_eq!((save.x, save.y), (12.0, 6.0), "{save:?}");
    let inner = list.iter().find_map(|d| match d { Draw::Text { rect, text, .. } if c.page.str(*text) == "in" => Some(*rect), _ => None }).unwrap();
    assert_eq!(inner.x, 0.0, "a Button with content spells its own padding: {inner:?}");
}

/// GPAR52: a newer anim taking every channel of a hide anim must not lose
/// the hide — the older player keeps its clock and hides ON TIME; a
/// partly superseded player stops writing the channel it lost; and a
/// newer anim's `stop(restore)` lands on the value the FIRST play found.
#[test]
fn superseded_player_keeps_its_clock_and_hands_over_the_base() {
    let src = "View v {\n Rectangle a width=10, height=10, on:hide=play(fade)\n}\nanim fade { a.opacity from=1, to=0, duration=1 }\nanim blink { a.opacity from=0.5, to=0.5, duration=0.2 }\nanim slide {\n a.opacity from=1, to=0, duration=1\n and a.translate.x from=0, to=100, duration=1\n}\nanim hold { a.opacity from=0.8, to=0.8, duration=0.2 }\n";
    let c = ok(src);
    let mut page = c.page;
    let a = page.find("a").unwrap();
    let fade = page.intern_lookup("fade");
    let blink = page.intern_lookup("blink");
    let slide = page.intern_lookup("slide");
    let hold = page.intern_lookup("hold");

    // The deferred hide survives a takeover and lands on time.
    let mut rt = Runtime::new(&page);
    rt.show(&mut page, a, false);
    rt.tick(&mut page, 0.1);
    rt.play(&page, blink, Finish::Nothing);
    rt.tick(&mut page, 0.3);
    assert!(!resolve(&page, &rt).gone(&page, a), "blink done, fade still has 0.6 s to run");
    rt.tick(&mut page, 0.65);
    assert!(resolve(&page, &rt).gone(&page, a), "the hide still happened when the file said");

    // A partly superseded player stops writing the channel it lost.
    let mut rt = Runtime::new(&page);
    rt.play(&page, slide, Finish::Nothing);
    rt.tick(&mut page, 0.2);
    rt.play(&page, hold, Finish::Nothing);
    rt.tick(&mut page, 0.3);
    assert!((rt.motion[a as usize].opacity - 0.8).abs() < 1e-4, "slide no longer writes opacity: {}", rt.motion[a as usize].opacity);
    assert!((rt.motion[a as usize].translate[0] - 50.0).abs() < 1e-3, "slide still slides: {}", rt.motion[a as usize].translate[0]);

    // The taker inherits the true base: stop(restore) goes back to 1.0.
    let mut rt = Runtime::new(&page);
    rt.play(&page, fade, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    rt.play(&page, blink, Finish::Nothing);
    rt.tick(&mut page, 0.05);
    rt.stop(blink, true);
    assert_eq!(rt.motion[a as usize].opacity, 1.0, "restored to the pre-fade value, not fade's mid-flight 0.5");
}

/// GPAR37: a written ceiling is hard — a capped box keeps the cap and
/// CLIPS what pokes past it (a capped Label always cuts its text); an
/// uncapped hug box grows around its content and clips nothing.
#[test]
fn a_written_ceiling_clips_what_pokes_past_it() {
    let c = ok("View v {\n Layers z halign=left, valign=top {\n  Panel plate max_width=100, halign=left, valign=top { Label l text=\"a very long label that does not wrap at all\", font_size=10 }\n  Panel loose halign=left, valign=top, margin_top=100 { Rectangle r 300x10 }\n  Label cap text=\"also long text here\", font_size=10, max_width=40, halign=left, valign=top, margin_top=200\n }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let list = draw(page, &res, &rects);
    let plate = rects[page.find("plate").unwrap() as usize];
    let l = rects[page.find("l").unwrap() as usize];
    assert_eq!(plate.w, 100.0, "the cap holds: {plate:?}");
    assert!(l.w > 100.0, "the label keeps the width its text needs (text does not bend to a box): {l:?}");
    let clips: Vec<Rect> = list.iter().filter_map(|d| if let Draw::ClipPush(r) = d { Some(*r) } else { None }).collect();
    assert!(clips.contains(&plate), "the capped plate clips: {clips:?}");
    assert!(clips.contains(&rects[page.find("cap").unwrap() as usize]), "a capped Label cuts its text: {clips:?}");
    assert_eq!(clips.len(), 2, "the uncapped box clips nothing: {clips:?}");
    assert_eq!(rects[page.find("loose").unwrap() as usize].w, 300.0, "an unwritten size grows around its content");
}

/// GPAR40 (exact half): an exact-sized box cuts a child that pokes past it
/// — the human wrote the size. A child hanging out through a NEGATIVE
/// margin or marked `overlay=true` is left alone (that hang is written
/// too); a hug box grows and cuts nothing.
#[test]
fn an_exact_size_clips_but_a_written_overhang_stays() {
    let c = ok("View v {\n Layers z halign=left, valign=top {\n  Panel box width=100, height=40, halign=left, valign=top { Rectangle r 300x10 }\n  Panel hang width=100, height=40, halign=left, valign=top, margin_top=100 { Rectangle badge 20x20, halign=right, valign=top, margin_right=-10 }\n  Panel ov width=100, height=40, halign=left, valign=top, margin_top=200 { Rectangle o 300x10, overlay=true }\n  Panel grows halign=left, valign=top, margin_top=300 { Rectangle g 300x10 }\n }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let list = draw(page, &res, &rects);
    let clips: Vec<Rect> = list.iter().filter_map(|d| if let Draw::ClipPush(r) = d { Some(*r) } else { None }).collect();
    let rect = |n: &str| rects[page.find(n).unwrap() as usize];
    assert_eq!(rect("box").w, 100.0);
    assert!(clips.contains(&rect("box")), "an exact box cuts the poking child: {clips:?}");
    assert!(!clips.contains(&rect("hang")), "a negative-margin hang is written: {clips:?}");
    assert!(!clips.contains(&rect("ov")), "an overlay child is outside the accounting: {clips:?}");
    assert!(!clips.contains(&rect("grows")), "a hug box grows instead: {clips:?}");
    assert_eq!(rect("grows").w, 300.0);
    assert_eq!(clips.len(), 1, "{clips:?}");
}

/// A box that fills a WRITTEN box takes that box (decided 2026-08-30, the
/// toolbar regression): the author wrote 80, so the unwritten filler is 80
/// even when its child is 150 — and so is a filler of that filler. The
/// child itself keeps its size (the written box clips it). The floor still
/// holds where the room comes from nowhere the author wrote: a filler in a
/// free parent grows to its content; a hug box is sized by content, so its
/// filler is too; a `min_*` floor is a written number and still wins.
#[test]
fn a_filler_inside_a_written_box_takes_the_box() {
    let c = ok("View v {\n Layers box width=200, height=80, halign=left, valign=top { Layers filler { Layers deeper { Rectangle big width=100, height=150, halign=left, valign=top } } }\n Panel hug width=200, height=hug, halign=left, valign=top, margin_top=300 { Layers hfill { Rectangle big3 width=100, height=150, halign=left, valign=top } }\n Layers box2 width=200, height=80, halign=left, valign=top, margin_top=500 { Layers floored min_height=100 { Rectangle big4 width=100, height=150 } }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 60.0), &FixedAdvance);
    let rect = |n: &str| rects[page.find(n).unwrap() as usize];
    assert_eq!(rect("box").h, 80.0);
    assert_eq!(rect("filler").h, 80.0, "the filler takes the written 80: {:?}", rect("filler"));
    assert_eq!(rect("deeper").h, 80.0, "written flows down through fillers: {:?}", rect("deeper"));
    assert_eq!(rect("big").h, 150.0, "the child keeps its own size: {:?}", rect("big"));
    // The floor where the room comes from the viewport: a 60-tall
    // viewport, a 150-tall child — the root and its filler both grow.
    let c = ok("View v {\n Layers free { Rectangle big2 width=100, height=150, halign=left, valign=top }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 60.0), &FixedAdvance);
    let free = rects[page.find("free").unwrap() as usize];
    assert_eq!(free.h, 150.0, "a filler of the viewport still floors at its content: {free:?}");
    assert_eq!(rect("hfill").h, 150.0, "a hug box is sized by content, so is its filler: {:?}", rect("hfill"));
    assert_eq!(rect("floored").h, 100.0, "a min_height floor is a written number and wins: {:?}", rect("floored"));
}

/// A NEGATIVE margin is the part that hangs out on purpose and comes off
/// the claim a child makes on a content-sized parent (decided 2026-08-30,
/// away from the Godot addon's "counts as zero", D242): a 100-wide box
/// with `margin_right=-30` asks its hug parent for 70; a positive margin
/// still adds; the claim never goes below zero; and a filler in a free
/// parent whose child hangs 70 below through a negative margin floors at
/// the part that stays inside.
#[test]
fn a_negative_margin_hangs_out_and_is_not_claimed() {
    let c = ok("View v {\n Panel plate width=hug, height=hug, halign=left, valign=top { Rectangle r width=100, height=40, halign=left, valign=top, margin_right=-30 }\n Panel plate2 width=hug, height=hug, halign=left, valign=top, margin_top=100 { Rectangle r2 width=100, height=40, halign=left, valign=top, margin_right=30 }\n Panel plate3 width=hug, height=hug, halign=left, valign=top, margin_top=200 { Rectangle r3 width=100, height=40, halign=left, valign=top, margin_right=-300 }\n Layers zone margin_top=300 { Rectangle bg width=fill, height=150, valign=bottom, margin_bottom=-70 }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 380.0), &FixedAdvance);
    let rect = |n: &str| rects[page.find(n).unwrap() as usize];
    assert_eq!(rect("plate").w, 70.0, "the hanging 30 is not claimed: {:?}", rect("plate"));
    assert_eq!(rect("r").w, 100.0, "the child is still 100 wide and hangs out: {:?}", rect("r"));
    assert_eq!(rect("plate2").w, 130.0, "a positive margin is space to keep: {:?}", rect("plate2"));
    assert_eq!(rect("plate3").w, 0.0, "never below zero: {:?}", rect("plate3"));
    assert_eq!(rect("zone").h, 80.0, "the filler floors at the part inside (150 - 70), which fits the 80 it was given: {:?}", rect("zone"));
}

/// `layout::offscreen` names the nodes shoved wholly past the viewport
/// (the toolbar's buttons, GPAR100) and stays quiet about a box parked
/// there on purpose through a negative margin (the fullscreen tile at
/// `margin_right=-80`, its children with it), about gone nodes, and about
/// a Scroll's content below the fold.
#[test]
fn offscreen_names_shoved_nodes_not_parked_ones() {
    let c = ok("View v {\n Rectangle low width=50, height=50, halign=left, valign=top, margin_top=900\n Button tile 60x60, halign=right, valign=bottom, margin_right=-80 { Rectangle inner 10x10 }\n Rectangle gone width=50, height=50, halign=left, valign=top, margin_top=950, visibility=gone\n Scroll list width=100, height=100, halign=left, valign=top { Column { Rectangle a width=100, height=300; Rectangle b width=100, height=300 } }\n Layers box width=200, height=80, halign=left, valign=top, margin_top=200 { Rectangle bottomed width=100, height=40, valign=bottom, margin_bottom=-500 }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let names: Vec<&str> = gogo::layout::offscreen(page, &res, &rects, Rect::new(0.0, 0.0, 400.0, 400.0)).iter().map(|id| page.str(page.nodes[*id as usize].name)).collect();
    assert_eq!(names, vec!["low"], "{rects:?}");
}

/// GPAR39 (docs/todo-gogo-parity.md): margins never stretch a written
/// `hug` — `width=hug, margin=8` is a content-wide box pinned 8 in from
/// the near edge (Godot's reading), not a parent−16 span. Needed because
/// the both-edges span rule used to swallow `hug`: the written word was
/// silently ignored and a content-sized card drew as a near-full-width
/// plate. Two named edges still span a box with NO size word.
#[test]
fn margins_never_stretch_a_hug_box() {
    let c = ok("View v {\n Panel card width=hug, height=hug, margin=8, valign=top { Rectangle r 50x20 }\n Panel span margin=8, margin_top=100, height=40 { }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let rect = |n: &str| rects[page.find(n).unwrap() as usize];
    let card = rect("card");
    assert_eq!(card.w, 50.0, "hug keeps its content width: {card:?}");
    assert_eq!(card.x, 8.0, "pinned 8 in from the near edge: {card:?}");
    assert_eq!(card.h, 20.0, "the aligned axis hugs too: {card:?}");
    let span = rect("span");
    assert_eq!(span.w, 384.0, "no size word: two named edges still span (400 - 8 - 8): {span:?}");
    assert_eq!(span.x, 8.0);
}

/// GPAR43 (E11): `scale=` means the whole picture at that size — the box
/// shrinks AND the sizes drawn from props shrink with it: glyph size,
/// corner radius, border width. Without this, `when held scale=0.9` on a
/// captioned button keeps full-size text poking out of a 90 % plate.
#[test]
fn scale_shrinks_glyphs_corners_and_borders_with_the_box() {
    let c = ok("arrangement small\nView v {\n Button b text=\"OK\", width=100, height=40, halign=left, valign=top, corner_radius=8, border_thickness=2, border_color=#FF0000FF, font_size=20\n}\nwhen small { b.scale = 0.5 }\n");
    let mut rt = Runtime::new(&c.page);
    rt.go(0);
    let list = draws(&c.page, &rt);
    // The plate: half the box (about its center), half the corner, half the border.
    let plate = list
        .iter()
        .find_map(|d| match d {
            Draw::Quad { rect, corner, border, .. } if *corner > 0.0 => Some((*rect, *corner, *border)),
            _ => None,
        })
        .expect("button plate");
    assert_eq!(plate, (Rect::new(25.0, 10.0, 50.0, 20.0), 4.0, 1.0), "{list:#?}");
    // The caption: glyphs at half size, never the written 20.
    let size = list
        .iter()
        .find_map(|d| match d {
            Draw::Text { text, font_size, .. } if c.page.str(*text) == "OK" => Some(*font_size),
            _ => None,
        })
        .expect("caption");
    assert_eq!(size, 10.0, "{list:#?}");
}

/// GPAR37 (the hit half): what a written cap clips is not tappable — the
/// picture and the finger agree. The Button pokes past its capped parent;
/// a tap on the visible half hits, a tap on the clipped half misses. An
/// uncapped hug parent grows instead, so the same Button stays tappable
/// end to end.
#[test]
fn clipped_overflow_is_not_hit_testable() {
    let c = ok("View v {\n Layers z halign=left, valign=top {\n  Layers cap width=100, height=40, halign=left, valign=top { Button b text=\"wide\", width=300, height=40 }\n  Layers free halign=left, valign=top, margin_top=100 { Button b2 text=\"wide\", width=300, height=40 }\n }\n}\n");
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let b = page.find("b").unwrap();
    let b2 = page.find("b2").unwrap();
    assert_eq!(rects[b as usize].w, 300.0, "the Button keeps its written width");
    assert_eq!(hit(page, &res, &rects, 50.0, 20.0, false), Some(b), "the visible half taps");
    assert_eq!(hit(page, &res, &rects, 250.0, 20.0, false), None, "the clipped half takes no taps");
    assert_eq!(hit(page, &res, &rects, 250.0, 120.0, false), Some(b2), "under a hug parent the same spot still taps");
}

/// GPAR50: the finger lands on the DRAWN picture — while an anim
/// translates a button, taps follow the moved rect, not the layout slot
/// it left (Godot writes motion onto the engine control, so its picking
/// moves with it).
#[test]
fn hit_follows_the_drawn_rect_during_motion() {
    let c = ok("anim slide { b.translate.x to=100, duration=1, easing=linear }\nView v { Button b width=80, height=40, halign=left, valign=top, text=\"x\" }");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    let slide = page.intern("slide");
    rt.play(&page, slide, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    let res = resolve_animated(&page, &mut rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    assert_eq!(hit(&page, &res, &rects, 90.0, 20.0, false), Some(b), "the tap lands where the button is drawn");
    assert_ne!(hit(&page, &res, &rects, 20.0, 20.0, false), Some(b), "the slot it left takes no taps");
}
