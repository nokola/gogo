//! The text engine on real fonts: it measures, wraps, shapes and
//! rasterizes Roboto, takes more faces by key, scans the OS fonts, and
//! puts emoji in the color atlas.
//!
//! Needed as a file of its own, outside `src`, because every test here
//! reads the Roboto files in `assets/Fonts` at the top of the repository,
//! and a crates.io package does not carry them. `Cargo.toml` leaves this
//! file out of the package (`exclude`), so `cargo test` on a downloaded
//! package runs only what the package can feed.

use gogo_text::{EpxMeasure, FaceMetrics, GlyphQuad, HostColorGlyph, TextEngine, TextPx, styled_key};
use gogo_ui::layout::TextMeasure;
use std::cell::RefCell;
use std::path::Path;

/// A run whose glyph grows the atlas mid-way comes out whole against
/// the grown atlas: its uvs equal a re-shape's. Red when the glyphs
/// shaped before the grow keep uvs into the atlas that was and the run
/// sits in the cache that way: the user sees a label without its first
/// letters ("ve changes?" for "Save changes?").
#[test]
fn a_run_that_grows_the_atlas_is_shaped_whole_against_the_grown_one() {
    let mut e = engine();
    let before: u32 = e.generation;
    // 400-px glyphs: nine fill the 1024 atlas, so the alphabet grows it
    // inside this one run.
    let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let shaped: Vec<GlyphQuad> = e.shape(text, px(400.0), None, None, 0.0, 0.0, 100_000.0, 0);
    assert!(e.generation > before, "the precondition: the run grew the atlas");
    // Shaped again past the cache: the box width is part of the cache
    // key and moves no glyph of a start-aligned line.
    let again: Vec<GlyphQuad> = e.shape(text, px(400.0), None, None, 0.0, 0.0, 100_001.0, 0);
    assert_eq!(shaped.len(), again.len());
    for (a, b) in shaped.iter().zip(&again) {
        assert_eq!(a.uv, b.uv, "a glyph of the run that grew the atlas, shaped whole: {a:?} vs {b:?}");
    }
}

/// A right-to-left line in a wide box starts at the box's left edge
/// like a Latin one, and `halign` still moves it: centre and end land
/// where they would for Latin text, not twice as far. Red when
/// cosmic-text's default stands: the Arabic word sits against the
/// right edge of the wrap width. Ignored unless asked for: it needs an
/// OS font with Arabic letters, which a bare test machine may not
/// have (see `engine_with_emoji`).
#[test]
#[ignore = "needs an OS font with Arabic letters: run with --include-ignored on Windows"]
fn a_right_to_left_line_starts_at_the_left_edge_and_halign_still_moves_it() {
    let mut e = engine();
    e.load_system_fonts(); // the desktop's Arabic face (Arial / Segoe UI)
    assert!(e.has_glyph('ع'), "precondition: an Arabic face");
    let span = |q: &[GlyphQuad]| -> (f32, f32) { (q.iter().map(|g| g.x).fold(f32::MAX, f32::min), q.iter().map(|g| g.x + g.w).fold(f32::MIN, f32::max)) };
    let box_w: f32 = 600.0;
    let (left, right) = span(&e.shape("عربي", px(40.0), Some(box_w), None, 0.0, 0.0, box_w, 0));
    assert!(left < 20.0, "start = the left edge: {left}..{right}");
    let w: f32 = right - left;
    let (c_left, _) = span(&e.shape("عربي", px(40.0), Some(box_w), None, 0.0, 0.0, box_w, 1));
    assert!((c_left - (box_w - w) / 2.0).abs() < 20.0, "centred once: {c_left} for width {w}");
    let (_, e_right) = span(&e.shape("عربي", px(40.0), Some(box_w), None, 0.0, 0.0, box_w, 2));
    assert!((e_right - box_w).abs() < 20.0 && e_right <= box_w + 2.0, "end = the right edge: {e_right}");
    let (l_left, _) = span(&e.shape("English", px(40.0), Some(box_w), None, 0.0, 0.0, box_w, 0));
    assert!(l_left < 20.0, "Latin unchanged: {l_left}");
}

fn engine() -> TextEngine {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Regular.ttf")).expect("font");
    TextEngine::new(&[("assets/Fonts/Roboto Regular.ttf", bytes)])
}

