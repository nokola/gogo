//! `atlas` lines (2026-09-27, user: "build all of it — grid form, named
//! slices, json import, edge="): one picture cut into slices that
//! `source=` names. Pins the three forms and the import, the draws each
//! slice becomes (a window into its picture; nine for a 9-slice; a
//! trimmed slice placed inside its untrimmed box), layout's natural size,
//! slices set by code, the errors, and that no hot type grew.
use std::collections::HashMap;

use gogo::KeptFrame;
use gogo::draw::{Draw, Fit, Sampling, draw};
use gogo::layout::{FixedAdvance, Rect};
use gogo::page::{ImageKey, ImageSlice, NO_IMAGE, PropKey, Value};
use gogo::runtime::{Runtime, resolve};
use gogo::{Compiled, CompileOptions, compile_with};

/// A PNG's first 24 bytes: all `picture_size` reads.
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    b.extend_from_slice(b"IHDR");
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b
}

/// The files the tests' atlases read.
fn files() -> HashMap<&'static str, Vec<u8>> {
    let mut f = HashMap::new();
    f.insert("res://s/animals.png", png(480, 720));
    f.insert("res://s/toolbar.png", png(96, 64));
    f.insert("res://s/bubble.png", png(64, 32));
    f.insert("res://ui/ui.png", png(128, 64));
    f.insert("res://ui/ui.json", HASH_JSON.as_bytes().to_vec());
    f.insert("res://ui/list.json", ARRAY_JSON.as_bytes().to_vec());
    f
}

/// A TexturePacker "JSON (Hash)" export: one plain frame, one trimmed
/// frame (20x10 pixels that sat at 10,5 inside a 40x20 box).
const HASH_JSON: &str = r#"{
  "frames": {
    "buttons/Play-Hover.png": { "frame": {"x": 0, "y": 0, "w": 32, "h": 32}, "rotated": false, "trimmed": false,
        "spriteSourceSize": {"x": 0, "y": 0, "w": 32, "h": 32}, "sourceSize": {"w": 32, "h": 32} },
    "star.png": { "frame": {"x": 32, "y": 0, "w": 20, "h": 10}, "rotated": false, "trimmed": true,
        "spriteSourceSize": {"x": 10, "y": 5, "w": 20, "h": 10}, "sourceSize": {"w": 40, "h": 20} },
    "panel.png": { "frame": {"x": 64, "y": 0, "w": 64, "h": 64}, "rotated": false, "trimmed": false }
  },
  "meta": { "image": "ui.png", "size": {"w": 128, "h": 64} }
}"#;

/// The same picture as a "JSON (Array)" export.
const ARRAY_JSON: &str = r#"{ "frames": [ { "filename": "01.png", "frame": {"x": 0, "y": 32, "w": 16, "h": 16} } ], "meta": { "image": "ui.png" } }"#;

fn build(src: &str) -> Compiled {
    let f = files();
    let read = |p: &str| f.get(p).cloned().ok_or_else(|| "no such file".to_string());
    compile_with(src, &|_| None, CompileOptions { assets: Some(&read), ..CompileOptions::default() })
}

fn ok(src: &str) -> Compiled {
    let c = build(src);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    build(src).diags.iter().map(|d| d.code.clone()).collect()
}

fn image_draws(c: &Compiled, w: f32, h: f32) -> Vec<(Rect, ImageKey, Fit, Option<[f32; 4]>, Sampling)> {
    let rt = Runtime::new(&c.page);
    let f = KeptFrame::fresh(&c.page, resolve(&c.page, &rt), Rect::new(0.0, 0.0, w, h), &FixedAdvance);
    draw(&c.page, f.frame()).iter().filter_map(|d| if let Draw::Image { rect, image, fit, window, sampling, .. } = d { Some((*rect, *image, *fit, *window, *sampling)) } else { None }).collect()
}

fn picture(c: &Compiled, path: &str) -> ImageKey {
    c.images.get(path).expect("the atlas picture is in the image table")
}

