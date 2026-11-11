//! The `Reloader` with the `hot-reload` feature ON (`gogo_ui::reload`): a
//! page opened through it shows its file; a save reaches every open page
//! that reads the saved file, once, with the page's state kept; a save
//! that cannot be shown changes nothing and says why.
//!
//! These tests need the feature: `cargo test --workspace` turns it on
//! (the window example asks for it), `cargo test -p gogo-ui --features
//! hot-reload` does too. `reloader_off.rs` is the other half.
#![cfg(feature = "hot-reload")]

mod reload_kit;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use gogo_ui::reload::{Refused, Reloader, ShapeChange, Sources, Why};
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{Color_Straight_Srgb_4xU8_RGBA, Page, PropKey, Value, compile};
use reload_kit::{Kit, build};

const THEME: &str = "$accent = #FF0000FF\n";

const PAGE: &str = r#"uses "theme.gogo"
View root {
    Column col {
        Label title text="Icon Size"/iconSize, font_color=$accent
        Button go text="OK"/ok
        Scroll list { Label inner text="x"/_ }
    }
}
"#;

/// A second page, which reads no other file.
const OTHER: &str = r#"View root {
    Column col {
        Label note text="OK"/ok, font_color=#0000FFFF
    }
}
"#;

const RED: Value = Value::VColor(Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255]));
const GREEN: Value = Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255]));

fn kit() -> Kit {
    build(&[("page.gogo", PAGE), ("other.gogo", OTHER)], &[("theme.gogo", THEME)], &[])
}

fn title_color(page: &Page, rt: &Runtime) -> Value {
    resolve(page, rt).prop(page, page.find("title").unwrap(), PropKey::FontColor).expect("the title has a color")
}

#[test]
fn a_page_opened_after_an_edit_shows_the_edit() {
    let kit = kit();
    let mut reload = Reloader::new(kit.tables, kit.sources());
    assert!(reload.is_on());

    // The file as the build read it: the page as built, nothing to do.
    let mut page: Page = reload.open(kit.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    assert_eq!(title_color(&page, &rt), RED);
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));

    // Edited while the page was closed: the next open shows the edit.
    kit.edit("page.gogo", "font_color=$accent", "font_color=#00FF00FF");
    let mut page: Page = reload.open(kit.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    assert_eq!(title_color(&page, &rt), GREEN);
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false), "it shows its file already");

    // A file that cannot be shown: the page as built, and its first
    // refresh says why.
    kit.edit("page.gogo", "Button go", "Rectangle extra\n        Button go");
    let mut page: Page = reload.open(kit.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    assert_eq!(title_color(&page, &rt), RED, "the page as built");
    let refused: Refused = reload.refresh(&mut page, &mut rt).unwrap_err();
    assert_eq!(refused.file, "page.gogo");
    assert!(matches!(refused.why, Why::Shape(ShapeChange::Count { .. })), "{refused}");
    assert!(refused.to_string().starts_with("page.gogo: node count changed") && refused.to_string().contains("needs a rebuild"), "{refused}");
}

/// A host that makes its pages with `Page::from_static`, in many places,
/// changes none of them: the first `refresh` finds the page's file by the
/// built page the copy was made from.
#[test]
fn a_copy_of_a_built_page_is_taken_over_at_its_first_refresh() {
    let kit = kit();
    let mut reload = Reloader::new(kit.tables, kit.sources());

    // The file as built: nothing to do, now and after an unrelated mark.
    let mut page: Page = kit.page("page.gogo");
    let mut rt = Runtime::new(&page);
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));

    // Edited before the copy was made, and nobody marked a change since:
    // the copy's first refresh still brings it in step.
    kit.edit("page.gogo", "font_color=$accent", "font_color=#00FF00FF");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    let mut late: Page = kit.page("page.gogo");
    let mut late_rt = Runtime::new(&late);
    assert_eq!(title_color(&late, &late_rt), RED, "the copy as built");
    assert_eq!(reload.refresh(&mut late, &mut late_rt), Ok(true));
    assert_eq!(title_color(&late, &late_rt), GREEN);
    assert_eq!(reload.refresh(&mut late, &mut late_rt), Ok(false));

    // A later save reaches it like any open page.
    kit.edit("page.gogo", "font_color=#00FF00FF", "font_color=#FF0000FF");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut late, &mut late_rt), Ok(true));
    assert_eq!(title_color(&late, &late_rt), RED);
}

