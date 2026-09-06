//! Two language additions of 2026-09-06 (user): `text_fit=stretch` /
//! `text_flip=` on a Label (one glyph pulled over the box — the text
//! page's speech bubbles), and the 3D turnstile: a `tap_pulse` rides the
//! `swing` motion channel and the draw walk wraps the node in a
//! `SwingPush` with the WP7 camera (`swing_focal`); an anim can drive the
//! same lane by name.
use gogo::draw::{Draw, draw, swing_focal};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::NO_STR;
use gogo::runtime::{Runtime, resolve, TAP_PULSE_ANGLE_DEG, TAP_PULSE_BACK_SEC, TAP_PULSE_OUT_SEC};
use gogo::{Finish, Page, compile, has_errors};

fn ok(src: &str) -> Page {
    let c = compile(src, &|_| None);
    assert!(!has_errors(&c.diags), "{:#?}", c.diags);
    c.page
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{} {}\n", d.code, d.message)).collect()
}

fn draws(page: &Page, rt: &Runtime) -> (Vec<Draw>, Vec<Rect>) {
    let res = resolve(page, rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 300.0), &FixedAdvance);
    (draw(page, &res, &rects), rects)
}

/// A stretched Label draws ONE `TextStretch` over its rect, the flip
/// as spelled, and no ordinary text.
#[test]
fn a_stretched_label_draws_one_glyph_over_its_box() {
    let page = ok("View v { Label bubble text=\"0\", font=\"bubbles\", text_fit=stretch, text_flip=horizontal, width=200, height=100, font_color=#FF0000FF }");
    let rt = Runtime::new(&page);
    let (d, rects) = draws(&page, &rt);
    let b = page.find("bubble").unwrap();
    let stretched: Vec<&Draw> = d.iter().filter(|x| matches!(x, Draw::TextStretch { .. })).collect();
    assert_eq!(stretched.len(), 1, "{d:?}");
    let Draw::TextStretch { rect, text, font, flip, color, .. } = stretched[0] else { unreachable!() };
    assert_eq!(*rect, rects[b]);
    assert_eq!(page.str(*text), "0");
    assert_eq!(page.str(*font), "bubbles");
    assert_eq!(*flip, (true, false));
    assert_eq!(color.0, [255, 0, 0, 255]);
    assert!(!d.iter().any(|x| matches!(x, Draw::Text { .. })), "stretched text is not also drawn as text");

    // `text_flip=` alone (fit unset) changes nothing: text stays text.
    let page = ok("View v { Label l text=\"0\", text_flip=both, width=200, height=100 }");
    let rt = Runtime::new(&page);
    let (d, _) = draws(&page, &rt);
    assert!(d.iter().any(|x| matches!(x, Draw::Text { .. })) && !d.iter().any(|x| matches!(x, Draw::TextStretch { .. })));
    // The four flip words.
    for (word, want) in [("none", (false, false)), ("vertical", (false, true)), ("both", (true, true))] {
        let page = ok(&format!("View v {{ Label l text=\"0\", text_fit=stretch, text_flip={word}, width=10, height=10 }}"));
        let rt = Runtime::new(&page);
        let (d, _) = draws(&page, &rt);
        let Some(Draw::TextStretch { flip, font, .. }) = d.iter().find(|x| matches!(x, Draw::TextStretch { .. })) else { panic!("{d:?}") };
        assert_eq!(*flip, want, "{word}");
        assert_eq!(*font, NO_STR, "no font named: the default face");
    }
}

/// The words are closed sets, and a Label's alone.
#[test]
fn text_fit_words_are_closed_and_label_only() {
    let m = messages("View v { Label l text=\"0\", text_fit=squash }");
    assert!(m.contains("G0057") && m.contains("none/stretch"), "{m}");
    let m = messages("View v { Label l text=\"0\", text_fit=stretch, text_flip=sideways }");
    assert!(m.contains("G0057") && m.contains("horizontal/vertical/both"), "{m}");
    let m = messages("View v { Panel p text_fit=stretch }");
    assert!(m.contains("stretches a Label's one glyph"), "{m}");
    let m = messages("View v { Button b text=\"0\", text_flip=both }");
    assert!(m.contains("stretches a Label's one glyph"), "{m}");
}

/// A completed tap's turnstile is a `SwingPush` about the node's left
/// edge with the WP7 camera on its center, wrapping the node's own
/// draws, gone once the swing settles.
#[test]
fn a_tap_pulse_swings_the_box_in_3d() {
    let page = ok("View v { Button b tap_pulse=true, width=200, height=50, margin_left=20, margin_top=30, halign=left, valign=top, background=#FF0000FF }");
    let mut page = page;
    let mut rt = Runtime::new(&page);
    let b = page.find("b").unwrap();
    let (d, rects) = draws(&page, &rt);
    assert!(!d.iter().any(|x| matches!(x, Draw::SwingPush { .. })), "flat at rest");
    rt.tap_pulse(b);
    rt.tick(&mut page, TAP_PULSE_OUT_SEC);
    let (d, _) = draws(&page, &rt);
    let r = rects[b];
    let push = d.iter().position(|x| matches!(x, Draw::SwingPush { .. })).expect("a SwingPush");
    let pop = d.iter().position(|x| matches!(x, Draw::SwingPop)).expect("a SwingPop");
    let Draw::SwingPush { angle, pivot_x, cx, cy, focal } = d[push] else { unreachable!() };
    assert!((angle - TAP_PULSE_ANGLE_DEG).abs() < 1e-3, "{angle}");
    assert_eq!((pivot_x, cx, cy), (r.x, r.x + r.w / 2.0, r.y + r.h / 2.0), "hinge on the left edge, camera on the center");
    assert!((focal - swing_focal(r.h)).abs() < 1e-4);
    // Silverlight's camera: half the height over tan(28.5°).
    assert!((swing_focal(50.0) - 25.0 / (28.5f32).to_radians().tan()).abs() < 1e-3);
    assert!(push < pop, "the push wraps the node's draws");
    assert!(d[push + 1..pop].iter().any(|x| matches!(x, Draw::Quad { color, .. } if color.0 == [255, 0, 0, 255])), "the plate is inside the swing: {d:?}");
    rt.tick(&mut page, TAP_PULSE_BACK_SEC + 0.01);
    let (d, _) = draws(&page, &rt);
    assert!(!d.iter().any(|x| matches!(x, Draw::SwingPush { .. })), "settled flat: no push");
}

/// An anim drives the same lane by name: `box.swing from=0, to=90`.
#[test]
fn an_anim_can_drive_the_swing_lane() {
    let src = "anim turn {\n box.swing from=0, to=90, duration=1, easing=linear\n}\nView v { Rectangle box width=100, height=100, color=#FFFFFFFF }\n";
    let mut page = ok(src);
    let mut rt = Runtime::new(&page);
    let turn = page.intern_lookup("turn");
    rt.play_id(&page, turn, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    let (d, _) = draws(&page, &rt);
    let Some(Draw::SwingPush { angle, .. }) = d.iter().find(|x| matches!(x, Draw::SwingPush { .. })) else { panic!("no swing: {d:?}") };
    assert!((angle - 45.0).abs() < 1e-3, "{angle}");
}
