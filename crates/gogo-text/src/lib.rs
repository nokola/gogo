//! `gogo-text` shapes text and rasterizes glyphs for gogo, in pure Rust
//! (`cosmic-text` + `swash`): no GPU, no window.
//!
//! Needed as its own crate so that `gogo-ui` stays free of a text engine
//! (it measures through `TextMeasure`) and a renderer only uploads what
//! this crate rasterizes.
//!
//! Glyphs go into two atlases, shelf-packed: an 8-bit coverage atlas for
//! letters, tinted by the text color, and an RGBA color atlas for emoji,
//! drawn in their own colors. Each doubles when it fills, and every glyph
//! of the grown one is rasterized again on demand. All sizes here are
//! DEVICE pixels: the caller converts epx (`TextPx::from_epx` for the
//! size of a label's text; `EpxMeasure` does all of it for the layout).
//!
//! Emoji come from a COLOR FACE: one with color palettes or color bitmap
//! strikes, such as the phone's own emoji font after
//! `add_fallback_font_file` or the OS scan. A host that installed a
//! painter (`set_host_color_glyph_painter`) paints an emoji first; else swash
//! does (COLRv0 layers and CBDT/sbix bitmaps, so Windows' Segoe UI Emoji
//! draws). Swash draws nothing for the COLRv1 file Android 13+ ships,
//! which is why an Android host paints emoji with the platform's own text
//! stack: a text box then shows exactly the emoji the keyboard offered,
//! on every Android version and from every phone maker.
//!
//! Faces arrive two ways: the page fonts at construction, and later by
//! key. A later face is a file's bytes (`add_face`: the file is parsed
//! when it is added, so a host adds it when the key is first needed and
//! still starts fast) or an OS family after the one system scan
//! (`load_system_fonts` + `add_system_face`). A font key may end in a
//! style suffix (`styled_key`): `#bold` thickens the coverage and
//! `#italic` skews it. Both are faux styles, for a face that has no bold
//! or italic member of its own.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use cosmic_text::fontdb;
use cosmic_text::{Align, Attrs, Buffer, CacheKey, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight, Wrap};
use gogo_ui::layout::TextMeasure;
use gogo_ui::ImageKey;

/// The gap between two lines of a label, in epx: none above the first
/// line, none below the last.
///
/// Needed in epx so that wrapped text is set equally tight on every
/// screen. A gap fixed in device pixels is three times as wide, measured
/// against the letters, on a screen with 1 device pixel per epx as on a
/// phone with 3.
pub const LINE_GAP_EPX: f32 = 1.0;

/// The size of a label's text on one screen, in device pixels: its font
/// size and the gap between two of its lines.
///
/// Needed as one value because the layout measures a label and the
/// renderer draws it, and both must turn its size in epx into the same
/// pixels. Measured one pixel bigger than it is drawn, a label wraps a
/// word early; with another gap, its lines sit off the box the layout
/// gave it. `from_epx` is the one place that does the turning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextPx {
    /// The font size: a whole number of device pixels, 1 or more.
    pub font_size: f32,
    /// The gap between two lines: a whole number of device pixels.
    pub line_gap: f32,
}

impl TextPx {
    /// The size of text of `font_size_epx` on a screen that has `epx`
    /// device pixels per epx.
    ///
    /// The font size is cut down to a whole pixel. The gap is
    /// `LINE_GAP_EPX` rounded to the nearest whole pixel. Neither goes
    /// under 1.
    ///
    /// Needed whole because glyphs are rasterized on whole pixels: with a
    /// gap of 2.6 pixels the lines of one label would sit 2 and 3 pixels
    /// apart in turns.
    pub fn from_epx(font_size_epx: f32, epx: f32) -> TextPx {
        TextPx { font_size: (font_size_epx * epx).floor().max(1.0), line_gap: (LINE_GAP_EPX * epx).round().max(1.0) }
    }

    /// A size for text that is one line, where no gap shows: one glyph
    /// (`TextEngine::glyph`), or letters rasterized ahead of their first
    /// frame (`TextEngine::warm`).
    fn one_line(font_size: f32) -> TextPx {
        TextPx { font_size, line_gap: 0.0 }
    }
}

/// Entries a measure cache or a shape cache holds before it is emptied.
///
/// Needed because a label that changes every few frames (a frame counter,
/// a zoom readout) is a new string each time, so an unbounded cache would
/// grow for as long as the app runs.
///
/// Emptying costs one re-shape per visible string on the next frame, far
/// below a frame.
pub const CACHE_CAP: usize = 4096;

/// Faux bold: how far the coverage grows on every side, in pixels per
/// pixel of font size. 0.6 px at a 24 px face.
///
/// Needed as a share of the size so that `#bold` looks equally heavy at
/// every font size.
pub const BOLD_GROW_PER_PX: f32 = 0.6 / 24.0;

/// The font-key style suffix (`"Title#bold"`) is gogo vocabulary: the
/// app spells it, this engine reads it.
pub use gogo_ui::font_style::{split_style, styled_key};

/// A face's vertical metrics in font units, read from the file at load.
#[derive(Clone, Copy, Debug)]
pub struct FaceMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub units_per_em: f32,
}

impl FaceMetrics {
    /// The metrics a face gets when its file carries none: Roboto's hhea
    /// metrics (1900 / 500 over 2048).
    ///
    /// Needed so that text in such a face still gets a line height. The
    /// engine answers with it too while no face is loaded.
    pub const FALLBACK: FaceMetrics = FaceMetrics { ascent: 1900.0, descent: 500.0, units_per_em: 2048.0 };