#[test]
fn a_save_reaches_an_open_page_once_and_keeps_its_state() {
    let kit = kit();
    let mut reload = Reloader::new(kit.tables, kit.sources());
    let mut page: Page = reload.open(kit.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    let (col, list) = (page.find("col").unwrap(), page.find("list").unwrap());
    rt.set_prop_instant(col, PropKey::Spacing, Value::VNum(4.0));
    rt.set_scroll(list, [0.0, 321.0]);

    // A save nobody marked is not looked at: `refresh` costs a compare.
    kit.edit("page.gogo", "font_color=$accent", "font_color=#00FF00FF");
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));
    assert_eq!(title_color(&page, &rt), RED);

    reload.mark_changed();
    assert!(reload.has_pending_change());
    assert!(reload.take_changed(), "true once per change");
    assert!(!reload.take_changed());
    assert!(!reload.has_pending_change());
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    assert_eq!(title_color(&page, &rt), GREEN, "the edit shows");
    assert_eq!(resolve(&page, &rt).prop(&page, col, PropKey::Spacing), Some(Value::VNum(4.0)), "a code write carries");
    assert_eq!(rt.scroll_of(list), [0.0, 321.0], "the scroll carries");
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false), "one save, one swap");

    // A change mark with the files as they are: nothing to do.
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));
}

#[test]
fn a_save_that_cannot_be_shown_changes_nothing_and_says_why() {
    let kit = kit();
    let mut reload = Reloader::new(kit.tables, kit.sources());
    let mut page: Page = reload.open(kit.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    let good: String = kit.text("page.gogo");

    // A typo: the compiler's error, with the file and the line.
    kit.edit("page.gogo", "font_color=$accent", "font_colour=$accent");
    reload.mark_changed();
    let refused: Refused = reload.refresh(&mut page, &mut rt).unwrap_err();
    let Why::Errors(errors) = &refused.why else { panic!("{refused}") };
    assert_eq!(errors[0].line, 4);
    assert!(refused.to_string().starts_with(&format!("page.gogo:4 {} ", errors[0].code)), "{refused}");
    assert_eq!(title_color(&page, &rt), RED, "the page on screen is the one before");
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false), "the same save is said once");

    // Another file saved while this one is still wrong: said again.
    kit.edit("other.gogo", "#0000FFFF", "#00FFFFFF");
    reload.mark_changed();
    assert!(reload.refresh(&mut page, &mut rt).is_err());

    // A key the app was not built with.
    kit.save("page.gogo", &good.replace("/iconSize", "/brandNew"));
    reload.mark_changed();
    let refused: Refused = reload.refresh(&mut page, &mut rt).unwrap_err();
    assert!(refused.to_string().contains("G8004") && refused.to_string().contains("rebuild"), "{refused}");

    // The file put back to what the page shows: nothing to compile, and
    // the host hears that the page is in step again.
    kit.save("page.gogo", &good);
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));

    // A fixed file after a refused one.
    kit.edit("page.gogo", "font_color=$accent", "font_colour=$accent");
    reload.mark_changed();
    assert!(reload.refresh(&mut page, &mut rt).is_err());
    kit.save("page.gogo", &good.replace("font_color=$accent", "font_color=#00FF00FF"));
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    assert_eq!(title_color(&page, &rt), GREEN);
}

#[test]
fn a_saved_uses_file_reaches_the_pages_that_use_it_and_no_other() {
    let kit = kit();
    let mut reload = Reloader::new(kit.tables, kit.sources());
    let (mut page, mut other): (Page, Page) = (reload.open(kit.built("page.gogo")), reload.open(kit.built("other.gogo")));
    let (mut rt, mut other_rt) = (Runtime::new(&page), Runtime::new(&other));

    kit.save("theme.gogo", "$accent = #00FF00FF\n");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true), "the page that reads the theme");
    assert_eq!(title_color(&page, &rt), GREEN);
    assert_eq!(reload.refresh(&mut other, &mut other_rt), Ok(false), "the page that does not");

    // Two open copies of one page both take the save.
    let mut twin: Page = reload.open(kit.built("page.gogo"));
    let mut twin_rt = Runtime::new(&twin);
    kit.save("theme.gogo", "$accent = #FF0000FF\n");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    assert_eq!(reload.refresh(&mut twin, &mut twin_rt), Ok(true));
    assert_eq!((title_color(&page, &rt), title_color(&twin, &twin_rt)), (RED, RED));
}