/// Text of `font_size` device pixels on a screen with 1 device pixel per
/// epx: the line gap is 1 pixel.
fn px(font_size: f32) -> TextPx {
    TextPx::from_epx(font_size, 1.0)
}

/// Roboto plus Windows' Segoe UI Emoji as a fallback face (a color
/// face: COLRv0 layers swash paints itself).
///
/// Needed by the emoji tests, which are `#[ignore]` for that reason:
/// the file exists on Windows only, and a test that returned early
/// elsewhere would count as passed without having run. A Windows host
/// runs them with `cargo test -p gogo-text -- --include-ignored`.
fn engine_with_emoji() -> TextEngine {
    let segoe = Path::new("C:/Windows/Fonts/seguiemj.ttf");
    let mut e = engine();
    assert!(e.add_fallback_font_file(segoe), "this test needs {}", segoe.display());
    e
}

/// The color atlas texel at the top-left of `q`'s slot, RGBA.
fn color_texel_of(e: &TextEngine, q: &GlyphQuad) -> [u8; 4] {
    let (atlas, side) = e.color_atlas();
    let (x, y) = ((q.uv[0] * side as f32).round() as usize, (q.uv[1] * side as f32).round() as usize);
    let i = (y * side as usize + x) * 4;
    [atlas[i], atlas[i + 1], atlas[i + 2], atlas[i + 3]]
}

/// An emoji of a color face lands in the COLOR atlas with its own
/// colors (`color: true`, texels that are not gray), a letter beside
/// it in the coverage atlas; both atlases report dirty. Red when the
/// rasterizer keeps coverage masks only: every other glyph is dropped,
/// and the user sees a box for each emoji.
#[test]
#[ignore = "needs Windows' Segoe UI Emoji font: run with --include-ignored on Windows"]
fn an_emoji_lands_in_the_color_atlas_beside_the_letters() {
    let mut e: TextEngine = engine_with_emoji();
    assert!(!e.take_color_dirty(), "nothing packed yet");
    let quads: Vec<GlyphQuad> = e.shape("a😀", px(40.0), None, None, 0.0, 0.0, 0.0, 0);
    assert_eq!(quads.len(), 2, "{quads:?}");
    assert!(!quads[0].color, "the letter: coverage atlas");
    assert!(quads[1].color, "the emoji: color atlas");
    assert!(quads[1].w > 20.0 && quads[1].h > 20.0, "an emoji at 40 px is a real box: {:?}", quads[1]);
    assert!(e.take_dirty() && e.take_color_dirty());
    // Somewhere in the emoji's slot a texel is opaque and colored.
    let (atlas, side) = e.color_atlas();
    let (x0, y0, x1, y1) = ((quads[1].uv[0] * side as f32) as usize, (quads[1].uv[1] * side as f32) as usize, (quads[1].uv[2] * side as f32) as usize, (quads[1].uv[3] * side as f32) as usize);
    let colored = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).any(|(x, y)| {
        let i = (y * side as usize + x) * 4;
        let (r, g, b, a) = (atlas[i], atlas[i + 1], atlas[i + 2], atlas[i + 3]);
        // swash's layered coverage tops out a hair under 255.
        a >= 250 && (r != g || g != b)
    });
    assert!(colored, "the slot holds the emoji's own colors");
}