    /// One line's own height in px at `font_size` px (no spacing): a
    /// single-line label's box. The scaled ascent and the scaled descent
    /// are each rounded UP to a whole pixel, the way FreeType sizes a
    /// face (`FT_Size_Metrics`).
    ///
    /// Needed because glyphs are rasterized on whole pixels, so a line
    /// needs both rounded up: 24 px Roboto is 23 + 6 = 29 px. Rounding
    /// the sum instead gives round(28.13) = 28, one pixel short on every
    /// row of a list.
    pub fn font_height(&self, font_size: f32) -> f32 {
        let scale = font_size / self.units_per_em;
        (self.ascent * scale).ceil() + (self.descent * scale).ceil()
    }

    /// The pitch between lines: the face height plus the line gap.
    pub fn line_height(&self, size: TextPx) -> f32 {
        self.font_height(size.font_size) + size.line_gap
    }

    /// The box height of `lines` lines: the gap sits only BETWEEN lines
    /// (lines × font height + (lines − 1) × the line gap).
    pub fn block_height(&self, size: TextPx, lines: f32) -> f32 {
        let n = lines.max(1.0);
        n * self.font_height(size.font_size) + (n - 1.0) * size.line_gap
    }
}

/// Where a glyph sits in its atlas plus its placement relative to the pen.
#[derive(Clone, Copy, Debug)]
struct Slot {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    /// In the color atlas (an emoji), not the coverage atlas.
    color: bool,
}

/// One glyph to draw: a rect in device pixels and its atlas uv.
#[derive(Clone, Copy, Debug)]
pub struct GlyphQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uv: [f32; 4],
    /// The uv is into the COLOR atlas (`color_atlas`): draw the texels as
    /// they are, the text color's alpha only. False = the coverage atlas,
    /// tinted by the text color.
    pub color: bool,
}

/// One emoji the host painted (`HostColorGlyphPainter`): straight (not
/// premultiplied) RGBA rows, and where the ink sits against the pen —
/// `left` px right of it, `top` px above the baseline (FreeType's
/// bitmap_left / bitmap_top).
///
/// Needed so a raster from outside the engine (Android's text stack)
/// lands in the color atlas like swash's own: same rows, same placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostColorGlyph {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub rgba: Vec<u8>,
}

/// The host's emoji painter: the text of one shaped cluster ("😀",
/// "👨‍👩‍👧", a flag pair) and the font size in device px; `None` = the
/// host draws nothing for it (the engine then tries swash).
pub type HostColorGlyphPainter = Box<dyn FnMut(&str, f32) -> Option<HostColorGlyph>>;

/// The color atlas's first side; it doubles when it fills (`grow_color`).
const COLOR_ATLAS_START: u32 = 512;

/// A shelf packer over one square atlas: slots go left to right along a
/// row, a new row starts when one does not fit.
///
/// Needed because both atlases (coverage and color) pack the same way and
/// grow the same way — one packer, two sizes.
#[derive(Clone, Copy, Debug)]
struct Shelf {
    size: u32,
    x: u32,
    y: u32,
    h: u32,
}

impl Shelf {
    fn new(size: u32) -> Shelf {
        Shelf { size, x: 0, y: 0, h: 0 }
    }

    /// The top-left of a `w` × `h` slot, or `None` when the atlas is full.
    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if self.x + w > self.size {
            self.y += self.h;
            self.x = 0;
            self.h = 0;
        }
        if self.y + h > self.size {
            return None;
        }
        let pos = (self.x, self.y);
        self.x += w;
        self.h = self.h.max(h);
        Some(pos)
    }
}

/// A loaded font file: which family/weight `Attrs` asks for, and its
/// vertical metrics.
#[derive(Clone, Debug)]
struct Face {
    family: String,
    weight: Weight,
    metrics: FaceMetrics,
}

pub struct TextEngine {
    fs: FontSystem,
    cache: SwashCache,
    /// path key (as written in the .gogo) → face.
    faces: HashMap<String, Face>,
    default_face: Option<Face>,
    atlas: Vec<u8>,
    atlas_size: u32,
    shelf: Shelf,
    /// (glyph, faux bold) → its coverage-atlas slot; a bold glyph is its
    /// own raster.
    slots: HashMap<(CacheKey, bool), Option<Slot>>,
    /// The emoji atlas: straight RGBA, `color_shelf.size` square.
    color_atlas: Vec<u8>,
    color_shelf: Shelf,
    /// A swash-painted color glyph → its color-atlas slot.
    color_slots: HashMap<CacheKey, Option<Slot>>,
    /// A host-painted cluster (its text, the size's bits) → its
    /// color-atlas slot.
    host_slots: HashMap<(String, u32), Option<Slot>>,
    /// Whether a face is a color face (palettes or color strikes), by id —
    /// read from the file once.
    color_faces: HashMap<fontdb::ID, bool>,
    /// The host's emoji painter, when it has one (Android).
    host_color_glyph_painter: Option<HostColorGlyphPainter>,
    /// The color atlas changed since the last `take_color_dirty`.
    color_dirty: bool,
    /// The OS font scan ran (once per engine).
    system_loaded: bool,
    /// Family names the scan found, sorted, each once.
    system_families: Vec<String>,
    /// The atlas changed since the last `take_dirty`.
    dirty: bool,
    /// Set when the atlas grew: every uv handed out before is stale.
    pub generation: u32,
    /// Keyed by a hash of (text, size, width, font) — no String is built
    /// to look a measure up (a page lays out every animated frame). The
    /// engine serves every page, and their `StrId`s overlap, so the text
    /// itself is hashed, not its id. A 64-bit collision would return a
    /// wrong size for one string in ~10^19; accepted.
    measure_cache: HashMap<u64, (f32, f32)>,
    /// Shaped quads at the origin, keyed by a hash of (text, size, width,
    /// font, box_w, halign), with the atlas generation they were shaped
    /// against.
    shape_cache: HashMap<u64, (u32, Vec<GlyphQuad>)>,
}

