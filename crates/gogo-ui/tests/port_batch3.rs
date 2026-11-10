//! Pins for the layout batch (docs/todo-gogo.md GP16–GP18, GP21, GP22,
//! GP25): a fill child floors at its content, a hug wrap label stays one
//! line, a seat fits its occupant, Tiles abut and keep content heights, a
//! `clip_text` label claims no width, and a timed arrangement flip glides
//! every re-posed box to its new rect.
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::runtime::{Runtime, resolve, resolve_animated};
use gogo::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn lay(src: &str, w: f32, h: f32) -> (Page, Vec<Rect>) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, w, h), &FixedAdvance);
    (c.page, rects)
}

fn rect(p: &Page, r: &[Rect], name: &str) -> Rect {
    r[(p.find(name).unwrap()).index()]
}

/// GP16: two fill Buttons in a tight Row never go under their labels — the
/// wide one takes its content, the other the rest.
#[test]
fn fill_child_floors_at_its_content() {
    // "wide label here" = 15 chars × 5.5 = 82.5 + 24 padding = 106.5; Row 150.
    let (p, r) = lay("View v {\n Row bar width=150, height=40, halign=left, valign=top {\n  Button a width=fill, text=\"wide label here\", font_size=10\n  Button b width=fill, text=\"x\", font_size=10\n }\n}\n", 400.0, 400.0);
    let a = rect(&p, &r, "a");
    let b = rect(&p, &r, "b");
    assert!((a.w - 106.5).abs() < 0.01, "floored at its content: {a:?}");
    assert!((b.w - (150.0 - 106.5)).abs() < 0.01, "{b:?}");
    // With room to spare the shares are equal, content or not.
    let (p, r) = lay("View v {\n Row bar width=400, height=40, halign=left, valign=top {\n  Button a width=fill, text=\"wide label here\", font_size=10\n  Button b width=fill, text=\"x\", font_size=10\n }\n}\n", 400.0, 400.0);
    assert_eq!(rect(&p, &r, "a").w, 200.0);
    assert_eq!(rect(&p, &r, "b").w, 200.0);
}

/// GP17: a hug wrap label in a narrow Column measures its whole text on
/// one line; `max_width=fill` binds it to the column and it wraps there;
/// `width=fill` wraps at the column too.
#[test]
fn hug_wrap_label_stays_one_line_until_a_width_binds() {
    let text = "one two three four five six";
    let (p, r) = lay(&format!("View v {{\n Column c width=100, halign=left, valign=top {{\n  Label a text=\"{text}\", font_size=10, wrap=true, halign=left\n  Label b text=\"{text}\", font_size=10, wrap=true, halign=left, max_width=fill\n  Label d text=\"{text}\", font_size=10, wrap=true, width=fill\n }}\n}}\n"), 400.0, 400.0);
    let a = rect(&p, &r, "a");
    assert_eq!((a.w, a.h), (text.chars().count() as f32 * 5.5, 12.5), "hug: one line, wider than the column: {a:?}");
    // The direct form — the cap law (2026-09-01): a capped hug is legal
    // under any parent; `max_width=fill` binds at the Column's width.
    let b = rect(&p, &r, "b");
    assert!(b.w <= 100.0 && b.h > 12.5, "capped hug wraps in the column: {b:?}");
    let d = rect(&p, &r, "d");
    assert!((d.w - 100.0).abs() < 0.01 && d.h > 12.5, "fill wraps at the column: {d:?}");
    // No width word and no alignment = a stretch child: its box binds the
    // text (the tip dialogs' paragraphs) — it wraps and its HEIGHT counts
    // the wrapped lines, so the next sibling sits below the whole text.
    let (p, r) = lay(&format!("View v {{
 Column c width=100, halign=left, valign=top {{
  Label e text=\"{text}\", font_size=10, wrap=true
  Rectangle after height=10
 }}
}}
"), 400.0, 400.0);
    let e = rect(&p, &r, "e");
    assert!(e.h > 12.5, "stretch wraps: {e:?}");
    assert!((rect(&p, &r, "after").y - e.h).abs() < 0.01);
}