/// With a host painter installed, an emoji is painted by the host
/// (its raster lands in the color atlas, placed by its left/top), a
/// letter still by swash, and a host that declines (`None`) leaves
/// the emoji to swash. The painter runs once per (cluster, size).
#[test]
#[ignore = "needs Windows' Segoe UI Emoji font: run with --include-ignored on Windows"]
fn the_host_paints_an_emoji_when_it_has_one() {
    let mut e: TextEngine = engine_with_emoji();
    let asked: std::rc::Rc<RefCell<Vec<(String, f32)>>> = std::rc::Rc::new(RefCell::new(Vec::new()));
    let log = asked.clone();
    e.set_host_color_glyph_painter(Some(Box::new(move |cluster: &str, px: f32| {
        log.borrow_mut().push((cluster.to_string(), px));
        // A 6×6 solid green square sitting 2 px right of the pen, 5 above the baseline.
        (cluster == "😀").then(|| HostColorGlyph { width: 6, height: 6, left: 2, top: 5, rgba: [0, 255, 0, 255].repeat(36) })
    })));
    let quads: Vec<GlyphQuad> = e.shape("a😀", px(40.0), None, None, 0.0, 0.0, 0.0, 0);
    assert_eq!(quads.len(), 2, "{quads:?}");
    assert!(!quads[0].color, "the letter: swash, coverage atlas");
    assert_eq!(asked.borrow().as_slice(), &[("😀".to_string(), 40.0)], "asked once, the cluster text and the px size");
    assert!(quads[1].color);
    assert_eq!((quads[1].w, quads[1].h), (6.0, 6.0), "the host's raster size");
    assert_eq!(color_texel_of(&e, &quads[1]), [0, 255, 0, 255], "the host's pixels");
    // The second shape of the same run is served from the cache.
    let _ = e.shape("a😀", px(40.0), None, None, 0.0, 0.0, 0.0, 0);
    assert_eq!(asked.borrow().len(), 1, "no second ask for a cached run");
    // The quad sits `left` px right of the pen and `top` px above the
    // baseline: against a painter that reports (0, 0), it is 2 right
    // and 5 up. A new painter repaints (the old slots are dropped).
    e.set_host_color_glyph_painter(Some(Box::new(|_: &str, _: f32| Some(HostColorGlyph { width: 6, height: 6, left: 0, top: 0, rgba: [0, 0, 255, 255].repeat(36) }))));
    let at_pen: Vec<GlyphQuad> = e.shape("a😀", px(40.0), None, None, 0.0, 0.0, 0.0, 0);
    assert_eq!((quads[1].x - at_pen[1].x, quads[1].y - at_pen[1].y), (2.0, -5.0), "{quads:?} vs {at_pen:?}");
    assert_eq!(color_texel_of(&e, &at_pen[1]), [0, 0, 255, 255], "the new painter's pixels");
    // A host that declines leaves the emoji to swash.
    e.set_host_color_glyph_painter(Some(Box::new(|_: &str, _: f32| None)));
    let declined: Vec<GlyphQuad> = e.shape("a😀", px(40.0), None, None, 0.0, 0.0, 0.0, 0);
    assert!(declined[1].color && declined[1].w > 20.0, "swash's own raster: {declined:?}");
}

/// A host raster that fills the color atlas grows it, and the run is
/// shaped whole against the grown one (the coverage atlas's law,
/// `a_run_that_grows_the_atlas_is_shaped_whole_against_the_grown_one`).
#[test]
#[ignore = "needs Windows' Segoe UI Emoji font: run with --include-ignored on Windows"]
fn a_host_emoji_that_fills_the_color_atlas_grows_it() {
    let mut e: TextEngine = engine_with_emoji();
    e.set_host_color_glyph_painter(Some(Box::new(|_: &str, _: f32| Some(HostColorGlyph { width: 300, height: 300, left: 0, top: 300, rgba: [255, 0, 0, 255].repeat(300 * 300) }))));
    let before: u32 = e.generation;
    // Four 300-px squares do not fit a 512 atlas: it doubles mid-run.
    let quads: Vec<GlyphQuad> = e.shape("😀😁😂🤣", px(40.0), None, None, 0.0, 0.0, 0.0, 0);
    assert!(e.generation > before, "the precondition: the run grew the color atlas");
    assert_eq!(e.color_atlas().1, 1024);
    assert_eq!(quads.len(), 4, "{quads:?}");
    for q in &quads {
        assert!(q.color);
        assert_eq!(color_texel_of(&e, q), [255, 0, 0, 255], "every slot is into the grown atlas: {q:?}");
    }
}