/// The measure / shape cache key: a hash over the request, no allocation.
fn cache_key(text: &str, size: TextPx, max_width: Option<f32>, font: Option<&str>, box_w: f32, halign: u8) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    size.font_size.to_bits().hash(&mut h);
    size.line_gap.to_bits().hash(&mut h);
    max_width.unwrap_or(-1.0).to_bits().hash(&mut h);
    font.unwrap_or("").hash(&mut h);
    box_w.to_bits().hash(&mut h);
    halign.hash(&mut h);
    h.finish()
}

impl TextEngine {
    /// `fonts`: (key as the .gogo names it, file bytes). The first is the
    /// default face.
    pub fn new(fonts: &[(&str, Vec<u8>)]) -> TextEngine {
        let mut db = cosmic_text::fontdb::Database::new();
        let mut loaded: Vec<(String, cosmic_text::fontdb::ID, String, Weight)> = Vec::new();
        for (key, bytes) in fonts {
            let before: Vec<_> = db.faces().map(|f| f.id).collect();
            db.load_font_data(bytes.clone());
            if let Some(face) = db.faces().find(|f| !before.contains(&f.id)) {
                let family = face.families.first().map(|(n, _)| n.clone()).unwrap_or_default();
                loaded.push((key.to_string(), face.id, family, face.weight));
            }
        }
        // Only the fonts handed in. The system-font scan costs hundreds of
        // milliseconds on a phone, so it runs when a host asks for it
        // (`load_system_fonts`), never here.
        let mut fs = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
        let mut faces = HashMap::new();
        let mut default_face = None;
        for (key, id, family, weight) in loaded {
            let metrics = metrics_of(&mut fs, id, weight);
            let f = Face { family, weight, metrics };
            if default_face.is_none() {
                default_face = Some(f.clone());
            }
            faces.insert(key, f);
        }
        let atlas_size = 1024;
        TextEngine {
            fs,
            cache: SwashCache::new(),
            faces,
            default_face,
            atlas: vec![0; (atlas_size * atlas_size) as usize],
            atlas_size,
            shelf: Shelf::new(atlas_size),
            slots: HashMap::new(),
            color_atlas: vec![0; (COLOR_ATLAS_START * COLOR_ATLAS_START * 4) as usize],
            color_shelf: Shelf::new(COLOR_ATLAS_START),
            color_slots: HashMap::new(),
            host_slots: HashMap::new(),
            color_faces: HashMap::new(),
            host_color_glyph_painter: None,
            // Not dirty: nothing points into it until the first emoji
            // packs, so a boot without one uploads nothing.
            color_dirty: false,
            system_loaded: false,
            system_families: Vec::new(),
            dirty: true,
            generation: 0,
            measure_cache: HashMap::new(),
            shape_cache: HashMap::new(),
        }
    }

    /// Adds a face that `font=` or a draw can name by `key`; `bytes` is a
    /// TTF/OTF file. An existing key keeps its face.
    ///
    /// Needed for fonts that are not page fonts: the ones a user picks
    /// from a list, or a file fetched after start.
    ///
    /// WARNING: parses the file now. Call it when the key is first
    /// needed, not at start: parsing 28 font files took about a second on
    /// a phone.
    ///
    /// Returns false when the bytes hold no face.
    pub fn add_face(&mut self, key: &str, bytes: Vec<u8>) -> bool {
        if self.faces.contains_key(key) {
            return true;
        }
        let db = self.fs.db_mut();
        let before: usize = db.faces().count();
        db.load_font_data(bytes);
        let Some((id, family, weight)) = db.faces().nth(before).map(|f| (f.id, f.families.first().map(|(n, _)| n.clone()).unwrap_or_default(), f.weight)) else {
            return false;
        };
        let metrics = metrics_of(&mut self.fs, id, weight);
        self.faces.insert(key.to_string(), Face { family, weight, metrics });
        true
    }

    /// Whether `key` (style suffix ignored) names a registered face.
    pub fn has_face(&self, key: &str) -> bool {
        self.faces.contains_key(split_style(key).0)
    }

