//! The shortest road from `.gogo` text to something a host can draw.
//!
//!     cargo run -p gogo-ui --example first_page
//!
//! gogo does the middle of a UI: it compiles a page, lays it out for a
//! screen size and answers with a list of draw commands. A host program
//! supplies the two ends. It measures text (here `FixedAdvance`, a
//! stand-in that gives every letter the same width), and it puts the
//! commands on screen (here they are printed).
//!
//! This is the one-off road, right for a first look and for tests. A host
//! that draws every frame keeps a `Runtime` and a `KeptFrame` and asks
//! for `frame_retained`, which redoes only what changed
//! (docs/gogo-design.md).

use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{Compiled, Draw, KeptFrame, Page, compile, draw, has_errors};

const PAGE: &str = r#"
View hello {
    Column menu halign=center, valign=center, spacing=12 {
        Label title text="Hello from gogo", font_size=24
        Button startButton text="Start", min_width=160, min_height=44
    }
}
"#;

fn main() {
    // 1. Compile. This page names no other file, so the loader that
    //    `uses "..."` lines would call has nothing to answer.
    let compiled: Compiled = compile(PAGE, &|_| None);
    for d in &compiled.diags {
        println!("{:?} {} at line {}: {}", d.severity, d.code, d.line, d.message);
    }
    assert!(!has_errors(&compiled.diags), "the page does not compile");
    let page: Page = compiled.page;

    // 2. Lay it out for a 360 x 640 screen. Sizes are epx: the same
    //    number means the same size on a desktop and on a phone.
    let runtime: Runtime = Runtime::new(&page);
    let screen: Rect = Rect::new(0.0, 0.0, 360.0, 640.0);
    let kept: KeptFrame = KeptFrame::fresh(&page, resolve(&page, &runtime), screen, &FixedAdvance);
    for name in ["title", "startButton"] {
        let r: Rect = kept.frame().rects()[page.find(name).expect("a node of that name").index()];
        println!("{name}: x={} y={} w={} h={}", r.x, r.y, r.w, r.h);
    }

    // 3. Ask for the draw list: plain rectangles, text runs and pictures,
    //    in the order to paint them. Drawing them is the host's job.
    let draws: Vec<Draw> = draw(&page, kept.frame());
    println!("{} draw commands:", draws.len());
    for d in &draws {
        println!("  {d:?}");
    }
}