/// A fallback file serves the glyphs no page font has: Roboto has no
/// Arabic, Windows' Arial does. The face gets no key, a missing file
/// is a false, and the measure cache is dropped so a label measured
/// as boxes is measured again. Ignored unless asked for: Arial is a
/// Windows file (see `engine_with_emoji`).
#[test]
#[ignore = "needs Windows' Arial font: run with --include-ignored on Windows"]
fn a_fallback_file_serves_glyphs_no_page_font_has() {
    let arial = Path::new("C:/Windows/Fonts/arial.ttf");
    assert!(arial.exists(), "this test needs {}", arial.display());
    let mut e = engine();
    assert!(!e.has_glyph('س'), "Roboto has no Arabic");
    let boxes_w: f32 = e.measure("سلام", px(20.0), None, None).0;
    assert!(e.add_fallback_font_file(arial));
    assert!(e.has_glyph('س'));
    assert!(!e.has_face("C:/Windows/Fonts/arial.ttf"), "a fallback face has no key");
    let glyphs_w: f32 = e.measure("سلام", px(20.0), None, None).0;
    assert_ne!(boxes_w, glyphs_w, "the measure cache was dropped");
    assert!(!e.add_fallback_font_file(Path::new("C:/Windows/Fonts/no-such-font.ttf")));
}

/// The Roboto faces' metrics as the files carry them (the layout
/// contract reads them, never a fitted constant).
#[test]
fn roboto_faces_carry_their_metrics() {
    for name in ["Roboto Regular", "Roboto Light", "Roboto Medium", "Roboto Bold"] {
        let path = format!("{}/../../assets/Fonts/{name}.ttf", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).expect("font");
        let e = TextEngine::new(&[(name, bytes)]);
        let m = e.metrics(None);
        println!("{name}: ascent {} descent {} upem {} -> 24px {} 28px {} 36px {} 48px {}", m.ascent, m.descent, m.units_per_em, m.font_height(24.0), m.font_height(28.0), m.font_height(36.0), m.font_height(48.0));
        assert!(m.units_per_em > 0.0);
    }
}

#[test]
fn measures_wraps_and_rasterizes() {
    let mut e = engine();
    let (w, h) = e.measure("Hello", px(20.0), None, None);
    assert!(w > 30.0 && w < 70.0, "{w}");
    assert_eq!(h, 24.0);
    // Whole-pixel ascent + descent at each size, not a ratio of it.
    let m = e.metrics(None);
    assert_eq!(m.font_height(24.0), 29.0);
    assert_eq!(m.font_height(28.0), 33.0);
    assert_eq!(m.font_height(20.0), 24.0);
    let (_, h2) = e.measure("one two three four five six seven", px(20.0), Some(100.0), None);
    assert!(h2 >= 48.0, "{h2}");
    let quads = e.shape("Hi", px(20.0), None, None, 10.0, 5.0, 0.0, 0);
    assert_eq!(quads.len(), 2);
    assert!(quads[0].x >= 10.0 && quads[1].x > quads[0].x);
    let (atlas, size) = e.atlas();
    assert!(atlas.iter().any(|b| *b > 128), "atlas has ink");
    assert_eq!(size, 1024);
    assert!(e.take_dirty());
    assert!(!e.take_dirty());
}

/// A style suffix on the font key is a faux style: bold grows the
/// coverage, italic skews it — the same glyph, its own atlas slot.
#[test]
fn a_style_suffix_fakes_bold_and_italic() {
    let mut e = engine();
    let key = "assets/Fonts/Roboto Regular.ttf";
    let plain = e.glyph("H", 24.0, Some(key)).unwrap();
    let bold = e.glyph("H", 24.0, Some(&styled_key(key, true, false))).unwrap();
    let italic = e.glyph("H", 24.0, Some(&styled_key(key, false, true))).unwrap();
    // 24 px → 1 px of growth each side.
    assert_eq!(bold.w, plain.w + 2.0, "bold {bold:?} vs {plain:?}");
    assert_eq!(bold.h, plain.h + 2.0);
    assert!(italic.w > plain.w, "the skew widens the ink box: {italic:?} vs {plain:?}");
    assert_eq!(e.measure("Hi", px(24.0), None, Some(key)), e.measure("Hi", px(24.0), None, Some(&styled_key(key, true, false))), "bold keeps the advances");
}

/// A face added by key after construction shapes under that key.
#[test]
fn a_face_added_later_shapes_under_its_key() {
    let mut e = engine();
    assert!(!e.has_face("Heavy"));
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Bold.ttf")).expect("font");
    assert!(e.add_face("Heavy", bytes));
    assert!(e.has_face("Heavy") && e.has_face("Heavy#bold"));
    let (regular, _) = e.measure("The quick brown fox", px(24.0), None, None);
    let (heavy, _) = e.measure("The quick brown fox", px(24.0), None, Some("Heavy"));
    assert!((regular - heavy).abs() > 1.0, "a different face measures differently: {regular} vs {heavy}");
    assert!(!e.add_face("junk", vec![0, 1, 2, 3]), "no face in those bytes");
}