    /// The one OS font scan: every face under the system font folders
    /// joins the database (mapped, not copied), and their family names
    /// become `system_families`. A second call does nothing.
    ///
    /// Needed for a font picker that lists the device's fonts, and for
    /// script fallback: a glyph no page font has is then looked up across
    /// the OS faces instead of drawing as a box.
    ///
    /// WARNING: hundreds of milliseconds on a phone. Call it when such a
    /// picker first opens, never at start. A host that needs the letters
    /// of one script adds that one file instead
    /// (`add_fallback_font_file`), and scans only when the file is
    /// missing.
    ///
    /// Returns the family count.
    pub fn load_system_fonts(&mut self) -> usize {
        if self.system_loaded {
            return self.system_families.len();
        }
        self.system_loaded = true;
        let db = self.fs.db_mut();
        let before: usize = db.faces().count();
        #[cfg(target_os = "android")]
        {
            // fontdb scans no Android folder itself; these are where every
            // Android build keeps its fonts.
            for dir in ["/system/fonts", "/system/font", "/data/fonts", "/product/fonts"] {
                db.load_fonts_dir(dir);
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            db.load_system_fonts();
        }
        let mut names: Vec<String> = db.faces().skip(before).filter_map(|f| f.families.first().map(|(n, _)| n.clone())).collect();
        names.sort();
        names.dedup();
        self.system_families = names;
        // Fallback can now find glyphs it could not before.
        self.measure_cache.clear();
        self.shape_cache.clear();
        self.system_families.len()
    }

    /// The family names `load_system_fonts` found (empty before it ran).
    pub fn system_families(&self) -> &[String] {
        &self.system_families
    }

    /// One font FILE joins the database as a fallback face: a glyph no
    /// page font has is then looked up in it. Mapped, not copied (a .ttc
    /// adds every face it holds).
    ///
    /// Needed so that a language in another script costs ONE file, not
    /// the whole OS scan: Roboto has no Arabic letters, so an Arabic page
    /// draws every label as a row of boxes until the phone's Arabic font
    /// (Noto Naskh Arabic) is added here.
    ///
    /// The face gets no key, so `font=` cannot name it. The measure and
    /// shape caches are dropped, because a label measured as boxes is
    /// wrong now; the atlas and its glyphs stay.
    ///
    /// Returns false when the file is missing, unreadable, or holds no
    /// face.
    pub fn add_fallback_font_file(&mut self, path: &Path) -> bool {
        let db = self.fs.db_mut();
        let before: usize = db.faces().count();
        if db.load_font_file(path).is_err() || db.faces().count() == before {
            return false;
        }
        self.measure_cache.clear();
        self.shape_cache.clear();
        true
    }

    /// Whether any loaded face (page fonts, fallback files, the OS scan)
    /// has a glyph for `ch`. Parses every face on the way: a load-time
    /// or test answer, never per frame.
    pub fn has_glyph(&mut self, ch: char) -> bool {
        let ids: Vec<fontdb::ID> = self.fs.db_mut().faces().map(|f| f.id).collect();
        // The cmap, not `unicode_codepoints` (empty without cosmic's
        // monospace_fallback feature); glyph 0 is .notdef, the box.
        ids.into_iter().any(|id| self.fs.get_font(id, Weight::NORMAL).is_some_and(|f| f.as_swash().charmap().map(ch) != 0))
    }

    /// A face `key` names from the OS family `family` (one of
    /// `system_families`): its regular member when it has one. Returns
    /// false for an unknown family.
    pub fn add_system_face(&mut self, key: &str, family: &str) -> bool {
        if self.faces.contains_key(key) {
            return true;
        }
        let found = self
            .fs
            .db()
            .faces()
            .filter(|f| f.families.iter().any(|(n, _)| n == family))
            .min_by_key(|f| (f.style != fontdb::Style::Normal, (f.weight.0 as i32 - Weight::NORMAL.0 as i32).abs()))
            .map(|f| (f.id, f.weight));
        let Some((id, weight)) = found else {
            return false;
        };
        let metrics = metrics_of(&mut self.fs, id, weight);
        self.faces.insert(key.to_string(), Face { family: family.to_string(), weight, metrics });
        true
    }

    /// The one glyph of `ch` at `font_size` px with the pen at the origin:
    /// its ink box and atlas uv, or None when the face draws nothing for
    /// it. The `text_fit=stretch` word maps this box onto a Label's rect.
    pub fn glyph(&mut self, ch: &str, font_size: f32, font: Option<&str>) -> Option<GlyphQuad> {
        self.shape(ch, TextPx::one_line(font_size), None, font, 0.0, 0.0, 0.0, 0).into_iter().find(|g| g.w > 0.0 && g.h > 0.0)
    }

    pub fn atlas(&self) -> (&[u8], u32) {
        (&self.atlas, self.atlas_size)
    }

    /// The emoji atlas: straight RGBA rows, `side` square.
    pub fn color_atlas(&self) -> (&[u8], u32) {
        (&self.color_atlas, self.color_shelf.size)
    }

    pub fn take_color_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.color_dirty, false)
    }

    /// Installs (or removes) the host's emoji painter. A load-time call:
    /// drops the shape cache and every host-painted slot, so a cluster
    /// the old painter drew is painted again by the new one.
    pub fn set_host_color_glyph_painter(&mut self, host: Option<HostColorGlyphPainter>) {
        self.host_color_glyph_painter = host;
        self.host_slots.clear();
        self.shape_cache.clear();
    }

    /// Whether the face `id` carries color glyphs: color palettes (COLR,
    /// any version) or color bitmap strikes (CBDT, sbix).
    ///
    /// Needed to tell an emoji from a letter: an emoji goes to the host
    /// painter, a letter never does. Read once per face; a face that
    /// cannot be opened is not one.
    fn is_color_face(&mut self, id: fontdb::ID) -> bool {
        if let Some(c) = self.color_faces.get(&id) {
            return *c;
        }
        let is_color: bool = self.fs.get_font(id, Weight::NORMAL).is_some_and(|f| {
            let s = f.as_swash();
            s.color_palettes().len() > 0 || s.color_strikes().len() > 0
        });
        self.color_faces.insert(id, is_color);
        is_color
    }

    /// The language text is shaped for (a BCP-47 tag: `de`, `bn-IN`). Line
    /// breaking and script fallback follow it, so this moves with the
    /// string table (`gogo_ui::Locale`) — translated strings alone would break
    /// mid-glyph in a script the old tag never expected. Rebuilds the
    /// font system on the SAME font database (no file re-read) and drops
    /// the measure/shape caches; the atlas and its glyphs stay. A load-time
    /// call, not per frame.
    pub fn set_locale(&mut self, bcp47: &str) {
        if self.fs.locale() == bcp47 {
            return;
        }
        let placeholder = FontSystem::new_with_locale_and_db(String::new(), fontdb::Database::new());
        let (_, db) = std::mem::replace(&mut self.fs, placeholder).into_locale_and_db();
        self.fs = FontSystem::new_with_locale_and_db(bcp47.to_string(), db);
        self.measure_cache.clear();
        self.shape_cache.clear();
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    /// The face `font` names (its style suffix ignored), else the default.
    fn face(&self, font: Option<&str>) -> Option<&Face> {
        font.and_then(|f| self.faces.get(split_style(f).0)).or(self.default_face.as_ref())
    }

    /// (bold, italic) as the key's suffix spells them.
    fn style(font: Option<&str>) -> (bool, bool) {
        font.map_or((false, false), |f| {
            let (_, b, i) = split_style(f);
            (b, i)
        })
    }

    fn attrs<'a>(&'a self, font: Option<&str>) -> Attrs<'a> {
        // Faux italic is the shaper's own 14° skew; faux bold is ours, at
        // raster time (`rasterize`).
        let (_, italic) = Self::style(font);
        let flags = if italic { CacheKeyFlags::FAKE_ITALIC } else { CacheKeyFlags::empty() };
        match self.face(font) {
            Some(f) => Attrs::new().family(Family::Name(&f.family)).weight(f.weight).cache_key_flags(flags),
            None => Attrs::new().cache_key_flags(flags),
        }
    }

    /// The metrics of the face `font` names, else the default face's, else
    /// `FaceMetrics::FALLBACK`.
    pub fn metrics(&self, font: Option<&str>) -> FaceMetrics {
        self.face(font).map_or(FaceMetrics::FALLBACK, |f| f.metrics)
    }

    fn buffer(&mut self, text: &str, size: TextPx, max_width: Option<f32>, font: Option<&str>) -> Buffer {
        let line_height = self.metrics(font).line_height(size);
        let mut buffer = Buffer::new(&mut self.fs, Metrics::new(size.font_size, line_height));
        buffer.set_wrap(if max_width.is_some() { Wrap::WordOrGlyph } else { Wrap::None });
        buffer.set_size(max_width, None);
        let attrs = self.attrs(font);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        // Every line starts at the box's left edge; `halign` then shifts it
        // (`shape_pass`). Needed because cosmic-text's default puts a
        // right-to-left line (Arabic, Urdu, Hebrew) against the RIGHT edge
        // of the wrap width: a left-aligned label as wide as its row would
        // show عربي at the far right of the row. The letters still run
        // right to left inside the line; only where the line sits changes.
        for line in buffer.lines.iter_mut() {
            line.set_align(Some(Align::Left));
        }
        buffer.shape_until_scroll(&mut self.fs, false);
        buffer
    }

    /// Width and height in device pixels.
    pub fn measure(&mut self, text: &str, size: TextPx, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        let key = cache_key(text, size, max_width, font, 0.0, 0);
        if let Some(v) = self.measure_cache.get(&key) {
            return *v;
        }
        let buffer = self.buffer(text, size, max_width, font);
        let mut w: f32 = 0.0;
        let mut lines: f32 = 0.0;
        for run in buffer.layout_runs() {
            w = w.max(run.line_w);
            lines += 1.0;
        }
        let h = self.metrics(font).block_height(size, lines);
        let v = (w.ceil(), h);
        if self.measure_cache.len() >= CACHE_CAP {
            self.measure_cache.clear();
        }
        self.measure_cache.insert(key, v);
        v
    }

    /// Shapes `text` into glyph quads with the pen at (x, y) = the top-left
    /// of the text box; rasterizes new glyphs into the atlas. `halign`:
    /// 0 start, 1 center, 2 end within `box_w` (when wrapping).
    pub fn shape(&mut self, text: &str, size: TextPx, max_width: Option<f32>, font: Option<&str>, x: f32, y: f32, box_w: f32, halign: u8) -> Vec<GlyphQuad> {
        let mut out: Vec<GlyphQuad> = Vec::new();
        self.shape_into(text, size, max_width, font, x, y, box_w, halign, &mut out);
        out
    }

    /// `shape` appending to `out` instead of returning a vector.
    ///
    /// Needed because a renderer shapes every label of every frame
    /// through here: a run the cache holds then costs no allocation, so a
    /// frame that shows only cached text allocates nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn shape_into(&mut self, text: &str, size: TextPx, max_width: Option<f32>, font: Option<&str>, x: f32, y: f32, box_w: f32, halign: u8, out: &mut Vec<GlyphQuad>) {
        let (dx, dy) = (x.round(), y.round());
        let key = cache_key(text, size, max_width, font, box_w, halign);
        if let Some((generation, quads)) = self.shape_cache.get(&key) {
            if *generation == self.generation {
                out.extend(quads.iter().map(|q| GlyphQuad { x: q.x + dx, y: q.y + dy, ..*q }));
                return;
            }
        }
        let quads = self.shape_uncached(text, size, max_width, font, box_w, halign);
        if self.shape_cache.len() >= CACHE_CAP {
            self.shape_cache.clear();
        }
        out.extend(quads.iter().map(|q| GlyphQuad { x: q.x + dx, y: q.y + dy, ..*q }));
        self.shape_cache.insert(key, (self.generation, quads));
    }

    /// Shapes `text` into quads with the pen at the origin, every uv into
    /// the atlas as it is when the run ends.
    ///
    /// Needed because a glyph packed past the atlas's edge grows the
    /// atlas (`pack` → `grow`), and every uv handed out before, this
    /// run's earlier glyphs included, is into the atlas that was. A run
    /// kept that way shows without its first letters ("ve changes?" for
    /// "Save changes?") and stays that way, because it goes into the
    /// shape cache with the stale uvs.
    ///
    /// The run is shaped again against the grown atlas, in the same
    /// buffer. A run that grows nothing (the usual one) is shaped once.
    fn shape_uncached(&mut self, text: &str, size: TextPx, max_width: Option<f32>, font: Option<&str>, box_w: f32, halign: u8) -> Vec<GlyphQuad> {
        let mut out: Vec<GlyphQuad> = Vec::new();
        loop {
            let generation: u32 = self.generation;
            out.clear();
            self.shape_pass(text, size, max_width, font, box_w, halign, &mut out);
            if self.generation == generation {
                return out;
            }
        }
    }

    /// One shaping pass into `out`; its uvs are good only if neither atlas
    /// grew during it (`shape_uncached` checks and runs it again).
    fn shape_pass(&mut self, text: &str, size: TextPx, max_width: Option<f32>, font: Option<&str>, box_w: f32, halign: u8, out: &mut Vec<GlyphQuad>) {
        let (x, y) = (0.0f32, 0.0f32);
        let (bold, _) = Self::style(font);
        let buffer = self.buffer(text, size, max_width, font);
        // Per glyph: its raster key, its face, and the bytes of its line it
        // covers (the cluster an emoji painter is asked for).
        let lines: Vec<&str> = buffer.lines.iter().map(|l| l.text()).collect();
        let runs: Vec<(f32, f32, Vec<(cosmic_text::PhysicalGlyph, fontdb::ID, &str)>)> = buffer
            .layout_runs()
            .map(|run| (run.line_w, run.line_y, run.glyphs.iter().map(|g| (g.physical((0.0, 0.0), 1.0), g.font_id, &lines[run.line_i][g.start..g.end])).collect()))
            .collect();
        for (line_w, line_y, glyphs) in runs {
            let shift = match halign {
                1 => ((box_w - line_w) / 2.0).max(0.0),
                2 => (box_w - line_w).max(0.0),
                _ => 0.0,
            };
            for (g, font_id, cluster) in glyphs {
                let Some(slot) = self.slot(g.cache_key, bold, font_id, cluster) else { continue };
                if slot.width == 0 || slot.height == 0 {
                    continue;
                }
                out.push(GlyphQuad {
                    x: (x + shift).round() + g.x as f32 + slot.left as f32,
                    // cosmic-text puts half of the line gap above each
                    // line; `block_height` counts none above the first, so
                    // every row moves up by half a gap.
                    y: y.round() + (line_y - size.line_gap / 2.0).round() + g.y as f32 - slot.top as f32,
                    w: slot.width as f32,
                    h: slot.height as f32,
                    uv: [slot.u0, slot.v0, slot.u1, slot.v1],
                    color: slot.color,
                });
            }
        }
    }

    /// The slot of one glyph, rasterizing it on first sight. An emoji (a
    /// glyph of a color face) goes to the host painter when there is one,
    /// else to swash; a letter to swash. A raster that grew its atlas is
    /// packed again into the grown one (`None` from the first try).
    fn slot(&mut self, key: CacheKey, bold: bool, font_id: fontdb::ID, cluster: &str) -> Option<Slot> {
        if self.host_color_glyph_painter.is_some() && self.is_color_face(font_id) {
            let host_key = (cluster.to_string(), key.font_size_bits);
            if let Some(s) = self.host_slots.get(&host_key) {
                return *s;
            }
            if let Some(slot) = self.host_slot(cluster, key.font_size_bits) {
                self.host_slots.insert(host_key, slot);
                return slot;
            }
            // The host drew nothing for it: swash's turn, below.
        }
        if let Some(s) = self.slots.get(&(key, bold)) {
            return *s;
        }
        if let Some(s) = self.color_slots.get(&key) {
            return *s;
        }
        let generation = self.generation;
        let slot = self.rasterize(key, bold);
        let slot = if slot.is_none() && self.generation != generation {
            // An atlas grew under this glyph: pack it again into the new one.
            self.rasterize(key, bold)
        } else {
            slot
        };
        if slot.is_some_and(|s| s.color) {
            self.color_slots.insert(key, slot);
        } else {
            self.slots.insert((key, bold), slot);
        }
        slot
    }

    /// The host's raster of `cluster` packed into the color atlas; `None`
    /// = the host drew nothing (the caller falls back to swash). A pack
    /// that grows the atlas packs again into the grown one.
    fn host_slot(&mut self, cluster: &str, font_size_bits: u32) -> Option<Option<Slot>> {
        let painted: HostColorGlyph = (self.host_color_glyph_painter.as_mut()?)(cluster, f32::from_bits(font_size_bits))?;
        if painted.width == 0 || painted.height == 0 || painted.rgba.len() != (painted.width * painted.height * 4) as usize {
            return Some(Some(Slot { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, left: 0, top: 0, width: 0, height: 0, color: true }));
        }
        let first = self.pack_color(&painted.rgba, painted.width, painted.height, painted.left, painted.top, false);
        Some(first.or_else(|| self.pack_color(&painted.rgba, painted.width, painted.height, painted.left, painted.top, false)))
    }

    fn rasterize(&mut self, key: CacheKey, bold: bool) -> Option<Slot> {
        let image = self.cache.get_image(&mut self.fs, key).clone();
        let slot = image.and_then(|img| {
            let (mut w, mut h) = (img.placement.width, img.placement.height);
            let (mut left, mut top) = (img.placement.left, img.placement.top);
            if img.content == SwashContent::Color {
                // A color bitmap or COLRv0 layers (swash hands RGBA rows):
                // the color atlas, never emboldened.
                if w == 0 || h == 0 {
                    return Some(Slot { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, left: 0, top: 0, width: 0, height: 0, color: true });
                }
                return self.pack_color(&img.data, w, h, left, top, true);
            }
            if img.content != SwashContent::Mask {
                return None;
            }
            if w == 0 || h == 0 {
                return Some(Slot { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, left: 0, top: 0, width: 0, height: 0, color: false });
            }
            let mut data: std::borrow::Cow<[u8]> = std::borrow::Cow::Borrowed(&img.data);
            if bold {
                let grow: u32 = (f32::from_bits(key.font_size_bits) * BOLD_GROW_PER_PX).round().max(1.0) as u32;
                data = std::borrow::Cow::Owned(embolden(&img.data, w, h, grow));
                w += 2 * grow;
                h += 2 * grow;
                left -= grow as i32;
                top += grow as i32;
            }
            let (px, py) = self.pack(w + 1, h + 1)?;
            for row in 0..h {
                let src = &data[(row * w) as usize..((row + 1) * w) as usize];
                let dst = ((py + row) * self.atlas_size + px) as usize;
                self.atlas[dst..dst + w as usize].copy_from_slice(src);
            }
            self.dirty = true;
            let s = self.atlas_size as f32;
            Some(Slot { u0: px as f32 / s, v0: py as f32 / s, u1: (px + w) as f32 / s, v1: (py + h) as f32 / s, left, top, width: w, height: h, color: false })
        });
        slot
    }

    /// `rgba` (`w` x `h`) into the color atlas, which holds STRAIGHT
    /// color (the renderer's image quads premultiply on sample):
    /// `premultiplied` rows (swash's) are divided out on the way in. `None`
    /// after a grow (the caller packs again).
    fn pack_color(&mut self, rgba: &[u8], w: u32, h: u32, left: i32, top: i32, premultiplied: bool) -> Option<Slot> {
        let Some((px, py)) = self.color_shelf.pack(w + 1, h + 1) else {
            self.grow_color();
            return None;
        };
        let side = self.color_shelf.size;
        for row in 0..h {
            let src = &rgba[(row * w * 4) as usize..((row + 1) * w * 4) as usize];
            let dst = (((py + row) * side + px) * 4) as usize;
            let out = &mut self.color_atlas[dst..dst + (w * 4) as usize];
            out.copy_from_slice(src);
            if premultiplied {
                for texel in out.chunks_exact_mut(4) {
                    let a = texel[3] as u32;
                    if a != 0 && a != 255 {
                        for c in &mut texel[..3] {
                            *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                        }
                    }
                }
            }
        }
        self.color_dirty = true;
        let s = side as f32;
        Some(Slot { u0: px as f32 / s, v0: py as f32 / s, u1: (px + w) as f32 / s, v1: (py + h) as f32 / s, left, top, width: w, height: h, color: true })
    }

    /// Shelf packing into the coverage atlas; `None` after a grow (the
    /// caller retries via `slot`).
    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let pos = self.shelf.pack(w, h);
        if pos.is_none() {
            self.grow();
        }
        pos
    }

    fn grow(&mut self) {
        self.shape_cache.clear();
        self.atlas_size *= 2;
        self.atlas = vec![0; (self.atlas_size * self.atlas_size) as usize];
        self.shelf = Shelf::new(self.atlas_size);
        self.slots.clear();
        self.generation += 1;
        self.dirty = true;
    }

    /// The color atlas doubles: every emoji slot handed out is stale, the
    /// shape cache with them (`generation`, the same stale-uv law as the
    /// coverage atlas's `grow`).
    fn grow_color(&mut self) {
        self.shape_cache.clear();
        let side: u32 = self.color_shelf.size * 2;
        self.color_atlas = vec![0; (side * side * 4) as usize];
        self.color_shelf = Shelf::new(side);
        self.color_slots.clear();
        self.host_slots.clear();
        self.generation += 1;
        self.color_dirty = true;
    }

    /// Rasterizes every glyph of `chars` at `font_size`.
    ///
    /// Needed so that the first frame that shows them rasterizes nothing:
    /// a host calls it at start with the letters its first screen shows.
    pub fn warm(&mut self, chars: &str, font_size: f32, font: Option<&str>) {
        let _ = self.shape(chars, TextPx::one_line(font_size), None, font, 0.0, 0.0, 0.0, 0);
    }
}

