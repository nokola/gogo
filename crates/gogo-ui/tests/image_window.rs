//! `window=` / `sampling=` on an Image and the `%` anim endpoint
//! (2026-09-06, user — the canvas view's words): a window is the texture
//! rectangle the draw shows (texture pixels, the fit maps it), the
//! sampling word rides the draw, bad spellings are refused, and a
//! `translate.x from=-100%` lane starts one node-width to the left,
//! measured from the last glide.
use gogo_ui::anim::Finish;
use gogo_ui::KeptFrame;
use gogo_ui::draw::{Draw, Fit, Sampling, draw};
use gogo_ui::layout::{FixedAdvance, Rect, layout};
use gogo_ui::runtime::{Runtime, resolve, resolve_animated};
use gogo_ui::{Compiled, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn image_draw(src: &str) -> (Option<[f32; 4]>, Sampling, Fit) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let f = KeptFrame::fresh(&c.page, resolve(&c.page, &rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let draws = draw(&c.page, f.frame());
    draws.iter().find_map(|d| if let Draw::Image { window, sampling, fit, .. } = d { Some((*window, *sampling, *fit)) } else { None }).expect("an Image draw")
}

#[test]
fn a_window_and_the_sampling_word_reach_the_draw() {
    let (window, sampling, fit) = image_draw("View v { Image i 40x20, halign=left, valign=top, source=\"res://t.png\", fit=distort, window=[8, 4, 16, 8], sampling=pixel }");
    assert_eq!(window, Some([8.0, 4.0, 16.0, 8.0]));
    assert_eq!(sampling, Sampling::Pixel);
    assert_eq!(fit, Fit::Distort);
    // Unset: the whole texture, smoothed.
    let (window, sampling, _) = image_draw("View v { Image i 40x20, source=\"res://t.png\" }");
    assert_eq!(window, None);
    assert_eq!(sampling, Sampling::Smooth);
}

#[test]
fn a_bad_window_or_sampling_word_is_refused() {
    for bad in ["window=[8, 4]", "window=[0, 0, 0, 10]", "window=[-1, 0, 4, 4]", "window=5"] {
        let src = format!("View v {{ Image i 40x20, source=\"res://t.png\", {bad} }}");
        assert!(codes(&src).contains(&"G2010".to_string()), "{bad}: {:?}", codes(&src));
    }
    assert!(gogo_ui::has_errors(&compile("View v { Image i 40x20, source=\"res://t.png\", sampling=blurry }", &|_| None).diags), "sampling is a closed set");
    // The words are an Image's alone.
    assert!(gogo_ui::has_errors(&compile("View v { Panel p 40x20, window=[0, 0, 4, 4] }", &|_| None).diags));
    assert!(gogo_ui::has_errors(&compile("View v { Rectangle r 40x20, sampling=pixel }", &|_| None).diags));
}

#[test]
fn a_percent_endpoint_is_a_fraction_of_the_nodes_own_box() {
    let c = ok("anim slideUp { sheet.translate.y from=100%, to=0, duration=1 }\nanim pushLeft { sheet.translate.x from=0, to=-100%, duration=1 }\nView v { Panel sheet width=fill, height=50, valign=bottom }\n");
    let mut page = c.page;
    let sheet = page.find("sheet").unwrap();
    let mut rt = Runtime::new(&page);
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 200.0, 400.0), &FixedAdvance);
    let rects = f.rects();
    assert_eq!(rects[sheet.index()], Rect::new(0.0, 350.0, 200.0, 50.0));
    rt.glide(rects.to_vec());

    // 100% of the sheet's height: it starts one sheet-height below its seat.
    let id = page.intern_lookup("slideUp");
    rt.play_id(&page, id, Finish::Nothing);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.motion[sheet.index()].translate, [0.0, 50.0], "{:?}", r.motion[sheet.index()]);
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert!((r.motion[sheet.index()].translate[1] - 25.0).abs() < 0.01, "{:?}", r.motion[sheet.index()]);
    for _ in 0..8 {
        rt.tick(&mut page, 0.1);
    }
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.motion[sheet.index()].translate, [0.0, 0.0]);

    // -100% of its width (fill = the view's 200): the push lands one
    // screen width to the left.
    let id = page.intern_lookup("pushLeft");
    rt.play_id(&page, id, Finish::Nothing);
    for _ in 0..12 {
        rt.tick(&mut page, 0.1);
    }
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.motion[sheet.index()].translate[0], -200.0, "{:?}", r.motion[sheet.index()]);
}

/// A sheet hidden at play has no box: the percent waits for the next
/// glide and the sheet's first laid-out frame is already one height below.
#[test]
fn a_percent_on_a_hidden_node_resolves_at_the_next_glide() {
    let c = ok("anim slideUp { sheet.translate.y from=100%, to=0, duration=1 }\nView v { Panel sheet width=fill, height=50, valign=bottom, visibility=gone }\n");
    let mut page = c.page;
    let sheet = page.find("sheet").unwrap();
    let mut rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    rt.glide(layout(&page, &res, Rect::new(0.0, 0.0, 200.0, 400.0), &FixedAdvance));
    rt.set_visible(&mut page, sheet, true);
    let id = page.intern_lookup("slideUp");
    rt.play_id(&page, id, Finish::Nothing);
    // The frame's tick before the layout: nothing to measure yet.
    rt.tick(&mut page, 1.0 / 60.0);
    let r = resolve_animated(&page, &mut rt);
    rt.glide(layout(&page, &r, Rect::new(0.0, 0.0, 200.0, 400.0), &FixedAdvance));
    let r = resolve_animated(&page, &mut rt);
    assert!((r.motion[sheet.index()].translate[1] - 50.0).abs() < 1.5, "one sheet-height below at its first laid-out frame: {:?}", r.motion[sheet.index()]);
    for _ in 0..70 {
        rt.tick(&mut page, 1.0 / 60.0);
    }
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.motion[sheet.index()].translate, [0.0, 0.0]);
}

#[test]
fn a_percent_on_any_other_lane_is_refused() {
    assert!(codes("anim a { p.scale from=50%, to=1, duration=1 }\nView v { Panel p 40x20 }\n").contains(&"G0064".to_string()));
    assert!(codes("anim a { p.opacity to=100%, duration=1 }\nView v { Panel p 40x20 }\n").contains(&"G0064".to_string()));
    assert!(codes("anim a { p.rotate from=10%, to=0, duration=1 }\nView v { Panel p 40x20 }\n").contains(&"G0064".to_string()));
}
