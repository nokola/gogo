//! `text` — shaping and glyph rasterization for gogo, pure Rust
//! (`cosmic-text` + `swash`): no GPU, no OS. Needed as its own crate so
//! `gogo` stays text-engine-free (it measures through `TextMeasure`) and
//! the backend only uploads what this crate rasterizes. Limit: one 8-bit
//! coverage atlas, shelf-packed; when it fills it doubles and every glyph
//! is rasterized again on demand. All sizes here are DEVICE pixels — the
//! caller converts epx.
//!
//! Faces arrive two ways: the page fonts at construction, and later by
//! key — a file's bytes (`add_face`, the picker's built-in fonts, parsed
//! on first use so boot stays fast) or an OS family after the one system
//! scan (`load_system_fonts` + `add_system_face`). A font key may end in
//! a style suffix (`styled_key`): `#bold` thickens the coverage, `#italic`
//! skews it — faux styles, since no shipped face carries real ones
//! (shipped FontLoader.MakeStyled).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use cosmic_text::fontdb;
use cosmic_text::{Attrs, Buffer, CacheKey, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight, Wrap};
use gogo::layout::TextMeasure;
use gogo::ImageKey;

/// Godot's default `Label` line_spacing (px), added between lines.
pub const LINE_SPACING_PX: f32 = 3.0;

/// Entries a measure / shape cache holds before it is emptied. Needed
/// because a label that changes every few frames (the debug overlay's
/// stats line, a zoom readout) is a new string each time — an unbounded
/// cache would grow for as long as the app runs. Emptying costs one
/// re-shape per visible string on the next frame, far below a frame.
pub const CACHE_CAP: usize = 4096;

/// Faux bold: how far the coverage grows on every side, in pixels per
/// pixel of font size — 0.6 px at a 24 px face, the weight shipped's
/// `VariationEmbolden = 0.6` reads as at the text page's default size.
pub const BOLD_GROW_PER_PX: f32 = 0.6 / 24.0;

/// The font-key style suffix (`"Angelina#bold"`) is gogo vocabulary: the
/// app spells it, this engine reads it.
pub use gogo::font_style::{split_style, styled_key};

/// A face's vertical metrics in font units, read from the file at load.
#[derive(Clone, Copy, Debug)]
pub struct FaceMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub units_per_em: f32,
}

impl FaceMetrics {
    /// Roboto's hhea metrics (1900 / 500 over 2048): the fallback when a
    /// face carries none.
    pub const ROBOTO: FaceMetrics = FaceMetrics { ascent: 1900.0, descent: 500.0, units_per_em: 2048.0 };

    /// One line's own height in px at `font_size` px (no spacing) — a
    /// single-line label's box, the way Godot measures it: FreeType rounds
    /// the scaled ascender up and the descender down to whole pixels
    /// (`FT_Size_Metrics`), so 24 px Roboto is 23 + 6 = 29, not
    /// round(28.13) = 28 — one pixel per list row otherwise.
    pub fn font_height(&self, font_size: f32) -> f32 {
        let scale = font_size / self.units_per_em;
        (self.ascent * scale).ceil() + (self.descent * scale).ceil()
    }

    /// The pitch between lines: the face height plus the line spacing.
    pub fn line_height(&self, font_size: f32) -> f32 {
        self.font_height(font_size) + LINE_SPACING_PX
    }

    /// The box height of `lines` lines: spacing sits only BETWEEN lines
    /// (Godot: lines × font height + (lines − 1) × line_spacing).
    pub fn block_height(&self, font_size: f32, lines: f32) -> f32 {
        let n = lines.max(1.0);
        n * self.font_height(font_size) + (n - 1.0) * LINE_SPACING_PX
    }
}

/// Where a glyph sits in the atlas plus its placement relative to the pen.
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
}