/// A face's vertical metrics as its file carries them, or
/// `FaceMetrics::FALLBACK` when the file has none.
///
/// Needed because the line height comes from them, not from a constant
/// fitted to one size.
fn metrics_of(fs: &mut FontSystem, id: fontdb::ID, weight: Weight) -> FaceMetrics {
    fs.get_font(id, weight)
        .map(|font| {
            let m = font.as_swash().metrics(&[]);
            FaceMetrics { ascent: m.ascent, descent: m.descent, units_per_em: m.units_per_em as f32 }
        })
        .filter(|m| m.units_per_em > 0.0 && m.ascent > 0.0)
        .unwrap_or(FaceMetrics::FALLBACK)
}

/// Faux bold: the coverage mask grown by `grow` pixels on every side (a
/// square max filter — FreeType's EmboldenXY on the raster instead of the
/// outline). The result is `(w + 2·grow) × (h + 2·grow)`.
fn embolden(mask: &[u8], w: u32, h: u32, grow: u32) -> Vec<u8> {
    let (w2, h2) = (w + 2 * grow, h + 2 * grow);
    let mut out = vec![0u8; (w2 * h2) as usize];
    // Output (x, y) is source (x − grow, y − grow); its neighbours within
    // `grow` are the source pixels in [x − 2·grow, x], clipped to the mask.
    for y in 0..h2 {
        let (sy0, sy1) = (y.saturating_sub(2 * grow), y.min(h - 1));
        for x in 0..w2 {
            let (sx0, sx1) = (x.saturating_sub(2 * grow), x.min(w - 1));
            let mut best: u8 = 0;
            for sy in sy0..=sy1 {
                for sx in sx0..=sx1 {
                    best = best.max(mask[(sy * w + sx) as usize]);
                }
            }
            out[(y * w2 + x) as usize] = best;
        }
    }
    out
}