#[test]
fn a_reloader_that_is_off_and_a_page_with_no_file_follow_nothing() {
    let kit = kit();
    let mut off = Reloader::off();
    assert!(!off.is_on());
    let mut page: Page = off.open(kit.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    kit.edit("page.gogo", "font_color=$accent", "font_color=#00FF00FF");
    off.mark_changed();
    assert!(!off.take_changed());
    assert_eq!(off.refresh(&mut page, &mut rt), Ok(false));
    assert_eq!(title_color(&page, &rt), RED);

    // A page the host compiled and never handed over follows no file.
    let mut reload = Reloader::new(kit.tables, kit.sources());
    let mut loose: Page = compile(OTHER, &|_| None).page;
    let mut loose_rt = Runtime::new(&loose);
    kit.edit("other.gogo", "#0000FFFF", "#00FF00FF");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut loose, &mut loose_rt), Ok(false));
    assert_eq!(resolve(&loose, &loose_rt).prop(&loose, loose.find("note").unwrap(), PropKey::FontColor), Some(Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0, 0, 255, 255]))));

    // A page whose file is not in the sources (a phone the files were
    // never copied to) is the page as built.
    let bare = build(&[("page.gogo", PAGE)], &[("theme.gogo", THEME)], &[]);
    let mut nowhere = Reloader::new(bare.tables, Sources::read_with(|_| None));
    let mut page: Page = nowhere.open(bare.built("page.gogo"));
    let mut rt = Runtime::new(&page);
    nowhere.mark_changed();
    assert_eq!(nowhere.refresh(&mut page, &mut rt), Ok(false));
    assert_eq!(title_color(&page, &rt), RED);
}

/// A program with no build: it compiles its page when it starts and finds
/// its nodes by name, so a reload may change anything.
#[test]
fn a_page_the_host_compiled_follows_its_file_through_any_edit() {
    let kit = kit();
    let mut reload = Reloader::files(kit.sources());
    let mut page: Page = compile(&kit.text("other.gogo"), &|_| None).page;
    let mut rt = Runtime::new(&page);
    reload.follow(&mut page, "other.gogo");
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(false));
    let col = page.find("col").unwrap();
    rt.set_prop_instant(col, PropKey::Spacing, Value::VNum(4.0));

    // The same nodes: the state carries.
    kit.edit("other.gogo", "#0000FFFF", "#00FF00FF");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    assert_eq!(resolve(&page, &rt).prop(&page, page.find("note").unwrap(), PropKey::FontColor), Some(GREEN));
    assert_eq!(resolve(&page, &rt).prop(&page, col, PropKey::Spacing), Some(Value::VNum(4.0)), "a code write carries");

    // A node more, a key and a picture nobody built in: all fine here.
    // The page starts fresh, and the host writes its values again.
    kit.edit("other.gogo", "Label note", "Label added text=\"New\"/brandNew\n        Label note");
    reload.mark_changed();
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    assert!(page.find("added").is_some(), "the new node is on the page");
    assert_eq!(resolve(&page, &rt).prop(&page, page.find("col").unwrap(), PropKey::Spacing), None, "a fresh page: the code's write is gone");

    // A file that does not compile keeps the page.
    kit.edit("other.gogo", "font_color=#00FF00FF", "font_colour=#00FF00FF");
    reload.mark_changed();
    let refused: Refused = reload.refresh(&mut page, &mut rt).unwrap_err();
    assert!(refused.to_string().starts_with("other.gogo:"), "{refused}");
    assert!(page.find("added").is_some());
}

/// A folder as the sources: a saved file is noticed by the reloader's own
/// thread, which wakes the host.
#[test]
fn a_saved_file_in_a_watched_folder_wakes_the_host() {
    let dir = std::env::temp_dir().join(format!("gogo_reloader_dir_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("other.gogo"), OTHER).unwrap();

    let mut reload = Reloader::files(Sources::dir(dir.clone()));
    let woken = Arc::new(AtomicU32::new(0));
    let count = woken.clone();
    reload.on_change(move || {
        count.fetch_add(1, Ordering::SeqCst);
    });
    let mut page: Page = compile(OTHER, &|_| None).page;
    let mut rt = Runtime::new(&page);
    reload.follow(&mut page, "other.gogo");
    assert!(!reload.has_pending_change(), "what is in the folder at the start is not a change");

    std::fs::write(dir.join("other.gogo"), OTHER.replace("#0000FFFF", "#00FF00FF")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !reload.has_pending_change() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(reload.has_pending_change(), "the save was noticed");
    assert!(woken.load(Ordering::SeqCst) >= 1, "and the host was woken");
    assert_eq!(reload.refresh(&mut page, &mut rt), Ok(true));
    assert_eq!(resolve(&page, &rt).prop(&page, page.find("note").unwrap(), PropKey::FontColor), Some(GREEN));
    drop(reload);
    let _ = std::fs::remove_dir_all(&dir);
}
