//! Pins for the first parity batch (docs/todo-gogo-parity.md GPAR11,
//! GPAR42, GPAR52): the built-in disabled dim is a bottom-of-the-ladder
//! `when`, a text-only Button's padding reaches the draw pass, and a
//! player whose channels a newer one took keeps its clock and hands over
//! the true base.
use gogo::anim::Finish;
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::DISABLED_OPACITY;
use gogo::runtime::{Runtime, resolve};
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