/// The OS scan runs once, names each family once, and a family can
/// become a face. Skips its OS-specific half where the folders are
/// empty (a bare CI box).
#[test]
fn system_fonts_load_once_and_name_families() {
    let mut e = engine();
    assert!(e.system_families().is_empty());
    let n = e.load_system_fonts();
    assert_eq!(e.load_system_fonts(), n, "the second call is a no-op");
    let names = e.system_families().to_vec();
    assert_eq!(names.len(), n);
    let mut sorted = names.clone();
    sorted.dedup();
    assert_eq!(sorted, names, "sorted, no repeats");
    if names.is_empty() {
        eprintln!("SKIP: no system fonts on this box");
        return;
    }
    assert!(!e.add_system_face("os", "no such family 0x1234"));
    assert!(e.add_system_face("os", &names[0]));
    let (w, _) = e.measure("Hello", px(24.0), None, Some("os"));
    assert!(w > 0.0);
}

/// One glyph reports its ink box — what a `TextStretch` maps onto its
/// rect (an ornament font's bubble stretched over a callout).
#[test]
fn glyph_reports_an_ink_box() {
    let mut e = engine();
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Bold.ttf")).expect("font");
    assert!(e.add_face("second", bytes));
    let g = e.glyph("0", 100.0, Some("second")).expect("ink");
    assert!(g.w > 20.0 && g.h > 20.0, "{g:?}");
    assert!(e.glyph(" ", 100.0, Some("second")).is_none(), "a space has no ink");
}

/// The top of the ink of each line of a shaped run: the smallest glyph
/// `y` among the glyphs that share a row, rows top to bottom.
fn line_tops(quads: &[GlyphQuad]) -> Vec<f32> {
    let mut tops: Vec<f32> = Vec::new();
    for q in quads {
        if !tops.contains(&q.y) {
            tops.push(q.y);
        }
    }
    tops.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    tops
}

/// The lines of a wrapped label sit one font height plus the line gap
/// apart, and its box is that many lines with a gap between each two:
/// 1 pixel on a screen with 1 device pixel per epx, 3 pixels on a phone
/// with 3.
///
/// Red when the gap is a fixed number of device pixels: on the 1-pixel
/// screen the lines of a wrapped label then sit 3 pixels apart, three
/// times as loose as the same label on the phone.
#[test]
fn the_lines_of_a_wrapped_label_are_one_epx_apart_on_every_screen() {
    let mut e = engine();
    let m: FaceMetrics = e.metrics(None);
    // "H" three times, in a box one "H" wide: three lines, the same glyph
    // on each, so the rows of ink are exactly one line pitch apart.
    for (epx, gap) in [(1.0f32, 1.0f32), (2.0, 2.0), (3.0, 3.0)] {
        let size: TextPx = TextPx::from_epx(20.0, epx);
        assert_eq!(size.line_gap, gap, "at {epx} device pixels per epx");
        let (one_w, one_h) = e.measure("H", size, None, None);
        assert_eq!(one_h, m.font_height(size.font_size), "one line has no gap");
        let narrow: Option<f32> = Some(one_w + 1.0);
        let (_, h) = e.measure("H H H", size, narrow, None);
        assert_eq!(h, 3.0 * one_h + 2.0 * gap, "three lines and two gaps at {epx} device pixels per epx");
        let tops: Vec<f32> = line_tops(&e.shape("H H H", size, narrow, None, 0.0, 0.0, one_w + 1.0, 0));
        assert_eq!(tops.len(), 3, "{tops:?}");
        assert_eq!((tops[1] - tops[0], tops[2] - tops[1]), (one_h + gap, one_h + gap), "the rows of ink at {epx} device pixels per epx");
        // The last row of ink ends inside the box the measure gave.
        let one: Vec<GlyphQuad> = e.shape("H", size, None, None, 0.0, 0.0, 0.0, 0);
        assert_eq!(tops[0], one[0].y, "the first line sits where a one-line label's does");
        assert!(tops[2] + one[0].h <= h, "the third line's ink ends at {} in a box {h} tall", tops[2] + one[0].h);
    }
}