#[test]
fn a_grid_cell_draws_its_square_of_the_picture() {
    let c = ok("atlas animals source=\"res://s/animals.png\", cell=80x80\nView v { Image i 40x40, halign=left, valign=top, source=animals[7], sampling=pixel }");
    let draws = image_draws(&c, 200.0, 200.0);
    assert_eq!(draws.len(), 1);
    let (rect, image, fit, window, sampling) = draws[0];
    // Cell 7 of a 6-wide grid: row 1, column 1.
    assert_eq!(window, Some([80.0, 80.0, 80.0, 80.0]));
    assert_eq!(image, picture(&c, "res://s/animals.png"), "hosts only ever see the whole picture");
    assert_eq!((rect, fit, sampling), (Rect::new(0.0, 0.0, 40.0, 40.0), Fit::Contain, Sampling::Pixel));
    // 480x720 in 80x80 squares: 54 slices, one picture.
    assert_eq!(c.page.slices.len(), 54);
    assert_eq!(c.images.len(), 1);
}

#[test]
fn a_named_slice_and_a_whole_picture_atlas() {
    let c = ok("atlas toolbar source=\"res://s/toolbar.png\" {\n slice play x=0, y=0, size=48x48\n slice pause x=48, y=0, size=48x48\n}\natlas bubble source=\"res://s/bubble.png\"\nView v { Column c { Image a 20x20, source=toolbar.pause\n Image b 20x20, source=bubble } }");
    let draws = image_draws(&c, 200.0, 200.0);
    assert_eq!(draws[0].3, Some([48.0, 0.0, 48.0, 48.0]));
    // A whole-picture atlas with no `edge=` IS the picture: no slice, no window.
    assert_eq!(draws[1].1, picture(&c, "res://s/bubble.png"));
    assert_eq!(draws[1].3, None);
    assert_eq!(c.page.slices.len(), 2);
}

#[test]
fn a_nine_slice_keeps_its_corners_and_stretches_the_rest() {
    // 64x32 picture, 10-pixel bands; shown at 200x100.
    let c = ok("atlas bubble source=\"res://s/bubble.png\", edge=10\nView v { Image i 200x100, halign=left, valign=top, source=bubble }");
    let draws = image_draws(&c, 400.0, 400.0);
    assert_eq!(draws.len(), 9, "{draws:#?}");
    let pic = picture(&c, "res://s/bubble.png");
    assert!(draws.iter().all(|d| d.1 == pic && d.2 == Fit::Distort));
    // Corners at their own size (1 picture px = 1 epx), in reading order.
    assert_eq!((draws[0].0, draws[0].3), (Rect::new(0.0, 0.0, 10.0, 10.0), Some([0.0, 0.0, 10.0, 10.0])));
    assert_eq!((draws[2].0, draws[2].3), (Rect::new(190.0, 0.0, 10.0, 10.0), Some([54.0, 0.0, 10.0, 10.0])));
    assert_eq!((draws[8].0, draws[8].3), (Rect::new(190.0, 90.0, 10.0, 10.0), Some([54.0, 22.0, 10.0, 10.0])));
    // The middle stretches both ways.
    assert_eq!((draws[4].0, draws[4].3), (Rect::new(10.0, 10.0, 180.0, 80.0), Some([10.0, 10.0, 44.0, 12.0])));

    // Smaller than two corners: every band shrinks together (10x10 box,
    // 20 px of bands across → half size), the middle has no room.
    let small = ok("atlas bubble source=\"res://s/bubble.png\", edge=10\nView v { Image i 10x10, halign=left, valign=top, source=bubble }");
    let draws = image_draws(&small, 400.0, 400.0);
    assert_eq!(draws.len(), 4, "only the corners have room: {draws:#?}");
    assert_eq!(draws[0].0, Rect::new(0.0, 0.0, 5.0, 5.0));
    assert_eq!(draws[3].0, Rect::new(5.0, 5.0, 5.0, 5.0));
}

#[test]
fn edge_words_follow_the_padding_rule() {
    // `edge=` writes all four, a side word one, the later word wins.
    let c = ok("atlas toolbar source=\"res://s/toolbar.png\" {\n slice b x=0, y=0, size=96x64, edge=8, edge_bottom=20\n}\nView v { Image i 96x64, halign=left, valign=top, source=toolbar.b }");
    assert_eq!(c.page.slices[0].edge, [8, 8, 8, 20]);
    let c = ok("atlas toolbar source=\"res://s/toolbar.png\" {\n slice b x=0, y=0, size=96x64, edge_bottom=20, edge=8\n}\nView v { Image i 96x64, source=toolbar.b }");
    assert_eq!(c.page.slices[0].edge, [8, 8, 8, 8]);
}