/// One glyph to draw: a rect in device pixels and its atlas uv.
#[derive(Clone, Copy, Debug)]
pub struct GlyphQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uv: [f32; 4],
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
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
    /// (glyph, faux bold) → its atlas slot; a bold glyph is its own raster.
    slots: HashMap<(CacheKey, bool), Option<Slot>>,
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
fn cache_key(text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>, box_w: f32, halign: u8) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    font_size.to_bits().hash(&mut h);
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
        // Only our fonts: no system-font scan (hundreds of ms on Android).
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
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
            slots: HashMap::new(),
            system_loaded: false,
            system_families: Vec::new(),
            dirty: true,
            generation: 0,
            measure_cache: HashMap::new(),
            shape_cache: HashMap::new(),
        }
    }

    /// A face `font=` / a draw can name by `key` — `bytes` is a TTF/OTF
    /// file. Parses now, so call it when the key is first needed, not at
    /// boot (28 picker fonts would cost the boot a second on a phone).
    /// An existing key keeps its face. Returns false when the bytes hold
    /// no face.
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
    /// become `system_families`. Needed for the picker's OS arm (shipped
    /// FontLoader lists the device's fonts after the built-ins) and for
    /// script fallback — a glyph no page font has is then looked up
    /// across the OS faces instead of drawing a box. Hundreds of
    /// milliseconds on a phone: call it when the picker first opens,
    /// never at boot. A second call does nothing. Returns the family
    /// count.
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
            // Android build keeps its fonts (Godot's OS.GetSystemFonts
            // read the same).
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
        self.shape(ch, font_size, None, font, 0.0, 0.0, 0.0, 0).into_iter().find(|g| g.w > 0.0 && g.h > 0.0)
    }

    pub fn atlas(&self) -> (&[u8], u32) {
        (&self.atlas, self.atlas_size)
    }

    /// The language text is shaped for (a BCP-47 tag: `de`, `bn-IN`). Line
    /// breaking and script fallback follow it, so this moves with the
    /// string table (`gogo::Locale`) — translated strings alone would break
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

    /// The metrics of the face `font` names (the default face, else Roboto's).
    pub fn metrics(&self, font: Option<&str>) -> FaceMetrics {
        self.face(font).map_or(FaceMetrics::ROBOTO, |f| f.metrics)
    }

    fn buffer(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> Buffer {
        let line_height = self.metrics(font).line_height(font_size);
        let mut buffer = Buffer::new(&mut self.fs, Metrics::new(font_size, line_height));
        buffer.set_wrap(if max_width.is_some() { Wrap::WordOrGlyph } else { Wrap::None });
        buffer.set_size(max_width, None);
        let attrs = self.attrs(font);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fs, false);
        buffer
    }

    /// Width and height in device pixels.
    pub fn measure(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        let key = cache_key(text, font_size, max_width, font, 0.0, 0);
        if let Some(v) = self.measure_cache.get(&key) {
            return *v;
        }
        let buffer = self.buffer(text, font_size, max_width, font);
        let mut w: f32 = 0.0;
        let mut lines: f32 = 0.0;
        for run in buffer.layout_runs() {
            w = w.max(run.line_w);
            lines += 1.0;
        }
        let h = self.metrics(font).block_height(font_size, lines);
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
    pub fn shape(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>, x: f32, y: f32, box_w: f32, halign: u8) -> Vec<GlyphQuad> {
        let key = cache_key(text, font_size, max_width, font, box_w, halign);
        if let Some((generation, quads)) = self.shape_cache.get(&key) {
            if *generation == self.generation {
                let (dx, dy) = (x.round(), y.round());
                return quads.iter().map(|q| GlyphQuad { x: q.x + dx, y: q.y + dy, ..*q }).collect();
            }
        }
        let quads = self.shape_uncached(text, font_size, max_width, font, box_w, halign);
        if self.shape_cache.len() >= CACHE_CAP {
            self.shape_cache.clear();
        }
        self.shape_cache.insert(key, (self.generation, quads.clone()));
        let (dx, dy) = (x.round(), y.round());
        quads.into_iter().map(|q| GlyphQuad { x: q.x + dx, y: q.y + dy, ..q }).collect()
    }

    /// Quads with the pen at the origin.
    fn shape_uncached(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>, box_w: f32, halign: u8) -> Vec<GlyphQuad> {
        let (x, y) = (0.0f32, 0.0f32);
        let (bold, _) = Self::style(font);
        let buffer = self.buffer(text, font_size, max_width, font);
        let mut out = Vec::new();
        let runs: Vec<(f32, f32, Vec<cosmic_text::PhysicalGlyph>)> = buffer.layout_runs().map(|run| (run.line_w, run.line_y, run.glyphs.iter().map(|g| g.physical((0.0, 0.0), 1.0)).collect())).collect();
        for (line_w, line_y, glyphs) in runs {
            let shift = match halign {
                1 => ((box_w - line_w) / 2.0).max(0.0),
                2 => (box_w - line_w).max(0.0),
                _ => 0.0,
            };
            for g in glyphs {
                let Some(slot) = self.slot(g.cache_key, bold) else { continue };
                if slot.width == 0 || slot.height == 0 {
                    continue;
                }
                out.push(GlyphQuad {
                    x: (x + shift).round() + g.x as f32 + slot.left as f32,
                    // cosmic splits the line spacing half above each line;
                    // Godot puts none above the first, so the rows sit
                    // half a spacing higher.
                    y: y.round() + (line_y - LINE_SPACING_PX / 2.0).round() + g.y as f32 - slot.top as f32,
                    w: slot.width as f32,
                    h: slot.height as f32,
                    uv: [slot.u0, slot.v0, slot.u1, slot.v1],
                });
            }
        }
        out
    }

    fn slot(&mut self, key: CacheKey, bold: bool) -> Option<Slot> {
        if let Some(s) = self.slots.get(&(key, bold)) {
            return *s;
        }
        let generation = self.generation;
        let slot = self.rasterize(key, bold);
        if slot.is_none() && self.generation != generation {
            // The atlas grew under this glyph: pack it again into the new one.
            let again = self.rasterize(key, bold);
            self.slots.insert((key, bold), again);
            return again;
        }
        self.slots.insert((key, bold), slot);
        slot
    }

    fn rasterize(&mut self, key: CacheKey, bold: bool) -> Option<Slot> {
        let image = self.cache.get_image(&mut self.fs, key).clone();
        let slot = image.and_then(|img| {
            if img.content != SwashContent::Mask {
                return None;
            }
            let (mut w, mut h) = (img.placement.width, img.placement.height);
            let (mut left, mut top) = (img.placement.left, img.placement.top);
            if w == 0 || h == 0 {
                return Some(Slot { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, left: 0, top: 0, width: 0, height: 0 });
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
            Some(Slot { u0: px as f32 / s, v0: py as f32 / s, u1: (px + w) as f32 / s, v1: (py + h) as f32 / s, left, top, width: w, height: h })
        });
        slot
    }

    /// Shelf packing; `None` after a grow (the caller retries via `slot`).
    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if self.shelf_x + w > self.atlas_size {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + h > self.atlas_size {
            self.grow();
            return None;
        }
        let pos = (self.shelf_x, self.shelf_y);
        self.shelf_x += w;
        self.shelf_h = self.shelf_h.max(h);
        Some(pos)
    }

    fn grow(&mut self) {
        self.shape_cache.clear();
        self.atlas_size *= 2;
        self.atlas = vec![0; (self.atlas_size * self.atlas_size) as usize];
        self.shelf_x = 0;
        self.shelf_y = 0;
        self.shelf_h = 0;
        self.slots.clear();
        self.generation += 1;
        self.dirty = true;
    }

    /// Rasterizes every glyph of `chars` at `font_size` so the first frame
    /// that shows them does no work (the boot warm).
    pub fn warm(&mut self, chars: &str, font_size: f32, font: Option<&str>) {
        let _ = self.shape(chars, font_size, None, font, 0.0, 0.0, 0.0, 0);
    }
}

/// A face's vertical metrics as the file carries them: the line height
/// comes from them, not from a constant fitted to one size. Roboto's when
/// the file has none.
fn metrics_of(fs: &mut FontSystem, id: fontdb::ID, weight: Weight) -> FaceMetrics {
    fs.get_font(id, weight)
        .map(|font| {
            let m = font.as_swash().metrics(&[]);
            FaceMetrics { ascent: m.ascent, descent: m.descent, units_per_em: m.units_per_em as f32 }
        })
        .filter(|m| m.units_per_em > 0.0 && m.ascent > 0.0)
        .unwrap_or(FaceMetrics::ROBOTO)
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

/// `TextMeasure` for gogo's layout: epx in, epx out, device pixels inside.
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
        // Godot sizes fonts as `int(epx * size)` (truncated): same here, or
        // every line measures a hair wider and wraps a word early.
        let (w, h) = e.measure(text, (font_size * self.epx).floor().max(1.0), max_width.map(|w| w * self.epx), font.or(self.font.as_deref()));
        (w / self.epx, h / self.epx)
    }

    fn image_size(&self, key: ImageKey) -> Option<(f32, f32)> {
        self.images.get(key.index()).copied().flatten().map(|(w, h)| (w as f32, h as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> TextEngine {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Regular.ttf")).expect("font");
        TextEngine::new(&[("assets/Fonts/Roboto Regular.ttf", bytes)])
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
        let (w, h) = e.measure("Hello", 20.0, None, None);
        assert!(w > 30.0 && w < 70.0, "{w}");
        assert_eq!(h, 24.0);
        // PR4: Godot's per-size whole-pixel ascent + descent, not a ratio.
        let m = e.metrics(None);
        assert_eq!(m.font_height(24.0), 29.0);
        assert_eq!(m.font_height(28.0), 33.0);
        assert_eq!(m.font_height(20.0), 24.0);
        let (_, h2) = e.measure("one two three four five six seven", 20.0, Some(100.0), None);
        assert!(h2 >= 48.0, "{h2}");
        let quads = e.shape("Hi", 20.0, None, None, 10.0, 5.0, 0.0, 0);
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
        assert_eq!(e.measure("Hi", 24.0, None, Some(key)), e.measure("Hi", 24.0, None, Some(&styled_key(key, true, false))), "bold keeps the advances");
    }

    /// A face added by key after construction shapes under that key.
    #[test]
    fn a_face_added_later_shapes_under_its_key() {
        let mut e = engine();
        assert!(!e.has_face("Angelina"));
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/angelina.ttf")).expect("font");
        assert!(e.add_face("Angelina", bytes));
        assert!(e.has_face("Angelina") && e.has_face("Angelina#bold"));
        let (roboto, _) = e.measure("Hello", 24.0, None, None);
        let (angelina, _) = e.measure("Hello", 24.0, None, Some("Angelina"));
        assert!((roboto - angelina).abs() > 1.0, "a different face measures differently: {roboto} vs {angelina}");
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
        let (w, _) = e.measure("Hello", 24.0, None, Some("os"));
        assert!(w > 0.0);
    }

    /// An ornament glyph reports its ink box — the bubble's stretch source.
    #[test]
    fn glyph_reports_an_ink_box() {
        let mut e = engine();
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/ornaments/Alin_Speech_Bubbles_2.ttf")).expect("font");
        assert!(e.add_face("bubbles", bytes));
        let g = e.glyph("0", 100.0, Some("bubbles")).expect("ink");
        assert!(g.w > 20.0 && g.h > 20.0, "{g:?}");
        assert!(e.glyph(" ", 100.0, Some("bubbles")).is_none(), "a space has no ink");
    }
}