/// `gogo-ui`'s `TextMeasure` answered by a `TextEngine`: epx in, epx out,
/// device pixels inside.
///
/// Needed because the layout works in epx and knows no font: it asks a
/// `TextMeasure` how big a label is, and this one converts to the device
/// pixels the engine shapes in, and back.
pub struct EpxMeasure<'a> {
    pub engine: &'a RefCell<TextEngine>,
    pub epx: f32,
    /// The page's default font key (the `theme { font = … }` line).
    pub font: Option<String>,
    /// (width, height) px by `ImageKey` for `aspect=natural`; `None` = no
    /// readable header (a host picture, or a bad file).
    pub images: &'a [Option<(u32, u32)>],
}

impl TextMeasure for EpxMeasure<'_> {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        let mut e = self.engine.borrow_mut();
        // `TextPx::from_epx` is how a renderer sizes the label when it
        // draws it, so the label is measured at the size it is drawn at.
        let (w, h) = e.measure(text, TextPx::from_epx(font_size, self.epx), max_width.map(|w| w * self.epx), font.or(self.font.as_deref()));
        (w / self.epx, h / self.epx)
    }

    fn image_size(&self, key: ImageKey) -> Option<(f32, f32)> {
        self.images.get(key.index()).copied().flatten().map(|(w, h)| (w as f32, h as f32))
    }

    /// Everything that changes an answer without the page changing: the
    /// epx, the default font, and the image sizes (a picture code
    /// uploads later lands a size a `natural` aspect lays out by). The
    /// fonts themselves are fixed at `TextEngine::new`. A hash, so the
    /// retained layout compares one number per frame.
    fn identity(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.epx.to_bits().hash(&mut h);
        self.font.hash(&mut h);
        self.images.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TextPx::from_epx` cuts the font size down to a whole pixel and
    /// rounds the 1 epx line gap to a whole pixel; neither goes under 1.
    #[test]
    fn a_text_size_is_whole_pixels() {
        let rows: [(f32, f32, f32, f32); 8] = [
            // font size in epx, device pixels per epx -> font px, gap px
            (16.0, 1.0, 16.0, 1.0),
            (16.0, 1.25, 20.0, 1.0),
            (16.0, 1.5, 24.0, 2.0),
            (16.0, 2.0, 32.0, 2.0),
            (16.0, 2.625, 42.0, 3.0),
            (16.0, 3.0, 48.0, 3.0),
            (15.5, 1.0, 15.0, 1.0),
            (0.2, 0.25, 1.0, 1.0),
        ];
        for (font_size_epx, epx, font_px, gap_px) in rows {
            assert_eq!(TextPx::from_epx(font_size_epx, epx), TextPx { font_size: font_px, line_gap: gap_px }, "{font_size_epx} epx at {epx} device pixels per epx");
        }
    }

    /// A block of lines is that many font heights with a gap between
    /// each two: none above the first line, none below the last.
    #[test]
    fn a_block_has_a_gap_between_lines_only() {
        let m: FaceMetrics = FaceMetrics::FALLBACK;
        // 24 px of Roboto's metrics: 23 up, 6 down.
        assert_eq!(m.font_height(24.0), 29.0);
        let one: TextPx = TextPx { font_size: 24.0, line_gap: 1.0 };
        let three: TextPx = TextPx { font_size: 24.0, line_gap: 3.0 };
        assert_eq!((m.block_height(one, 1.0), m.block_height(three, 1.0)), (29.0, 29.0), "one line: no gap");
        assert_eq!((m.block_height(one, 3.0), m.block_height(three, 3.0)), (89.0, 93.0), "three lines: two gaps");
        assert_eq!((m.line_height(one), m.line_height(three)), (30.0, 32.0), "the pitch from one line to the next");
        assert_eq!(m.block_height(one, 0.0), 29.0, "an empty label is one line tall");
    }
}
