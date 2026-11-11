//! The `Reloader` with the `hot-reload` feature OFF (`gogo_ui::reload`):
//! the same calls a host writes for a development build, and every one of
//! them does nothing. A page opened through it is the page as built and
//! never changes, whatever its file says.
//!
//! `cargo test -p gogo-ui` and the packaged crate run these; a workspace
//! run has the feature on (the window example asks for it) and runs
//! `reloader.rs` instead.
#![cfg(not(feature = "hot-reload"))]

mod reload_kit;

use gogo_ui::reload::{Reloader, Sources};
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{Color_Straight_Srgb_4xU8_RGBA, Page, PropKey, Value, compile};
use reload_kit::build;

const PAGE: &str = r#"View root {
    Column col {
        Label title text="Icon Size"/iconSize, font_color=#FF0000FF
    }
}
"#;

const RED: Value = Value::VColor(Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255]));

fn title_color(page: &Page, rt: &Runtime) -> Value {
    resolve(page, rt).prop(page, page.find("title").unwrap(), PropKey::FontColor).expect("the title has a color")
}

#[test]
fn without_the_feature_every_reload_call_does_nothing() {
    let kit = build(&[("page.gogo", PAGE)], &[], &[]);
    let mut reload = Reloader::new(kit.tables, kit.sources());
    assert!(!reload.is_on());
    reload.on_change(|| panic!("nothing is watched"));

    // The file says another color than the build read: the page as built.
    kit.edit("page.gogo", "#FF0000FF", "#00FF00FF");
    let mut page: Page = reload.open(kit.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    assert_eq!(title_color(&page, &rt), RED);

    reload.mark_changed();
    assert!(!reload.has_pending_change());
    assert!(!reload.take_changed());
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));
    assert_eq!(title_color(&page, &rt), RED);
}

#[test]
fn without_the_feature_a_page_the_host_compiled_stays_as_compiled() {
    let kit = build(&[("page.gogo", PAGE)], &[], &[]);
    let mut reload = Reloader::files(Sources::dir(std::env::temp_dir().join("gogo_no_such_folder")));
    let mut page: Page = compile(&kit.text("page.gogo"), &|_| None).page;
    let mut rt = Runtime::new(&page);
    reload.follow(&mut page, "page.gogo");
    kit.edit("page.gogo", "#FF0000FF", "#00FF00FF");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));
    assert_eq!(title_color(&page, &rt), RED);
    assert!(!Reloader::off().is_on());
}
