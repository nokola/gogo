//! D14: a hostless widget poses its OWN body on the screen class —
//! `when screen.landscapeSmall { card.width = 420 }` beside the body — and
//! the pose compiles once per instance as a DEFAULT: a page's pose on the
//! same instance wins, and the two never clash as two writers.
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::runtime::{Runtime, resolve};
use gogo::{Orientation, Page, ScreenClass, compile};

fn cards() -> Page {
    let src = "widget Card {
    Panel card width=fill, height=40 { Label l text=\"c\" }
    when screen.landscapeSmall {
        card.width = 420
        card.halign = left
    }
}
View v { Column { Card a; Card b { when screen.landscapeSmall width=300 } } }";
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c.page
}

fn widths(page: &Page, rt: &Runtime, view: Rect) -> (f32, f32) {
    let res = resolve(page, rt);
    let rects = layout(page, &res, view, &FixedAdvance);
    // The body root IS the instance (`a/card` aliases `a`).
    (rects[page.find("a").unwrap()].w, rects[page.find("b").unwrap()].w)
}

#[test]
fn a_widgets_screen_pose_lands_per_instance_and_the_pages_pose_wins() {
    let page = cards();
    let mut rt = Runtime::new(&page);
    rt.set_screen(&page, Orientation::Portrait, ScreenClass::PortraitSmall);
    assert_eq!(widths(&page, &rt, Rect::new(0.0, 0.0, 360.0, 800.0)), (360.0, 360.0), "portrait: the body's own width=fill");
    rt.set_screen(&page, Orientation::Landscape, ScreenClass::LandscapeSmall);
    let (a, b) = widths(&page, &rt, Rect::new(0.0, 0.0, 800.0, 360.0));
    assert_eq!(a, 420.0, "the widget's pose, per instance");
    assert_eq!(b, 300.0, "the page's instance-line pose wins over the widget's default");
}

/// Any other state word at widget scope still names nothing to answer.
#[test]
fn a_hostless_widgets_state_when_is_still_refused() {
    let c = compile("widget Card { Panel card width=fill { Label l text=\"c\" }
 when held { card.width = 1 } }
View v { Card a }", &|_| None);
    assert!(c.diags.iter().any(|d| d.code == "G5001"), "{:#?}", c.diags);
}