/// A one-line label sits at the same height whatever the line gap is: the
/// gap is space BETWEEN lines, none above the first.
#[test]
fn one_line_sits_the_same_at_every_line_gap() {
    let mut e = engine();
    let at = |e: &mut TextEngine, line_gap: f32| -> Vec<(f32, f32)> { e.shape("Hxg", TextPx { font_size: 24.0, line_gap }, None, None, 0.0, 0.0, 0.0, 0).iter().map(|q| (q.x, q.y)).collect() };
    let none: Vec<(f32, f32)> = at(&mut e, 0.0);
    assert_eq!(none.len(), 3);
    for line_gap in [1.0, 2.0, 3.0, 4.0] {
        assert_eq!(at(&mut e, line_gap), none, "a gap of {line_gap}");
    }
}

/// The layout's measure answers in epx for the pixels the renderer will
/// draw: a wrapped label's height is its lines and gaps in device
/// pixels, divided by the scale.
#[test]
fn the_layouts_measure_counts_the_gap_of_its_screen() {
    let engine: RefCell<TextEngine> = RefCell::new(engine());
    let fh_px: f32 = engine.borrow().metrics(None).font_height(40.0);
    let measure = EpxMeasure { engine: &engine, epx: 2.0, font: None, images: &[] };
    // 20 epx is 40 device pixels here; the gap is 2.
    let (one_w, one_h) = measure.measure("H", 20.0, None, None);
    assert_eq!(one_h, fh_px / 2.0);
    let (_, h) = measure.measure("H H H", 20.0, Some(one_w + 0.5), None);
    assert_eq!(h, (3.0 * fh_px + 2.0 * 2.0) / 2.0);
}

/// The narrowest a text folds to is its longest word. `measure` at a wrap
/// width of 0 cannot say it: it breaks between letters.
#[test]
fn the_narrowest_a_text_folds_to_is_its_longest_word() {
    let engine: RefCell<TextEngine> = RefCell::new(engine());
    let measure = EpxMeasure { engine: &engine, epx: 1.0, font: None, images: &[] };
    let text = "hello wonderful world";
    let (word, _) = measure.measure("wonderful", 15.0, None, None);
    let (letter, _) = measure.measure(text, 15.0, Some(0.0), None);
    assert!(letter < word / 3.0, "a wrap width of 0 breaks between letters: {letter} against {word}");
    assert_eq!(measure.narrowest_wrap(text, 15.0, None), word);
    assert_eq!(measure.narrowest_wrap("wonderful", 15.0, None), word, "one word");
    assert_eq!(measure.narrowest_wrap("", 15.0, None), 0.0, "no text");
    // On a screen with two device pixels per epx the answer is still in epx.
    let twice = EpxMeasure { engine: &engine, epx: 2.0, font: None, images: &[] };
    assert_eq!(twice.narrowest_wrap(text, 15.0, None), twice.measure("wonderful", 15.0, None, None).0);
}

/// A wrapping label that fills what is left of a Row too narrow for it
/// keeps its longest word whole. Red when the label's smallest width
/// comes from a measure at a wrap width of 0: with Roboto that is one
/// letter, and the label wraps a letter per line.
#[test]
fn a_wrapping_label_in_a_tight_row_keeps_its_longest_word_whole() {
    let engine: RefCell<TextEngine> = RefCell::new(engine());
    let measure = EpxMeasure { engine: &engine, epx: 1.0, font: None, images: &[] };
    let c = gogo_ui::compile("View v {\n Row r width=60, halign=left, valign=top {\n Rectangle k width=50, height=10\n Label t text=\"hello wonderful world\", wrap=true, width=fill\n }\n}", &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    let rt = gogo_ui::Runtime::new(&c.page);
    let look = gogo_ui::resolve(&c.page, &rt);
    let rects = gogo_ui::layout(&c.page, &look, gogo_ui::Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 }, &measure);
    let label = rects[c.page.find("t").unwrap().index()];
    let (word, _) = measure.measure("wonderful", 15.0, None, None);
    assert_eq!(label.w, word, "as wide as `wonderful`");
    assert_eq!(label.h, measure.measure("hello wonderful world", 15.0, Some(word + 0.01), None).1, "one word to a line");
}