#[test]
fn a_json_import_names_its_slices_and_places_a_trimmed_one_inside_its_box() {
    let c = ok("atlas ui from=\"res://ui/ui.json\" {\n slice panel edge=16\n}\nView v { Column c {\n Image a 32x32, source=ui.buttons_play_hover\n Image b 80x40, source=ui.star, fit=distort\n Image p 100x100, source=ui.panel } }");
    let pic = picture(&c, "res://ui/ui.png");
    let draws = image_draws(&c, 400.0, 400.0);
    assert_eq!((draws[0].1, draws[0].3), (pic, Some([0.0, 0.0, 32.0, 32.0])));
    // star: 20x10 pixels at 10,5 of a 40x20 box, the box distorted to
    // 80x40 (x2): the pixels land at 20,10 inside the Image, 40x20.
    let b = draws[1].0;
    let a_top = draws[0].0.y;
    assert_eq!((b.w, b.h), (40.0, 20.0));
    assert_eq!(b.y - (a_top + 32.0), 10.0, "10 epx below the Image's top: {draws:#?}");
    assert_eq!(draws[1].3, Some([32.0, 0.0, 20.0, 10.0]));
    // The block's `edge=` made `panel` a 9-slice.
    assert_eq!(draws.len(), 2 + 9);
    // The Array layout reads the same way; `01.png` becomes `_01`.
    let arr = ok("atlas list from=\"res://ui/list.json\"\nView v { Image a 16x16, source=list._01 }");
    assert_eq!(image_draws(&arr, 100.0, 100.0)[0].3, Some([0.0, 32.0, 16.0, 16.0]));
}

#[test]
fn a_trimmed_slice_fits_like_its_untrimmed_box() {
    // contain: the 40x20 box in 100x100 scales x2.5 and centers (y 25);
    // the pixels (10,5 inside) land at 25, 37.5, 50x25.
    let c = ok("atlas ui from=\"res://ui/ui.json\"\nView v { Image s 100x100, halign=left, valign=top, source=ui.star }");
    let d = image_draws(&c, 400.0, 400.0)[0];
    assert_eq!((d.0, d.2, d.3), (Rect::new(25.0, 37.5, 50.0, 25.0), Fit::Distort, Some([32.0, 0.0, 20.0, 10.0])));
    // cover: x5 (the box is 200x100, centered: it starts at x -70); the
    // pixels (50..150 in the box) start at -20 and are cut at the Image's
    // left edge, so the window starts 20/5 = 4 picture pixels in.
    let c = ok("atlas ui from=\"res://ui/ui.json\"\nView v { Image s 60x100, halign=left, valign=top, source=ui.star, fit=cover }");
    let d = image_draws(&c, 400.0, 400.0)[0];
    assert_eq!(d.0, Rect::new(0.0, 25.0, 60.0, 50.0));
    assert_eq!(d.3, Some([32.0 + 4.0, 0.0, 12.0, 10.0]), "the window loses what the Image cut: {d:?}");
}