/// GP18: the occupant of a seat takes the socket's whole rect — its own
/// alignment and margins never bind — and the seat's minimum is the
/// occupant's without margins.
#[test]
fn seat_fits_its_occupant_to_the_socket() {
    let c = ok("arrangement side\nView v {\n Layers host halign=left, valign=top {\n  seat socket width=200, height=100\n  Panel pane width=fill, height=fill, halign=left, margin=10 { Rectangle x width=fill, height=fill }\n }\n}\nwhen side { pane.seat = socket }\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.go_id(gogo::ArrangementId::new(0));
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let socket = rect(&page, &rects, "socket");
    let pane = rect(&page, &rects, "pane");
    assert_eq!(pane, socket, "fitted to the socket, margins ignored");
    // A hug seat holding a 50×30 box pinned by margins is 50×30, not 60×40.
    let c = ok("arrangement side\nView v {\n Column col halign=left, valign=top {\n  seat socket\n  Layers free { Rectangle box 50x30, margin_left=10, margin_top=10 }\n }\n}\nwhen side { box.seat = socket }\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.go_id(gogo::ArrangementId::new(0));
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let socket = rect(&page, &rects, "socket");
    assert_eq!((socket.w, socket.h), (50.0, 30.0), "{socket:?}");
}

/// GP21 + GP22: Tiles cells abut (`spacing=` says nothing), a cell keeps
/// its own height even with `height=fill`, and a `clip_text` caption
/// claims no width so it never widens the lattice.
#[test]
fn tiles_abut_keep_content_heights_and_clip_text_claims_nothing() {
    let (p, r) = lay("View v {\n Tiles t cols=2, spacing=8, width=200, halign=left, valign=top {\n  Rectangle a height=40\n  Rectangle b min_height=20, height=fill\n  Rectangle c height=30, span=2\n }\n}\n", 400.0, 400.0);
    let a = rect(&p, &r, "a");
    let b = rect(&p, &r, "b");
    let c = rect(&p, &r, "c");
    assert_eq!(a, Rect::new(0.0, 0.0, 100.0, 40.0));
    assert_eq!(b, Rect::new(100.0, 0.0, 100.0, 20.0), "content height, no gutter: {b:?}");
    assert_eq!(c, Rect::new(0.0, 40.0, 200.0, 30.0), "the next row starts at the tallest cell, no gutter: {c:?}");
    let (p, r) = lay("View v {\n Tiles t cols=3, width=hug, halign=left, valign=top {\n  Label cap text=\"a very long caption that would widen the tile\", font_size=10, clip_text=true\n  Rectangle x 40x40\n }\n}\n", 400.0, 400.0);
    let t = rect(&p, &r, "t");
    assert_eq!(t.w, 40.0, "the widest cell's OWN claim is the 40 box, not the caption: {t:?}");
}

/// GP25: under `transition base -> wide duration=1`, a box whose halign
/// flips glides across; without a rule it snaps.
#[test]
fn timed_arrangement_glides_every_reposed_box() {
    let src = "arrangement wide\nView v {\n Rectangle box 100x50, halign=left, valign=top { when wide { halign = right } }\n}\ntransition base -> wide duration=1\n";
    let c = ok(src);
    let mut page = c.page;
    let b = (page.find("box").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let frame = |rt: &mut Runtime, page: &mut Page, dt: f32| -> Rect {
        rt.tick(page, dt);
        let res = resolve_animated(page, rt);
        let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        rt.glide(rects)[b]
    };
    assert_eq!(frame(&mut rt, &mut page, 0.0).x, 0.0);
    rt.go_id(gogo::ArrangementId::new(0));
    let start = frame(&mut rt, &mut page, 0.0);
    assert_eq!(start.x, 0.0, "the glide starts where the box was");
    let mid = frame(&mut rt, &mut page, 0.5);
    assert!(mid.x > 100.0 && mid.x < 200.0, "half way across: {mid:?}");
    assert!(rt.animating());
    let end = frame(&mut rt, &mut page, 0.6);
    assert_eq!(end.x, 300.0);
    assert!(!rt.animating());
    // No rule: instant.
    let c = ok("arrangement wide\nView v {\n Rectangle box 100x50, halign=left, valign=top { when wide { halign = right } }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    frame(&mut rt, &mut page, 0.0);
    rt.go_id(gogo::ArrangementId::new(0));
    assert_eq!(frame(&mut rt, &mut page, 0.0).x, 300.0);
}