#[test]
fn natural_aspect_reads_the_slice_size() {
    let c = ok("atlas toolbar source=\"res://s/toolbar.png\" {\n slice wide x=0, y=48, size=96x16\n}\natlas ui from=\"res://ui/ui.json\"\nView v { Column c { Image a width=192, aspect=natural, source=toolbar.wide\n Image s width=80, aspect=natural, source=ui.star } }");
    let rt = Runtime::new(&c.page);
    let f = KeptFrame::fresh(&c.page, resolve(&c.page, &rt), Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let (a, s) = (c.page.find("a").unwrap(), c.page.find("s").unwrap());
    assert_eq!(f.rects()[a.index()].h, 32.0, "96x16 at 192 wide");
    assert_eq!(f.rects()[s.index()].h, 40.0, "a trimmed slice lays out as its 40x20 box, not its 20x10 pixels");
}

#[test]
fn a_slice_set_by_code_draws_like_a_written_one() {
    let mut c = ok("atlas animals source=\"res://s/animals.png\", cell=80x80\nView v { Image i 40x40, source=\"\" }");
    let i = c.page.find("i").unwrap();
    let mut rt = Runtime::new(&c.page);
    rt.set_prop(i, PropKey::Source, Value::VImage(ImageKey::from_slice_index(53)));
    let f = KeptFrame::fresh(&c.page, resolve(&c.page, &rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let d = draw(&c.page, f.frame());
    let window = d.iter().find_map(|d| if let Draw::Image { window, .. } = d { *window } else { None });
    assert_eq!(window, Some([400.0, 640.0, 80.0, 80.0]), "the last cell");
    let _ = &mut c;
}

#[test]
fn a_theme_value_and_a_widget_param_carry_a_slice() {
    let c = ok("atlas toolbar source=\"res://s/toolbar.png\" {\n slice play x=0, y=0, size=48x48\n}\n$icon = toolbar.play\nwidget Tile $pic=\"\" { Image img 20x20, source=$pic }\nView v { Column c { Image a 20x20, source=$icon\n Tile t pic=toolbar.play } }");
    let draws = image_draws(&c, 200.0, 200.0);
    assert_eq!(draws.len(), 2);
    assert!(draws.iter().all(|d| d.3 == Some([0.0, 0.0, 48.0, 48.0])), "{draws:#?}");
}

#[test]
fn one_atlas_twice_keeps_its_ids_and_a_different_one_is_refused() {
    // The build shares one image table: a `uses` file under two pages
    // declares the same atlas twice.
    let f = files();
    let read = |p: &str| f.get(p).cloned().ok_or_else(|| "no such file".to_string());
    let shared = std::cell::RefCell::new(gogo::Images::default());
    let opts = CompileOptions { assets: Some(&read), images: Some(&shared), ..CompileOptions::default() };
    let line = "atlas animals source=\"res://s/animals.png\", cell=80x80\n";
    let a = compile_with(&format!("{line}View v {{ Image i 4x4, source=animals[3] }}"), &|_| None, opts);
    let b = compile_with(&format!("{line}View v {{ Image i 4x4, source=animals[3] }}"), &|_| None, opts);
    assert!(!gogo::has_errors(&a.diags) && !gogo::has_errors(&b.diags));
    assert_eq!(shared.borrow().slices.len(), 54, "no second copy of the slices");
    let other = compile_with("atlas animals source=\"res://s/animals.png\", cell=40x40\nView v { }", &|_| None, opts);
    assert!(other.diags.iter().any(|d| d.code == "G2019"), "{:?}", other.diags);
}

#[test]
fn the_atlas_code_module_spells_what_the_files_spell() {
    let c = ok("atlas bubble source=\"res://s/bubble.png\"\natlas animals source=\"res://s/animals.png\", cell=240x720\natlas toolbar source=\"res://s/toolbar.png\" {\n slice playNow x=0, y=0, size=48x48\n}\nView v { }");
    let code = gogo::emit::emit_atlases(&c.images.atlases).unwrap();
    assert!(code.contains("pub const BUBBLE: ImageKey = ImageKey::new(0);"), "{code}");
    assert!(code.contains("pub const ANIMALS: [ImageKey; 2] = [ImageKey::new(2147483648), ImageKey::new(2147483649)];"), "{code}");
    assert!(code.contains("pub mod toolbar {") && code.contains("pub const PLAY_NOW: ImageKey = ImageKey::new(2147483650);"), "{code}");
    let slices = gogo::emit::emit_slices(&c.images.slices);
    assert!(slices.starts_with("/// Every atlas slice") && slices.contains("pub static SLICES: [gogo::page::ImageSlice; 3]"), "{slices}");
}

#[test]
fn atlas_errors_name_the_fix() {
    let v = "View v { }";
    // G2011: shape.
    assert!(codes(&format!("atlas icons cell=48x48\n{v}")).contains(&"G2011".into()));
    assert!(codes(&format!("atlas icons source=\"res://s/toolbar.png\", from=\"res://ui/ui.json\"\n{v}")).contains(&"G2011".into()));
    assert!(codes("View v { atlas icons source=\"res://s/toolbar.png\" }").contains(&"G2011".into()), "an atlas is a file-level line");
    assert!(codes(&format!("atlas icons source=\"res://s/toolbar.png\", tint=red\n{v}")).contains(&"G2011".into()));
    // G2012: a file the compile cannot read.
    assert!(codes(&format!("atlas icons source=\"res://s/missing.png\", cell=8x8\n{v}")).contains(&"G2012".into()));
    let bare = compile_with(&format!("atlas icons source=\"res://s/toolbar.png\", cell=8x8\n{v}"), &|_| None, CompileOptions::default());
    assert!(bare.diags.iter().any(|d| d.code == "G2012"), "no asset reader: {:?}", bare.diags);
    // G2013: the cells do not tile the picture.
    assert!(codes(&format!("atlas animals source=\"res://s/animals.png\", cell=80x70\n{v}")).contains(&"G2013".into()));
    // G2014: a slice past the picture; one name twice.
    assert!(codes(&format!("atlas t source=\"res://s/toolbar.png\" {{\n slice a x=90, y=0, size=8x8\n}}\n{v}")).contains(&"G2014".into()));
    assert!(codes(&format!("atlas t source=\"res://s/toolbar.png\" {{\n slice a x=0, y=0, size=8x8\n slice a x=8, y=0, size=8x8\n}}\n{v}")).contains(&"G2014".into()));
    // G2015: bands that leave no middle; a trimmed slice.
    assert!(codes(&format!("atlas b source=\"res://s/bubble.png\", edge=16\n{v}")).contains(&"G2015".into()));
    assert!(codes(&format!("atlas ui from=\"res://ui/ui.json\" {{\n slice star edge=2\n}}\n{v}")).contains(&"G2015".into()));
    // G2016: no such atlas / cell / slice; the wrong spelling for the form.
    assert!(codes("View v { Image i 4x4, source=nope[1] }").contains(&"G2016".into()));
    assert!(codes("atlas animals source=\"res://s/animals.png\", cell=80x80\nView v { Image i 4x4, source=animals[54] }").contains(&"G2016".into()));
    assert!(codes("atlas animals source=\"res://s/animals.png\", cell=80x80\nView v { Image i 4x4, source=animals }").contains(&"G2016".into()));
    assert!(codes("atlas ui from=\"res://ui/ui.json\"\nView v { Image i 4x4, source=ui.nope }").contains(&"G2016".into()));
    assert!(codes("atlas animals source=\"res://s/animals.png\", cell=80x80\nView v { Rectangle r 4x4, color=animals[1] }").contains(&"G2016".into()));
    assert!(codes("View v { Image i 4x4, source=animals[x] }").contains(&"G2016".into()), "a cell number is a whole number");
    // G2018: `window=` on a slice; `fit=` on a 9-slice.
    assert!(codes("atlas animals source=\"res://s/animals.png\", cell=80x80\nView v { Image i 4x4, source=animals[1], window=[0, 0, 2, 2] }").contains(&"G2018".into()));
    assert!(codes("atlas b source=\"res://s/bubble.png\", edge=4\nView v { Image i 4x4, source=b, fit=cover }").contains(&"G2018".into()));
}

#[test]
fn json_import_errors_say_what_the_export_got_wrong() {
    let mut f = files();
    f.insert("res://ui/rot.json", br#"{"frames": {"a.png": {"frame": {"x":0,"y":0,"w":4,"h":4}, "rotated": true}}, "meta": {"image": "ui.png"}}"#.to_vec());
    f.insert("res://ui/clash.json", br#"{"frames": {"a.png": {"frame": {"x":0,"y":0,"w":4,"h":4}}, "A.jpg": {"frame": {"x":4,"y":0,"w":4,"h":4}}}, "meta": {"image": "ui.png"}}"#.to_vec());
    f.insert("res://ui/stale.json", br#"{"frames": {}, "meta": {"image": "ui.png", "size": {"w": 256, "h": 256}}}"#.to_vec());
    f.insert("res://ui/broken.json", b"{\"frames\": {".to_vec());
    let read = |p: &str| f.get(p).cloned().ok_or_else(|| "no such file".to_string());
    for (json, needle) in [("rot", "turn off rotation"), ("clash", "both become the name `a`"), ("stale", "export the atlas again"), ("broken", "line 1")] {
        let c = compile_with(&format!("atlas x from=\"res://ui/{json}.json\"\nView v {{ }}"), &|_| None, CompileOptions { assets: Some(&read), ..CompileOptions::default() });
        let d = c.diags.iter().find(|d| d.code == "G2017").unwrap_or_else(|| panic!("{json}: {:?}", c.diags));
        assert!(d.message.contains(needle), "{json}: {}", d.message);
    }
}

#[test]
fn no_hot_type_grew() {
    // A slice rides in the 4-byte `ImageKey` and its geometry in the
    // build's table, so a `Value` (every property of every page) and a
    // `Draw` (every draw command every frame) keep their size.
    assert_eq!(std::mem::size_of::<Value>(), 16);
    assert_eq!(std::mem::size_of::<Draw>(), 48);
    assert_eq!(std::mem::size_of::<ImageKey>(), 4);
    assert_eq!(std::mem::size_of::<ImageSlice>(), 28);
    // A slice id never collides with a picture id or `NO_IMAGE`.
    assert!(ImageKey::from_slice_index(0).is_slice() && !ImageKey::from_index(0).is_slice() && !NO_IMAGE.is_slice());
}
